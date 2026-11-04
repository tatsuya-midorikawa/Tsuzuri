/* Tests of the tzroutine runtime (TZ01), run by tests/tzroutine_runtime.mjs. By hand:
 * clang -std=c11 -Wall -Wextra -Werror -O2 -pthread tests/tzroutine_runtime.c -o t && ./t [case]
 * (on Windows: --target=x86_64-pc-windows-msvc -fuse-ld=lld instead of -pthread). */
#if defined(_WIN32)
/* The Windows netpoller is in the unit when net.c is (TZ08 Phase 3): its sockets come first. */
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
#define TZ_NET_WINSOCK 1
#endif
/* TZ10: a test pauses sysmon's kick between its read of the gate and its signal. */
static void urgent_kick_pause(void) __attribute__((unused));
#define TZR_KICK_PAUSE() urgent_kick_pause()
#include "../src/runtime/tzroutine.c"

#include <assert.h>
#include <fenv.h>
#if !defined(_WIN32)
#include <signal.h>
#include <sys/socket.h>
#include <sys/wait.h>
#endif

#define CHECK(cond)                                                              \
    do {                                                                         \
        if (!(cond)) {                                                           \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #cond); \
            exit(1);                                                             \
        }                                                                        \
    } while (0)

static double seconds(void) { return (double)tzr_nanotime() / 1e9; }

static tzr_chan *chan_make(int64_t elem, int64_t cap) {
    tzr_chan *c = calloc(1, sizeof(tzr_chan) + (size_t)(elem * cap));
    tsuzuri_tz_chan_init(c, elem, cap);
    return c;
}

static void join(void *scope) { CHECK(tsuzuri_tz_scope_leave(scope) == 0); }

/* 1. spawn many */
static _Atomic int64_t counter;
static void incr(void *env) {
    atomic_fetch_add(&counter, (int64_t)(intptr_t)env);
}
static void test_spawn(void) {
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    for (int i = 0; i < 100000; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    join(scope);
    CHECK(atomic_load(&counter) == 100000);
    printf("spawn: 100000 tzroutines in %.3f s\n", seconds() - t);
}

/* 2. nested spawn: a tree of 2^15 routines */
static void tree(void *env) {
    intptr_t depth = (intptr_t)env;
    atomic_fetch_add(&counter, 1);
    if (depth > 0) {
        tsuzuri_tz_go(tree, (void *)(depth - 1));
        tsuzuri_tz_go(tree, (void *)(depth - 1));
    }
}
static void test_tree(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(tree, (void *)(intptr_t)14);
    join(scope);
    CHECK(atomic_load(&counter) == (1 << 15) - 1);
    printf("tree: %lld routines\n", (long long)atomic_load(&counter));
}

/* 3. unbuffered ping-pong */
typedef struct { tzr_chan *ping, *pong; int64_t rounds; } pingpong;
static void ponger(void *env) {
    pingpong *pp = env;
    for (int64_t i = 0; i < pp->rounds; i++) {
        int64_t v;
        CHECK(tsuzuri_tz_chan_recv(pp->ping, &v, 1) == TZR_OK);
        v++;
        CHECK(tsuzuri_tz_chan_send(pp->pong, &v, 1) == TZR_OK);
    }
}
static void pinger(void *env) {
    pingpong *pp = env;
    for (int64_t i = 0; i < pp->rounds; i++) {
        int64_t v = i, w;
        CHECK(tsuzuri_tz_chan_send(pp->ping, &v, 1) == TZR_OK);
        CHECK(tsuzuri_tz_chan_recv(pp->pong, &w, 1) == TZR_OK);
        CHECK(w == i + 1);
    }
}
static void test_pingpong(int64_t rounds, int main_thread) {
    pingpong pp = {chan_make(8, 0), chan_make(8, 0), rounds};
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    tsuzuri_tz_go(ponger, &pp);
    if (main_thread) pinger(&pp);
    else tsuzuri_tz_go(pinger, &pp);
    join(scope);
    double e = seconds() - t;
    printf("pingpong(%s): %lld round trips, %.0f ns each\n", main_thread ? "main" : "routines", (long long)rounds, e * 1e9 / (double)rounds);
    free(pp.ping);
    free(pp.pong);
}

/* 4. MPMC buffered */
typedef struct { tzr_chan *c; int64_t count; _Atomic int64_t *sum; tzr_chan *done; } mpmc;
static void producer(void *env) {
    mpmc *m = env;
    for (int64_t i = 1; i <= m->count; i++) CHECK(tsuzuri_tz_chan_send(m->c, &i, 1) == TZR_OK);
    int64_t one = 1;
    CHECK(tsuzuri_tz_chan_send(m->done, &one, 1) == TZR_OK);
}
static void consumer(void *env) {
    mpmc *m = env;
    int64_t v, local = 0;
    while (tsuzuri_tz_chan_recv(m->c, &v, 1) == TZR_OK) local += v;
    atomic_fetch_add(m->sum, local);
}
static void closer(void *env) {
    mpmc *m = env;
    int64_t v;
    for (int i = 0; i < 4; i++) CHECK(tsuzuri_tz_chan_recv(m->done, &v, 1) == TZR_OK);
    CHECK(tsuzuri_tz_chan_close(m->c) == TZR_OK);
    CHECK(tsuzuri_tz_chan_close(m->c) == TZR_CLOSED);
}
static void test_mpmc(int64_t cap) {
    _Atomic int64_t sum = 0;
    mpmc m = {chan_make(8, cap), 50000, &sum, chan_make(8, 4)};
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(producer, &m);
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(consumer, &m);
    tsuzuri_tz_go(closer, &m);
    join(scope);
    CHECK(atomic_load(&sum) == 4 * (50000LL * 50001LL / 2));
    int64_t v;
    CHECK(tsuzuri_tz_chan_recv(m.c, &v, 1) == TZR_CLOSED);
    CHECK(tsuzuri_tz_chan_send(m.c, &v, 1) == TZR_CLOSED);
    printf("mpmc(cap %lld): ok\n", (long long)cap);
    free(m.c);
    free(m.done);
}

/* 5. select fairness and default */
static void feeder(void *env) {
    tzr_chan *c = env;
    int64_t v = 1;
    for (;;) {
        if (tsuzuri_tz_chan_send(c, &v, 1) != TZR_OK) return;
    }
}
static void test_select(void) {
    tzr_chan *a = chan_make(8, 0), *b = chan_make(8, 0), *quit = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(feeder, a);
    tsuzuri_tz_go(feeder, b);
    int64_t hits[2] = {0, 0};
    for (int i = 0; i < 20000; i++) {
        /* both cases ready at each choice: a feeder waits in each send queue */
        for (;;) {
            tzr_lock_acquire(&a->lock);
            int ready_a = a->sendq.first != NULL;
            tzr_lock_release(&a->lock);
            tzr_lock_acquire(&b->lock);
            int ready_b = b->sendq.first != NULL;
            tzr_lock_release(&b->lock);
            if (ready_a && ready_b) break;
            tsuzuri_tz_yield();
        }
        int64_t va = 0, vb = 0;
        tzr_scase cases[2] = {{.c = a, .elem = &va}, {.c = b, .elem = &vb}};
        int32_t ok;
        int64_t k = tsuzuri_tz_select(cases, 2, 1, &ok);
        CHECK(k == 0 || k == 1);
        CHECK(ok == 1);
        hits[k]++;
    }
    /* both feeders are always ready eventually; the choice must not starve either */
    CHECK(hits[0] > 4000 && hits[1] > 4000);
    /* default: nothing is ready on quit */
    int64_t q;
    tzr_scase none[1] = {{.c = quit, .elem = &q}};
    int32_t ok;
    CHECK(tsuzuri_tz_select(none, 1, 0, &ok) == -1);
    tsuzuri_tz_chan_close(a);
    tsuzuri_tz_chan_close(b);
    /* drain the feeders: they get CLOSED from their blocked send and return */
    join(scope);
    /* a closed channel is ready for receive with ok = 0 */
    int64_t v;
    tzr_scase closed[2] = {{.c = quit, .elem = &q}, {.c = a, .elem = &v}};
    CHECK(tsuzuri_tz_select(closed, 2, 1, &ok) == 1 && ok == 0);
    printf("select: a=%lld b=%lld\n", (long long)hits[0], (long long)hits[1]);
    free(a);
    free(b);
    free(quit);
}

/* 6. select send + recv waiting together, many rounds across routines */
typedef struct { tzr_chan *in, *out, *stop; int64_t n; } relay;
static void relayer(void *env) {
    relay *r = env;
    int64_t got = 0;
    for (;;) {
        int64_t v = 0, s = 0;
        tzr_scase cases[2] = {{.c = r->in, .elem = &v}, {.c = r->stop, .elem = &s}};
        int32_t ok;
        int64_t k = tsuzuri_tz_select(cases, 2, 1, &ok);
        if (k == 1) break;
        CHECK(ok);
        tzr_scase out[1] = {{.c = r->out, .elem = &v, .send = 1}};
        CHECK(tsuzuri_tz_select(out, 1, 1, &ok) == 0 && ok);
        got++;
    }
    CHECK(got == r->n);
}
static void test_relay(void) {
    relay r = {chan_make(8, 0), chan_make(8, 0), chan_make(8, 0), 100000};
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(relayer, &r);
    for (int64_t i = 0; i < r.n; i++) {
        int64_t v = i, w;
        CHECK(tsuzuri_tz_chan_send(r.in, &v, 1) == TZR_OK);
        CHECK(tsuzuri_tz_chan_recv(r.out, &w, 1) == TZR_OK && w == i);
    }
    CHECK(tsuzuri_tz_chan_close(r.stop) == TZR_OK);
    join(scope);
    printf("relay: ok\n");
    free(r.in);
    free(r.out);
    free(r.stop);
}

/* 7. mutex */
static _Atomic uint64_t mutex_word;
static int64_t plain;
static void locker(void *env) {
    (void)env;
    for (int i = 0; i < 20000; i++) {
        CHECK(tsuzuri_tz_mutex_lock(&mutex_word) == 0);
        plain++;
        if (i % 1000 == 0) tsuzuri_tz_yield();
        tsuzuri_tz_mutex_unlock(&mutex_word);
    }
}
static void test_mutex(void) {
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 16; i++) tsuzuri_tz_go(locker, NULL);
    join(scope);
    CHECK(plain == 16 * 20000);
    printf("mutex: ok\n");
}

/* 8. waitgroup */
static _Atomic uint64_t wg;
static void wg_worker(void *env) {
    atomic_fetch_add(&counter, (int64_t)(intptr_t)env);
    CHECK(tsuzuri_tz_waitgroup_add(&wg, -1) == 0);
}
static void wg_waiter(void *env) {
    (void)env;
    CHECK(tsuzuri_tz_waitgroup_wait(&wg) == 0);
    CHECK(atomic_load(&counter) == 1000);
}
static void test_waitgroup(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    CHECK(tsuzuri_tz_waitgroup_add(&wg, 1000) == 0);
    for (int i = 0; i < 3; i++) tsuzuri_tz_go(wg_waiter, NULL);
    for (int i = 0; i < 1000; i++) tsuzuri_tz_go(wg_worker, (void *)(intptr_t)1);
    CHECK(tsuzuri_tz_waitgroup_wait(&wg) == 0);
    CHECK(atomic_load(&counter) == 1000);
    join(scope);
    CHECK(tsuzuri_tz_waitgroup_add(&wg, -1) == 1); /* negative counter */
    atomic_store(&wg, 0);
    printf("waitgroup: ok\n");
}

/* 9. sleep */
static void sleeper(void *env) {
    CHECK(tsuzuri_tz_sleep((int64_t)(intptr_t)env) == 0);
    atomic_fetch_add(&counter, 1);
}
static void test_sleep(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    for (int i = 0; i < 2000; i++) tsuzuri_tz_go(sleeper, (void *)(intptr_t)20000000);
    join(scope);
    double e = seconds() - t;
    CHECK(atomic_load(&counter) == 2000);
    CHECK(e >= 0.020);
    printf("sleep: 2000 x 20 ms in %.3f s\n", e);
}

/* 10. external threads (pseudo-Gs) each with their own scope */
static void *external(void *arg) {
    (void)arg;
    test_pingpong(20000, 1);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 1000; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    join(scope);
    return NULL;
}
#if defined(_WIN32)
static DWORD WINAPI external_thread(LPVOID arg) {
    external(arg);
    return 0;
}
/* A host thread that registers and then runs fibers keeps its pseudo-G, which is the thread's: the
 * fiber where it registered can go away, the thread calls in from another one, and the pseudo-G goes
 * when the thread ends (the TLS callback). */
static void *fiber_registered;
static VOID CALLBACK fiber_other(LPVOID arg) {
    (void)arg;
    DeleteFiber(fiber_registered);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 100; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    join(scope);
    ExitThread(0);
}
static DWORD WINAPI fiber_thread(LPVOID arg) {
    (void)arg;
    (void)tsuzuri_tz_count(); /* registers this thread */
    (void)tzr_current();
    fiber_registered = ConvertThreadToFiber(NULL);
    CHECK(fiber_registered != NULL);
    void *other = CreateFiber(0, fiber_other, NULL);
    CHECK(other != NULL);
    SwitchToFiber(other);
    return 1; /* not reached: the other fiber ends the thread */
}
static int pseudo_running(void) {
    tzr_mutex_lock(&tzr_sched.lock);
    int n = tzr_sched.npseudo_running;
    tzr_mutex_unlock(&tzr_sched.lock);
    return n;
}
#endif
static void test_external(void) {
    atomic_store(&counter, 0);
#if defined(_WIN32)
    HANDLE t[4];
    for (int i = 0; i < 4; i++) CHECK((t[i] = CreateThread(NULL, 0, external_thread, NULL, 0, NULL)) != NULL);
    for (int i = 0; i < 4; i++) CHECK(WaitForSingleObject(t[i], INFINITE) == WAIT_OBJECT_0 && CloseHandle(t[i]));
    int before = pseudo_running();
    HANDLE f = CreateThread(NULL, 0, fiber_thread, NULL, 0, NULL);
    CHECK(f != NULL && WaitForSingleObject(f, INFINITE) == WAIT_OBJECT_0 && CloseHandle(f));
    CHECK(pseudo_running() == before); /* the ended thread's pseudo-G is gone */
    atomic_fetch_sub(&counter, 100);
#else
    pthread_t t[4];
    for (int i = 0; i < 4; i++) pthread_create(&t[i], NULL, external, NULL);
    for (int i = 0; i < 4; i++) pthread_join(t[i], NULL);
#endif
    CHECK(atomic_load(&counter) == 4000);
    printf("external threads: ok\n");
}

/* 11. deep recursion on a routine stack. The frame size depends on the compiler and the sanitizer,
 * so it is measured and the depth is chosen from a number of bytes. */
static uintptr_t deep_pad[2];
static int64_t deep(int64_t n) {
    volatile char pad[512];
    pad[n % 512] = (char)n; /* an index the compiler cannot know keeps the whole array */
    if (n < 2) deep_pad[n] = (uintptr_t)&pad[0];
    if (n == 0) return 0;
    return deep(n - 1) + 1 + (pad[n % 512] != (char)n);
}
static int64_t depth_for(int64_t bytes) {
    deep(1);
    uintptr_t frame = deep_pad[1] > deep_pad[0] ? deep_pad[1] - deep_pad[0] : deep_pad[0] - deep_pad[1];
    return bytes / (int64_t)(frame ? frame : 512);
}
static void deep_routine(void *env) { CHECK(deep((int64_t)(intptr_t)env) == (int64_t)(intptr_t)env); }
static void test_deep(void) {
    void *scope = tsuzuri_tz_scope_enter();
    int64_t depth = depth_for(900 * 1024); /* 900 KiB of the default 1 MiB stack */
    tsuzuri_tz_go(deep_routine, (void *)(intptr_t)depth);
    join(scope);
    printf("deep recursion: %lld frames: ok\n", (long long)depth);
}

/* 12. deadlock: main waits on a channel nobody sends on (run in a child) */
static void blocked(void *env) {
    int64_t v;
    tsuzuri_tz_chan_recv(env, &v, 1);
}
static const char *self_path;
#if defined(_WIN32)
/* On Windows a stack overflow ends a child with STATUS_STACK_OVERFLOW (the reason is not reported:
 * stack.c is Unix only); any other crash is not an overflow. */
