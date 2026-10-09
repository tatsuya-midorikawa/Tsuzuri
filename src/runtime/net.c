// Sockets of the standard Net module (E09): TCP streams and listeners, UDP sockets, name resolution, and the
// readiness waits of the async operations. macOS and Linux use POSIX sockets and Windows uses Winsock (the
// driver links ws2_32). The driver links this file when the program reaches Net.__*.
//
// A socket is a handle, (generation << 32) | (slot + 1), into a table of open sockets. Every open gets the
// next generation, which wraps only after 2^31 opens, so a closed or stale handle is rejected (InvalidInput,
// EBADF) instead of reaching whichever socket now has the descriptor number. Inside, every socket is
// non-blocking: a call that may wait tries the system call, waits for readiness until the deadline of the
// whole call, and tries again.
//
// Every function that returns bytes writes its descriptor first and on every path (NULL and 0 on failure), and
// returns a status: 0 on success or (kind << 32) | (code & 0xffffffff) with the kinds of os.c (1 NotFound,
// 2 PermissionDenied, 3 AlreadyExists, 4 InvalidInput, 5 InvalidEncoding, 6 Interrupted, 7 Other).
// tsuzuri_net_open and tsuzuri_net_accept return a handle (positive) or the negated status instead.
// An address crosses as a 20-byte record: family 4 or 6, 0, the port (big endian), and 16 address bytes (an
// IPv4 address is the last 4). The feature macros must come before the first include.
//
// The async operations do their I/O on the thread that runs the executor and never wait there: a timeout of 0
// means "try once", and a call that would have to wait returns TZ_NET_PENDING (kind Other, code 0), which no
// system failure has. What the executor waits for is readiness, and one poller thread watches it for all the
// waits: it starts with the first wait, sleeps in poll until a descriptor is ready, a deadline passes, or a
// change wakes it, and ends when no wait is left. It posts the completion of a wait, 0 for ready or a status,
// with the `tsuzuri_async_post` that the IR passes in, so a completion owns nothing and a completion that
// nobody receives costs nothing. Only a connect that is under way is owned by its wait, until it finishes.
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#if defined(_WIN32)
// The task runtime of the same translation unit defines this macro too, to the same value.
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
// The MSVC target reads this request for the import library; a MinGW target takes it from the link line (-lws2_32).
#if defined(_MSC_VER)
#pragma comment(lib, "ws2_32.lib")
#endif
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <pthread.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <time.h>
#include <unistd.h>
#endif
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

// The platform layer. The system's error numbers get names of their own, so that this file never redefines the
// errno names that the rest of the runtime reads.
#if defined(_WIN32)
#define TZ_NET_API
typedef SOCKET tz_net_fd;
typedef WSAPOLLFD tz_net_pollfd;
#define TZ_NET_NO_FD INVALID_SOCKET
#define TZ_NET_POLL_READ POLLRDNORM
#define TZ_NET_POLL_WRITE POLLWRNORM
#define TZ_NET_SEND_FLAGS 0
#define TZ_NET_SHUTDOWN_READ SD_RECEIVE
#define TZ_NET_SHUTDOWN_WRITE SD_SEND
#define TZ_NET_SHUTDOWN_BOTH SD_BOTH
#define TZ_NET_E_INTR WSAEINTR
#define TZ_NET_E_WOULDBLOCK WSAEWOULDBLOCK
#define TZ_NET_E_INPROGRESS WSAEINPROGRESS
#define TZ_NET_E_ACCES WSAEACCES
#define TZ_NET_E_ADDRINUSE WSAEADDRINUSE
#define TZ_NET_E_ADDRNOTAVAIL WSAEADDRNOTAVAIL
#define TZ_NET_E_AFNOSUPPORT WSAEAFNOSUPPORT
#define TZ_NET_E_INVAL WSAEINVAL
#define TZ_NET_E_MSGSIZE WSAEMSGSIZE
#define TZ_NET_E_TIMEDOUT WSAETIMEDOUT
#define TZ_NET_E_CONNABORTED WSAECONNABORTED
#define TZ_NET_E_CONNREFUSED WSAECONNREFUSED
#define TZ_NET_E_CONNRESET WSAECONNRESET
#define TZ_NET_E_PIPE WSAESHUTDOWN
#define TZ_NET_E_NETUNREACH WSAENETUNREACH
#define TZ_NET_E_HOSTUNREACH WSAEHOSTUNREACH
#define TZ_NET_E_NETDOWN WSAENETDOWN
#define TZ_NET_E_HOSTDOWN WSAEHOSTDOWN
#define TZ_NET_E_BADF WSAEBADF
#define TZ_NET_E_MFILE WSAEMFILE
#define TZ_NET_E_NOMEM WSA_NOT_ENOUGH_MEMORY
#else
#define TZ_NET_API __attribute__((weak, visibility("hidden")))
typedef int tz_net_fd;
typedef struct pollfd tz_net_pollfd;
#define TZ_NET_NO_FD (-1)
#define TZ_NET_POLL_READ POLLIN
#define TZ_NET_POLL_WRITE POLLOUT
#define TZ_NET_SHUTDOWN_READ SHUT_RD
#define TZ_NET_SHUTDOWN_WRITE SHUT_WR
#define TZ_NET_SHUTDOWN_BOTH SHUT_RDWR
#define TZ_NET_E_INTR EINTR
#define TZ_NET_E_INPROGRESS EINPROGRESS
#define TZ_NET_E_ACCES EACCES
#define TZ_NET_E_ADDRINUSE EADDRINUSE
#define TZ_NET_E_ADDRNOTAVAIL EADDRNOTAVAIL
#define TZ_NET_E_AFNOSUPPORT EAFNOSUPPORT
#define TZ_NET_E_INVAL EINVAL
#define TZ_NET_E_MSGSIZE EMSGSIZE
#define TZ_NET_E_TIMEDOUT ETIMEDOUT
#define TZ_NET_E_CONNABORTED ECONNABORTED
#define TZ_NET_E_CONNREFUSED ECONNREFUSED
#define TZ_NET_E_CONNRESET ECONNRESET
#define TZ_NET_E_PIPE EPIPE
#define TZ_NET_E_NETUNREACH ENETUNREACH
#define TZ_NET_E_HOSTUNREACH EHOSTUNREACH
#define TZ_NET_E_NETDOWN ENETDOWN
#define TZ_NET_E_HOSTDOWN EHOSTDOWN
#define TZ_NET_E_BADF EBADF
#define TZ_NET_E_MFILE EMFILE
#define TZ_NET_E_NOMEM ENOMEM
#if EAGAIN == EWOULDBLOCK
#define TZ_NET_E_WOULDBLOCK EAGAIN
#else
#define TZ_NET_E_WOULDBLOCK EAGAIN
#define TZ_NET_E_WOULDBLOCK_ALSO EWOULDBLOCK
#endif
#endif

// Writing to a socket whose peer is gone must be an error, not SIGPIPE: MSG_NOSIGNAL where it exists, and
// SO_NOSIGPIPE on every socket where that is the way (macOS). Windows has no such signal.
#if !defined(_WIN32)
#if defined(MSG_NOSIGNAL)
#define TZ_NET_SEND_FLAGS MSG_NOSIGNAL
#elif defined(SO_NOSIGPIPE)
#define TZ_NET_SEND_FLAGS 0
#else
#error "Net needs MSG_NOSIGNAL or SO_NOSIGPIPE so that a closed peer does not end the process with SIGPIPE"
#endif
#endif

// Linux creates and accepts a socket that is already non-blocking and closed on exec, with no window in
// which a program started by another thread could inherit it.
#if defined(__linux__) && defined(SOCK_CLOEXEC) && defined(SOCK_NONBLOCK)
#define TZ_NET_ATOMIC_FLAGS 1
#endif

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

