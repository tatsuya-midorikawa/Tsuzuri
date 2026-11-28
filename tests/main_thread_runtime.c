/* Tests of the MainThread runtime (M09), run by tests/main_thread.mjs on macOS. By hand:
 * clang -std=c11 -Wall -Wextra -Werror -O1 tests/main_thread_runtime.c -framework CoreFoundation -o t && ./t
 * The expectations follow from the documented rules (posts run on the main thread in their order,
 * deadlines in theirs, what waits at the end of a boundary is dropped once in the order of the posts
 * and never called, and a boundary keeps no memory per post), not from the runtime under test.
 * TZ_TEST_NO_MALLOC_STATS leaves the memory check out (a sanitizer's allocator counts otherwise). */
#include "../src/runtime/mainthread-darwin.c"

#include <CoreFoundation/CoreFoundation.h>
#include <malloc/malloc.h>
#include <stdio.h>
#include <string.h>

#define CHECK(cond)                                                              \
    do {                                                                         \
        if (!(cond)) {                                                           \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #cond); \
            exit(1);                                                             \
        }                                                                        \
    } while (0)

static void serve(double seconds) { CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, false); }

/* Serves the main queue until `*count` reaches `least`, for five seconds at most. */
static void serve_until(const int64_t *count, int64_t least) {
    for (int turn = 0; turn < 500 && *count < least; turn++) serve(0.01);
}

static int64_t runs;
static int64_t drops;
static int64_t off_main;
static int64_t next_drop;
static int64_t drops_out_of_order;

static void count_run(void *box) {
    (void)box;
    if (!pthread_main_np()) off_main++;
    runs++;
}

/* The boxes of a round are the numbers of its posts, from 1. */
static void ordered_drop(void *box) {
    if ((int64_t)(intptr_t)box != ++next_drop) drops_out_of_order++;
    drops++;
}

#if !defined(TZ_TEST_NO_MALLOC_STATS)
static size_t in_use(void) {
    malloc_statistics_t statistics;
    malloc_zone_statistics(NULL, &statistics);
    return statistics.size_in_use;
}
#endif

/* A round of a bench sample whose main thread never serves the queue: posts, deadlines that come and
 * deadlines that never do. */
enum { UNSERVED = 100000 };
static void unserved_round(void) {
    tsuzuri_mainthread_boundary_open();
    next_drop = 0;
    for (int64_t index = 1; index <= UNSERVED; index++) {
        int64_t nanoseconds = index % 16 == 0 ? INT64_MAX : index % 4 == 1 ? 1000000000 : 0;
        tsuzuri_mainthread_post(nanoseconds, count_run, ordered_drop, (void *)(intptr_t)index);
    }
    tsuzuri_mainthread_boundary_done();
}

static void test_unserved(void) {
    runs = drops = drops_out_of_order = 0;
    unserved_round();
    CHECK(runs == 0);
    CHECK(drops == UNSERVED);
    CHECK(drops_out_of_order == 0);
#if !defined(TZ_TEST_NO_MALLOC_STATS)
    size_t first = in_use();
#endif
    unserved_round();
    unserved_round();
    CHECK(runs == 0);
    CHECK(drops == 3 * UNSERVED);
    CHECK(drops_out_of_order == 0);
#if !defined(TZ_TEST_NO_MALLOC_STATS)
    /* A record of its own per post, or a call of the queue per post, would keep megabytes here. */
    size_t later = in_use();
    CHECK(later <= first + 64 * 1024);
#endif
    printf("unserved: ok\n");
}

static char order[64];
static int order_length;

static void letter_run(void *box) {
    if (!pthread_main_np()) off_main++;
    CHECK(order_length < (int)sizeof order - 1);
    order[order_length++] = (char)(intptr_t)box;
    runs++;
}

static void letter_drop(void *box) {
    (void)box;
    drops++;
}

static void letter(int64_t nanoseconds, char name) {
    tsuzuri_mainthread_post(nanoseconds, letter_run, letter_drop, (void *)(intptr_t)name);
}

