// The native reactor of Async.block_on (B08 Phase 3): a monotonic clock in milliseconds, a wait
// that ends at a deadline or as soon as another thread posts a completion, and the completions
// that other threads posted. Each thread has its own executor, and an operation id carries the
// index of the thread that began the operation in its bits 39 to 62 (src/llvm.rs), so a posted
// completion goes to that thread's mailbox. macOS and Linux use POSIX threads; Windows uses a
// slim reader/writer lock and a condition variable. The driver links it when a program reaches
// Async.block_on. Sockets (E09) will add their readiness to the same wait.
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
#include <windows.h>
#else
#include <pthread.h>
#include <time.h>
#endif
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef EINVAL
#define EINVAL 22
#endif
#ifndef EOVERFLOW
#define EOVERFLOW 132
#endif

#if defined(_WIN32)
// Windows objects carry no weak or hidden symbols, and one runtime object serves a program.
#define TZ_ASYNC_API
#define TZ_ASYNC_EXPORT
#else
#define TZ_ASYNC_API __attribute__((weak, visibility("hidden")))
// The host calls tsuzuri_async_post from any thread, so objects and libraries keep it visible.
#define TZ_ASYNC_EXPORT __attribute__((weak))
#endif

#define TZ_ASYNC_THREAD_SHIFT 39

extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

struct tz_async_pending {
    int64_t operation;
    int posted;
};

// The completions posted to one thread: a ring of (operation, value) pairs. A mailbox exists only
// while that thread has registered operations.
struct tz_async_mailbox {
    int64_t thread;
    int64_t *queue;
    size_t head, count, capacity;
    struct tz_async_pending *pending;
    size_t pending_count, pending_capacity;
};
static struct tz_async_mailbox *tz_async_boxes;
static size_t tz_async_box_count, tz_async_box_capacity;

static void tz_async_check(int status, const char *action) {
    if (status == 0) return;
    fprintf(stderr, "Tsuzuri async runtime: %s failed (%d)\n", action, status);
    abort();
}

static void *tz_async_allocate(size_t bytes) {
    if (bytes > INT64_MAX) tz_async_check(EOVERFLOW, "mailbox size");
    return tsuzuri_alloc((int64_t)bytes);
}

static void *tz_async_grow(void *previous, size_t used, size_t bytes) {
    void *next = tz_async_allocate(bytes);
    if (used) memcpy(next, previous, used);
    tsuzuri_free(previous);
    return next;
}

// The platform layer: one lock and one condition variable guard every mailbox. A wait gives the
// lock up and takes it back, and never returns early without the caller measuring the clock
// again, so a wake-up may be spurious.
#if defined(_WIN32)
static SRWLOCK tz_async_lock = SRWLOCK_INIT;
static CONDITION_VARIABLE tz_async_signal = CONDITION_VARIABLE_INIT;

// A failed call that left no error code still fails.
static int tz_async_last_error(void) {
    DWORD error = GetLastError();
    return error ? (int)error : EINVAL;
}

static void tz_async_ready(void) {}
static void tz_async_acquire(void) { AcquireSRWLockExclusive(&tz_async_lock); }
static void tz_async_release(void) { ReleaseSRWLockExclusive(&tz_async_lock); }
static void tz_async_notify(void) { WakeAllConditionVariable(&tz_async_signal); }

static void tz_async_wait_signal(void) {
    if (!SleepConditionVariableSRW(&tz_async_signal, &tz_async_lock, INFINITE, 0)) {
        tz_async_check(tz_async_last_error(), "completion wait");
    }
}

// Waits for at most `milliseconds`, a positive count below a day. The timeout follows the
// system timer, which Windows rounds up to its tick (about 15.6 ms unless a host raised it).
static void tz_async_wait_timeout(int64_t milliseconds) {
    if (!SleepConditionVariableSRW(&tz_async_signal, &tz_async_lock, (DWORD)milliseconds, 0)
        && GetLastError() != ERROR_TIMEOUT) {
        tz_async_check(tz_async_last_error(), "timer wait");
    }
}
#else
static pthread_mutex_t tz_async_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t tz_async_signal;
static pthread_once_t tz_async_once = PTHREAD_ONCE_INIT;

static void tz_async_init(void) {
    pthread_condattr_t attributes;
    tz_async_check(pthread_condattr_init(&attributes), "condition attributes");
#if !defined(__APPLE__)
    // macOS has no condition clock; its relative wait below measures the monotonic clock itself.
    tz_async_check(pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC), "condition clock");
#endif
    tz_async_check(pthread_cond_init(&tz_async_signal, &attributes), "condition initialization");
    tz_async_check(pthread_condattr_destroy(&attributes), "condition attributes cleanup");
}