// The runtime's way to complete an async operation; the IR passes `tsuzuri_async_post`.
typedef int32_t (*tz_net_post_function)(int64_t operation, int64_t value);

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

// What a wait is for: the `events` argument of tsuzuri_net_watch.
enum { TZ_NET_WATCH_READ = 1, TZ_NET_WATCH_WRITE = 2 };

#define TZ_NET_RECORD 20
#define TZ_NET_MAX_READ (INT64_C(1) << 24)
#define TZ_NET_STACK_READ 65536
#define TZ_NET_MAX_DATAGRAM 65536
#define TZ_NET_MAX_RESOLVED 64
#define TZ_NET_MAX_SEND (INT64_C(1) << 30)

// An error number that no system reports: a call that was told not to wait would have had to.
#define TZ_NET_E_PENDING (-1)
// The deadline of a call that must not wait (a timeout of 0), apart from -1 for no deadline.
#define TZ_NET_ATTEMPT (-2)

#if defined(TZ_NET_E_WOULDBLOCK_ALSO)
#define TZ_NET_WOULD_BLOCK(error) ((error) == TZ_NET_E_WOULDBLOCK || (error) == TZ_NET_E_WOULDBLOCK_ALSO)
#else
#define TZ_NET_WOULD_BLOCK(error) ((error) == TZ_NET_E_WOULDBLOCK)
#endif

// A connect that goes on by itself: interrupted or slow, and on Windows the way every non-blocking connect starts.
#if defined(_WIN32)
#define TZ_NET_CONNECT_PENDING(error) ((error) == TZ_NET_E_INTR || (error) == TZ_NET_E_INPROGRESS || (error) == TZ_NET_E_WOULDBLOCK)
#else
#define TZ_NET_CONNECT_PENDING(error) ((error) == TZ_NET_E_INTR || (error) == TZ_NET_E_INPROGRESS)
#endif

// ---- Locks, the clock, and the error number of the last socket call ----

#if defined(_WIN32)
typedef SRWLOCK tz_net_mutex;
#define TZ_NET_MUTEX_INIT SRWLOCK_INIT
static void tz_net_mutex_lock(tz_net_mutex *mutex) { AcquireSRWLockExclusive(mutex); }
static void tz_net_mutex_unlock(tz_net_mutex *mutex) { ReleaseSRWLockExclusive(mutex); }

static int tz_net_errno(void) { return WSAGetLastError(); }

// The monotonic clock in nanoseconds. Whole seconds and the rest are scaled apart, so that nothing overflows.
static int64_t tz_net_now(void) {
    LARGE_INTEGER frequency, counter;
    if (!QueryPerformanceFrequency(&frequency) || !QueryPerformanceCounter(&counter) || frequency.QuadPart <= 0) abort();
    int64_t seconds = counter.QuadPart / frequency.QuadPart;
    int64_t rest = counter.QuadPart % frequency.QuadPart;
    return seconds * INT64_C(1000000000) + rest * INT64_C(1000000000) / frequency.QuadPart;
}

static INIT_ONCE tz_net_start_once = INIT_ONCE_STATIC_INIT;
static int tz_net_start_error;

static BOOL CALLBACK tz_net_start_winsock(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    (void)once;
    (void)parameter;
    (void)context;
    WSADATA data;
    tz_net_start_error = WSAStartup(MAKEWORD(2, 2), &data);
    return TRUE;
}

// Winsock starts once, with the first call that needs it: 0 or the error number of WSAStartup.
static int tz_net_start(void) {
    if (!InitOnceExecuteOnce(&tz_net_start_once, tz_net_start_winsock, NULL, NULL)) return (int)GetLastError();
    return tz_net_start_error;
}

static int tz_net_close_fd(tz_net_fd descriptor) { return closesocket(descriptor); }

static int tz_net_poll(tz_net_pollfd *entries, size_t count, int timeout) { return WSAPoll(entries, (ULONG)count, timeout); }
#else
typedef pthread_mutex_t tz_net_mutex;
#define TZ_NET_MUTEX_INIT PTHREAD_MUTEX_INITIALIZER
static void tz_net_mutex_lock(tz_net_mutex *mutex) {
    if (pthread_mutex_lock(mutex) != 0) abort();
}
static void tz_net_mutex_unlock(tz_net_mutex *mutex) {
    if (pthread_mutex_unlock(mutex) != 0) abort();
}

static int tz_net_errno(void) { return errno; }

static int64_t tz_net_now(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) abort();
    return (int64_t)now.tv_sec * INT64_C(1000000000) + (int64_t)now.tv_nsec;
}

static int tz_net_start(void) { return 0; }

static int tz_net_close_fd(tz_net_fd descriptor) { return close(descriptor); }

static int tz_net_poll(tz_net_pollfd *entries, size_t count, int timeout) { return poll(entries, (nfds_t)count, timeout); }
#endif

// ---- Statuses ----

static int64_t tz_net_status(int kind, int code) {
    return ((int64_t)kind << 32) | (int64_t)(uint32_t)code;
}

