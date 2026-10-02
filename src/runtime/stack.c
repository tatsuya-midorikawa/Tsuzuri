/* Stack overflow report for native executables (E14 Phase 3). Linked into the executables Tsuzuri
   builds (build --emit exe, run), never into objects: a host that links an object keeps its own
   signal settings. A stack overflow is reported the way a trap is, then ends the process by
   abort(), as other language runtimes do. A fault that is not an overflow keeps its default
   action. */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#if defined(__unix__) || defined(__APPLE__)
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#if defined(__linux__)
#include <sys/auxv.h>
#include <sys/resource.h>
#endif

/* The memory a fault must hit to count as an overflow: the guard page of a stack and the pages
   around it, so a frame that skips part of the guard is still recognised. */
enum { TZ_STACK_THREADS = 64, TZ_STACK_REACH = 128 * 1024, TZ_STACK_ALTSTACK = 64 * 1024 };

static struct {
    _Atomic uintptr_t start;
    _Atomic uintptr_t end;
} tz_stack_windows[TZ_STACK_THREADS];
static atomic_uint tz_stack_count;

/* The report is written before anything can end the process, by every thread that overflows, so
   it is never lost to a second thread that overflows at the same time. */
static void tz_stack_fault(int signal, siginfo_t *info, void *context) {
    (void)context;
    uintptr_t address = (uintptr_t)info->si_addr;
    unsigned count = atomic_load_explicit(&tz_stack_count, memory_order_acquire);
    if (count > TZ_STACK_THREADS) count = TZ_STACK_THREADS;
    for (unsigned slot = 0; slot < count; ++slot) {
        uintptr_t end = atomic_load_explicit(&tz_stack_windows[slot].end, memory_order_acquire);
        uintptr_t start = atomic_load_explicit(&tz_stack_windows[slot].start, memory_order_relaxed);
        if (end != 0 && address >= start && address < end) {
            static const char message[] = "trap: stack overflow\n";
            (void)!write(2, message, sizeof message - 1);
            abort();
        }
    }
    /* Not an overflow: deliver the default action on the thread that received the signal. */
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_handler = SIG_DFL;
    sigemptyset(&action.sa_mask);
    if (sigaction(signal, &action, NULL) != 0 || raise(signal) != 0) _exit(128 + signal);
}

static void tz_stack_register(uintptr_t low) {
    unsigned slot = atomic_fetch_add_explicit(&tz_stack_count, 1, memory_order_acq_rel);
    if (slot >= TZ_STACK_THREADS) return;
    atomic_store_explicit(&tz_stack_windows[slot].start, low > TZ_STACK_REACH ? low - TZ_STACK_REACH : 0, memory_order_relaxed);
    atomic_store_explicit(&tz_stack_windows[slot].end, low + TZ_STACK_REACH / 2, memory_order_release);
}

#if defined(__linux__)
/* The main thread's stack grows on demand, down to RLIMIT_STACK below the end of its mapping, which
   is the end of the executable's file name. pthread_getattr_np cannot be used for it: musl reports
   only what is mapped right now, glibc reads /proc. A fault beyond the limit hits just below it. */
static uintptr_t tz_stack_main_low(void) {
    struct rlimit limit;
    if (getrlimit(RLIMIT_STACK, &limit) != 0 || limit.rlim_cur == RLIM_INFINITY || limit.rlim_cur == 0) return 0;
    const char *name = (const char *)getauxval(AT_EXECFN);
    uintptr_t end = name ? (uintptr_t)name + strlen(name) + 1 : (uintptr_t)__builtin_frame_address(0);
    uintptr_t page = (uintptr_t)sysconf(_SC_PAGESIZE);
    uintptr_t top = (end + page - 1) & ~(page - 1);
    return top > (uintptr_t)limit.rlim_cur ? top - (uintptr_t)limit.rlim_cur : 0;
}
#endif

/* The lowest usable address of the calling thread's stack, or 0 if it is unknown. */
static uintptr_t tz_stack_low(int main_thread) {
    pthread_t self = pthread_self();
#if defined(__APPLE__)
    (void)main_thread;
    uintptr_t high = (uintptr_t)pthread_get_stackaddr_np(self);
    size_t size = pthread_get_stacksize_np(self);
    return size != 0 && size <= high ? high - size : 0;
#elif defined(__linux__)
    if (main_thread) return tz_stack_main_low();
    pthread_attr_t attributes;
    void *address = NULL;
    size_t size = 0;
    if (pthread_getattr_np(self, &attributes) != 0) return 0;
    int status = pthread_attr_getstack(&attributes, &address, &size);
    pthread_attr_destroy(&attributes);
    return status == 0 && size != 0 ? (uintptr_t)address : 0;
#else
    (void)self;
    (void)main_thread;
    return 0;
#endif
}

/* Gives the calling thread a stack the handler can run on, and registers its guard.
   Returns 1 if the thread is covered. */
static int tz_stack_prepare(int main_thread) {
    uintptr_t low = tz_stack_low(main_thread);
    if (low == 0) return 0;
    void *memory = malloc(TZ_STACK_ALTSTACK);
    if (!memory) return 0;
    stack_t alternate = { .ss_sp = memory, .ss_size = TZ_STACK_ALTSTACK, .ss_flags = 0 };
    if (sigaltstack(&alternate, NULL) != 0) {
        free(memory);
        return 0;
    }
    tz_stack_register(low);
    return 1;
}

/* Called first by every thread the task runtime starts. */
__attribute__((visibility("hidden")))
void tsuzuri_stack_thread(void) {
    (void)tz_stack_prepare(0);
}

__attribute__((constructor))
static void tz_stack_install(void) {
    if (!tz_stack_prepare(1)) return;
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_sigaction = tz_stack_fault;
    action.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGSEGV, &action, NULL) != 0) return;
    (void)sigaction(SIGBUS, &action, NULL);
}
#endif
