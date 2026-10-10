// Tests of the socket runtime of the standard Net module (E09) without the compiler: the readiness waits of the
// poller thread, the connect that a wait owns, closing and cancelling, and the descriptors that must not leak. It links
// src/runtime/net.c and completes operations through a recording `post`, as the IR passes `tsuzuri_async_post` in.
// Everything stays on 127.0.0.1 with ephemeral ports. No result depends on how long something took: a bound on waiting
// only says that the program is hung. Build it with the address, thread, or undefined-behaviour sanitizer as well.
#undef NDEBUG
#define _POSIX_C_SOURCE 200809L
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#endif
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

struct tz_net_buffer {
    unsigned char *data;
    int64_t length;
};
typedef int32_t (*post_function)(int64_t, int64_t);

extern int64_t tsuzuri_net_open(struct tz_net_buffer *, int32_t, int64_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_accept(struct tz_net_buffer *, int64_t, int64_t);
extern int64_t tsuzuri_net_read(struct tz_net_buffer *, int32_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_write(int32_t, int64_t, const unsigned char *, int64_t, int64_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_send(int32_t, int64_t, const unsigned char *, int64_t, int64_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_close(int32_t, int64_t);
extern int64_t tsuzuri_net_names(struct tz_net_buffer *, int64_t);
extern int64_t tsuzuri_net_resolve(struct tz_net_buffer *, const unsigned char *, int64_t, int64_t);
extern void tsuzuri_net_watch(post_function, int64_t, int64_t, int32_t, int64_t);
extern void tsuzuri_net_connect(post_function, int64_t, int64_t, int64_t, int64_t, int64_t);
extern void tsuzuri_net_unwatch(int64_t);

static atomic_int_fast64_t live;
void *tsuzuri_alloc(int64_t size) {
    atomic_fetch_add(&live, 1);
    return malloc((size_t)size);
}
void tsuzuri_free(void *pointer) {
    if (pointer) atomic_fetch_sub(&live, 1);
    free(pointer);
}

#define LOOPBACK INT64_C(0x7f000001)
// The address record that a received datagram starts with.
#define RECORD_BYTES 20
#define OTHER INT64_C(7)
#define INVALID_INPUT INT64_C(4)
// The status of a call that was told not to wait and would have had to: kind Other, code 0.
#define PENDING (INT64_C(7) << 32)
#define TIMED_OUT ((INT64_C(7) << 32) | ETIMEDOUT)
#define NONE INT64_MIN
// Operations of the stress test are counted, not recorded.
#define STRESS_BASE INT64_C(1000000)

// The completions that the poller posted, and the one operation whose post is refused (as after a cancel).
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t arrived = PTHREAD_COND_INITIALIZER;
struct completion {
    int64_t operation;
    int64_t value;
};
static struct completion completions[4096];
static size_t completion_count;
static int64_t refused_operation;
static atomic_int_fast64_t stress_completions;

static int32_t post(int64_t operation, int64_t value) {
    if (operation >= STRESS_BASE) {
        atomic_fetch_add(&stress_completions, 1);
        return 1;
    }
    pthread_mutex_lock(&lock);
    int accepted = operation != refused_operation;
    if (accepted) {
        assert(completion_count < sizeof completions / sizeof completions[0]);
        for (size_t index = 0; index < completion_count; index++) assert(completions[index].operation != operation && "an operation completes once");
        completions[completion_count].operation = operation;
        completions[completion_count].value = value;
        completion_count++;
        pthread_cond_broadcast(&arrived);
    }
    pthread_mutex_unlock(&lock);
    return accepted;
}

static void deadline_after(struct timespec *at, int milliseconds) {
    clock_gettime(CLOCK_REALTIME, at);
    at->tv_sec += milliseconds / 1000;
    at->tv_nsec += (long)(milliseconds % 1000) * 1000000L;
    if (at->tv_nsec >= 1000000000L) {
        at->tv_sec++;
        at->tv_nsec -= 1000000000L;
    }
}

// The value that `operation` completed with, waiting at most `milliseconds` for it: NONE when it did not.
static int64_t completion_of(int64_t operation, int milliseconds) {
    struct timespec until;
    deadline_after(&until, milliseconds);
    pthread_mutex_lock(&lock);
    for (;;) {
        for (size_t index = 0; index < completion_count; index++) {
            if (completions[index].operation == operation) {
                int64_t value = completions[index].value;
                pthread_mutex_unlock(&lock);
                return value;
            }
        }
        if (pthread_cond_timedwait(&arrived, &lock, &until) != 0) break;
    }
    pthread_mutex_unlock(&lock);
    return NONE;
}

// A completion is expected within a minute, which only a hung program misses.
static int64_t completed(int64_t operation) {
    int64_t value = completion_of(operation, 60000);
    assert(value != NONE && "the operation completed");
    return value;
}

static int descriptor_limit(void) {
    long limit = sysconf(_SC_OPEN_MAX);
    return limit < 0 || limit > 16384 ? 16384 : (int)limit;
}

static int open_descriptors(void) {
    int count = 0;
    int limit = descriptor_limit();
    for (int descriptor = 0; descriptor < limit; descriptor++) {
        if (fcntl(descriptor, F_GETFD) != -1) count++;
    }
    return count;
}

// The descriptors of the poller go when its last wait does, which is soon after: wait for the count to settle.
static void expect_descriptors(int expected) {
    for (int attempt = 0; attempt < 2000; attempt++) {
        if (open_descriptors() == expected) return;
        struct timespec pause = {0, 5 * 1000000L};
        nanosleep(&pause, NULL);
    }
    fprintf(stderr, "descriptors: %d, expected %d\n", open_descriptors(), expected);
    abort();
}

static int port_of(const unsigned char *record) {
    return (record[2] << 8) | record[3];
}

// A listener on a free loopback port: its handle and port.
static int64_t listen_on_loopback(int *port) {
    struct tz_net_buffer names;
    int64_t listener = tsuzuri_net_open(&names, 1, 0, 0, LOOPBACK, -1);
    assert(listener > 0 && names.length == 40);
    *port = port_of(names.data);
    tsuzuri_free(names.data);
    return listener;
}

static int64_t connect_to(int port) {
    struct tz_net_buffer names;
    int64_t handle = tsuzuri_net_open(&names, 0, port, 0, LOOPBACK, 5000);
    assert(handle > 0);
    tsuzuri_free(names.data);
    return handle;
}

static int64_t accept_from(int64_t listener) {
    struct tz_net_buffer names;
    int64_t handle = tsuzuri_net_accept(&names, listener, 5000);
    assert(handle > 0);
    tsuzuri_free(names.data);
    return handle;
}

// A connected pair on a listener of its own: (client, server).
static void pair_on(int64_t listener, int port, int64_t *client, int64_t *server) {
    *client = connect_to(port);
    *server = accept_from(listener);
}

static void send_byte(int64_t handle, unsigned char byte) {
    assert(tsuzuri_net_write(0, handle, &byte, 1, 0, 0, 0, 5000) == 0);
}

static int64_t read_now(int64_t handle, struct tz_net_buffer *bytes) {
    return tsuzuri_net_read(bytes, 0, handle, 64, 0);
}

static void close_all(const int64_t *handles, size_t count) {
    for (size_t index = 0; index < count; index++) assert(tsuzuri_net_close(0, handles[index]) == 0);
}

static atomic_int_fast64_t next_operation = 1;

static int64_t operation(void) {
    return atomic_fetch_add(&next_operation, 1);
}

// A wait that ends when the socket is readable, and a read that tries once.
static void test_ready(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    struct tz_net_buffer bytes;
    assert(read_now(server, &bytes) == PENDING && bytes.data == NULL && "an empty socket would wait");
    int64_t op = operation();
    tsuzuri_net_watch(post, op, server, 1, -1);
    assert(completion_of(op, 100) == NONE && "nothing yet");
    send_byte(client, 'x');
    assert(completed(op) == 0);
    assert(read_now(server, &bytes) == 0 && bytes.length == 1 && bytes.data[0] == 'x');
    tsuzuri_free(bytes.data);
    assert(read_now(server, &bytes) == PENDING);
    op = operation();
    tsuzuri_net_watch(post, op, client, 2, -1);
    assert(completed(op) == 0 && "an idle connection can be written");
    // A closed peer is readable: the wait ends, and the read says that it is the end.
    op = operation();
    tsuzuri_net_watch(post, op, server, 1, -1);
    assert(tsuzuri_net_close(0, client) == 0);
    assert(completed(op) == 0);
    assert(read_now(server, &bytes) == 0 && bytes.length == 0 && bytes.data == NULL);
    assert(tsuzuri_net_close(0, server) == 0);
    assert(tsuzuri_net_close(0, listener) == 0);
    expect_descriptors(baseline);
}

static void test_timeout(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    int64_t op = operation();
    tsuzuri_net_watch(post, op, server, 1, 30);
    assert(completed(op) == TIMED_OUT);
    // A timeout of 0 is a deadline that has passed; one below -1 and an unknown kind of event are not valid.
    op = operation();
    tsuzuri_net_watch(post, op, server, 1, 0);
    assert(completed(op) == TIMED_OUT);
    op = operation();
    tsuzuri_net_watch(post, op, server, 1, -5);
    assert((completed(op) >> 32) == INVALID_INPUT);
    op = operation();
    tsuzuri_net_watch(post, op, server, 3, -1);
    assert((completed(op) >> 32) == INVALID_INPUT);
    // Several waits for one socket, one of which times out first.
    int64_t first = operation(), second = operation();
    tsuzuri_net_watch(post, first, server, 1, 20);
    tsuzuri_net_watch(post, second, server, 1, -1);
    assert(completed(first) == TIMED_OUT);
    assert(completion_of(second, 50) == NONE);
    send_byte(client, 1);
    assert(completed(second) == 0);
    int64_t all[] = {client, server, listener};
    close_all(all, 3);
    expect_descriptors(baseline);
}

// Closing a socket that waits ends the wait, and the handle is dead; a handle that was never open is refused at once.
static void test_close_and_stale(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    int64_t first = operation(), second = operation();
    tsuzuri_net_watch(post, first, server, 1, -1);
    tsuzuri_net_watch(post, second, server, 1, -1);
    assert(completion_of(first, 50) == NONE);
    assert(tsuzuri_net_close(0, server) == 0);
    assert(completed(first) == 0 && completed(second) == 0 && "a wait on a closed socket ends");
    struct tz_net_buffer bytes;
    int64_t status = read_now(server, &bytes);
    assert((status >> 32) == INVALID_INPUT && (status & 0xffffffff) == EBADF);
    int64_t op = operation();
    tsuzuri_net_watch(post, op, server, 1, -1);
    assert(completed(op) == status && "a stale handle is refused");
    op = operation();
    tsuzuri_net_watch(post, op, 0, 1, -1);
    assert(completed(op) == status);
    op = operation();
    tsuzuri_net_watch(post, op, listener, 1, -1);
    assert(completion_of(op, 50) == NONE && "a listener can be watched");
    assert(tsuzuri_net_close(0, listener) == 0);
    assert(completed(op) == 0);
    assert(tsuzuri_net_close(0, client) == 0);
    expect_descriptors(baseline);
}

// A wait that was taken back completes nothing; the poller goes away and comes back with the next wait.
static void test_unwatch(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    int pair_descriptors = open_descriptors();
    int64_t op = operation();
    tsuzuri_net_watch(post, op, server, 1, -1);
    tsuzuri_net_unwatch(op);
    tsuzuri_net_unwatch(op);
    tsuzuri_net_unwatch(operation());
    send_byte(client, 1);
    assert(completion_of(op, 100) == NONE && "no completion after unwatch");
    expect_descriptors(pair_descriptors);
    op = operation();
    tsuzuri_net_watch(post, op, server, 1, -1);
    assert(completed(op) == 0 && "the poller starts again");
    expect_descriptors(pair_descriptors);
    int64_t all[] = {client, server, listener};
    close_all(all, 3);
    expect_descriptors(baseline);
}

// Many waits at once, each for its own socket, finish exactly once.
static void test_many(void) {
    enum { PAIRS = 60 };
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port);
    int64_t clients[PAIRS], servers[PAIRS], operations[PAIRS];
    for (int index = 0; index < PAIRS; index++) {
        pair_on(listener, port, &clients[index], &servers[index]);
        operations[index] = operation();
        tsuzuri_net_watch(post, operations[index], servers[index], 1, -1);
    }
    for (int index = PAIRS - 1; index >= 0; index -= 2) send_byte(clients[index], (unsigned char)index);
    for (int index = PAIRS - 1; index >= 0; index -= 2) assert(completed(operations[index]) == 0);
    for (int index = 0; index < PAIRS - 1; index += 2) assert(completion_of(operations[index], 1) == NONE);
    for (int index = 0; index < PAIRS - 1; index += 2) send_byte(clients[index], (unsigned char)index);
    for (int index = 0; index < PAIRS - 1; index += 2) assert(completed(operations[index]) == 0);
    close_all(clients, PAIRS);
    close_all(servers, PAIRS);
    assert(tsuzuri_net_close(0, listener) == 0);
    expect_descriptors(baseline);
}

// A connect that a wait owns: success, refusal, and a post that is refused.
static void test_connect(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port);
    int64_t op = operation();
    tsuzuri_net_connect(post, op, port, 0, LOOPBACK, -1);
    int64_t handle = completed(op);
    assert(handle > 0);
    struct tz_net_buffer names;
    assert(tsuzuri_net_names(&names, handle) == 0 && names.length == 40);
    // The local record, then the peer record: IPv4 on 127.0.0.1, the peer at the target's port.
    assert(names.data[0] == 4 && names.data[16] == 127 && names.data[19] == 1 && port_of(names.data) != 0 && port_of(names.data) != port);
    assert(names.data[20] == 4 && names.data[36] == 127 && names.data[39] == 1 && port_of(names.data + 20) == port);
    tsuzuri_free(names.data);
    int64_t server = accept_from(listener);
    send_byte(server, 'k');
    struct tz_net_buffer bytes;
    assert(tsuzuri_net_read(&bytes, 0, handle, 8, 5000) == 0 && bytes.length == 1 && bytes.data[0] == 'k');
    tsuzuri_free(bytes.data);
    int64_t both[] = {handle, server};
    close_all(both, 2);

    // The operation is gone before the connect ends: the post is refused and the new socket is closed, which the
    // listener's side sees as the end of the stream.
    op = operation();
    atomic_thread_fence(memory_order_seq_cst);
    pthread_mutex_lock(&lock);
    refused_operation = op;
    pthread_mutex_unlock(&lock);
    tsuzuri_net_connect(post, op, port, 0, LOOPBACK, -1);
    server = accept_from(listener);
    assert(tsuzuri_net_read(&bytes, 0, server, 8, 60000) == 0 && bytes.length == 0 && "the socket of the refused post was closed");
    assert(completion_of(op, 10) == NONE);
    assert(tsuzuri_net_close(0, server) == 0);
    pthread_mutex_lock(&lock);
    refused_operation = 0;
    pthread_mutex_unlock(&lock);

    // A port that nobody listens on, a timeout of 0 (a deadline that has passed), and one below -1.
    assert(tsuzuri_net_close(0, listener) == 0);
    op = operation();
    tsuzuri_net_connect(post, op, port, 0, LOOPBACK, 5000);
    int64_t refused = completed(op);
    assert(refused < 0 && ((-refused) >> 32) == OTHER && ((-refused) & 0xffffffff) == ECONNREFUSED);
    op = operation();
    tsuzuri_net_connect(post, op, port, 0, LOOPBACK, 0);
    assert(completed(op) == -TIMED_OUT);
    op = operation();
    tsuzuri_net_connect(post, op, port, 0, LOOPBACK, -3);
    assert(((-completed(op)) >> 32) == INVALID_INPUT);
    expect_descriptors(baseline);
}

// Fills the queue of a listener so that a connect to it hangs, as on Linux, where the system drops the connection
// request: the number of connections that were made, or minus one more than that when the next connect ended some
// other way (macOS resets it), with the error in `reason`.
static int fill_queue(int port, int64_t *handles, int capacity, int *reason) {
    for (int count = 0; count < capacity; count++) {
        struct tz_net_buffer names;
        int64_t handle = tsuzuri_net_open(&names, 0, port, 0, LOOPBACK, 200);
        if (handle > 0) {
            tsuzuri_free(names.data);
            handles[count] = handle;
        } else {
            *reason = (int)((-handle) & 0xffffffff);
            return -handle == TIMED_OUT ? count : -(count + 1);
        }
    }
    *reason = 0;
    return -(capacity + 1);
}

// A connect that hangs, taken back (its socket is closed and nothing completes), and one that reaches its deadline
// (the status, and its socket is closed too).
static void check_hanging_connect(int64_t host, int port) {
    int during = open_descriptors();
    int64_t op = operation();
    tsuzuri_net_connect(post, op, port, 0, host, -1);
    assert(completion_of(op, 200) == NONE && "the connect waits");
    assert(open_descriptors() > during && "the wait owns a socket");
    tsuzuri_net_unwatch(op);
    assert(completion_of(op, 100) == NONE);
    expect_descriptors(during);
    op = operation();
    tsuzuri_net_connect(post, op, port, 0, host, 60);
    assert(completed(op) == -TIMED_OUT);
    expect_descriptors(during);
}

static void test_connect_cancel(void) {
    struct rlimit limits;
    if (getrlimit(RLIMIT_NOFILE, &limits) == 0 && limits.rlim_cur < limits.rlim_max) {
        limits.rlim_cur = limits.rlim_max > 16384 ? 16384 : limits.rlim_max;
        if (setrlimit(RLIMIT_NOFILE, &limits) != 0) {
            limits.rlim_cur = 10240;
            setrlimit(RLIMIT_NOFILE, &limits);
        }
    }
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port);
    static int64_t filling[8192];
    int reason;
    int count = fill_queue(port, filling, 8192, &reason);
    int tested = count >= 0;
    if (tested) {
        check_hanging_connect(LOOPBACK, port);
        close_all(filling, (size_t)count);
    } else {
        close_all(filling, (size_t)(-count - 1));
    }
    assert(tsuzuri_net_close(0, listener) == 0);
    expect_descriptors(baseline);
    if (tested) return;
    // Another way to a connect that hangs is an address that nothing answers (192.0.2.1 is reserved for examples):
    // where a route to it exists, the request goes unanswered. Where it does not, there is nothing to cancel.
    struct tz_net_buffer names;
    int64_t probe = tsuzuri_net_open(&names, 0, 9, 0, INT64_C(0xc0000201), 200);
    if (probe == -TIMED_OUT) {
        check_hanging_connect(INT64_C(0xc0000201), 9);
        return;
    }
    if (probe > 0) {
        tsuzuri_free(names.data);
        assert(tsuzuri_net_close(0, probe) == 0);
    }
    printf("net runtime: the system neither hangs a connect to a full queue (error %d) nor to 192.0.2.1, so the cancel of a connect that waits is not tested\n", reason);
    expect_descriptors(baseline);
}

// Handles that producers made and consumers have not taken, and the churn of both.
#define SLOTS 16
static atomic_int_fast64_t slots[SLOTS];
static atomic_int_fast64_t stress_operations = STRESS_BASE;

struct stress {
    int port;
    int64_t listener;
    int rounds;
    int index;
};

static void *produce(void *argument) {
    struct stress *work = argument;
    for (int round = 0; round < work->rounds; round++) {
        int64_t client = connect_to(work->port);
        int64_t server = accept_from(work->listener);
        int64_t old = atomic_exchange(&slots[(round + work->index) % SLOTS], server);
        if (old != 0) assert(tsuzuri_net_close(0, old) == 0);
        if (round % 3 == 0) send_byte(client, 1);
        assert(tsuzuri_net_close(0, client) == 0);
    }
    return NULL;
}

static atomic_int stress_done;

static void *consume(void *argument) {
    struct stress *work = argument;
    for (int round = 0; !atomic_load(&stress_done) || round < 50; round++) {
        int64_t handle = atomic_exchange(&slots[(round * 7 + work->index) % SLOTS], 0);
        if (handle == 0) {
            sched_yield();
            continue;
        }
        int64_t reading = atomic_fetch_add(&stress_operations, 2), writing = reading + 1;
        tsuzuri_net_watch(post, reading, handle, 1, round % 5 == 0 ? 1 : -1);
        tsuzuri_net_watch(post, writing, handle, 2, round % 2 == 0 ? 1 : -1);
        switch (round % 4) {
        case 0:
            tsuzuri_net_unwatch(reading);
            break;
        case 1:
            tsuzuri_net_unwatch(writing);
            tsuzuri_net_unwatch(reading);
            break;
        case 2:
            assert(tsuzuri_net_close(0, handle) == 0);
            tsuzuri_net_unwatch(writing);
            handle = 0;
            break;
        default:
            break;
        }
        if (handle != 0) assert(tsuzuri_net_close(0, handle) == 0);
    }
    return NULL;
}

// A client that connects to the loopback port and resets the connection at once (an RST: close with SO_LINGER and a zero
// timeout), before anything accepts it.
static void connect_and_reset(int port) {
    int raw = socket(AF_INET, SOCK_STREAM, 0);
    assert(raw >= 0);
    struct sockaddr_in address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons((uint16_t)port);
    address.sin_addr.s_addr = htonl(0x7f000001u);
    assert(connect(raw, (struct sockaddr *)&address, sizeof address) == 0);
    struct linger abort_close = {1, 0};
    assert(setsockopt(raw, SOL_SOCKET, SO_LINGER, &abort_close, sizeof abort_close) == 0);
    assert(close(raw) == 0);
}

// A connection that its peer reset before accept took it is not an error of accept, which waits for, and returns, the next
// connection: Windows reports such a connection as WSAECONNRESET from accept (net.c skips it there), macOS drops it or
// refuses an option on it, and Linux hands it over and fails its first read. So what comes back is the connection that was
// reset (and its read fails) or the good one, never an error, and the good one follows. The Windows runtime is checked by its
// CI only; this pins the contract on the systems that run here, and the E2E "accept_reset" of tests/net.mjs runs it on Windows.
static void test_accept_reset(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port);
    connect_and_reset(port);
    int64_t good = connect_to(port);
    send_byte(good, 'g');
    int found = 0;
    for (int attempt = 0; attempt < 2 && !found; attempt++) {
        struct tz_net_buffer names;
        int64_t accepted = tsuzuri_net_accept(&names, listener, 5000);
        assert(accepted > 0 && "accept returns a connection and never reports the reset of another");
        tsuzuri_free(names.data);
        struct tz_net_buffer bytes;
        int64_t status = tsuzuri_net_read(&bytes, 0, accepted, 1, 5000);
        if (status == 0) {
            found = bytes.length == 1 && bytes.data[0] == 'g';
            tsuzuri_free(bytes.data);
        }
        assert(tsuzuri_net_close(0, accepted) == 0);
    }
    assert(found && "the connection that was not reset is accepted next");
    assert(tsuzuri_net_close(0, good) == 0);
    assert(tsuzuri_net_close(0, listener) == 0);
    expect_descriptors(baseline);
}