static int overflowed(int status) { return (DWORD)status == (DWORD)STATUS_STACK_OVERFLOW; }
static int exited_with(int status, int code) { return status == code; }
/* A fresh process with TSUZURI_TZROUTINE_STACK_KIB set to stack_kib (NULL: inherited). */
static int run_child_with(const char *name, const char *stack_kib) {
    fflush(stdout);
    static const char variable[] = "TSUZURI_TZROUTINE_STACK_KIB";
    char self[MAX_PATH + 1], command[2 * MAX_PATH + 64], previous[64];
    DWORD length = GetModuleFileNameA(NULL, self, sizeof self);
    CHECK(length > 0 && length < sizeof self);
    snprintf(command, sizeof command, "\"%s\" %s", self, name);
    DWORD had = GetEnvironmentVariableA(variable, previous, sizeof previous);
    CHECK(had < sizeof previous);
    if (stack_kib) CHECK(SetEnvironmentVariableA(variable, stack_kib));
    STARTUPINFOA startup = {.cb = sizeof startup};
    PROCESS_INFORMATION process;
    CHECK(CreateProcessA(self, command, NULL, NULL, FALSE, 0, NULL, NULL, &startup, &process));
    if (stack_kib) CHECK(SetEnvironmentVariableA(variable, had ? previous : NULL));
    CHECK(WaitForSingleObject(process.hProcess, INFINITE) == WAIT_OBJECT_0);
    DWORD code;
    CHECK(GetExitCodeProcess(process.hProcess, &code));
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return (int)code;
}
static int run_child(const char *name) { return run_child_with(name, NULL); }
#else
/* A stack overflow ends a child by SIGSEGV or SIGBUS, or by SIGABRT after the report of stack.c. */
static int overflowed(int status) {
    return WIFSIGNALED(status) && (WTERMSIG(status) == SIGSEGV || WTERMSIG(status) == SIGBUS || WTERMSIG(status) == SIGABRT);
}
static int exited_with(int status, int code) { return WIFEXITED(status) && WEXITSTATUS(status) == code; }
/* A fresh process (fork would copy a process whose scheduler threads do not exist in the child). */
static int run_child(const char *name) {
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        execl(self_path, self_path, name, (char *)NULL);
        _exit(127);
    }
    int status;
    waitpid(pid, &status, 0);
    return status;
}
#endif
static void deadlock_body(void) {
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(blocked, c);
    tsuzuri_tz_go(blocked, c);
    int64_t v;
    int32_t r = tsuzuri_tz_chan_recv(c, &v, 1);
    _exit(r == TZR_DEADLOCK ? 42 : 1);
    (void)scope;
}
static void deadlock_scope_body(void) {
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(blocked, c);
    _exit(tsuzuri_tz_scope_leave(scope) == 1 ? 43 : 1);
}
static void overflow_routine(void *env) { (void)env; deep(1 << 20); }
static void overflow_body(void) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(overflow_routine, NULL);
    join(scope);
}
/* 14. TSUZURI_TZROUTINE_STACK_KIB sizes the stacks (a child per setting) */
static void stack_body(int64_t kib) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(deep_routine, (void *)(intptr_t)depth_for(kib * 1024));
    join(scope);
}
#if !defined(_WIN32)
static int run_child_with(const char *name, const char *stack_kib) {
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        if (stack_kib) setenv("TSUZURI_TZROUTINE_STACK_KIB", stack_kib, 1);
        execl(self_path, self_path, name, (char *)NULL);
        _exit(127);
    }
    int status;
    waitpid(pid, &status, 0);
    return status;
}
#endif
static void test_stack_size(void) {
    int s = run_child_with("child-deep-100", "64"); /* 100 KiB of frames on a 64 KiB stack */
    CHECK(overflowed(s));
    s = run_child_with("child-deep-3500", "4096"); /* 3.5 MiB on a 4 MiB stack */
    CHECK(exited_with(s, 0));
    s = run_child_with("child-deep-3500", NULL); /* the default 1 MiB is too small for it */
    CHECK(overflowed(s));
    const char *invalid[] = {"0", "abc", "2000000", "64k", ""};
    for (int i = 0; i < 5; i++) {
        s = run_child_with("child-deep-900", invalid[i]); /* each is ignored: the default 1 MiB */
        CHECK(exited_with(s, 0));
    }
    printf("stack sizes: ok\n");
}

static void test_failures(void) {
    int s = run_child("child-deadlock");
    CHECK(exited_with(s, 42));
    s = run_child("child-deadlock-scope");
    CHECK(exited_with(s, 43));
    s = run_child("child-overflow");
    CHECK(overflowed(s));
    printf("deadlock and overflow: ok\n");
}

/* 13. callee-saved floating-point registers survive switches */
static double fp_work(int yield) {
    double a = 1.1, b = 2.2, c = 3.3, d = 4.4, e = 5.5, f = 6.6, g = 7.7, h = 8.8;
    for (int i = 0; i < 2000; i++) {
        a = a * 1.0000001 + b; b = b * 0.9999999 + c; c = c + d * 1e-9; d = d - e * 1e-9;
        e = e * 1.0000002; f = f + g * 1e-7; g = g * 0.9999998; h = h + a * 1e-12;
        if (yield) tsuzuri_tz_yield();
    }
    return a + b + c + d + e + f + g + h;
}
static double fp_expected;
static void fp_routine(void *env) {
    (void)env;
    CHECK(fp_work(1) == fp_expected);
}
static void test_fp(void) {
    fp_expected = fp_work(0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 64; i++) tsuzuri_tz_go(fp_routine, NULL);
    join(scope);
    printf("fp registers: ok\n");
}

/* 16. the end of a boundary (TZ02): an empty scope returns at once, a full one after its tzroutines */
/* 23. the messages of the generated code's traps (TZ02, TZ03): a child writes them all */
static void report_body(void) {
    for (int32_t what = 0; what <= 9; what++) tsuzuri_tz_report(what); /* 0 and 9 write nothing */
    _exit(47);
}
static void test_report(void) {
    CHECK(exited_with(run_child("child-report"), 47));
    printf("report: ok\n");
}

static void test_boundary(void) {
    tsuzuri_tz_boundary_leave(tsuzuri_tz_scope_enter());
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 100; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&counter) == 100);
    CHECK(tsuzuri_tz_count() == 1);
    printf("boundary: ok\n");
}

/* 15. a sleep whose deadline saturates at INT64_MAX, next to short ones: the timer thread then
 * waits for at most a day at a time instead of computing a deadline that overflows (UBSan checks) */
static void forever_sleeper(void *env) {
    (void)env;
    tsuzuri_tz_sleep(INT64_MAX);
}
static void test_long_sleep(void) {
    tsuzuri_tz_go(forever_sleeper, NULL); /* in the root scope: nothing waits for it */
    CHECK(tsuzuri_tz_sleep(20000000) == 0);
    CHECK(tsuzuri_tz_sleep(1000000) == 0);
    CHECK(tsuzuri_tz_count() == 2);
    printf("long sleep: ok\n");
}

/* 17. each tzroutine keeps its own rounding mode across switches (MXCSR on x86-64, FPCR on
 * aarch64), and a new one starts with round-to-nearest */
static void fp_mode_routine(void *env) {
    int mode = (int)(intptr_t)env;
    CHECK(fegetround() == FE_TONEAREST);
    CHECK(fesetround(mode) == 0);
    for (int i = 0; i < 200; i++) {
        tsuzuri_tz_yield();
        CHECK(fegetround() == mode);
    }
    fesetround(FE_TONEAREST);
}
static void fp_leave_upward(void *env) {
    (void)env;
    CHECK(fesetround(FE_UPWARD) == 0); /* returns without restoring it */
}
static void fp_check_default(void *env) {
    (void)env;
    CHECK(fegetround() == FE_TONEAREST);
}
/* the exception flags are per routine too: x87's status word and MXCSR's flags on x86-64, FPSR on
 * aarch64 (fetestexcept reads them all) */
static void fp_flag_raiser(void *env) {
    (void)env;
    feclearexcept(FE_ALL_EXCEPT);
    feraiseexcept(FE_INVALID);
    for (int i = 0; i < 100; i++) {
        tsuzuri_tz_yield();
        CHECK(fetestexcept(FE_INVALID) != 0);
    }
    feclearexcept(FE_ALL_EXCEPT);
}
static void fp_flag_clean(void *env) {
    (void)env;
    CHECK(fetestexcept(FE_ALL_EXCEPT) == 0);
    for (int i = 0; i < 100; i++) {
        tsuzuri_tz_yield();
        CHECK(fetestexcept(FE_ALL_EXCEPT) == 0);
    }
}
#if defined(__x86_64__)
/* x87 alone: 0/0 sets the invalid flag (masked) of the x87 status word */
static unsigned x87_status(void) {
    unsigned short status;
    __asm__ volatile("fnstsw %0" : "=m"(status));
    return status & 0x3f;
}
static void x87_raiser(void *env) {
    (void)env;
    __asm__ volatile("fnclex\n fldz\n fldz\n fdivrp\n fstp %%st(0)" ::: "memory");
    for (int i = 0; i < 100; i++) {
        tsuzuri_tz_yield();
        CHECK(x87_status() & 1);
    }
    __asm__ volatile("fnclex");
}
static void x87_clean(void *env) {
    (void)env;
    CHECK(x87_status() == 0);
    for (int i = 0; i < 100; i++) {
        tsuzuri_tz_yield();
        CHECK(x87_status() == 0);
    }
}
#endif
static void test_fp_mode(void) {
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 64; i++) tsuzuri_tz_go(fp_mode_routine, (void *)(intptr_t)(i % 2 ? FE_UPWARD : FE_TONEAREST));
    join(scope);
    /* a reused G (or Windows fiber) does not keep the mode its last routine left */
    for (int round = 0; round < 4; round++) {
        scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < 64; i++) tsuzuri_tz_go(fp_leave_upward, NULL);
        join(scope);
        scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < 64; i++) tsuzuri_tz_go(fp_check_default, NULL);
        join(scope);
    }
    scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 64; i++) tsuzuri_tz_go(i % 2 ? fp_flag_raiser : fp_flag_clean, NULL);
    join(scope);
#if defined(__x86_64__)
    scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 64; i++) tsuzuri_tz_go(i % 2 ? x87_raiser : x87_clean, NULL);
    join(scope);
#endif
    printf("fp rounding modes: ok\n");
}

/* 18. a finite wake of more than 64 waiters wakes that many; the rest stay */
static _Atomic uint64_t wake_word;
static void wake_waiter(void *env) {
    (void)env;
    CHECK(tsuzuri_tz_addr_wait(&wake_word, 0) == 0);
    atomic_fetch_add(&counter, 1);
}
static int64_t queued_waiters(void *addr) {
    size_t b = tzr_bucket(addr);
    int64_t n = 0;
    tzr_lock_acquire(&tzr_buckets[b].lock);
    for (tzr_waiter *w = tzr_buckets[b].first; w; w = w->next) n += w->addr == addr;
    tzr_lock_release(&tzr_buckets[b].lock);
    return n;
}
static void test_wake_many(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 200; i++) tsuzuri_tz_go(wake_waiter, NULL);
    while (queued_waiters((void *)&wake_word) < 200) tsuzuri_tz_yield();
    atomic_store(&wake_word, 1);
    CHECK(tsuzuri_tz_addr_wake(&wake_word, 150) == 150);
    CHECK(queued_waiters((void *)&wake_word) == 50);
    CHECK(tsuzuri_tz_addr_wake(&wake_word, -1) == 50);
    join(scope);
    CHECK(atomic_load(&counter) == 200);
    /* a WaitGroup counter that would pass INT64_MAX is an error, not an overflow (UBSan checks) */
    _Atomic uint64_t group = 0;
    CHECK(tsuzuri_tz_waitgroup_add(&group, INT64_MAX) == 0);
    CHECK(tsuzuri_tz_waitgroup_add(&group, 1) == 1);
    printf("wake many: ok\n");
}

/* 19. Routine.count is exact as soon as a join returns */
static void test_count_after_join(void) {
    for (int round = 0; round < 2000; round++) {
        void *scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < 4; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
        join(scope);
        CHECK(tsuzuri_tz_count() == 1);
    }
    printf("count after join: ok\n");
}

/* 20. the deadlock detector sees the end of the last running thread and the last timer, and a
 * select over many cases fits in a small stack (children with a watchdog) */
static void watchdog_sleep(void) {
#if defined(_WIN32)
    Sleep(20000);
#else
    sleep(20);
#endif
}
#if defined(_WIN32)
static DWORD WINAPI watchdog(LPVOID arg) {
    (void)arg;
    watchdog_sleep();
    _exit(97);
}
static void start_watchdog(void) { CloseHandle(CreateThread(NULL, 0, watchdog, NULL, 0, NULL)); }
#else
static void *watchdog(void *arg) {
    (void)arg;
    watchdog_sleep();
    _exit(97);
}
static void start_watchdog(void) {
    pthread_t thread;
    pthread_create(&thread, NULL, watchdog, NULL);
    pthread_detach(thread);
}
#endif
static void *leaving_thread(void *arg) {
    tsuzuri_tz_count(); /* becomes a pseudo-G, running */
    (void)tzr_current();
    atomic_store((_Atomic int *)arg, 1);
    for (int i = 0; i < 100; i++) tsuzuri_tz_yield();
    return NULL; /* its exit leaves only parked participants */
}
#if defined(_WIN32)
static DWORD WINAPI leaving_entry(LPVOID arg) {
    leaving_thread(arg);
    return 0;
}
#endif
static void exit_deadlock_body(void) {
    start_watchdog();
    tzr_chan *c = chan_make(8, 0);
    _Atomic int started = 0;
#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, leaving_entry, (void *)&started, 0, NULL);
    CloseHandle(thread);
#else
    pthread_t thread;
    pthread_create(&thread, NULL, leaving_thread, (void *)&started);
    pthread_detach(thread);
#endif
    while (!atomic_load(&started)) tsuzuri_tz_yield();
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(blocked, c);
    _exit(tsuzuri_tz_scope_leave(scope) == 1 ? 44 : 1);
    (void)thread;
}
static void sleep_then_block(void *env) {
    CHECK(tsuzuri_tz_sleep(5000000) == 0);
    blocked(env);
}
static void timer_deadlock_body(void) {
    start_watchdog();
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 8; i++) tsuzuri_tz_go(sleep_then_block, c);
    _exit(tsuzuri_tz_scope_leave(scope) == 1 ? 45 : 1);
}
static void big_select_routine(void *env) {
    (void)env;
    int64_t n = 40000;
    tzr_chan **channels = calloc((size_t)n, sizeof *channels);
    tzr_scase *cases = calloc((size_t)n, sizeof *cases);
    int64_t values[1];
    for (int64_t i = 0; i < n; i++) {
        channels[i] = chan_make(8, 1);
        cases[i] = (tzr_scase){.c = channels[i], .elem = values};
    }
    int64_t one = 7;
    CHECK(tsuzuri_tz_chan_send(channels[n / 2], &one, 0) == TZR_OK);
    int32_t ok;
    CHECK(tsuzuri_tz_select(cases, n, 1, &ok) == n / 2 && ok == 1 && values[0] == 7);
    CHECK(tsuzuri_tz_select(cases, n, 0, &ok) == -1);
    for (int64_t i = 0; i < n; i++) free(channels[i]);
    free(channels);
    free(cases);
}
static void big_select_body(void) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(big_select_routine, NULL);
    join(scope);
}
/* A host thread that handles a deadlock status (as --trap-mode return lets it) can wait again: the
 * verdict is retired, its wait records are gone, and a later deadlock is found again. */
static void recover_body(void) {
    start_watchdog();
    tzr_chan *c = chan_make(8, 0);
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_DEADLOCK);
    CHECK(c->recvq.first == NULL); /* the record of the wait is not left in the queue */
    CHECK(tsuzuri_tz_chan_send(c, &v, 1) == TZR_DEADLOCK);
    CHECK(c->sendq.first == NULL);
    static _Atomic uint64_t word = 1;
    CHECK(tsuzuri_tz_addr_wait(&word, 1) == 2);
    CHECK(queued_waiters((void *)&word) == 0);
    CHECK(tsuzuri_tz_addr_wake(&word, -1) == 0);
    /* and an ordinary wait still works */
    tzr_chan *buffered = chan_make(8, 1);
    v = 7;
    CHECK(tsuzuri_tz_chan_send(buffered, &v, 1) == TZR_OK);
    CHECK(tsuzuri_tz_chan_recv(buffered, &v, 1) == TZR_OK && v == 7);
    CHECK(tsuzuri_tz_sleep(1000000) == 0);
    _exit(46);
}
static void test_detector(void) {
    int s = run_child("child-exit-deadlock");
    CHECK(exited_with(s, 44));
    for (int i = 0; i < 4; i++) {
        s = run_child("child-timer-deadlock");
        CHECK(exited_with(s, 45));
    }
    s = run_child("child-recover");
    CHECK(exited_with(s, 46));
    s = run_child_with("child-big-select", "64"); /* 64 KiB stacks */
    CHECK(exited_with(s, 0));
    printf("detector and large select: ok\n");
}

/* 21. a routine queued behind two routines that hand off to each other through runnext still runs
 * (with one P, the hand-off would otherwise keep the local ring waiting forever) */
static _Atomic int starve_stop;
typedef struct { tzr_chan *ping, *pong; } starve_pair;
static void starve_flag(void *env) {
    (void)env;
    atomic_store(&starve_stop, 1);
}
static void starve_ponger(void *env) {
    starve_pair *pair = env;
    int64_t v;
    while (tsuzuri_tz_chan_recv(pair->ping, &v, 1) == TZR_OK && v >= 0) CHECK(tsuzuri_tz_chan_send(pair->pong, &v, 1) == TZR_OK);
}
static void starve_pinger(void *env) {
    starve_pair *pair = env;
    for (int64_t round = 0; !atomic_load(&starve_stop); round++) {
        int64_t v = round, w;
        CHECK(tsuzuri_tz_chan_send(pair->ping, &v, 1) == TZR_OK);
        /* spawned into runnext, then pushed to the ring when the receive below wakes the partner */
        if (round == 10) tsuzuri_tz_go(starve_flag, NULL);
        CHECK(tsuzuri_tz_chan_recv(pair->pong, &w, 1) == TZR_OK && w == round);
    }
    int64_t stop = -1;
    CHECK(tsuzuri_tz_chan_send(pair->ping, &stop, 1) == TZR_OK);
}
static void starve_root(void *env) {
    tsuzuri_tz_go(starve_ponger, env);
    tsuzuri_tz_go(starve_pinger, env);
}
static void test_starvation(void) {
    start_watchdog();
    starve_pair pair = {chan_make(8, 0), chan_make(8, 0)};
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(starve_root, &pair);
    join(scope);
    free(pair.ping);
    free(pair.pong);
    printf("no starvation behind runnext: ok\n");
}