static void tz_async_ready(void) {
    tz_async_check(pthread_once(&tz_async_once, tz_async_init), "reactor initialization");
}
static void tz_async_acquire(void) { tz_async_check(pthread_mutex_lock(&tz_async_lock), "mailbox lock"); }
static void tz_async_release(void) { tz_async_check(pthread_mutex_unlock(&tz_async_lock), "mailbox unlock"); }
static void tz_async_notify(void) {
    tz_async_check(pthread_cond_broadcast(&tz_async_signal), "completion notification");
}

static void tz_async_wait_signal(void) {
    tz_async_check(pthread_cond_wait(&tz_async_signal, &tz_async_lock), "completion wait");
}

// Waits for at most `milliseconds`, a positive count below a day.
static void tz_async_wait_timeout(int64_t milliseconds) {
    struct timespec until = {(time_t)(milliseconds / 1000), (long)(milliseconds % 1000) * 1000000L};
#if defined(__APPLE__)
    int status = pthread_cond_timedwait_relative_np(&tz_async_signal, &tz_async_lock, &until);
#else
    struct timespec start;
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) tz_async_check(errno, "monotonic clock");
    until.tv_sec += start.tv_sec;
    until.tv_nsec += start.tv_nsec;
    if (until.tv_nsec >= 1000000000L) {
        until.tv_sec++;
        until.tv_nsec -= 1000000000L;
    }
    int status = pthread_cond_timedwait(&tz_async_signal, &tz_async_lock, &until);
#endif
    if (status != ETIMEDOUT) tz_async_check(status, "timer wait");
}
#endif

// The caller holds the lock.
static struct tz_async_mailbox *tz_async_find(int64_t thread) {
    for (size_t index = 0; index < tz_async_box_count; index++) {
        if (tz_async_boxes[index].thread == thread) return &tz_async_boxes[index];
    }
    return NULL;
}

// The caller holds the lock. Pointers to mailboxes stay valid until the next call.
static struct tz_async_mailbox *tz_async_open(int64_t thread) {
    struct tz_async_mailbox *box = tz_async_find(thread);
    if (box) return box;
    if (tz_async_box_count == tz_async_box_capacity) {
        size_t capacity = tz_async_box_capacity ? tz_async_box_capacity * 2 : 4;
        if (capacity > SIZE_MAX / sizeof(struct tz_async_mailbox)) tz_async_check(EOVERFLOW, "mailbox capacity");
        struct tz_async_mailbox *boxes = tz_async_grow(tz_async_boxes,
            tz_async_box_count * sizeof(*boxes), capacity * sizeof(*boxes));
        tz_async_boxes = boxes;
        tz_async_box_capacity = capacity;
    }
    box = &tz_async_boxes[tz_async_box_count++];
    box->thread = thread;
    box->queue = NULL;
    box->head = box->count = box->capacity = 0;
    box->pending = NULL;
    box->pending_count = box->pending_capacity = 0;
    return box;
}

// ponytail: mailbox and operation lookup are linear under one mutex; shard by thread if measured
// completion throughput needs it.
static struct tz_async_pending *tz_async_pending(struct tz_async_mailbox *box, int64_t operation) {
    for (size_t index = 0; index < box->pending_count; index++) {
        if (box->pending[index].operation == operation) return &box->pending[index];
    }
    return NULL;
}

// The caller holds the lock; the mailbox is empty.
static void tz_async_close(struct tz_async_mailbox *box) {
    tsuzuri_free(box->queue);
    tsuzuri_free(box->pending);
    *box = tz_async_boxes[--tz_async_box_count];
    if (tz_async_box_count == 0) {
        tsuzuri_free(tz_async_boxes);
        tz_async_boxes = NULL;
        tz_async_box_capacity = 0;
    }
}

#if defined(_WIN32)
TZ_ASYNC_API int64_t tsuzuri_async_clock(void) {
    LARGE_INTEGER frequency, counter;
    if (!QueryPerformanceFrequency(&frequency) || !QueryPerformanceCounter(&counter)
        || frequency.QuadPart <= 0) {
        tz_async_check(tz_async_last_error(), "monotonic clock");
    }
    // Whole seconds and the rest apart: the rest times 1000 stays far below 2^63.
    int64_t seconds = counter.QuadPart / frequency.QuadPart;
    int64_t rest = counter.QuadPart % frequency.QuadPart;
    if (seconds < 0 || seconds > (INT64_MAX - 999) / 1000) tz_async_check(EOVERFLOW, "clock range");
    return seconds * 1000 + rest * 1000 / frequency.QuadPart;
}
#else
TZ_ASYNC_API int64_t tsuzuri_async_clock(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) tz_async_check(errno, "monotonic clock");
    if (now.tv_sec < 0 || now.tv_sec > (INT64_MAX - 999) / 1000) tz_async_check(EOVERFLOW, "clock range");
    return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}
#endif

