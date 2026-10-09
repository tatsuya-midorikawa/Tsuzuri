// Sockets of the standard Net module (E09): TCP streams and listeners, UDP sockets, and name resolution.
// POSIX only (macOS and Linux). The driver links this file when the program reaches Net.__*.
//
// A socket is a handle, (generation << 32) | (slot + 1), into a table of open sockets. Every open gets the
// next generation, which wraps only after 2^31 opens, so a closed or stale handle is rejected (InvalidInput,
// EBADF) instead of reaching whichever socket now has the descriptor number. Inside, every socket is
// non-blocking: a call that may wait tries the system call, waits for readiness with poll until the deadline
// of the whole call, and tries again.
//
// Every function that returns bytes writes its descriptor first and on every path (NULL and 0 on failure), and
// returns a status: 0 on success or (kind << 32) | (code & 0xffffffff) with the kinds of os.c (1 NotFound,
// 2 PermissionDenied, 3 AlreadyExists, 4 InvalidInput, 5 InvalidEncoding, 6 Interrupted, 7 Other).
// tsuzuri_net_open and tsuzuri_net_accept return a handle (positive) or the negated status instead.
// An address crosses as a 20-byte record: family 4 or 6, 0, the port (big endian), and 16 address bytes (an
// IPv4 address is the last 4). The feature macros must come before the first include.
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <time.h>
#include <unistd.h>

// Writing to a socket whose peer is gone must be an error, not SIGPIPE: MSG_NOSIGNAL where it exists, and
// SO_NOSIGPIPE on every socket where that is the way (macOS).
#if defined(MSG_NOSIGNAL)
#define TZ_NET_SEND_FLAGS MSG_NOSIGNAL
#elif defined(SO_NOSIGPIPE)
#define TZ_NET_SEND_FLAGS 0
#else
#error "Net needs MSG_NOSIGNAL or SO_NOSIGPIPE so that a closed peer does not end the process with SIGPIPE"
#endif

#define TZ_NET_API __attribute__((weak, visibility("hidden")))

// Linux creates and accepts a socket that is already non-blocking and closed on exec, with no window in
// which a program started by another thread could inherit it.
#if defined(__linux__) && defined(SOCK_CLOEXEC) && defined(SOCK_NONBLOCK)
#define TZ_NET_ATOMIC_FLAGS 1
#endif

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_net_buffer {
    unsigned char *data;
    int64_t length;
};

enum {
    TZ_NET_NOT_FOUND = 1,
    TZ_NET_PERMISSION_DENIED = 2,
    TZ_NET_ALREADY_EXISTS = 3,
    TZ_NET_INVALID_INPUT = 4,
    TZ_NET_INTERRUPTED = 6,
    TZ_NET_OTHER = 7,
};

enum { TZ_NET_STREAM = 1, TZ_NET_LISTENER = 2, TZ_NET_DATAGRAM = 3 };

#define TZ_NET_RECORD 20
#define TZ_NET_MAX_READ (INT64_C(1) << 24)
#define TZ_NET_STACK_READ 65536
#define TZ_NET_MAX_DATAGRAM 65536
#define TZ_NET_MAX_RESOLVED 64
#define TZ_NET_MAX_SEND (INT64_C(1) << 30)

#if EAGAIN == EWOULDBLOCK
#define TZ_NET_WOULD_BLOCK(error) ((error) == EAGAIN)
#else
#define TZ_NET_WOULD_BLOCK(error) ((error) == EAGAIN || (error) == EWOULDBLOCK)
#endif

static int64_t tz_net_status(int kind, int code) {
    return ((int64_t)kind << 32) | (int64_t)(uint32_t)code;
}