/* 22. two receive cases on the same channel are both chosen when the select has to wait */
typedef struct { tzr_chan *c; int64_t rounds; } same_feed;
static void same_feeder(void *env) {
    same_feed *feed = env;
    for (int64_t i = 0; i < feed->rounds; i++) {
        for (;;) { /* send once the select is waiting on both cases */
            tzr_lock_acquire(&feed->c->lock);
            int waiting = feed->c->recvq.first != NULL && feed->c->recvq.first != feed->c->recvq.last;
            tzr_lock_release(&feed->c->lock);
            if (waiting) break;
            tsuzuri_tz_yield();
        }
        CHECK(tsuzuri_tz_chan_send(feed->c, &i, 1) == TZR_OK);
    }
}
static void test_select_same(void) {
    same_feed feed = {chan_make(8, 0), 4000};
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(same_feeder, &feed);
    int64_t hits[2] = {0, 0};
    for (int64_t i = 0; i < feed.rounds; i++) {
        int64_t a = -1, b = -1;
        tzr_scase cases[2] = {{.c = feed.c, .elem = &a}, {.c = feed.c, .elem = &b}};
        int32_t ok;
        int64_t k = tsuzuri_tz_select(cases, 2, 1, &ok);
        CHECK((k == 0 || k == 1) && ok == 1 && (k == 0 ? a : b) == i);
        hits[k]++;
    }
    join(scope);
    CHECK(hits[0] > 1000 && hits[1] > 1000);
    printf("select on one channel twice: %lld / %lld\n", (long long)hits[0], (long long)hits[1]);
    free(feed.c);
}

/* TZ08: blocking calls nest, and after the outer exit the G has a P again: it spawns and switches,
 * and with one P the tzroutines that it spawned run. */
static _Atomic int64_t blocking_hits;
static void blocking_child(void *env) {
    (void)env;
    atomic_fetch_add(&blocking_hits, 1);
}
static void blocking_routine(void *env) {
    (void)env;
    tsuzuri_tz_blocking_enter();
    tsuzuri_tz_blocking_enter();
    for (volatile int spin = 0; spin < 20000; spin++) {
    }
    tsuzuri_tz_blocking_exit();
    tsuzuri_tz_blocking_exit();
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(blocking_child, NULL);
    tsuzuri_tz_yield();
    join(scope);
    atomic_fetch_add(&blocking_hits, 100);
}
static void test_blocking(void) {
    for (int round = 0; round < 50; round++) {
        atomic_store(&blocking_hits, 0);
        void *scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < 8; i++) tsuzuri_tz_go(blocking_routine, NULL);
        join(scope);
        CHECK(atomic_load(&blocking_hits) == 8 * 104);
    }
}

/* TZ02, TZ08: the spawn of the main thread makes sysmon give the P of a blocking call to an M, and
 * the G leaves its call before that M runs, so it waits for the P. The M hands the P over and parks:
 * it stops spinning, or tzr_wakep would never start an M again and the next spawn of the main thread
 * would wait for nobody. With one P, the first round sets this up. */
static _Atomic int donor_inside;
static _Atomic int donor_leave;
static _Atomic int64_t donor_hits;
static void donor_child(void *env) {
    (void)env;
    atomic_fetch_add(&donor_hits, 1);
}
static void donor_blocker(void *env) {
    (void)env;
    tsuzuri_tz_blocking_enter();
    atomic_store(&donor_inside, 1);
    while (!atomic_load(&donor_leave)) {
    }
    tsuzuri_tz_blocking_exit();
    atomic_fetch_add(&donor_hits, 100);
}
static void test_donor(void) {
    for (int round = 0; round < 100; round++) {
        atomic_store(&donor_inside, 0);
        atomic_store(&donor_leave, 0);
        atomic_store(&donor_hits, 0);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(donor_blocker, NULL);
        while (!atomic_load(&donor_inside)) tzr_os_yield();
        tsuzuri_tz_go(donor_child, NULL);
        atomic_store(&donor_leave, 1);
        join(scope);
        CHECK(atomic_load(&donor_hits) == 101);
        /* the Ms that looked for work park within microseconds; a loaded runner may not run a woken
         * M for a while (the windows.yml run of #88 waited longer than 2 s once), and the bound is only
         * for a spinner that never parks */
        double t = seconds();
        while (atomic_load(&tzr_sched.nmspinning) != 0 && seconds() - t < 10.0) tzr_os_yield();
        CHECK(atomic_load(&tzr_sched.nmspinning) == 0);
    }
}

/* 25. timers (TZ05). The channel blocks come from the caller, as the generated code allocates them;
 * `timer_blocks` counts them as the program's allocator would. */
static _Atomic int64_t timer_blocks;
static tzr_chan *timer_block(void) {
    atomic_fetch_add(&timer_blocks, 1);
    tzr_chan *block = calloc(1, sizeof(tzr_chan) + sizeof(int64_t));
    CHECK(block != NULL);
    return block;
}
static void timer_block_free(void *block) {
    atomic_fetch_sub(&timer_blocks, 1);
    free(block);
}
/* The drop of one channel handle, as the generated code's. */
static void timer_chan_drop(tzr_chan *c) {
    if (tsuzuri_tz_chan_release(c)) timer_block_free(c);
}
/* The timers in the heap. Not tzr_sched.ntimers: the timer thread lowers it after it has started a
 * fired after_func's tzroutine (so that the detector never sees neither), which may have run and
 * ended by then. */
static int64_t pending_timers(void) {
    tzr_lock_acquire(&tzr_timers.lock);
    int64_t count = (int64_t)tzr_timers.count;
    tzr_lock_release(&tzr_timers.lock);
    return count;
}
static void *timer_after(int64_t ns, tzr_chan *block) {
    return tsuzuri_tz_timer_new(block, timer_block_free, ns, 0, TZR_TIMER_SEND, 0, NULL, NULL, NULL);
}
static void *timer_handle(int kind, int64_t ns) {
    return tsuzuri_tz_timer_new(timer_block(), timer_block_free, ns, kind == TZR_TIMER_TICK ? ns : 0, kind, 1, NULL,
                                NULL, NULL);
}

/* Time.after: one time, not before the deadline; the timer lets go of its reference by the end of
 * the boundary, so the caller's drop frees the block */
static int64_t timer_received;
static void timer_receiver(void *env) {
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(env, &v, 1) == TZR_OK);
    timer_received = v;
}
static void test_timer_channel(void) {
    void *scope = tsuzuri_tz_scope_enter();
    int64_t start = tzr_nanotime();
    tzr_chan *c = timer_block();
    timer_after(20000000, c);
    tsuzuri_tz_go(timer_receiver, c);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(timer_received >= start + 20000000);
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(c, &v, 0) == TZR_WOULD_BLOCK);
    CHECK(atomic_load(&c->refs) == 1);
    timer_chan_drop(c);
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    /* the main thread waits on one too */
    scope = tsuzuri_tz_scope_enter();
    c = timer_block();
    start = tzr_nanotime();
    timer_after(5000000, c);
    CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK && v >= start + 5000000);
    timer_chan_drop(c);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("timer channel: ok\n");
}

/* Timer.stop and Timer.reset: a stopped timer sends nothing, and no time sent before a stop or a
 * reset is received after it */
static void test_timer_stop(void) {
    void *scope = tsuzuri_tz_scope_enter();
    void *t = timer_handle(TZR_TIMER_SEND, INT64_MAX);
    tzr_chan *c = tsuzuri_tz_timer_chan(t);
    CHECK(pending_timers() == 1);
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    CHECK(tsuzuri_tz_timer_stop(t) == 0);
    CHECK(pending_timers() == 0);
    int64_t start = tzr_nanotime(), v = 0;
    CHECK(tsuzuri_tz_timer_reset(t, 5000000) == 0);
    CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK && v >= start + 5000000);
    CHECK(tsuzuri_tz_timer_stop(t) == 0); /* it fired */
    CHECK(tsuzuri_tz_timer_reset(t, INT64_MAX) == 0);
    CHECK(tsuzuri_tz_timer_reset(t, 1) == 1);
    CHECK(tsuzuri_tz_sleep(20000000) == 0);
    CHECK(tsuzuri_tz_chan_length(c) == 1); /* sent into the buffer */
    CHECK(tsuzuri_tz_timer_reset(t, INT64_MAX) == 0);
    CHECK(tsuzuri_tz_chan_length(c) == 0); /* the reset dropped it */
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    /* a stopped timer never sends: nothing after its deadline */
    CHECK(tsuzuri_tz_timer_reset(t, 1000000) == 0);
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    CHECK(tsuzuri_tz_sleep(10000000) == 0);
    CHECK(tsuzuri_tz_chan_recv(c, &v, 0) == TZR_WOULD_BLOCK);
    tsuzuri_tz_timer_free(t);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&timer_blocks) == 1); /* the channel handle keeps the block */
    timer_chan_drop(c);
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("timer stop: ok\n");
}

/* A program may send on a timer's channel too: a stop or a reset that empties the full buffer lets a
 * sender that waits for room put its item in, as a receive would, and the next receive takes it. */
static void timer_chan_sender(void *env) {
    int64_t v = 42;
    CHECK(tsuzuri_tz_chan_send(env, &v, 1) == TZR_OK);
}
static int sender_queued(tzr_chan *c) {
    tzr_lock_acquire(&c->lock);
    int queued = c->sendq.first != NULL;
    tzr_lock_release(&c->lock);
    return queued;
}
static void test_timer_senders(void) {
    for (int reset = 0; reset < 2; reset++) {
        void *scope = tsuzuri_tz_scope_enter();
        void *t = timer_handle(TZR_TIMER_SEND, INT64_MAX);
        tzr_chan *c = tsuzuri_tz_timer_chan(t);
        int64_t v = 7;
        CHECK(tsuzuri_tz_chan_send(c, &v, 0) == TZR_OK); /* the buffer of one is full */
        tsuzuri_tz_go(timer_chan_sender, c);
        while (!sender_queued(c)) CHECK(tsuzuri_tz_sleep(100000) == 0);
        if (reset) CHECK(tsuzuri_tz_timer_reset(t, INT64_MAX) == 1);
        else CHECK(tsuzuri_tz_timer_stop(t) == 1);
        CHECK(!sender_queued(c) && tsuzuri_tz_chan_length(c) == 1);
        CHECK(tsuzuri_tz_chan_recv(c, &v, 0) == TZR_OK && v == 42);
        CHECK(tsuzuri_tz_chan_recv(c, &v, 0) == TZR_WOULD_BLOCK);
        tsuzuri_tz_timer_free(t);
        tsuzuri_tz_boundary_leave(scope);
        timer_chan_drop(c);
    }
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("timer senders: ok\n");
}

/* Ticker: five times in order; a tick that the receiver has not taken the last one of is dropped;
 * stop ends it and drops what it sent; reset starts it again */
static void test_ticker(void) {
    void *scope = tsuzuri_tz_scope_enter();
    void *t = timer_handle(TZR_TIMER_TICK, 2000000);
    tzr_chan *c = tsuzuri_tz_timer_chan(t);
    int64_t last = 0, v = 0;
    for (int i = 0; i < 5; i++) {
        CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK);
        CHECK(v > last);
        last = v;
    }
    CHECK(tsuzuri_tz_sleep(30000000) == 0);
    CHECK(tsuzuri_tz_chan_length(c) == 1);
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    CHECK(tsuzuri_tz_chan_length(c) == 0);
    CHECK(tsuzuri_tz_sleep(10000000) == 0);
    CHECK(tsuzuri_tz_chan_recv(c, &v, 0) == TZR_WOULD_BLOCK);
    CHECK(tsuzuri_tz_timer_reset(t, 1000000) == 0);
    for (int i = 0; i < 3; i++) CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK && v > last);
    CHECK(pending_timers() == 1);
    tsuzuri_tz_timer_free(t);
    CHECK(pending_timers() == 0);
    timer_chan_drop(c);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&timer_blocks) == 0);
    printf("ticker: ok\n");
}

/* Time.tick: once the caller's handle is gone, the next tick stops it and frees the block */
static void test_ticker_drop(void) {
    void *scope = tsuzuri_tz_scope_enter();
    tzr_chan *c = timer_block();
    tsuzuri_tz_timer_new(c, timer_block_free, 1000000, 1000000, TZR_TIMER_TICK, 0, NULL, NULL, NULL);
    int64_t v = 0;
    for (int i = 0; i < 3; i++) CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK);
    timer_chan_drop(c);
    while (atomic_load(&timer_blocks) != 0) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    CHECK(pending_timers() == 0);
    tsuzuri_tz_boundary_leave(scope);
    printf("ticker drop: ok\n");
}

/* Time.after_func: the function runs once on a new tzroutine of the boundary; a stopped one keeps
 * it for a reset, and the drop of the handle drops it without a call */
static _Atomic int64_t func_runs, func_drops;
static void func_run(void *box) {
    CHECK(box == &func_runs);
    CHECK(tzr_current()->kind == TZR_KIND_ROUTINE);
    atomic_fetch_add(&func_runs, 1);
}
static void func_drop(void *box) {
    CHECK(box == &func_runs);
    atomic_fetch_add(&func_drops, 1);
}
static void *timer_after(int64_t ns, tzr_chan *block);
static void func_waits_for_timer(void *box) {
    CHECK(box == &func_runs);
    tzr_chan *c = timer_block();
    timer_after(2000000, c);
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_OK);
    timer_chan_drop(c);
    atomic_fetch_add(&func_runs, 1);
}
static void *timer_func(int64_t ns) {
    return tsuzuri_tz_timer_new(timer_block(), timer_block_free, ns, 0, TZR_TIMER_FUNC, 1, func_run, func_drop,
                                &func_runs);
}
static void test_after_func(void) {
    atomic_store(&func_runs, 0);
    atomic_store(&func_drops, 0);
    void *scope = tsuzuri_tz_scope_enter();
    void *t = timer_func(5000000);
    tzr_chan *c = tsuzuri_tz_timer_chan(t);
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(c, &v, 1) == TZR_CLOSED); /* closed and empty */
    while (atomic_load(&func_runs) == 0) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    CHECK(tsuzuri_tz_timer_reset(t, 1) == 0); /* it ran: there is nothing to start again */
    CHECK(tsuzuri_tz_timer_stop(t) == 0 && pending_timers() == 0);
    tsuzuri_tz_timer_free(t);
    timer_chan_drop(c);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&func_runs) == 1 && atomic_load(&func_drops) == 0);
    scope = tsuzuri_tz_scope_enter();
    t = timer_func(INT64_MAX);
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    CHECK(tsuzuri_tz_timer_reset(t, INT64_MAX) == 0); /* the same function, again */
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    CHECK(tsuzuri_tz_timer_reset(t, 1000000) == 0);
    while (atomic_load(&func_runs) == 1) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    tsuzuri_tz_timer_free(t);
    t = timer_func(INT64_MAX);
    CHECK(tsuzuri_tz_timer_stop(t) == 1);
    tsuzuri_tz_timer_free(t);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&func_runs) == 2 && atomic_load(&func_drops) == 1);
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("after func: ok\n");
}

/* The end of a boundary stops its pending timers without waiting for them: Time.after's and
 * Time.tick's go with their references, a handle's record stays stopped for its drop, and an
 * after_func's function is dropped. One that fires as the boundary ends runs and is waited for,
 * or is dropped: never both, never neither. */
static void test_boundary_timers(void) {
    atomic_store(&func_runs, 0);
    atomic_store(&func_drops, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tzr_chan *after = timer_block();
    timer_after(INT64_MAX, after);
    tzr_chan *tick = timer_block();
    tsuzuri_tz_timer_new(tick, timer_block_free, INT64_MAX, INT64_MAX, TZR_TIMER_TICK, 0, NULL, NULL, NULL);
    void *timer = timer_handle(TZR_TIMER_SEND, INT64_MAX);
    void *func = timer_func(INT64_MAX);
    timer_chan_drop(after);
    timer_chan_drop(tick);
    CHECK(pending_timers() == 4);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(pending_timers() == 0);
    CHECK(atomic_load(&timer_blocks) == 2); /* the handles' records keep their channels */
    CHECK(atomic_load(&func_runs) == 0 && atomic_load(&func_drops) == 1);
    CHECK(tsuzuri_tz_timer_stop(timer) == 0 && tsuzuri_tz_timer_stop(func) == 0);
    tsuzuri_tz_timer_free(timer);
    tsuzuri_tz_timer_free(func);
    CHECK(atomic_load(&timer_blocks) == 0 && atomic_load(&func_drops) == 1);
    enum { N = 1000 };
    static void *funcs[N];
    for (int round = 0; round < 3; round++) {
        atomic_store(&func_runs, 0);
        atomic_store(&func_drops, 0);
        scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < N; i++) funcs[i] = timer_func(i % 3 == 0 ? 0 : (int64_t)(i % 50) * 20000);
        tsuzuri_tz_boundary_leave(scope);
        CHECK(atomic_load(&func_runs) + atomic_load(&func_drops) == N);
        CHECK(pending_timers() == 0);
        for (int i = 0; i < N; i++) tsuzuri_tz_timer_free(funcs[i]);
        CHECK(atomic_load(&func_runs) + atomic_load(&func_drops) == N);
    }
    CHECK(atomic_load(&timer_blocks) == 0);
    /* a stopped after_func whose Timer outlives the boundary: the boundary drops its function, and
     * a later reset has nothing to start */
    atomic_store(&func_runs, 0);
    atomic_store(&func_drops, 0);
    scope = tsuzuri_tz_scope_enter();
    void *kept = timer_func(INT64_MAX);
    CHECK(tsuzuri_tz_timer_stop(kept) == 1);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&func_drops) == 1 && atomic_load(&func_runs) == 0);
    CHECK(tsuzuri_tz_timer_reset(kept, 1) == 0);
    CHECK(tsuzuri_tz_sleep(5000000) == 0);
    CHECK(atomic_load(&func_runs) == 0 && pending_timers() == 0);
    tsuzuri_tz_timer_free(kept);
    CHECK(atomic_load(&func_drops) == 1 && atomic_load(&timer_blocks) == 0);
    /* an after_func that fires as the boundary ends and waits for a timer of its own: the boundary
     * waits for it instead of stopping the timer it waits for */
    for (int round = 0; round < 200; round++) {
        atomic_store(&func_runs, 0);
        atomic_store(&func_drops, 0);
        scope = tsuzuri_tz_scope_enter();
        void *later = tsuzuri_tz_timer_new(timer_block(), timer_block_free, round % 2, 0, TZR_TIMER_FUNC, 1,
                                           func_waits_for_timer, func_drop, &func_runs);
        tsuzuri_tz_boundary_leave(scope);
        CHECK(atomic_load(&func_runs) + atomic_load(&func_drops) == 1);
        tsuzuri_tz_timer_free(later);
    }
    CHECK(atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("boundary timers: ok\n");
}

