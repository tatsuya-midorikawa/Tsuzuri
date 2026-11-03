/* The tzroutine runtime (TZ01, D-61): M:N scheduler, stacks, context switch, parking lot, timers,
 * channels and select. POSIX (macOS, Linux) and Windows (TZ01 Phase 2, src/runtime/tzroutine-windows.h)
 * on aarch64 and x86-64. The generated code calls the tsuzuri_tz_ functions (TZ02 and later);
 * everything else is internal.
 *
 * Names: G = one tzroutine (or the pseudo-G of a thread that is not a scheduler worker), M = an OS
 * thread of the scheduler, P = the right to run tzroutines (there are `nprocs` of them).
 *
 * Rules that keep it correct (each one is checked by tests/tzroutine_runtime.c):
 *  R1. A G never holds a tzr_lock across a context switch, except the locks that tzr_park releases
 *      on g0 after the switch (the waiter is fully off its stack before anyone can wake it).
 *  R2. Thread-local state is read only through tzr_this_m(), which is noinline: code after a switch
 *      may run on another thread, so no TLS address may be kept across tzr_park or tzr_yield.
 *  R3. Lock order: tzr_sched.lock is innermost. Channel locks (sorted by address in select), parking-lot
 *      bucket locks and tzr_timers.lock may be held while tzr_sched.lock is taken (tzr_ready), never after.
 *  R4. Only a tzr_lock may be released by tzr_park's unlock callback (never a pthread mutex).
 *
 * TZR_SINGLE (TZ03) builds the channels alone for the default WebAssembly target, where one thread
 * runs the whole program and nothing is imported; see the section "one thread" below.
 */
#if defined(TZR_SINGLE)
/* ---------------------------------------------------------------- one thread (TZR_SINGLE) */

/* The default WebAssembly target runs one thread and a program there spawns no tzroutine (the
 * driver refuses one that does). So there is one G, the locks do nothing, and an operation that
 * would wait finds that nobody can wake it: tzr_park returns 1, the deadlock status, and the
 * generated code traps at the call, as DataChannel's channel-wasm.ll does. A waiter is unlinked
 * before its operation returns, so no queue holds a sudog when another operation runs. No thread,
 * mapping, TLS, signal or standard error is used: the object imports nothing. */
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#define memcpy __builtin_memcpy
#define memset __builtin_memset
#ifndef TZR_API
#define TZR_API __attribute__((weak, visibility("hidden")))
#endif

typedef struct { uint32_t word; uint32_t pad; } tzr_lock;
static inline void tzr_lock_acquire(tzr_lock *l) { (void)l; }
static inline void tzr_lock_release(tzr_lock *l) { (void)l; }

enum { TZR_KIND_ROUTINE, TZR_KIND_THREAD };
typedef struct tzr_g tzr_g;
typedef struct tzr_sudog tzr_sudog;
typedef struct tzr_chan tzr_chan;
struct tzr_g {
    int kind;
    tzr_sudog *param;
    _Atomic uint32_t select_done;
    uint64_t rand;
};

static tzr_g tzr_single = {.kind = TZR_KIND_THREAD, .rand = 0x2545F4914F6CDD1DULL};
static tzr_g *tzr_current(void) { return &tzr_single; }

static int tzr_park(void (*unlock)(void *), void *arg, const char *reason) {
    (void)reason;
    if (unlock) unlock(arg);
    return 1;
}

static void tzr_ready(tzr_g *g) { (void)g; }
static void tzr_ready_unlock(tzr_g *g, tzr_lock *lock) {
    (void)g;
    (void)lock;
}

/* No standard error: the trap at the call site is the report. */
TZR_API void tsuzuri_tz_report(int32_t what) { (void)what; }

/* xorshift64*, as the scheduler's: the choice among the ready cases of a select. */
static uint64_t tzr_rand(uint64_t *state) {
    uint64_t x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

/* Routine's questions: one thread, and no tzroutine but the caller's. */
TZR_API void tsuzuri_tz_yield(void) {}
TZR_API int64_t tsuzuri_tz_count(void) { return 1; }
TZR_API int64_t tsuzuri_tz_max_procs(void) { return 1; }
TZR_API int64_t tsuzuri_tz_cpu_count(void) { return 1; }

/* The sync types of TZ06 on one thread: a held lock is held by the one G, so taking it again is
 * the nesting that traps, and a wait that nobody can end is a deadlock (status 2). */
TZR_API int32_t tsuzuri_tz_mutex32_lock(_Atomic uint32_t *word) {
    if (atomic_load_explicit(word, memory_order_relaxed) != 0) return 1;
    atomic_store_explicit(word, 1, memory_order_relaxed);
    return 0;
}
TZR_API void tsuzuri_tz_mutex32_unlock(_Atomic uint32_t *word) { atomic_store_explicit(word, 0, memory_order_relaxed); }
TZR_API int32_t tsuzuri_tz_rw_read_lock(_Atomic uint64_t *word) {
    if (atomic_load_explicit(word, memory_order_relaxed) != 0) return 1;
    atomic_store_explicit(word, 1, memory_order_relaxed);
    return 0;
}
TZR_API void tsuzuri_tz_rw_read_unlock(_Atomic uint64_t *word) { atomic_store_explicit(word, 0, memory_order_relaxed); }
TZR_API int32_t tsuzuri_tz_rw_write_lock(_Atomic uint64_t *word) { return tsuzuri_tz_rw_read_lock(word); }
TZR_API void tsuzuri_tz_rw_write_unlock(_Atomic uint64_t *word) { atomic_store_explicit(word, 0, memory_order_relaxed); }
TZR_API int32_t tsuzuri_tz_held_depth(void) { return 0; }
TZR_API void tsuzuri_tz_held_restore(int32_t depth) { (void)depth; }
TZR_API int32_t tsuzuri_tz_once_begin(_Atomic uint64_t *word) {
    uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
    if (s == 2) return 0;
    if (s == 1) return 2; /* the function calls its own Once */
    atomic_store_explicit(word, 1, memory_order_relaxed);
    return 1;
}
TZR_API void tsuzuri_tz_once_end(_Atomic uint64_t *word) { atomic_store_explicit(word, 2, memory_order_relaxed); }
TZR_API int32_t tsuzuri_tz_cond_wait(_Atomic uint64_t *sequence, _Atomic uint32_t *mutex, uint64_t observed) {
    (void)sequence;
    (void)mutex;
    (void)observed;
    return 2;
}
TZR_API void tsuzuri_tz_cond_wake(_Atomic uint64_t *sequence, int64_t n) {
    (void)n;
    atomic_fetch_add_explicit(sequence, 1, memory_order_relaxed);
}
TZR_API int32_t tsuzuri_tz_sem_acquire(_Atomic uint64_t *word, uint64_t weight, int32_t block) {
    uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
    if (s >= weight) {
        atomic_store_explicit(word, s - weight, memory_order_relaxed);
        return 0;
    }
    return block ? 2 : 1;
}
TZR_API int32_t tsuzuri_tz_sem_release(_Atomic uint64_t *word, uint64_t weight, uint64_t capacity) {
    uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
    if (weight > capacity - s) return 1;
    atomic_store_explicit(word, s + weight, memory_order_relaxed);
    return 0;
}
TZR_API int32_t tsuzuri_tz_waitgroup_add(_Atomic uint64_t *word, int64_t delta) {
    uint64_t v = atomic_load_explicit(word, memory_order_relaxed) + (uint64_t)delta;
    atomic_store_explicit(word, v, memory_order_relaxed);
    return v > (uint64_t)INT64_MAX;
}
TZR_API int32_t tsuzuri_tz_waitgroup_wait(_Atomic uint64_t *word) {
    return atomic_load_explicit(word, memory_order_relaxed) == 0 ? 0 : 2;
}

#else
#if defined(_WIN32)
static void tzr_fatal(const char *message);
#include "tzroutine-windows.h"
#else
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <os/lock.h>
#endif
#if defined(__linux__)
#include <linux/futex.h>
#include <sys/syscall.h>
#endif
#endif

#if defined(__has_feature)
#if __has_feature(thread_sanitizer)
#define TZR_TSAN 1
#endif
#endif
#if defined(TZR_TSAN)
#include <sanitizer/tsan_interface.h>
/* TSan must be told about every stack switch. Flag 0 makes the switch order memory, which is what
 * it does: the same OS thread runs both sides. Cross-thread hand-offs still need the run-queue
 * and lock synchronization, and TSan checks those. */
#define TZR_TSAN_SWITCH(fiber) __tsan_switch_to_fiber((fiber), 0)
#else
#define TZR_TSAN_SWITCH(fiber) ((void)0)
#endif

#if defined(__has_feature)
#if __has_feature(address_sanitizer)
#define TZR_ASAN 1
#endif
#endif
#if defined(__SANITIZE_ADDRESS__) && !defined(TZR_ASAN)
#define TZR_ASAN 1
#endif
#if defined(TZR_ASAN) && defined(_WIN32)
#error "tzroutine: AddressSanitizer is supported on POSIX only (a fiber's stack bounds are the OS's)"
#endif
#if defined(TZR_ASAN)
/* ASan keeps a fake stack per stack (detect_stack_use_after_return): each switch tells it which stack
 * runs next, and the context that resumes takes its own fake stack back. */
#include <sanitizer/common_interface_defs.h>
#define TZR_ASAN_START(save, bottom, size) __sanitizer_start_switch_fiber((save), (bottom), (size))
#define TZR_ASAN_FINISH(save, bottom, size) __sanitizer_finish_switch_fiber((save), (bottom), (size))
#endif

#if !defined(__aarch64__) && !defined(__x86_64__)
#error "tzroutine: unsupported architecture"
#endif

#ifndef TZR_API
#define TZR_API __attribute__((weak, visibility("hidden")))
#endif
#define TZR_NOINLINE __attribute__((noinline))
#define TZR_MAXPROCS 256
#define TZR_RUNQ 256
#define TZR_GUARD (128u * 1024u)
#define TZR_DEFAULT_STACK (1024u * 1024u)
#if defined(_WIN32)
/* A fiber's committed stack cannot be given back (tzr_stack_trim is a no-op), so the global cache of
 * finished Gs stays small there. */
#define TZR_STACK_CACHE 64
#else
#define TZR_STACK_CACHE 1024
#endif
#define TZR_P_GFREE 64
#define TZR_LOCAL_CACHE 256 /* finished Gs kept untrimmed in all the Ps' lists together */
#define TZR_RUNNEXT_TURNS 61 /* takes from runnext in a row before the local ring gets a turn */

/* ---------------------------------------------------------------- failure */

static void tzr_fatal(const char *message) {
    fprintf(stderr, "Tsuzuri runtime: %s\n", message);
    fflush(stderr);
    abort();
}

#if defined(TZ_STACK_GUARD)
/* src/runtime/stack.c: the alternate signal stack of a thread, and the hook its handler asks first. */
void tsuzuri_stack_scheduler_thread(void);
extern _Atomic(int32_t (*)(uintptr_t)) tsuzuri_stack_guard_hook;
#endif
TZR_API int32_t tsuzuri_tz_stack_guard_hit(uintptr_t address);

#if !defined(_WIN32)
/* ---------------------------------------------------------------- OS calls (POSIX) */

/* The OS calls that tzroutine-windows.h maps to Win32, kept in small functions. */
typedef pthread_mutex_t tzr_mutex;
typedef pthread_cond_t tzr_cond;
#define TZR_MUTEX_INIT PTHREAD_MUTEX_INITIALIZER
#define TZR_COND_INIT PTHREAD_COND_INITIALIZER
static inline void tzr_mutex_lock(tzr_mutex *m) { pthread_mutex_lock(m); }
static inline void tzr_mutex_unlock(tzr_mutex *m) { pthread_mutex_unlock(m); }
static inline void tzr_cond_signal(tzr_cond *c) { pthread_cond_signal(c); }
/* The timer thread's wakeup measures its timeouts on the monotonic clock, so that a change of the
 * wall clock cannot delay a timer. Linux sets the clock of the condition variable (which is then
 * not statically initialized); macOS waits for a relative time. */
static inline void tzr_timer_cond_init(tzr_cond *c) {
#if defined(__linux__)
    pthread_condattr_t attr;
    pthread_condattr_init(&attr);
    pthread_condattr_setclock(&attr, CLOCK_MONOTONIC);
    pthread_cond_init(c, &attr);
    pthread_condattr_destroy(&attr);
#else
    (void)c;
#endif
}
/* Waits at most `ns` nanoseconds (at most one day; the caller looks at the clock again), or until
 * signalled when ns < 0. */
static inline void tzr_cond_wait_for(tzr_cond *c, tzr_mutex *m, int64_t ns) {
    if (ns < 0) {
        pthread_cond_wait(c, m);
        return;
    }
    const int64_t day = 86400 * (int64_t)1000000000;
    if (ns > day) ns = day; /* the deadline of a saturated sleep would overflow the sum below */
#if defined(__APPLE__)
    struct timespec relative = {(time_t)(ns / 1000000000), (long)(ns % 1000000000)};
    pthread_cond_timedwait_relative_np(c, m, &relative);
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    int64_t abs = (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec + ns;
    ts.tv_sec = abs / 1000000000;
    ts.tv_nsec = abs % 1000000000;
    pthread_cond_timedwait(c, m, &ts);
#endif
}

typedef pthread_once_t tzr_once;
#define TZR_ONCE_INIT PTHREAD_ONCE_INIT
static inline void tzr_run_once(tzr_once *once, void (*initialize)(void)) { pthread_once(once, initialize); }

static inline void tzr_os_yield(void) { sched_yield(); }
static inline long tzr_online_cpus(void) { return sysconf(_SC_NPROCESSORS_ONLN); }
static inline size_t tzr_page_size(void) { return (size_t)sysconf(_SC_PAGESIZE); }

/* Starts a detached thread whose stack is `stack` bytes (0: the default). */
static void tzr_thread_start(void *(*run)(void *), void *arg, size_t stack, const char *failure) {
    pthread_t thread;
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    if (stack) pthread_attr_setstacksize(&attr, stack);
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    if (pthread_create(&thread, &attr, run, arg) != 0) tzr_fatal(failure);
    pthread_attr_destroy(&attr);
}

/* ---------------------------------------------------------------- locks */

/* tzr_lock: 8 bytes, all-zero is unlocked, unlocked by the thread that locked it (R1). It is the only
 * kind of lock that tzr_park may release on g0 (R4). Under TSan it is a plain atomic spin lock, so
 * that TSan, which treats each stack as its own thread, sees acquire/release instead of a mutex
 * that one "thread" locks and another unlocks. */
#if defined(TZR_TSAN)
typedef struct { _Atomic uint32_t word; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) {
    uint32_t c = 0;
    while (!atomic_compare_exchange_weak_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) {
        c = 0;
        sched_yield();
    }
}
static void tzr_lock_release(tzr_lock *l) { atomic_store_explicit(&l->word, 0, memory_order_release); }
#elif defined(__APPLE__)
typedef struct { os_unfair_lock lock; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) { os_unfair_lock_lock(&l->lock); }
static void tzr_lock_release(tzr_lock *l) { os_unfair_lock_unlock(&l->lock); }
#else
typedef struct { _Atomic uint32_t word; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) {
    uint32_t c = 0;
    if (atomic_compare_exchange_strong_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) return;
    for (int spin = 0; spin < 64; spin++) {
        c = 0;
        if (atomic_load_explicit(&l->word, memory_order_relaxed) == 0
            && atomic_compare_exchange_weak_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) return;
#if defined(__x86_64__)
        __builtin_ia32_pause();
#else
        __asm__ volatile("yield");
#endif
    }
    while (atomic_exchange_explicit(&l->word, 2, memory_order_acquire) != 0)
        syscall(SYS_futex, &l->word, FUTEX_WAIT_PRIVATE, 2, NULL, NULL, 0);
}
static void tzr_lock_release(tzr_lock *l) {
    if (atomic_exchange_explicit(&l->word, 0, memory_order_release) == 2)
        syscall(SYS_futex, &l->word, FUTEX_WAKE_PRIVATE, 1, NULL, NULL, 0);
}
#endif

/* tzr_note: a one-shot wakeup for an OS thread (an idle M, a parked pseudo-G). */
typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    int set;
} tzr_note;

static void tzr_note_init(tzr_note *n) {
    pthread_mutex_init(&n->mutex, NULL);
    pthread_cond_init(&n->cond, NULL);
    n->set = 0;
}
static void tzr_note_wake(tzr_note *n) {
    pthread_mutex_lock(&n->mutex);
    n->set = 1;
    pthread_cond_signal(&n->cond);
    pthread_mutex_unlock(&n->mutex);
}
static void tzr_note_sleep(tzr_note *n) {
    pthread_mutex_lock(&n->mutex);
    while (!n->set) pthread_cond_wait(&n->cond, &n->mutex);
    n->set = 0;
    pthread_mutex_unlock(&n->mutex);
}
static inline void tzr_note_destroy(tzr_note *n) {
    pthread_mutex_destroy(&n->mutex);
    pthread_cond_destroy(&n->cond);
}

static int64_t tzr_nanotime(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec;
}

/* ---------------------------------------------------------------- context switch */

/* void tzr_switch(void **save_sp, void *load_sp): pushes the callee-saved registers, stores sp in
 * *save_sp, loads load_sp, pops the registers saved there and returns into that context. */
#if defined(__aarch64__)
#if defined(__APPLE__)
#define TZR_SYM(name) "_" #name
#else
#define TZR_SYM(name) #name
#endif
__attribute__((naked, noinline)) static void tzr_switch(void **save_sp, void *load_sp) {
    /* FPCR (rounding mode, exception traps) and FPSR (exception flags) are part of each context;
     * they are written back only when they differ, because writing them can stall. */
    __asm__ volatile(
        "sub sp, sp, #176\n"
        "stp d8, d9, [sp, #0]\n"
        "stp d10, d11, [sp, #16]\n"
        "stp d12, d13, [sp, #32]\n"
        "stp d14, d15, [sp, #48]\n"
        "stp x19, x20, [sp, #64]\n"
        "stp x21, x22, [sp, #80]\n"
        "stp x23, x24, [sp, #96]\n"
        "stp x25, x26, [sp, #112]\n"
        "stp x27, x28, [sp, #128]\n"
        "stp x29, x30, [sp, #144]\n"
        "mrs x9, fpcr\n"
        "mrs x10, fpsr\n"
        "stp x9, x10, [sp, #160]\n"
        "mov x9, sp\n"
        "str x9, [x0]\n"
        "mov sp, x1\n"
        "ldp d8, d9, [sp, #0]\n"
        "ldp d10, d11, [sp, #16]\n"
        "ldp d12, d13, [sp, #32]\n"
        "ldp d14, d15, [sp, #48]\n"
        "ldp x19, x20, [sp, #64]\n"
        "ldp x21, x22, [sp, #80]\n"
        "ldp x23, x24, [sp, #96]\n"
        "ldp x25, x26, [sp, #112]\n"
        "ldp x27, x28, [sp, #128]\n"
        "ldp x29, x30, [sp, #144]\n"
        "ldp x9, x10, [sp, #160]\n"
        "mrs x11, fpcr\n"
        "cmp x9, x11\n"
        "b.eq 1f\n"
        "msr fpcr, x9\n"
        "1:\n"
        "mrs x11, fpsr\n"
        "cmp x10, x11\n"
        "b.eq 2f\n"
        "msr fpsr, x10\n"
        "2:\n"
        "add sp, sp, #176\n"
        "ret\n");
}
#define TZR_FRAME_BYTES 176
static void *tzr_initial_frame(char *top, void (*start)(void)) {
    uint64_t *frame = (uint64_t *)(top - TZR_FRAME_BYTES);
    memset(frame, 0, TZR_FRAME_BYTES);
    frame[18] = 0;                        /* x29: ends the frame-pointer chain */
    frame[19] = (uint64_t)(uintptr_t)start; /* x30: where the first switch returns */
    frame[20] = 0;                        /* FPCR: round to nearest, no traps */
    frame[21] = 0;                        /* FPSR: no exception flag */
    return frame;
}
#else /* x86-64 System V */
__attribute__((naked, noinline)) static void tzr_switch(void **save_sp, void *load_sp) {
    __asm__ volatile(
        "pushq %rbp\n"
        "pushq %rbx\n"
        "pushq %r12\n"
        "pushq %r13\n"
        "pushq %r14\n"
        "pushq %r15\n"
        /* MXCSR and the x87 environment (control, status and tag words): rounding, masks and the
         * exception flags of each context stay its own */
        "subq $40, %rsp\n"
        "stmxcsr (%rsp)\n"
        "fnstenv 4(%rsp)\n"
        "movq %rsp, (%rdi)\n"
        "movq %rsi, %rsp\n"
        "ldmxcsr (%rsp)\n"
        "fldenv 4(%rsp)\n"
        "addq $40, %rsp\n"
        "popq %r15\n"
        "popq %r14\n"
        "popq %r13\n"
        "popq %r12\n"
        "popq %rbx\n"
        "popq %rbp\n"
        "ret\n");
}
static void *tzr_initial_frame(char *top, void (*start)(void)) {
    /* After `ret` the start function sees rsp = top - 8, as if it had been called. */
    uint64_t *slot = (uint64_t *)(top - 16);
    slot[1] = 0;                              /* fake return address of `start` */
    slot[0] = (uint64_t)(uintptr_t)start;     /* popped by ret */
    uint64_t *frame = slot - 11;
    memset(frame, 0, 11 * sizeof(uint64_t));  /* the environments, then r15 r14 r13 r12 rbx rbp = 0 */
    /* The default MXCSR, then the x87 environment of fninit: control 0x037F, status 0, every
     * register empty (tag 0xFFFF). */
    uint32_t defaults[4] = {0x1F80u, 0x037Fu, 0u, 0xFFFFu};
    memcpy(frame, defaults, sizeof defaults);
    return frame;
}
#endif

/* g0 -> G and G -> g0 (tzroutine-windows.h switches fibers instead). */
#define TZR_TO_G(m, g) tzr_switch(&(m)->g0_sp, (g)->sp)
#define TZR_TO_G0(m, g) tzr_switch(&(g)->sp, (m)->g0_sp)
#endif /* !_WIN32 */

/* ---------------------------------------------------------------- structures */

enum { TZR_G_IDLE, TZR_G_RUNNABLE, TZR_G_RUNNING, TZR_G_WAITING, TZR_G_DEAD };
enum { TZR_KIND_ROUTINE, TZR_KIND_THREAD };

typedef struct tzr_g tzr_g;
typedef struct tzr_m tzr_m;
typedef struct tzr_p tzr_p;
typedef struct tzr_sudog tzr_sudog;
typedef struct tzr_chan tzr_chan;

typedef struct tzr_timer tzr_timer;

typedef struct tzr_scope {
    tzr_lock lock;
    int64_t live;  /* tzroutines spawned in this scope that have not finished */
    tzr_g *waiter;  /* the G waiting in tsuzuri_tz_scope_leave */
    struct tzr_scope *outer; /* the thread's scope before tsuzuri_tz_scope_enter */
    struct tzr_scope *boundary; /* the boundary whose end stops the timers made in this scope (TZ05) */
    tzr_timer *timers; /* the timers listed in this boundary (tzr_timers.lock) */
} tzr_scope;

struct tzr_g {
    void *sp;
    void (*code)(void *);
    void *env;
    char *stack;       /* mapping: guard + usable stack; NULL for a pseudo-G */
    size_t stack_size; /* usable bytes above the guard */
    _Atomic int status;
    int kind;
    tzr_g *link;        /* run queue, free list */
    tzr_g *all_next;    /* allgs */
    tzr_g *all_prev;
    tzr_scope *scope;       /* the scope this G is counted in (NULL for a pseudo-G) */
    tzr_scope *spawn_scope; /* where tsuzuri_tz_go counts children (NULL: scope, or the root) */
    uint64_t id;
    const char *wait_reason;
    tzr_note note;      /* pseudo-G only */
    int doomed;        /* pseudo-G only: woken by the deadlock detector */
    int idle;          /* pseudo-G only: not counted as running (an M's thread outside a blocking call, or
                        * a thread that waits for the items it gave the task runtime) */
    uint64_t rand;
    tzr_sudog *param;   /* the sudog that completed this G's wait */
    _Atomic uint32_t select_done;
    uintptr_t *held;    /* the locks that it holds, in the order taken (TZ06), with their kinds */
    uint32_t held_count, held_capacity;
#if defined(TZR_TSAN)
    void *tsan_fiber;
#endif
#if defined(_WIN32)
    void *fiber;        /* the G's stack and registers; kept with the G for the next spawn */
#endif
};

struct tzr_p {
    int id;
    _Atomic uint32_t head;
    _Atomic uint32_t tail;
    _Atomic(tzr_g *) runq[TZR_RUNQ];
    _Atomic(tzr_g *) runnext;
    tzr_p *link;
    uint32_t schedtick;
    uint32_t runnext_streak; /* owner only: tzr_runqget's takes from runnext in a row */
    tzr_g *gfree;
    int gfree_count;
};

struct tzr_m {
    void *g0_sp;
    tzr_g *curg;
    tzr_p *p;
    tzr_p *nextp;
    tzr_note park;
    tzr_m *link;
    int spinning;
    int blocked;   /* inside a blocking call of curg: the thread acts as a pseudo-G */
    void (*after)(tzr_m *, tzr_g *);
    void (*after_unlock)(void *);
    void *after_arg;
    uint64_t rand;
#if defined(TZR_TSAN)
    void *tsan_g0;
#endif
#if defined(_WIN32)
    void *g0_fiber;
#endif
#if defined(TZR_ASAN)
    const void *g0_bottom; /* the bounds of this M's own stack, for ASan's switches */
    size_t g0_size;
#endif
};

static struct {
    tzr_mutex lock;
    tzr_once once;
    tzr_g *runq_head, *runq_tail;
    _Atomic int64_t runq_size;
    tzr_p *pidle;
    _Atomic int npidle;
    tzr_m *midle;
    int nmidle;
    tzr_m *pwait;                 /* Ms back from a blocking call, waiting for a P */
    _Atomic int npwait;           /* their number, read without the lock by the scheduling loop */
    int nmblocked;                /* Ms in a blocking call: their threads count as pseudo-Gs */
    int mcount;
    _Atomic int nmspinning;
    tzr_p *allp[TZR_MAXPROCS];
    int nprocs;
    _Atomic int64_t ngoroutines;  /* spawned tzroutines that have not finished */
    int npseudo_running;          /* pseudo-Gs that are not parked or idle */
    int npseudo_parked;
    int64_t npending;             /* items of the task runtime that have not started (TZ03) */
    _Atomic int64_t ntimers;      /* timers that will still fire */
    _Atomic int local_cached;     /* finished Gs in the Ps' lists (at most TZR_LOCAL_CACHE) */
    _Atomic uint64_t next_id;
    tzr_g *allgs;                  /* every G with a stack, for the deadlock report */
    tzr_lock allgs_lock;
    tzr_g *gfree;                  /* finished Gs with their stacks, at most TZR_STACK_CACHE */
    int gfree_count;
    tzr_lock gfree_lock;
    size_t stack_size;
    int deadlocked;
} tzr_sched = {.lock = TZR_MUTEX_INIT, .once = TZR_ONCE_INIT};

static _Thread_local tzr_m *tzr_tls_m;
static _Thread_local tzr_g *tzr_tls_pseudo;

/* R2: every TLS read goes through these noinline functions. */
static TZR_NOINLINE tzr_m *tzr_this_m(void) { return tzr_tls_m; }

static uint64_t tzr_rand(uint64_t *state) {
    /* xorshift64*; the state is never zero */
    uint64_t x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static void tzr_start(void);
static tzr_g *tzr_pseudo(void);

/* The G of the calling code: the G of the M, or the pseudo-G of a thread that is not an M or is
 * an M inside a blocking call (whose code runs as the thread's, never switching the G away). */
static TZR_NOINLINE tzr_g *tzr_current(void) {
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->curg != NULL && !m->blocked) return m->curg;
    return tzr_pseudo();
}

/* ---------------------------------------------------------------- stacks */

#if !defined(_WIN32)
/* A stack is one private mapping: TZR_GUARD bytes of PROT_NONE at the low end, then stack_size
 * usable bytes. The OS commits pages only when they are touched. A frame can be larger than the
 * guard, so the compiler builds the code of a program that spawns with stack probes (TZ02): a
 * frame touches the guard instead of jumping over it. */
static char *tzr_stack_map(size_t size) {
    size_t total = TZR_GUARD + size;
    void *map = mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (map == MAP_FAILED) return NULL;
    if (mprotect(map, TZR_GUARD, PROT_NONE) != 0) {
        munmap(map, total);
        return NULL;
    }
    return map;
}

/* Gives the touched pages of a cached stack back to the OS, except the top 16 KiB. */
static void tzr_stack_trim(tzr_g *g) {
    size_t keep = 16 * 1024;
    if (g->stack_size > keep) {
#if defined(__APPLE__)
        madvise(g->stack + TZR_GUARD, g->stack_size - keep, MADV_FREE);
#else
        madvise(g->stack + TZR_GUARD, g->stack_size - keep, MADV_DONTNEED);
#endif
    }
}
#else
/* A fiber's stack belongs to the OS, which commits it as it grows (tzroutine-windows.h). */
static void tzr_stack_trim(tzr_g *g) { (void)g; }
#endif

/* Whether a fault at `address` hit the guard of the tzroutine that this thread is running. Called
 * by the SIGSEGV/SIGBUS handler of src/runtime/stack.c on the faulting thread's alternate stack:
 * it reads only this thread's M and the G's fixed fields, so it is async-signal-safe. */
TZR_API int32_t tsuzuri_tz_stack_guard_hit(uintptr_t address) {
    tzr_m *m = tzr_tls_m;
    tzr_g *g = m ? m->curg : NULL;
    if (g == NULL || g->stack == NULL) return 0;
    uintptr_t low = (uintptr_t)g->stack;
    return address >= low && address < low + TZR_GUARD + 4096;
}

/* ---------------------------------------------------------------- run queues */

/* The local run queue is a ring that only its P's M pushes to (tail) and that thieves pop from
 * (head, with CAS); see Go's runqput/runqget/runqsteal for the same design. */
static void tzr_globrunqput(tzr_g *g) {
    g->link = NULL;
    if (tzr_sched.runq_tail) tzr_sched.runq_tail->link = g;
    else tzr_sched.runq_head = g;
    tzr_sched.runq_tail = g;
    atomic_fetch_add(&tzr_sched.runq_size, 1);
}

static tzr_g *tzr_globrunqget(tzr_p *p, int max);

static void tzr_runqput_slow(tzr_p *p, tzr_g *g, uint32_t head, uint32_t tail) {
    tzr_g *batch[TZR_RUNQ / 2 + 1];
    uint32_t n = (tail - head) / 2;
    for (uint32_t i = 0; i < n; i++) batch[i] = atomic_load_explicit(&p->runq[(head + i) % TZR_RUNQ], memory_order_relaxed);
    if (!atomic_compare_exchange_strong_explicit(&p->head, &head, head + n, memory_order_acq_rel, memory_order_relaxed)) {
        /* a thief took some; the ring has room now */
        uint32_t t = atomic_load_explicit(&p->tail, memory_order_relaxed);
        atomic_store_explicit(&p->runq[t % TZR_RUNQ], g, memory_order_relaxed);
        atomic_store_explicit(&p->tail, t + 1, memory_order_release);
        return;
    }
    batch[n] = g;
    tzr_mutex_lock(&tzr_sched.lock);
    for (uint32_t i = 0; i <= n; i++) tzr_globrunqput(batch[i]);
    tzr_mutex_unlock(&tzr_sched.lock);
}

static void tzr_runqput(tzr_p *p, tzr_g *g, int next) {
    if (next) {
        g = atomic_exchange_explicit(&p->runnext, g, memory_order_acq_rel);
        if (g == NULL) return;
    }
    uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
    if (tail - head < TZR_RUNQ) {
        atomic_store_explicit(&p->runq[tail % TZR_RUNQ], g, memory_order_relaxed);
        atomic_store_explicit(&p->tail, tail + 1, memory_order_release);
        return;
    }
    tzr_runqput_slow(p, g, head, tail);
}

static tzr_g *tzr_runnext_take(tzr_p *p) {
    tzr_g *next = atomic_load_explicit(&p->runnext, memory_order_relaxed);
    if (next != NULL && atomic_compare_exchange_strong_explicit(&p->runnext, &next, NULL, memory_order_acq_rel, memory_order_relaxed))
        return next;
    return NULL;
}

/* runnext first, but after TZR_RUNNEXT_TURNS takes in a row the ring goes first once: two routines
 * that keep waking each other into runnext (a rendezvous on one P) cannot starve the ring. */
static tzr_g *tzr_runqget(tzr_p *p) {
    tzr_g *next;
    if (p->runnext_streak < TZR_RUNNEXT_TURNS && (next = tzr_runnext_take(p))) {
        p->runnext_streak++;
        return next;
    }
    p->runnext_streak = 0;
    for (;;) {
        uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
        uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
        if (tail == head) break;
        tzr_g *g = atomic_load_explicit(&p->runq[head % TZR_RUNQ], memory_order_relaxed);
        if (atomic_compare_exchange_weak_explicit(&p->head, &head, head + 1, memory_order_acq_rel, memory_order_relaxed)) return g;
    }
    return tzr_runnext_take(p); /* the ring was empty */
}

static int tzr_runq_empty(tzr_p *p) {
    uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_acquire);
    return head == tail && atomic_load_explicit(&p->runnext, memory_order_acquire) == NULL;
}

