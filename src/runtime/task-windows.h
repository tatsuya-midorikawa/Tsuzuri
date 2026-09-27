#ifndef TZ_TASK_WINDOWS_H
#define TZ_TASK_WINDOWS_H
#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef TZ_TASK_CREATE_THREAD
#define TZ_TASK_CREATE_THREAD CreateThread
#endif
#ifndef TZ_TASK_WAIT_THREAD
#define TZ_TASK_WAIT_THREAD WaitForSingleObject
#endif
#ifndef TZ_TASK_CLOSE_THREAD
#define TZ_TASK_CLOSE_THREAD CloseHandle
#endif

typedef SRWLOCK pthread_mutex_t;
typedef CONDITION_VARIABLE pthread_cond_t;
typedef INIT_ONCE pthread_once_t;
typedef HANDLE pthread_t;
typedef void pthread_attr_t;
#define PTHREAD_MUTEX_INITIALIZER SRWLOCK_INIT
#define PTHREAD_COND_INITIALIZER CONDITION_VARIABLE_INIT
#define PTHREAD_ONCE_INIT INIT_ONCE_STATIC_INIT
#define _SC_NPROCESSORS_ONLN 1

static _Noreturn void tz_windows_fail(const char *operation) {
    fprintf(stderr, "Tsuzuri task runtime: %s failed (%lu)\n", operation, (unsigned long)GetLastError());
    abort();
}

static inline long sysconf(int name) {
    (void)name;
    return (long)GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
}

static inline int pthread_mutex_lock(pthread_mutex_t *mutex) { AcquireSRWLockExclusive(mutex); return 0; }
static inline int pthread_mutex_unlock(pthread_mutex_t *mutex) { ReleaseSRWLockExclusive(mutex); return 0; }
static inline int pthread_mutex_destroy(pthread_mutex_t *mutex) { (void)mutex; return 0; }
static inline int pthread_cond_broadcast(pthread_cond_t *condition) { WakeAllConditionVariable(condition); return 0; }
static inline int pthread_cond_destroy(pthread_cond_t *condition) { (void)condition; return 0; }
static inline int pthread_cond_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!SleepConditionVariableSRW(condition, mutex, INFINITE, 0)) tz_windows_fail("SleepConditionVariableSRW");
    return 0;
}

struct tz_windows_start { void *(*run)(void *); void *context; };

static DWORD WINAPI tz_windows_thread(LPVOID pointer) {
    struct tz_windows_start start = *(struct tz_windows_start *)pointer;
    free(pointer);
    start.run(start.context);
    return 0;
}

static inline int pthread_create(pthread_t *thread, const pthread_attr_t *attributes, void *(*run)(void *), void *context) {
    (void)attributes;
    struct tz_windows_start *start = malloc(sizeof(*start));
    if (!start) tz_windows_fail("allocate worker context");
    start->run = run;
    start->context = context;
    *thread = TZ_TASK_CREATE_THREAD(NULL, 0, tz_windows_thread, start, 0, NULL);
    if (!*thread) { free(start); tz_windows_fail("CreateThread"); }
    return 0;
}

static inline int pthread_join(pthread_t thread, void **result) {
    (void)result;
    if (TZ_TASK_WAIT_THREAD(thread, INFINITE) != WAIT_OBJECT_0) tz_windows_fail("WaitForSingleObject");
    if (!TZ_TASK_CLOSE_THREAD(thread)) tz_windows_fail("CloseHandle");
    return 0;
}

static BOOL CALLBACK tz_windows_once(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    (void)once;
    (void)context;
    void (**initialize)(void) = parameter;
    (*initialize)();
    return TRUE;
}

static inline int pthread_once(pthread_once_t *once, void (*initialize)(void)) {
    if (!InitOnceExecuteOnce(once, tz_windows_once, &initialize, NULL)) tz_windows_fail("InitOnceExecuteOnce");
    return 0;
}

#endif