#if !defined(_WIN32)
/* Time.sleep_ms on a tzroutine waits through the hook that the scheduler installs: 2,000 of them
 * sleep at once without holding the Ms. A thread is not a tzroutine: os.c stops it itself. */
static void hook_sleeper(void *env) {
    (void)env;
    int32_t (*hook)(int64_t) = atomic_load(&tsuzuri_os_sleep_hook);
    CHECK(hook != NULL && hook(20000000) == 1);
    atomic_fetch_add(&counter, 1);
}
static void test_sleep_ms_hook(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    for (int i = 0; i < 2000; i++) tsuzuri_tz_go(hook_sleeper, NULL);
    join(scope);
    double e = seconds() - t;
    CHECK(atomic_load(&counter) == 2000);
    CHECK(e >= 0.020);
    CHECK(atomic_load(&tsuzuri_os_sleep_hook)(1) == 0);
    printf("sleep_ms hook: 2000 x 20 ms in %.3f s\n", e);
}
#else
static void test_sleep_ms_hook(void) { printf("sleep_ms hook: no os.c on Windows\n"); }
#endif

/* 26. the sync types (TZ06) */
static _Atomic uint32_t mutex32;
static int64_t mutex32_plain;
static _Atomic uint32_t mutex32_other;
static void mutex32_locker(void *env) {
    int64_t rounds = (int64_t)(intptr_t)env;
    for (int64_t i = 0; i < rounds; i++) {
        CHECK(tsuzuri_tz_mutex32_lock(&mutex32) == 0);
        CHECK(tsuzuri_tz_mutex32_lock(&mutex32) == 1); /* the same mutex again: the generated code traps */
        CHECK(tsuzuri_tz_mutex32_lock(&mutex32_other) == 0); /* another one nests */
        int64_t v = mutex32_plain;
        if (i % 16 == 0) tsuzuri_tz_yield(); /* a holder may switch away (Go's Mutex) */
        mutex32_plain = v + 1;
        tsuzuri_tz_mutex32_unlock(&mutex32_other);
        tsuzuri_tz_mutex32_unlock(&mutex32);
    }
}
static void test_mutex32(void) {
    mutex32_plain = 0;
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 16; i++) tsuzuri_tz_go(mutex32_locker, (void *)(intptr_t)1000);
    mutex32_locker((void *)(intptr_t)1000); /* the main thread too */
    join(scope);
    CHECK(mutex32_plain == 17000);
    CHECK(atomic_load(&mutex32) == 0 && tzr_current()->held_count == 0);
    /* a depth that a trap left: restore releases what was taken above it */
    int32_t depth = tsuzuri_tz_held_depth();
    CHECK(tsuzuri_tz_mutex32_lock(&mutex32) == 0);
    _Atomic uint64_t rw = 0;
    CHECK(tsuzuri_tz_rw_read_lock(&rw) == 0);
    tsuzuri_tz_held_restore(depth);
    CHECK(atomic_load(&mutex32) == 0 && atomic_load(&rw) == 0 && tsuzuri_tz_held_depth() == depth);
    printf("mutex32: ok\n");
}

/* RwMutex: 8 readers and 2 writers; a writer never sees a reader, and readers share */
static _Atomic uint64_t rw_word;
static _Atomic int64_t rw_readers, rw_writers, rw_shared, rw_violations;
static int64_t rw_value;
static void rw_reader(void *env) {
    (void)env;
    for (int i = 0; i < 1000; i++) {
        CHECK(tsuzuri_tz_rw_read_lock(&rw_word) == 0);
        CHECK(tsuzuri_tz_rw_read_lock(&rw_word) == 1); /* again in the same G: reported 6 */
        CHECK(tsuzuri_tz_rw_write_lock(&rw_word) == 1);
        int64_t now = atomic_fetch_add(&rw_readers, 1) + 1;
        if (now > 1) atomic_fetch_add(&rw_shared, 1);
        if (atomic_load(&rw_writers) != 0) atomic_fetch_add(&rw_violations, 1);
        if (i % 64 == 0) tsuzuri_tz_yield();
        atomic_fetch_sub(&rw_readers, 1);
        tsuzuri_tz_rw_read_unlock(&rw_word);
    }
}
static void rw_writer(void *env) {
    (void)env;
    for (int i = 0; i < 1000; i++) {
        CHECK(tsuzuri_tz_rw_write_lock(&rw_word) == 0);
        CHECK(tsuzuri_tz_rw_read_lock(&rw_word) == 1);
        if (atomic_fetch_add(&rw_writers, 1) != 0 || atomic_load(&rw_readers) != 0) atomic_fetch_add(&rw_violations, 1);
        rw_value++;
        if (i % 64 == 0) tsuzuri_tz_yield();
        atomic_fetch_sub(&rw_writers, 1);
        tsuzuri_tz_rw_write_unlock(&rw_word);
    }
}
static void test_rwmutex(void) {
    rw_value = 0;
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 8; i++) tsuzuri_tz_go(rw_reader, NULL);
    for (int i = 0; i < 2; i++) tsuzuri_tz_go(rw_writer, NULL);
    join(scope);
    CHECK(atomic_load(&rw_violations) == 0);
    CHECK(rw_value == 2000);
    CHECK(atomic_load(&rw_word) == 0);
    printf("rwmutex: ok (%lld shared reads)\n", (long long)atomic_load(&rw_shared));
}

/* Writer preference with two writers queued: a reader that comes while they wait goes after both,
 * also after the first one has taken the lock and let it go. */
static _Atomic uint64_t pref_word;
static int pref_order[3];
static _Atomic int pref_count;
static void pref_writer(void *env) {
    CHECK(tsuzuri_tz_rw_write_lock(&pref_word) == 0);
    pref_order[atomic_fetch_add(&pref_count, 1)] = (int)(intptr_t)env;
    tsuzuri_tz_rw_write_unlock(&pref_word);
}
static void pref_reader(void *env) {
    (void)env;
    CHECK(tsuzuri_tz_rw_read_lock(&pref_word) == 0);
    pref_order[atomic_fetch_add(&pref_count, 1)] = 0;
    tsuzuri_tz_rw_read_unlock(&pref_word);
}
static void test_rw_writers_first(void) {
    for (int round = 0; round < 20; round++) {
        atomic_store(&pref_count, 0);
        CHECK(tsuzuri_tz_rw_read_lock(&pref_word) == 0);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(pref_writer, (void *)(intptr_t)1);
        tsuzuri_tz_go(pref_writer, (void *)(intptr_t)2);
        while ((atomic_load(&pref_word) & TZR_RW_WAITERS) != 2 * TZR_RW_WAITER) CHECK(tsuzuri_tz_sleep(100000) == 0);
        tsuzuri_tz_go(pref_reader, NULL);
        CHECK(tsuzuri_tz_sleep(1000000) == 0);
        tsuzuri_tz_rw_read_unlock(&pref_word);
        join(scope);
        CHECK(atomic_load(&pref_count) == 3 && pref_order[2] == 0);
        CHECK(atomic_load(&pref_word) == 0);
    }
    printf("rw writers first: ok\n");
}

/* Once: 16 callers at once, the function runs once and everyone returns after it */
static _Atomic uint64_t once_word;
static _Atomic int64_t once_runs, once_after;
static void once_caller(void *env) {
    (void)env;
    int32_t status = tsuzuri_tz_once_begin(&once_word);
    CHECK(status == 0 || status == 1);
    if (status == 1) {
        for (int i = 0; i < 100; i++) tsuzuri_tz_yield(); /* the others wait meanwhile */
        atomic_fetch_add(&once_runs, 1);
        tsuzuri_tz_once_end(&once_word);
    }
    if (atomic_load(&once_runs) == 1) atomic_fetch_add(&once_after, 1);
}
static void test_once(void) {
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 16; i++) tsuzuri_tz_go(once_caller, NULL);
    join(scope);
    CHECK(atomic_load(&once_runs) == 1 && atomic_load(&once_after) == 16);
    CHECK(tsuzuri_tz_once_begin(&once_word) == 0);
    printf("once: ok\n");
}

/* Cond: a queue that consumers wait on until it is not empty */
static _Atomic uint32_t cond_mutex;
static _Atomic uint64_t cond_sequence;
static int64_t cond_queue, cond_taken;
static void cond_consumer(void *env) {
    (void)env;
    for (int i = 0; i < 250; i++) {
        CHECK(tsuzuri_tz_mutex32_lock(&cond_mutex) == 0);
        while (cond_queue == 0) {
            uint64_t observed = atomic_load(&cond_sequence);
            CHECK(tsuzuri_tz_cond_wait(&cond_sequence, &cond_mutex, observed) == 0);
        }
        cond_queue--;
        cond_taken++;
        tsuzuri_tz_mutex32_unlock(&cond_mutex);
    }
}
static void cond_producer(void *env) {
    (void)env;
    for (int i = 0; i < 500; i++) {
        CHECK(tsuzuri_tz_mutex32_lock(&cond_mutex) == 0);
        cond_queue++;
        tsuzuri_tz_mutex32_unlock(&cond_mutex);
        tsuzuri_tz_cond_wake(&cond_sequence, i % 2 ? 1 : -1);
        if (i % 8 == 0) tsuzuri_tz_yield();
    }
}
static void test_cond(void) {
    cond_queue = cond_taken = 0;
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(cond_consumer, NULL);
    for (int i = 0; i < 2; i++) tsuzuri_tz_go(cond_producer, NULL);
    join(scope);
    CHECK(cond_taken == 1000 && cond_queue == 0);
    printf("cond: ok\n");
}

/* Semaphore: capacity 3 and 20 tzroutines; at most 3 hold a weight at once */
static _Atomic uint64_t sem_word;
static _Atomic int64_t sem_inside, sem_most;
static void sem_worker(void *env) {
    (void)env;
    for (int i = 0; i < 100; i++) {
        CHECK(tsuzuri_tz_sem_acquire(&sem_word, 1, 1) == 0);
        int64_t now = atomic_fetch_add(&sem_inside, 1) + 1;
        for (int64_t most = atomic_load(&sem_most); now > most && !atomic_compare_exchange_weak(&sem_most, &most, now);) {
        }
        tsuzuri_tz_yield();
        atomic_fetch_sub(&sem_inside, 1);
        CHECK(tsuzuri_tz_sem_release(&sem_word, 1, 3) == 0);
    }
}
static void test_semaphore(void) {
    atomic_store(&sem_word, 3);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 20; i++) tsuzuri_tz_go(sem_worker, NULL);
    join(scope);
    CHECK(atomic_load(&sem_most) <= 3 && atomic_load(&sem_most) >= 1);
    CHECK(atomic_load(&sem_word) == 3);
    CHECK(tsuzuri_tz_sem_acquire(&sem_word, 2, 0) == 0);
    CHECK(tsuzuri_tz_sem_acquire(&sem_word, 2, 0) == 1); /* only 1 left: try_acquire fails */
    CHECK(tsuzuri_tz_sem_release(&sem_word, 2, 3) == 0);
    CHECK(tsuzuri_tz_sem_release(&sem_word, 1, 3) == 1); /* more than it acquired: reported 7 */
    printf("semaphore: ok (at most %lld)\n", (long long)atomic_load(&sem_most));
}

/* ---------------------------------------------------------------- contexts (TZ07) */

static _Atomic int64_t ctx_blocks;
static void *ctx_block(void) {
    atomic_fetch_add(&ctx_blocks, 1);
    void *block = calloc(1, sizeof(tzr_chan));
    CHECK(block != NULL);
    return block;
}
static void ctx_block_free(void *block) {
    atomic_fetch_sub(&ctx_blocks, 1);
    free(block);
}
static void *ctx_new(void *parent, int64_t deadline) {
    return tsuzuri_tz_ctx_new(parent, deadline, ctx_block(), ctx_block_free);
}
/* The drop of a handle of a done channel, as the generated code's. */
static void ctx_done_drop(tzr_chan *c) {
    if (tsuzuri_tz_chan_release(c)) ctx_block_free(c);
}
/* Whether the done channel is closed, without waiting. */
static int ctx_closed(void *ctx) {
    tzr_chan *done = tsuzuri_tz_ctx_done(ctx);
    unsigned char unit = 0;
    int closed = tsuzuri_tz_chan_recv(done, &unit, 0) == TZR_CLOSED;
    ctx_done_drop(done);
    return closed;
}
static void ctx_waiter(void *env) {
    unsigned char unit = 0;
    CHECK(tsuzuri_tz_chan_recv(env, &unit, 1) == TZR_CLOSED);
    ctx_done_drop(env);
    atomic_fetch_add(&counter, 1);
}

/* A cancel closes the done channels of the context and every descendant with its reason, wakes
 * the tzroutines that wait on them and leaves the rest of the tree; a second cancel keeps the
 * reason; a child of a cancelled context starts cancelled; a chain of 100,000 is cancelled and
 * freed without recursion. */
static void test_context_tree(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    void *root = ctx_new(NULL, INT64_MAX);
    void *a = ctx_new(root, INT64_MAX);
    void *b = ctx_new(a, INT64_MAX);
    void *c = ctx_new(b, INT64_MAX);
    void *other = ctx_new(root, INT64_MAX);
    void *watched[3] = {a, b, c};
    for (int i = 0; i < 3; i++)
        for (int j = 0; j < 4; j++) tsuzuri_tz_go(ctx_waiter, tsuzuri_tz_ctx_done(watched[i]));
    CHECK(tsuzuri_tz_ctx_err(c) == 0 && !ctx_closed(c));
    tsuzuri_tz_ctx_cancel(a);
    join(scope);
    CHECK(atomic_load(&counter) == 12);
    CHECK(tsuzuri_tz_ctx_err(a) == 1 && tsuzuri_tz_ctx_err(b) == 1 && tsuzuri_tz_ctx_err(c) == 1);
    CHECK(ctx_closed(a) && ctx_closed(b) && ctx_closed(c));
    CHECK(tsuzuri_tz_ctx_err(root) == 0 && tsuzuri_tz_ctx_err(other) == 0 && !ctx_closed(other));
    tsuzuri_tz_ctx_cancel(root); /* a root is never cancelled */
    CHECK(tsuzuri_tz_ctx_err(root) == 0 && !ctx_closed(root) && tsuzuri_tz_ctx_err(other) == 0);
    tsuzuri_tz_ctx_cancel(c);
    CHECK(tsuzuri_tz_ctx_err(c) == 1);
    void *late = ctx_new(b, INT64_MAX);
    CHECK(tsuzuri_tz_ctx_err(late) == 1 && ctx_closed(late));
    CHECK(tsuzuri_tz_ctx_deadline(root) == INT64_MAX && tsuzuri_tz_ctx_deadline(late) == INT64_MAX);
    void *contexts[6] = {root, a, b, c, other, late};
    for (int i = 0; i < 6; i++) tsuzuri_tz_ctx_release(contexts[i]);
    enum { DEPTH = 100000 };
    void **chain = malloc(DEPTH * sizeof *chain);
    CHECK(chain != NULL);
    chain[0] = ctx_new(NULL, INT64_MAX);
    for (int i = 1; i < DEPTH; i++) chain[i] = ctx_new(chain[i - 1], INT64_MAX);
    tsuzuri_tz_ctx_cancel(chain[1]);
    CHECK(tsuzuri_tz_ctx_err(chain[DEPTH - 1]) == 1 && tsuzuri_tz_ctx_err(chain[0]) == 0);
    for (int i = 0; i < DEPTH; i++) tsuzuri_tz_ctx_release(chain[i]); /* the parents first */
    free(chain);
    CHECK(atomic_load(&ctx_blocks) == 0);
    printf("context tree: ok\n");
}

/* A deadline cancels with reason 2, not before it; a child inherits an earlier deadline of its
 * parent; a deadline that has passed cancels at once; the drop of the last handle stops a pending
 * deadline. */
static void test_context_deadline(void) {
    void *scope = tsuzuri_tz_scope_enter();
    void *root = ctx_new(NULL, INT64_MAX);
    int64_t start = tzr_nanotime();
    void *timed = tsuzuri_tz_ctx_timeout(root, 20000000, ctx_block(), ctx_block_free);
    CHECK(tsuzuri_tz_ctx_deadline(timed) >= start + 20000000);
    void *child = ctx_new(timed, INT64_MAX - 1);
    CHECK(tsuzuri_tz_ctx_deadline(child) == tsuzuri_tz_ctx_deadline(timed));
    tzr_chan *done = tsuzuri_tz_ctx_done(child);
    unsigned char unit = 0;
    CHECK(tsuzuri_tz_chan_recv(done, &unit, 1) == TZR_CLOSED);
    CHECK(tzr_nanotime() - start >= 20000000);
    CHECK(tsuzuri_tz_ctx_err(timed) == 2 && tsuzuri_tz_ctx_err(child) == 2);
    ctx_done_drop(done);
    void *past = ctx_new(root, 0);
    CHECK(tsuzuri_tz_ctx_err(past) == 2 && ctx_closed(past));
    void *saturated = tsuzuri_tz_ctx_timeout(root, INT64_MAX, ctx_block(), ctx_block_free);
    CHECK(tsuzuri_tz_ctx_deadline(saturated) == INT64_MAX - 1 && tsuzuri_tz_ctx_err(saturated) == 0);
    void *contexts[4] = {timed, child, past, saturated};
    for (int i = 0; i < 4; i++) tsuzuri_tz_ctx_release(contexts[i]);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(pending_timers() == 0);
    /* in a fresh boundary, so that no deadline above is still being stopped */
    scope = tsuzuri_tz_scope_enter();
    void *distant = ctx_new(root, tzr_nanotime() + 3600 * (int64_t)1000000000);
    CHECK(pending_timers() == 1);
    tsuzuri_tz_ctx_release(distant);
    CHECK(pending_timers() == 0);
    tsuzuri_tz_ctx_release(root);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&ctx_blocks) == 0 && pending_timers() == 0);
    printf("context deadline: ok\n");
}

