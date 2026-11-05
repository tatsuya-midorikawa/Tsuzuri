// Sockets of the standard Net module (E09): TCP streams and listeners, UDP sockets, and name resolution. macOS
// and Linux use POSIX sockets and Windows uses Winsock (the driver links ws2_32). The driver links this file when
// the program reaches Net.__*.
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
// A timeout of 0 means "try once": a call that would have to wait returns TZ_NET_PENDING (kind Other, code 0),
// which no system failure has. A wait on a tzroutine waits in the scheduler's netpoller (TZ08), and any other
// wait in poll.
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#if defined(_WIN32)
// Another part of the same translation unit may define this macro too, to the same value.
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
// The request that turns off the WSAECONNRESET of a UDP socket (mstcpip.h of one SDK, nowhere in another).
#ifndef SIO_UDP_CONNRESET
#define SIO_UDP_CONNRESET _WSAIOW(IOC_VENDOR, 12)
#endif
// The MSVC target reads this request for the import library; a MinGW target takes it from the link line (-lws2_32).
#if defined(_MSC_VER)
#pragma comment(lib, "ws2_32.lib")
#endif
// The scheduler's netpoller (src/runtime/tzroutine.c, TZ08) polls sockets when this file is in the unit.
#define TZ_NET_WINSOCK 1
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

/* The blocking-call hooks (TZ08): os.c, io.c, net.c and gpu.c call them around a call that may wait
 * for the OS, and the scheduler sets them when it starts (tzr_init), so that a tzroutine in such a
 * call lets sysmon give its P away. The parts of the runtime are one translation unit, so the first
 * part that has this block defines them; a program without tzroutines leaves them null. */
#ifndef TSUZURI_BLOCKING_HOOKS_DEFINED
#define TSUZURI_BLOCKING_HOOKS_DEFINED
#include <stdatomic.h>
/* Windows has no weak data: a selectany definition is shared by the units that define it (the runner of
 * `tsuzuri test` compiles each part of the runtime alone). */
#if defined(_WIN32)
__attribute__((selectany)) void (*_Atomic tsuzuri_blocking_enter_hook)(void) = 0;
__attribute__((selectany)) void (*_Atomic tsuzuri_blocking_exit_hook)(void) = 0;
#else
__attribute__((weak, visibility("hidden"))) void (*_Atomic tsuzuri_blocking_enter_hook)(void);
__attribute__((weak, visibility("hidden"))) void (*_Atomic tsuzuri_blocking_exit_hook)(void);
#endif
#define TZ_BLOCKING_CALL(hook)                                                                   \
    do {                                                                                         \
        void (*tz_blocking_hook)(void) = atomic_load_explicit(&(hook), memory_order_acquire);   \
        if (tz_blocking_hook != 0) tz_blocking_hook();                                          \
    } while (0)
#define TZ_BLOCKING_ENTER() TZ_BLOCKING_CALL(tsuzuri_blocking_enter_hook)
#define TZ_BLOCKING_EXIT() TZ_BLOCKING_CALL(tsuzuri_blocking_exit_hook)
#endif

/* net.c's hooks (TZ08 Phase 2), which the scheduler sets when it starts: the wait of a tzroutine for
 * a socket, in the netpoller (0 ready, 1 timed out, -1 when net.c must poll itself), and the close of
 * a socket, before the descriptor closes (the waits for it end). A wait asks `open(handle,
 * descriptor)` (handle 0: none) under the lock that the close hook takes, and net.c forgets the
 * handle before it calls the close hook: a wait either finds its handle closed and does not start, or
 * is woken by the close, also when the descriptor is taken by another socket in between. Defined by
 * the first part that has this block. */
#ifndef TSUZURI_NET_HOOKS_DEFINED
#define TSUZURI_NET_HOOKS_DEFINED
#if defined(_WIN32)
__attribute__((selectany)) int32_t (*_Atomic tsuzuri_net_wait_hook)(intptr_t descriptor, int32_t write, int64_t timeout,
                                                                     int32_t (*open)(int64_t, intptr_t), int64_t handle) = 0;
__attribute__((selectany)) void (*_Atomic tsuzuri_net_close_hook)(intptr_t descriptor) = 0;
#else
__attribute__((weak, visibility("hidden"))) int32_t (*_Atomic tsuzuri_net_wait_hook)(
    intptr_t descriptor, int32_t write, int64_t timeout, int32_t (*open)(int64_t, intptr_t), int64_t handle);
__attribute__((weak, visibility("hidden"))) void (*_Atomic tsuzuri_net_close_hook)(intptr_t descriptor);
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

// An error number that no system reports: a call that was told not to wait would have had to.
#define TZ_NET_E_PENDING (-1)
// What tz_net_wait returns when the handle that it waited for was closed meanwhile (TZ08): the call
// reports the closed handle, as the next call on it would.
#define TZ_NET_E_CLOSED (-3)
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