static int64_t tz_net_error(int error) {
    int kind;
    if (error == TZ_NET_E_PENDING) return tz_net_status(TZ_NET_OTHER, 0);
    switch (error) {
    case TZ_NET_E_ACCES:
#if !defined(_WIN32)
    case EPERM:
#endif
        kind = TZ_NET_PERMISSION_DENIED;
        break;
    case TZ_NET_E_ADDRINUSE:
        kind = TZ_NET_ALREADY_EXISTS;
        break;
    case TZ_NET_E_INVAL:
    case TZ_NET_E_AFNOSUPPORT:
    case TZ_NET_E_ADDRNOTAVAIL:
    case TZ_NET_E_MSGSIZE:
        kind = TZ_NET_INVALID_INPUT;
        break;
    case TZ_NET_E_INTR:
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
    return tz_net_status(TZ_NET_INVALID_INPUT, TZ_NET_E_BADF);
}

static void tz_net_clear(struct tz_net_buffer *output) {
    output->data = NULL;
    output->length = 0;
}

// Copies `length` bytes into a buffer the caller of the descriptor owns.
static int64_t tz_net_deliver(struct tz_net_buffer *output, const unsigned char *bytes, size_t length) {
    tz_net_clear(output);
    if (length == 0) return 0;
    if (length > (size_t)INT64_MAX) return tz_net_status(TZ_NET_OTHER, TZ_NET_E_NOMEM);
    unsigned char *copy = tsuzuri_alloc((int64_t)length);
    memcpy(copy, bytes, length);
    output->data = copy;
    output->length = (int64_t)length;
    return 0;
}

// ---- The table of open sockets, and the lock that guards it ----

struct tz_net_socket {
    tz_net_fd descriptor;
    int kind; // 0 when the slot is free
    int family;
    int64_t generation;
    size_t next_free; // for a free slot: the next free slot + 1, or 0
    unsigned char names[2 * TZ_NET_RECORD]; // the local address record, then the peer's
};

static tz_net_mutex tz_net_table_lock = TZ_NET_MUTEX_INIT;
static struct tz_net_socket *tz_net_sockets = NULL;
static size_t tz_net_capacity = 0;
static size_t tz_net_free_head = 0; // the first free slot + 1, or 0
static size_t tz_net_open_count = 0;
static int64_t tz_net_generation = 0;

static void tz_net_acquire(void) {
    tz_net_mutex_lock(&tz_net_table_lock);
}

static void tz_net_release(void) {
    tz_net_mutex_unlock(&tz_net_table_lock);
}

// Opens a slot for `descriptor` with the 40 bytes of addresses `names` (NULL: zeros): 0 and the handle, or an
// error number (the descriptor stays the caller's).
static int tz_net_register(tz_net_fd descriptor, int kind, int family, const unsigned char *names, int64_t *handle) {
    int status = 0;
    tz_net_acquire();
    if (tz_net_free_head == 0) {
        size_t next = tz_net_capacity == 0 ? 8 : tz_net_capacity * 2;
        struct tz_net_socket *larger = NULL;
        if (next <= UINT32_MAX && next > tz_net_capacity) larger = realloc(tz_net_sockets, next * sizeof(struct tz_net_socket));
        if (larger == NULL) {
            status = next > UINT32_MAX ? TZ_NET_E_MFILE : TZ_NET_E_NOMEM;
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
        if (names != NULL) memcpy(entry->names, names, sizeof entry->names);
        else memset(entry->names, 0, sizeof entry->names);
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
static int tz_net_forget(int64_t handle, tz_net_fd *descriptor) {
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

// ---- Deadlines and blocking waits ----

// The deadline of a call that may wait `timeout` milliseconds: -1 for none, TZ_NET_ATTEMPT for 0.
static int64_t tz_net_deadline(int64_t timeout) {
    if (timeout < 0) return -1;
    if (timeout == 0) return TZ_NET_ATTEMPT;
    return tz_net_now() + timeout * INT64_C(1000000);
}

// Waits until the socket can do `events` or the deadline passes: 0 when it can (an error or a hang-up counts, and
// the next call reports it), TZ_NET_E_TIMEDOUT at the deadline, TZ_NET_E_PENDING for a call that must not wait,
// or the error number of poll. An interrupted poll starts again with the time that is left, rounded up to a
// whole millisecond so that the last fraction does not spin.
static int tz_net_wait(tz_net_fd descriptor, short events, int64_t deadline) {
    if (deadline == TZ_NET_ATTEMPT) return TZ_NET_E_PENDING;
    for (;;) {
        int timeout = -1;
        if (deadline >= 0) {
            int64_t remaining = deadline - tz_net_now();
            if (remaining <= 0) return TZ_NET_E_TIMEDOUT;
            int64_t milliseconds = (remaining + 999999) / 1000000;
            timeout = milliseconds > INT_MAX ? INT_MAX : (int)milliseconds;
        }
        tz_net_pollfd entry;
        entry.fd = descriptor;
        entry.events = events;
        entry.revents = 0;
        int ready = tz_net_poll(&entry, 1, timeout);
        if (ready > 0) return 0;
        if (ready < 0 && tz_net_errno() != TZ_NET_E_INTR) return tz_net_errno();
    }
}

// ---- Addresses ----

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

// ---- Creating sockets ----

// A socket that is non-blocking, closed in a program that this one starts, and quiet when its peer is gone.
// Linux sets the first two when it creates or accepts the socket, and has no SO_NOSIGPIPE; the other systems
// need these calls, and a socket that they accept keeps the listener's non-blocking state. Windows sets
// non-blocking mode and takes the handle out of what a child inherits.
static int tz_net_prepare(tz_net_fd descriptor) {
#if defined(_WIN32)
    u_long enable = 1;
    if (ioctlsocket(descriptor, FIONBIO, &enable) != 0) return -1;
    if (!SetHandleInformation((HANDLE)descriptor, HANDLE_FLAG_INHERIT, 0)) {
        WSASetLastError((int)GetLastError());
        return -1;
    }
#else
    (void)descriptor;
#if !defined(TZ_NET_ATOMIC_FLAGS)
    int flags = fcntl(descriptor, F_GETFD);
    if (flags < 0 || fcntl(descriptor, F_SETFD, flags | FD_CLOEXEC) < 0) return -1;
    flags = fcntl(descriptor, F_GETFL);
    if (flags < 0 || fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) < 0) return -1;
#endif
#if defined(SO_NOSIGPIPE)
    int enable = 1;
    if (setsockopt(descriptor, SOL_SOCKET, SO_NOSIGPIPE, &enable, sizeof enable) != 0) return -1;
#endif
#endif
    return 0;
}

// A new socket: TZ_NET_NO_FD and the error number of the system on failure. An IPv6 socket never takes IPv4
// connections (IPV6_V6ONLY), so "::" and "0.0.0.0" are separate on every system.
static tz_net_fd tz_net_create(int family, int type) {
#if defined(_WIN32)
    tz_net_fd descriptor = WSASocketW(family, type, 0, NULL, 0, WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
#elif defined(TZ_NET_ATOMIC_FLAGS)
    tz_net_fd descriptor = socket(family, type | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
#else
    tz_net_fd descriptor = socket(family, type, 0);
#endif
    if (descriptor == TZ_NET_NO_FD) return TZ_NET_NO_FD;
    int ready = tz_net_prepare(descriptor);
    if (ready == 0 && family == AF_INET6) {
        int enable = 1;
        ready = setsockopt(descriptor, IPPROTO_IPV6, IPV6_V6ONLY, (const char *)&enable, sizeof enable);
    }
    if (ready != 0) {
        int error = tz_net_errno();
        tz_net_close_fd(descriptor);
#if defined(_WIN32)
        WSASetLastError(error);
#else
        errno = error;
#endif
        return TZ_NET_NO_FD;
    }
    return descriptor;
}

// The local address of a socket goes in the first 20 bytes of `records` and `peer` (NULL leaves zeros) in the next
// 20. The peer is the address of a connect's target or the one that accept returned, not read from the socket,
// because a connection that the peer has already reset no longer has a peer name (getpeername fails), but still
// is a connection that was made.
static int tz_net_names(tz_net_fd descriptor, unsigned char *records, const struct sockaddr *peer) {
    struct sockaddr_storage address;
    socklen_t length = sizeof address;
    memset(records, 0, 2 * TZ_NET_RECORD);
    if (getsockname(descriptor, (struct sockaddr *)&address, &length) != 0) return tz_net_errno();
    tz_net_record(records, (const struct sockaddr *)&address);
    if (peer != NULL) tz_net_record(records + TZ_NET_RECORD, peer);
    return 0;
}

// Closes a handle that nobody has seen yet: the socket of a connect whose completion nobody wants.
static void tz_net_discard(int64_t handle) {
    tz_net_fd descriptor;
    if (tz_net_forget(handle, &descriptor)) tz_net_close_fd(descriptor);
}

// Registers an open socket and hands the caller its handle and the 40 bytes of addresses; a failure closes the socket.
static int64_t tz_net_publish(struct tz_net_buffer *output, tz_net_fd descriptor, int kind, int family, const struct sockaddr *peer) {
    unsigned char records[2 * TZ_NET_RECORD];
    int error = tz_net_names(descriptor, records, peer);
    int64_t handle = 0;
    if (error == 0) error = tz_net_register(descriptor, kind, family, records, &handle);
    if (error != 0) {
        tz_net_close_fd(descriptor);
        return -tz_net_error(error);
    }
    int64_t status = tz_net_deliver(output, records, sizeof records);
    if (status != 0) {
        tz_net_discard(handle);
        return -status;
    }
    return handle;
}

// ---- The readiness waits and the poller thread ----

// A wait is one registered interest of an async operation: its socket ready for `events`, or a connect that the
// wait owns until it finishes. `serial` orders the waits by registration.
struct tz_net_wait {
    uint64_t serial;
    int64_t operation;
    int64_t handle; // the socket of a readiness wait; 0 for a connect
    tz_net_fd descriptor;
    short events;
    int connecting; // the wait owns `descriptor`, a socket that connects
    int family;
    int64_t deadline; // monotonic nanoseconds, or -1
    unsigned char peer[TZ_NET_RECORD]; // a connect's target
};

static tz_net_mutex tz_net_poll_lock = TZ_NET_MUTEX_INIT;
static struct tz_net_wait *tz_net_waits = NULL; // in the order of `serial`
static size_t tz_net_wait_count = 0;
static size_t tz_net_wait_capacity = 0;
static uint64_t tz_net_wait_serial = 0;
static int tz_net_poller_running = 0;
static tz_net_post_function tz_net_post = NULL;

// The poller sleeps in poll on its wake channel too, so that a new or removed wait changes what it watches at
// once: a pipe, or on Windows (where poll waits on sockets only) a UDP socket that sends to itself.
#if defined(_WIN32)
static SOCKET tz_net_wake_socket = INVALID_SOCKET;
static struct sockaddr_in tz_net_wake_address;

static int tz_net_wake_open(void) {
    SOCKET descriptor = tz_net_create(AF_INET, SOCK_DGRAM);
    if (descriptor == INVALID_SOCKET) return tz_net_errno();
    struct sockaddr_in address;
    int length = sizeof address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(descriptor, (struct sockaddr *)&address, sizeof address) != 0 || getsockname(descriptor, (struct sockaddr *)&address, &length) != 0) {
        int error = tz_net_errno();
        closesocket(descriptor);
        return error;
    }
    tz_net_wake_address = address;
    tz_net_wake_socket = descriptor;
    return 0;
}

static tz_net_fd tz_net_wake_descriptor(void) { return tz_net_wake_socket; }

static void tz_net_wake_signal(void) {
    char byte = 1;
    sendto(tz_net_wake_socket, &byte, 1, 0, (const struct sockaddr *)&tz_net_wake_address, sizeof tz_net_wake_address);
}

static void tz_net_wake_drain(void) {
    char bytes[64];
    while (recv(tz_net_wake_socket, bytes, sizeof bytes, 0) > 0) {
    }
}

static void tz_net_wake_close(void) {
    if (tz_net_wake_socket != INVALID_SOCKET) closesocket(tz_net_wake_socket);
    tz_net_wake_socket = INVALID_SOCKET;
}
#else
static int tz_net_wake_pipe[2] = {-1, -1};

static int tz_net_wake_open(void) {
#if defined(__linux__)
    if (pipe2(tz_net_wake_pipe, O_CLOEXEC | O_NONBLOCK) != 0) return errno;
#else
    if (pipe(tz_net_wake_pipe) != 0) return errno;
    for (int index = 0; index < 2; index++) {
        int flags = fcntl(tz_net_wake_pipe[index], F_GETFD);
        int status = flags < 0 ? -1 : fcntl(tz_net_wake_pipe[index], F_SETFD, flags | FD_CLOEXEC);
        if (status == 0) {
            flags = fcntl(tz_net_wake_pipe[index], F_GETFL);
            status = flags < 0 ? -1 : fcntl(tz_net_wake_pipe[index], F_SETFL, flags | O_NONBLOCK);
        }
        if (status != 0) {
            int error = errno;
            close(tz_net_wake_pipe[0]);
            close(tz_net_wake_pipe[1]);
            tz_net_wake_pipe[0] = tz_net_wake_pipe[1] = -1;
            return error;
        }
    }
#endif
    return 0;
}

static tz_net_fd tz_net_wake_descriptor(void) { return tz_net_wake_pipe[0]; }

static void tz_net_wake_signal(void) {
    char byte = 1;
    // A full pipe already has a wake-up in it.
    if (write(tz_net_wake_pipe[1], &byte, 1) < 0) {
    }
}

static void tz_net_wake_drain(void) {
    char bytes[64];
    while (read(tz_net_wake_pipe[0], bytes, sizeof bytes) > 0) {
    }
}

static void tz_net_wake_close(void) {
    if (tz_net_wake_pipe[0] >= 0) close(tz_net_wake_pipe[0]);
    if (tz_net_wake_pipe[1] >= 0) close(tz_net_wake_pipe[1]);
    tz_net_wake_pipe[0] = tz_net_wake_pipe[1] = -1;
}
#endif

// Settles a wait that has left the list. A connect that is under way ends here: its result is a new handle
// (positive) or the negated status; a rejected post means that the operation is gone, so the new socket is closed.
// A wait for readiness completes with 0, or with the status of why it did not.
static void tz_net_settle(const struct tz_net_wait *wait, int expired) {
    if (!wait->connecting) {
        tz_net_post(wait->operation, expired ? tz_net_error(TZ_NET_E_TIMEDOUT) : 0);
        return;
    }
    int64_t result = 0;
    int error = expired ? TZ_NET_E_TIMEDOUT : 0;
    if (error == 0) {
        int pending = 0;
        socklen_t size = sizeof pending;
        error = getsockopt(wait->descriptor, SOL_SOCKET, SO_ERROR, (char *)&pending, &size) != 0 ? tz_net_errno() : pending;
    }
    if (error == 0) {
        unsigned char records[2 * TZ_NET_RECORD];
        error = tz_net_names(wait->descriptor, records, NULL);
        memcpy(records + TZ_NET_RECORD, wait->peer, TZ_NET_RECORD);
        if (error == 0) error = tz_net_register(wait->descriptor, TZ_NET_STREAM, wait->family, records, &result);
    }
    if (error != 0) {
        tz_net_close_fd(wait->descriptor);
        result = -tz_net_error(error);
    }
    if (tz_net_post(wait->operation, result) == 0 && result > 0) tz_net_discard(result);
}

// The caller holds the poll lock. Settles every wait with the failure `error` (an error number).
static void tz_net_settle_all(int error) {
    for (size_t index = 0; index < tz_net_wait_count; index++) {
        const struct tz_net_wait *wait = &tz_net_waits[index];
        if (wait->connecting) {
            tz_net_close_fd(wait->descriptor);
            tz_net_post(wait->operation, -tz_net_error(error));
        } else {
            tz_net_post(wait->operation, tz_net_error(error));
        }
    }
    tz_net_wait_count = 0;
}

// The caller holds the poll lock, and `entries` and `serials` are what the poller watched: its wake channel and
// then one entry per wait, as of `snapshot` waits. A wait that is ready or past its deadline is settled and leaves
// the list; waits registered since are checked against their deadlines only.
static void tz_net_process(const tz_net_pollfd *entries, const uint64_t *serials, size_t snapshot) {
    int64_t now = tz_net_now();
    size_t kept = 0;
    size_t seen = 0;
    for (size_t index = 0; index < tz_net_wait_count; index++) {
        const struct tz_net_wait wait = tz_net_waits[index];
        while (seen < snapshot && serials[seen] < wait.serial) seen++;
        short events = seen < snapshot && serials[seen] == wait.serial ? entries[1 + seen].revents : 0;
        if (events != 0) {
            tz_net_settle(&wait, 0);
        } else if (wait.deadline >= 0 && wait.deadline <= now) {
            tz_net_settle(&wait, 1);
        } else {
            tz_net_waits[kept++] = wait;
        }
    }
    tz_net_wait_count = kept;
}

static void tz_net_poller_main(void) {
    tz_net_pollfd *entries = NULL;
    uint64_t *serials = NULL;
    size_t capacity = 0;
    for (;;) {
        tz_net_mutex_lock(&tz_net_poll_lock);
        if (tz_net_wait_count == 0) {
            tz_net_wake_close();
            free(tz_net_waits);
            tz_net_waits = NULL;
            tz_net_wait_capacity = 0;
            tz_net_poller_running = 0;
            tz_net_mutex_unlock(&tz_net_poll_lock);
            free(entries);
            free(serials);
            return;
        }
        size_t count = tz_net_wait_count;
        if (count + 1 > capacity) {
            size_t next = capacity == 0 ? 16 : capacity * 2;
            while (next < count + 1) next *= 2;
            tz_net_pollfd *larger_entries = realloc(entries, next * sizeof *entries);
            if (larger_entries != NULL) entries = larger_entries;
            uint64_t *larger_serials = realloc(serials, next * sizeof *serials);
            if (larger_serials != NULL) serials = larger_serials;
            if (larger_entries == NULL || larger_serials == NULL) {
                tz_net_settle_all(TZ_NET_E_NOMEM);
                tz_net_mutex_unlock(&tz_net_poll_lock);
                continue;
            }
            capacity = next;
        }
        int64_t earliest = -1;
        entries[0].fd = tz_net_wake_descriptor();
        entries[0].events = TZ_NET_POLL_READ;
        entries[0].revents = 0;
        for (size_t index = 0; index < count; index++) {
            const struct tz_net_wait *wait = &tz_net_waits[index];
            entries[1 + index].fd = wait->descriptor;
            entries[1 + index].events = wait->events;
            entries[1 + index].revents = 0;
            serials[index] = wait->serial;
            if (wait->deadline >= 0 && (earliest < 0 || wait->deadline < earliest)) earliest = wait->deadline;
        }
        int timeout = -1;
        if (earliest >= 0) {
            int64_t remaining = earliest - tz_net_now();
            int64_t milliseconds = remaining <= 0 ? 0 : (remaining + 999999) / 1000000;
            timeout = milliseconds > INT_MAX ? INT_MAX : (int)milliseconds;
        }
        tz_net_mutex_unlock(&tz_net_poll_lock);
        int ready = tz_net_poll(entries, count + 1, timeout);
        int error = ready < 0 ? tz_net_errno() : 0;
        tz_net_mutex_lock(&tz_net_poll_lock);
        if (ready < 0 && error != TZ_NET_E_INTR) {
            tz_net_settle_all(error);
        } else {
            if (ready > 0 && entries[0].revents != 0) tz_net_wake_drain();
            tz_net_process(entries, serials, count);
        }
        tz_net_mutex_unlock(&tz_net_poll_lock);
    }
}

#if defined(_WIN32)
static DWORD WINAPI tz_net_poller_entry(LPVOID unused) {
    (void)unused;
    tz_net_poller_main();
    return 0;
}

// Starts the poller on a thread that nobody joins: 0 or an error number.
static int tz_net_spawn(void) {
    HANDLE thread = CreateThread(NULL, 0, tz_net_poller_entry, NULL, 0, NULL);
    if (thread == NULL) return (int)GetLastError();
    CloseHandle(thread);
    return 0;
}
#else
static void *tz_net_poller_entry(void *unused) {
    (void)unused;
    tz_net_poller_main();
    return NULL;
}

static int tz_net_spawn(void) {
    pthread_attr_t attributes;
    pthread_t thread;
    int error = pthread_attr_init(&attributes);
    if (error != 0) return error;
    error = pthread_attr_setdetachstate(&attributes, PTHREAD_CREATE_DETACHED);
    if (error == 0) error = pthread_create(&thread, &attributes, tz_net_poller_entry, NULL);
    pthread_attr_destroy(&attributes);
    return error;
}
#endif

// The caller holds the poll lock. Adds a wait and makes the poller notice it, starting the poller if it is not
// running: 0 or an error number (the wait is not added, and a connect's socket stays the caller's).
static int tz_net_add_wait(const struct tz_net_wait *wait) {
    if (tz_net_wait_count == tz_net_wait_capacity) {
        size_t next = tz_net_wait_capacity == 0 ? 16 : tz_net_wait_capacity * 2;
        struct tz_net_wait *larger = realloc(tz_net_waits, next * sizeof *larger);
        if (larger == NULL) return TZ_NET_E_NOMEM;
        tz_net_waits = larger;
        tz_net_wait_capacity = next;
    }
    if (tz_net_poller_running) {
        tz_net_wake_signal();
    } else {
        int error = tz_net_wake_open();
        if (error == 0) {
            error = tz_net_spawn();
            if (error != 0) tz_net_wake_close();
        }
        if (error != 0) return error;
        tz_net_poller_running = 1;
    }
    tz_net_waits[tz_net_wait_count] = *wait;
    tz_net_waits[tz_net_wait_count].serial = ++tz_net_wait_serial;
    tz_net_wait_count++;
    return 0;
}

// Removes the waits of `operation`, or, when `handle` is not 0, the readiness waits on that socket (the caller
// holds the poll lock): how many. `settle` completes a removed wait as ready; otherwise a removed connect gives
// up its socket.
static size_t tz_net_remove_waits(int64_t operation, int64_t handle, int settle) {
    size_t kept = 0;
    size_t removed = 0;
    for (size_t index = 0; index < tz_net_wait_count; index++) {
        const struct tz_net_wait wait = tz_net_waits[index];
        int selected = handle != 0 ? !wait.connecting && wait.handle == handle : wait.operation == operation;
        if (!selected) {
            tz_net_waits[kept++] = wait;
            continue;
        }
        removed++;
        if (settle) tz_net_post(wait.operation, 0);
        else if (wait.connecting) tz_net_close_fd(wait.descriptor);
    }
    tz_net_wait_count = kept;
    if (removed > 0 && tz_net_poller_running) tz_net_wake_signal();
    return removed;
}

// A wait on a socket that is being closed ends as ready, and the retry reports the closed handle. Called before
// the descriptor is closed, so that no wait can watch a descriptor number that another socket takes over.
static void tz_net_cancel_handle(int64_t handle) {
    tz_net_mutex_lock(&tz_net_poll_lock);
    if (tz_net_wait_count > 0) tz_net_remove_waits(0, handle, 1);
    tz_net_mutex_unlock(&tz_net_poll_lock);
}

// ---- Opening, accepting, reading, writing, and closing ----

// op 0 connects to the address, 1 listens on it, and 2 binds a UDP socket to it. A connect waits at most `timeout`
// milliseconds (-1: for the system). A failure returns the negated status, a success a handle and the local and
// peer address records (the peer of a listener and of a UDP socket is all zeros).
TZ_NET_API int64_t tsuzuri_net_open(struct tz_net_buffer *output, int32_t operation, int64_t meta, int64_t high, int64_t low, int64_t timeout) {
    tz_net_clear(output);
    if (operation < 0 || operation > 2) return -tz_net_invalid_input();
    int error = tz_net_start();
    if (error != 0) return -tz_net_error(error);
    struct sockaddr_storage target;
    socklen_t length = tz_net_address(&target, meta, high, low);
    int family = target.ss_family;
    tz_net_fd descriptor = tz_net_create(family, operation == 2 ? SOCK_DGRAM : SOCK_STREAM);
    if (descriptor == TZ_NET_NO_FD) return -tz_net_error(tz_net_errno());
    if (operation == 0) {
        int64_t deadline = tz_net_deadline(timeout);
        if (connect(descriptor, (struct sockaddr *)&target, length) != 0) {
            error = tz_net_errno();
            // An interrupted or slow connect goes on by itself; it is finished when the socket can be written.
            if (TZ_NET_CONNECT_PENDING(error)) {
                error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, deadline);
                if (error == 0) {
                    int pending = 0;
                    socklen_t size = sizeof pending;
                    error = getsockopt(descriptor, SOL_SOCKET, SO_ERROR, (char *)&pending, &size) != 0 ? tz_net_errno() : pending;
                }
            }
        }
    } else {
        int enable = 1;
        // A listener can bind again at once after a restart, but not over a socket that still listens. Windows
        // lets SO_REUSEADDR share a port with a live socket, so it asks for the port exclusively instead.
#if defined(_WIN32)
        if (operation == 1 && setsockopt(descriptor, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (const char *)&enable, sizeof enable) != 0) error = tz_net_errno();
#else
        if (operation == 1 && setsockopt(descriptor, SOL_SOCKET, SO_REUSEADDR, &enable, sizeof enable) != 0) error = tz_net_errno();
#endif
        if (error == 0 && bind(descriptor, (struct sockaddr *)&target, length) != 0) error = tz_net_errno();
        if (error == 0 && operation == 1 && listen(descriptor, SOMAXCONN) != 0) error = tz_net_errno();
    }
    if (error != 0) {
        tz_net_close_fd(descriptor);
        return -tz_net_error(error);
    }
    return tz_net_publish(output, descriptor, operation == 0 ? TZ_NET_STREAM : operation == 1 ? TZ_NET_LISTENER : TZ_NET_DATAGRAM, family, operation == 0 ? (const struct sockaddr *)&target : NULL);
}

// Takes a connection from a listener, waiting at most `timeout` milliseconds (-1: for one; 0: not at all, and
// -TZ_NET_PENDING when none is waiting). A handle and the local and peer address records, or the negated status.
TZ_NET_API int64_t tsuzuri_net_accept(struct tz_net_buffer *output, int64_t handle, int64_t timeout) {
    tz_net_clear(output);
    struct tz_net_socket listener;
    if (!tz_net_find(handle, TZ_NET_LISTENER, &listener)) return -tz_net_bad_handle();
    int64_t deadline = tz_net_deadline(timeout);
    struct sockaddr_storage peer;
    for (;;) {
        socklen_t peer_length = sizeof peer;
#if defined(TZ_NET_ATOMIC_FLAGS)
        tz_net_fd descriptor = accept4(listener.descriptor, (struct sockaddr *)&peer, &peer_length, SOCK_CLOEXEC | SOCK_NONBLOCK);
#else
        tz_net_fd descriptor = accept(listener.descriptor, (struct sockaddr *)&peer, &peer_length);
#endif
        if (descriptor != TZ_NET_NO_FD) {
            if (tz_net_prepare(descriptor) == 0) {
                return tz_net_publish(output, descriptor, TZ_NET_STREAM, listener.family, (const struct sockaddr *)&peer);
            }
            int error = tz_net_errno();
            tz_net_close_fd(descriptor);
            // macOS refuses a socket option (EINVAL) on a connection that the peer has already reset: like
            // ECONNABORTED, that is a connection that is gone, and the next one is the answer.
            if (error != TZ_NET_E_INVAL && error != TZ_NET_E_CONNABORTED) return -tz_net_error(error);
            continue;
        }
        int error = tz_net_errno();
        // A connection that the peer gave up before this call is not an error for the next one.
        if (error == TZ_NET_E_INTR || error == TZ_NET_E_CONNABORTED) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return -tz_net_error(error);
        error = tz_net_wait(listener.descriptor, TZ_NET_POLL_READ, deadline);
        if (error != 0) return -tz_net_error(error);
    }
}

// One receive on a stream into `buffer`: the count, or -1 and the error number.
static int64_t tz_net_receive(tz_net_fd descriptor, unsigned char *buffer, size_t length) {
#if defined(_WIN32)
    int count = recv(descriptor, (char *)buffer, length > INT_MAX ? INT_MAX : (int)length, 0);
    return count == SOCKET_ERROR ? -1 : count;
#else
    return (int64_t)recv(descriptor, buffer, length, 0);
#endif
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

// One datagram into `buffer` and its source into `source`: the count, or -1 and the error number. A datagram
// that does not fit is consumed, and reported as truncated by `*truncated` (POSIX) or by the error (Windows).
static int64_t tz_net_receive_from(tz_net_fd descriptor, unsigned char *buffer, size_t capacity, struct sockaddr_storage *source, int *truncated) {
    *truncated = 0;
#if defined(_WIN32)
    int source_length = sizeof *source;
    int count = recvfrom(descriptor, (char *)buffer, (int)capacity, 0, (struct sockaddr *)source, &source_length);
    return count == SOCKET_ERROR ? -1 : count;
#else
    struct iovec vector;
    struct msghdr message;
    memset(&message, 0, sizeof message);
    vector.iov_base = buffer;
    vector.iov_len = capacity;
    message.msg_name = source;
    message.msg_namelen = sizeof *source;
    message.msg_iov = &vector;
    message.msg_iovlen = 1;
    ssize_t count = recvmsg(descriptor, &message, 0);
    *truncated = count >= 0 && (message.msg_flags & MSG_TRUNC) != 0;
    return (int64_t)count;
#endif
}

// A datagram: its source address record, then its bytes. A datagram longer than `maximum` is consumed and EMSGSIZE.
static int64_t tz_net_receive_datagram(struct tz_net_buffer *output, tz_net_fd descriptor, int64_t maximum, int64_t deadline) {
    // No datagram is longer than 65,535 bytes, so a bigger buffer never fills.
    size_t capacity = maximum < TZ_NET_MAX_DATAGRAM ? (size_t)maximum : TZ_NET_MAX_DATAGRAM;
    unsigned char packet[TZ_NET_RECORD + TZ_NET_MAX_DATAGRAM];
    for (;;) {
        struct sockaddr_storage source;
        int truncated;
        int64_t count = tz_net_receive_from(descriptor, packet + TZ_NET_RECORD, capacity, &source, &truncated);
        if (count >= 0) {
            if (truncated) return tz_net_error(TZ_NET_E_MSGSIZE);
            tz_net_record(packet, (const struct sockaddr *)&source);
            return tz_net_deliver(output, packet, TZ_NET_RECORD + (size_t)count);
        }
        int error = tz_net_errno();
        if (error == TZ_NET_E_INTR) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
        error = tz_net_wait(descriptor, TZ_NET_POLL_READ, deadline);
        if (error != 0) return tz_net_error(error);
    }
}

// What has arrived on a stream, at most `maximum` bytes, none at the end of the stream. A small read lands on
// the stack and is copied exactly; a big one lands in the memory it is delivered in.
static int64_t tz_net_receive_stream(struct tz_net_buffer *output, tz_net_fd descriptor, int64_t maximum, int64_t deadline) {
    unsigned char stack[TZ_NET_STACK_READ];
    int owned = maximum > TZ_NET_STACK_READ;
    unsigned char *buffer = owned ? tsuzuri_alloc(maximum) : stack;
    for (;;) {
        int64_t count = tz_net_receive(descriptor, buffer, (size_t)maximum);
        if (count >= 0) return tz_net_finish_read(output, buffer, owned, maximum, (size_t)count);
        int error = tz_net_errno();
        if (error != TZ_NET_E_INTR) {
            if (TZ_NET_WOULD_BLOCK(error)) error = tz_net_wait(descriptor, TZ_NET_POLL_READ, deadline);
            if (error != 0) {
                if (owned) tsuzuri_free(buffer);
                return tz_net_error(error);
            }
        }
    }
}

// op 0 receives from a stream: what has arrived, at most `maximum` bytes, none at the end of the stream. op 1
// receives one datagram: its source address record, then the datagram. Waits at most `timeout` milliseconds
// (-1: for data) for the whole call; 0 does not wait, and the status is TZ_NET_PENDING when nothing has arrived.
TZ_NET_API int64_t tsuzuri_net_read(struct tz_net_buffer *output, int32_t operation, int64_t handle, int64_t maximum, int64_t timeout) {
    tz_net_clear(output);
    if ((operation != 0 && operation != 1) || maximum < 1 || maximum > TZ_NET_MAX_READ) return tz_net_invalid_input();
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, operation == 0 ? TZ_NET_STREAM : TZ_NET_DATAGRAM, &socket_entry)) return tz_net_bad_handle();
    int64_t deadline = tz_net_deadline(timeout);
    return operation == 1 ? tz_net_receive_datagram(output, socket_entry.descriptor, maximum, deadline)
                          : tz_net_receive_stream(output, socket_entry.descriptor, maximum, deadline);
}

// Sends `length` bytes of a stream once: the count, or -1 and the error number.
static int64_t tz_net_send_stream(tz_net_fd descriptor, const unsigned char *data, size_t length) {
#if defined(_WIN32)
    int count = send(descriptor, (const char *)data, length > INT_MAX ? INT_MAX : (int)length, 0);
    return count == SOCKET_ERROR ? -1 : count;
#else
    return (int64_t)send(descriptor, data, length, TZ_NET_SEND_FLAGS);
#endif
}

// Sends one datagram: 0 or -1 and the error number.
static int tz_net_send_datagram(tz_net_fd descriptor, const unsigned char *data, size_t length, const struct sockaddr *target, socklen_t size) {
#if defined(_WIN32)
    return sendto(descriptor, (const char *)data, (int)length, TZ_NET_SEND_FLAGS, target, size) == SOCKET_ERROR ? -1 : 0;
#else
    return sendto(descriptor, data, length, TZ_NET_SEND_FLAGS, target, size) < 0 ? -1 : 0;
#endif
}

// op 0 sends all of `data` on a stream, however many sends that takes, within `timeout` milliseconds (-1: no
// limit) for the whole call. op 1 sends `data` as one datagram to the address (the timeout does not apply).
TZ_NET_API int64_t tsuzuri_net_write(int32_t operation, int64_t handle, const unsigned char *data, int64_t length, int64_t meta, int64_t high, int64_t low, int64_t timeout) {
    if ((operation != 0 && operation != 1) || length < 0) return tz_net_invalid_input();
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, operation == 0 ? TZ_NET_STREAM : TZ_NET_DATAGRAM, &socket_entry)) return tz_net_bad_handle();
    tz_net_fd descriptor = socket_entry.descriptor;
    if (operation == 1) {
        if (length > TZ_NET_MAX_DATAGRAM) return tz_net_error(TZ_NET_E_MSGSIZE);
        struct sockaddr_storage target;
        socklen_t size = tz_net_address(&target, meta, high, low);
        for (;;) {
            if (tz_net_send_datagram(descriptor, data, (size_t)length, (struct sockaddr *)&target, size) == 0) return 0;
            int error = tz_net_errno();
            if (error == TZ_NET_E_INTR) continue;
            if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
            error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, -1);
            if (error != 0) return tz_net_error(error);
        }
    }
    int64_t deadline = tz_net_deadline(timeout);
    int64_t sent = 0;
    while (sent < length) {
        int64_t chunk = length - sent < TZ_NET_MAX_SEND ? length - sent : TZ_NET_MAX_SEND;
        int64_t count = tz_net_send_stream(descriptor, data + sent, (size_t)chunk);
        if (count > 0) {
            sent += count;
            continue;
        }
        int error = count < 0 ? tz_net_errno() : TZ_NET_E_WOULDBLOCK;
        if (error == TZ_NET_E_INTR) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
        error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, deadline);
        if (error != 0) return tz_net_error(error);
    }
    return 0;
}