// A datagram that goes to a port that nobody listens on must not make the next receive of the same socket fail: Windows
// reports the ICMP "port unreachable" as WSAECONNRESET from the next receive unless SIO_UDP_CONNRESET is turned off (net.c does
// that, and skips the error if it is still reported), and no other system reports it for a socket that is not connected. As for
// test_accept_reset, this pins the contract here, and the E2E "udp_gone" of tests/net.mjs runs it on Windows.
static void test_datagram_after_closed_port(void) {
    int baseline = open_descriptors();
    struct tz_net_buffer names;
    int64_t server = tsuzuri_net_open(&names, 2, 0, 0, LOOPBACK, -1);
    assert(server > 0 && names.length == 40);
    int server_port = port_of(names.data);
    tsuzuri_free(names.data);
    int64_t spare = tsuzuri_net_open(&names, 2, 0, 0, LOOPBACK, -1);
    assert(spare > 0 && names.length == 40);
    int gone_port = port_of(names.data);
    tsuzuri_free(names.data);
    assert(tsuzuri_net_close(0, spare) == 0);
    const unsigned char hello[] = {'x'};
    assert(tsuzuri_net_write(1, server, hello, 1, gone_port, 0, LOOPBACK, -1) == 0);
    // Nothing has arrived, so the receive that follows times out; it reports nothing about the datagram that was sent.
    struct tz_net_buffer bytes;
    int64_t status = tsuzuri_net_read(&bytes, 1, server, 64, 100);
    assert(status == TIMED_OUT && "the receive after a send to a closed port waits, and times out");
    // And the socket still takes the datagram that does come, from another socket.
    int64_t sender = tsuzuri_net_open(&names, 2, 0, 0, LOOPBACK, -1);
    assert(sender > 0);
    tsuzuri_free(names.data);
    assert(tsuzuri_net_write(1, sender, hello, 1, server_port, 0, LOOPBACK, -1) == 0);
    status = tsuzuri_net_read(&bytes, 1, server, 64, 5000);
    assert(status == 0 && bytes.length == RECORD_BYTES + 1 && bytes.data[RECORD_BYTES] == 'x');
    tsuzuri_free(bytes.data);
    assert(tsuzuri_net_close(0, sender) == 0);
    assert(tsuzuri_net_close(0, server) == 0);
    expect_descriptors(baseline);
}