static int64_t tz_net_error(int error) {
    int kind;
    switch (error) {
    case EACCES:
    case EPERM:
        kind = TZ_NET_PERMISSION_DENIED;
        break;
    case EADDRINUSE:
        kind = TZ_NET_ALREADY_EXISTS;
        break;
    case EINVAL:
    case EAFNOSUPPORT:
    case EADDRNOTAVAIL:
    case EMSGSIZE:
        kind = TZ_NET_INVALID_INPUT;
        break;
    case EINTR:
        kind = TZ_NET_INTERRUPTED;
        break;
    default:
        kind = TZ_NET_OTHER;
        break;
    }
    return tz_net_status(kind, error);
}

static int64_t tz_net_invalid_input(void) {
    return tz_net_status(TZ_NET_INVALID_INPUT, 0);
}

// A handle that is closed, stale, or of another kind of socket.
static int64_t tz_net_bad_handle(void) {
    return tz_net_status(TZ_NET_INVALID_INPUT, EBADF);
}

static void tz_net_clear(struct tz_net_buffer *output) {
    output->data = NULL;
    output->length = 0;
}

// Copies `length` bytes into a buffer the caller of the descriptor owns.
static int64_t tz_net_deliver(struct tz_net_buffer *output, const unsigned char *bytes, size_t length) {
    tz_net_clear(output);
    if (length == 0) return 0;
    if (length > (size_t)INT64_MAX) return tz_net_status(TZ_NET_OTHER, ENOMEM);
    unsigned char *copy = tsuzuri_alloc((int64_t)length);
    memcpy(copy, bytes, length);
    output->data = copy;
    output->length = (int64_t)length;
    return 0;
}

// The table of open sockets, and the lock that guards it.
struct tz_net_socket {
    int descriptor;
    int kind; // 0 when the slot is free
    int family;
    int64_t generation;
    size_t next_free; // for a free slot: the next free slot + 1, or 0
};

static pthread_mutex_t tz_net_lock = PTHREAD_MUTEX_INITIALIZER;
static struct tz_net_socket *tz_net_sockets = NULL;
static size_t tz_net_capacity = 0;
static size_t tz_net_free_head = 0; // the first free slot + 1, or 0
static size_t tz_net_open_count = 0;
static int64_t tz_net_generation = 0;

static void tz_net_acquire(void) {
    if (pthread_mutex_lock(&tz_net_lock) != 0) abort();
}

static void tz_net_release(void) {
    if (pthread_mutex_unlock(&tz_net_lock) != 0) abort();
}

// Opens a slot for `descriptor`: 0 and the handle, or an errno value (the descriptor stays the caller's).
static int tz_net_register(int descriptor, int kind, int family, int64_t *handle) {
    int status = 0;
    tz_net_acquire();
    if (tz_net_free_head == 0) {
        size_t next = tz_net_capacity == 0 ? 8 : tz_net_capacity * 2;
        struct tz_net_socket *larger = NULL;
        if (next <= UINT32_MAX && next > tz_net_capacity) larger = realloc(tz_net_sockets, next * sizeof(struct tz_net_socket));
        if (larger == NULL) {
            status = next > UINT32_MAX ? EMFILE : ENOMEM;
        } else {
            for (size_t index = tz_net_capacity; index < next; index++) {
                larger[index].kind = 0;
                larger[index].next_free = index + 1 < next ? index + 2 : 0;
            }
            tz_net_sockets = larger;
            tz_net_free_head = tz_net_capacity + 1;
            tz_net_capacity = next;
        }
    }
    if (status == 0) {
        size_t slot = tz_net_free_head - 1;
        struct tz_net_socket *entry = &tz_net_sockets[slot];
        tz_net_free_head = entry->next_free;
        tz_net_generation = tz_net_generation >= INT32_MAX ? 1 : tz_net_generation + 1;
        entry->descriptor = descriptor;
        entry->kind = kind;
        entry->family = family;
        entry->generation = tz_net_generation;
        entry->next_free = 0;
        tz_net_open_count++;
        *handle = (entry->generation << 32) | (int64_t)(slot + 1);
    }
    tz_net_release();
    return status;
}