/* Moves about half of victim's queue to p's queue and returns one G. */
static tzr_g *tzr_runqsteal(tzr_p *p, tzr_p *victim, int steal_next) {
    tzr_g *batch[TZR_RUNQ / 2];
    uint32_t n;
    for (;;) {
        uint32_t head = atomic_load_explicit(&victim->head, memory_order_acquire);
        uint32_t tail = atomic_load_explicit(&victim->tail, memory_order_acquire);
        n = tail - head;
        n = n - n / 2;
        if (n == 0) {
            if (steal_next) {
                tzr_g *next = atomic_load_explicit(&victim->runnext, memory_order_relaxed);
                if (next != NULL && atomic_compare_exchange_strong_explicit(&victim->runnext, &next, NULL, memory_order_acq_rel, memory_order_relaxed))
                    return next;
            }
            return NULL;
        }
        if (n > TZR_RUNQ / 2) continue; /* inconsistent snapshot */
        for (uint32_t i = 0; i < n; i++) batch[i] = atomic_load_explicit(&victim->runq[(head + i) % TZR_RUNQ], memory_order_relaxed);
        if (atomic_compare_exchange_weak_explicit(&victim->head, &head, head + n, memory_order_acq_rel, memory_order_relaxed)) break;
    }
    tzr_g *g = batch[--n];
    if (n == 0) return g;
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
    for (uint32_t i = 0; i < n; i++) atomic_store_explicit(&p->runq[(tail + i) % TZR_RUNQ], batch[i], memory_order_relaxed);
    atomic_store_explicit(&p->tail, tail + n, memory_order_release);
    return g;
}

static tzr_g *tzr_globrunqget(tzr_p *p, int max) {
    /* tzr_sched.lock held */
    int64_t size = atomic_load(&tzr_sched.runq_size);
    if (size == 0) return NULL;
    int64_t n = size / tzr_sched.nprocs + 1;
    if (n > size) n = size;
    if (max > 0 && n > max) n = max;
    if (n > TZR_RUNQ / 2) n = TZR_RUNQ / 2;
    tzr_g *g = tzr_sched.runq_head;
    tzr_sched.runq_head = g->link;
    if (tzr_sched.runq_head == NULL) tzr_sched.runq_tail = NULL;
    atomic_fetch_sub(&tzr_sched.runq_size, n);
    for (int64_t i = 1; i < n; i++) {
        tzr_g *more = tzr_sched.runq_head;
        tzr_sched.runq_head = more->link;
        if (tzr_sched.runq_head == NULL) tzr_sched.runq_tail = NULL;
        tzr_runqput(p, more, 0);
    }
    return g;
}

/* ---------------------------------------------------------------- Ps and Ms */

static tzr_p *tzr_pidleget(void) {
    tzr_p *p = tzr_sched.pidle;
    if (p) {
        tzr_sched.pidle = p->link;
        atomic_fetch_sub(&tzr_sched.npidle, 1);
    }
    return p;
}

/* tzr_sched.lock held (or the scheduler not started). An M that is back from a blocking call and
 * waits for a P takes it first. */
static void tzr_pidleput(tzr_p *p) {
    tzr_m *waiter = tzr_sched.pwait;
    if (waiter) {
        tzr_sched.pwait = waiter->link;
        atomic_fetch_sub(&tzr_sched.npwait, 1);
        waiter->nextp = p;
        tzr_note_wake(&waiter->park);
        return;
    }
    p->link = tzr_sched.pidle;
    tzr_sched.pidle = p;
    atomic_fetch_add(&tzr_sched.npidle, 1);
}

static void *tzr_mstart(void *arg);

/* Starts an M with a P (an idle one when p is NULL). tzr_sched.lock not held. */
static void tzr_startm(tzr_p *p, int spinning) {
    tzr_mutex_lock(&tzr_sched.lock);
    if (p == NULL) {
        p = tzr_pidleget();
        if (p == NULL) {
            if (spinning) atomic_fetch_sub(&tzr_sched.nmspinning, 1); /* before another P can be seen idle */
            tzr_mutex_unlock(&tzr_sched.lock);
            return;
        }
    }
    tzr_m *m = tzr_sched.midle;
    if (m) {
        tzr_sched.midle = m->link;
        tzr_sched.nmidle--;
        m->nextp = p;
        m->spinning = spinning;
        tzr_mutex_unlock(&tzr_sched.lock);
        tzr_note_wake(&m->park);
        return;
    }
    m = calloc(1, sizeof *m);
    if (m == NULL) tzr_fatal("out of memory starting a scheduler thread");
    tzr_note_init(&m->park);
    m->nextp = p;
    m->spinning = spinning;
    m->rand = 0x9E3779B97F4A7C15ULL ^ (uint64_t)(uintptr_t)m;
    tzr_sched.mcount++;
    tzr_mutex_unlock(&tzr_sched.lock);
    tzr_thread_start(tzr_mstart, m, 256 * 1024, "cannot start a scheduler thread"); /* g0 runs only the scheduler */
}