// Producers and consumers share one poller, and close, cancel, and watch each other's sockets. The results are not what is
// tested, but a crash, a hang, a post of one operation twice, and a leaked socket are.
static void test_stress(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port);
    enum { PRODUCERS = 2, CONSUMERS = 2 };
    pthread_t producers[PRODUCERS], consumers[CONSUMERS];
    struct stress work[PRODUCERS + CONSUMERS];
    for (int index = 0; index < PRODUCERS + CONSUMERS; index++) {
        work[index].port = port;
        work[index].listener = listener;
        work[index].rounds = 200;
        work[index].index = index;
    }
    atomic_store(&stress_done, 0);
    for (int index = 0; index < CONSUMERS; index++) assert(pthread_create(&consumers[index], NULL, consume, &work[PRODUCERS + index]) == 0);
    for (int index = 0; index < PRODUCERS; index++) assert(pthread_create(&producers[index], NULL, produce, &work[index]) == 0);
    for (int index = 0; index < PRODUCERS; index++) assert(pthread_join(producers[index], NULL) == 0);
    atomic_store(&stress_done, 1);
    for (int index = 0; index < CONSUMERS; index++) assert(pthread_join(consumers[index], NULL) == 0);
    for (int index = 0; index < SLOTS; index++) {
        int64_t left = atomic_exchange(&slots[index], 0);
        if (left != 0) assert(tsuzuri_net_close(0, left) == 0);
    }
    assert(tsuzuri_net_close(0, listener) == 0);
    expect_descriptors(baseline);
}