// Sends from `data[offset, length)` without waiting. op 0 sends as much of a stream's bytes as the system takes
// and returns how many (0 when nothing remains); op 1 sends all of `data` as one datagram to the address and
// returns `length`. A call that would have to wait returns -TZ_NET_PENDING (when nothing was sent), and any other
// failure the negated status; a failure after some bytes were sent is left to the next call.
TZ_NET_API int64_t tsuzuri_net_send(int32_t operation, int64_t handle, const unsigned char *data, int64_t length, int64_t offset, int64_t meta, int64_t high, int64_t low) {
    if ((operation != 0 && operation != 1) || length < 0 || offset < 0 || offset > length) return -tz_net_invalid_input();
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, operation == 0 ? TZ_NET_STREAM : TZ_NET_DATAGRAM, &socket_entry)) return -tz_net_bad_handle();
    tz_net_fd descriptor = socket_entry.descriptor;
    if (operation == 1) {
        if (length > TZ_NET_MAX_DATAGRAM) return -tz_net_error(TZ_NET_E_MSGSIZE);
        struct sockaddr_storage target;
        socklen_t size = tz_net_address(&target, meta, high, low);
        for (;;) {
            if (tz_net_send_datagram(descriptor, data, (size_t)length, (struct sockaddr *)&target, size) == 0) return length;
            int error = tz_net_errno();
            if (error != TZ_NET_E_INTR) return -tz_net_error(TZ_NET_WOULD_BLOCK(error) ? TZ_NET_E_PENDING : error);
        }
    }
    int64_t sent = 0;
    while (offset + sent < length) {
        int64_t chunk = length - offset - sent < TZ_NET_MAX_SEND ? length - offset - sent : TZ_NET_MAX_SEND;
        int64_t count = tz_net_send_stream(descriptor, data + offset + sent, (size_t)chunk);
        if (count > 0) {
            sent += count;
            continue;
        }
        int error = count < 0 ? tz_net_errno() : TZ_NET_E_WOULDBLOCK;
        if (error == TZ_NET_E_INTR) continue;
        if (sent > 0) return sent;
        return -tz_net_error(TZ_NET_WOULD_BLOCK(error) ? TZ_NET_E_PENDING : error);
    }
    return sent;
}