// Finds the open socket of `handle` that is of `kind` (0 for any) and copies it: 1 when found. The copy
// outlives the lock, so a concurrent open cannot move the table under a caller.
static int tz_net_find(int64_t handle, int kind, struct tz_net_socket *found) {
    int present = 0;
    if (handle > 0) {
        uint64_t slot = (uint64_t)handle & UINT64_C(0xffffffff);
        int64_t generation = handle >> 32;
        tz_net_acquire();
        if (slot != 0 && slot <= tz_net_capacity) {
            const struct tz_net_socket *entry = &tz_net_sockets[slot - 1];
            if (entry->kind != 0 && entry->generation == generation && (kind == 0 || entry->kind == kind)) {
                *found = *entry;
                present = 1;
            }
        }
        tz_net_release();
    }
    return present;
}

// Frees the slot of `handle`: 1 and its descriptor when the handle was open. The table itself is freed when
// no socket is open.
static int tz_net_forget(int64_t handle, int *descriptor) {
    int present = 0;
    if (handle > 0) {
        uint64_t slot = (uint64_t)handle & UINT64_C(0xffffffff);
        int64_t generation = handle >> 32;
        tz_net_acquire();
        if (slot != 0 && slot <= tz_net_capacity) {
            struct tz_net_socket *entry = &tz_net_sockets[slot - 1];
            if (entry->kind != 0 && entry->generation == generation) {
                *descriptor = entry->descriptor;
                entry->kind = 0;
                entry->next_free = tz_net_free_head;
                tz_net_free_head = slot;
                present = 1;
                if (--tz_net_open_count == 0) {
                    free(tz_net_sockets);
                    tz_net_sockets = NULL;
                    tz_net_capacity = 0;
                    tz_net_free_head = 0;
                }
            }
        }
        tz_net_release();
    }
    return present;
}

// The monotonic clock in nanoseconds, and the deadline of a call that may wait `timeout` milliseconds (-1: no deadline).
static int64_t tz_net_now(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) abort();
    return (int64_t)now.tv_sec * INT64_C(1000000000) + (int64_t)now.tv_nsec;
}

static int64_t tz_net_deadline(int64_t timeout) {
    return timeout < 0 ? -1 : tz_net_now() + timeout * INT64_C(1000000);
}

// Waits until the socket can do `events` or the deadline passes: 0 when it can (an error or a hang-up counts, and
// the next call reports it), ETIMEDOUT at the deadline, or the errno of poll. An interrupted poll starts again
// with the time that is left, rounded up to a whole millisecond so that the last fraction does not spin.
static int tz_net_wait(int descriptor, short events, int64_t deadline) {
    for (;;) {
        int timeout = -1;
        if (deadline >= 0) {
            int64_t remaining = deadline - tz_net_now();
            if (remaining <= 0) return ETIMEDOUT;
            int64_t milliseconds = (remaining + 999999) / 1000000;
            timeout = milliseconds > INT_MAX ? INT_MAX : (int)milliseconds;
        }
        struct pollfd entry;
        entry.fd = descriptor;
        entry.events = events;
        entry.revents = 0;
        int ready = poll(&entry, 1, timeout);
        if (ready > 0) return 0;
        if (ready < 0 && errno != EINTR) return errno;
    }
}

// A 20-byte address record of a socket address, or all zeros for another family.
static void tz_net_record(unsigned char *record, const struct sockaddr *address) {
    memset(record, 0, TZ_NET_RECORD);
    if (address->sa_family == AF_INET) {
        const struct sockaddr_in *v4 = (const struct sockaddr_in *)address;
        record[0] = 4;
        memcpy(record + 2, &v4->sin_port, 2);
        memcpy(record + 16, &v4->sin_addr, 4);
    } else if (address->sa_family == AF_INET6) {
        const struct sockaddr_in6 *v6 = (const struct sockaddr_in6 *)address;
        record[0] = 6;
        memcpy(record + 2, &v6->sin6_port, 2);
        memcpy(record + 4, &v6->sin6_addr, 16);
    }
}