// The system's own reading of an address text is not the strict one of Net.parse_ip, and each system has its forms. The
// runtime refuses (InvalidInput, code 0) every host that the system reads as a numeric address, a valid one too, whatever
// the std code checked before the call: Net.resolve gives it only what its strict parser cannot read. A name is still
// looked up. getaddrinfo with AI_NUMERICHOST is the oracle, and it never asks a resolver, so nothing leaves the machine;
// a host that the system takes for a name is not given to the runtime here, which would ask a resolver.
static int numeric_for_system(const char *host) {
    struct addrinfo hints;
    memset(&hints, 0, sizeof hints);
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST;
    struct addrinfo *list = NULL;
    if (getaddrinfo(host, NULL, &hints, &list) != 0) return 0;
    freeaddrinfo(list);
    return 1;
}

static int64_t resolve_text(const char *host, struct tz_net_buffer *found) {
    return tsuzuri_net_resolve(found, (const unsigned char *)host, (int64_t)strlen(host), 80);
}

static void test_resolve(void) {
    static const char *forms[] = {
        "127.0.0.1", "0.0.0.0", "255.255.255.255", "::1", "::", "2001:db8::1", "::ffff:192.0.2.1", "1:2:3:4:5:6:7:8", "fe80::1",
        "127.1", "0x7f.1", "0x7f000001", "2130706433", "1.2.3", "0177.0.0.1", "010.0.0.1", "0", "0x0", "4294967296", "fe80::1%lo0",
        "00", "0x", "0X7F.0.0.1", "08.0.0.1", "0x.1", "1.256", "1.16777215", "99999999999999999999", "1.2.3.0x4", "1.2.0x304",
        "0x1.0x2.0x3.0x4", "::1%", "fe80::1%1", "ff02::1%en0", "1.2.3.4 ", "1.2.3.4 x",
    };
    int refused = 0;
    struct tz_net_buffer found = {NULL, 0};
    for (size_t index = 0; index < sizeof forms / sizeof forms[0]; index++) {
        if (!numeric_for_system(forms[index])) continue;
        assert(resolve_text(forms[index], &found) == (INVALID_INPUT << 32));
        assert(found.data == NULL && found.length == 0);
        refused++;
    }
    assert(refused >= 9 && "every system reads the valid literals as addresses");
    int lenient = numeric_for_system("0x7f.1");
    int sweep = 0;
    // A seeded sweep of dotted numbers in the bases that inet_aton knows, and of IPv6 forms with zones.
    static const char *numbers[] = {"0", "1", "7", "127", "255", "256", "0x7f", "0X7F", "0x", "00", "010", "0177", "08", "65535", "65536", "16777215", "16777216", "2130706433", "4294967296", "99999999999999999999"};
    static const char *groups[] = {"0", "1", "fe80", "2001", "db8", "ffff", "abcd", "ABCD", "1.2.3.4", "g", "", "12345"};
    static const char *zones[] = {"", "", "%lo0", "%1", "%", "%eth0"};
    uint32_t state = 0x1f2e3d4cu;
#define NEXT(bound) (state = state * 1664525u + 1013904223u, (size_t)((state >> 8) % (bound)))
    for (int round = 0; round < 3000; round++) {
        char host[160];
        size_t length = 0;
        if (round % 2 == 0) {
            size_t parts = 1 + NEXT(4);
            for (size_t part = 0; part < parts; part++) length += (size_t)snprintf(host + length, sizeof host - length, "%s%s", part ? "." : "", numbers[NEXT(sizeof numbers / sizeof numbers[0])]);
        } else {
            size_t parts = 2 + NEXT(7);
            if (NEXT(3) == 0) length += (size_t)snprintf(host + length, sizeof host - length, "::");
            for (size_t part = 0; part < parts; part++) length += (size_t)snprintf(host + length, sizeof host - length, "%s%s", part ? ":" : "", groups[NEXT(sizeof groups / sizeof groups[0])]);
            length += (size_t)snprintf(host + length, sizeof host - length, "%s", zones[NEXT(sizeof zones / sizeof zones[0])]);
        }
        if (length == 0 || length > 253 || !numeric_for_system(host)) continue;
        assert(resolve_text(host, &found) == (INVALID_INPUT << 32));
        assert(found.data == NULL && found.length == 0);
        sweep++;
    }
#undef NEXT
    // A system that reads only dotted decimal has no more forms to refuse; every system of the CI has them.
    if (lenient) assert(sweep >= 100 && "the sweep reaches addresses that the system reads");
    else printf("net runtime: skip: this system reads no hexadecimal or short IPv4 forms, so the sweep has little to refuse (%d)\n", sweep);
    // A name goes through to the system's lookup: found (whole 20-byte records) or not found, never refused as an address.
    int64_t local = resolve_text("localhost", &found);
    assert(local == 0 || (local >> 32) != INVALID_INPUT);
    if (local == 0) {
        assert(found.data != NULL && found.length > 0 && found.length % 20 == 0);
        tsuzuri_free(found.data);
    }
}