// Closes a handle: the handle is dead afterwards even when the system reports a failure. Waits on the socket end
// first, and the descriptor closes after that.
static int64_t tz_net_close_handle(int64_t handle) {
    tz_net_fd descriptor;
    if (!tz_net_forget(handle, &descriptor)) return tz_net_bad_handle();
    tz_net_cancel_handle(handle);
    // After EINTR the descriptor state is unspecified, and retrying could close another one.
    if (tz_net_close_fd(descriptor) == 0) return 0;
    int error = tz_net_errno();
    return error == TZ_NET_E_INTR ? 0 : tz_net_error(error);
}

// op 0 closes a socket of any kind. op 1, 2, and 3 end the reading, the writing, or both of a stream.
TZ_NET_API int64_t tsuzuri_net_close(int32_t operation, int64_t handle) {
    if (operation < 0 || operation > 3) return tz_net_invalid_input();
    if (operation == 0) return tz_net_close_handle(handle);
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, TZ_NET_STREAM, &socket_entry)) return tz_net_bad_handle();
    int how = operation == 1 ? TZ_NET_SHUTDOWN_READ : operation == 2 ? TZ_NET_SHUTDOWN_WRITE : TZ_NET_SHUTDOWN_BOTH;
    return shutdown(socket_entry.descriptor, how) == 0 ? 0 : tz_net_error(tz_net_errno());
}