// A socket address of the scalars that Tsuzuri passes: `meta` is the family flag (bit 16) and the port, and
// `high` and `low` are the first and last 8 bytes of an IPv6 address (`low` alone is an IPv4 address).
static socklen_t tz_net_address(struct sockaddr_storage *storage, int64_t meta, int64_t high, int64_t low) {
    memset(storage, 0, sizeof *storage);
    uint16_t port = (uint16_t)((uint64_t)meta & 0xffff);
    if (((meta >> 16) & 1) == 0) {
        struct sockaddr_in *v4 = (struct sockaddr_in *)storage;
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__NetBSD__) || defined(__OpenBSD__)
        v4->sin_len = sizeof *v4;
#endif
        v4->sin_family = AF_INET;
        v4->sin_port = htons(port);
        v4->sin_addr.s_addr = htonl((uint32_t)((uint64_t)low & UINT64_C(0xffffffff)));
        return sizeof *v4;
    }
    struct sockaddr_in6 *v6 = (struct sockaddr_in6 *)storage;
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__NetBSD__) || defined(__OpenBSD__)
    v6->sin6_len = sizeof *v6;
#endif
    v6->sin6_family = AF_INET6;
    v6->sin6_port = htons(port);
    for (int index = 0; index < 8; index++) {
        v6->sin6_addr.s6_addr[index] = (uint8_t)((uint64_t)high >> (56 - 8 * index));
        v6->sin6_addr.s6_addr[8 + index] = (uint8_t)((uint64_t)low >> (56 - 8 * index));
    }
    return sizeof *v6;
}

// A socket that is non-blocking, closed in a program that this one starts, and quiet when its peer is gone.
// Linux sets the first two when it creates or accepts the socket, and has no SO_NOSIGPIPE; the other systems
// need these calls, and a socket that they accept keeps the listener's non-blocking state.
static int tz_net_prepare(int descriptor) {
#if !defined(TZ_NET_ATOMIC_FLAGS)
    int flags = fcntl(descriptor, F_GETFD);
    if (flags < 0 || fcntl(descriptor, F_SETFD, flags | FD_CLOEXEC) < 0) return -1;
    flags = fcntl(descriptor, F_GETFL);
    if (flags < 0 || fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) < 0) return -1;
#endif
#if defined(SO_NOSIGPIPE)
    int enable = 1;
    if (setsockopt(descriptor, SOL_SOCKET, SO_NOSIGPIPE, &enable, sizeof enable) != 0) return -1;
#else
    (void)descriptor;
#endif
    return 0;
}

