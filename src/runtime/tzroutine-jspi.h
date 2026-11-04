/* The WebAssembly half of the tzroutine runtime for --wasm-feature jspi (TZ09): the scheduler of
 * src/runtime/tzroutine.c for one thread whose tzroutines suspend their WebAssembly stacks with
 * JavaScript Promise Integration. tzroutine.c includes it in place of its OS calls and M:N scheduler
 * when TZR_JSPI is defined, and the driver puts it in the same translation unit (as it does with
 * tzroutine-windows.h); the channels, select, the parking lot, the sync types, the timers and the
 * contexts of tzroutine.c run on top of it unchanged.
 *
 * One G runs at a time, in turn with the others (concurrent, not parallel): there is one M and one
 * P, and the run queue is the JavaScript glue's (src/runtime/bindings-core.mjs). A G that waits
 * calls tzr_jspi_suspend (src/runtime/tzroutine-jspi.ll), which suspends its WebAssembly stack
 * through the Suspending import `tsuzuri_tz.suspend`; the glue resumes the G that it takes from its
 * queue next, or starts a new one by calling the export `tsuzuri_tz_entry` through
 * WebAssembly.promising. A G's frames in linear memory are on a stack of its own (64 KiB from the
 * module's allocator, D3), whose bounds the stack checks of the generated code compare against; an
 * export call is the boundary G and runs on the module's stack (bounds 0, 0).
 *
 * The imports of `tsuzuri_tz` (TZ09 spec 3): suspend (in tzroutine-jspi.ll), spawn and ready (a G
 * can run: the glue queues it), clock (the monotonic nanoseconds of performance.now) and timer (the
 * earliest deadline of the runtime's timers changed; -1 when none is left), on which the glue sets a
 * setTimeout that calls the export `tsuzuri_tz_fire`. The glue finds a deadlock (no G can run, no
 * timer and no host wait is left, and a G waits) and rejects the export call with a RuntimeError:
 * a G's park never returns the deadlock status here. WebAssembly has no standard error, so the
 * runtime writes no report: a failure traps at its call site (`RuntimeError: unreachable`). */
#ifndef TZR_JSPI_H
#define TZR_JSPI_H
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>

#if !defined(__wasm32__)
#error "tzroutine-jspi.h: the JSPI scheduler is for wasm32"
#endif

/* Freestanding: -mbulk-memory makes these memory.copy and memory.fill. */
#define memcpy __builtin_memcpy
#define memset __builtin_memset
#define memmove __builtin_memmove

#ifndef TZR_API
#define TZR_API __attribute__((weak, visibility("hidden")))
#endif
#define TZR_NOINLINE __attribute__((noinline))
#define TZR_JSPI_IMPORT(name) __attribute__((import_module("tsuzuri_tz"), import_name(name)))
/* A G's stack in linear memory (D3). */
#define TZR_JSPI_STACK (64u * 1024u)
/* Finished Gs kept with their stacks for the next spawn (a stack is 64 KiB of a 16 MiB memory). */
#define TZR_JSPI_CACHE 16

/* ---------------------------------------------------------------- failure, memory, time */

static void tzr_fatal(const char *message) {
    (void)message;
    __builtin_trap();
}

/* The module's allocator (`tsuzuri_alloc` of src/llvm_abi.rs, which a module that spawns keeps).
 * Each block of the runtime carries its size in a 16-byte header, so that realloc can copy it. */
void *tsuzuri_alloc(int64_t size);
void tsuzuri_free(void *pointer);

static void *tzr_jspi_malloc(size_t size) {
    unsigned char *block = tsuzuri_alloc((int64_t)size + 16);
    if (block == NULL) return NULL;
    *(size_t *)block = size;
    return block + 16;
}
static void tzr_jspi_free(void *pointer) {
    if (pointer != NULL) tsuzuri_free((unsigned char *)pointer - 16);
}
static void *tzr_jspi_calloc(size_t count, size_t size) {
    if (size != 0 && count > SIZE_MAX / size - 1) return NULL;
    void *pointer = tzr_jspi_malloc(count * size);
    if (pointer != NULL) memset(pointer, 0, count * size);
    return pointer;
}
static void *tzr_jspi_realloc(void *pointer, size_t size) {
    void *grown = tzr_jspi_malloc(size);
    if (grown == NULL || pointer == NULL) return grown;
    size_t old = *(size_t *)((unsigned char *)pointer - 16);
    memcpy(grown, pointer, old < size ? old : size);
    tzr_jspi_free(pointer);
    return grown;
}
#define malloc tzr_jspi_malloc
#define calloc tzr_jspi_calloc
#define realloc tzr_jspi_realloc
#define free tzr_jspi_free

