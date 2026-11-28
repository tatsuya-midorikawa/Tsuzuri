// M09: the C host of tests/main_thread/handles. It hands out a handle that stands for an object
// of the main thread, counts the handles that are alive, and calls the program's callbacks on the
// calling thread or on a thread of its own, as a framework's delegate or completion handler would.
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>

typedef void (*m09_callback)(int64_t);

static int64_t m09_live;

void *m09_window_make(void) {
    m09_live += 1;
    return malloc(1);
}

void m09_window_close(void *window) {
    m09_live -= 1;
    free(window);
}

int64_t m09_windows_live(void) { return m09_live; }

void m09_call_here(m09_callback callback, int64_t value) { callback(value); }

struct m09_call {
    m09_callback callback;
    int64_t value;
};

static void *m09_thread(void *argument) {
    struct m09_call *call = argument;
    call->callback(call->value);
    return NULL;
}

// Calls `callback(value)` on a new thread and returns when it has returned.
void m09_call_on_thread(m09_callback callback, int64_t value) {
    struct m09_call call = {callback, value};
    pthread_t thread;
    if (pthread_create(&thread, NULL, m09_thread, &call) != 0) abort();
    pthread_join(thread, NULL);
}