// The 40 bytes of addresses of an open socket: its local address record, then its peer's (zeros for a listener
// and a UDP socket), as they were when it was opened.
TZ_NET_API int64_t tsuzuri_net_names(struct tz_net_buffer *output, int64_t handle) {
    tz_net_clear(output);
    struct tz_net_socket socket_entry;
    if (!tz_net_find(handle, 0, &socket_entry)) return tz_net_bad_handle();
    return tz_net_deliver(output, socket_entry.names, sizeof socket_entry.names);
}

// ---- Async operations ----

// Starts a wait for `handle` to be ready for `events` (TZ_NET_WATCH_READ or TZ_NET_WATCH_WRITE) within `timeout`
// milliseconds (-1: no limit), and completes `operation` through `post`: with 0 when the socket is ready (an
// error or a hang-up counts, and the retry reports it) or closed, and otherwise with the status of why it is not:
// ETIMEDOUT at the deadline, EBADF for a handle that is not open. Several waits may be for one socket.
TZ_NET_API void tsuzuri_net_watch(tz_net_post_function post, int64_t operation, int64_t handle, int32_t events, int64_t timeout) {
    int64_t failure = 0;
    int added = 0;
    if ((events != TZ_NET_WATCH_READ && events != TZ_NET_WATCH_WRITE) || timeout < -1) {
        failure = tz_net_invalid_input();
    } else if (timeout == 0) {
        failure = tz_net_error(TZ_NET_E_TIMEDOUT);
    } else {
        // The descriptor is looked up under the poll lock, and a close takes that lock before it closes the
        // descriptor, so a wait never watches a descriptor number that another socket has taken over.
        tz_net_mutex_lock(&tz_net_poll_lock);
        struct tz_net_socket socket_entry;
        if (!tz_net_find(handle, 0, &socket_entry)) {
            failure = tz_net_bad_handle();
        } else {
            struct tz_net_wait wait;
            memset(&wait, 0, sizeof wait);
            wait.operation = operation;
            wait.handle = handle;
            wait.descriptor = socket_entry.descriptor;
            wait.events = events == TZ_NET_WATCH_READ ? TZ_NET_POLL_READ : TZ_NET_POLL_WRITE;
            wait.deadline = timeout < 0 ? -1 : tz_net_now() + timeout * INT64_C(1000000);
            tz_net_post = post;
            int error = tz_net_add_wait(&wait);
            if (error != 0) failure = tz_net_error(error);
            else added = 1;
        }
        tz_net_mutex_unlock(&tz_net_poll_lock);
    }
    if (!added) post(operation, failure);
}