static void tzr_wakep(void) {
    if (atomic_load(&tzr_sched.npidle) == 0) return;
    int expected = 0;
    if (atomic_load(&tzr_sched.nmspinning) != 0 || !atomic_compare_exchange_strong(&tzr_sched.nmspinning, &expected, 1)) return;
    tzr_startm(NULL, 1);
}

static void tzr_checkdead(void);

/* Parks the M until tzr_startm gives it a P. tzr_sched.lock held on entry, not on return. */
static void tzr_stopm(tzr_m *m) {
    m->link = tzr_sched.midle;
    tzr_sched.midle = m;
    tzr_sched.nmidle++;
    tzr_checkdead();
    tzr_mutex_unlock(&tzr_sched.lock);
    tzr_note_sleep(&m->park);
    m->p = m->nextp;
    m->nextp = NULL;
}

/* ---------------------------------------------------------------- deadlock */

static tzr_g *tzr_pseudo_list; /* every pseudo-G, linked through `link` */
static tzr_lock tzr_pseudo_lock;

static void tzr_report_deadlock(void) {
    /* tzr_sched.lock held */
    int64_t waiting = 0;
    char reasons[512] = "";
    tzr_lock_acquire(&tzr_sched.allgs_lock);
    for (tzr_g *g = tzr_sched.allgs; g; g = g->all_next) {
        if (atomic_load(&g->status) != TZR_G_WAITING) continue;
        waiting++;
        if (strlen(reasons) < 400 && g->wait_reason && !strstr(reasons, g->wait_reason)) {
            if (reasons[0]) strcat(reasons, ", ");
            strcat(reasons, g->wait_reason);
        }
    }
    tzr_lock_release(&tzr_sched.allgs_lock);
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g *g = tzr_pseudo_list; g; g = g->link) {
        if (atomic_load(&g->status) != TZR_G_WAITING) continue;
        if (strlen(reasons) < 400 && g->wait_reason && !strstr(reasons, g->wait_reason)) {
            if (reasons[0]) strcat(reasons, ", ");
            strcat(reasons, g->wait_reason);
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    fprintf(stderr, "Tsuzuri runtime: all tzroutines are asleep - deadlock! (%lld waiting%s%s)\n",
            (long long)(waiting + tzr_sched.npseudo_parked), reasons[0] ? ": " : "", reasons);
    fflush(stderr);
}

/* Called with tzr_sched.lock held whenever an M goes idle or a pseudo-G parks. */
static void tzr_checkdead(void) {
    if (tzr_sched.deadlocked) return;
    int running = tzr_sched.mcount - tzr_sched.nmidle - tzr_sched.nmblocked + tzr_sched.npseudo_running;
    if (running > 0) return;
    if (atomic_load(&tzr_sched.runq_size) > 0) return;
    if (atomic_load(&tzr_sched.ntimers) > 0) return;
    if (tzr_sched.npending > 0) return; /* an item that has not started may wake a waiter */
    if (atomic_load(&tzr_sched.ngoroutines) == 0 && tzr_sched.npseudo_parked == 0) return;
    for (int i = 0; i < tzr_sched.nprocs; i++)
        if (!tzr_runq_empty(tzr_sched.allp[i])) return;
    tzr_sched.deadlocked = 1;
    tzr_report_deadlock();
    /* Wake every parked pseudo-G with doomed = 1: its operation returns the deadlock status and the
     * generated code traps at that call site. With no pseudo-G parked, trap here. */
    int woke = 0;
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g *g = tzr_pseudo_list; g; g = g->link) {
        if (atomic_load(&g->status) == TZR_G_WAITING) {
            g->doomed = 1;
            atomic_store(&g->status, TZR_G_RUNNING);
            tzr_sched.npseudo_parked--;
            tzr_sched.npseudo_running++;
            tzr_note_wake(&g->note);
            woke++;
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    if (!woke) __builtin_trap();
    /* The woken pseudo-Gs run now; a host that handles the status may wait again, and a later
     * deadlock must be found again. */
    tzr_sched.deadlocked = 0;
}

/* ---------------------------------------------------------------- scheduling */

static void tzr_execute(tzr_m *m, tzr_g *g) {
    m->curg = g;
    m->p->schedtick++;
    atomic_store(&g->status, TZR_G_RUNNING);
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(g->tsan_fiber);
#endif
#if defined(TZR_ASAN)
    void *fake = NULL;
    TZR_ASAN_START(&fake, g->stack + TZR_GUARD, g->stack_size);
#endif
    TZR_TO_G(m, g);
#if defined(TZR_ASAN)
    TZR_ASAN_FINISH(fake, NULL, NULL);
#endif
    /* back on g0: the G parked, yielded or exited */
    tzr_g *prev = m->curg;
    m->curg = NULL;
    void (*after)(tzr_m *, tzr_g *) = m->after;
    m->after = NULL;
    if (after) after(m, prev);
}

static tzr_g *tzr_findrunnable(tzr_m *m) {
    for (;;) {
        tzr_p *p = m->p;
        tzr_g *g;
        if (p->schedtick % 61 == 0 && atomic_load(&tzr_sched.runq_size) > 0) {
            tzr_mutex_lock(&tzr_sched.lock);
            g = tzr_globrunqget(p, 1);
            tzr_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
        }
        if ((g = tzr_runqget(p))) return g;
        if (atomic_load(&tzr_sched.runq_size) > 0) {
            tzr_mutex_lock(&tzr_sched.lock);
            g = tzr_globrunqget(p, 0);
            tzr_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
        }
        /* steal, unless enough Ms are already spinning */
        int busy = tzr_sched.nprocs - atomic_load(&tzr_sched.npidle);
        if (m->spinning || 2 * atomic_load(&tzr_sched.nmspinning) < busy) {
            if (!m->spinning) {
                m->spinning = 1;
                atomic_fetch_add(&tzr_sched.nmspinning, 1);
            }
            for (int round = 0; round < 4; round++) {
                int start = (int)(tzr_rand(&m->rand) % (uint64_t)tzr_sched.nprocs);
                for (int i = 0; i < tzr_sched.nprocs; i++) {
                    tzr_p *victim = tzr_sched.allp[(start + i) % tzr_sched.nprocs];
                    if (victim == p) continue;
                    if ((g = tzr_runqsteal(p, victim, round == 3))) return g;
                }
            }
        }
        /* nothing: give the P back and sleep */
        tzr_mutex_lock(&tzr_sched.lock);
        if (atomic_load(&tzr_sched.runq_size) > 0) {
            g = tzr_globrunqget(p, 0);
            tzr_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
            continue;
        }
        tzr_pidleput(p);
        m->p = NULL;
        if (m->spinning) { /* before the P can be seen idle */
            m->spinning = 0;
            atomic_fetch_sub(&tzr_sched.nmspinning, 1);
        }
        tzr_mutex_unlock(&tzr_sched.lock);
        /* A G may have been queued after our last look: before nmspinning dropped, or by the last
         * spinner, which looked for an idle P before this one was published. Look again, spinning
         * or not, or nobody would wake for it. */
        for (int i = 0; i < tzr_sched.nprocs; i++) {
            if (!tzr_runq_empty(tzr_sched.allp[i]) || atomic_load(&tzr_sched.runq_size) > 0) {
                tzr_mutex_lock(&tzr_sched.lock);
                tzr_p *again = tzr_pidleget();
                tzr_mutex_unlock(&tzr_sched.lock);
                if (again) {
                    m->p = again;
                    m->spinning = 1;
                    atomic_fetch_add(&tzr_sched.nmspinning, 1);
                }
                break;
            }
        }
        if (m->p) continue;
        tzr_mutex_lock(&tzr_sched.lock);
        tzr_stopm(m); /* returns with m->p set */
    }
}

/* An M back from a blocking call waits for a P, which only an M that runs out of work gives back:
 * one that keeps finding work, say a tzroutine that yields until the waiting one sets a flag, would
 * keep its P for ever. So between two Gs an M hands its P (with its queue) to a waiting M and
 * stops; the G it ran last is in a queue already. An M that tzr_wakep started is still spinning
 * here: it stops spinning before it parks, or tzr_wakep would never start an M again, and as the
 * last spinner it wakes another for the work that it was started for, as tzr_schedule does. With
 * tzr_sched.lock not held. */
static void tzr_serve_pwait(tzr_m *m) {
    if (atomic_load(&tzr_sched.npwait) == 0) return;
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_m *waiter = tzr_sched.pwait;
    if (waiter == NULL) {
        tzr_mutex_unlock(&tzr_sched.lock);
        return;
    }
    tzr_sched.pwait = waiter->link;
    atomic_fetch_sub(&tzr_sched.npwait, 1);
    waiter->nextp = m->p;
    m->p = NULL;
    tzr_note_wake(&waiter->park);
    if (m->spinning) {
        m->spinning = 0;
        if (atomic_fetch_sub(&tzr_sched.nmspinning, 1) == 1) {
            /* tzr_startm takes the lock; meanwhile this M, neither idle nor blocked, counts as
             * running for tzr_checkdead */
            tzr_mutex_unlock(&tzr_sched.lock);
            tzr_wakep();
            tzr_mutex_lock(&tzr_sched.lock);
        }
    }
    tzr_stopm(m); /* returns with a P */
}

static void tzr_schedule(tzr_m *m) {
    for (;;) {
        tzr_serve_pwait(m);
        tzr_g *g = tzr_findrunnable(m);
        if (m->spinning) {
            m->spinning = 0;
            if (atomic_fetch_sub(&tzr_sched.nmspinning, 1) == 1) tzr_wakep();
        }
        tzr_execute(m, g);
    }
}

static void *tzr_mstart(void *arg) {
    tzr_m *m = arg;
    tzr_tls_m = m;
#if defined(TZR_TSAN)
    m->tsan_g0 = __tsan_get_current_fiber();
#endif
#if defined(_WIN32)
    m->g0_fiber = ConvertThreadToFiberEx(NULL, FIBER_FLAG_FLOAT_SWITCH);
    if (m->g0_fiber == NULL) tzr_fatal("cannot make a scheduler thread a fiber");
#endif
#if defined(TZ_STACK_GUARD)
    tsuzuri_stack_scheduler_thread(); /* the alternate stack on which stack.c reports an overflow */
#endif
    m->p = m->nextp;
    m->nextp = NULL;
    tzr_schedule(m);
    return NULL;
}

/* A whole decimal number in [low, high] from the environment, or `fallback` for anything else. */
static long tzr_env(const char *name, long low, long high, long fallback) {
    const char *text = getenv(name);
    if (text == NULL || *text == '\0') return fallback;
    char *end = NULL;
    errno = 0;
    long n = strtol(text, &end, 10);
    if (errno != 0 || end == text || *end != '\0' || n < low || n > high) return fallback;
    return n;
}

/* Time.sleep_ms (src/runtime/os.c) on a tzroutine waits like Time.sleep instead of stopping the M's
 * thread (TZ05). os.c and this file are in one translation unit when both are used, so the hook has
 * one definition; a program without tzroutines leaves it null. os.c has no Windows version. */
#if !defined(_WIN32)
#ifndef TSUZURI_OS_SLEEP_HOOK_DEFINED
#define TSUZURI_OS_SLEEP_HOOK_DEFINED
__attribute__((weak, visibility("hidden"))) int32_t (*_Atomic tsuzuri_os_sleep_hook)(int64_t);
#endif
static int32_t tzr_sleep_if_routine(int64_t ns);
#endif

static void tzr_init(void) {
    long cpus = tzr_online_cpus();
    int nprocs = cpus < 1 ? 1 : (cpus > TZR_MAXPROCS ? TZR_MAXPROCS : (int)cpus);
    nprocs = (int)tzr_env("TSUZURI_MAXPROCS", 1, TZR_MAXPROCS, nprocs);
    size_t page = tzr_page_size();
    size_t stack = (size_t)tzr_env("TSUZURI_TZROUTINE_STACK_KIB", 64, 1048576, TZR_DEFAULT_STACK / 1024) * 1024;
    tzr_sched.stack_size = (stack + page - 1) / page * page;
    tzr_sched.nprocs = nprocs;
#if defined(TZ_STACK_GUARD)
    atomic_store_explicit(&tsuzuri_stack_guard_hook, tsuzuri_tz_stack_guard_hit, memory_order_release);
#endif
#if !defined(_WIN32)
    atomic_store_explicit(&tsuzuri_os_sleep_hook, tzr_sleep_if_routine, memory_order_release);
#endif
    for (int i = nprocs - 1; i >= 0; i--) {
        tzr_p *p = calloc(1, sizeof *p);
        if (p == NULL) tzr_fatal("out of memory starting the scheduler");
        p->id = i;
        tzr_sched.allp[i] = p;
        tzr_pidleput(p);
    }
}

static void tzr_start(void) { tzr_run_once(&tzr_sched.once, tzr_init); }

/* ---------------------------------------------------------------- pseudo-G */

static tzr_once tzr_pseudo_key_once = TZR_ONCE_INIT;

/* A thread that ends takes its pseudo-G with it. It cannot be parked or inside a scope then: a
 * host thread returns from the export only after tsuzuri_tz_scope_leave. */
static void tzr_pseudo_destroy(void *arg) {
    tzr_g *g = arg;
    tzr_tls_pseudo = NULL; /* a later destructor of this thread that calls in gets a fresh pseudo-G */
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g **link = &tzr_pseudo_list; *link; link = &(*link)->link) {
        if (*link == g) {
            *link = g->link;
            break;
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    tzr_mutex_lock(&tzr_sched.lock);
    if (!g->idle) tzr_sched.npseudo_running--;
    tzr_checkdead(); /* the last running thread may leave only parked participants */
    tzr_mutex_unlock(&tzr_sched.lock);
    tzr_note_destroy(&g->note);
    free(g->held);
    free(g);
}

#if defined(_WIN32)
/* The pseudo-G of a thread goes when the thread ends: the loader calls the TLS callbacks of the
 * module with DLL_THREAD_DETACH on the ending thread, whatever fibers the thread ran (a pseudo-G is
 * the thread's). It does not call them when the process exits (that is DLL_PROCESS_DETACH, after
 * ExitProcess has stopped the other threads, one of which may hold a lock), so nothing is freed
 * then. The section is in the TLS directory of the CRT, which _Thread_local links in. */
static void NTAPI tzr_tls_callback(PVOID module, DWORD reason, PVOID reserved) {
    (void)module;
    (void)reserved;
    if (reason == DLL_THREAD_DETACH && tzr_tls_pseudo != NULL) tzr_pseudo_destroy(tzr_tls_pseudo);
}
__attribute__((section(".CRT$XLB"), used)) static const PIMAGE_TLS_CALLBACK tzr_tls_callback_entry = tzr_tls_callback;
static void tzr_pseudo_key_init(void) {}
static void tzr_pseudo_key_set(tzr_g *g) { (void)g; }
#else
static pthread_key_t tzr_pseudo_key;
static void tzr_pseudo_key_init(void) {
    if (pthread_key_create(&tzr_pseudo_key, tzr_pseudo_destroy) != 0) tzr_fatal("cannot create a thread key");
}
static void tzr_pseudo_key_set(tzr_g *g) { pthread_setspecific(tzr_pseudo_key, g); }
#endif

static tzr_g *tzr_pseudo(void) {
    tzr_g *g = tzr_tls_pseudo;
    if (g) return g;
    tzr_start();
    tzr_run_once(&tzr_pseudo_key_once, tzr_pseudo_key_init);
    g = calloc(1, sizeof *g);
    if (g == NULL) tzr_fatal("out of memory");
    g->kind = TZR_KIND_THREAD;
    atomic_store(&g->status, TZR_G_RUNNING);
    g->rand = 0x2545F4914F6CDD1DULL ^ (uint64_t)(uintptr_t)g;
    g->id = atomic_fetch_add(&tzr_sched.next_id, 1) + 1;
    tzr_note_init(&g->note);
    tzr_lock_acquire(&tzr_pseudo_lock);
    g->link = tzr_pseudo_list;
    tzr_pseudo_list = g;
    tzr_lock_release(&tzr_pseudo_lock);
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_sched.npseudo_running++;
    tzr_mutex_unlock(&tzr_sched.lock);
    tzr_tls_pseudo = g;
    tzr_pseudo_key_set(g);
    return g;
}

/* ---------------------------------------------------------------- park and ready */

static void tzr_after_park(tzr_m *m, tzr_g *g) {
    (void)g;
    void (*unlock)(void *) = m->after_unlock;
    void *arg = m->after_arg;
    m->after_unlock = NULL;
    if (unlock) unlock(arg);
}

/* G -> g0 for a G that will be resumed (park, yield). Under ASan the context that resumes takes its
 * fake stack back and learns the stack of the g0 that resumed it, which may be another M's (R2). */
static void tzr_switch_out(tzr_m *m, tzr_g *g) {
#if defined(TZR_ASAN)
    void *fake = NULL;
    TZR_ASAN_START(&fake, m->g0_bottom, m->g0_size);
#endif
    TZR_TO_G0(m, g);
#if defined(TZR_ASAN)
    m = tzr_this_m();
    TZR_ASAN_FINISH(fake, &m->g0_bottom, &m->g0_size);
#endif
}

/* Parks the current G until tzr_ready. `unlock(arg)` runs once the G is off its stack (R1).
 * Returns 1 only for a pseudo-G that the deadlock detector woke. */
static int tzr_park(void (*unlock)(void *), void *arg, const char *reason) {
    tzr_g *g = tzr_current();
    g->wait_reason = reason;
    if (g->kind == TZR_KIND_THREAD) {
        tzr_mutex_lock(&tzr_sched.lock);
        atomic_store(&g->status, TZR_G_WAITING);
        tzr_sched.npseudo_running--;
        tzr_sched.npseudo_parked++;
        tzr_mutex_unlock(&tzr_sched.lock);
        if (unlock) unlock(arg);
        tzr_mutex_lock(&tzr_sched.lock);
        tzr_checkdead();
        tzr_mutex_unlock(&tzr_sched.lock);
        tzr_note_sleep(&g->note);
        int doomed = g->doomed;
        g->doomed = 0; /* observed: the next wait starts afresh */
        return doomed;
    }
    tzr_m *m = tzr_this_m();
    atomic_store(&g->status, TZR_G_WAITING);
    m->after = tzr_after_park;
    m->after_unlock = unlock;
    m->after_arg = arg;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
    tzr_switch_out(m, g);
    return 0;
}

static void tzr_ready(tzr_g *g) {
    if (g->kind == TZR_KIND_THREAD) {
        tzr_mutex_lock(&tzr_sched.lock);
        if (atomic_load(&g->status) != TZR_G_WAITING) {
            tzr_mutex_unlock(&tzr_sched.lock);
            return; /* doomed already */
        }
        atomic_store(&g->status, TZR_G_RUNNING);
        tzr_sched.npseudo_parked--;
        tzr_sched.npseudo_running++;
        tzr_mutex_unlock(&tzr_sched.lock);
        tzr_note_wake(&g->note);
        return;
    }
    int expected = TZR_G_WAITING;
    if (!atomic_compare_exchange_strong(&g->status, &expected, TZR_G_RUNNABLE)) tzr_fatal("bad tzroutine status in ready");
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->p != NULL) {
        tzr_runqput(m->p, g, 1);
    } else {
        tzr_mutex_lock(&tzr_sched.lock);
        tzr_globrunqput(g);
        tzr_mutex_unlock(&tzr_sched.lock);
    }
    tzr_wakep();
}

/* Wakes g, which a waker took out of a queue that `lock` guards, and releases the lock. A pseudo-G
 * is woken with the lock still held (R3 allows it): one that the deadlock detector woke meanwhile
 * takes the lock to cancel its wait, and so cannot go on before this wake, which is then a no-op. */
static void tzr_ready_unlock(tzr_g *g, tzr_lock *lock) {
    if (g->kind == TZR_KIND_THREAD) {
        tzr_ready(g);
        tzr_lock_release(lock);
    } else {
        tzr_lock_release(lock);
        tzr_ready(g);
    }
}

static void tzr_after_yield(tzr_m *m, tzr_g *g) {
    (void)m;
    atomic_store(&g->status, TZR_G_RUNNABLE);
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_globrunqput(g);
    tzr_mutex_unlock(&tzr_sched.lock);
}

TZR_API void tsuzuri_tz_yield(void) {
    tzr_m *m = tzr_this_m();
    if (m == NULL || m->curg == NULL || m->blocked) {
        tzr_os_yield();
        return;
    }
    tzr_g *g = m->curg;
    m->after = tzr_after_yield;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
    tzr_switch_out(m, g);
}

/* ---------------------------------------------------------------- spawn and exit */

static void tzr_after_exit(tzr_m *m, tzr_g *g) {
    atomic_store(&g->status, TZR_G_DEAD);
    tzr_scope *scope = g->scope;
    g->scope = NULL;
    g->spawn_scope = NULL;
    g->code = NULL;
    g->env = NULL;
    /* keep the G and its stack for the next spawn */
    tzr_p *p = m->p;
    int local = 0;
    if (p->gfree_count < TZR_P_GFREE) { /* untrimmed: bounded for the whole process */
        local = atomic_fetch_add(&tzr_sched.local_cached, 1) < TZR_LOCAL_CACHE;
        if (!local) atomic_fetch_sub(&tzr_sched.local_cached, 1);
    }
    if (local) {
        g->link = p->gfree;
        p->gfree = g;
        p->gfree_count++;
    } else {
        tzr_stack_trim(g);
        tzr_lock_acquire(&tzr_sched.gfree_lock);
        int keep = tzr_sched.gfree_count < TZR_STACK_CACHE;
        if (keep) {
            g->link = tzr_sched.gfree;
            tzr_sched.gfree = g;
            tzr_sched.gfree_count++;
        }
        tzr_lock_release(&tzr_sched.gfree_lock);
        if (!keep) {
            tzr_lock_acquire(&tzr_sched.allgs_lock);
            if (g->all_prev) g->all_prev->all_next = g->all_next;
            else tzr_sched.allgs = g->all_next;
            if (g->all_next) g->all_next->all_prev = g->all_prev;
            tzr_lock_release(&tzr_sched.allgs_lock);
#if defined(_WIN32)
            DeleteFiber(g->fiber); /* not running: this is g0 */
#else
            munmap(g->stack, TZR_GUARD + g->stack_size);
#endif
#if defined(TZR_TSAN)
            __tsan_destroy_fiber(g->tsan_fiber);
#endif
            free(g->held);
            free(g);
        }
    }
    tzr_g *waiter = NULL;
    tzr_lock_acquire(&scope->lock);
    /* Before the scope can be seen empty: a join that returns sees the exact count. */
    atomic_fetch_sub(&tzr_sched.ngoroutines, 1);
    if (--scope->live == 0) {
        waiter = scope->waiter;
        scope->waiter = NULL;
    }
    if (waiter) tzr_ready_unlock(waiter, &scope->lock);
    else tzr_lock_release(&scope->lock);
}

#if defined(_WIN32)
/* A G's fiber runs one tzroutine after another: tsuzuri_tz_go reuses a finished G by replacing its
 * code and env, and the next TZR_TO_G returns from the TZR_TO_G0 below into the next iteration. */
static VOID CALLBACK tzr_fiber_start(LPVOID arg) {
    tzr_g *g = arg;
    for (;;) {
        fesetenv(FE_DFL_ENV); /* each routine starts with the default rounding and exception masks */
#if defined(_M_X64) || defined(__x86_64__)
        __asm__ volatile("fnclex"); /* and no x87 exception flag of the routine before */
#endif
        g->code(g->env);
        tzr_m *m = tzr_this_m(); /* R2: the body may have moved this G to another M */
        m->after = tzr_after_exit;
        TZR_TO_G0(m, g);
    }
}
#else
static void tzr_gstart(void) {
    tzr_m *m = tzr_this_m();
#if defined(TZR_ASAN)
    TZR_ASAN_FINISH(NULL, &m->g0_bottom, &m->g0_size); /* first entry: learn the stack of g0 */
#endif
    tzr_g *g = m->curg;
    g->code(g->env);
    m = tzr_this_m(); /* R2: the body may have moved this G to another M */
    m->after = tzr_after_exit;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
#if defined(TZR_ASAN)
    TZR_ASAN_START(NULL, m->g0_bottom, m->g0_size); /* this use of the stack ends here */
#endif
    TZR_TO_G0(m, g);
    __builtin_trap();
}
#endif

static tzr_scope tzr_root_scope = {.boundary = &tzr_root_scope};

static tzr_g *tzr_galloc(void) {
    tzr_m *m = tzr_this_m();
    tzr_g *g = NULL;
    if (m && m->p && m->p->gfree) {
        g = m->p->gfree;
        m->p->gfree = g->link;
        m->p->gfree_count--;
        atomic_fetch_sub(&tzr_sched.local_cached, 1);
        return g;
    }
    tzr_lock_acquire(&tzr_sched.gfree_lock);
    if (tzr_sched.gfree) {
        g = tzr_sched.gfree;
        tzr_sched.gfree = g->link;
        tzr_sched.gfree_count--;
    }
    tzr_lock_release(&tzr_sched.gfree_lock);
    if (g) return g;
    g = calloc(1, sizeof *g);
    if (g == NULL) return NULL;
#if defined(_WIN32)
    g->fiber = tzr_fiber_create(tzr_sched.stack_size, tzr_fiber_start, g);
    if (g->fiber == NULL) {
        free(g);
        return NULL;
    }
#else
    g->stack = tzr_stack_map(tzr_sched.stack_size);
    if (g->stack == NULL) {
        free(g);
        return NULL;
    }
#endif
    g->stack_size = tzr_sched.stack_size;
    g->kind = TZR_KIND_ROUTINE;
#if defined(TZR_TSAN)
    g->tsan_fiber = __tsan_create_fiber(0);
#endif
    g->rand = 0x9E3779B97F4A7C15ULL ^ (uint64_t)(uintptr_t)g;
    tzr_lock_acquire(&tzr_sched.allgs_lock);
    g->all_next = tzr_sched.allgs;
    g->all_prev = NULL;
    if (tzr_sched.allgs) tzr_sched.allgs->all_prev = g;
    tzr_sched.allgs = g;
    tzr_lock_release(&tzr_sched.allgs_lock);
    return g;
}

/* The scope that counts what `g` spawns: its spawn scope, the scope it is counted in, or the root. */
static tzr_scope *tzr_spawn_scope(tzr_g *g) {
    tzr_scope *scope = g->spawn_scope ? g->spawn_scope : g->scope;
    return scope ? scope : &tzr_root_scope;
}

/* Counts one tzroutine in `scope` before it exists: an after_func timer that fires reserves its
 * tzroutine under the timers' lock, so that a boundary that stops the timers next sees it (TZ05). */
static void tzr_spawn_reserve(tzr_scope *scope) {
    tzr_lock_acquire(&scope->lock);
    scope->live++;
    tzr_lock_release(&scope->lock);
    atomic_fetch_add(&tzr_sched.ngoroutines, 1);
}

/* Starts `code(env)` on a new G counted in `scope` (already counted when `reserved`). It does not
 * ask for the caller's G, so the timer thread spawns without becoming a pseudo-G. */
static void tzr_spawn(tzr_scope *scope, void (*code)(void *), void *env, int reserved) {
    tzr_g *g = tzr_galloc();
    if (g == NULL) tzr_fatal("cannot allocate a tzroutine stack (out of memory or address space)");
    g->code = code;
    g->env = env;
    g->scope = scope;
    g->id = atomic_fetch_add(&tzr_sched.next_id, 1) + 1;
    g->wait_reason = NULL;
#if !defined(_WIN32)
    g->sp = tzr_initial_frame(g->stack + TZR_GUARD + g->stack_size, tzr_gstart);
#endif
    if (!reserved) tzr_spawn_reserve(scope);
    atomic_store(&g->status, TZR_G_RUNNABLE);
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->p != NULL) {
        tzr_runqput(m->p, g, 1);
    } else {
        tzr_mutex_lock(&tzr_sched.lock);
        tzr_globrunqput(g);
        tzr_mutex_unlock(&tzr_sched.lock);
    }
    tzr_wakep();
}

TZR_API void tsuzuri_tz_go(void (*code)(void *), void *env) {
    tzr_start();
    tzr_spawn(tzr_spawn_scope(tzr_current()), code, env, 0);
}

/* ---------------------------------------------------------------- scopes */

/* A scope of a boundary (main, an export call, a test, a bench sample, a pool worker's item): the
 * timers made in it are stopped at its end (TZ05). */
TZR_API void *tsuzuri_tz_scope_enter(void) {
    tzr_scope *scope = calloc(1, sizeof *scope);
    if (scope == NULL) tzr_fatal("out of memory");
    scope->boundary = scope;
    tzr_g *g = tzr_current();
    scope->outer = g->spawn_scope;
    g->spawn_scope = scope;
    return scope;
}

static void tzr_unlock_scope(void *arg) { tzr_lock_release(&((tzr_scope *)arg)->lock); }

/* Waits until every tzroutine spawned in `scope` (and their descendants) has finished. Returns 1
 * when the deadlock detector ended the wait. */
static int tzr_scope_wait(tzr_scope *scope) {
    for (;;) {
        tzr_lock_acquire(&scope->lock);
        if (scope->live == 0) {
            tzr_lock_release(&scope->lock);
            return 0;
        }
        tzr_g *g = tzr_current();
        scope->waiter = g;
        if (tzr_park(tzr_unlock_scope, scope, "end of scope (waiting for tzroutines)")) {
            /* the detector woke it: cancel the wait, unless the last tzroutine ended meanwhile */
            tzr_lock_acquire(&scope->lock);
            if (scope->waiter == g) scope->waiter = NULL;
            int ended = scope->live == 0;
            tzr_lock_release(&scope->lock);
            if (!ended) return 1;
        }
    }
    return 0;
}

TZR_API int32_t tsuzuri_tz_scope_leave(void *opaque) {
    tzr_scope *scope = opaque;
    int doomed = tzr_scope_wait(scope);
    tzr_current()->spawn_scope = scope->outer;
    if (!doomed) free(scope);
    return doomed;
}

static int tzr_boundary_settle(tzr_scope *scope);

/* The end of a boundary: waits for the scope's tzroutines (TZ02), then stops the timers made in it
 * that have not fired, without waiting for them (TZ05). A fired after_func spawns into the scope,
 * and the drop of a stopped one runs code, so it waits again until a pass finds nothing. A deadlock
 * is reported by the detector; the boundary has no source position of its own, so it traps here. */
TZR_API void tsuzuri_tz_boundary_leave(void *opaque) {
    tzr_scope *scope = opaque;
    do {
        if (tzr_scope_wait(scope)) __builtin_trap();
    } while (tzr_boundary_settle(scope));
    tzr_current()->spawn_scope = scope->outer;
    free(scope);
}

/* ---------------------------------------------------------------- blocking calls */

/* With tzr_sched.lock held: a pseudo-G counts as running or not. A G is on an M, which counts. */
static void tzr_set_running(tzr_g *g, int running) {
    if (g == NULL || g->kind != TZR_KIND_THREAD || g->idle == !running) return;
    g->idle = !running;
    tzr_sched.npseudo_running += running ? 1 : -1;
}


/* A G that calls into code which blocks its OS thread without this runtime's knowledge (the join
 * of the task runtime's Task.parallel, Task.scope and Parallel.*) gives its P away for the call, as
 * Go's entersyscallblock does, so that what is queued, and the work that the call waits for (the
 * child tzroutine of a pool worker), runs even with one P. That code may call back into Tsuzuri on
 * this thread (the task runtime runs an item on the thread that submits it): until the call
 * returns, the thread acts as a pseudo-G, whose waits stop the thread and never switch the G away,
 * so the code of the call stays on its thread. After the call the M takes an idle P, or waits until
 * an M gives one back. Calls nest; a pseudo-G does nothing here. */
TZR_API void tsuzuri_tz_block_enter(void) {
    tzr_m *m = tzr_this_m();
    if (m == NULL || m->curg == NULL || m->blocked++ > 0) return;
    tzr_p *p = m->p;
    m->p = NULL;
    tzr_g *thread = tzr_pseudo(); /* counted as running from here; a new one is */
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_sched.nmblocked++;
    tzr_set_running(thread, 1);
    if (!tzr_runq_empty(p) || atomic_load(&tzr_sched.runq_size) > 0) {
        tzr_mutex_unlock(&tzr_sched.lock);
        tzr_startm(p, 0);
        return;
    }
    tzr_pidleput(p);
    tzr_mutex_unlock(&tzr_sched.lock);
    /* a G queued before the P was seen idle would wait for nobody */
    if (atomic_load(&tzr_sched.runq_size) > 0) tzr_wakep();
}

TZR_API void tsuzuri_tz_block_leave(void) {
    tzr_m *m = tzr_this_m();
    if (m == NULL || m->curg == NULL || --m->blocked > 0) return;
    tzr_g *thread = tzr_pseudo();
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_set_running(thread, 0);
    tzr_sched.nmblocked--; /* the M counts again, as running */
    tzr_p *p = tzr_pidleget();
    if (p == NULL) {
        m->link = tzr_sched.pwait;
        tzr_sched.pwait = m;
        atomic_fetch_add(&tzr_sched.npwait, 1);
        tzr_mutex_unlock(&tzr_sched.lock);
        tzr_note_sleep(&m->park);
        p = m->nextp;
        m->nextp = NULL;
    } else {
        tzr_mutex_unlock(&tzr_sched.lock);
    }
    m->p = p;
}

/* The depth of the blocking calls of the calling G, and leaving those that a trap skipped: a failure
 * domain restores the depth it was entered with, so that its M runs tzroutines again (TZ02). */
TZR_API int32_t tsuzuri_tz_block_depth(void) {
    tzr_m *m = tzr_this_m();
    return m != NULL && m->curg != NULL ? m->blocked : 0;
}

TZR_API void tsuzuri_tz_block_restore(int32_t depth) {
    for (;;) {
        tzr_m *m = tzr_this_m();
        if (m == NULL || m->curg == NULL || m->blocked <= depth) return;
        tsuzuri_tz_block_leave();
    }
}

/* The spawn scope of the calling G, and setting it back: a failure domain that a trap left
 * restores the scope that it was entered with (TZ02), since the trap skipped the scope leave of the
 * code between. */
TZR_API void *tsuzuri_tz_scope_current(void) { return tzr_current()->spawn_scope; }
TZR_API void tsuzuri_tz_scope_reset(void *scope) { tzr_current()->spawn_scope = scope; }

/* The message of a failure that the generated code traps for (TZ02, TZ03, TZ06). The numbers are
 * fixed: a new message goes at the end. */
TZR_API void tsuzuri_tz_report(int32_t what) {
    static const char *const messages[] = {
        "",
        "send on closed Channel",
        "close of closed Channel",
        "negative WaitGroup counter",
        "Mutex.with_lock cannot be nested",
        "a tzroutine cannot yield or wait inside Mutex.with_lock; move the call outside the critical section",
        "RwMutex cannot be locked again by the tzroutine that holds it",
        "Semaphore released more than it acquired",
        "Semaphore weight is negative or larger than its capacity",
    };
    if (what > 0 && what < (int32_t)(sizeof messages / sizeof messages[0])) {
        fprintf(stderr, "Tsuzuri runtime: %s\n", messages[what]);
        fflush(stderr);
    }
}

/* Routine.count: the tzroutines that exist, counting the caller's thread as one (Go counts main). */
TZR_API int64_t tsuzuri_tz_count(void) { return atomic_load(&tzr_sched.ngoroutines) + 1; }

/* Routine.max_procs: the number of Ps (TSUZURI_MAXPROCS, or the online CPUs, at most 256). */
TZR_API int64_t tsuzuri_tz_max_procs(void) {
    tzr_start();
    return tzr_sched.nprocs;
}

/* Routine.cpu_count: the online CPUs, at least 1. */
TZR_API int64_t tsuzuri_tz_cpu_count(void) {
    long cpus = tzr_online_cpus();
    return cpus < 1 ? 1 : cpus;
}

/* ---------------------------------------------------------------- Task workers (TZ03) */

/* The items of a Task.parallel, Task.scope or Parallel.* call run on the threads of the task runtime
 * (src/runtime/task.c), which join this runtime as pseudo-Gs only when they first call in. The
 * generated code announces each batch of items before it hands them over, so that the deadlock
 * detector counts an item that has not started as running (it may still wake a waiter), and each
 * item marks its thread as running while it runs: the thread that waits for the batch, and a worker
 * between items, are idle and do not hide a deadlock. A batch is known by its context pointer, which
 * is in the caller's frame: no other call of the moment has the same one. A G that calls in is on an
 * M, which counts as running however long the call takes.
 *
 * A trap that a failure domain recovers from skips the end of each batch that the trapping thread
 * announced on the way (the task runtime has joined their items: it raises an item's trap in the
 * submitting thread only after the join). Every place that catches such a trap drops what this
 * thread announced after its mark: the item callback, which catches the trap of its item and raises
 * it again (so an item always ends, and its thread goes back to the state it had before), and the
 * failure domain. Batches are numbered in the order of announcement, and a mark is the last number:
 * a batch that another thread announced, or one that this thread announced before the mark, stays. */
typedef struct tzr_batch {
    const void *context;
    uint64_t seq;        /* the order of announcement */
    int64_t unstarted;   /* items that have not started */
    int64_t running;     /* items that have started and not ended */
    tzr_g *submitter;    /* the pseudo-G that waits for the batch, idle while items remain */
    const void *owner;   /* the thread that announced it: its M, or its pseudo-G */
    struct tzr_batch *next;
} tzr_batch;

static tzr_batch *tzr_batches; /* tzr_sched.lock */
static uint64_t tzr_batch_seq; /* tzr_sched.lock: the number of the last batch announced */

/* What an item's callback keeps in its frame between the start and the end of the item. */
typedef struct {
    uint64_t mark; /* the last batch announced when the item started */
    int32_t idle;  /* the thread's state before the item */
    int32_t unused;
} tzr_item_frame;

/* The thread that runs the caller, whatever G or pseudo-G it acts as: a G inside a blocking call
 * stays on its M. */
static const void *tzr_thread_owner(void) {
    tzr_m *m = tzr_this_m();
    if (m != NULL) return m;
    return tzr_pseudo();
}

/* Whether this thread has called in before (a pool worker has not, before its first item). */
static TZR_NOINLINE int tzr_known_thread(void) { return tzr_this_m() != NULL || tzr_tls_pseudo != NULL; }

static tzr_batch *tzr_batch_of(const void *context) {
    for (tzr_batch *batch = tzr_batches; batch; batch = batch->next)
        if (batch->context == context) return batch;
    return NULL;
}

/* Drops the batches that `owner` announced after `mark` (tzr_sched.lock held): their items no
 * longer hold off the deadlock verdict. Returns whether there was one. */
static int tzr_drop_batches(uint64_t mark, const void *owner) {
    int dropped = 0;
    for (tzr_batch **link = &tzr_batches; *link != NULL;) {
        tzr_batch *batch = *link;
        if (batch->seq <= mark || batch->owner != owner) {
            link = &batch->next;
            continue;
        }
        *link = batch->next;
        tzr_sched.npending -= batch->unstarted;
        free(batch);
        dropped = 1;
    }
    return dropped;
}

/* Before the task runtime takes the n items of `context`. */
TZR_API void tsuzuri_tz_parallel_begin(const void *context, int64_t n) {
    tzr_g *g = tzr_current();
    if (n <= 0) return;
    tzr_batch *batch = malloc(sizeof *batch);
    if (batch == NULL) tzr_fatal("out of memory");
    batch->context = context;
    batch->unstarted = n;
    batch->running = 0;
    batch->submitter = g->kind == TZR_KIND_THREAD ? g : NULL;
    batch->owner = tzr_thread_owner();
    tzr_mutex_lock(&tzr_sched.lock);
    batch->seq = ++tzr_batch_seq;
    batch->next = tzr_batches;
    tzr_batches = batch;
    tzr_sched.npending += n;
    tzr_set_running(g, 0); /* no verdict: the items are pending */
    tzr_mutex_unlock(&tzr_sched.lock);
}

/* After the task runtime has finished or skipped every item of `context`. */
TZR_API void tsuzuri_tz_parallel_end(const void *context, int64_t n) {
    tzr_g *g = tzr_current();
    if (n <= 0) return;
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_batch *batch = NULL;
    for (tzr_batch **link = &tzr_batches; *link; link = &(*link)->next) {
        if ((*link)->context == context) {
            batch = *link;
            *link = batch->next;
            break;
        }
    }
    if (batch != NULL) tzr_sched.npending -= batch->unstarted; /* skipped after a failure */
    tzr_set_running(g, 1);
    tzr_mutex_unlock(&tzr_sched.lock);
    free(batch);
}

/* The mark of a failure domain, and dropping after it what this thread announced inside the domain
 * that a trap left: the thread then counts as running again. */
TZR_API uint64_t tsuzuri_tz_parallel_mark(void) {
    tzr_mutex_lock(&tzr_sched.lock);
    uint64_t mark = tzr_batch_seq;
    tzr_mutex_unlock(&tzr_sched.lock);
    return mark;
}

TZR_API void tsuzuri_tz_parallel_reset(uint64_t mark) {
    const void *owner = tzr_thread_owner();
    tzr_g *g = tzr_current();
    tzr_mutex_lock(&tzr_sched.lock);
    if (tzr_drop_batches(mark, owner)) tzr_set_running(g, 1);
    tzr_mutex_unlock(&tzr_sched.lock);
}

/* The start of an item of `context`, on whichever thread runs it: a pool worker, the thread that
 * submitted the batch, or one that runs it while it waits (a DataChannel wait runs an item of any
 * batch, inside an item of its own). */
TZR_API void tsuzuri_tz_task_enter(const void *context, tzr_item_frame *frame) {
    int known = tzr_known_thread();
    tzr_g *g = tzr_current();
    tzr_mutex_lock(&tzr_sched.lock);
    tzr_batch *batch = tzr_batch_of(context);
    if (batch != NULL && batch->unstarted > 0) {
        batch->unstarted--;
        batch->running++;
        tzr_sched.npending--;
    }
    frame->mark = tzr_batch_seq;
    frame->idle = !known || g->idle; /* a new pseudo-G counts as running, but a worker came from the pool */
    tzr_set_running(g, 1);
    tzr_mutex_unlock(&tzr_sched.lock);
}

/* The end of an item, also of one that trapped: its thread goes back to the state it had before,
 * and the batches that a trap in the item left are dropped. When it was the last item of the
 * batch, the thread that waits for the batch is about to go on, and counts as running at once:
 * there is no moment in which neither counts. */
TZR_API void tsuzuri_tz_task_leave(const void *context, const tzr_item_frame *frame) {
    const void *owner = tzr_thread_owner();
    tzr_g *g = tzr_current();
    tzr_mutex_lock(&tzr_sched.lock);
    (void)tzr_drop_batches(frame->mark, owner);
    tzr_set_running(g, !frame->idle);
    tzr_batch *batch = tzr_batch_of(context);
    if (batch != NULL && batch->running > 0 && --batch->running == 0 && batch->unstarted == 0)
        tzr_set_running(batch->submitter, 1);
    tzr_checkdead();
    tzr_mutex_unlock(&tzr_sched.lock);
}

/* ---------------------------------------------------------------- parking lot (sync) */

/* Address-keyed waiting, like a futex: tzr_addr_wait parks while *addr == expected, tzr_addr_wake
 * wakes up to n waiters of addr. Mutex, RwMutex, WaitGroup, Once, Cond and Semaphore use it. */
#define TZR_BUCKETS 251
typedef struct tzr_waiter {
    tzr_g *g;
    void *addr;
    struct tzr_waiter *next;
} tzr_waiter;
static struct {
    tzr_lock lock;
    tzr_waiter *first, *last;
} tzr_buckets[TZR_BUCKETS];

static size_t tzr_bucket(void *addr) { return ((uintptr_t)addr >> 3) % TZR_BUCKETS; }
static void tzr_unlock_bucket(void *arg) { tzr_lock_release(&tzr_buckets[(size_t)(uintptr_t)arg].lock); }

/* Takes `target` out of bucket b, if it is there (the bucket locked). */
static int tzr_bucket_unlink(size_t b, tzr_waiter *target) {
    tzr_waiter *last = NULL;
    for (tzr_waiter **link = &tzr_buckets[b].first; *link; link = &(*link)->next) {
        if (*link == target) {
            *link = target->next;
            if (*link == NULL) tzr_buckets[b].last = last;
            return 1;
        }
        last = *link;
    }
    return 0;
}

/* Parks while the word at addr, of 64 bits when `wide` and of 32 otherwise (the Mutex's), equals
 * `expected`. Returns 1 for a doomed pseudo-G (deadlock). */
static int tzr_addr_wait(void *addr, uint64_t expected, int wide, const char *reason) {
    size_t b = tzr_bucket(addr);
    tzr_lock_acquire(&tzr_buckets[b].lock);
    uint64_t now = wide ? atomic_load((_Atomic uint64_t *)addr) : atomic_load((_Atomic uint32_t *)addr);
    if (now != expected) {
        tzr_lock_release(&tzr_buckets[b].lock);
        return 0;
    }
    tzr_waiter w = {tzr_current(), (void *)addr, NULL};
    if (tzr_buckets[b].last) tzr_buckets[b].last->next = &w;
    else tzr_buckets[b].first = &w;
    tzr_buckets[b].last = &w;
    if (!tzr_park(tzr_unlock_bucket, (void *)(uintptr_t)b, reason)) return 0;
    /* The detector woke it: cancel the wait, unless a waker took it first (that wake happened with
     * the bucket locked, so it is over, and the wait ended normally). */
    tzr_lock_acquire(&tzr_buckets[b].lock);
    int queued = tzr_bucket_unlink(b, &w);
    tzr_lock_release(&tzr_buckets[b].lock);
    return queued;
}

/* Wakes up to n waiters of addr (all of them when n < 0) with the bucket locked (R3 allows it), so
 * that a waiter that wakes and waits again cannot join the same wake. A waiter is on its own stack,
 * so it is unlinked before it is woken and never read after. */
static int64_t tzr_addr_wake(void *addr, int64_t n) {
    size_t b = tzr_bucket(addr);
    int64_t count = 0;
    tzr_lock_acquire(&tzr_buckets[b].lock);
    tzr_waiter **link = &tzr_buckets[b].first;
    tzr_waiter *last = NULL;
    while (*link && (n < 0 || count < n)) {
        tzr_waiter *w = *link;
        if (w->addr == (void *)addr) {
            *link = w->next;
            count++;
            tzr_ready(w->g);
        } else {
            last = w;
            link = &w->next;
        }
    }
    if (*link == NULL) tzr_buckets[b].last = last;
    tzr_lock_release(&tzr_buckets[b].lock);
    return count;
}

/* The parking lot for the sync types that TZ06 writes in C (RwMutex, Once, Cond, Semaphore). */
TZR_API int32_t tsuzuri_tz_addr_wait(_Atomic uint64_t *addr, uint64_t expected) {
    return tzr_addr_wait(addr, expected, 1, "sync wait") ? 2 : 0;
}

TZR_API int64_t tsuzuri_tz_addr_wake(_Atomic uint64_t *addr, int64_t n) {
    (void)tzr_current(); /* a host thread counts as running while it wakes (the detector) */
    return tzr_addr_wake(addr, n < 0 ? -1 : n);
}

/* Mutex: the word is 0 free, 1 held, 2 held with waiters (Drepper's three states). */
TZR_API int32_t tsuzuri_tz_mutex_lock(_Atomic uint64_t *word) {
    (void)tzr_current(); /* a host thread that holds the lock counts as running (the detector) */
    uint64_t c = 0;
    if (atomic_compare_exchange_strong_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed)) return 0;
    for (int spin = 0; spin < 32; spin++) {
        c = 0;
        if (atomic_load_explicit(word, memory_order_relaxed) == 0
            && atomic_compare_exchange_weak_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed)) return 0;
    }
    while (atomic_exchange_explicit(word, 2, memory_order_acquire) != 0)
        if (tzr_addr_wait(word, 2, 1, "Mutex lock")) return 2;
    return 0;
}