/* A child keeps its parent: the parent's handles may go first. A context whose deadline pends at
 * the end of its boundary has the deadline stopped there, and its handles still work. */
static void test_context_release(void) {
    void *scope = tsuzuri_tz_scope_enter();
    void *root = ctx_new(NULL, INT64_MAX);
    void *parent = ctx_new(root, INT64_MAX);
    void *child = ctx_new(parent, INT64_MAX);
    void *shared = parent;
    tsuzuri_tz_ctx_share(shared);
    tsuzuri_tz_ctx_release(root);
    tsuzuri_tz_ctx_release(parent);
    tsuzuri_tz_ctx_cancel(shared);
    CHECK(tsuzuri_tz_ctx_err(child) == 1);
    tsuzuri_tz_ctx_release(shared);
    tsuzuri_tz_ctx_release(child);
    CHECK(atomic_load(&ctx_blocks) == 0);
    root = ctx_new(NULL, INT64_MAX);
    void *timed = ctx_new(root, tzr_nanotime() + 3600 * (int64_t)1000000000);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(pending_timers() == 0 && tsuzuri_tz_ctx_err(timed) == 0);
    tsuzuri_tz_ctx_cancel(timed);
    CHECK(tsuzuri_tz_ctx_err(timed) == 1);
    tsuzuri_tz_ctx_release(timed);
    tsuzuri_tz_ctx_release(root);
    CHECK(atomic_load(&ctx_blocks) == 0);
    printf("context release: ok\n");
}

/* 10,000 children with a deadline of 1 ms, released at once, while their timers fire. */
static void test_context_deadline_race(void) {
    void *scope = tsuzuri_tz_scope_enter();
    void *root = ctx_new(NULL, INT64_MAX);
    for (int i = 0; i < 10000; i++) {
        void *child = tsuzuri_tz_ctx_timeout(root, (int64_t)(i % 5) * 250000, ctx_block(), ctx_block_free);
        if (i % 7 == 0) tsuzuri_tz_ctx_cancel(child);
        tsuzuri_tz_ctx_release(child);
    }
    tsuzuri_tz_ctx_release(root);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(pending_timers() == 0);
    while (atomic_load(&ctx_blocks) != 0) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    printf("context deadline race: ok\n");
}

/* A deadline that expires as its boundary ends counts in the boundary until its cancel and its
 * reference are over: right after the end of the boundary, only the root's block is left. */
static void test_context_expire_boundary(void) {
    void *root = ctx_new(NULL, INT64_MAX);
    for (int round = 0; round < 2000; round++) {
        void *scope = tsuzuri_tz_scope_enter();
        void *child = tsuzuri_tz_ctx_timeout(root, (int64_t)(round % 4) * 50000, ctx_block(), ctx_block_free);
        if (round % 2) CHECK(tsuzuri_tz_sleep((int64_t)(round % 3) * 50000) == 0);
        tsuzuri_tz_ctx_release(child);
        tsuzuri_tz_boundary_leave(scope);
        CHECK(atomic_load(&ctx_blocks) == 1);
    }
    tsuzuri_tz_ctx_release(root);
    CHECK(atomic_load(&ctx_blocks) == 0 && pending_timers() == 0);
    printf("context expire boundary: ok\n");
}

/* Context.after_func: the function runs on a new tzroutine once the context is cancelled (at once
 * when it has ended), never when stopped first; a function that never ran is dropped by the drop
 * of its handle or the end of its boundary. A dormant function is no pending timer. */
static void test_context_after(void) {
    atomic_store(&func_runs, 0);
    atomic_store(&func_drops, 0);
    void *scope = tsuzuri_tz_scope_enter();
    void *root = ctx_new(NULL, INT64_MAX);
    void *ctx = ctx_new(root, INT64_MAX);
    void *after = tsuzuri_tz_ctx_after(ctx, timer_block(), timer_block_free, func_run, func_drop, &func_runs);
    void *stopped = tsuzuri_tz_ctx_after(ctx, timer_block(), timer_block_free, func_run, func_drop, &func_runs);
    CHECK(pending_timers() == 0);
    CHECK(tsuzuri_tz_ctx_stop_after(stopped) == 1 && tsuzuri_tz_ctx_stop_after(stopped) == 0);
    tsuzuri_tz_ctx_cancel(ctx);
    while (atomic_load(&func_runs) == 0) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    CHECK(tsuzuri_tz_ctx_stop_after(after) == 0);
    void *ended = tsuzuri_tz_ctx_after(ctx, timer_block(), timer_block_free, func_run, func_drop, &func_runs);
    while (atomic_load(&func_runs) == 1) CHECK(tsuzuri_tz_sleep(1000000) == 0);
    tsuzuri_tz_ctx_free_after(after);
    tsuzuri_tz_ctx_free_after(stopped);
    tsuzuri_tz_ctx_free_after(ended);
    CHECK(atomic_load(&func_runs) == 2 && atomic_load(&func_drops) == 1);
    void *waiting = tsuzuri_tz_ctx_after(root, timer_block(), timer_block_free, func_run, func_drop, &func_runs);
    tsuzuri_tz_ctx_release(ctx);
    tsuzuri_tz_boundary_leave(scope);
    CHECK(atomic_load(&func_runs) == 2 && atomic_load(&func_drops) == 2);
    tsuzuri_tz_ctx_cancel(root); /* after the boundary: the function was dropped there */
    tsuzuri_tz_ctx_free_after(waiting);
    tsuzuri_tz_ctx_release(root);
    CHECK(atomic_load(&func_runs) == 2 && atomic_load(&func_drops) == 2);
    CHECK(atomic_load(&ctx_blocks) == 0 && atomic_load(&timer_blocks) == 0 && pending_timers() == 0);
    printf("context after: ok\n");
}

/* ---------------------------------------------------------------- Parallel and Routine.scope (TZ11) */

/* tsuzuri_tz_parallel calls every index once, from a pseudo-G and from a tzroutine, with any number
 * of Ps; a nested call inside a callback works on its own; inside a trap boundary (TZ12) the caller
 * calls every index itself, in order, and no helper tzroutine starts. */
enum { CLAIMS_MAX = 10000 };
static _Atomic int64_t claims[CLAIMS_MAX];
static void claim(void *ctx, uint64_t i) {
    CHECK(ctx == claims && i < CLAIMS_MAX);
    atomic_fetch_add(&claims[i], 1);
}
static void check_claims(uint64_t n) {
    for (uint64_t i = 0; i < CLAIMS_MAX; i++) CHECK(atomic_load(&claims[i]) == (i < n ? 1 : 0));
    for (uint64_t i = 0; i < CLAIMS_MAX; i++) atomic_store(&claims[i], 0);
}
static void claims_routine(void *env) {
    uint64_t n = (uint64_t)(uintptr_t)env;
    CHECK(tsuzuri_tz_parallel(claim, claims, n) == 0);
    check_claims(n);
}
static _Atomic int64_t nested_total;
static void nested_inner(void *ctx, uint64_t i) {
    (void)ctx;
    atomic_fetch_add(&nested_total, (int64_t)i + 1);
}
static void nested_outer(void *ctx, uint64_t i) {
    (void)ctx;
    (void)i;
    CHECK(tsuzuri_tz_parallel(nested_inner, NULL, 100) == 0);
}
#if !defined(_WIN32)
static void *test_owner(void) { return &nested_total; }
static _Atomic int64_t ordered_next;
static pthread_t ordered_thread;
/* Inside a trap boundary every index runs on the calling thread, in order (no helper). */
static void ordered(void *ctx, uint64_t i) {
    CHECK(ctx == claims && (int64_t)i == atomic_fetch_add(&ordered_next, 1));
    CHECK(pthread_equal(pthread_self(), ordered_thread));
    claim(ctx, i);
}
#endif
static void test_parallel(void) {
    uint64_t sizes[] = {0, 1, 7, 10000};
    for (int k = 0; k < 4; k++) {
        CHECK(tsuzuri_tz_parallel(claim, claims, sizes[k]) == 0);
        check_claims(sizes[k]);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(claims_routine, (void *)(uintptr_t)sizes[k]);
        join(scope);
    }
    atomic_store(&nested_total, 0);
    CHECK(tsuzuri_tz_parallel(nested_outer, NULL, 16) == 0);
    CHECK(atomic_load(&nested_total) == 16 * 5050);
#if !defined(_WIN32)
    tsuzuri_trap_hooks.owner = test_owner;
    atomic_store(&ordered_next, 0);
    ordered_thread = pthread_self();
    CHECK(tsuzuri_tz_parallel(ordered, claims, 50) == 0);
    CHECK(atomic_load(&ordered_next) == 50);
    tsuzuri_trap_hooks.owner = NULL;
    check_claims(50);
#endif
    printf("parallel: ok\n");
}

/* Routine.scope: every child runs once on its own tzroutine and may wait for the others: 64
 * children pass a value around a ring of channels, which needs them all at once (with one P too). */
enum { RING = 64 };
static tzr_chan *ring[RING];
static int64_t ring_seen[RING];
static void ring_child(void *ctx, uint64_t i) {
    CHECK(ctx == ring);
    int64_t v = 0;
    if (i == 0) {
        v = 1;
        CHECK(tsuzuri_tz_chan_send(ring[1], &v, 1) == TZR_OK);
        CHECK(tsuzuri_tz_chan_recv(ring[0], &v, 1) == TZR_OK);
    } else {
        CHECK(tsuzuri_tz_chan_recv(ring[i], &v, 1) == TZR_OK);
        v++;
        CHECK(tsuzuri_tz_chan_send(ring[(i + 1) % RING], &v, 1) == TZR_OK);
    }
    ring_seen[i] = v;
}
static void scope_spawner(void *ctx, uint64_t i) {
    (void)ctx;
    tsuzuri_tz_go(incr, (void *)(uintptr_t)(i + 1));
}
static void test_scope_run(void) {
    for (int i = 0; i < RING; i++) ring[i] = chan_make(sizeof(int64_t), 0);
    CHECK(tsuzuri_tz_scope_run(ring_child, ring, RING) == 0);
    CHECK(ring_seen[0] == RING);
    for (int i = 1; i < RING; i++) CHECK(ring_seen[i] == i + 1);
    for (int i = 0; i < RING; i++) {
        CHECK(tsuzuri_tz_chan_release(ring[i]) == 1);
        free(ring[i]);
    }
    CHECK(tsuzuri_tz_scope_run(ring_child, ring, 0) == 0);
    /* the call waits for what the children spawned too */
    atomic_store(&counter, 0);
    CHECK(tsuzuri_tz_scope_run(scope_spawner, NULL, 100) == 0);
    CHECK(atomic_load(&counter) == 5050);
    printf("scope run: ok\n");
}

/* 30. blocking calls (TZ08): a G in a blocking call lets sysmon give its P away, takes a P again
 * after the call, keeps the deadlock detector from judging while it is in the call, and stays on its
 * thread in a callback of the call and in Routine.with_locked_thread. */