// Connects without waiting here: completes `operation` through `post` with a new handle (positive) when the
// connection is made, or the negated status when it fails or `timeout` milliseconds (-1: for the system) pass.
// The socket belongs to the operation until then: `tsuzuri_net_unwatch` closes it, and a completion that
// `post` refuses closes the new socket, so the connect never leaves a descriptor that nobody can close.
TZ_NET_API void tsuzuri_net_connect(tz_net_post_function post, int64_t operation, int64_t meta, int64_t high, int64_t low, int64_t timeout) {
    int64_t result = 0;
    int error = tz_net_start();
    struct sockaddr_storage target;
    socklen_t length = tz_net_address(&target, meta, high, low);
    int family = target.ss_family;
    tz_net_fd descriptor = TZ_NET_NO_FD;
    if (error == 0 && timeout == 0) error = TZ_NET_E_TIMEDOUT;
    if (error == 0 && timeout < -1) error = TZ_NET_E_INVAL;
    if (error == 0) {
        descriptor = tz_net_create(family, SOCK_STREAM);
        if (descriptor == TZ_NET_NO_FD) error = tz_net_errno();
    }
    if (error == 0 && connect(descriptor, (struct sockaddr *)&target, length) != 0) {
        error = tz_net_errno();
        if (TZ_NET_CONNECT_PENDING(error)) {
            struct tz_net_wait wait;
            memset(&wait, 0, sizeof wait);
            wait.operation = operation;
            wait.descriptor = descriptor;
            wait.events = TZ_NET_POLL_WRITE;
            wait.connecting = 1;
            wait.family = family;
            wait.deadline = timeout < 0 ? -1 : tz_net_now() + timeout * INT64_C(1000000);
            tz_net_record(wait.peer, (const struct sockaddr *)&target);
            tz_net_mutex_lock(&tz_net_poll_lock);
            tz_net_post = post;
            error = tz_net_add_wait(&wait);
            tz_net_mutex_unlock(&tz_net_poll_lock);
            if (error == 0) return;
        }
    } else if (error == 0) {
        unsigned char records[2 * TZ_NET_RECORD];
        error = tz_net_names(descriptor, records, (const struct sockaddr *)&target);
        if (error == 0) error = tz_net_register(descriptor, TZ_NET_STREAM, family, records, &result);
    }
    if (error != 0) {
        if (descriptor != TZ_NET_NO_FD) tz_net_close_fd(descriptor);
        result = -tz_net_error(error);
    }
    if (post(operation, result) == 0 && result > 0) tz_net_discard(result);
}