TZ_ASYNC_API void tsuzuri_async_register(int64_t operation) {
    int64_t thread = operation > 0 ? operation >> TZ_ASYNC_THREAD_SHIFT : 0;
    if (thread == 0) tz_async_check(EINVAL, "operation registration");
    tz_async_ready();
    tz_async_acquire();
    struct tz_async_mailbox *box = tz_async_open(thread);
    if (tz_async_pending(box, operation) != NULL) tz_async_check(EINVAL, "duplicate operation");
    if (box->pending_count == box->pending_capacity) {
        size_t capacity = box->pending_capacity ? box->pending_capacity * 2 : 16;
        if (capacity > SIZE_MAX / sizeof(struct tz_async_pending)) tz_async_check(EOVERFLOW, "operation capacity");
        struct tz_async_pending *pending = tz_async_grow(box->pending,
            box->pending_count * sizeof(*pending), capacity * sizeof(*pending));
        box->pending = pending;
        box->pending_capacity = capacity;
    }
    box->pending[box->pending_count++] = (struct tz_async_pending){operation, 0};
    tz_async_release();
}

// Retiring an operation also removes a completion that raced with cancellation.
TZ_ASYNC_API void tsuzuri_async_retire(int64_t operation) {
    int64_t thread = operation > 0 ? operation >> TZ_ASYNC_THREAD_SHIFT : 0;
    tz_async_acquire();
    struct tz_async_mailbox *box = tz_async_find(thread);
    if (box) {
        struct tz_async_pending *pending = tz_async_pending(box, operation);
        if (pending) {
            *pending = box->pending[--box->pending_count];
            size_t kept = 0;
            for (size_t index = 0; index < box->count; index++) {
                size_t from = (box->head + index) % box->capacity;
                if (box->queue[2 * from] == operation) continue;
                size_t to = (box->head + kept++) % box->capacity;
                box->queue[2 * to] = box->queue[2 * from];
                box->queue[2 * to + 1] = box->queue[2 * from + 1];
            }
            box->count = kept;
            if (box->pending_count == 0) tz_async_close(box);
        }
    }
    tz_async_release();
}

// Returns 1 when queued, or 0 when the id is unknown, already posted, completed, or cancelled.
// A rejected post allocates nothing, including when it arrives after the executor has finished.
TZ_ASYNC_EXPORT int32_t tsuzuri_async_post(int64_t operation, int64_t value) {
    int64_t thread = operation > 0 ? operation >> TZ_ASYNC_THREAD_SHIFT : 0;
    if (thread <= 0) return 0;
    tz_async_ready();
    tz_async_acquire();
    struct tz_async_mailbox *box = tz_async_find(thread);
    struct tz_async_pending *pending = box ? tz_async_pending(box, operation) : NULL;
    if (!pending || pending->posted) {
        tz_async_release();
        return 0;
    }
    if (box->count == box->capacity) {
        size_t capacity = box->capacity ? box->capacity * 2 : 16;
        if (capacity > SIZE_MAX / (2 * sizeof(int64_t))) tz_async_check(EOVERFLOW, "completion capacity");
        int64_t *queue = tz_async_allocate(capacity * 2 * sizeof(int64_t));
        for (size_t index = 0; index < box->count; index++) {
            size_t from = (box->head + index) % box->capacity;
            queue[2 * index] = box->queue[2 * from];
            queue[2 * index + 1] = box->queue[2 * from + 1];
        }
        tsuzuri_free(box->queue);
        box->queue = queue;
        box->head = 0;
        box->capacity = capacity;
    }
    size_t at = (box->head + box->count) % box->capacity;
    box->queue[2 * at] = operation;
    box->queue[2 * at + 1] = value;
    box->count++;
    pending->posted = 1;
    tz_async_notify();
    tz_async_release();
    return 1;
}

// Takes the earliest completion posted to `thread`: 1 and the pair, or 0 when there is none.
TZ_ASYNC_API int32_t tsuzuri_async_take(int64_t thread, int64_t *operation, int64_t *value) {
    tz_async_ready();
    tz_async_acquire();
    struct tz_async_mailbox *box = tz_async_find(thread);
    int32_t found = box != NULL && box->count > 0;
    if (found) {
        *operation = box->queue[2 * box->head];
        *value = box->queue[2 * box->head + 1];
        box->head = (box->head + 1) % box->capacity;
        box->count--;
    } else {
        *operation = 0;
        *value = 0;
    }
    tz_async_release();
    return found;
}

// Blocks `thread` until the clock reaches `deadline` or a completion is posted to it; INT64_MAX
// waits for a post.
TZ_ASYNC_API void tsuzuri_async_wait(int64_t thread, int64_t deadline) {
    tz_async_ready();
    tz_async_acquire();
    for (;;) {
        struct tz_async_mailbox *box = tz_async_find(thread);
        if (box && box->count > 0) break;
        int64_t now = tsuzuri_async_clock();
        if (now >= deadline) break;
        if (deadline == INT64_MAX) {
            tz_async_wait_signal();
            continue;
        }
        // A day at most: the loop measures the clock again after any wake-up.
        int64_t remaining = deadline - now;
        if (remaining > INT64_C(86400000)) remaining = INT64_C(86400000);
        tz_async_wait_timeout(remaining);
    }
    tz_async_release();
}