// Not inlined: on a tzroutine a wait in the netpoller may go on on another thread (TZ08), and an inlined
// read could keep the address of the first thread's errno across it.
__attribute__((noinline)) static int tz_net_errno(void) { return errno; }

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
    if (error == TZ_NET_E_CLOSED) return tz_net_status(TZ_NET_INVALID_INPUT, TZ_NET_E_BADF);
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

// Opens a slot for `descriptor`: 0 and the handle, or an error number (the descriptor stays the caller's).
static int tz_net_register(tz_net_fd descriptor, int kind, int family, int64_t *handle) {
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

// Whether `handle` is still open with `descriptor`. The netpoller asks it under the lock that the close hook takes,
// which a close calls after it has forgotten the handle (TZ08).
static int32_t tz_net_still_open(int64_t handle, intptr_t descriptor) {
    struct tz_net_socket open;
    return tz_net_find(handle, 0, &open) && (intptr_t)open.descriptor == descriptor;
}

// Waits until the socket can do `events` or the deadline passes: 0 when it can (an error or a hang-up counts, and
// the next call reports it), TZ_NET_E_TIMEDOUT at the deadline, TZ_NET_E_PENDING for a call that must not wait,
// or the error number of poll. An interrupted poll starts again with the time that is left, rounded up to a
// whole millisecond so that the last fraction does not spin. On a tzroutine the scheduler's netpoller waits
// instead, and only the tzroutine waits (TZ08); it leaves the wait to poll when it cannot take it. Such a wait
// may end because another tzroutine closed the socket, or not start because it was closed already: when
// `handle` (0 for none) is no longer open with this descriptor, the result is TZ_NET_E_CLOSED.
static int tz_net_wait(tz_net_fd descriptor, short events, int64_t deadline, int64_t handle) {
    if (deadline == TZ_NET_ATTEMPT) return TZ_NET_E_PENDING;
    int32_t (*netpoll)(intptr_t, int32_t, int64_t, int32_t (*)(int64_t, intptr_t), int64_t) =
        atomic_load_explicit(&tsuzuri_net_wait_hook, memory_order_acquire);
    for (;;) {
        int timeout = -1;
        int64_t remaining = -1;
        if (deadline >= 0) {
            remaining = deadline - tz_net_now();
            if (remaining <= 0) return TZ_NET_E_TIMEDOUT;
            int64_t milliseconds = (remaining + 999999) / 1000000;
            timeout = milliseconds > INT_MAX ? INT_MAX : (int)milliseconds;
        }
        if (netpoll != NULL) {
            int32_t waited = netpoll((intptr_t)descriptor, events == TZ_NET_POLL_WRITE, remaining, tz_net_still_open, handle);
            if (waited == 0) {
                struct tz_net_socket open;
                if (handle != 0 && (!tz_net_find(handle, 0, &open) || open.descriptor != descriptor)) return TZ_NET_E_CLOSED;
                return 0;
            }
            if (waited == 1) return TZ_NET_E_TIMEDOUT;
            netpoll = NULL;
        }
        tz_net_pollfd entry;
        entry.fd = descriptor;
        entry.events = events;
        entry.revents = 0;
        TZ_BLOCKING_ENTER();
        int ready = tz_net_poll(&entry, 1, timeout);
        int error = ready < 0 ? tz_net_errno() : 0;
        TZ_BLOCKING_EXIT();
        if (ready > 0) return 0;
        if (ready < 0 && error != TZ_NET_E_INTR) return error;
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
#if defined(_WIN32)
    if (ready == 0 && type == SOCK_DGRAM) {
        // Windows reports the ICMP "port unreachable" that follows a send to a closed port as WSAECONNRESET from the next
        // receive on the same socket, even when it is not connected, which no other system does: a server that answered a
        // client that has gone would read a ConnectionReset that belongs to nothing it asked. The request turns that off;
        // if it fails, the receive below still skips such an error, so the failure is not worth refusing the socket for.
        BOOL report = FALSE;
        DWORD returned = 0;
        (void)WSAIoctl(descriptor, SIO_UDP_CONNRESET, &report, sizeof report, NULL, 0, &returned, NULL, NULL);
    }
#endif
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

// Closes a handle that nobody has seen yet: the socket whose addresses could not be delivered.
static void tz_net_discard(int64_t handle) {
    tz_net_fd descriptor;
    if (tz_net_forget(handle, &descriptor)) tz_net_close_fd(descriptor);
}

// Registers an open socket and hands the caller its handle and the 40 bytes of addresses; a failure closes the socket.
static int64_t tz_net_publish(struct tz_net_buffer *output, tz_net_fd descriptor, int kind, int family, const struct sockaddr *peer) {
    unsigned char records[2 * TZ_NET_RECORD];
    int error = tz_net_names(descriptor, records, peer);
    int64_t handle = 0;
    if (error == 0) error = tz_net_register(descriptor, kind, family, &handle);
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
                error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, deadline, 0);
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

// An error of accept that belongs to a connection that is gone, not to the listener: the peer gave it up before this call
// took it, and the next connection is the answer. That is ECONNABORTED everywhere; Windows says WSAECONNRESET for a
// connection that the peer has reset (a client that connects and resets at once would end every accept loop otherwise).
#if defined(_WIN32)
#define TZ_NET_GONE_BEFORE_ACCEPT(error) ((error) == TZ_NET_E_CONNABORTED || (error) == TZ_NET_E_CONNRESET)
#else
#define TZ_NET_GONE_BEFORE_ACCEPT(error) ((error) == TZ_NET_E_CONNABORTED)
#endif

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
            // ECONNABORTED (and WSAECONNRESET on Windows), that is a connection that is gone, and the next one is the answer.
            if (error != TZ_NET_E_INVAL && !TZ_NET_GONE_BEFORE_ACCEPT(error)) return -tz_net_error(error);
            continue;
        }
        int error = tz_net_errno();
        // A connection that the peer gave up before this call is not an error for the next one.
        if (error == TZ_NET_E_INTR || TZ_NET_GONE_BEFORE_ACCEPT(error)) continue;
        if (!TZ_NET_WOULD_BLOCK(error)) return -tz_net_error(error);
        error = tz_net_wait(listener.descriptor, TZ_NET_POLL_READ, deadline, handle);
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
static int64_t tz_net_receive_datagram(struct tz_net_buffer *output, tz_net_fd descriptor, int64_t maximum, int64_t deadline, int64_t handle) {
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
#if defined(_WIN32)
        // The report of an earlier send to a closed port (see tz_net_create): it says nothing about this receive, so it is
        // skipped, and the datagram that is there, or the wait, comes next.
        if (error == TZ_NET_E_CONNRESET) continue;
#endif
        if (!TZ_NET_WOULD_BLOCK(error)) return tz_net_error(error);
        error = tz_net_wait(descriptor, TZ_NET_POLL_READ, deadline, handle);
        if (error != 0) return tz_net_error(error);
    }
}

// What has arrived on a stream, at most `maximum` bytes, none at the end of the stream. A small read lands on
// the stack and is copied exactly; a big one lands in the memory it is delivered in.
static int64_t tz_net_receive_stream(struct tz_net_buffer *output, tz_net_fd descriptor, int64_t maximum, int64_t deadline, int64_t handle) {
    unsigned char stack[TZ_NET_STACK_READ];
    int owned = maximum > TZ_NET_STACK_READ;
    unsigned char *buffer = owned ? tsuzuri_alloc(maximum) : stack;
    for (;;) {
        int64_t count = tz_net_receive(descriptor, buffer, (size_t)maximum);
        if (count >= 0) return tz_net_finish_read(output, buffer, owned, maximum, (size_t)count);
        int error = tz_net_errno();
        if (error != TZ_NET_E_INTR) {
            if (TZ_NET_WOULD_BLOCK(error)) error = tz_net_wait(descriptor, TZ_NET_POLL_READ, deadline, handle);
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
    return operation == 1 ? tz_net_receive_datagram(output, socket_entry.descriptor, maximum, deadline, handle)
                          : tz_net_receive_stream(output, socket_entry.descriptor, maximum, deadline, handle);
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
            error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, -1, handle);
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
        error = tz_net_wait(descriptor, TZ_NET_POLL_WRITE, deadline, handle);
        if (error != 0) return tz_net_error(error);
    }
    return 0;
}

// Closes a handle: the handle is dead afterwards even when the system reports a failure. Waits on the socket end
// first, and the descriptor closes after that.
static int64_t tz_net_close_handle(int64_t handle) {
    tz_net_fd descriptor;
    if (!tz_net_forget(handle, &descriptor)) return tz_net_bad_handle();
    // A tzroutine that waits on the socket wakes and reports the closed handle; a wait that starts later finds
    // the handle forgotten above and does not start (TZ08).
    void (*closing)(intptr_t) = atomic_load_explicit(&tsuzuri_net_close_hook, memory_order_acquire);
    if (closing != NULL) closing((intptr_t)descriptor);
    // After EINTR the descriptor state is unspecified, and retrying could close another one. A close can
    // wait (a lingering socket): a tzroutine lets its P go (TZ08).
    TZ_BLOCKING_ENTER();
    int closed = tz_net_close_fd(descriptor);
    int error = closed == 0 ? 0 : tz_net_errno();
    TZ_BLOCKING_EXIT();
    if (closed == 0) return 0;
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
    // The resolver may wait for the network: a tzroutine lets its P go (TZ08), which keeps errno.
    TZ_BLOCKING_ENTER();
    int failure = getaddrinfo(name, NULL, &hints, &list);
    TZ_BLOCKING_EXIT();
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