// A new socket: -1 and errno on failure. An IPv6 socket never takes IPv4 connections (IPV6_V6ONLY), so "::" and
// "0.0.0.0" are separate on every system.
static int tz_net_create(int family, int type) {
#if defined(TZ_NET_ATOMIC_FLAGS)
    int descriptor = socket(family, type | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
#else
    int descriptor = socket(family, type, 0);
#endif
    if (descriptor < 0) return -1;
    int ready = tz_net_prepare(descriptor);
    if (ready == 0 && family == AF_INET6) {
        int enable = 1;
        ready = setsockopt(descriptor, IPPROTO_IPV6, IPV6_V6ONLY, &enable, sizeof enable);
    }
    if (ready != 0) {
        int error = errno;
        close(descriptor);
        errno = error;
        return -1;
    }
    return descriptor;
}

// The local address of a socket goes in the first 20 bytes of `records` and the peer address in the next 20: the
// address of a connect's target or the one that accept returned (`peer`; NULL leaves zeros). It is not read from
// the socket, because a connection that the peer has already reset no longer has a peer name (getpeername fails),
// but still is a connection that was made.
static int tz_net_names(int descriptor, unsigned char *records, const struct sockaddr *peer) {
    struct sockaddr_storage address;
    socklen_t length = sizeof address;
    memset(records, 0, 2 * TZ_NET_RECORD);
    if (getsockname(descriptor, (struct sockaddr *)&address, &length) != 0) return errno;
    tz_net_record(records, (const struct sockaddr *)&address);
    if (peer != NULL) tz_net_record(records + TZ_NET_RECORD, peer);
    return 0;
}

// Registers an open socket and hands the caller its handle and the 40 bytes of addresses; a failure closes the socket.
static int64_t tz_net_publish(struct tz_net_buffer *output, int descriptor, int kind, int family, const struct sockaddr *peer) {
    unsigned char records[2 * TZ_NET_RECORD];
    int error = tz_net_names(descriptor, records, peer);
    int64_t handle = 0;
    if (error == 0) error = tz_net_register(descriptor, kind, family, &handle);
    if (error != 0) {
        close(descriptor);
        return -tz_net_error(error);
    }
    int64_t status = tz_net_deliver(output, records, sizeof records);
    return status == 0 ? handle : -status;
}

// op 0 connects to the address, 1 listens on it, and 2 binds a UDP socket to it. A connect waits at most `timeout`
// milliseconds (-1: for the system). A failure returns the negated status, a success a handle and the local and
// peer address records (the peer of a listener and of a UDP socket is all zeros).
TZ_NET_API int64_t tsuzuri_net_open(struct tz_net_buffer *output, int32_t operation, int64_t meta, int64_t high, int64_t low, int64_t timeout) {
    tz_net_clear(output);
    if (operation < 0 || operation > 2) return -tz_net_invalid_input();
    struct sockaddr_storage target;
    socklen_t length = tz_net_address(&target, meta, high, low);
    int family = target.ss_family;
    int descriptor = tz_net_create(family, operation == 2 ? SOCK_DGRAM : SOCK_STREAM);
    if (descriptor < 0) return -tz_net_error(errno);
    int error = 0;
    if (operation == 0) {
        int64_t deadline = tz_net_deadline(timeout);
        if (connect(descriptor, (struct sockaddr *)&target, length) != 0) {
            error = errno;
            // An interrupted or slow connect goes on by itself; it is finished when the socket can be written.
            if (error == EINTR || error == EINPROGRESS) {
                error = tz_net_wait(descriptor, POLLOUT, deadline);
                if (error == 0) {
                    int pending = 0;
                    socklen_t size = sizeof pending;
                    error = getsockopt(descriptor, SOL_SOCKET, SO_ERROR, &pending, &size) != 0 ? errno : pending;
                }
            }
        }
    } else {
        int enable = 1;
        // A listener can bind again at once after a restart, but not over a socket that still listens.
        if (operation == 1 && setsockopt(descriptor, SOL_SOCKET, SO_REUSEADDR, &enable, sizeof enable) != 0) error = errno;
        if (error == 0 && bind(descriptor, (struct sockaddr *)&target, length) != 0) error = errno;
        if (error == 0 && operation == 1 && listen(descriptor, SOMAXCONN) != 0) error = errno;
    }
    if (error != 0) {
        close(descriptor);
        return -tz_net_error(error);
    }
    return tz_net_publish(output, descriptor, operation == 0 ? TZ_NET_STREAM : operation == 1 ? TZ_NET_LISTENER : TZ_NET_DATAGRAM, family, operation == 0 ? (const struct sockaddr *)&target : NULL);
}

// Takes a connection from a listener, waiting at most `timeout` milliseconds (-1: for one). A handle and the
// local and peer address records, or the negated status.
TZ_NET_API int64_t tsuzuri_net_accept(struct tz_net_buffer *output, int64_t handle, int64_t timeout) {
    tz_net_clear(output);
    struct tz_net_socket listener;
    if (!tz_net_find(handle, TZ_NET_LISTENER, &listener)) return -tz_net_bad_handle();
    int64_t deadline = tz_net_deadline(timeout);
    struct sockaddr_storage peer;
    for (;;) {
        socklen_t peer_length = sizeof peer;
#if defined(TZ_NET_ATOMIC_FLAGS)
        int descriptor = accept4(listener.descriptor, (struct sockaddr *)&peer, &peer_length, SOCK_CLOEXEC | SOCK_NONBLOCK);
#else
        int descriptor = accept(listener.descriptor, (struct sockaddr *)&peer, &peer_length);
#endif
        if (descriptor >= 0) {
            if (tz_net_prepare(descriptor) == 0) {
                return tz_net_publish(output, descriptor, TZ_NET_STREAM, listener.family, (const struct sockaddr *)&peer);
            }
            int error = errno;
            close(descriptor);
            // macOS refuses a socket option (EINVAL) on a connection that the peer has already reset: like
            // ECONNABORTED, that is a connection that is gone, and the next one is the answer.
            if (error != EINVAL && error != ECONNABORTED) return -tz_net_error(error);
            continue;
        }
        int error = errno;
        // A connection that the peer gave up before this call is not an error for the next one.
        if (error == EINTR || error == ECONNABORTED) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return -tz_net_error(error);
        error = tz_net_wait(listener.descriptor, POLLIN, deadline);
        if (error != 0) return -tz_net_error(error);
    }
}

// Delivers `count` bytes that were received into `buffer`, which holds `capacity` bytes of Tsuzuri memory when
// it is not the stack (`owned`); the bytes are the result as they are, or an exact copy.
static int64_t tz_net_finish_read(struct tz_net_buffer *output, unsigned char *buffer, int owned, int64_t capacity, size_t count) {
    if (!owned) return tz_net_deliver(output, buffer, count);
    if (count == 0) {
        tsuzuri_free(buffer);
        tz_net_clear(output);
        return 0;
    }
    if ((int64_t)count == capacity) {
        output->data = buffer;
        output->length = (int64_t)count;
        return 0;
    }
    int64_t status = tz_net_deliver(output, buffer, count);
    tsuzuri_free(buffer);
    return status;
}

// A datagram: its source address record, then its bytes. A datagram longer than `maximum` is consumed and EMSGSIZE.
static int64_t tz_net_receive_datagram(struct tz_net_buffer *output, int descriptor, int64_t maximum, int64_t deadline) {
    // No datagram is longer than 65,535 bytes, so a bigger buffer never fills.
    size_t capacity = maximum < TZ_NET_MAX_DATAGRAM ? (size_t)maximum : TZ_NET_MAX_DATAGRAM;
    unsigned char packet[TZ_NET_RECORD + TZ_NET_MAX_DATAGRAM];
    for (;;) {
        struct sockaddr_storage source;
        struct iovec vector;
        struct msghdr message;
        memset(&message, 0, sizeof message);
        vector.iov_base = packet + TZ_NET_RECORD;
        vector.iov_len = capacity;
        message.msg_name = &source;
        message.msg_namelen = sizeof source;
        message.msg_iov = &vector;
        message.msg_iovlen = 1;
        ssize_t count = recvmsg(descriptor, &message, 0);
        if (count >= 0) {
            if ((message.msg_flags & MSG_TRUNC) != 0) return tz_net_error(EMSGSIZE);
            tz_net_record(packet, (const struct sockaddr *)&source);
            return tz_net_deliver(output, packet, TZ_NET_RECORD + (size_t)count);
        }
        int error = errno;
        if (error == EINTR) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
        error = tz_net_wait(descriptor, POLLIN, deadline);
        if (error != 0) return tz_net_error(error);
    }
}

// What has arrived on a stream, at most `maximum` bytes, none at the end of the stream. A small read lands on
// the stack and is copied exactly; a big one lands in the memory it is delivered in.
static int64_t tz_net_receive_stream(struct tz_net_buffer *output, int descriptor, int64_t maximum, int64_t deadline) {
    unsigned char stack[TZ_NET_STACK_READ];
    int owned = maximum > TZ_NET_STACK_READ;
    unsigned char *buffer = owned ? tsuzuri_alloc(maximum) : stack;
    for (;;) {
        ssize_t count = recv(descriptor, buffer, (size_t)maximum, 0);
        if (count >= 0) return tz_net_finish_read(output, buffer, owned, maximum, (size_t)count);
        int error = errno;
        if (error != EINTR) {
            if (TZ_NET_WOULD_BLOCK(error)) error = tz_net_wait(descriptor, POLLIN, deadline);
            if (error != 0) {
                if (owned) tsuzuri_free(buffer);
                return tz_net_error(error);
            }
        }
    }
}

// op 0 receives from a stream: what has arrived, at most `maximum` bytes, none at the end of the stream. op 1
// receives one datagram: its source address record, then the datagram. Waits at most `timeout` milliseconds
// (-1: for data) for the whole call.
TZ_NET_API int64_t tsuzuri_net_read(struct tz_net_buffer *output, int32_t operation, int64_t handle, int64_t maximum, int64_t timeout) {
    tz_net_clear(output);
    if ((operation != 0 && operation != 1) || maximum < 1 || maximum > TZ_NET_MAX_READ) return tz_net_invalid_input();
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, operation == 0 ? TZ_NET_STREAM : TZ_NET_DATAGRAM, &socket_entry)) return tz_net_bad_handle();
    int64_t deadline = tz_net_deadline(timeout);
    return operation == 1 ? tz_net_receive_datagram(output, socket_entry.descriptor, maximum, deadline)
                          : tz_net_receive_stream(output, socket_entry.descriptor, maximum, deadline);
}