TZR_API void tsuzuri_tz_mutex_unlock(_Atomic uint64_t *word) {
    (void)tzr_current();
    if (atomic_exchange_explicit(word, 0, memory_order_release) == 2) tzr_addr_wake(word, 1);
}

/* WaitGroup: the word is the counter. */
TZR_API int32_t tsuzuri_tz_waitgroup_add(_Atomic uint64_t *word, int64_t delta) {
    (void)tzr_current();
    /* Unsigned arithmetic: a counter outside [0, INT64_MAX] is an error, not an overflow. */
    uint64_t v = atomic_fetch_add(word, (uint64_t)delta) + (uint64_t)delta;
    if (v > (uint64_t)INT64_MAX) return 1; /* the generated code traps: negative WaitGroup counter */
    if (v == 0) tzr_addr_wake(word, -1);
    return 0;
}

TZR_API int32_t tsuzuri_tz_waitgroup_wait(_Atomic uint64_t *word) {
    (void)tzr_current();
    for (;;) {
        uint64_t v = atomic_load(word);
        if (v == 0) return 0;
        if (tzr_addr_wait(word, v, 1, "WaitGroup.wait")) return 2;
    }
}

/* ---------------------------------------------------------------- sync (TZ06) */

/* The locks that a G holds (Mutex, and the read and write locks of RwMutex), in the order taken, so
 * that taking one again traps instead of waiting for itself (D4), and a failure domain that a trap
 * left releases what was taken inside it (tsuzuri_tz_held_restore). The list is the G's own, out of
 * the frames: a trap leaves the frames of the critical sections behind. The low bits of an entry
 * are its kind (the words are 8-byte aligned in their cells). */