// Takes `operation` back: a wait that has not completed is removed, and a connect that is under way is closed.
// An operation that has completed or never started is left alone.
TZ_NET_API void tsuzuri_net_unwatch(int64_t operation) {
    tz_net_mutex_lock(&tz_net_poll_lock);
    if (tz_net_wait_count > 0) tz_net_remove_waits(operation, 0, 0);
    tz_net_mutex_unlock(&tz_net_poll_lock);
}

// ---- Error classes and name resolution ----

// The Net.ErrorKind index of an error number: 0 TimedOut, 1 ConnectionRefused, 2 ConnectionReset, 3 AddressInUse,
// 4 AddressNotAvailable, 5 Unreachable, and 6 for anything else (and for 0, which is no system code).
TZ_NET_API int64_t tsuzuri_net_classify(int32_t code) {
    switch (code) {
    case TZ_NET_E_TIMEDOUT:
        return 0;
    case TZ_NET_E_CONNREFUSED:
        return 1;
    case TZ_NET_E_CONNRESET:
    case TZ_NET_E_CONNABORTED:
    case TZ_NET_E_PIPE:
        return 2;
    case TZ_NET_E_ADDRINUSE:
        return 3;
    case TZ_NET_E_ADDRNOTAVAIL:
        return 4;
    case TZ_NET_E_NETUNREACH:
    case TZ_NET_E_HOSTUNREACH:
    case TZ_NET_E_NETDOWN:
    case TZ_NET_E_HOSTDOWN:
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
#if defined(EAI_SYSTEM)
    if (failure == EAI_SYSTEM) return tz_net_error(errno);
#endif
    return tz_net_status(TZ_NET_OTHER, 0);
}

// Looks a host up (the system's resolver, which blocks and cannot be interrupted): the 20-byte address records
// with `port`, in the system's order, without repeats, at most 64. The runtime does not decide what an address is:
// `Net.resolve` gives the host text that `Net.parse_ip` can read to that parser alone, and a host that the system
// itself reads as a numeric address ("127.1", "0x7f000001", "fe80::1%en0": each system has its own forms) is
// refused here (InvalidInput), so no text that the strict parser rejects is ever resolved as an address.
TZ_NET_API int64_t tsuzuri_net_resolve(struct tz_net_buffer *output, const unsigned char *host, int64_t length, int64_t port) {
    tz_net_clear(output);
    if (length < 1 || length > 253 || port < 0 || port > 65535 || memchr(host, 0, (size_t)length) != NULL) return tz_net_invalid_input();
    int started = tz_net_start();
    if (started != 0) return tz_net_error(started);
    char name[254];
    memcpy(name, host, (size_t)length);
    name[length] = '\0';
    struct addrinfo hints;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    struct addrinfo *list = NULL;
    // AI_NUMERICHOST forbids every name lookup: it only asks whether the system would read the text as an address.
    hints.ai_flags = AI_NUMERICHOST;
    if (getaddrinfo(name, NULL, &hints, &list) == 0) {
        freeaddrinfo(list);
        return tz_net_invalid_input();
    }
    hints.ai_flags = 0;
    list = NULL;
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
