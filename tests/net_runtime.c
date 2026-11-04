// Tests of the socket runtime of the standard Net module (E09) without the compiler: the calls that try once (a timeout of
// 0), timeouts, stale handles, a connection reset before accept, a datagram to a closed port, threads that churn sockets
// through the handle table, the descriptors that must not leak, and the refusal of address text. It links src/runtime/net.c
// alone, so every wait is a poll of the calling thread (a tzroutine's waits go to the netpoller: tests/net.mjs, block
// "tzroutine"). Everything stays on 127.0.0.1 with ephemeral ports. No result depends on how long something took: a bound on
// waiting only says that the program is hung. Build it with the address, thread, or undefined-behaviour sanitizer as well.
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
extern int64_t tsuzuri_net_open(struct tz_net_buffer *, int32_t, int64_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_accept(struct tz_net_buffer *, int64_t, int64_t);
extern int64_t tsuzuri_net_read(struct tz_net_buffer *, int32_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_write(int32_t, int64_t, const unsigned char *, int64_t, int64_t, int64_t, int64_t, int64_t);
extern int64_t tsuzuri_net_close(int32_t, int64_t);
extern int64_t tsuzuri_net_resolve(struct tz_net_buffer *, const unsigned char *, int64_t, int64_t);

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

// The descriptors that are open must come back to `expected`. A sanitizer may open one of its own a moment late, so a count
// that differs is asked again for a while before it fails.
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

// A call with a timeout of 0 tries once: an accept or a read that would have to wait is PENDING and allocates nothing, and
// one that finds something takes it.
static void test_try_once(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    struct tz_net_buffer names;
    assert(tsuzuri_net_accept(&names, listener, 0) == -PENDING && names.data == NULL && "no connection waits");
    pair_on(listener, port, &client, &server);
    struct tz_net_buffer bytes;
    assert(read_now(server, &bytes) == PENDING && bytes.data == NULL && "an empty socket would wait");
    // A write of two bytes arrives as one segment: the read that waits takes the first, and the second is there for a read
    // that does not.
    const unsigned char both[] = {'x', 'y'};
    assert(tsuzuri_net_write(0, client, both, 2, 0, 0, 0, 5000) == 0);
    assert(tsuzuri_net_read(&bytes, 0, server, 1, 5000) == 0 && bytes.length == 1 && bytes.data[0] == 'x');
    tsuzuri_free(bytes.data);
    assert(read_now(server, &bytes) == 0 && bytes.length == 1 && bytes.data[0] == 'y');
    tsuzuri_free(bytes.data);
    assert(read_now(server, &bytes) == PENDING && bytes.data == NULL);
    // A closed peer is the end of the stream, for a read that waits and for one that does not.
    assert(tsuzuri_net_close(0, client) == 0);
    assert(tsuzuri_net_read(&bytes, 0, server, 64, 5000) == 0 && bytes.length == 0 && bytes.data == NULL);
    assert(read_now(server, &bytes) == 0 && bytes.length == 0 && bytes.data == NULL);
    // A datagram socket with nothing in it would wait too.
    int64_t datagram = tsuzuri_net_open(&names, 2, 0, 0, LOOPBACK, -1);
    assert(datagram > 0);
    tsuzuri_free(names.data);
    assert(tsuzuri_net_read(&bytes, 1, datagram, 64, 0) == PENDING && bytes.data == NULL);
    int64_t all[] = {server, listener, datagram};
    close_all(all, 3);
    expect_descriptors(baseline);
}

// A timeout is for the whole call, and the socket stays good after it.
static void test_timeout(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    struct tz_net_buffer names;
    assert(tsuzuri_net_accept(&names, listener, 30) == -TIMED_OUT && names.data == NULL);
    pair_on(listener, port, &client, &server);
    struct tz_net_buffer bytes;
    assert(tsuzuri_net_read(&bytes, 0, server, 64, 30) == TIMED_OUT && bytes.data == NULL);
    send_byte(client, 1);
    assert(tsuzuri_net_read(&bytes, 0, server, 64, 5000) == 0 && bytes.length == 1 && bytes.data[0] == 1);
    tsuzuri_free(bytes.data);
    int64_t all[] = {client, server, listener};
    close_all(all, 3);
    expect_descriptors(baseline);
}

// A closed handle is dead, and a handle that was never open is refused the same way, at once.
static void test_stale(void) {
    int baseline = open_descriptors();
    int port;
    int64_t listener = listen_on_loopback(&port), client, server;
    pair_on(listener, port, &client, &server);
    assert(tsuzuri_net_close(0, server) == 0);
    struct tz_net_buffer bytes;
    int64_t status = read_now(server, &bytes);
    assert((status >> 32) == INVALID_INPUT && (status & 0xffffffff) == EBADF);
    assert(tsuzuri_net_read(&bytes, 0, server, 64, 5000) == status && "a stale handle does not wait");
    assert(read_now(0, &bytes) == status);
    assert(tsuzuri_net_close(0, server) == status);
    int64_t all[] = {client, listener};
    close_all(all, 2);
    expect_descriptors(baseline);
}

// Handles that producers made and consumers have not taken, and the churn of both.
#define SLOTS 16
static atomic_int_fast64_t slots[SLOTS];

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
static atomic_int_fast64_t stress_reads;

static void *consume(void *argument) {
    struct stress *work = argument;
    for (int round = 0; !atomic_load(&stress_done) || round < 50; round++) {
        int64_t handle = atomic_exchange(&slots[(round * 7 + work->index) % SLOTS], 0);
        if (handle == 0) {
            sched_yield();
            continue;
        }
        // A read that tries once finds the byte, the end of the stream, or nothing yet; it fails only when the handle is dead.
        struct tz_net_buffer bytes;
        int64_t status = read_now(handle, &bytes);
        assert(status == 0 || status == PENDING || (status >> 32) == OTHER);
        if (status == 0) tsuzuri_free(bytes.data);
        assert(tsuzuri_net_close(0, handle) == 0);
        atomic_fetch_add(&stress_reads, 1);
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
// CI only; this pins the contract on the systems that run here, and the case "accept_reset" of the block "portable" of
// tests/net.mjs runs it on Windows.
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
// test_accept_reset, this pins the contract here, and the line "vanished" of the case "udp" of the block "portable" of
// tests/net.mjs runs it on Windows.
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

// Producers and consumers share the handle table: producers open connections and put them in slots, and consumers take them,
// read once, and close them. The results are not what is tested, but a crash, a hang, a handle used after its close, and a
// leaked socket are.
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
    int64_t all[] = {client, server, listener};
    close_all(all, 3);
    // Wait until the descriptors have been steady for a while.
    int start = open_descriptors();
    for (int steady = 0; steady < 10;) {
        struct timespec pause = {0, 10 * 1000000L};
        nanosleep(&pause, NULL);
        int now = open_descriptors();
        steady = now == start ? steady + 1 : 0;
        start = now;
    }
    test_try_once();
    test_timeout();
    test_stale();
    test_accept_reset();
    test_datagram_after_closed_port();
    test_stress();
    expect_descriptors(start);
    // The system's resolver may keep a descriptor of its own (a connection to a resolver service), which is not a leak.
    test_resolve();
    assert(atomic_load(&live) == 0 && "every buffer was freed");
    printf("net runtime: tries once, timeouts, stale handles, and threads that churn sockets passed (%lld reads of the churn); a connection reset before accept and a datagram sent to a closed port report nothing; a host that the system reads as an address is refused\n", (long long)atomic_load(&stress_reads));
    return 0;
}