enum { TZR_HELD_MUTEX = 0, TZR_HELD_READ = 1, TZR_HELD_WRITE = 2 };
#define TZR_HELD_KIND 3u

static int tzr_held_has(tzr_g *g, const void *word) {
    for (uint32_t i = 0; i < g->held_count; i++)
        if ((g->held[i] & ~(uintptr_t)TZR_HELD_KIND) == (uintptr_t)word) return 1;
    return 0;
}

static void tzr_held_push(tzr_g *g, const void *word, unsigned kind) {
    if (g->held_count == g->held_capacity) {
        uint32_t capacity = g->held_capacity ? g->held_capacity * 2 : 8;
        uintptr_t *held = realloc(g->held, capacity * sizeof *held);
        if (held == NULL) tzr_fatal("out of memory");
        g->held = held;
        g->held_capacity = capacity;
    }
    g->held[g->held_count++] = (uintptr_t)word | kind;
}

/* Takes the last entry of `word` off the list (a critical section ends where it began). */
static void tzr_held_pop(tzr_g *g, const void *word) {
    for (uint32_t i = g->held_count; i-- > 0;) {
        if ((g->held[i] & ~(uintptr_t)TZR_HELD_KIND) == (uintptr_t)word) {
            memmove(&g->held[i], &g->held[i + 1], (g->held_count - i - 1) * sizeof *g->held);
            g->held_count--;
            return;
        }
    }
}

/* Mutex.with_lock of a program that uses the tzroutine runtime (TZ06): the 32-bit word of the cell
 * is 0 free, 1 held, 2 held with waiters. Only the G waits: the lock is the G's, so a holder may
 * wait for a channel, a select or another lock (Go's sync.Mutex). Returns 1 when the G holds this
 * mutex already (the generated code reports 4 and traps), 2 on a deadlock. */
TZR_API int32_t tsuzuri_tz_mutex32_lock(_Atomic uint32_t *word) {
    tzr_g *g = tzr_current();
    if (tzr_held_has(g, word)) return 1;
    uint32_t c = 0;
    if (!atomic_compare_exchange_strong_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed)) {
        int taken = 0;
        for (int spin = 0; spin < 32 && !taken; spin++) {
            c = 0;
            taken = atomic_load_explicit(word, memory_order_relaxed) == 0
                    && atomic_compare_exchange_weak_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed);
        }
        if (!taken)
            while (atomic_exchange_explicit(word, 2, memory_order_acquire) != 0)
                if (tzr_addr_wait(word, 2, 0, "Mutex lock")) return 2;
    }
    tzr_held_push(g, word, TZR_HELD_MUTEX);
    return 0;
}

TZR_API void tsuzuri_tz_mutex32_unlock(_Atomic uint32_t *word) {
    tzr_held_pop(tzr_current(), word);
    if (atomic_exchange_explicit(word, 0, memory_order_release) == 2) tzr_addr_wake(word, 1);
}

/* RwMutex (TZ06): the low 62 bits of the word count the readers, bit 62 says that a writer waits and
 * bit 63 that a writer holds it. A writer that waits keeps new readers out, so readers cannot starve
 * it (Go's policy). Every change that can let a waiter in wakes them all: the last reader when a
 * writer waits, and the writer's release; a woken waiter looks again. 1 means the G holds this lock
 * already in some mode (reported 6), 2 a deadlock. */
#define TZR_RW_WRITER (UINT64_C(1) << 63)
#define TZR_RW_WAITING (UINT64_C(1) << 62)
#define TZR_RW_READERS (TZR_RW_WAITING - 1)

TZR_API int32_t tsuzuri_tz_rw_read_lock(_Atomic uint64_t *word) {
    tzr_g *g = tzr_current();
    if (tzr_held_has(g, word)) return 1;
    for (;;) {
        uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
        if ((s & (TZR_RW_WRITER | TZR_RW_WAITING)) == 0) {
            if (atomic_compare_exchange_weak_explicit(word, &s, s + 1, memory_order_acquire, memory_order_relaxed)) break;
            continue;
        }
        if (tzr_addr_wait(word, s, 1, "RwMutex.with_read")) return 2;
    }
    tzr_held_push(g, word, TZR_HELD_READ);
    return 0;
}

TZR_API void tsuzuri_tz_rw_read_unlock(_Atomic uint64_t *word) {
    tzr_held_pop(tzr_current(), word);
    uint64_t s = atomic_fetch_sub_explicit(word, 1, memory_order_release) - 1;
    if ((s & TZR_RW_READERS) == 0 && (s & TZR_RW_WAITING)) tzr_addr_wake(word, -1);
}

TZR_API int32_t tsuzuri_tz_rw_write_lock(_Atomic uint64_t *word) {
    tzr_g *g = tzr_current();
    if (tzr_held_has(g, word)) return 1;
    for (;;) {
        uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
        if ((s & (TZR_RW_WRITER | TZR_RW_READERS)) == 0) {
            if (atomic_compare_exchange_weak_explicit(word, &s, (s & ~TZR_RW_WAITING) | TZR_RW_WRITER,
                                                      memory_order_acquire, memory_order_relaxed)) break;
            continue;
        }
        if ((s & TZR_RW_WAITING) == 0) {
            if (!atomic_compare_exchange_weak_explicit(word, &s, s | TZR_RW_WAITING, memory_order_relaxed,
                                                       memory_order_relaxed)) continue;
            s |= TZR_RW_WAITING;
        }
        if (tzr_addr_wait(word, s, 1, "RwMutex.with_write")) return 2;
    }
    tzr_held_push(g, word, TZR_HELD_WRITE);
    return 0;
}

TZR_API void tsuzuri_tz_rw_write_unlock(_Atomic uint64_t *word) {
    tzr_held_pop(tzr_current(), word);
    atomic_store_explicit(word, 0, memory_order_release);
    tzr_addr_wake(word, -1);
}

/* The depth of the calling G's held locks, and the release of those that a trap left above it: a
 * failure domain and a pool item restore the depth that they began with (TZ06, as the mark of the
 * thread-owned Mutex of TZ02 was). */
TZR_API int32_t tsuzuri_tz_held_depth(void) { return (int32_t)tzr_current()->held_count; }

TZR_API void tsuzuri_tz_held_restore(int32_t depth) {
    tzr_g *g = tzr_current();
    while (g->held_count > (uint32_t)depth) {
        uintptr_t entry = g->held[g->held_count - 1];
        void *word = (void *)(entry & ~(uintptr_t)TZR_HELD_KIND);
        switch (entry & TZR_HELD_KIND) {
        case TZR_HELD_MUTEX: tsuzuri_tz_mutex32_unlock(word); break;
        case TZR_HELD_READ: tsuzuri_tz_rw_read_unlock(word); break;
        default: tsuzuri_tz_rw_write_unlock(word); break;
        }
    }
}