static void block_ms(int ms) {
#if defined(_WIN32)
    Sleep((DWORD)ms);
#else
    struct timespec t = {ms / 1000, (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0) {
    }
#endif
}
#if defined(_WIN32)
typedef DWORD thread_id;
static thread_id this_thread(void) { return GetCurrentThreadId(); }
static int same_thread(thread_id a, thread_id b) { return a == b; }
#else
typedef pthread_t thread_id;
static thread_id this_thread(void) { return pthread_self(); }
static int same_thread(thread_id a, thread_id b) { return pthread_equal(a, b); }
#endif
static _Atomic int64_t handoff_order;
static _Atomic int64_t handoff_slept_at, handoff_rounds_at;
static _Atomic int handoff_inside, handoff_released;
/* Stays in its blocking call until `flag` is set, with a watchdog that fails a hang. */
static void block_until(_Atomic int *flag) {
    double t = seconds();
    while (!atomic_load(flag)) {
        CHECK(seconds() - t < 60.0);
        block_ms(1);
    }
}
static void handoff_sleeper(void *env) {
    (void)env;
    tsuzuri_tz_blocking_enter();
    atomic_store(&handoff_inside, 1);
    block_until(&handoff_released); /* the round trips must run meanwhile, on the P that it gave up */
    tsuzuri_tz_blocking_exit();
    atomic_store(&handoff_slept_at, atomic_fetch_add(&handoff_order, 1) + 1);
}
static void handoff_pinger(void *env) {
    pingpong *pp = env;
    pinger(pp);
    atomic_store(&handoff_rounds_at, atomic_fetch_add(&handoff_order, 1) + 1);
    atomic_store(&handoff_released, 1);
}
static void test_blocking_handoff(void) {
    pingpong pp = {chan_make(8, 0), chan_make(8, 0), 1000};
    atomic_store(&handoff_order, 0);
    atomic_store(&handoff_inside, 0);
    atomic_store(&handoff_released, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(handoff_sleeper, NULL);
    while (!atomic_load(&handoff_inside)) tzr_os_yield();
    tsuzuri_tz_go(ponger, &pp);
    tsuzuri_tz_go(handoff_pinger, &pp);
    join(scope);
    /* the 1,000 round trips ran on the P that the sleeper's call gave up, even with one P */
    CHECK(atomic_load(&handoff_rounds_at) == 1 && atomic_load(&handoff_slept_at) == 2);
    free(pp.ping);
    free(pp.pong);
    printf("blocking handoff: ok\n");
}

static _Atomic int64_t reacquire_rounds;
static _Atomic int reacquire_stop;
static _Atomic int64_t reacquire_seen[2];
static void reacquire_spinner(void *env) {
    (void)env;
    while (!atomic_load(&reacquire_stop)) {
        atomic_fetch_add(&reacquire_rounds, 1);
        tsuzuri_tz_yield();
    }
}
static void reacquire_blocker(void *env) {
    int which = (int)(intptr_t)env;
    for (int i = 0; i < 4; i++) {
        int64_t before = atomic_load(&reacquire_rounds);
        tsuzuri_tz_blocking_enter();
        /* the spinner runs while this G is in its call: it waits until it has (a watchdog fails a hang) */
        double t = seconds();
        while (atomic_load(&reacquire_rounds) == before) {
            CHECK(seconds() - t < 60.0);
            block_ms(1);
        }
        tsuzuri_tz_blocking_exit();
        /* back with a P: the G goes on here */
        atomic_fetch_add(&reacquire_seen[which], atomic_load(&reacquire_rounds) - before);
        tsuzuri_tz_yield();
    }
}
static void reacquire_root(void *env) {
    (void)env;
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(reacquire_blocker, (void *)(intptr_t)0);
    tsuzuri_tz_go(reacquire_blocker, (void *)(intptr_t)1);
    CHECK(tsuzuri_tz_scope_leave(scope) == 0);
    atomic_store(&reacquire_stop, 1);
}
static void test_blocking_reacquire(void) {
    atomic_store(&reacquire_rounds, 0);
    atomic_store(&reacquire_stop, 0);
    atomic_store(&reacquire_seen[0], 0);
    atomic_store(&reacquire_seen[1], 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(reacquire_spinner, NULL);
    tsuzuri_tz_go(reacquire_root, NULL);
    join(scope);
    /* the spinner ran while each blocker was in its calls */
    CHECK(atomic_load(&reacquire_seen[0]) > 0 && atomic_load(&reacquire_seen[1]) > 0);
    CHECK(atomic_load(&tzr_sched.nmsys) == 0);
    printf("blocking reacquire: ok\n");
}

/* A G in a blocking call may wake the others when it returns: everybody else waiting is no deadlock. */
static void late_sender(void *env) {
    tsuzuri_tz_blocking_enter();
    block_ms(100);
    tsuzuri_tz_blocking_exit();
    int64_t v = 5;
    CHECK(tsuzuri_tz_chan_send(env, &v, 1) == TZR_OK);
}
static void early_receiver(void *env) {
    int64_t v = 0;
    CHECK(tsuzuri_tz_chan_recv(env, &v, 1) == TZR_OK && v == 5);
}
static void test_blocking_deadlock(void) {
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(early_receiver, c);
    tsuzuri_tz_go(early_receiver, c);
    tsuzuri_tz_go(late_sender, c);
    tsuzuri_tz_go(late_sender, c);
    join(scope);
    free(c);
    printf("blocking deadlock: ok\n");
}

/* A callback of a blocking call runs on the G with a P again, and a wait in it keeps the G on the
 * thread that has the host's frames; the call goes on after the callback. */
static _Atomic int64_t callback_got;
static void callback_body(tzr_chan *c) {
    thread_id before = this_thread();
    int32_t depth = tsuzuri_tz_callback_enter();
    CHECK(depth == 1);
    for (int64_t i = 1; i <= 20; i++) {
        CHECK(tsuzuri_tz_chan_send(c, &i, 1) == TZR_OK); /* waits for the receiver */
        CHECK(tsuzuri_tz_sleep(100000) == 0);
        CHECK(same_thread(before, this_thread()));
    }
    tsuzuri_tz_callback_exit(depth);
}
static void callback_caller(void *env) {
    tsuzuri_tz_blocking_enter();
    block_ms(5);
    callback_body(env); /* the host calls back */
    block_ms(5);
    tsuzuri_tz_blocking_exit();
    tsuzuri_tz_yield();
}
static void callback_receiver(void *env) {
    int64_t v;
    for (int i = 0; i < 20; i++) {
        CHECK(tsuzuri_tz_chan_recv(env, &v, 1) == TZR_OK);
        atomic_fetch_add(&callback_got, v);
    }
}
static void test_blocking_callback(void) {
    tzr_chan *c = chan_make(8, 0);
    atomic_store(&callback_got, 0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) {
        tsuzuri_tz_go(callback_caller, c);
        tsuzuri_tz_go(callback_receiver, c);
    }
    join(scope);
    CHECK(atomic_load(&callback_got) == 4 * 210);
    /* a thread that is not an M, and a callback outside a call, do nothing */
    CHECK(tsuzuri_tz_callback_enter() == 0);
    tsuzuri_tz_callback_exit(0);
    free(c);
    printf("blocking callback: ok\n");
}

/* Routine.with_locked_thread: the G stays on one thread through yields, waits and sleeps, and
 * others still run (with one P, its waits give the P away). */
static _Atomic int64_t locked_moves;
static void locked_partner(void *env) {
    int64_t v;
    while (tsuzuri_tz_chan_recv(env, &v, 1) == TZR_OK && v >= 0) CHECK(tsuzuri_tz_chan_send(env, &v, 1) == TZR_OK);
}
static void locked_worker(void *env) {
    tzr_chan *c = env;
    tsuzuri_tz_lock_thread();
    thread_id home = this_thread();
    for (int64_t i = 0; i < 200; i++) {
        CHECK(tsuzuri_tz_chan_send(c, &i, 1) == TZR_OK);
        int64_t back = -2;
        CHECK(tsuzuri_tz_chan_recv(c, &back, 1) == TZR_OK && back == i);
        if (i % 10 == 0) CHECK(tsuzuri_tz_sleep(50000) == 0);
        tsuzuri_tz_yield();
        if (!same_thread(home, this_thread())) atomic_fetch_add(&locked_moves, 1);
    }
    tsuzuri_tz_unlock_thread();
    int64_t stop = -1;
    CHECK(tsuzuri_tz_chan_send(c, &stop, 1) == TZR_OK);
}
/* A locked G that yields until a G back from a blocking call sets a flag: with one P its yield gives the
 * P to that G's M, which waits for it, or the two would wait for each other for ever. */
static _Atomic int locked_flag, blocked_inside;
static void locked_yielder(void *env) {
    (void)env;
    tsuzuri_tz_lock_thread();
    double t = seconds();
    while (!atomic_load(&locked_flag)) {
        CHECK(seconds() - t < 60.0);
        tsuzuri_tz_yield();
    }
    tsuzuri_tz_unlock_thread();
}
static void flag_after_call(void *env) {
    (void)env;
    tsuzuri_tz_blocking_enter();
    atomic_store(&blocked_inside, 1);
    block_ms(20);
    tsuzuri_tz_blocking_exit();
    atomic_store(&locked_flag, 1);
}
static void test_locked_yield(void) {
    for (int round = 0; round < 5; round++) {
        atomic_store(&locked_flag, 0);
        atomic_store(&blocked_inside, 0);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(flag_after_call, NULL);
        while (!atomic_load(&blocked_inside)) tzr_os_yield();
        tsuzuri_tz_go(locked_yielder, NULL);
        join(scope);
    }
    printf("locked yield: ok\n");
}

/* Two locked Gs that yield until a queued G sets the flag: with one P too, the queued G runs at the
 * next yield; the two do not hand the P only to each other (which let it run only now and then). */
static _Atomic int locked_yielding, setter_queued;
static _Atomic int64_t yields_after_queued;
static void locked_counting_yielder(void *env) {
    (void)env;
    atomic_fetch_add(&locked_yielding, 1);
    tsuzuri_tz_lock_thread();
    while (!atomic_load(&locked_flag)) {
        if (atomic_load(&setter_queued)) atomic_fetch_add(&yields_after_queued, 1);
        tsuzuri_tz_yield();
    }
    tsuzuri_tz_unlock_thread();
}
static void flag_setter(void *env) {
    (void)env;
    atomic_store(&locked_flag, 1);
}
static void test_locked_yielders(void) {
    for (int round = 0; round < 5; round++) {
        atomic_store(&locked_flag, 0);
        atomic_store(&locked_yielding, 0);
        atomic_store(&setter_queued, 0);
        atomic_store(&yields_after_queued, 0);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(locked_counting_yielder, NULL);
        tsuzuri_tz_go(locked_counting_yielder, NULL);
        while (atomic_load(&locked_yielding) < 2) tzr_os_yield();
        tsuzuri_tz_go(flag_setter, NULL);
        atomic_store(&setter_queued, 1);
        join(scope);
        /* a few yields of each until the M that runs the queue gets the P (thousands before) */
        CHECK(atomic_load(&yields_after_queued) < 100);
    }
    printf("locked yielders: ok\n");
}

static void test_locked_thread(void) {
    atomic_store(&locked_moves, 0);
    tzr_chan *c[3];
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 3; i++) {
        c[i] = chan_make(8, 0);
        tsuzuri_tz_go(locked_worker, c[i]);
        tsuzuri_tz_go(locked_partner, c[i]);
    }
    join(scope);
    CHECK(atomic_load(&locked_moves) == 0);
    CHECK(tzr_sched.nmlocked == 0);
    for (int i = 0; i < 3; i++) free(c[i]);
    tsuzuri_tz_lock_thread(); /* a thread that is not an M: nothing */
    tsuzuri_tz_unlock_thread();
    printf("locked thread: ok\n");
}

/* 31. the netpoller (TZ08 Phases 2 and 3): a G that waits for a socket waits alone, wakes when its
 * socket is ready, at its deadline, or when the socket is closed. A pair of connected sockets: a Unix
 * socket pair, or two loopback TCP sockets on Windows. */
#if defined(_WIN32)
typedef SOCKET sock;
static void nonblocking_pair(sock fds[2]) {
    static int started;
    if (!started) {
        WSADATA data;
        CHECK(WSAStartup(MAKEWORD(2, 2), &data) == 0);
        started = 1;
    }
    sock listener = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address;
    int length = sizeof address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    CHECK(listener != INVALID_SOCKET && bind(listener, (struct sockaddr *)&address, sizeof address) == 0);
    CHECK(getsockname(listener, (struct sockaddr *)&address, &length) == 0 && listen(listener, 1) == 0);
    fds[1] = socket(AF_INET, SOCK_STREAM, 0);
    CHECK(connect(fds[1], (struct sockaddr *)&address, sizeof address) == 0);
    fds[0] = accept(listener, NULL, NULL);
    CHECK(fds[0] != INVALID_SOCKET);
    closesocket(listener);
    for (int i = 0; i < 2; i++) {
        u_long enable = 1;
        CHECK(ioctlsocket(fds[i], FIONBIO, &enable) == 0);
    }
}
static int sock_read(sock fd, char *byte) { return recv(fd, byte, 1, 0); }
static int sock_write(sock fd, const char *byte) { return send(fd, byte, 1, 0); }
static int sock_would_block(void) { return WSAGetLastError() == WSAEWOULDBLOCK; }
static void sock_close(sock fd) { closesocket(fd); }
#else
typedef int sock;
static void nonblocking_pair(sock fds[2]) {
    CHECK(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
    for (int i = 0; i < 2; i++) CHECK(fcntl(fds[i], F_SETFL, fcntl(fds[i], F_GETFL) | O_NONBLOCK) == 0);
}
static int sock_read(sock fd, char *byte) { return (int)read(fd, byte, 1); }
static int sock_write(sock fd, const char *byte) { return (int)write(fd, byte, 1); }
static int sock_would_block(void) { return errno == EAGAIN || errno == EWOULDBLOCK; }
static void sock_close(sock fd) { close(fd); }
#endif
/* Reads one byte as net.c does: try, and wait in the netpoller while there is none. */
static int read_byte(sock fd, int64_t timeout, char *byte) {
    for (;;) {
        int n = sock_read(fd, byte);
        if (n == 1) return 0;
        if (n == 0) return 2;
        CHECK(sock_would_block());
        int32_t waited = tzr_netpoll_wait((intptr_t)fd, 0, timeout, NULL, 0);
        CHECK(waited == 0 || waited == 1);
        if (waited == 1) return 1;
    }
}
/* After the waits: no G waits in the netpoller, and no timer is pending. A NET timer that fired is
 * counted out by the timer thread after it lets its lock go, and the G that it woke may have left
 * before that, so the count may still fall for a moment. */
static void netpoll_settled(void) {
    CHECK(atomic_load(&tzr_sched.nnetwait) == 0);
    double t = seconds();
    while (atomic_load(&tzr_sched.ntimers) != 0) {
        CHECK(seconds() - t < 10.0);
        tzr_os_yield();
    }
}
static sock netpoll_fds[100][2];
static _Atomic int64_t netpoll_turn;
static int64_t netpoll_woke[100];
static void netpoll_reader(void *env) {
    int i = (int)(intptr_t)env;
    char byte;
    CHECK(read_byte(netpoll_fds[i][0], -1, &byte) == 0 && byte == (char)i);
    netpoll_woke[i] = atomic_fetch_add(&netpoll_turn, 1);
}
static void netpoll_writer(void *env) {
    (void)env;
    for (int i = 99; i >= 0; i--) {
        char byte = (char)i;
        int64_t before = atomic_load(&netpoll_turn);
        CHECK(sock_write(netpoll_fds[i][1], &byte) == 1);
        while (atomic_load(&netpoll_turn) == before) tsuzuri_tz_yield();
    }
}
static void test_netpoll_pipe(void) {
    for (int i = 0; i < 100; i++) nonblocking_pair(netpoll_fds[i]);
    atomic_store(&netpoll_turn, 0);
    tzr_mutex_lock(&tzr_sched.lock);
    int ms = tzr_sched.mcount; /* the cases before may have left idle Ms */
    tzr_mutex_unlock(&tzr_sched.lock);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 100; i++) tsuzuri_tz_go(netpoll_reader, (void *)(intptr_t)i);
    /* every reader waits in the netpoller before the writer starts */
    while (atomic_load(&tzr_sched.nnetwait) < 100) tzr_os_yield();
    /* they wait without their Ms: a blocking wait would hold one M each */
    tzr_mutex_lock(&tzr_sched.lock);
    ms = tzr_sched.mcount - ms;
    tzr_mutex_unlock(&tzr_sched.lock);
    CHECK(ms <= tzr_sched.nprocs + 4);
    tsuzuri_tz_go(netpoll_writer, NULL);
    join(scope);
    /* each reader woke when its descriptor had a byte: in the order of the writes */
    for (int i = 0; i < 100; i++) CHECK(netpoll_woke[i] == 99 - i);
    for (int i = 0; i < 100; i++) {
        sock_close(netpoll_fds[i][0]);
        sock_close(netpoll_fds[i][1]);
    }
    CHECK(atomic_load(&tzr_sched.nnetwait) == 0);
    printf("netpoll pipe: ok\n");
}

static _Atomic int64_t netpoll_ticks;
static _Atomic int netpoll_stop;
static void netpoll_ticker(void *env) {
    (void)env;
    while (!atomic_load(&netpoll_stop)) {
        atomic_fetch_add(&netpoll_ticks, 1);
        CHECK(tsuzuri_tz_sleep(1000000) == 0);
    }
}
static void netpoll_timeout_reader(void *env) {
    sock *fds = env;
    char byte;
    double t = seconds();
    int64_t ticks = atomic_load(&netpoll_ticks);
    CHECK(read_byte(fds[0], 30000000, &byte) == 1); /* nobody writes: the deadline */
    CHECK(seconds() - t >= 0.029);
    CHECK(atomic_load(&netpoll_ticks) > ticks); /* the other tzroutine ran meanwhile */
    /* ready before its deadline: the timer goes */
    CHECK(sock_write(fds[1], "x") == 1);
    CHECK(read_byte(fds[0], 5000000000, &byte) == 0 && byte == 'x');
    atomic_store(&netpoll_stop, 1);
}
static void test_netpoll_timeout(void) {
    sock fds[2];
    nonblocking_pair(fds);
    atomic_store(&netpoll_stop, 0);
    atomic_store(&netpoll_ticks, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(netpoll_ticker, NULL);
    tsuzuri_tz_go(netpoll_timeout_reader, fds);
    join(scope);
    sock_close(fds[0]);
    sock_close(fds[1]);
    netpoll_settled();
    printf("netpoll timeout: ok\n");
}

/* net.c's table of open handles, for one handle (7): open while `netpoll_open_handle` says so. */
static _Atomic int netpoll_open_handle;
static int32_t netpoll_open(int64_t handle, intptr_t descriptor) {
    (void)descriptor;
    return handle == 7 && atomic_load(&netpoll_open_handle);
}
static void netpoll_close_reader(void *env) {
    sock fd = *(sock *)env;
    /* the close of its socket wakes it as ready */
    CHECK(tzr_netpoll_wait((intptr_t)fd, 0, -1, netpoll_open, 7) == 0);
}
static void netpoll_closed_reader(void *env) {
    sock fd = *(sock *)env;
    /* a reader that got EAGAIN before its handle was closed and its descriptor taken by another
     * socket: the wait does not start on the other socket, it is ready (net.c reports the close) */
    CHECK(tzr_netpoll_wait((intptr_t)fd, 0, -1, netpoll_open, 7) == 0 && atomic_load(&tzr_sched.nnetwait) == 0);
}
static void netpoll_reused_reader(void *env) {
    sock fd = *(sock *)env;
    char byte;
    for (;;) {
        int n = sock_read(fd, &byte);
        if (n == 1) break;
        CHECK(n < 0 && sock_would_block());
        CHECK(tzr_netpoll_wait((intptr_t)fd, 0, -1, netpoll_open, 7) == 0);
    }
    CHECK(byte == 'r');
}
static void test_netpoll_close(void) {
    for (int round = 0; round < 20; round++) {
        sock fds[2];
        nonblocking_pair(fds);
        atomic_store(&netpoll_open_handle, 1);
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(netpoll_close_reader, &fds[0]);
        while (atomic_load(&tzr_sched.nnetwait) == 0) tzr_os_yield();
        /* net.c's close: it forgets the handle, calls the hook, then closes the descriptor */
        atomic_store(&netpoll_open_handle, 0);
        tzr_netpoll_close((intptr_t)fds[0]);
        join(scope);
        sock_close(fds[0]);
        sock_close(fds[1]);
        sock again[2];
        nonblocking_pair(again); /* usually takes the same descriptors */
        scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(netpoll_closed_reader, &again[0]);
        join(scope);
        /* the socket that took the descriptor waits as any other: nothing of the close is left */
        atomic_store(&netpoll_open_handle, 1);
        scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(netpoll_reused_reader, &again[0]);
        while (atomic_load(&tzr_sched.nnetwait) == 0) tzr_os_yield();
        CHECK(sock_write(again[1], "r") == 1);
        join(scope);
        sock_close(again[0]);
        sock_close(again[1]);
    }
    printf("netpoll close: ok\n");
}

/* Readiness and deadlines that fall together: each wait is settled once, by whichever comes first. */
typedef struct { sock fds[2]; int64_t timeout; } netpoll_racer;
static _Atomic int64_t netpoll_outcomes[3];
static void netpoll_race_reader(void *env) {
    netpoll_racer *r = env;
    char byte;
    int outcome = read_byte(r->fds[0], r->timeout, &byte);
    atomic_fetch_add(&netpoll_outcomes[outcome], 1);
    if (outcome == 1) CHECK(read_byte(r->fds[0], -1, &byte) == 0); /* the byte comes anyway */
}
static void netpoll_race_writer(void *env) {
    netpoll_racer *r = env;
    for (int spin = (int)(r->timeout / 1000); spin > 0; spin--) tsuzuri_tz_yield();
    CHECK(sock_write(r->fds[1], "y") == 1);
}
static void test_netpoll_race(void) {
    enum { N = 64 };
    netpoll_racer racers[N];
    for (int i = 0; i < 3; i++) atomic_store(&netpoll_outcomes[i], 0);
    for (int round = 0; round < 20; round++) {
        void *scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < N; i++) {
            nonblocking_pair(racers[i].fds);
            racers[i].timeout = (int64_t)(i % 8) * 25000;
            tsuzuri_tz_go(netpoll_race_reader, &racers[i]);
            tsuzuri_tz_go(netpoll_race_writer, &racers[i]);
        }
        join(scope);
        for (int i = 0; i < N; i++) {
            sock_close(racers[i].fds[0]);
            sock_close(racers[i].fds[1]);
        }
    }
    CHECK(atomic_load(&netpoll_outcomes[0]) + atomic_load(&netpoll_outcomes[1]) == 20 * N);
    netpoll_settled();
    printf("netpoll race: %lld ready, %lld timed out\n", (long long)atomic_load(&netpoll_outcomes[0]),
           (long long)atomic_load(&netpoll_outcomes[1]));
}

/* 32. DataChannel on the scheduler (TZ12): the block that the generated code makes (the header of ten
 * words, the lock and the two queues zero), its ring, its close by either end, its waits (a waiter
 * stops its G or its thread only), the deadlock verdict, and a wait while holding a Mutex. */
static struct tzr_dchan *dchan_make(uint64_t capacity, uint64_t item_size) {
    struct tzr_dchan *c = malloc(sizeof *c + capacity * (item_size ? item_size : 1));
    memset(c, 0xAB, sizeof *c + capacity * (item_size ? item_size : 1)); /* not zeroed, as tz.alloc */
    c->capacity = capacity;
    c->item_size = item_size;
    c->head = c->count = 0;
    c->senders = c->receivers = 1;
    c->owners = 2;
    memset(&c->lock, 0, sizeof c->lock);
    c->waiters[0] = c->waiters[1] = NULL;
    return c;
}
static void test_dchan_ring(void) {
    struct tzr_dchan *c = dchan_make(3, 8);
    for (int64_t round = 0; round < 10; round++) {
        for (int64_t i = 0; i < 3; i++) {
            int64_t v = round * 10 + i;
            CHECK(tsuzuri_channel_send(c, &v) == 0);
        }
        for (int64_t i = 0; i < 3; i++) {
            int64_t v = -1;
            CHECK(tsuzuri_channel_recv(c, &v) == 0 && v == round * 10 + i);
        }
    }
    /* zero-sized items count and move nothing */
    struct tzr_dchan *units = dchan_make(2, 0);
    CHECK(tsuzuri_channel_send(units, NULL) == 0 && tsuzuri_channel_send(units, NULL) == 0);
    CHECK(units->count == 2 && tsuzuri_channel_recv(units, NULL) == 0 && units->count == 1);
    /* the last sender closes it for the receivers once it is empty, and the last owner frees it */
    int64_t v = 7;
    CHECK(tsuzuri_channel_send(c, &v) == 0);
    tsuzuri_channel_clone_sender(c);
    CHECK(c->senders == 2 && c->owners == 3);
    CHECK(tsuzuri_channel_close(c, 0) == 0);
    CHECK(tsuzuri_channel_close(c, 0) == 0);
    CHECK(tsuzuri_channel_recv(c, &v) == 0 && v == 7);
    CHECK(tsuzuri_channel_recv(c, &v) == 1);
    CHECK(tsuzuri_channel_close(c, 1) == 1);
    /* without a receiver, a send keeps its item */
    struct tzr_dchan *d = dchan_make(1, 8);
    CHECK(tsuzuri_channel_close(d, 1) == 0);
    CHECK(tsuzuri_channel_send(d, &v) == 1);
    CHECK(tsuzuri_channel_close(d, 0) == 1);
    free(c);
    free(d);
    free(units);
    printf("DataChannel ring: ok\n");
}

typedef struct { struct tzr_dchan *c; int64_t count; _Atomic int64_t *sum; } dchan_job;
static void dchan_sender(void *env) {
    dchan_job *job = env;
    for (int64_t i = 1; i <= job->count; i++) CHECK(tsuzuri_channel_send(job->c, &i) == 0);
}
static void dchan_receiver(void *env) {
    dchan_job *job = env;
    int64_t v, local = 0;
    for (int64_t i = 0; i < job->count; i++) {
        CHECK(tsuzuri_channel_recv(job->c, &v) == 0);
        local += v;
    }
    atomic_fetch_add(job->sum, local);
}
/* Waits on both sides, with any number of Ps: the waiters are tzroutines, which wait without their
 * threads, so more of them than threads complete (with one P too). */
static void test_dchan_waits(void) {
    for (uint64_t capacity = 1; capacity <= 8; capacity *= 8) {
        struct tzr_dchan *c = dchan_make(capacity, 8);
        _Atomic int64_t sum = 0;
        dchan_job job = {c, 5000, &sum};
        void *scope = tsuzuri_tz_scope_enter();
        for (int i = 0; i < 16; i++) {
            tsuzuri_tz_go(dchan_sender, &job);
            tsuzuri_tz_go(dchan_receiver, &job);
        }
        join(scope);
        CHECK(atomic_load(&sum) == 16 * 5000 * 5001 / 2);
        free(c);
    }
    /* the thread that runs main waits too, as a pseudo-G */
    struct tzr_dchan *c = dchan_make(1, 8);
    _Atomic int64_t sum = 0;
    dchan_job job = {c, 3000, &sum};
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(dchan_sender, &job);
    dchan_receiver(&job);
    join(scope);
    CHECK(atomic_load(&sum) == 3000 * 3001 / 2);
    free(c);
    printf("DataChannel waits: ok\n");
}

/* The last sender wakes every receiver that waits for an item; the last receiver every sender that
 * waits for room, which keeps its item. */
static _Atomic int dchan_closed_seen;
static void dchan_waiting_receiver(void *env) {
    int64_t v;
    CHECK(tsuzuri_channel_recv(env, &v) == 1);
    atomic_fetch_add(&dchan_closed_seen, 1);
}
static void dchan_waiting_sender(void *env) {
    int64_t v = 9;
    CHECK(tsuzuri_channel_send(env, &v) == 1);
    atomic_fetch_add(&dchan_closed_seen, 1);
}
static int dchan_waiters(struct tzr_dchan *c, int side) {
    tzr_lock_acquire(&c->lock);
    int n = 0;
    for (tzr_dwaiter *w = c->waiters[side]; w; w = w->next) n++;
    tzr_lock_release(&c->lock);
    return n;
}
static void test_dchan_close(void) {
    struct tzr_dchan *c = dchan_make(1, 8);
    atomic_store(&dchan_closed_seen, 0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 3; i++) tsuzuri_tz_go(dchan_waiting_receiver, c);
    while (dchan_waiters(c, 0) < 3) tzr_os_yield();
    CHECK(tsuzuri_channel_close(c, 0) == 0);
    join(scope);
    CHECK(atomic_load(&dchan_closed_seen) == 3);
    CHECK(tsuzuri_channel_close(c, 1) == 1);
    free(c);
    struct tzr_dchan *d = dchan_make(1, 8);
    int64_t v = 1;
    CHECK(tsuzuri_channel_send(d, &v) == 0); /* full */
    atomic_store(&dchan_closed_seen, 0);
    scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 3; i++) tsuzuri_tz_go(dchan_waiting_sender, d);
    while (dchan_waiters(d, 1) < 3) tzr_os_yield();
    CHECK(tsuzuri_channel_close(d, 1) == 0);
    join(scope);
    CHECK(atomic_load(&dchan_closed_seen) == 3);
    CHECK(tsuzuri_channel_close(d, 0) == 1);
    free(d);
    printf("DataChannel close: ok\n");
}

/* A tzroutine may wait on a DataChannel while it holds a Mutex (the old pool's lock refused it). */
static _Atomic uint32_t dchan_mutex;
static void dchan_locked_receiver(void *env) {
    CHECK(tsuzuri_tz_mutex32_lock(&dchan_mutex) == 0);
    int64_t v = 0;
    CHECK(tsuzuri_channel_recv(env, &v) == 0 && v == 42);
    tsuzuri_tz_mutex32_unlock(&dchan_mutex);
}
static void dchan_late_sender(void *env) {
    CHECK(tsuzuri_tz_sleep(2000000) == 0);
    int64_t v = 42;
    CHECK(tsuzuri_channel_send(env, &v) == 0);
}
static void test_dchan_lock(void) {
    struct tzr_dchan *c = dchan_make(1, 8);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(dchan_locked_receiver, c);
    tsuzuri_tz_go(dchan_late_sender, c);
    join(scope);
    free(c);
    printf("DataChannel in a lock: ok\n");
}

#if !defined(_WIN32)
/* Everybody waits: the detector reports the DataChannel wait and the operation returns 2. */
static void dchan_deadlock_body(void) {
    start_watchdog();
    struct tzr_dchan *c = dchan_make(1, 8);
    int64_t v;
    _exit(tsuzuri_channel_recv(c, &v) == 2 ? 48 : 1);
}
#endif
static void test_dchan_deadlock(void) {
#if !defined(_WIN32)
    int s = run_child("child-dchan-deadlock");
    CHECK(exited_with(s, 48));
#endif
    printf("DataChannel deadlock: ok\n");
}

/* TZ10: a request of sysmon (made by hand here) makes the G that checks yield, takes itself back,
 * and leaves the global count at 0; a check without a request, or on a pseudo-G, does nothing. */
static void preempt_ask(tzr_p *p) { (void)tzr_preempt_ask(p); }
static _Atomic int preempt_flag_moved;
static void preempt_flag_body(void *env) {
    (void)env;
    for (int round = 0; round < 100; round++) {
        tzr_p *p = tzr_this_m()->p;
        tsuzuri_tz_preempt_check(); /* nobody asked (unless sysmon did, if the OS stopped the thread) */
        p = tzr_this_m()->p;
        uint32_t tick = atomic_load(&p->schedtick);
        preempt_ask(p);
        CHECK(atomic_load(&tsuzuri_tz_preempt_pending) >= 1);
        tsuzuri_tz_preempt_check(); /* yields: back on another P, or on p after another start there */
        tzr_p *now = tzr_this_m()->p;
        CHECK(now != p || atomic_load(&p->schedtick) != tick);
        atomic_fetch_add(&preempt_flag_moved, 1);
    }
}
static void test_preempt_flag(void) {
    atomic_store(&preempt_flag_moved, 0);
    tsuzuri_tz_preempt_check(); /* a pseudo-G */
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(preempt_flag_body, NULL);
    join(scope);
    CHECK(atomic_load(&preempt_flag_moved) == 400);
    CHECK(atomic_load(&tsuzuri_tz_preempt_pending) == 0);
    printf("preempt flag: ok\n");
}

/* TZ10: with every P running a G that never waits and checks only when the global count says so
 * (as the generated code does), a G that sleeps 10 ms at a time still wakes, 50 times in a row,
 * while they run: sysmon asks them to yield. Without the requests it would never run again (the
 * watchdog ends the case). Afterwards the count goes back to 0. Lower bounds only. */
static _Atomic int preempt_busy_stop;
static _Atomic int64_t preempt_busy_checks;
static void preempt_busy(void *env) {
    (void)env;
    uint64_t x = 1;
    while (!atomic_load_explicit(&preempt_busy_stop, memory_order_relaxed)) {
        for (int i = 0; i < 1000; i++) x = x * 6364136223846793005ULL + 1442695040888963407ULL;
        if (atomic_load_explicit(&tsuzuri_tz_preempt_pending, memory_order_relaxed)) {
            atomic_fetch_add(&preempt_busy_checks, 1);
            tsuzuri_tz_preempt_check();
        }
    }
    CHECK(x != 0);
}
static void preempt_ticker(void *env) {
    int64_t *wakes = env;
    for (int i = 0; i < 50; i++) {
        CHECK(tsuzuri_tz_sleep(10000000) == 0);
        (*wakes)++;
    }
    atomic_store(&preempt_busy_stop, 1);
}
static void test_preempt_sysmon(void) {
    start_watchdog();
    atomic_store(&preempt_busy_stop, 0);
    atomic_store(&preempt_busy_checks, 0);
    int64_t wakes = 0;
    void *scope = tsuzuri_tz_scope_enter();
    for (int64_t i = 0; i < tsuzuri_tz_max_procs(); i++) tsuzuri_tz_go(preempt_busy, NULL);
    tsuzuri_tz_go(preempt_ticker, &wakes);
    join(scope);
    CHECK(wakes == 50);
    CHECK(atomic_load(&preempt_busy_checks) > 0);
    for (int i = 0; i < 2000 && atomic_load(&tsuzuri_tz_preempt_pending) != 0; i++) block_ms(1);
    CHECK(atomic_load(&tsuzuri_tz_preempt_pending) == 0);
    printf("preempt sysmon: %lld checks\n", (long long)atomic_load(&preempt_busy_checks));
}

/* A request whose G ends without a check: when every P is idle sysmon clears it before it sleeps, or
 * the count would stay up and keep every check on its slow path. */
static void preempt_ask_and_leave(void *env) {
    (void)env;
    preempt_ask(tzr_this_m()->p);
}
static void test_preempt_idle(void) {
    for (int round = 0; round < 50; round++) {
        void *scope = tsuzuri_tz_scope_enter();
        tsuzuri_tz_go(preempt_ask_and_leave, NULL);
        join(scope);
        for (int i = 0; i < 2000 && atomic_load(&tsuzuri_tz_preempt_pending) != 0; i++) block_ms(1);
        CHECK(atomic_load(&tsuzuri_tz_preempt_pending) == 0);
    }
    printf("preempt idle: ok\n");
}

/* TZ10 Phase 2: functions of the section of generated code stand for what the compiler emits. A loop
 * without calls there is stopped by sysmon's signal; the same loop elsewhere (the runtime's code) is
 * not. The stopped G keeps every register: a computation with many live integer and floating-point
 * values gives the same result as on a thread that nobody stops. */
#if defined(TZR_ASYNC)
#if defined(__APPLE__)
#define GENERATED __attribute__((section("__TEXT,__tz_text,regular,pure_instructions"), noinline))
#else
#define GENERATED __attribute__((section("tz_text"), noinline))
#endif
static _Atomic int async_stop;
/* SIGURGs that reach the handler the program had before the runtime's (main installs it first): only
 * the ones that are not sysmon's. */
static _Atomic int urgent_foreign;
static void urgent_counted(int sig) {
    (void)sig;
    atomic_fetch_add(&urgent_foreign, 1);
}
GENERATED static uint64_t async_spin(_Atomic int *stop) {
    uint64_t x = 1;
    while (!atomic_load_explicit(stop, memory_order_relaxed)) x = x * 6364136223846793005ULL + 1442695040888963407ULL;
    return x;
}
static void async_spinner(void *env) {
    (void)env;
    CHECK(async_spin(&async_stop) != 0);
}
static void async_ticker(void *env) {
    int64_t *wakes = env;
    for (int i = 0; i < 20; i++) {
        CHECK(tsuzuri_tz_sleep(10000000) == 0);
        (*wakes)++;
    }
    atomic_store(&async_stop, 1);
}
static void test_preempt_async(void) {
    start_watchdog();
    atomic_store(&async_stop, 0);
    uint64_t before = atomic_load(&tzr_async.yielded);
    int64_t wakes = 0;
    void *scope = tsuzuri_tz_scope_enter();
    for (int64_t i = 0; i < tsuzuri_tz_max_procs(); i++) tsuzuri_tz_go(async_spinner, NULL);
    tsuzuri_tz_go(async_ticker, &wakes);
    join(scope);
    CHECK(wakes == 20);
    CHECK(atomic_load(&tzr_async.yielded) > before);
    printf("preempt async: %llu stops\n", (unsigned long long)(atomic_load(&tzr_async.yielded) - before));
}

/* Integer and floating-point accumulators that stay in registers across the whole loop. */
GENERATED static void async_mix(int64_t rounds, uint64_t *ints, double *floats) {
    uint64_t a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8;
    double p = 1.0, q = 0.5, r = 0.25, s = 0.125, t = 3.0, u = 5.0, v = 7.0, w = 11.0;
    for (int64_t i = 0; i < rounds; i++) {
        a = a * 6364136223846793005ULL + 1442695040888963407ULL;
        b ^= a >> 7;
        c += b * 3;
        d = (d << 1) ^ c;
        e += d >> 3;
        f ^= e * 5;
        g += f >> 11;
        h ^= g + a;
        p = p * 0.999999 + (double)(a & 1023) * 1e-6;
        q = q * 0.999998 + (double)(b & 511) * 1e-6;
        r = r + p * 1e-9 - q * 1e-9;
        s = s * 0.5 + r;
        t = t + s * 1e-12;
        u = u * 1.0000001 - t * 1e-15;
        v = v + (double)(h & 7);
        w = w - (double)(g & 3) * 0.5;
    }
    ints[0] = a ^ b ^ c ^ d;
    ints[1] = e ^ f ^ g ^ h;
    floats[0] = p + q + r + s;
    floats[1] = t + u + v + w;
}
typedef struct { uint64_t ints[2]; double floats[2]; } async_mixed;
static void async_mixer(void *env) {
    async_mixed *out = env;
    async_mix(40000000, out->ints, out->floats);
}
static void test_preempt_async_values(void) {
    async_mixed expected;
    async_mix(40000000, expected.ints, expected.floats); /* on the main thread: never stopped */
    uint64_t before = atomic_load(&tzr_async.yielded);
    enum { N = 6 };
    async_mixed results[N];
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < N; i++) tsuzuri_tz_go(async_mixer, &results[i]);
    join(scope);
    for (int i = 0; i < N; i++) {
        CHECK(results[i].ints[0] == expected.ints[0] && results[i].ints[1] == expected.ints[1]);
        CHECK(memcmp(results[i].floats, expected.floats, sizeof expected.floats) == 0);
    }
    /* the comparison holds only for a kernel that was stopped */
    CHECK(atomic_load(&tzr_async.yielded) > before);
    printf("preempt async values: %llu stops\n", (unsigned long long)(atomic_load(&tzr_async.yielded) - before));
}

