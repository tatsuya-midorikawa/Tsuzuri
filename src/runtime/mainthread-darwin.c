/* MainThread (M09): hands owned functions to the process main thread of an Apple target through the
 * main dispatch queue, which `[NSApp run]`, `CFRunLoopRun` and `dispatch_main` serve. A function that
 * is still waiting at the end of the boundary of `main`, of a test or of a bench sample is dropped
 * without being called. This part joins the C unit only when the program uses `MainThread`.
 *
 * The functions wait in lists of this file, and the main queue holds only calls that serve those
 * lists and carry no pointer: at most one call for the posts, and a timer for the earliest deadline
 * (a libdispatch call cannot be taken back). So the end of a boundary frees every record, and a
 * program whose main thread never serves the queue (a bench that posts) keeps nothing per post. */
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

#if defined(__has_include) && __has_include(<dispatch/dispatch.h>)
#include <dispatch/dispatch.h>
#else
/* A sysroot without the libdispatch headers: the C functions and the main queue's data symbol of
 * libSystem, as <dispatch/dispatch.h> declares them. */
struct dispatch_queue_s;
typedef struct dispatch_queue_s *dispatch_queue_t;
typedef uint64_t dispatch_time_t;
typedef void (*dispatch_function_t)(void *);
extern struct dispatch_queue_s _dispatch_main_q;
#define dispatch_get_main_queue() (&_dispatch_main_q)
#define DISPATCH_TIME_NOW ((dispatch_time_t)0)
#define DISPATCH_TIME_FOREVER (~(dispatch_time_t)0)
extern dispatch_time_t dispatch_time(dispatch_time_t when, int64_t delta);
extern void dispatch_async_f(dispatch_queue_t queue, void *context, dispatch_function_t work);
extern void dispatch_after_f(dispatch_time_t when, dispatch_queue_t queue, void *context, dispatch_function_t work);
#endif

/* Declared here too: the feature macros of another part of the unit may hide it in <pthread.h>. */
extern int pthread_main_np(void);

#if defined(TZ_REPORT_OSLOG)
/* M19, src/runtime/oslog-darwin.c: an executable whose standard error is closed or /dev/null (an
 * application that Finder started) sends its reports to unified logging. Only the driver's unit of an
 * executable defines the macro. */
int32_t tsuzuri_report_to_log(void);
void tsuzuri_report_whole(int32_t category, const char *text, int64_t length);
#endif

/* As the tzroutine runtime's functions: two Tsuzuri objects in one host share one definition. */
#define TZ_MAINTHREAD_API __attribute__((weak, visibility("hidden")))

/* One posted function that has not run. `run` calls the owned function, drops it and frees its box;
 * `drop` drops it without calling it and frees its box. */
struct tz_mainthread_record {
    /* [0]: every record, in the order of the posts. [1]: the posts that wait for the next call of
     * tz_mainthread_drain, in their order, or the functions with a deadline, by deadline. */
    struct tz_mainthread_record *prev[2];
    struct tz_mainthread_record *next[2];
    void (*run)(void *);
    void (*drop)(void *);
    void *box;
    uint64_t order;    /* the number of the post, from 1 */
    uint64_t deadline; /* 0 for a post; else the monotonic time in nanoseconds, UINT64_MAX for never */
};

struct tz_mainthread_list {
    struct tz_mainthread_record *first;
    struct tz_mainthread_record *last;
};

static pthread_mutex_t tz_mainthread_lock = PTHREAD_MUTEX_INITIALIZER;
static struct tz_mainthread_list tz_mainthread_all;
static struct tz_mainthread_list tz_mainthread_ready;
static struct tz_mainthread_list tz_mainthread_timed;
static uint64_t tz_mainthread_posts;
/* The main queue holds a call of tz_mainthread_drain that has not begun. */
static int tz_mainthread_draining;
/* The deadlines of the timers that the main queue holds and that have not fired, latest first: a timer
 * is set only for a deadline that is earlier than all of them, so they fire from the end. One that
 * finds no room is not written down, which costs at most a timer that fires for nothing. */
enum { TZ_MAINTHREAD_TIMERS = 32 };
static uint64_t tz_mainthread_timers[TZ_MAINTHREAD_TIMERS];
static int tz_mainthread_timer_count;
/* The boundary of `main`, of the test or of the last bench sample has ended. */
static int tz_mainthread_done;