/* Once and OnceValue (TZ06): the word is 0 not run, 1 running, 2 done. begin returns 1 to the caller
 * that runs the function, 0 once it is done (after waiting for the one that runs it), 2 on a
 * deadlock (a function that calls its own Once waits for itself, D6). end publishes the value. */
TZR_API int32_t tsuzuri_tz_once_begin(_Atomic uint64_t *word) {
    (void)tzr_current();
    for (;;) {
        uint64_t s = atomic_load_explicit(word, memory_order_acquire);
        if (s == 2) return 0;
        if (s == 0) {
            if (atomic_compare_exchange_weak_explicit(word, &s, 1, memory_order_acquire, memory_order_acquire)) return 1;
            continue;
        }
        if (tzr_addr_wait(word, 1, 1, "Once.call")) return 2;
    }
}

TZR_API void tsuzuri_tz_once_end(_Atomic uint64_t *word) {
    (void)tzr_current();
    atomic_store_explicit(word, 2, memory_order_release);
    tzr_addr_wake(word, -1);
}

/* Cond.wait_with (TZ06 Phase 2): with the mutex held and the predicate false, `observed` is the
 * sequence number read under the lock. Releases the mutex, waits while the number is unchanged and
 * takes the mutex again. A signal adds one to the number, so a wake between the release and the
 * wait is not lost; a spurious one is harmless (the caller checks the predicate again). */
TZR_API int32_t tsuzuri_tz_cond_wait(_Atomic uint64_t *sequence, _Atomic uint32_t *mutex, uint64_t observed) {
    tsuzuri_tz_mutex32_unlock(mutex);
    int doomed = tzr_addr_wait(sequence, observed, 1, "Cond.wait_with");
    int32_t status = tsuzuri_tz_mutex32_lock(mutex);
    return doomed ? 2 : status;
}

TZR_API void tsuzuri_tz_cond_wake(_Atomic uint64_t *sequence, int64_t n) {
    (void)tzr_current();
    atomic_fetch_add_explicit(sequence, 1, memory_order_release);
    tzr_addr_wake(sequence, n);
}

/* Semaphore (x/sync/semaphore.Weighted, TZ06 Phase 2): the word is the free weight. The generated
 * code checks the weight against the capacity first. A release wakes every waiter, which take
 * what they can in no particular order (D7). */
TZR_API int32_t tsuzuri_tz_sem_acquire(_Atomic uint64_t *word, uint64_t weight, int32_t block) {
    (void)tzr_current();
    for (;;) {
        uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
        if (s >= weight) {
            if (atomic_compare_exchange_weak_explicit(word, &s, s - weight, memory_order_acquire, memory_order_relaxed)) return 0;
            continue;
        }
        if (!block) return 1;
        if (tzr_addr_wait(word, s, 1, "Semaphore.acquire")) return 2;
    }
}

/* Returns 1 when the free weight would pass the capacity (reported 7). */
TZR_API int32_t tsuzuri_tz_sem_release(_Atomic uint64_t *word, uint64_t weight, uint64_t capacity) {
    (void)tzr_current();
    uint64_t s = atomic_load_explicit(word, memory_order_relaxed);
    do {
        if (weight > capacity - s) return 1;
    } while (!atomic_compare_exchange_weak_explicit(word, &s, s + weight, memory_order_release, memory_order_relaxed));
    tzr_addr_wake(word, -1);
    return 0;
}

#endif /* TZR_SINGLE */

/* ---------------------------------------------------------------- channels */

struct tzr_sudog {
    tzr_g *g;
    tzr_sudog *next, *prev;
    void *elem;
    tzr_chan *c;
    int is_select;
    int success;
};

typedef struct {
    tzr_sudog *first, *last;
} tzr_waitq;

/* The block that Channel.make allocates: this header, then capacity * elem_size bytes. */
struct tzr_chan {
    tzr_lock lock;
    _Atomic uint64_t refs;
    uint64_t elem_size;
    uint64_t capacity;
    uint64_t count;
    uint64_t sendx;
    uint64_t recvx;
    uint64_t closed;
    tzr_waitq recvq;
    tzr_waitq sendq;
    uint64_t reserved[4 + (8 - sizeof(void *)) / 2]; /* the queues are half as large on wasm32 */
};
_Static_assert(sizeof(struct tzr_chan) == 128, "channel header is 128 bytes");
#define TZR_CHAN_BUF(c, i) ((char *)(c) + sizeof(struct tzr_chan) + (i) * (c)->elem_size)

enum { TZR_OK = 0, TZR_CLOSED = 1, TZR_DEADLOCK = 2, TZR_WOULD_BLOCK = 3 };

static void tzr_enqueue(tzr_waitq *q, tzr_sudog *s) {
    s->next = NULL;
    s->prev = q->last;
    if (q->last) q->last->next = s;
    else q->first = s;
    q->last = s;
}

static void tzr_unlink(tzr_waitq *q, tzr_sudog *s) {
    if (s->prev) s->prev->next = s->next;
    else if (q->first == s) q->first = s->next;
    else return; /* not queued */
    if (s->next) s->next->prev = s->prev;
    else q->last = s->prev;
    s->next = s->prev = NULL;
}

/* Takes the first waiter that is not a select that another case already won. */
static tzr_sudog *tzr_dequeue(tzr_waitq *q) {
    for (;;) {
        tzr_sudog *s = q->first;
        if (s == NULL) return NULL;
        q->first = s->next;
        if (q->first) q->first->prev = NULL;
        else q->last = NULL;
        s->next = s->prev = NULL;
        if (s->is_select) {
            uint32_t expected = 0;
            if (!atomic_compare_exchange_strong(&s->g->select_done, &expected, 1)) continue;
        }
        return s;
    }
}

TZR_API void tsuzuri_tz_chan_init(tzr_chan *c, int64_t elem_size, int64_t capacity) {
    memset(c, 0, sizeof *c);
    atomic_store(&c->refs, 1);
    c->elem_size = (uint64_t)elem_size;
    c->capacity = (uint64_t)capacity;
}

TZR_API void tsuzuri_tz_chan_share(tzr_chan *c) { atomic_fetch_add_explicit(&c->refs, 1, memory_order_relaxed); }

/* Returns 1 when this was the last handle: the generated code then takes the buffered items with
 * tsuzuri_tz_chan_drain, drops them and frees the block. */
TZR_API int32_t tsuzuri_tz_chan_release(tzr_chan *c) {
    return atomic_fetch_sub_explicit(&c->refs, 1, memory_order_acq_rel) == 1;
}

TZR_API int32_t tsuzuri_tz_chan_drain(tzr_chan *c, void *out) {
    if (c->count == 0) return 0;
    memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
    c->recvx = (c->recvx + 1) % c->capacity;
    c->count--;
    return 1;
}

static void tzr_unlock_chan(void *arg) { tzr_lock_release(&((tzr_chan *)arg)->lock); }

static void tzr_send_to(tzr_chan *c, tzr_sudog *s, const void *elem) {
    /* c->lock held; s is a receiver */
    if (s->elem) memcpy(s->elem, elem, c->elem_size);
    s->success = 1;
    s->g->param = s;
}

static void tzr_recv_from(tzr_chan *c, tzr_sudog *s, void *out) {
    /* c->lock held; s is a sender and the buffer, if any, is full */
    if (c->capacity == 0) {
        memcpy(out, s->elem, c->elem_size);
    } else {
        memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
        memcpy(TZR_CHAN_BUF(c, c->recvx), s->elem, c->elem_size);
        c->recvx = (c->recvx + 1) % c->capacity;
        c->sendx = c->recvx;
    }
    s->success = 1;
    s->g->param = s;
}

/* After the detector woke a waiting pseudo-G: takes its sudog out of q and returns 1, or returns 0
 * when a waker took it first (that wake happened with the channel locked: the operation is done). */
static int tzr_cancel(tzr_chan *c, tzr_waitq *q, tzr_sudog *s) {
    tzr_lock_acquire(&c->lock);
    int queued = s->prev != NULL || q->first == s;
    if (queued) tzr_unlink(q, s);
    tzr_lock_release(&c->lock);
    return queued;
}

/* block = 0 is try_send. Returns TZR_OK, TZR_CLOSED (the generated code traps for send),
 * TZR_WOULD_BLOCK (try only) or TZR_DEADLOCK. */
static int32_t tzr_chan_send(tzr_chan *c, const void *elem, int32_t block) {
    tzr_lock_acquire(&c->lock);
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    tzr_sudog *s = tzr_dequeue(&c->recvq);
    if (s) {
        tzr_send_to(c, s, elem);
        tzr_ready_unlock(s->g, &c->lock);
        return TZR_OK;
    }
    if (c->count < c->capacity) {
        memcpy(TZR_CHAN_BUF(c, c->sendx), elem, c->elem_size);
        c->sendx = (c->sendx + 1) % c->capacity;
        c->count++;
        tzr_lock_release(&c->lock);
        return TZR_OK;
    }
    if (!block) {
        tzr_lock_release(&c->lock);
        return TZR_WOULD_BLOCK;
    }
    tzr_sudog me = {.g = tzr_current(), .elem = (void *)elem, .c = c};
    me.g->param = NULL;
    tzr_enqueue(&c->sendq, &me);
    if (tzr_park(tzr_unlock_chan, c, "Channel.send") && tzr_cancel(c, &c->sendq, &me)) return TZR_DEADLOCK;
    return me.success ? TZR_OK : TZR_CLOSED;
}

TZR_API int32_t tsuzuri_tz_chan_send(tzr_chan *c, const void *elem, int32_t block) {
    (void)tzr_current(); /* a host thread counts as running while it hands over (the detector) */
    return tzr_chan_send(c, elem, block);
}

/* Returns TZR_OK (out holds the item), TZR_CLOSED (closed and empty), TZR_WOULD_BLOCK or TZR_DEADLOCK. */
TZR_API int32_t tsuzuri_tz_chan_recv(tzr_chan *c, void *out, int32_t block) {
    (void)tzr_current();
    tzr_lock_acquire(&c->lock);
    tzr_sudog *s = tzr_dequeue(&c->sendq);
    if (s) {
        tzr_recv_from(c, s, out);
        tzr_ready_unlock(s->g, &c->lock);
        return TZR_OK;
    }
    if (c->count > 0) {
        memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
        c->recvx = (c->recvx + 1) % c->capacity;
        c->count--;
        tzr_lock_release(&c->lock);
        return TZR_OK;
    }
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    if (!block) {
        tzr_lock_release(&c->lock);
        return TZR_WOULD_BLOCK;
    }
    tzr_sudog me = {.g = tzr_current(), .elem = out, .c = c};
    me.g->param = NULL;
    tzr_enqueue(&c->recvq, &me);
    if (tzr_park(tzr_unlock_chan, c, "Channel.recv") && tzr_cancel(c, &c->recvq, &me)) return TZR_DEADLOCK;
    return me.success ? TZR_OK : TZR_CLOSED;
}

/* Returns TZR_OK, or TZR_CLOSED when it was closed already (the generated code traps). */
static int32_t tzr_chan_close(tzr_chan *c) {
    tzr_lock_acquire(&c->lock);
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    c->closed = 1;
    /* Every waiter wakes with the lock held (R3 allows it; see tzr_ready_unlock). A sudog is on its
     * waiter's stack, so it is not read after the wake. */
    for (;;) {
        tzr_sudog *s = tzr_dequeue(&c->recvq);
        if (s == NULL) s = tzr_dequeue(&c->sendq);
        if (s == NULL) break;
        s->success = 0;
        s->g->param = s;
        tzr_ready(s->g);
    }
    tzr_lock_release(&c->lock);
    return TZR_OK;
}

TZR_API int32_t tsuzuri_tz_chan_close(tzr_chan *c) {
    (void)tzr_current();
    return tzr_chan_close(c);
}

TZR_API int64_t tsuzuri_tz_chan_capacity(tzr_chan *c) { return (int64_t)c->capacity; }

TZR_API int64_t tsuzuri_tz_chan_length(tzr_chan *c) {
    tzr_lock_acquire(&c->lock);
    int64_t n = (int64_t)c->count;
    tzr_lock_release(&c->lock);
    return n;
}

#if !defined(TZR_SINGLE)
/* ---------------------------------------------------------------- timers */

/* A timer wakes a sleeper (WAKE, Time.sleep), sends the time to a channel once (SEND, Time.after and
 * Timer) or every period (TICK, Time.tick and Ticker), or starts a function on a new tzroutine
 * (FUNC, Time.after_func) (TZ05). A WAKE timer is on its sleeper's stack. The others are malloc'd
 * (TZ01 D9): Time.after's and Time.tick's belong to the runtime until they fire or stop; a Timer or
 * Ticker handle owns its record (`handle`), which the handle's drop frees. A pending timer other
 * than WAKE is listed in the boundary scope of the G that made it, whose end stops it. */
enum { TZR_TIMER_WAKE, TZR_TIMER_SEND, TZR_TIMER_TICK, TZR_TIMER_FUNC, TZR_TIMER_CTX };
/* DORMANT (TZ07): listed in its boundary but out of the heap and not counted in ntimers, until a
 * context's cancel starts it (Context.after_func): it never fires by the clock, so it does not keep
 * the deadlock detector from looking. */
enum { TZR_TIMER_PENDING, TZR_TIMER_FIRED, TZR_TIMER_STOPPED, TZR_TIMER_DORMANT };

struct tzr_timer {
    int64_t when;               /* the deadline, on tzr_nanotime's clock */
    int64_t period;             /* TICK */
    int kind;
    int state;                  /* tzr_timers.lock, as every field below that changes */
    int handle;                 /* a Timer or Ticker handle owns the record */
    size_t index;               /* its place in the heap while PENDING */
    tzr_g *g;                   /* WAKE: the sleeper */
    tzr_chan *chan;             /* SEND, TICK, and FUNC's closed one: the record holds one reference */
    void (*release)(void *);    /* frees the channel's block: the generated code's deallocator */
    void (*run)(void *);        /* FUNC: calls the boxed function once and frees the box */
    void (*drop)(void *);       /* FUNC: drops the boxed function without calling it */
    void *box;                  /* FUNC: the boxed function, until it runs or is dropped */
    void *ctx;                  /* CTX (TZ07): the context whose deadline this is; the timer is
                                 * part of it and holds one reference while it pends */
    tzr_scope *scope;           /* the boundary that lists it, or NULL */
    tzr_timer *scope_next, *scope_prev;
};

/* One heap of timers under a tzr_lock, and one thread that sleeps until the earliest one. The
 * thread sleeps on its own mutex and condition variable (`kicked` prevents a lost wakeup), so the
 * heap lock is a tzr_lock that tzr_park can release (R4). The thread never asks for a G of its own:
 * it sends, readies and spawns through functions that do not call tzr_current. */
static struct {
    tzr_lock lock;
    tzr_timer **heap;
    size_t count, capacity;
    int started;
    tzr_mutex wake_mutex;
    tzr_cond wake;
    int kicked;
} tzr_timers = {
    .wake_mutex = TZR_MUTEX_INIT,
#if !defined(__linux__)
    .wake = TZR_COND_INIT, /* Linux initializes it with the monotonic clock (tzr_timer_cond_init) */
#endif
};

static void tzr_heap_set(size_t i, tzr_timer *t) {
    tzr_timers.heap[i] = t;
    t->index = i;
}

static void tzr_heap_up(size_t i) {
    while (i > 0) {
        size_t parent = (i - 1) / 2;
        if (tzr_timers.heap[parent]->when <= tzr_timers.heap[i]->when) break;
        tzr_timer *t = tzr_timers.heap[parent];
        tzr_heap_set(parent, tzr_timers.heap[i]);
        tzr_heap_set(i, t);
        i = parent;
    }
}

static void tzr_heap_down(size_t i) {
    for (;;) {
        size_t l = 2 * i + 1, r = l + 1, m = i;
        if (l < tzr_timers.count && tzr_timers.heap[l]->when < tzr_timers.heap[m]->when) m = l;
        if (r < tzr_timers.count && tzr_timers.heap[r]->when < tzr_timers.heap[m]->when) m = r;
        if (m == i) return;
        tzr_timer *t = tzr_timers.heap[m];
        tzr_heap_set(m, tzr_timers.heap[i]);
        tzr_heap_set(i, t);
        i = m;
    }
}

static void *tzr_timerproc(void *arg);

/* Adds t to the heap as PENDING and wakes the thread when t is the earliest (tzr_timers.lock). */
static void tzr_heap_push(tzr_timer *t) {
    if (!tzr_timers.started) {
        tzr_timer_cond_init(&tzr_timers.wake);
        tzr_thread_start(tzr_timerproc, NULL, 0, "cannot start the timer thread");
        tzr_timers.started = 1;
    }
    if (tzr_timers.count == tzr_timers.capacity) {
        size_t capacity = tzr_timers.capacity ? tzr_timers.capacity * 2 : 64;
        tzr_timer **heap = realloc(tzr_timers.heap, capacity * sizeof *heap);
        if (heap == NULL) tzr_fatal("out of memory");
        tzr_timers.heap = heap;
        tzr_timers.capacity = capacity;
    }
    t->state = TZR_TIMER_PENDING;
    tzr_heap_set(tzr_timers.count, t);
    tzr_heap_up(tzr_timers.count++);
    if (tzr_timers.heap[0] == t) {
        tzr_mutex_lock(&tzr_timers.wake_mutex);
        tzr_timers.kicked = 1;
        tzr_cond_signal(&tzr_timers.wake);
        tzr_mutex_unlock(&tzr_timers.wake_mutex);
    }
}

/* Takes the PENDING timer t out of the heap (tzr_timers.lock). */
static void tzr_heap_remove(tzr_timer *t) {
    size_t i = t->index;
    tzr_timer *last = tzr_timers.heap[--tzr_timers.count];
    if (i < tzr_timers.count) {
        tzr_heap_set(i, last);
        tzr_heap_up(i);
        tzr_heap_down(last->index);
    }
}

/* The deadline `ns` from now, saturated at INT64_MAX; a negative wait is now. */
static int64_t tzr_deadline(int64_t now, int64_t ns) {
    if (ns <= 0) return now;
    return now > INT64_MAX - ns ? INT64_MAX : now + ns;
}

/* The pending timers fell by n: with none left, the detector looks again, as a G that a timer woke
 * may have run and parked again while it still saw the timer (R3 allows the order of the locks). */
static void tzr_timers_gone(int64_t n) {
    if (n > 0 && atomic_fetch_sub(&tzr_sched.ntimers, n) == n) {
        tzr_mutex_lock(&tzr_sched.lock);
        tzr_checkdead();
        tzr_mutex_unlock(&tzr_sched.lock);
    }
}

/* Lists t in `scope`, or takes it out of its scope (tzr_timers.lock). */
static void tzr_timer_list(tzr_timer *t, tzr_scope *scope) {
    t->scope = scope;
    t->scope_prev = NULL;
    t->scope_next = scope->timers;
    if (scope->timers) scope->timers->scope_prev = t;
    scope->timers = t;
}