/* The same loop in the runtime's code: sysmon's signals arrive (one more spinner than Ps waits for a
 * P, so sysmon asks), and none stops it. */
static uint64_t async_spin_runtime(int64_t until) {
    uint64_t x = 1;
    while (tzr_nanotime() < until)
        for (int i = 0; i < 1000; i++) x = x * 6364136223846793005ULL + 1442695040888963407ULL;
    return x;
}
static void async_runtime_spinner(void *env) {
    CHECK(async_spin_runtime(*(int64_t *)env) != 0);
}
static void test_preempt_async_runtime(void) {
    uint64_t stops = atomic_load(&tzr_async.stopped), signals = atomic_load(&tzr_async.signals);
    int foreign = atomic_load(&urgent_foreign);
    int64_t until = tzr_nanotime() + 300000000;
    void *scope = tsuzuri_tz_scope_enter();
    for (int64_t i = 0; i <= tsuzuri_tz_max_procs(); i++) tsuzuri_tz_go(async_runtime_spinner, &until);
    join(scope);
    CHECK(atomic_load(&tzr_async.signals) > signals);
    CHECK(atomic_load(&tzr_async.stopped) == stops);
    /* sysmon's signals that stopped nothing are not the program's; a SIGURG of its own still is */
    CHECK(atomic_load(&urgent_foreign) == foreign);
    raise(SIGURG);
    CHECK(atomic_load(&urgent_foreign) == foreign + 1);
    printf("preempt async runtime: %llu signals, no stop\n", (unsigned long long)(atomic_load(&tzr_async.signals) - signals));
}