TZR_JSPI_IMPORT("clock") int64_t tzr_jspi_clock(void);
static int64_t tzr_nanotime(void) { return tzr_jspi_clock(); }
static inline long tzr_online_cpus(void) { return 1; }

/* ---------------------------------------------------------------- locks */

/* One thread, and a G leaves it only where it waits (tzr_park, tsuzuri_tz_yield), never inside a
 * critical section of the runtime: the locks do nothing. tzr_lock keeps its 8 bytes, a word of the
 * channel blocks that the generated code lays out. */
typedef struct { uint32_t word; uint32_t pad; } tzr_lock;
static inline void tzr_lock_acquire(tzr_lock *l) { (void)l; }
static inline void tzr_lock_release(tzr_lock *l) { (void)l; }
typedef int tzr_mutex;
typedef int tzr_cond;
#define TZR_MUTEX_INIT 0
#define TZR_COND_INIT 0
static inline void tzr_mutex_lock(tzr_mutex *m) { (void)m; }
static inline void tzr_mutex_unlock(tzr_mutex *m) { (void)m; }

/* No trap boundary on WebAssembly: Parallel's chunks (task-wasm.ll there) never ask. */
static struct {
    void *(*owner)(void);
} tsuzuri_trap_hooks;

/* ---------------------------------------------------------------- structures */

enum { TZR_G_IDLE, TZR_G_RUNNABLE, TZR_G_RUNNING, TZR_G_WAITING, TZR_G_DEAD };
enum { TZR_KIND_ROUTINE, TZR_KIND_THREAD };

typedef struct tzr_g tzr_g;
typedef struct tzr_sudog tzr_sudog;
typedef struct tzr_chan tzr_chan;
typedef struct tzr_timer tzr_timer;
typedef struct tzr_netwait tzr_netwait;

/* As tzroutine.c's. */
typedef struct tzr_scope {
    tzr_lock lock;
    int64_t live;
    tzr_g *waiter;
    struct tzr_scope *outer;
    struct tzr_scope *boundary;
    tzr_timer *timers;
} tzr_scope;

struct tzr_g {
    uint32_t stack_base; /* the bounds that tzroutine-jspi.ll gives the stack checks (0, 0: the module's stack) */
    uint32_t stack_top;  /* where the G's stack pointer starts */
    void (*code)(void *);
    void *env;
    unsigned char *stack; /* from tsuzuri_alloc; NULL for the boundary G */
    _Atomic int status;
    int kind;
    tzr_g *link;              /* the list of finished Gs */
    tzr_scope *scope;         /* the scope this G is counted in (NULL for the boundary G) */
    tzr_scope *spawn_scope;   /* where tsuzuri_tz_go counts children (NULL: scope, or the root) */
    uint64_t id;
    const char *wait_reason;
    uint64_t rand;
    tzr_sudog *param;
    _Atomic uint32_t select_done;
    uintptr_t *held;
    uint32_t held_count, held_capacity;
};
_Static_assert(offsetof(struct tzr_g, stack_base) == 0 && offsetof(struct tzr_g, stack_top) == 4,
               "tzroutine-jspi.ll reads a G's bounds at offsets 0 and 4");

static struct {
    tzr_mutex lock;
    _Atomic int64_t ngoroutines; /* spawned tzroutines that have not finished */
    _Atomic int64_t ntimers;     /* timers that will still fire */
    _Atomic int64_t nnetwait;    /* none: a socket wait of WebAssembly is the glue's */
    int nprocs;
    uint64_t next_id;
    tzr_g *cur;    /* the G on the stack */
    tzr_g *gfree;  /* finished Gs with their stacks, at most TZR_JSPI_CACHE */
    int gfree_count;
    tzr_g *zombie; /* the G that finished last, when it was not cached: its stack goes at the next exit */
} tzr_sched = {.nprocs = 1};

/* The boundary G: every export call runs on the module's stack as this G. */
static tzr_g tzr_jspi_main = {.kind = TZR_KIND_THREAD, .status = TZR_G_RUNNING, .rand = 0x2545F4914F6CDD1DULL};