static void tzr_timer_unlist(tzr_timer *t) {
    if (t->scope == NULL) return;
    if (t->scope_prev) t->scope_prev->scope_next = t->scope_next;
    else t->scope->timers = t->scope_next;
    if (t->scope_next) t->scope_next->scope_prev = t->scope_prev;
    t->scope = NULL;
    t->scope_next = t->scope_prev = NULL;
}

/* Sends the time to a timer's channel without waiting: a receiver takes it, or the buffer of one
 * keeps it, or it is dropped (a receiver that has not taken the last one misses this one). */
static void tzr_timer_offer(tzr_chan *c, int64_t now) {
    (void)tzr_chan_send(c, &now, 0);
}

/* Empties a timer's channel: after a stop or a reset, no time sent before is received (Go 1.23).
 * The item is an i64, which needs no drop. */
static void tzr_timer_discard(tzr_chan *c) {
    tzr_lock_acquire(&c->lock);
    c->count = 0;
    c->sendx = c->recvx = 0;
    tzr_lock_release(&c->lock);
}

/* Drops the timer's reference to its channel; the last one frees the block. */
static void tzr_timer_unref(tzr_chan *c, void (*release)(void *)) {
    if (atomic_fetch_sub_explicit(&c->refs, 1, memory_order_acq_rel) == 1) release(c);
}

/* Defined with the contexts below: cancels a context whose deadline passed, and lets go of its
 * timer's reference. */
static void tzr_ctx_expire(void *ctx);

static void *tzr_timerproc(void *arg) {
    (void)arg;
    for (;;) {
        tzr_lock_acquire(&tzr_timers.lock);
        int64_t now = tzr_nanotime();
        int64_t gone = 0;
        tzr_scope *spawn_scope = NULL;
        void (*spawn_code)(void *) = NULL;
        void *spawn_env = NULL;
        void *expired = NULL;
        while (tzr_timers.count > 0 && tzr_timers.heap[0]->when <= now && spawn_code == NULL && expired == NULL) {
            tzr_timer *top = tzr_timers.heap[0];
            tzr_heap_remove(top);
            top->state = TZR_TIMER_FIRED;
            gone++;
            switch (top->kind) {
            case TZR_TIMER_WAKE:
                tzr_ready(top->g); /* the G may run and leave `top` (its stack frame) at once */
                break;
            case TZR_TIMER_SEND:
                tzr_timer_offer(top->chan, now);
                tzr_timer_unlist(top);
                if (!top->handle) {
                    tzr_timer_unref(top->chan, top->release);
                    free(top);
                }
                break;
            case TZR_TIMER_TICK:
                if (!top->handle && atomic_load(&top->chan->refs) == 1) {
                    /* Time.tick: nobody else holds the channel, so nobody can receive again */
                    tzr_timer_unlist(top);
                    tzr_timer_unref(top->chan, top->release);
                    free(top);
                    break;
                }
                tzr_timer_offer(top->chan, now); /* dropped when the receiver has not taken the last one */
                top->when = top->when > INT64_MAX - top->period ? INT64_MAX : top->when + top->period;
                if (top->when <= now) top->when = tzr_deadline(now, top->period); /* late: from now */
                tzr_heap_push(top);
                gone--;
                break;
            case TZR_TIMER_CTX:
                /* cancelled outside the timers' lock (D3 of TZ07), with the timer's reference */
                tzr_timer_unlist(top);
                expired = top->ctx;
                break;
            default: /* FUNC: it pends only with its function */
                spawn_scope = top->scope ? top->scope : &tzr_root_scope;
                spawn_code = top->run;
                spawn_env = top->box;
                top->box = NULL;
                tzr_timer_unlist(top);
                /* counted before the lock goes, so that the end of its boundary waits for it */
                tzr_spawn_reserve(spawn_scope);
                break;
            }
        }
        int64_t next = tzr_timers.count > 0 ? tzr_timers.heap[0]->when : -1;
        tzr_lock_release(&tzr_timers.lock);
        if (spawn_code != NULL) {
            /* the stack and the M it may start are not made under the timers' lock */
            tzr_spawn(spawn_scope, spawn_code, spawn_env, 1);
            tzr_timers_gone(gone);
            continue;
        }
        if (expired != NULL) {
            tzr_ctx_expire(expired);
            tzr_timers_gone(gone);
            continue;
        }
        tzr_timers_gone(gone);
        tzr_mutex_lock(&tzr_timers.wake_mutex);
        if (!tzr_timers.kicked) {
            if (next < 0) {
                tzr_cond_wait_for(&tzr_timers.wake, &tzr_timers.wake_mutex, -1);
            } else {
                /* the clock again: firing the batch above took time, and an overdue deadline waits not at all */
                int64_t left = next - tzr_nanotime();
                if (left > 0) tzr_cond_wait_for(&tzr_timers.wake, &tzr_timers.wake_mutex, left);
            }
        }
        tzr_timers.kicked = 0;
        tzr_mutex_unlock(&tzr_timers.wake_mutex);
    }
    return NULL;
}

static void tzr_unlock_timers(void *arg) {
    (void)arg;
    tzr_lock_release(&tzr_timers.lock);
}

TZR_API int32_t tsuzuri_tz_sleep(int64_t ns) {
    if (ns <= 0) {
        tsuzuri_tz_yield();
        return 0;
    }
    tzr_start();
    tzr_timer t = {.when = tzr_deadline(tzr_nanotime(), ns), .kind = TZR_TIMER_WAKE, .g = tzr_current()};
    tzr_lock_acquire(&tzr_timers.lock);
    tzr_heap_push(&t);
    atomic_fetch_add(&tzr_sched.ntimers, 1);
    return tzr_park(tzr_unlock_timers, NULL, "Time.sleep");
}

#if !defined(_WIN32)
static int32_t tzr_sleep_if_routine(int64_t ns) {
    tzr_m *m = tzr_this_m();
    if (m == NULL || m->curg == NULL || m->blocked) return 0; /* a thread: os.c stops it */
    tsuzuri_tz_sleep(ns);
    return 1;
}
#endif

/* Lists t in `scope` and puts it in the heap as PENDING, or leaves it DORMANT (TZ07). */
static void tzr_timer_arm(tzr_timer *t, tzr_scope *scope, int dormant) {
    tzr_lock_acquire(&tzr_timers.lock);
    tzr_timer_list(t, scope);
    if (dormant) {
        t->state = TZR_TIMER_DORMANT;
    } else {
        tzr_heap_push(t);
        atomic_fetch_add(&tzr_sched.ntimers, 1);
    }
    tzr_lock_release(&tzr_timers.lock);
}

/* Takes the waiting timer t out of the heap or out of its dormancy (tzr_timers.lock) and leaves it
 * STOPPED and unlisted. Returns 0 when it was not waiting, 1 when it was dormant and 2 when it
 * pended (and counted in ntimers). */
static int tzr_timer_take(tzr_timer *t) {
    int taken = t->state == TZR_TIMER_PENDING ? 2 : t->state == TZR_TIMER_DORMANT ? 1 : 0;
    if (taken == 2) tzr_heap_remove(t);
    if (taken) {
        t->state = TZR_TIMER_STOPPED;
        tzr_timer_unlist(t);
    }
    return taken;
}

/* The G's boundary, which lists the timers that it makes. */
static tzr_scope *tzr_timer_scope(tzr_g *g) { return tzr_spawn_scope(g)->boundary; }

/* Time.after, Time.tick, Timer.create, Ticker.create and Time.after_func (TZ05). `block` is the
 * channel's block (the 128-byte header and one i64), which the generated code allocates so that
 * the allocation is counted as the program's; the runtime frees it with `release` when it drops
 * the last reference. A timer without a handle holds the second reference of a channel that the
 * caller keeps the first of; a handle's record holds the only one, and a FUNC timer's channel is
 * closed at once (Timer.channel of an after_func). Returns the record. */
static tzr_timer *tzr_timer_record(void *block, void (*release)(void *), int64_t ns, int64_t period, int32_t kind,
                                   int32_t handle, void (*run)(void *), void (*drop)(void *), void *box) {
    tzr_start();
    tzr_chan *c = block;
    tsuzuri_tz_chan_init(c, (int64_t)sizeof(int64_t), 1);
    if (!handle) atomic_store(&c->refs, 2);
    if (kind == TZR_TIMER_FUNC) c->closed = 1;
    tzr_timer *t = calloc(1, sizeof *t);
    if (t == NULL) tzr_fatal("out of memory");
    t->when = tzr_deadline(tzr_nanotime(), ns);
    t->period = period;
    t->kind = kind;
    t->handle = handle;
    t->chan = c;
    t->release = release;
    t->run = run;
    t->drop = drop;
    t->box = box;
    return t;
}

TZR_API void *tsuzuri_tz_timer_new(void *block, void (*release)(void *), int64_t ns, int64_t period, int32_t kind,
                                   int32_t handle, void (*run)(void *), void (*drop)(void *), void *box) {
    tzr_timer *t = tzr_timer_record(block, release, ns, period, kind, handle, run, drop, box);
    tzr_timer_arm(t, tzr_timer_scope(tzr_current()), 0);
    return t;
}

/* Timer.channel and Ticker.channel: one more handle of the record's channel. */
TZR_API void *tsuzuri_tz_timer_chan(void *timer) {
    tzr_chan *c = ((tzr_timer *)timer)->chan;
    tsuzuri_tz_chan_share(c);
    return c;
}

/* Timer.stop and Ticker.stop: returns 1 when the timer was pending. Its channel keeps no time that
 * it sent before (Go 1.23); a FUNC timer keeps its function for a reset. */
TZR_API int32_t tsuzuri_tz_timer_stop(void *timer) {
    tzr_timer *t = timer;
    tzr_lock_acquire(&tzr_timers.lock);
    int taken = tzr_timer_take(t);
    if (t->kind != TZR_TIMER_FUNC) tzr_timer_discard(t->chan);
    tzr_lock_release(&tzr_timers.lock);
    if (taken == 2) tzr_timers_gone(1);
    return taken != 0;
}

/* Timer.reset and Ticker.reset: the timer fires `ns` from now (a ticker every `ns` from then on),
 * listed in the caller's boundary. Returns 1 when it was pending. A FUNC timer whose function has
 * run or was dropped has nothing to start: it stays as it is and returns 0. */
TZR_API int32_t tsuzuri_tz_timer_reset(void *timer, int64_t ns) {
    tzr_timer *t = timer;
    int64_t when = tzr_deadline(tzr_nanotime(), ns);
    tzr_scope *scope = tzr_timer_scope(tzr_current());
    tzr_lock_acquire(&tzr_timers.lock);
    if (t->kind == TZR_TIMER_FUNC && t->box == NULL) {
        tzr_lock_release(&tzr_timers.lock);
        return 0;
    }
    int taken = tzr_timer_take(t);
    tzr_timer_unlist(t);
    if (t->kind != TZR_TIMER_FUNC) tzr_timer_discard(t->chan);
    t->when = when;
    if (t->kind == TZR_TIMER_TICK) t->period = ns;
    tzr_timer_list(t, scope);
    tzr_heap_push(t);
    if (taken != 2) atomic_fetch_add(&tzr_sched.ntimers, 1);
    tzr_lock_release(&tzr_timers.lock);
    return taken != 0;
}

/* Starts a dormant timer now, in the boundary that it is listed in (TZ07: a context's cancel starts
 * the functions of Context.after_func). A timer that is not dormant stays as it is. */
static void tzr_timer_wake(tzr_timer *t) {
    tzr_lock_acquire(&tzr_timers.lock);
    if (t->state == TZR_TIMER_DORMANT) {
        t->when = tzr_nanotime();
        tzr_heap_push(t);
        atomic_fetch_add(&tzr_sched.ntimers, 1);
    }
    tzr_lock_release(&tzr_timers.lock);
}

/* The drop of a Timer or Ticker handle: stops the timer, drops a FUNC timer's function that has not
 * run, drops the record's reference to the channel and frees the record. */
TZR_API void tsuzuri_tz_timer_free(void *timer) {
    tzr_timer *t = timer;
    tzr_lock_acquire(&tzr_timers.lock);
    int taken = tzr_timer_take(t);
    tzr_timer_unlist(t);
    void *box = t->box;
    t->box = NULL;
    tzr_lock_release(&tzr_timers.lock);
    if (taken == 2) tzr_timers_gone(1);
    if (box) t->drop(box);
    tzr_timer_unref(t->chan, t->release);
    free(t);
}

static void tzr_ctx_unref(void *ctx);

/* Takes one timer out of the boundary `scope`'s list and stops it, then lets go of what the
 * runtime holds for it: a timer without a handle is freed with its reference to the channel, and a
 * FUNC timer's function is dropped (a handle keeps its stopped record). Returns 0 when the list
 * was empty. */
static int tzr_boundary_stop_one(tzr_scope *scope) {
    tzr_lock_acquire(&tzr_timers.lock);
    tzr_timer *t = scope->timers;
    if (t == NULL) {
        tzr_lock_release(&tzr_timers.lock);
        return 0;
    }
    int taken = tzr_timer_take(t);
    tzr_timer_unlist(t);
    void *box = t->box;
    void (*drop)(void *) = t->drop;
    t->box = NULL;
    /* a context's deadline lets go of its reference to the context, which owns the record */
    void *expired = t->kind == TZR_TIMER_CTX && taken ? t->ctx : NULL;
    /* after the lock goes, a handle's record may be freed by its drop on another G */
    tzr_timer *owned = t->handle || t->kind == TZR_TIMER_CTX ? NULL : t;
    tzr_lock_release(&tzr_timers.lock);
    if (taken == 2) tzr_timers_gone(1);
    if (expired) tzr_ctx_unref(expired);
    if (box) drop(box);
    if (owned) {
        tzr_timer_unref(owned->chan, owned->release);
        free(owned);
    }
    return 1;
}

/* After the boundary's tzroutines have finished: stops its timers and returns 1 when the scope must
 * be waited for again (a fired after_func spawned into it, or a dropped function made a timer). */
static int tzr_boundary_settle(tzr_scope *scope) {
    while (tzr_boundary_stop_one(scope)) {
    }
    tzr_lock_acquire(&scope->lock);
    int again = scope->live != 0;
    tzr_lock_release(&scope->lock);
    tzr_lock_acquire(&tzr_timers.lock);
    again |= scope->timers != NULL;
    tzr_lock_release(&tzr_timers.lock);
    return again;
}

/* ---------------------------------------------------------------- contexts (TZ07) */

/* Context: a tree of cancellation. Every context has a `done` channel (capacity 0, never sent on)
 * that cancel closes, for itself and every descendant. One lock guards every tree (D3), and it is
 * never held together with the timers' lock: a cancel marks under it and stops timers after it.
 *
 * References (`refs`): each Context handle, each live child (to its parent), the deadline timer
 * while it pends, and each AfterFunc. `handles` counts the handles alone: when the last one goes,
 * the deadline timer is stopped early (a child inherits the deadline and has a timer of its own).
 * The last reference unlinks the context from its parent, drops its reference to `done` and frees
 * it, then lets go of its parent's, in a loop. */
typedef struct tzr_ctx tzr_ctx;
typedef struct tzr_ctx_after {
    tzr_timer *timer;            /* a dormant FUNC timer, woken by the cancel */
    tzr_ctx *ctx;                /* holds a reference */
    struct tzr_ctx_after *next, *prev; /* in ctx->afters (tzr_ctx_lock) while listed */
    int listed;
    _Atomic int pins;            /* the handle, and a cancel that is waking the timer */
} tzr_ctx_after;

struct tzr_ctx {
    _Atomic uint64_t refs;
    _Atomic uint64_t handles;
    tzr_ctx *parent;
    tzr_ctx *children, *sibling, *prev_sibling; /* the live children (tzr_ctx_lock) */
    tzr_chan *done;              /* the context holds one reference */
    void (*release)(void *);     /* frees done's block */
    int err;                     /* tzr_ctx_lock: 0 none, 1 canceled, 2 deadline exceeded */
    int armed;                   /* tzr_ctx_lock: the deadline timer may pend */
    int64_t deadline;            /* INT64_MAX: none; fixed at creation */
    tzr_timer timer;             /* CTX */
    tzr_ctx_after *afters;       /* tzr_ctx_lock */
};
static tzr_lock tzr_ctx_lock;

/* Closes a channel without asking for the caller's G (the timer thread cancels too). */
static void tzr_chan_shut(tzr_chan *c) { (void)tzr_chan_close(c); }

static void tzr_ctx_unref(void *opaque) {
    tzr_ctx *ctx = opaque;
    while (ctx && atomic_fetch_sub_explicit(&ctx->refs, 1, memory_order_acq_rel) == 1) {
        tzr_ctx *parent = ctx->parent;
        if (parent) {
            tzr_lock_acquire(&tzr_ctx_lock);
            if (ctx->prev_sibling) ctx->prev_sibling->sibling = ctx->sibling;
            else if (parent->children == ctx) parent->children = ctx->sibling;
            if (ctx->sibling) ctx->sibling->prev_sibling = ctx->prev_sibling;
            tzr_lock_release(&tzr_ctx_lock);
        }
        tzr_timer_unref(ctx->done, ctx->release);
        free(ctx);
        ctx = parent;
    }
}

/* Stops the deadline timer of ctx, which a caller took (`armed` was 1) and holds a reference to;
 * lets go of the timer's reference when it was still pending (otherwise the timer thread does). */
static void tzr_ctx_disarm(tzr_ctx *ctx) {
    tzr_lock_acquire(&tzr_timers.lock);
    int taken = tzr_timer_take(&ctx->timer);
    tzr_lock_release(&tzr_timers.lock);
    if (taken == 2) tzr_timers_gone(1);
    if (taken) tzr_ctx_unref(ctx);
}

static void tzr_ctx_after_unpin(tzr_ctx_after *after) {
    if (atomic_fetch_sub_explicit(&after->pins, 1, memory_order_acq_rel) == 1) {
        tsuzuri_tz_timer_free(after->timer);
        tzr_ctx_unref(after->ctx);
        free(after);
    }
}

/* Cancels ctx and its descendants that are not cancelled yet, with `why` (1 canceled, 2 deadline):
 * without recursion, so that a deep tree uses no stack. */