/* A program that ignores SIGURG and left SA_SIGINFO in the flags: the runtime's handler passes a
 * SIGURG of the program's own to that disposition, which calls nothing (SIG_IGN shares the union
 * with sa_sigaction). */
static void urgent_nothing(void *env) { (void)env; }
static void urgent_ignored_body(void) {
    struct sigaction ignored;
    memset(&ignored, 0, sizeof ignored);
    ignored.sa_handler = SIG_IGN;
    ignored.sa_flags = SA_SIGINFO;
    sigemptyset(&ignored.sa_mask);
    sigaction(SIGURG, &ignored, NULL);
    void *scope = tsuzuri_tz_scope_enter(); /* the first spawn starts sysmon, which installs its handler */
    tsuzuri_tz_go(urgent_nothing, NULL);
    join(scope);
    for (int i = 0; i < 2000 && !atomic_load(&tzr_async.enabled); i++) {
        struct timespec t = {0, 1000000};
        nanosleep(&t, NULL);
    }
    if (!atomic_load(&tzr_async.enabled)) _exit(3);
    raise(SIGURG);
    _exit(48);
}
/* A handler of the program's own without SA_RESTART and with SIGUSR1 in its mask: the runtime passes
 * it a SIGURG with SIGUSR1 blocked, and a read that the SIGURG interrupts fails with EINTR, as before
 * the runtime's handler. */
static _Atomic int urgent_masked;
static void urgent_checked(int sig) {
    (void)sig;
    sigset_t now;
    pthread_sigmask(SIG_BLOCK, NULL, &now);
    if (sigismember(&now, SIGUSR1) == 1) atomic_store(&urgent_masked, 1);
    atomic_fetch_add(&urgent_foreign, 1);
}
static int urgent_pipe[2];
static void *urgent_poke(void *reader) {
    struct timespec t = {0, 50000000};
    nanosleep(&t, NULL);
    pthread_kill(*(pthread_t *)reader, SIGURG);
    t.tv_sec = 2;
    t.tv_nsec = 0;
    nanosleep(&t, NULL);
    char byte = 1;
    (void)!write(urgent_pipe[1], &byte, 1); /* ends a read that was restarted instead */
    return NULL;
}
static void urgent_forward_body(void) {
    struct sigaction checked;
    memset(&checked, 0, sizeof checked);
    checked.sa_handler = urgent_checked;
    sigemptyset(&checked.sa_mask);
    sigaddset(&checked.sa_mask, SIGUSR1);
    checked.sa_flags = 0;
    sigaction(SIGURG, &checked, NULL);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(urgent_nothing, NULL);
    join(scope);
    for (int i = 0; i < 2000 && !atomic_load(&tzr_async.enabled); i++) {
        struct timespec t = {0, 1000000};
        nanosleep(&t, NULL);
    }
    if (!atomic_load(&tzr_async.enabled) || pipe(urgent_pipe) != 0) _exit(3);
    pthread_t self = pthread_self(), poker;
    if (pthread_create(&poker, NULL, urgent_poke, &self) != 0) _exit(3);
    char byte;
    ssize_t n = read(urgent_pipe[0], &byte, 1);
    int interrupted = n < 0 && errno == EINTR;
    pthread_join(poker, NULL);
    _exit(interrupted && atomic_load(&urgent_foreign) == 1 && atomic_load(&urgent_masked) ? 49 : 4);
}
/* The runtime's handler installed and sysmon running: the first spawn starts both. */
static void urgent_runtime_ready(void) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(urgent_nothing, NULL);
    join(scope);
    for (int i = 0; i < 2000 && !atomic_load(&tzr_async.enabled); i++) {
        struct timespec t = {0, 1000000};
        nanosleep(&t, NULL);
    }
    if (!atomic_load(&tzr_async.enabled)) _exit(3);
}
static void urgent_install(void (*handler)(int), int flags) {
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_handler = handler;
    sigemptyset(&action.sa_mask);
    action.sa_flags = flags;
    sigaction(SIGURG, &action, NULL);
}

/* A SIGURG of the program's own between the publication of a resume frame and the signal that
 * tzr_async_entry sends itself goes to the program's handler, and resumes nothing. */
static _Atomic int urgent_window_ok;
static void urgent_window_g(void *env) {
    (void)env;
    tzr_m *m = tzr_this_m();
    _Alignas(64) unsigned char space[sizeof(tzr_async_frame) + 64];
    tzr_async_frame *frame = (tzr_async_frame *)space;
    memset(space, 0, sizeof space);
    frame->magic = TZR_ASYNC_MAGIC;
    atomic_store(&m->async_resume, frame);
    raise(SIGURG);
    atomic_store(&m->async_resume, NULL);
    atomic_store(&urgent_window_ok, frame->magic == TZR_ASYNC_MAGIC && atomic_load(&urgent_foreign) == 1);
}
static void urgent_window_body(void) {
    urgent_install(urgent_counted, SA_RESTART);
    urgent_runtime_ready();
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(urgent_window_g, NULL);
    join(scope);
    _exit(atomic_load(&urgent_window_ok) ? 50 : 4);
}

/* A handler of the program's own with SA_RESETHAND runs once, however many SIGURGs come. Linux only:
 * macOS's sigaction does not report SA_RESETHAND in the action that it returns. */
static void urgent_oneshot_body(void) {
    urgent_install(urgent_counted, SA_RESETHAND);
    urgent_runtime_ready();
    raise(SIGURG);
    raise(SIGURG);
    _exit(atomic_load(&urgent_foreign) == 1 ? 51 : 4);
}

/* A kick of sysmon that comes after the G entered a blocking call (sysmon saw it running just
 * before) sends nothing: a read in the call, under a handler without SA_RESTART, is not cut short. */
static tzr_p *urgent_kicked_p;
static _Atomic int urgent_reading;
static void *urgent_kicker(void *env) {
    (void)env;
    while (!atomic_load(&urgent_reading)) {
    }
    struct timespec t = {0, 50000000};
    nanosleep(&t, NULL);
    tzr_async_kick(urgent_kicked_p);
    t.tv_nsec = 200000000;
    nanosleep(&t, NULL);
    char byte = 1;
    (void)!write(urgent_pipe[1], &byte, 1);
    return NULL;
}
static _Atomic int urgent_read;
static void urgent_syscall_g(void *env) {
    (void)env;
    urgent_kicked_p = tzr_this_m()->p;
    tsuzuri_tz_blocking_enter();
    atomic_store(&urgent_reading, 1);
    char byte;
    ssize_t n = read(urgent_pipe[0], &byte, 1);
    tsuzuri_tz_blocking_exit();
    atomic_store(&urgent_read, (int)n);
}
static void urgent_syscall_body(void) {
    urgent_install(urgent_counted, 0);
    urgent_runtime_ready();
    if (pipe(urgent_pipe) != 0) _exit(3);
    pthread_t kicker;
    if (pthread_create(&kicker, NULL, urgent_kicker, NULL) != 0) _exit(3);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(urgent_syscall_g, NULL);
    join(scope);
    pthread_join(kicker, NULL);
    _exit(atomic_load(&urgent_read) == 1 && atomic_load(&urgent_foreign) == 0 ? 52 : 4);
}
/* The kick reads the gate open and is paused (sysmon descheduled) before it sends; meanwhile the G
 * enters a blocking call. It waits for the signal and its handler, however long the pause, so a read
 * in the call under a handler without SA_RESTART is not cut short. */
static _Atomic int urgent_pause_armed, urgent_pause_reached, urgent_kick_go;
static void urgent_kick_pause(void) {
    if (!atomic_load(&urgent_pause_armed)) return;
    atomic_store(&urgent_pause_reached, 1);
    struct timespec t = {0, 100000000};
    nanosleep(&t, NULL);
}
static void *urgent_paused_kicker(void *env) {
    (void)env;
    while (!atomic_load(&urgent_kick_go)) {
    }
    atomic_store(&urgent_pause_armed, 1);
    tzr_async_kick(urgent_kicked_p);
    struct timespec t = {0, 200000000};
    nanosleep(&t, NULL);
    char byte = 1;
    (void)!write(urgent_pipe[1], &byte, 1);
    return NULL;
}
static void urgent_paused_g(void *env) {
    (void)env;
    urgent_kicked_p = tzr_this_m()->p;
    atomic_store(&urgent_kick_go, 1);
    while (!atomic_load(&urgent_pause_reached)) {
    }
    tsuzuri_tz_blocking_enter();
    char byte;
    ssize_t n = read(urgent_pipe[0], &byte, 1);
    tsuzuri_tz_blocking_exit();
    atomic_store(&urgent_read, (int)n);
}
static void urgent_paused_body(void) {
    urgent_install(urgent_counted, 0);
    urgent_runtime_ready();
    if (pipe(urgent_pipe) != 0) _exit(3);
    pthread_t kicker;
    if (pthread_create(&kicker, NULL, urgent_paused_kicker, NULL) != 0) _exit(3);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(urgent_paused_g, NULL);
    join(scope);
    pthread_join(kicker, NULL);
    _exit(atomic_load(&urgent_read) == 1 && atomic_load(&urgent_foreign) == 0 ? 53 : 4);
}

/* A handler of the program's own with SA_NODEFER runs with SIGURG unblocked. */
static _Atomic int urgent_unblocked;
static void urgent_nodefer(int sig) {
    (void)sig;
    sigset_t now;
    pthread_sigmask(SIG_BLOCK, NULL, &now);
    if (sigismember(&now, SIGURG) == 0) atomic_store(&urgent_unblocked, 1);
    atomic_fetch_add(&urgent_foreign, 1);
}
static void urgent_nodefer_body(void) {
    urgent_install(urgent_nodefer, SA_NODEFER);
    urgent_runtime_ready();
    raise(SIGURG);
    _exit(atomic_load(&urgent_foreign) == 1 && atomic_load(&urgent_unblocked) ? 54 : 4);
}

static void test_preempt_async_edges(void) {
    CHECK(exited_with(run_child("child-urgent-window"), 50));
#if !defined(__APPLE__)
    CHECK(exited_with(run_child("child-urgent-oneshot"), 51));
#endif
    CHECK(exited_with(run_child("child-urgent-syscall"), 52));
    CHECK(exited_with(run_child("child-urgent-paused"), 53));
    CHECK(exited_with(run_child("child-urgent-nodefer"), 54));
    printf("preempt async edges: ok\n");
}
static void test_preempt_async_forward(void) {
    CHECK(exited_with(run_child("child-urgent-forward"), 49));
    printf("preempt async forward: ok\n");
}
static void test_preempt_async_ignored(void) {
    CHECK(exited_with(run_child("child-urgent-ignored"), 48));
    printf("preempt async ignored: ok\n");
}
#else
static void test_preempt_async(void) { printf("preempt async: not built (Phase 1 only)\n"); }
static void test_preempt_async_values(void) { printf("preempt async values: not built\n"); }
static void test_preempt_async_runtime(void) { printf("preempt async runtime: not built\n"); }
static void test_preempt_async_ignored(void) { printf("preempt async ignored: not built\n"); }
static void test_preempt_async_forward(void) { printf("preempt async forward: not built\n"); }
static void test_preempt_async_edges(void) { printf("preempt async edges: not built\n"); }
static void urgent_kick_pause(void) {}
#endif

int main(int argc, char **argv) {
    const char *only = argc > 1 ? argv[1] : NULL;
    self_path = argv[0];
    if (only && strcmp(only, "child-deadlock") == 0) deadlock_body();
    if (only && strcmp(only, "child-deadlock-scope") == 0) deadlock_scope_body();
    if (only && strcmp(only, "child-overflow") == 0) overflow_body();
    if (only && strcmp(only, "child-exit-deadlock") == 0) exit_deadlock_body();
    if (only && strcmp(only, "child-timer-deadlock") == 0) timer_deadlock_body();
    if (only && strcmp(only, "child-recover") == 0) recover_body();
    if (only && strcmp(only, "child-report") == 0) report_body();
#if defined(TZR_ASYNC)
    if (only && strcmp(only, "child-urgent-ignored") == 0) urgent_ignored_body();
    if (only && strcmp(only, "child-urgent-forward") == 0) urgent_forward_body();
    if (only && strcmp(only, "child-urgent-window") == 0) urgent_window_body();
    if (only && strcmp(only, "child-urgent-oneshot") == 0) urgent_oneshot_body();
    if (only && strcmp(only, "child-urgent-syscall") == 0) urgent_syscall_body();
    if (only && strcmp(only, "child-urgent-paused") == 0) urgent_paused_body();
    if (only && strcmp(only, "child-urgent-nodefer") == 0) urgent_nodefer_body();
#endif
#if !defined(_WIN32)
    if (only && strcmp(only, "child-dchan-deadlock") == 0) dchan_deadlock_body();
#endif
    if (only && strcmp(only, "child-big-select") == 0) { big_select_body(); return 0; }
    if (only && strcmp(only, "child-deep-100") == 0) { stack_body(100); return 0; }
    if (only && strcmp(only, "child-deep-900") == 0) { stack_body(900); return 0; }
    if (only && strcmp(only, "child-deep-3500") == 0) { stack_body(3500); return 0; }
#if defined(TZR_ASYNC)
    /* before the first spawn, whose sysmon installs the runtime's handler over this one */
    struct sigaction counted;
    memset(&counted, 0, sizeof counted);
    counted.sa_handler = urgent_counted;
    sigemptyset(&counted.sa_mask);
    sigaction(SIGURG, &counted, NULL);
#endif
#define RUN(name, call) if (!only || strcmp(only, name) == 0) call
    RUN("spawn", test_spawn());
    RUN("tree", test_tree());
    RUN("pingpong", test_pingpong(200000, 1));
    RUN("pingpong", test_pingpong(200000, 0));
    RUN("mpmc", test_mpmc(0));
    RUN("mpmc", test_mpmc(1));
    RUN("mpmc", test_mpmc(64));
    RUN("select", test_select());
    RUN("relay", test_relay());
    RUN("mutex", test_mutex());
    RUN("waitgroup", test_waitgroup());
    RUN("sleep", test_sleep());
    RUN("external", test_external());
    RUN("deep", test_deep());
    RUN("fp", test_fp());
    RUN("stack_size", test_stack_size());
    RUN("failures", test_failures());
    RUN("fp_mode", test_fp_mode());
    RUN("wake_many", test_wake_many());
    RUN("count_after_join", test_count_after_join());
    RUN("detector", test_detector());
    RUN("starvation", test_starvation());
    RUN("select_same", test_select_same());
    RUN("boundary", test_boundary());
    RUN("report", test_report());
    RUN("blocking", test_blocking());
    RUN("donor", test_donor());
    RUN("timer_channel", test_timer_channel());
    RUN("timer_stop", test_timer_stop());
    RUN("timer_senders", test_timer_senders());
    RUN("ticker", test_ticker());
    RUN("ticker_drop", test_ticker_drop());
    RUN("after_func", test_after_func());
    RUN("boundary_timers", test_boundary_timers());
    RUN("sleep_ms_hook", test_sleep_ms_hook());
    RUN("mutex32", test_mutex32());
    RUN("rwmutex", test_rwmutex());
    RUN("rw_writers_first", test_rw_writers_first());
    RUN("once", test_once());
    RUN("cond", test_cond());
    RUN("semaphore", test_semaphore());
    RUN("context_tree", test_context_tree());
    RUN("context_deadline", test_context_deadline());
    RUN("context_release", test_context_release());
    RUN("context_deadline_race", test_context_deadline_race());
    RUN("context_after", test_context_after());
    RUN("context_expire_boundary", test_context_expire_boundary());
    RUN("parallel", test_parallel());
    RUN("scope_run", test_scope_run());
    RUN("blocking_handoff", test_blocking_handoff());
    RUN("blocking_reacquire", test_blocking_reacquire());
    RUN("blocking_deadlock", test_blocking_deadlock());
    RUN("blocking_callback", test_blocking_callback());
    RUN("locked_thread", test_locked_thread());
    RUN("locked_yield", test_locked_yield());
    RUN("locked_yielders", test_locked_yielders());
    RUN("dchan_ring", test_dchan_ring());
    RUN("dchan_waits", test_dchan_waits());
    RUN("dchan_close", test_dchan_close());
    RUN("dchan_lock", test_dchan_lock());
    RUN("dchan_deadlock", test_dchan_deadlock());
    RUN("netpoll_pipe", test_netpoll_pipe());
    RUN("netpoll_timeout", test_netpoll_timeout());
    RUN("netpoll_close", test_netpoll_close());
    RUN("netpoll_race", test_netpoll_race());
    RUN("preempt_flag", test_preempt_flag());
    RUN("preempt_sysmon", test_preempt_sysmon());
    RUN("preempt_async", test_preempt_async());
    RUN("preempt_async_values", test_preempt_async_values());
    RUN("preempt_async_runtime", test_preempt_async_runtime());
    RUN("preempt_async_ignored", test_preempt_async_ignored());
    RUN("preempt_async_forward", test_preempt_async_forward());
    RUN("preempt_async_edges", test_preempt_async_edges());
    RUN("preempt_idle", test_preempt_idle());
    RUN("long_sleep", test_long_sleep()); /* last: its sleeper never ends */
    printf("all passed\n");
    return 0;
}
