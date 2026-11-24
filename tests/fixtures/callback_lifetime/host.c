#undef NDEBUG
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#if defined(_WIN32)
#include <windows.h>
#else
#include <pthread.h>
#include <time.h>
#endif

typedef int64_t (*w12_cb)(int64_t);

static w12_cb g_stored = NULL;
static w12_cb g_atexit_cb = NULL;
static _Atomic int64_t g_mark = 0;

void w12_register(w12_cb cb) {
    g_stored = cb;
}

void w22_register_tagged(int64_t tag, w12_cb cb) {
    (void)tag;
    g_stored = cb;
}

int64_t w12_fire(int64_t x) {
    return g_stored ? g_stored(x) : -1;
}

typedef struct {
    w12_cb cb;
    int64_t n;
    int64_t sum;
} w12_sum_job;

static void sum_body(w12_sum_job *job) {
    int64_t total = 0;
    for (int64_t i = 1; i <= job->n; i++) {
        total += job->cb(i);
    }
    job->sum = total;
}

#if defined(_WIN32)
static DWORD WINAPI sum_win(LPVOID p) { sum_body(p); return 0; }
#else
static void *sum_posix(void *p) { sum_body(p); return NULL; }
#endif

int64_t w12_run_thread_sum(w12_cb cb, int64_t n) {
    w12_sum_job job = { cb, n, 0 };
#if defined(_WIN32)
    HANDLE th = CreateThread(NULL, 0, sum_win, &job, 0, NULL);
    if (!th) return -1;
    WaitForSingleObject(th, INFINITE);
    CloseHandle(th);
#else
    pthread_t th;
    if (pthread_create(&th, NULL, sum_posix, &job) != 0) return -1;
    pthread_join(th, NULL);
#endif
    return job.sum;
}

void w12_host_mark(int64_t v) {
    atomic_store(&g_mark, v);
}

typedef struct {
    w12_cb cb;
    int64_t arg;
    int64_t out;
} w12_pool_job;

static void pool_body(w12_pool_job *job) {
    int64_t ret = job->cb(job->arg);
    /* W12 spec 3: on a host thread outside every boundary, cb MUST wait for its spawned tzroutines
     * before returning, so g_mark is already visible right here without sleeping. */
    int64_t seen = atomic_load(&g_mark);
    job->out = ret + seen;
}

#if defined(_WIN32)
static DWORD WINAPI pool_win(LPVOID p) { pool_body(p); return 0; }
#else
static void *pool_posix(void *p) { pool_body(p); return NULL; }
#endif

int64_t w12_pool_submit_wait(w12_cb cb, int64_t arg) {
    atomic_store(&g_mark, 0);
    w12_pool_job job = { cb, arg, 0 };
#if defined(_WIN32)
    HANDLE th = CreateThread(NULL, 0, pool_win, &job, 0, NULL);
    if (!th) return -1;
    WaitForSingleObject(th, INFINITE);
    CloseHandle(th);
#else
    pthread_t th;
    if (pthread_create(&th, NULL, pool_posix, &job) != 0) return -1;
    pthread_join(th, NULL);
#endif
    return job.out;
}

static _Atomic int g_idle_ready = 0;

static void idle_body(w12_cb cb) {
    (void)cb(1);
    atomic_store(&g_idle_ready, 1);
#if defined(_WIN32)
    Sleep(5000);
#else
    struct timespec ts = { 5, 0 };
    nanosleep(&ts, NULL);
#endif
}

#if defined(_WIN32)
static DWORD WINAPI idle_win(LPVOID p) { idle_body((w12_cb)p); return 0; }
#else
static void *idle_posix(void *p) { idle_body((w12_cb)p); return NULL; }
#endif

void w12_start_idle_host_thread(w12_cb cb) {
    atomic_store(&g_idle_ready, 0);
#if defined(_WIN32)
    HANDLE th = CreateThread(NULL, 0, idle_win, (LPVOID)cb, 0, NULL);
    if (th) CloseHandle(th);
    while (!atomic_load(&g_idle_ready)) SwitchToThread();
#else
    pthread_t th;
    if (pthread_create(&th, NULL, idle_posix, (void *)cb) == 0) pthread_detach(th);
    while (!atomic_load(&g_idle_ready)) {
        struct timespec ts = { 0, 1000000 };
        nanosleep(&ts, NULL);
    }
#endif
}

static void w12_on_exit(void) {
    if (g_atexit_cb) (void)g_atexit_cb(5);
}

void w12_register_atexit(w12_cb cb) {
    g_atexit_cb = cb;
    atexit(w12_on_exit);
}
