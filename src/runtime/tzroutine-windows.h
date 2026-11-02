/* The Windows half of the tzroutine runtime (TZ01 Phase 2): the OS calls of src/runtime/tzroutine.c
 * mapped to Win32. A tzroutine is a Fiber (its stack, guard and registers, including the
 * floating-point state with FIBER_FLAG_FLOAT_SWITCH, are the OS's); waiting uses SRW locks and
 * condition variables. Every name starts with tzr_ or TZR_, so that the driver can put this file in
 * the same translation unit as task-windows.h. tzroutine.c declares tzr_fatal before including it. */
#ifndef TZR_WINDOWS_H
#define TZR_WINDOWS_H
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <windows.h>
#include <errno.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* No weak definitions on COFF (task.c does the same). */
#define TZR_API

/* The scheduler's lock and the timer thread's wakeup. */
typedef SRWLOCK tzr_mutex;
typedef CONDITION_VARIABLE tzr_cond;
#define TZR_MUTEX_INIT SRWLOCK_INIT
#define TZR_COND_INIT CONDITION_VARIABLE_INIT
static inline void tzr_mutex_lock(tzr_mutex *m) { AcquireSRWLockExclusive(m); }
static inline void tzr_mutex_unlock(tzr_mutex *m) { ReleaseSRWLockExclusive(m); }
static inline void tzr_cond_signal(tzr_cond *c) { WakeConditionVariable(c); }
/* The timer thread's wakeup needs nothing more: the timeout below is relative. */
static inline void tzr_timer_cond_init(tzr_cond *c) { (void)c; }
/* Waits at most `ns` nanoseconds (rounded up to whole milliseconds and at most one day; the caller
 * looks at the clock again), or until signalled when ns < 0. */
static inline void tzr_cond_wait_for(tzr_cond *c, tzr_mutex *m, int64_t ns) {
    const int64_t day = 86400000;
    int64_t ms = ns < 0 ? day : ns / 1000000 + (ns % 1000000 != 0);
    if (ms > day) ms = day;
    (void)SleepConditionVariableSRW(c, m, (DWORD)ms, 0); /* FALSE with ERROR_TIMEOUT is a timeout */
}

typedef INIT_ONCE tzr_once;
#define TZR_ONCE_INIT INIT_ONCE_STATIC_INIT
static BOOL CALLBACK tzr_once_call(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    (void)once;
    (void)context;
    void (**initialize)(void) = parameter;
    (*initialize)();
    return TRUE;
}
static inline void tzr_run_once(tzr_once *once, void (*initialize)(void)) {
    if (!InitOnceExecuteOnce(once, tzr_once_call, &initialize, NULL)) tzr_fatal("InitOnceExecuteOnce failed");
}

static inline void tzr_os_yield(void) { (void)SwitchToThread(); }
static inline long tzr_online_cpus(void) { return (long)GetActiveProcessorCount(ALL_PROCESSOR_GROUPS); }
static inline size_t tzr_page_size(void) {
    SYSTEM_INFO info;
    GetSystemInfo(&info);
    return (size_t)info.dwPageSize;
}

/* tzr_lock: an SRW lock (8 bytes, all-zero is unlocked). R1 makes the thread that locked it the one
 * that unlocks it, which SRW locks require. */
typedef struct { SRWLOCK lock; } tzr_lock;
_Static_assert(sizeof(tzr_lock) == 8, "a tzr_lock is 8 bytes");
static void tzr_lock_acquire(tzr_lock *l) { AcquireSRWLockExclusive(&l->lock); }
static void tzr_lock_release(tzr_lock *l) { ReleaseSRWLockExclusive(&l->lock); }

/* tzr_note: a one-shot wakeup for an OS thread (an idle M, a parked pseudo-G). */
typedef struct {
    SRWLOCK lock;
    CONDITION_VARIABLE cond;
    int set;
} tzr_note;

static void tzr_note_init(tzr_note *n) {
    InitializeSRWLock(&n->lock);
    InitializeConditionVariable(&n->cond);
    n->set = 0;
}
static void tzr_note_wake(tzr_note *n) {
    AcquireSRWLockExclusive(&n->lock);
    n->set = 1;
    WakeConditionVariable(&n->cond);
    ReleaseSRWLockExclusive(&n->lock);
}
static void tzr_note_sleep(tzr_note *n) {
    AcquireSRWLockExclusive(&n->lock);
    while (!n->set) {
        if (!SleepConditionVariableSRW(&n->cond, &n->lock, INFINITE, 0)) tzr_fatal("SleepConditionVariableSRW failed");
    }
    n->set = 0;
    ReleaseSRWLockExclusive(&n->lock);
}
static inline void tzr_note_destroy(tzr_note *n) { (void)n; }

static int64_t tzr_nanotime(void) {
    LARGE_INTEGER frequency, counter;
    QueryPerformanceFrequency(&frequency);
    QueryPerformanceCounter(&counter);
    /* Whole seconds and the rest apart, so that the product stays far below 2^63 (async.c does the same). */
    int64_t seconds = counter.QuadPart / frequency.QuadPart;
    int64_t rest = counter.QuadPart % frequency.QuadPart;
    return seconds * 1000000000 + rest * 1000000000 / frequency.QuadPart;
}

/* Starts a detached thread whose stack reserves `stack` bytes (0: the executable's default). */
typedef struct {
    void *(*run)(void *);
    void *arg;
} tzr_thread_args;
static DWORD WINAPI tzr_thread_entry(LPVOID pointer) {
    tzr_thread_args args = *(tzr_thread_args *)pointer;
    free(pointer);
    args.run(args.arg);
    return 0;
}
static void tzr_thread_start(void *(*run)(void *), void *arg, size_t stack, const char *failure) {
    tzr_thread_args *args = malloc(sizeof *args);
    if (args == NULL) tzr_fatal(failure);
    args->run = run;
    args->arg = arg;
    HANDLE thread = CreateThread(NULL, stack, tzr_thread_entry, args, stack ? STACK_SIZE_PARAM_IS_A_RESERVATION : 0, NULL);
    if (thread == NULL) tzr_fatal(failure);
    CloseHandle(thread);
}

/* g0 -> G and G -> g0. The fiber saves its own registers; sp is unused. */
#define TZR_TO_G(m, g) ((void)(m), SwitchToFiber((g)->fiber))
#define TZR_TO_G0(m, g) ((void)(g), SwitchToFiber((m)->g0_fiber))

/* A G's fiber: `reserve` bytes of address space, committed as the stack grows. A commit as large
 * as the reserve would make the OS round the reserve up to 1 MiB, so it stays below it. */
static void *tzr_fiber_create(size_t reserve, LPFIBER_START_ROUTINE start, void *g) {
    size_t commit = reserve / 4 < 64 * 1024 ? reserve / 4 : 64 * 1024;
    return CreateFiberEx(commit, reserve, FIBER_FLAG_FLOAT_SWITCH, start, g);
}

#endif