static void tz_mainthread_unlink(struct tz_mainthread_list *list, int links, struct tz_mainthread_record *record) {
    if (record->prev[links]) {
        record->prev[links]->next[links] = record->next[links];
    } else {
        list->first = record->next[links];
    }
    if (record->next[links]) {
        record->next[links]->prev[links] = record->prev[links];
    } else {
        list->last = record->prev[links];
    }
    record->prev[links] = NULL;
    record->next[links] = NULL;
}

/* Puts `record` after `before` (at the front when `before` is NULL). */
static void tz_mainthread_link(struct tz_mainthread_list *list, int links, struct tz_mainthread_record *before,
                               struct tz_mainthread_record *record) {
    struct tz_mainthread_record *after = before ? before->next[links] : list->first;
    record->prev[links] = before;
    record->next[links] = after;
    if (before) {
        before->next[links] = record;
    } else {
        list->first = record;
    }
    if (after) {
        after->prev[links] = record;
    } else {
        list->last = record;
    }
}

/* Takes `record` out of both of its lists. The caller holds the lock and owns the record after it. */
static void tz_mainthread_take(struct tz_mainthread_record *record) {
    tz_mainthread_unlink(&tz_mainthread_all, 0, record);
    tz_mainthread_unlink(record->deadline == 0 ? &tz_mainthread_ready : &tz_mainthread_timed, 1, record);
}

/* The clock of the deadlines. It also counts the time that the machine sleeps, which the timers of
 * libdispatch (DISPATCH_TIME_NOW) do not: a timer fires at or after the deadline it was set for. */
static uint64_t tz_mainthread_now(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * 1000000000u + (uint64_t)now.tv_nsec;
}

static void tz_mainthread_timer(void *context);

/* With the lock: makes sure that a timer of the main queue fires at `deadline` or before it. A
 * deadline that libdispatch saturates never comes, and its function waits for the end of the boundary. */
static void tz_mainthread_arm(uint64_t deadline, uint64_t now) {
    if (deadline == UINT64_MAX) return;
    /* A timer that fires earlier looks at the deadlines again when it fires. */
    if (tz_mainthread_timer_count > 0 && tz_mainthread_timers[tz_mainthread_timer_count - 1] <= deadline) return;
    uint64_t delta = deadline > now ? deadline - now : 0;
    if (delta > (uint64_t)INT64_MAX) return;
    dispatch_time_t when = dispatch_time(DISPATCH_TIME_NOW, (int64_t)delta);
    if (when == DISPATCH_TIME_FOREVER) return;
    if (tz_mainthread_timer_count < TZ_MAINTHREAD_TIMERS) tz_mainthread_timers[tz_mainthread_timer_count++] = deadline;
    /* The timer carries its deadline, not a record: nothing dangles when the boundary frees the records. */
    dispatch_after_f(when, dispatch_get_main_queue(), (void *)(uintptr_t)deadline, tz_mainthread_timer);
}

/* The main queue calls this on the main thread: the posts that were made before the call began run in
 * their order. One that a function posts meanwhile waits for the next call, so the queue's other work
 * gets its turn. Each function runs outside the lock: it may post again. */
static void tz_mainthread_drain(void *context) {
    (void)context;
    pthread_mutex_lock(&tz_mainthread_lock);
    tz_mainthread_draining = 0;
    uint64_t last = tz_mainthread_posts;
    for (;;) {
        struct tz_mainthread_record *record = tz_mainthread_ready.first;
        /* After the end of the boundary nothing runs: tsuzuri_mainthread_boundary_done drops what waits. */
        if (record == NULL || tz_mainthread_done || record->order > last) break;
        tz_mainthread_take(record);
        pthread_mutex_unlock(&tz_mainthread_lock);
        record->run(record->box);
        free(record);
        pthread_mutex_lock(&tz_mainthread_lock);
    }
    int again = !tz_mainthread_done && tz_mainthread_ready.first != NULL && !tz_mainthread_draining;
    if (again) tz_mainthread_draining = 1;
    pthread_mutex_unlock(&tz_mainthread_lock);
    if (again) dispatch_async_f(dispatch_get_main_queue(), NULL, tz_mainthread_drain);
}

/* A timer of the main queue fired, on the main thread: the functions whose deadline has come run by
 * deadline, and the next deadline gets its timer. */
