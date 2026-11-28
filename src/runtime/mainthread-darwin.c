/* MainThread (M09): hands owned functions to the process main thread of an Apple target through the
 * main dispatch queue, which `[NSApp run]`, `CFRunLoopRun` and `dispatch_main` serve. A function that
 * is still waiting at the end of the boundary of `main`, of a test or of a bench sample is dropped
 * without being called. This part joins the C unit only when the program uses `MainThread`. */
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
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
void tsuzuri_report_log(int32_t category, const char *text, int64_t length);
#endif

/* As the tzroutine runtime's functions: two Tsuzuri objects in one host share one definition. */
#define TZ_MAINTHREAD_API __attribute__((weak, visibility("hidden")))

/* One posted function that has not run. `run` calls the owned function, drops it and frees its box;
 * `drop` drops it without calling it and frees its box. */
struct tz_mainthread_record {
    struct tz_mainthread_record *prev;
    struct tz_mainthread_record *next;
    void (*run)(void *);
    void (*drop)(void *);
    void *box;
    int queued;  /* the main queue holds the record and calls tz_mainthread_trampoline with it */
    int dropped; /* the boundary's end dropped the function; the trampoline only frees the record */
};

static pthread_mutex_t tz_mainthread_lock = PTHREAD_MUTEX_INITIALIZER;
/* The records whose function has neither run nor been dropped, in the order of the posts. */
static struct tz_mainthread_record *tz_mainthread_pending;
static struct tz_mainthread_record *tz_mainthread_last;
/* The boundary of `main`, of the test or of the last bench sample has ended. */
static int tz_mainthread_done;

static void tz_mainthread_unlink(struct tz_mainthread_record *record) {
    if (record->prev) {
        record->prev->next = record->next;
    } else {
        tz_mainthread_pending = record->next;
    }
    if (record->next) {
        record->next->prev = record->prev;
    } else {
        tz_mainthread_last = record->prev;
    }
    record->prev = NULL;
    record->next = NULL;
}

/* The main queue calls this on the main thread. The function and its drop run outside the lock: either
 * may post again. */
static void tz_mainthread_trampoline(void *context) {
    struct tz_mainthread_record *record = context;
    pthread_mutex_lock(&tz_mainthread_lock);
    if (record->dropped) {
        pthread_mutex_unlock(&tz_mainthread_lock);
        free(record);
        return;
    }
    tz_mainthread_unlink(record);
    pthread_mutex_unlock(&tz_mainthread_lock);
    record->run(record->box);
    free(record);
}

/* MainThread.post (`nanoseconds` 0) and MainThread.after: the main thread calls `run(box)` once, not
 * before `nanoseconds` have passed. After the end of the boundary the function is dropped here. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_post(int64_t nanoseconds, void (*run)(void *), void (*drop)(void *), void *box) {
    struct tz_mainthread_record *record = calloc(1, sizeof *record);
    if (record == NULL) {
        static const char message[] = "Tsuzuri runtime: out of memory\n";
#if defined(TZ_REPORT_OSLOG)
        if (tsuzuri_report_to_log()) {
            tsuzuri_report_log(1, message, (int64_t)(sizeof message - 1));
            abort();
        }
#endif
        (void)!write(2, message, sizeof message - 1);
        abort();
    }
    record->run = run;
    record->drop = drop;
    record->box = box;
    dispatch_time_t when = DISPATCH_TIME_NOW;
    if (nanoseconds > 0) when = dispatch_time(DISPATCH_TIME_NOW, nanoseconds);
    /* A deadline that saturates never comes, and the queue must not get it: the function waits for
     * the drop at the end of the boundary. */
    record->queued = when != DISPATCH_TIME_FOREVER;
    pthread_mutex_lock(&tz_mainthread_lock);
    if (tz_mainthread_done) {
        pthread_mutex_unlock(&tz_mainthread_lock);
        free(record);
        drop(box);
        return;
    }
    record->prev = tz_mainthread_last;
    if (tz_mainthread_last) {
        tz_mainthread_last->next = record;
    } else {
        tz_mainthread_pending = record;
    }
    tz_mainthread_last = record;
    int queued = record->queued;
    pthread_mutex_unlock(&tz_mainthread_lock);
    /* From here the record is the queue's (or the boundary's, which frees one that is not queued). */
    if (!queued) return;
    if (nanoseconds > 0) {
        dispatch_after_f(when, dispatch_get_main_queue(), record, tz_mainthread_trampoline);
    } else {
        dispatch_async_f(dispatch_get_main_queue(), record, tz_mainthread_trampoline);
    }
}

TZ_MAINTHREAD_API int32_t tsuzuri_mainthread_is_current(void) { return pthread_main_np() != 0; }

/* The entry of a bench sample: the functions posted in it wait for the main thread again. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_boundary_open(void) {
    pthread_mutex_lock(&tz_mainthread_lock);
    tz_mainthread_done = 0;
    pthread_mutex_unlock(&tz_mainthread_lock);
}

/* The end of the body of `main`, of a test or of a bench sample, before the boundary waits for its
 * tzroutines: drops every function that has not run. A drop runs the `Drop` of what the function
 * captured, outside the lock and in the order of the posts, and what it posts is dropped where it
 * posts. */
TZ_MAINTHREAD_API void tsuzuri_mainthread_boundary_done(void) {
    pthread_mutex_lock(&tz_mainthread_lock);
    tz_mainthread_done = 1;
    for (;;) {
        struct tz_mainthread_record *record = tz_mainthread_pending;
        if (record == NULL) break;
        tz_mainthread_unlink(record);
        void (*drop)(void *) = record->drop;
        void *box = record->box;
        int queued = record->queued;
        /* The queue still holds a queued record: its trampoline frees it. */
        record->dropped = 1;
        pthread_mutex_unlock(&tz_mainthread_lock);
        if (!queued) free(record);
        drop(box);
        pthread_mutex_lock(&tz_mainthread_lock);
    }
    pthread_mutex_unlock(&tz_mainthread_lock);
}