// op 0 sends all of `data` on a stream, however many sends that takes, within `timeout` milliseconds (-1: no
// limit) for the whole call. op 1 sends `data` as one datagram to the address (the timeout does not apply).
TZ_NET_API int64_t tsuzuri_net_write(int32_t operation, int64_t handle, const unsigned char *data, int64_t length, int64_t meta, int64_t high, int64_t low, int64_t timeout) {
    if ((operation != 0 && operation != 1) || length < 0) return tz_net_invalid_input();
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, operation == 0 ? TZ_NET_STREAM : TZ_NET_DATAGRAM, &socket_entry)) return tz_net_bad_handle();
    int descriptor = socket_entry.descriptor;
    if (operation == 1) {
        struct sockaddr_storage target;
        socklen_t size = tz_net_address(&target, meta, high, low);
        for (;;) {
            ssize_t count = sendto(descriptor, data, (size_t)length, TZ_NET_SEND_FLAGS, (struct sockaddr *)&target, size);
            if (count >= 0) return 0;
            int error = errno;
            if (error == EINTR) continue;
            if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
            error = tz_net_wait(descriptor, POLLOUT, -1);
            if (error != 0) return tz_net_error(error);
        }
    }
    int64_t deadline = tz_net_deadline(timeout);
    int64_t sent = 0;
    while (sent < length) {
        int64_t chunk = length - sent < TZ_NET_MAX_SEND ? length - sent : TZ_NET_MAX_SEND;
        ssize_t count = send(descriptor, data + sent, (size_t)chunk, TZ_NET_SEND_FLAGS);
        if (count > 0) {
            sent += count;
            continue;
        }
        int error = count < 0 ? errno : EAGAIN;
        if (error == EINTR) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
        error = tz_net_wait(descriptor, POLLOUT, deadline);
        if (error != 0) return tz_net_error(error);
    }
    return 0;
}