static void tz_mainthread_timer(void *context) {
    uint64_t fired = (uint64_t)(uintptr_t)context;
    pthread_mutex_lock(&tz_mainthread_lock);
    for (int index = tz_mainthread_timer_count - 1; index >= 0; index--) {
        if (tz_mainthread_timers[index] != fired) continue;
        for (int next = index + 1; next < tz_mainthread_timer_count; next++) {
            tz_mainthread_timers[next - 1] = tz_mainthread_timers[next];
        }
        tz_mainthread_timer_count--;
        break;
    }
    /* Read once: a function that schedules itself again has a later deadline and waits for its timer. */
    uint64_t now = tz_mainthread_now();
    for (;;) {
        struct tz_mainthread_record *record = tz_mainthread_timed.first;
        if (record == NULL || tz_mainthread_done || record->deadline > now) break;
        tz_mainthread_take(record);
        pthread_mutex_unlock(&tz_mainthread_lock);
        record->run(record->box);
        free(record);
        pthread_mutex_lock(&tz_mainthread_lock);
    }
    if (!tz_mainthread_done && tz_mainthread_timed.first != NULL) {
        tz_mainthread_arm(tz_mainthread_timed.first->deadline, tz_mainthread_now());
    }
    pthread_mutex_unlock(&tz_mainthread_lock);
}

/* MainThread.post (`nanoseconds` 0) and MainThread.after: the main thread calls `run(box)` once, not
 * before `nanoseconds` have passed. After the end of the boundary the function is dropped here. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_post(int64_t nanoseconds, void (*run)(void *), void (*drop)(void *), void *box) {
    struct tz_mainthread_record *record = calloc(1, sizeof *record);
    if (record == NULL) {
        static const char message[] = "Tsuzuri runtime: out of memory\n";
#if defined(TZ_REPORT_OSLOG)
        if (tsuzuri_report_to_log()) {
            tsuzuri_report_whole(1, message, (int64_t)(sizeof message - 1));
            abort();
        }
#endif
        (void)!write(2, message, sizeof message - 1);
        abort();
    }
    record->run = run;
    record->drop = drop;
    record->box = box;
    pthread_mutex_lock(&tz_mainthread_lock);
    if (tz_mainthread_done) {
        pthread_mutex_unlock(&tz_mainthread_lock);
        free(record);
        drop(box);
        return;
    }
    record->order = ++tz_mainthread_posts;
    tz_mainthread_link(&tz_mainthread_all, 0, tz_mainthread_all.last, record);
    int drain = 0;
    if (nanoseconds <= 0) {
        tz_mainthread_link(&tz_mainthread_ready, 1, tz_mainthread_ready.last, record);
        drain = !tz_mainthread_draining;
        tz_mainthread_draining = 1;
    } else {
        uint64_t now = tz_mainthread_now();
        record->deadline = (uint64_t)nanoseconds > UINT64_MAX - 1 - now ? UINT64_MAX : now + (uint64_t)nanoseconds;
        /* By deadline, and among equal deadlines by post. Deadlines mostly grow: the search starts at the end. */
        struct tz_mainthread_record *before = tz_mainthread_timed.last;
        while (before != NULL && before->deadline > record->deadline) before = before->prev[1];
        tz_mainthread_link(&tz_mainthread_timed, 1, before, record);
        if (tz_mainthread_timed.first == record) tz_mainthread_arm(record->deadline, now);
    }
    pthread_mutex_unlock(&tz_mainthread_lock);
    if (drain) dispatch_async_f(dispatch_get_main_queue(), NULL, tz_mainthread_drain);
}

TZ_MAINTHREAD_API int32_t tsuzuri_mainthread_is_current(void) { return pthread_main_np() != 0; }

/* The entry of a bench sample: the functions posted in it wait for the main thread again. A call or
 * a timer that the main queue still holds from the sample before serves them too. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_boundary_open(void) {
    pthread_mutex_lock(&tz_mainthread_lock);
    tz_mainthread_done = 0;
    pthread_mutex_unlock(&tz_mainthread_lock);
}

/* The end of the body of `main`, of a test or of a bench sample, before the boundary waits for its
 * tzroutines: drops every function that has not run and frees its record. A drop runs the `Drop` of
 * what the function captured, outside the lock and in the order of the posts; what it posts is dropped
 * where it posts, and if it serves the main queue, the calls there find the boundary ended and run
 * nothing. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_boundary_done(void) {
    pthread_mutex_lock(&tz_mainthread_lock);
    tz_mainthread_done = 1;
    for (;;) {
        struct tz_mainthread_record *record = tz_mainthread_all.first;
        if (record == NULL) break;
        tz_mainthread_take(record);
        pthread_mutex_unlock(&tz_mainthread_lock);
        void (*drop)(void *) = record->drop;
        void *box = record->box;
        free(record);
        drop(box);
        pthread_mutex_lock(&tz_mainthread_lock);
    }
    pthread_mutex_unlock(&tz_mainthread_lock);
}