static void order_reset(void) {
    memset(order, 0, sizeof order);
    order_length = 0;
    runs = drops = off_main = 0;
}

/* Posts in their order, then deadlines in theirs, whatever the order of the calls; nothing runs in place. */
static void test_served(void) {
    tsuzuri_mainthread_boundary_open();
    order_reset();
    letter(60000000, 'F');
    letter(0, 'A');
    letter(20000000, 'D');
    letter(0, 'B');
    letter(40000000, 'E');
    letter(-5, 'C');
    CHECK(runs == 0);
    serve_until(&runs, 6);
    CHECK(strcmp(order, "ABCDEF") == 0);
    CHECK(drops == 0 && off_main == 0);
    printf("served: ok\n");
}

static int64_t reposts;
static void repost_run(void *box) {
    letter_run(box);
    if (++reposts < 3) tsuzuri_mainthread_post(0, repost_run, letter_drop, box);
}

/* A function that posts again runs after the posts that were waiting before it ran. */
static void test_repost(void) {
    order_reset();
    reposts = 0;
    tsuzuri_mainthread_post(0, repost_run, letter_drop, (void *)(intptr_t)'R');
    letter(0, 'X');
    serve_until(&runs, 4);
    CHECK(strcmp(order, "RXRR") == 0);
    printf("repost: ok\n");
}

enum { FROM_THREAD = 2000 };
static int64_t thread_next;
static int64_t thread_out_of_order;
static void numbered_run(void *box) {
    if (!pthread_main_np()) off_main++;
    if ((int64_t)(intptr_t)box != ++thread_next) thread_out_of_order++;
    runs++;
}

static void *poster(void *argument) {
    (void)argument;
    for (int64_t index = 1; index <= FROM_THREAD; index++) {
        tsuzuri_mainthread_post(0, numbered_run, letter_drop, (void *)(intptr_t)index);
    }
    return NULL;
}

/* The posts of another thread run on the main thread, in the order of that thread's posts. */
static void test_other_thread(void) {
    order_reset();
    thread_next = thread_out_of_order = 0;
    pthread_t thread;
    CHECK(pthread_create(&thread, NULL, poster, NULL) == 0);
    serve_until(&runs, FROM_THREAD);
    CHECK(pthread_join(thread, NULL) == 0);
    serve_until(&runs, FROM_THREAD);
    CHECK(runs == FROM_THREAD && thread_out_of_order == 0 && off_main == 0 && drops == 0);
    printf("other thread: ok\n");
}

/* The drop of a capture may run a run loop (a window that closes). */
static void serving_drop(void *box) {
    (void)box;
    serve(0.05);
    drops++;
}

/* What still waits when the boundary ends is not called, even when a drop serves the main queue while
 * the others wait; what is posted after the end is dropped where it is posted. */
static void test_end_of_boundary(void) {
    order_reset();
    tsuzuri_mainthread_post(0, letter_run, serving_drop, (void *)(intptr_t)'P');
    letter(0, 'Q');
    letter(1000000, 'T');
    tsuzuri_mainthread_boundary_done();
    CHECK(runs == 0);
    CHECK(drops == 3);
    letter(0, 'L');
    letter(1000000, 'M');
    CHECK(runs == 0 && drops == 5);
    serve(0.05);
    CHECK(runs == 0 && drops == 5);

    /* The next sample: the calls that the queue may still hold from before serve the new posts. */
    tsuzuri_mainthread_boundary_open();
    order_reset();
    letter(1000000, 'Z');
    letter(0, 'Y');
    serve_until(&runs, 2);
    CHECK(strcmp(order, "YZ") == 0 && drops == 0 && off_main == 0);
    tsuzuri_mainthread_boundary_done();
    CHECK(drops == 0);
    printf("end of boundary: ok\n");
}

int main(void) {
    CHECK(tsuzuri_mainthread_is_current() == 1);
    test_unserved();
    test_served();
    test_repost();
    test_other_thread();
    test_end_of_boundary();
    printf("main_thread_runtime: all passed\n");
    return 0;
}