// op 0 closes a socket of any kind; the handle is dead afterwards even when the system reports a failure.
// op 1, 2, and 3 end the reading, the writing, or both of a stream.
TZ_NET_API int64_t tsuzuri_net_close(int32_t operation, int64_t handle) {
    if (operation < 0 || operation > 3) return tz_net_invalid_input();
    if (operation == 0) {
        int descriptor;
        if (!tz_net_forget(handle, &descriptor)) return tz_net_bad_handle();
        // After EINTR the descriptor state is unspecified, and retrying could close another one.
        return close(descriptor) != 0 && errno != EINTR ? tz_net_error(errno) : 0;
    }
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, TZ_NET_STREAM, &socket_entry)) return tz_net_bad_handle();
    int how = operation == 1 ? SHUT_RD : operation == 2 ? SHUT_WR : SHUT_RDWR;
    return shutdown(socket_entry.descriptor, how) == 0 ? 0 : tz_net_error(errno);
}

// The Net.ErrorKind index of an errno value: 0 TimedOut, 1 ConnectionRefused, 2 ConnectionReset, 3 AddressInUse,
// 4 AddressNotAvailable, 5 Unreachable, and 6 for anything else (and for 0, which is no system code).
TZ_NET_API int64_t tsuzuri_net_classify(int32_t code) {
    switch (code) {
    case ETIMEDOUT:
        return 0;
    case ECONNREFUSED:
        return 1;
    case ECONNRESET:
    case ECONNABORTED:
    case EPIPE:
        return 2;
    case EADDRINUSE:
        return 3;
    case EADDRNOTAVAIL:
        return 4;
    case ENETUNREACH:
    case EHOSTUNREACH:
    case ENETDOWN:
    case EHOSTDOWN:
        return 5;
    default:
        return 6;
    }
}

