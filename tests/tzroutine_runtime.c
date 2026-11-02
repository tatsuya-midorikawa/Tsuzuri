/* Tests of the tzroutine runtime (TZ01), run by tests/tzroutine_runtime.mjs. By hand:
 * clang -std=c11 -Wall -Wextra -Werror -O2 -pthread tests/tzroutine_runtime.c -o t && ./t [case]
 * (on Windows: --target=x86_64-pc-windows-msvc -fuse-ld=lld instead of -pthread). */
#include "../src/runtime/tzroutine.c"

#include <assert.h>
#include <fenv.h>
#if !defined(_WIN32)
#include <signal.h>
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
    for (int32_t what = 0; what <= 6; what++) tsuzuri_tz_report(what); /* 0 and 6 write nothing */
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

/* TZ02: a G inside a blocking call acts as its thread's pseudo-G. With one P, the tzroutines that
 * it spawns there run (the P went away), its wait there stops the thread instead of switching the G
 * away, the calls nest, and afterwards the G has a P again and can switch. */
static _Atomic int64_t blocking_hits;
static void blocking_child(void *env) {
    (void)env;
    atomic_fetch_add(&blocking_hits, 1);
}
static void blocking_routine(void *env) {
    (void)env;
    tsuzuri_tz_block_enter();
    void *scope = tsuzuri_tz_scope_enter(); /* as a pool callback on the joining thread does */
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(blocking_child, NULL);
    tsuzuri_tz_yield(); /* the thread's yield: the G stays */
    join(scope);
    tsuzuri_tz_block_enter();
    tsuzuri_tz_block_leave();
    tsuzuri_tz_block_leave();
    tsuzuri_tz_yield();
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

/* TZ02: the spawn of the main thread gives the idle P of a blocking call to an M that tzr_wakep starts
 * spinning, and the G leaves its call before that M runs, so it waits for the P. The M hands the P
 * over and parks: it stops spinning, or tzr_wakep would never start an M again and the next spawn of
 * the main thread would wait for nobody. With one P, the first round sets this up. */
static _Atomic int donor_inside;
static _Atomic int donor_leave;
static _Atomic int64_t donor_hits;
static void donor_child(void *env) {
    (void)env;
    atomic_fetch_add(&donor_hits, 1);
}
static void donor_blocker(void *env) {
    (void)env;
    tsuzuri_tz_block_enter();
    atomic_store(&donor_inside, 1);
    while (!atomic_load(&donor_leave)) {
    }
    tsuzuri_tz_block_leave();
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
        /* the Ms that looked for work park within microseconds */
        double t = seconds();
        while (atomic_load(&tzr_sched.nmspinning) != 0 && seconds() - t < 2.0) tzr_os_yield();
        CHECK(atomic_load(&tzr_sched.nmspinning) == 0);
    }
}

/* TZ03: the mark of a failure domain is the number of the last batch announced. A batch that
 * another thread announced before the mark and ended before the reset, and one that this thread
 * announced before the mark (the enclosing batch of an item that calls Domain.run), stay; what this
 * thread announced after the mark, which a trap left, goes. An item's end drops what was announced
 * in the item in the same way, and puts its thread back in the state it had before the item. */
static char batch_outer, batch_other, batch_inner, batch_item;
static void (*batch_step)(void);
static void batch_other_begin(void) { tsuzuri_tz_parallel_begin(&batch_other, 1); }
static void batch_other_end(void) { tsuzuri_tz_parallel_end(&batch_other, 1); }
#if defined(_WIN32)
static DWORD WINAPI batch_thread(LPVOID arg) {
    (void)arg;
    batch_step();
    return 0;
}
#else
static void *batch_thread(void *arg) {
    (void)arg;
    batch_step();
    return NULL;
}
#endif
static void on_other_thread(void (*step)(void)) {
    batch_step = step;
#if defined(_WIN32)
    HANDLE thread = CreateThread(NULL, 0, batch_thread, NULL, 0, NULL);
    CHECK(thread != NULL && WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0 && CloseHandle(thread));
#else
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, batch_thread, NULL) == 0 && pthread_join(thread, NULL) == 0);
#endif
}
static int64_t pending(void) {
    tzr_mutex_lock(&tzr_sched.lock);
    int64_t n = tzr_sched.npending;
    tzr_mutex_unlock(&tzr_sched.lock);
    return n;
}
static void test_batches(void) {
    tsuzuri_tz_parallel_begin(&batch_outer, 2); /* this thread waits for the batch */
    /* it runs one item, which announces a batch and traps: the item's end drops that batch */
    tzr_item_frame frame;
    tsuzuri_tz_task_enter(&batch_outer, &frame);
    CHECK(frame.idle && pending() == 1);
    tsuzuri_tz_parallel_begin(&batch_item, 4);
    tsuzuri_tz_task_leave(&batch_outer, &frame);
    CHECK(pending() == 1);
    tzr_mutex_lock(&tzr_sched.lock);
    CHECK(tzr_batch_of(&batch_item) == NULL && tzr_pseudo()->idle); /* waiting for the batch again */
    tzr_mutex_unlock(&tzr_sched.lock);
    /* a failure domain inside the wait, while another thread's batch comes and goes */
    on_other_thread(batch_other_begin);
    uint64_t mark = tsuzuri_tz_parallel_mark();
    on_other_thread(batch_other_end);
    tsuzuri_tz_parallel_begin(&batch_inner, 3);
    CHECK(pending() == 4);
    tsuzuri_tz_parallel_reset(mark);
    CHECK(pending() == 1);
    tzr_mutex_lock(&tzr_sched.lock);
    CHECK(tzr_batch_of(&batch_outer) != NULL && tzr_batch_of(&batch_inner) == NULL);
    CHECK(!tzr_pseudo()->idle); /* it runs again */
    tzr_mutex_unlock(&tzr_sched.lock);
    tsuzuri_tz_parallel_end(&batch_outer, 2);
    CHECK(pending() == 0);
}

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
    if (only && strcmp(only, "child-big-select") == 0) { big_select_body(); return 0; }
    if (only && strcmp(only, "child-deep-100") == 0) { stack_body(100); return 0; }
    if (only && strcmp(only, "child-deep-900") == 0) { stack_body(900); return 0; }
    if (only && strcmp(only, "child-deep-3500") == 0) { stack_body(3500); return 0; }
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
    RUN("batches", test_batches());
    RUN("long_sleep", test_long_sleep()); /* last: its sleeper never ends */
    printf("all passed\n");
    return 0;
}