static void tzr_ctx_cancel(tzr_ctx *ctx, int why) {
    size_t count = 0, capacity = 0, ndisarm = 0, nwake = 0, capdisarm = 0, capwake = 0;
    tzr_ctx **stack = NULL, **disarm = NULL;
    tzr_ctx_after **wake = NULL;
    tzr_lock_acquire(&tzr_ctx_lock);
    if (ctx->err) {
        tzr_lock_release(&tzr_ctx_lock);
        return;
    }
#define TZR_PUSH(array, n, cap, item)                                              \
    do {                                                                           \
        if ((n) == (cap)) {                                                        \
            size_t grown = (cap) ? (cap) * 2 : 16;                                 \
            void *next = realloc((array), grown * sizeof *(array));                \
            if (next == NULL) tzr_fatal("out of memory");                          \
            (array) = next;                                                        \
            (cap) = grown;                                                         \
        }                                                                          \
        (array)[(n)++] = (item);                                                   \
    } while (0)
    TZR_PUSH(stack, count, capacity, ctx);
    while (count > 0) {
        tzr_ctx *c = stack[--count];
        if (c->err) continue;
        c->err = why;
        tzr_chan_shut(c->done);
        if (c->armed) {
            c->armed = 0;
            atomic_fetch_add(&c->refs, 1);
            TZR_PUSH(disarm, ndisarm, capdisarm, c);
        }
        for (tzr_ctx_after *a = c->afters; a; a = a->next) {
            a->listed = 0;
            atomic_fetch_add(&a->pins, 1);
            TZR_PUSH(wake, nwake, capwake, a);
        }
        c->afters = NULL;
        for (tzr_ctx *child = c->children; child; child = child->sibling) TZR_PUSH(stack, count, capacity, child);
    }
#undef TZR_PUSH
    tzr_lock_release(&tzr_ctx_lock);
    for (size_t i = 0; i < ndisarm; i++) {
        tzr_ctx_disarm(disarm[i]);
        tzr_ctx_unref(disarm[i]);
    }
    for (size_t i = 0; i < nwake; i++) {
        tzr_timer_wake(wake[i]->timer);
        tzr_ctx_after_unpin(wake[i]);
    }
    free(stack);
    free(disarm);
    free(wake);
}

/* The deadline timer fired (on the timer thread, without the timers' lock): the timer's reference
 * keeps ctx alive until the end. */
static void tzr_ctx_expire(void *opaque) {
    tzr_ctx *ctx = opaque;
    tzr_lock_acquire(&tzr_ctx_lock);
    ctx->armed = 0;
    tzr_lock_release(&tzr_ctx_lock);
    tzr_ctx_cancel(ctx, 2);
    tzr_ctx_unref(ctx);
}

/* Context.background (parent NULL), with_cancel (deadline INT64_MAX), with_deadline and
 * with_timeout. `block` is the done channel's block from the generated code, freed with `release`.
 * A child of a cancelled context is cancelled with the same reason; a deadline later than the
 * parent's is the parent's; a deadline that has passed cancels at once. */
TZR_API void *tsuzuri_tz_ctx_new(void *parent_handle, int64_t deadline, void *block, void (*release)(void *)) {
    tzr_start();
    tzr_ctx *parent = parent_handle;
    tzr_ctx *ctx = calloc(1, sizeof *ctx);
    if (ctx == NULL) tzr_fatal("out of memory");
    tsuzuri_tz_chan_init(block, 1, 0);
    ctx->done = block;
    ctx->release = release;
    atomic_store(&ctx->refs, 1);
    atomic_store(&ctx->handles, 1);
    ctx->deadline = deadline;
    ctx->timer.kind = TZR_TIMER_CTX;
    ctx->timer.ctx = ctx;
    ctx->timer.state = TZR_TIMER_STOPPED;
    int inherited = 0;
    if (parent) {
        atomic_fetch_add(&parent->refs, 1);
        ctx->parent = parent;
        if (parent->deadline < ctx->deadline) ctx->deadline = parent->deadline;
        tzr_lock_acquire(&tzr_ctx_lock);
        if (parent->err) {
            ctx->err = parent->err;
            tzr_chan_shut(ctx->done);
            inherited = 1;
        }
        ctx->sibling = parent->children;
        if (parent->children) parent->children->prev_sibling = ctx;
        parent->children = ctx;
        tzr_lock_release(&tzr_ctx_lock);
    }
    if (inherited || ctx->deadline == INT64_MAX) return ctx;
    if (ctx->deadline <= tzr_nanotime()) {
        tzr_ctx_cancel(ctx, 2);
        return ctx;
    }
    atomic_fetch_add(&ctx->refs, 1); /* the timer's */
    ctx->timer.when = ctx->deadline;
    tzr_timer_arm(&ctx->timer, tzr_timer_scope(tzr_current()), 0);
    /* a cancel that came before `armed` did not see the timer: stop it here */
    tzr_lock_acquire(&tzr_ctx_lock);
    int late = ctx->err != 0;
    if (!late) ctx->armed = 1;
    tzr_lock_release(&tzr_ctx_lock);
    if (late) tzr_ctx_disarm(ctx);
    return ctx;
}

/* Context.with_timeout: the deadline is the nanoseconds from now; one that overflows is the latest
 * deadline (INT64_MAX - 1, since INT64_MAX is none). */
TZR_API void *tsuzuri_tz_ctx_timeout(void *parent, int64_t nanoseconds, void *block, void (*release)(void *)) {
    int64_t now = tzr_nanotime();
    int64_t deadline = nanoseconds >= INT64_MAX - now ? INT64_MAX - 1 : now + nanoseconds;
    return tsuzuri_tz_ctx_new(parent, deadline, block, release);
}

TZR_API void tsuzuri_tz_ctx_cancel(void *ctx) {
    (void)tzr_current(); /* a host thread counts as running while it wakes (the detector) */
    tzr_ctx_cancel(ctx, 1);
}

/* 0 none, 1 canceled, 2 deadline exceeded. */
TZR_API int32_t tsuzuri_tz_ctx_err(void *opaque) {
    tzr_ctx *ctx = opaque;
    tzr_lock_acquire(&tzr_ctx_lock);
    int32_t err = ctx->err;
    tzr_lock_release(&tzr_ctx_lock);
    return err;
}

/* INT64_MAX: none. */
TZR_API int64_t tsuzuri_tz_ctx_deadline(void *ctx) { return ((tzr_ctx *)ctx)->deadline; }

/* Context.done: one more handle of the done channel. */
TZR_API void *tsuzuri_tz_ctx_done(void *ctx) {
    tzr_chan *c = ((tzr_ctx *)ctx)->done;
    tsuzuri_tz_chan_share(c);
    return c;
}

TZR_API void tsuzuri_tz_ctx_share(void *opaque) {
    tzr_ctx *ctx = opaque;
    atomic_fetch_add(&ctx->handles, 1);
    atomic_fetch_add(&ctx->refs, 1);
}

/* The drop of a Context handle. */
TZR_API void tsuzuri_tz_ctx_release(void *opaque) {
    tzr_ctx *ctx = opaque;
    if (atomic_fetch_sub_explicit(&ctx->handles, 1, memory_order_acq_rel) == 1) {
        tzr_lock_acquire(&tzr_ctx_lock);
        int armed = ctx->armed;
        ctx->armed = 0;
        tzr_lock_release(&tzr_ctx_lock);
        if (armed) tzr_ctx_disarm(ctx);
    }
    tzr_ctx_unref(ctx);
}

/* Context.after_func (TZ07 Phase 3): the function runs on a new tzroutine once ctx is cancelled (at
 * once when it is already). The record is the AfterFunc handle's; its timer is a dormant FUNC timer
 * of the caller's boundary (the end of the boundary drops a function that never ran). */
TZR_API void *tsuzuri_tz_ctx_after(void *opaque, void *block, void (*release)(void *), void (*run)(void *),
                                   void (*drop)(void *), void *box) {
    tzr_ctx *ctx = opaque;
    tzr_ctx_after *after = calloc(1, sizeof *after);
    if (after == NULL) tzr_fatal("out of memory");
    after->timer = tzr_timer_record(block, release, INT64_MAX, 0, TZR_TIMER_FUNC, 1, run, drop, box);
    after->ctx = ctx;
    atomic_store(&after->pins, 1);
    atomic_fetch_add(&ctx->refs, 1);
    tzr_timer_arm(after->timer, tzr_timer_scope(tzr_current()), 1);
    tzr_lock_acquire(&tzr_ctx_lock);
    int cancelled = ctx->err != 0;
    if (!cancelled) {
        after->listed = 1;
        after->next = ctx->afters;
        if (ctx->afters) ctx->afters->prev = after;
        ctx->afters = after;
    }
    tzr_lock_release(&tzr_ctx_lock);
    if (cancelled) tzr_timer_wake(after->timer);
    return after;
}

/* Context.stop_after: 1 when the function had not started. */
TZR_API int32_t tsuzuri_tz_ctx_stop_after(void *after) {
    return tsuzuri_tz_timer_stop(((tzr_ctx_after *)after)->timer);
}

/* The drop of an AfterFunc: it never runs after this; a function that has not run is dropped. */
TZR_API void tsuzuri_tz_ctx_free_after(void *opaque) {
    tzr_ctx_after *after = opaque;
    tzr_lock_acquire(&tzr_ctx_lock);
    if (after->listed) {
        after->listed = 0;
        if (after->prev) after->prev->next = after->next;
        else after->ctx->afters = after->next;
        if (after->next) after->next->prev = after->prev;
    }
    tzr_lock_release(&tzr_ctx_lock);
    (void)tsuzuri_tz_timer_stop(after->timer);
    tzr_ctx_after_unpin(after);
}

/* ---------------------------------------------------------------- select */

/* One case of a select, built by the generated code in a stack array. */
typedef struct tzr_scase {
    tzr_chan *c;
    void *elem;      /* send: the value; recv: where the item goes */
    int32_t send;    /* 1 send, 0 recv */
    int32_t pad;
    tzr_sudog sudog;  /* scratch for the runtime */
} tzr_scase;

_Static_assert(sizeof(tzr_sudog) == 48, "a sudog is 6 words");
_Static_assert(sizeof(tzr_scase) == 72, "the generated code lays out a case as { ptr, ptr, i32, i32, [6 x i64] }");

typedef struct {
    tzr_scase *cases;
    uint16_t *lockorder;
    int64_t n;
} tzr_selock;

static void tzr_sellock(tzr_scase *cases, uint16_t *lockorder, int64_t n) {
    tzr_chan *last = NULL;
    for (int64_t i = 0; i < n; i++) {
        tzr_chan *c = cases[lockorder[i]].c;
        if (c != last) tzr_lock_acquire(&c->lock);
        last = c;
    }
}

static void tzr_selunlock(tzr_scase *cases, uint16_t *lockorder, int64_t n) {
    /* in reverse, and once per channel */
    for (int64_t i = n - 1; i >= 0; i--) {
        tzr_chan *c = cases[lockorder[i]].c;
        if (i > 0 && cases[lockorder[i - 1]].c == c) continue;
        tzr_lock_release(&c->lock);
    }
}

/* tzr_ready_unlock for the locks of a select. */
static void tzr_ready_selunlock(tzr_g *g, tzr_scase *cases, uint16_t *lockorder, int64_t n) {
    if (g->kind == TZR_KIND_THREAD) {
        tzr_ready(g);
        tzr_selunlock(cases, lockorder, n);
    } else {
        tzr_selunlock(cases, lockorder, n);
        tzr_ready(g);
    }
}

static void tzr_selunlock_arg(void *arg) {
    tzr_selock *s = arg;
    tzr_selunlock(s->cases, s->lockorder, s->n);
}

/* Returns the index of the chosen case, or -1 when block is 0 and no case was ready (the
 * `default` arm). *ok is 1 when a recv case got an item and 0 when its channel was closed; a send
 * case on a closed channel returns with *ok = 0 and the generated code traps. Returns -2 for a
 * doomed pseudo-G. n == 0 with block parks forever. n <= 65535. */
static int64_t tzr_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok, uint16_t *pollorder, uint16_t *lockorder);

TZR_API int64_t tsuzuri_tz_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok) {
    /* The two orders take 4 bytes a case: a few cases on the stack, more on the heap, so that a
     * select of 65,535 cases fits a 64 KiB stack and cannot jump over its guard. */
    enum { TZR_SELECT_STACK = 64 };
    if (n <= TZR_SELECT_STACK) {
        uint16_t orders[2 * TZR_SELECT_STACK];
        return tzr_select(cases, n, block, ok, orders, orders + TZR_SELECT_STACK);
    }
    uint16_t *orders = malloc((size_t)n * 2 * sizeof *orders);
    if (orders == NULL) tzr_fatal("out of memory");
    int64_t chosen = tzr_select(cases, n, block, ok, orders, orders + n);
    free(orders);
    return chosen;
}

static int64_t tzr_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok, uint16_t *pollorder, uint16_t *lockorder) {
    tzr_g *g = tzr_current();
    *ok = 0;
    if (n == 0) {
        if (!block) return -1;
        return tzr_park(NULL, NULL, "select (no cases)") ? -2 : -1;
    }
    /* pollorder: uniform random permutation (Fisher-Yates) */
    for (int64_t i = 0; i < n; i++) {
        int64_t j = (int64_t)(tzr_rand(&g->rand) % (uint64_t)(i + 1));
        pollorder[i] = pollorder[j];
        pollorder[j] = (uint16_t)i;
    }
    /* lockorder: by channel address, with a heap sort (O(n log n) in place, as Go does) built from
     * pollorder; cases of one channel end up next to each other */
#define TZR_CASE_KEY(k) ((uintptr_t)cases[k].c)
    for (int64_t i = 0; i < n; i++) {
        int64_t j = i;
        uint16_t k = pollorder[i];
        while (j > 0 && TZR_CASE_KEY(lockorder[(j - 1) / 2]) < TZR_CASE_KEY(k)) {
            lockorder[j] = lockorder[(j - 1) / 2];
            j = (j - 1) / 2;
        }
        lockorder[j] = k;
    }
    for (int64_t i = n - 1; i > 0; i--) {
        uint16_t k = lockorder[i];
        lockorder[i] = lockorder[0];
        int64_t j = 0;
        for (;;) {
            int64_t child = 2 * j + 1;
            if (child >= i) break;
            if (child + 1 < i && TZR_CASE_KEY(lockorder[child]) < TZR_CASE_KEY(lockorder[child + 1])) child++;
            if (TZR_CASE_KEY(k) >= TZR_CASE_KEY(lockorder[child])) break;
            lockorder[j] = lockorder[child];
            j = child;
        }
        lockorder[j] = k;
    }
#undef TZR_CASE_KEY
    tzr_sellock(cases, lockorder, n);
    /* pass 1: a ready case */
    for (int64_t k = 0; k < n; k++) {
        int64_t i = pollorder[k];
        tzr_scase *cs = &cases[i];
        tzr_chan *c = cs->c;
        if (cs->send) {
            if (c->closed) {
                tzr_selunlock(cases, lockorder, n);
                return i;
            }
            tzr_sudog *s = tzr_dequeue(&c->recvq);
            if (s) {
                tzr_send_to(c, s, cs->elem);
                tzr_ready_selunlock(s->g, cases, lockorder, n);
                *ok = 1;
                return i;
            }
            if (c->count < c->capacity) {
                memcpy(TZR_CHAN_BUF(c, c->sendx), cs->elem, c->elem_size);
                c->sendx = (c->sendx + 1) % c->capacity;
                c->count++;
                tzr_selunlock(cases, lockorder, n);
                *ok = 1;
                return i;
            }
        } else {
            tzr_sudog *s = tzr_dequeue(&c->sendq);
            if (s) {
                tzr_recv_from(c, s, cs->elem);
                tzr_ready_selunlock(s->g, cases, lockorder, n);
                *ok = 1;
                return i;
            }
            if (c->count > 0) {
                memcpy(cs->elem, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
                c->recvx = (c->recvx + 1) % c->capacity;
                c->count--;
                tzr_selunlock(cases, lockorder, n);
                *ok = 1;
                return i;
            }
            if (c->closed) {
                tzr_selunlock(cases, lockorder, n);
                return i;
            }
        }
    }
    if (!block) {
        tzr_selunlock(cases, lockorder, n);
        return -1;
    }
    /* pass 2: wait on every case */
    atomic_store(&g->select_done, 0);
    g->param = NULL;
    for (int64_t k = 0; k < n; k++) {
        int64_t i = pollorder[k]; /* random, so that cases on one channel are woken alike */
        tzr_scase *cs = &cases[i];
        cs->sudog = (tzr_sudog){.g = g, .elem = cs->elem, .c = cs->c, .is_select = 1};
        tzr_enqueue(cs->send ? &cs->c->sendq : &cs->c->recvq, &cs->sudog);
    }
    tzr_selock arg = {cases, lockorder, n};
    if (tzr_park(tzr_selunlock_arg, &arg, "select")) {
        /* The detector woke it: cancel every case, unless a waker chose one first (with its channel
         * locked, so that wake is over and pass 3 below finishes the select normally). */
        tzr_sellock(cases, lockorder, n);
        if (g->param == NULL) {
            for (int64_t i = 0; i < n; i++) tzr_unlink(cases[i].send ? &cases[i].c->sendq : &cases[i].c->recvq, &cases[i].sudog);
            tzr_selunlock(cases, lockorder, n);
            return -2;
        }
        tzr_selunlock(cases, lockorder, n);
    }
    /* pass 3: the waker set g->param to the winning sudog; leave the other queues */
    tzr_sellock(cases, lockorder, n);
    tzr_sudog *won = g->param;
    int64_t chosen = -1;
    for (int64_t i = 0; i < n; i++) {
        tzr_scase *cs = &cases[i];
        if (&cs->sudog == won) chosen = i;
        else tzr_unlink(cs->send ? &cs->c->sendq : &cs->c->recvq, &cs->sudog);
    }
    tzr_selunlock(cases, lockorder, n);
    if (chosen < 0) tzr_fatal("select woke without a case");
    *ok = won->success;
    return chosen;
}
#else
/* ---------------------------------------------------------------- select (one thread) */

/* A case as the generated code lays it out, `{ ptr, ptr, i32, i32, [6 x i64] }`, whatever the
 * size of a pointer. */
typedef struct tzr_scase {
    tzr_chan *c;
    void *elem;
    int32_t send;
    int32_t pad;
    uint64_t scratch[6];
} tzr_scase;

/* With one thread no queue holds a waiter, so a case is ready when it can complete at once; among
 * the ready cases one is chosen uniformly (reservoir sampling, as the native select's random order
 * does). A select that would wait returns -2, the deadlock status, and the generated code traps. */
TZR_API int64_t tsuzuri_tz_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok) {
    tzr_g *g = tzr_current();
    *ok = 0;
    int64_t chosen = -1;
    uint64_t ready = 0;
    for (int64_t i = 0; i < n; i++) {
        tzr_chan *c = cases[i].c;
        int now = cases[i].send ? (c->closed || c->count < c->capacity) : (c->count > 0 || c->closed);
        if (now && tzr_rand(&g->rand) % ++ready == 0) chosen = i;
    }
    if (chosen < 0) return block ? -2 : -1;
    tzr_scase *cs = &cases[chosen];
    tzr_chan *c = cs->c;
    if (cs->send) {
        if (c->closed) return chosen; /* *ok == 0: the generated code traps */
        memcpy(TZR_CHAN_BUF(c, c->sendx), cs->elem, c->elem_size);
        c->sendx = (c->sendx + 1) % c->capacity;
        c->count++;
        *ok = 1;
    } else if (c->count > 0) {
        memcpy(cs->elem, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
        c->recvx = (c->recvx + 1) % c->capacity;
        c->count--;
        *ok = 1;
    }
    return chosen; /* a receive from a closed and empty channel: *ok == 0 */
}
#endif /* TZR_SINGLE */