// What getaddrinfo reports, as a status: a name that is not found is NotFound (code 0), a failed system call is its
// errno, and the rest is Other with no code, because the numbers of EAI_ values mean something else as errno.
static int64_t tz_net_resolve_error(int failure) {
    if (failure == EAI_NONAME
#if defined(EAI_NODATA)
        || failure == EAI_NODATA
#endif
    ) {
        return tz_net_status(TZ_NET_NOT_FOUND, 0);
    }
    if (failure == EAI_SYSTEM) return tz_net_error(errno);
    return tz_net_status(TZ_NET_OTHER, 0);
}

// Looks a host up (the system's resolver, which blocks and cannot be interrupted): the 20-byte address records
// with `port`, in the system's order, without repeats, at most 64.
TZ_NET_API int64_t tsuzuri_net_resolve(struct tz_net_buffer *output, const unsigned char *host, int64_t length, int64_t port) {
    tz_net_clear(output);
    if (length < 1 || length > 253 || port < 0 || port > 65535 || memchr(host, 0, (size_t)length) != NULL) return tz_net_invalid_input();
    char name[254];
    memcpy(name, host, (size_t)length);
    name[length] = '\0';
    struct addrinfo hints;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo *list = NULL;
    int failure = getaddrinfo(name, NULL, &hints, &list);
    if (failure != 0) return tz_net_resolve_error(failure);
    unsigned char records[TZ_NET_MAX_RESOLVED * TZ_NET_RECORD];
    size_t count = 0;
    for (const struct addrinfo *entry = list; entry != NULL && count < TZ_NET_MAX_RESOLVED; entry = entry->ai_next) {
        if (entry->ai_addr == NULL || (entry->ai_family != AF_INET && entry->ai_family != AF_INET6)) continue;
        unsigned char *record = records + count * TZ_NET_RECORD;
        tz_net_record(record, entry->ai_addr);
        record[2] = (unsigned char)((uint64_t)port >> 8);
        record[3] = (unsigned char)((uint64_t)port & 0xff);
        int repeated = 0;
        for (size_t earlier = 0; earlier < count && !repeated; earlier++) {
            repeated = memcmp(records + earlier * TZ_NET_RECORD, record, TZ_NET_RECORD) == 0;
        }
        if (!repeated) count++;
    }
    freeaddrinfo(list);
    if (count == 0) return tz_net_status(TZ_NET_NOT_FOUND, 0);
    return tz_net_deliver(output, records, count * TZ_NET_RECORD);
}