int main(void) {
    // The first socket may make the system or a sanitizer open something of its own, which is not a leak.
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    int64_t warm = operation();
    tsuzuri_net_watch(post, warm, server, 2, -1);
    assert(completed(warm) == 0);
    int64_t all[] = {client, server, listener};
    close_all(all, 3);
    // Wait until the descriptors have been steady for a while: the poller of the warm-up is gone then.
    int start = open_descriptors();
    for (int steady = 0; steady < 10;) {
        struct timespec pause = {0, 10 * 1000000L};
        nanosleep(&pause, NULL);
        int now = open_descriptors();
        steady = now == start ? steady + 1 : 0;
        start = now;
    }
    test_ready();
    test_timeout();
    test_close_and_stale();
    test_unwatch();
    test_many();
    test_connect();
    test_connect_cancel();
    test_accept_reset();
    test_datagram_after_closed_port();
    test_stress();
    expect_descriptors(start);
    // The system's resolver may keep a descriptor of its own (a connection to a resolver service), which is not a leak.
    test_resolve();
    assert(atomic_load(&live) == 0 && "every buffer was freed");
    printf("net runtime: waits, timeouts, close, unwatch, connect, cancel, and threads that churn sockets passed (%lld completions of the churn); a connection reset before accept and a datagram sent to a closed port report nothing; a host that the system reads as an address is refused\n", (long long)atomic_load(&stress_completions));
    return 0;
}