static uint64_t tzr_rand(uint64_t *state) {
    /* xorshift64*; the state is never zero */
    uint64_t x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static void tzr_start(void) {}

static tzr_g *tzr_current(void) { return tzr_sched.cur != NULL ? tzr_sched.cur : &tzr_jspi_main; }

/* The deadlock detector is the glue's. tzroutine.c asks it to look again when the last timer is
 * gone: the glue learns that no timer is left. */
static void tzr_jspi_timers_report(void);
static void tzr_checkdead(void) { tzr_jspi_timers_report(); }

/* ---------------------------------------------------------------- park and ready */

/* tzroutine-jspi.ll: suspends the calling G until the glue resumes it, keeping its stack pointer and
 * the bounds of its stack checks across the suspension, on which the scheduler's stack is the
 * module's stack pointer. */
void tzr_jspi_suspend(tzr_g *g);
TZR_JSPI_IMPORT("spawn") void tzr_jspi_spawned(tzr_g *g);
TZR_JSPI_IMPORT("ready") void tzr_jspi_ready(tzr_g *g);

/* Parks the current G until tzr_ready. Nothing else runs between the unlock and the suspension, so
 * the unlock may come first (R1 holds: no lock is held across the switch). The glue ends a deadlock
 * by rejecting the export call: a park returns only when the G was woken. */
static int tzr_park(void (*unlock)(void *), void *arg, const char *reason) {
    tzr_g *g = tzr_current();
    g->wait_reason = reason;
    atomic_store_explicit(&g->status, TZR_G_WAITING, memory_order_relaxed);
    if (unlock) unlock(arg);
    tzr_jspi_suspend(g);
    tzr_sched.cur = g;
    atomic_store_explicit(&g->status, TZR_G_RUNNING, memory_order_relaxed);
    return 0;
}

static void tzr_ready(tzr_g *g) {
    if (atomic_load_explicit(&g->status, memory_order_relaxed) != TZR_G_WAITING) tzr_fatal("bad tzroutine status in ready");
    atomic_store_explicit(&g->status, TZR_G_RUNNABLE, memory_order_relaxed);
    tzr_jspi_ready(g);
}

static void tzr_ready_unlock(tzr_g *g, tzr_lock *lock) {
    tzr_lock_release(lock);
    tzr_ready(g);
}

/* Routine.yield: the G goes behind the Gs that can run. */
TZR_API void tsuzuri_tz_yield(void) {
    tzr_g *g = tzr_current();
    atomic_store_explicit(&g->status, TZR_G_RUNNABLE, memory_order_relaxed);
    tzr_jspi_ready(g);
    tzr_jspi_suspend(g);
    tzr_sched.cur = g;
    atomic_store_explicit(&g->status, TZR_G_RUNNING, memory_order_relaxed);
}

/* ---------------------------------------------------------------- spawn and exit */

static tzr_scope tzr_root_scope = {.boundary = &tzr_root_scope};

/* The scope that counts what `g` spawns, as tzroutine.c's: its spawn scope, its scope, or the root. */
static tzr_scope *tzr_spawn_scope(tzr_g *g) {
    tzr_scope *scope = g->spawn_scope ? g->spawn_scope : g->scope;
    return scope ? scope : &tzr_root_scope;
}

static void tzr_spawn_reserve(tzr_scope *scope) {
    scope->live++;
    atomic_fetch_add(&tzr_sched.ngoroutines, 1);
}

static void tzr_scope_hold(tzr_scope *scope) { scope->live++; }

static void tzr_scope_release(tzr_scope *scope) {
    if (--scope->live == 0 && scope->waiter != NULL) {
        tzr_g *waiter = scope->waiter;
        scope->waiter = NULL;
        tzr_ready(waiter);
    }
}

/* A G with its stack: a finished one, or a new one. A stack is never given back while the G that
 * finished on it is still on it (tzr_jspi_exit), so it is freed later, from another stack. */
static tzr_g *tzr_galloc(void) {
    tzr_g *g = tzr_sched.gfree;
    if (g != NULL) {
        tzr_sched.gfree = g->link;
        tzr_sched.gfree_count--;
        return g;
    }
    g = calloc(1, sizeof *g);
    if (g == NULL) return NULL;
    g->stack = tsuzuri_alloc(TZR_JSPI_STACK);
    if (g->stack == NULL) {
        free(g);
        return NULL;
    }
    uintptr_t base = (uintptr_t)g->stack;
    g->stack_base = (uint32_t)base;
    g->stack_top = (uint32_t)((base + TZR_JSPI_STACK) & ~(uintptr_t)15);
    g->kind = TZR_KIND_ROUTINE;
    g->rand = 0x9E3779B97F4A7C15ULL ^ (uint64_t)base;
    return g;
}

static void tzr_gdestroy(tzr_g *g) {
    tsuzuri_free(g->stack);
    free(g->held);
    free(g);
}

/* Starts `code(env)` on a new G counted in `scope` (already counted when `reserved`): the glue
 * queues it and calls tsuzuri_tz_entry when its turn comes. */
static void tzr_spawn(tzr_scope *scope, void (*code)(void *), void *env, int reserved) {
    tzr_g *g = tzr_galloc();
    if (g == NULL) tzr_fatal("cannot allocate a tzroutine stack");
    g->code = code;
    g->env = env;
    g->scope = scope;
    g->spawn_scope = NULL;
    g->id = ++tzr_sched.next_id;
    g->wait_reason = NULL;
    if (!reserved) tzr_spawn_reserve(scope);
    atomic_store_explicit(&g->status, TZR_G_RUNNABLE, memory_order_relaxed);
    tzr_jspi_spawned(g);
}

TZR_API void tsuzuri_tz_go(void (*code)(void *), void *env) { tzr_spawn(tzr_spawn_scope(tzr_current()), code, env, 0); }

/* The end of a G, on its own stack (tzroutine.c's tzr_after_exit runs on g0, which WebAssembly has
 * not): the G is cached or left as the zombie, whose stack the next exit frees, and its scope
 * counts it off. Nothing runs on this stack after tsuzuri_tz_entry returns. */
static void tzr_jspi_exit(tzr_g *g) {
    atomic_store_explicit(&g->status, TZR_G_DEAD, memory_order_relaxed);
    tzr_scope *scope = g->scope;
    g->scope = NULL;
    g->spawn_scope = NULL;
    g->code = NULL;
    g->env = NULL;
    if (tzr_sched.zombie != NULL) {
        tzr_gdestroy(tzr_sched.zombie);
        tzr_sched.zombie = NULL;
    }
    if (tzr_sched.gfree_count < TZR_JSPI_CACHE) {
        g->link = tzr_sched.gfree;
        tzr_sched.gfree = g;
        tzr_sched.gfree_count++;
    } else {
        tzr_sched.zombie = g;
    }
    atomic_fetch_sub(&tzr_sched.ngoroutines, 1);
    tzr_sched.cur = &tzr_jspi_main;
    tzr_scope_release(scope);
}

/* tsuzuri_tz_entry (tzroutine-jspi.ll) calls it on the new G's stack. */
TZR_API void tzr_jspi_run(tzr_g *g) {
    tzr_sched.cur = g;
    atomic_store_explicit(&g->status, TZR_G_RUNNING, memory_order_relaxed);
    g->code(g->env);
    tzr_jspi_exit(g);
}

/* ---------------------------------------------------------------- the calls of other parts */

/* One thread: a blocking call holds no P, a callback needs no M, and every G is on the thread. */
TZR_API void tsuzuri_tz_blocking_enter(void) {}
TZR_API void tsuzuri_tz_blocking_exit(void) {}
TZR_API int32_t tsuzuri_tz_callback_enter(void) { return 0; }
TZR_API void tsuzuri_tz_callback_exit(int32_t depth) { (void)depth; }
TZR_API void tsuzuri_tz_lock_thread(void) {}
TZR_API void tsuzuri_tz_unlock_thread(void) {}

TZR_API void *tsuzuri_tz_scope_current(void) { return tzr_current()->spawn_scope; }
TZR_API void tsuzuri_tz_scope_reset(void *scope) { tzr_current()->spawn_scope = scope; }

/* No standard error: the trap at the call site is the report (TZ09 spec 6). */
TZR_API void tsuzuri_tz_report(int32_t what) { (void)what; }

/* Routine.count counts the boundary G as one; there is one P and one thread. */
TZR_API int64_t tsuzuri_tz_count(void) { return atomic_load(&tzr_sched.ngoroutines) + 1; }
TZR_API int64_t tsuzuri_tz_max_procs(void) { return 1; }
TZR_API int64_t tsuzuri_tz_cpu_count(void) { return 1; }

#endif /* TZR_JSPI_H */
