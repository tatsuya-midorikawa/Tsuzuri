# TZ01 付録: 実測・参照実装・詳細設計

[TZ01](TZ01-tzroutine-runtime.md) の付録。B の参照実装は、この付録を書いた時点（2026-11-01、HEAD `9a76d8ac`）に、リポジトリの外の作業木で次の A のとおり検査した。**B-1〜B-3 はそのまま置く。** 直す必要を見つけたら、TZ01 の停止条件 S3 に従う。

## A. 実測

### A-1 環境と方法

- Apple M3（8 コア）、macOS 27.2、Apple clang 21.0.0、Node.js 24.22.2。x86-64 は同じ機械の Rosetta 2（`clang -arch x86_64`）。
- B-3 のドライバー（`TSUZURI_TSAN=1 TSUZURI_ASAN=1 node tests/tzroutine_runtime.mjs`）: `-O0`・`-O3` × `TSUZURI_MAXPROCS` 1・2・4 × 14 ケース、`stack.c` と連結した `-O2` の 14 ケース、x86-64 の `-O2` の 14 ケース、TSan（`-O1 -g -fsanitize=thread`）と ASan + UBSan（`-fsanitize=address,undefined`、`detect_stack_use_after_return=1`）の各 12 ケース。結果 `tzroutine runtime: 136 runs passed`、約 59 秒。
- 負荷: 試作の段階で、arm64 と x86-64 の全ケースを `TSUZURI_MAXPROCS` 1〜8 で交互に 40 巡（80 回）。失敗・停止なし。
- 連結の確認: `os.c`・`net.c`・`async.c`・`trap.c`・`task.c`・`tzroutine.c` をこの順に 1 つのファイルへ連結し、`clang -std=c11 -pthread -DTZ_TRAP_BOUNDARY -Wall -Wextra -c` が警告なしで通る（ドライバーの翻訳単位に足しても名前が衝突しない。内部の名前はすべて `tzr_`・`TZR_` で始まる）。

### A-2 結果（`-O2`、参照実装の `tests/tzroutine_runtime.c` が出す数）

| ケース | arm64、P 8 | arm64、P 2 | arm64、P 1 | x86-64（Rosetta）、P 8 |
| --- | --- | --- | --- | --- |
| `spawn`（10 万個の生成と待ち） | 0.047 秒 | 0.093 秒 | 0.209 秒 | 0.051 秒 |
| `pingpong`（G 同士、往復 1 回） | 303 ns | 231 ns | 120 ns | 292 ns |
| `pingpong`（擬似 G と G） | 約 5.2 µs | — | — | — |
| `sleep`（2,000 個 × 20 ms） | 0.027 秒 | 0.033 秒 | 0.033 秒 | 0.032 秒 |
| `select`（2 つの腕が選ばれた回数、2 万回） | 9,963 / 10,037 | 10,013 / 9,987 | 9,952 / 10,048 | 9,996 / 10,004 |

- P 1 の往復が速いのは、受け渡しが同じ P の `runnext` で完結し、M の起床がないため。P 8 では、起こした G を spinning の M が盗むことがある。
- 擬似 G との往復は、`main` の OS スレッドの起床（条件変数）を含むので µs 単位になる。`main` が大量の受け渡しをするプログラムは、受け渡しを tzroutine の間で行うほうが速い（TZ02 の文書に書く）。
- 数値は試作の参照値で、性能の主張や CI の閾値に使わない（AGENTS.md）。

### A-3 試作で見つけた問題と、参照実装の対処

| 問題 | 原因 | 対処（参照実装に入っている） |
| --- | --- | --- |
| TSan が `tzr_execute` で `g->sp` のデータ競合を報告した | `__tsan_switch_to_fiber` に `no_sync` を渡すと、切り替えが順序を作らない扱いになり、M のスレッドが知っていることが G の側に伝わらない | フラグ 0（切り替えが順序を作る。同じ OS スレッドなので事実どおり）。スレッドをまたぐ受け渡しは run queue とロックの同期で保たれ、TSan はそちらを検査する |
| TSan が `tsuzuri_tz_sleep` でデータ競合を報告した | タイマーのヒープを pthread の mutex で守り、`tzr_park` が g0（TSan から見ると別のスレッド）でその mutex を解放した | 規則 R4: `tzr_park` が解放するのは `tzr_lock` だけ。タイマーのヒープを `tzr_lock` にし、タイマーのスレッドが眠る mutex と条件変数を分けた（`kicked` で起こし損ねを防ぐ） |
| `deep` の試験の再帰が意図より浅かった | `volatile char pad[512]` の 1 要素しか触らないと、clang が frame を 32 バイトに縮めた | 添字を実行時の値にし（`pad[n % 512]`）、1 段の大きさを実行時に測って、目標のバイト数から深さを決める（`depth_for`） |
| `fork` した子でチャンネルの待ちが終わらない | 子プロセスにはスケジューラーのスレッドがない | 子は `fork` + `execl` で自分を起こし直す（`run_child`） |
| `task.c` と同じ翻訳単位で `struct tz_waiter` が衝突した | ドライバーはランタイムの C を 1 つのファイルに連結する | 内部の名前を `tzr_`・`TZR_` にした。生成コードが呼ぶ名前だけ `tsuzuri_tz_` |
| ガード付きのビルドで溢れの子が SIGABRT で終わる | `stack.c` が報告してから `abort()` する（期待どおり） | 子の判定は SIGSEGV・SIGBUS・SIGABRT のどれでもよい（`overflowed`） |

## B. 参照実装

### B-1 `src/runtime/tzroutine.c`（そのまま置く。`-std=c11 -Wall -Wextra -Werror` で警告なし）

C の公開関数は TZ01 の仕様 1.2。内部の名前はすべて `tzr_`・`TZR_` で始まる。`TZ_STACK_GUARD` はドライバーが `stack.c` を連結するときに付ける（TZ02）。

```c
/* The tzroutine runtime (TZ01, D-61): M:N scheduler, stacks, context switch, parking lot, timers,
 * channels and select. POSIX (macOS, Linux) on aarch64 and x86-64; Windows is TZ01 Phase 2. The
 * generated code calls the tsuzuri_tz_ functions (TZ02 and later); everything else is internal.
 *
 * Names: G = one tzroutine (or the pseudo-G of a thread that is not a scheduler worker), M = an OS
 * thread of the scheduler, P = the right to run tzroutines (there are `nprocs` of them).
 *
 * Rules that keep it correct (each one is checked by tests/tzroutine_runtime.c):
 *  R1. A G never holds a tzr_lock across a context switch, except the locks that tzr_park releases
 *      on g0 after the switch (the waiter is fully off its stack before anyone can wake it).
 *  R2. Thread-local state is read only through tzr_this_m(), which is noinline: code after a switch
 *      may run on another thread, so no TLS address may be kept across tzr_park or tzr_yield.
 *  R3. Lock order: tzr_sched.lock is innermost. Channel locks (sorted by address in select), parking-lot
 *      bucket locks and tzr_timers.lock may be held while tzr_sched.lock is taken (tzr_ready), never after.
 *  R4. Only a tzr_lock may be released by tzr_park's unlock callback (never a pthread mutex).
 */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <os/lock.h>
#endif
#if defined(__linux__)
#include <linux/futex.h>
#include <sys/syscall.h>
#endif

#if defined(__has_feature)
#if __has_feature(thread_sanitizer)
#define TZR_TSAN 1
#endif
#endif
#if defined(TZR_TSAN)
#include <sanitizer/tsan_interface.h>
/* TSan must be told about every stack switch. Flag 0 makes the switch order memory, which is what
 * it does: the same OS thread runs both sides. Cross-thread hand-offs still need the run-queue
 * and lock synchronization, and TSan checks those. */
#define TZR_TSAN_SWITCH(fiber) __tsan_switch_to_fiber((fiber), 0)
#else
#define TZR_TSAN_SWITCH(fiber) ((void)0)
#endif

#if !defined(__aarch64__) && !defined(__x86_64__)
#error "tzroutine: unsupported architecture"
#endif

#define TZR_API __attribute__((weak, visibility("hidden")))
#define TZR_NOINLINE __attribute__((noinline))
#define TZR_MAXPROCS 256
#define TZR_RUNQ 256
#define TZR_GUARD (128u * 1024u)
#define TZR_DEFAULT_STACK (1024u * 1024u)
#define TZR_STACK_CACHE 1024
#define TZR_P_GFREE 64

/* ---------------------------------------------------------------- failure */

static void tzr_fatal(const char *message) {
    fprintf(stderr, "Tsuzuri runtime: %s\n", message);
    fflush(stderr);
    abort();
}

#if defined(TZ_STACK_GUARD)
/* src/runtime/stack.c: the alternate signal stack of a thread, and the hook its handler asks first. */
void tsuzuri_stack_thread(void);
extern int32_t (*volatile tsuzuri_stack_guard_hook)(uintptr_t);
#endif
TZR_API int32_t tsuzuri_tz_stack_guard_hit(uintptr_t address);

/* ---------------------------------------------------------------- locks */

/* tzr_lock: 8 bytes, all-zero is unlocked, unlocked by the thread that locked it (R1). It is the only
 * kind of lock that tzr_park may release on g0 (R4). Under TSan it is a plain atomic spin lock, so
 * that TSan, which treats each stack as its own thread, sees acquire/release instead of a mutex
 * that one "thread" locks and another unlocks. */
#if defined(TZR_TSAN)
typedef struct { _Atomic uint32_t word; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) {
    uint32_t c = 0;
    while (!atomic_compare_exchange_weak_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) {
        c = 0;
        sched_yield();
    }
}
static void tzr_lock_release(tzr_lock *l) { atomic_store_explicit(&l->word, 0, memory_order_release); }
#elif defined(__APPLE__)
typedef struct { os_unfair_lock lock; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) { os_unfair_lock_lock(&l->lock); }
static void tzr_lock_release(tzr_lock *l) { os_unfair_lock_unlock(&l->lock); }
#else
typedef struct { _Atomic uint32_t word; uint32_t pad; } tzr_lock;
static void tzr_lock_acquire(tzr_lock *l) {
    uint32_t c = 0;
    if (atomic_compare_exchange_strong_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) return;
    for (int spin = 0; spin < 64; spin++) {
        c = 0;
        if (atomic_load_explicit(&l->word, memory_order_relaxed) == 0
            && atomic_compare_exchange_weak_explicit(&l->word, &c, 1, memory_order_acquire, memory_order_relaxed)) return;
#if defined(__x86_64__)
        __builtin_ia32_pause();
#else
        __asm__ volatile("yield");
#endif
    }
    while (atomic_exchange_explicit(&l->word, 2, memory_order_acquire) != 0)
        syscall(SYS_futex, &l->word, FUTEX_WAIT_PRIVATE, 2, NULL, NULL, 0);
}
static void tzr_lock_release(tzr_lock *l) {
    if (atomic_exchange_explicit(&l->word, 0, memory_order_release) == 2)
        syscall(SYS_futex, &l->word, FUTEX_WAKE_PRIVATE, 1, NULL, NULL, 0);
}
#endif

/* tzr_note: a one-shot wakeup for an OS thread (an idle M, a parked pseudo-G). */
typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    int set;
} tzr_note;

static void tzr_note_init(tzr_note *n) {
    pthread_mutex_init(&n->mutex, NULL);
    pthread_cond_init(&n->cond, NULL);
    n->set = 0;
}
static void tzr_note_wake(tzr_note *n) {
    pthread_mutex_lock(&n->mutex);
    n->set = 1;
    pthread_cond_signal(&n->cond);
    pthread_mutex_unlock(&n->mutex);
}
static void tzr_note_sleep(tzr_note *n) {
    pthread_mutex_lock(&n->mutex);
    while (!n->set) pthread_cond_wait(&n->cond, &n->mutex);
    n->set = 0;
    pthread_mutex_unlock(&n->mutex);
}

static int64_t tzr_nanotime(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec;
}

/* ---------------------------------------------------------------- context switch */

/* void tzr_switch(void **save_sp, void *load_sp): pushes the callee-saved registers, stores sp in
 * *save_sp, loads load_sp, pops the registers saved there and returns into that context. */
#if defined(__aarch64__)
#if defined(__APPLE__)
#define TZR_SYM(name) "_" #name
#else
#define TZR_SYM(name) #name
#endif
__attribute__((naked, noinline)) static void tzr_switch(void **save_sp, void *load_sp) {
    __asm__ volatile(
        "sub sp, sp, #160\n"
        "stp d8, d9, [sp, #0]\n"
        "stp d10, d11, [sp, #16]\n"
        "stp d12, d13, [sp, #32]\n"
        "stp d14, d15, [sp, #48]\n"
        "stp x19, x20, [sp, #64]\n"
        "stp x21, x22, [sp, #80]\n"
        "stp x23, x24, [sp, #96]\n"
        "stp x25, x26, [sp, #112]\n"
        "stp x27, x28, [sp, #128]\n"
        "stp x29, x30, [sp, #144]\n"
        "mov x9, sp\n"
        "str x9, [x0]\n"
        "mov sp, x1\n"
        "ldp d8, d9, [sp, #0]\n"
        "ldp d10, d11, [sp, #16]\n"
        "ldp d12, d13, [sp, #32]\n"
        "ldp d14, d15, [sp, #48]\n"
        "ldp x19, x20, [sp, #64]\n"
        "ldp x21, x22, [sp, #80]\n"
        "ldp x23, x24, [sp, #96]\n"
        "ldp x25, x26, [sp, #112]\n"
        "ldp x27, x28, [sp, #128]\n"
        "ldp x29, x30, [sp, #144]\n"
        "add sp, sp, #160\n"
        "ret\n");
}
#define TZR_FRAME_BYTES 160
static void *tzr_initial_frame(char *top, void (*start)(void)) {
    uint64_t *frame = (uint64_t *)(top - TZR_FRAME_BYTES);
    memset(frame, 0, TZR_FRAME_BYTES);
    frame[18] = 0;                        /* x29: ends the frame-pointer chain */
    frame[19] = (uint64_t)(uintptr_t)start; /* x30: where the first switch returns */
    return frame;
}
#else /* x86-64 System V */
__attribute__((naked, noinline)) static void tzr_switch(void **save_sp, void *load_sp) {
    __asm__ volatile(
        "pushq %rbp\n"
        "pushq %rbx\n"
        "pushq %r12\n"
        "pushq %r13\n"
        "pushq %r14\n"
        "pushq %r15\n"
        "subq $8, %rsp\n"
        "stmxcsr (%rsp)\n"
        "fnstcw 4(%rsp)\n"
        "movq %rsp, (%rdi)\n"
        "movq %rsi, %rsp\n"
        "ldmxcsr (%rsp)\n"
        "fldcw 4(%rsp)\n"
        "addq $8, %rsp\n"
        "popq %r15\n"
        "popq %r14\n"
        "popq %r13\n"
        "popq %r12\n"
        "popq %rbx\n"
        "popq %rbp\n"
        "ret\n");
}
static void *tzr_initial_frame(char *top, void (*start)(void)) {
    /* After `ret` the start function sees rsp = top - 8, as if it had been called. */
    uint64_t *slot = (uint64_t *)(top - 16);
    slot[1] = 0;                              /* fake return address of `start` */
    slot[0] = (uint64_t)(uintptr_t)start;     /* popped by ret */
    uint64_t *frame = slot - 7;
    memset(frame, 0, 7 * sizeof(uint64_t));   /* r15 r14 r13 r12 rbx rbp = 0 */
    uint32_t control[2] = {0x1F80u, 0x037Fu}; /* default MXCSR and x87 control word */
    memcpy(frame, control, sizeof control);
    return frame;
}
#endif

/* ---------------------------------------------------------------- structures */

enum { TZR_G_IDLE, TZR_G_RUNNABLE, TZR_G_RUNNING, TZR_G_WAITING, TZR_G_DEAD };
enum { TZR_KIND_ROUTINE, TZR_KIND_THREAD };

typedef struct tzr_g tzr_g;
typedef struct tzr_m tzr_m;
typedef struct tzr_p tzr_p;
typedef struct tzr_sudog tzr_sudog;
typedef struct tzr_chan tzr_chan;

typedef struct tzr_scope {
    tzr_lock lock;
    int64_t live;  /* tzroutines spawned in this scope that have not finished */
    tzr_g *waiter;  /* the G waiting in tsuzuri_tz_scope_leave */
    struct tzr_scope *outer; /* the thread's scope before tsuzuri_tz_scope_enter */
} tzr_scope;

struct tzr_g {
    void *sp;
    void (*code)(void *);
    void *env;
    char *stack;       /* mapping: guard + usable stack; NULL for a pseudo-G */
    size_t stack_size; /* usable bytes above the guard */
    _Atomic int status;
    int kind;
    tzr_g *link;        /* run queue, free list */
    tzr_g *all_next;    /* allgs */
    tzr_g *all_prev;
    tzr_scope *scope;       /* the scope this G is counted in (NULL for a pseudo-G) */
    tzr_scope *spawn_scope; /* where tsuzuri_tz_go counts children (NULL: scope, or the root) */
    uint64_t id;
    const char *wait_reason;
    tzr_note note;      /* pseudo-G only */
    int doomed;        /* pseudo-G only: woken by the deadlock detector */
    uint64_t rand;
    tzr_sudog *param;   /* the sudog that completed this G's wait */
    _Atomic uint32_t select_done;
#if defined(TZR_TSAN)
    void *tsan_fiber;
#endif
};

struct tzr_p {
    int id;
    _Atomic uint32_t head;
    _Atomic uint32_t tail;
    _Atomic(tzr_g *) runq[TZR_RUNQ];
    _Atomic(tzr_g *) runnext;
    tzr_p *link;
    uint32_t schedtick;
    tzr_g *gfree;
    int gfree_count;
};

struct tzr_m {
    void *g0_sp;
    tzr_g *curg;
    tzr_p *p;
    tzr_p *nextp;
    tzr_note park;
    tzr_m *link;
    int spinning;
    void (*after)(tzr_m *, tzr_g *);
    void (*after_unlock)(void *);
    void *after_arg;
    pthread_t thread;
    uint64_t rand;
#if defined(TZR_TSAN)
    void *tsan_g0;
#endif
};

static struct {
    pthread_mutex_t lock;
    pthread_once_t once;
    tzr_g *runq_head, *runq_tail;
    _Atomic int64_t runq_size;
    tzr_p *pidle;
    _Atomic int npidle;
    tzr_m *midle;
    int nmidle;
    int mcount;
    _Atomic int nmspinning;
    tzr_p *allp[TZR_MAXPROCS];
    int nprocs;
    _Atomic int64_t ngoroutines;  /* spawned tzroutines that have not finished */
    int npseudo_running;          /* pseudo-Gs that are not parked */
    int npseudo_parked;
    _Atomic int64_t ntimers;      /* timers that will still fire */
    _Atomic uint64_t next_id;
    tzr_g *allgs;                  /* every G with a stack, for the deadlock report */
    tzr_lock allgs_lock;
    tzr_g *gfree;                  /* finished Gs with their stacks, at most TZR_STACK_CACHE */
    int gfree_count;
    tzr_lock gfree_lock;
    size_t stack_size;
    int deadlocked;
} tzr_sched = {.lock = PTHREAD_MUTEX_INITIALIZER, .once = PTHREAD_ONCE_INIT};

static _Thread_local tzr_m *tzr_tls_m;
static _Thread_local tzr_g *tzr_tls_pseudo;

/* R2: every TLS read goes through these noinline functions. */
static TZR_NOINLINE tzr_m *tzr_this_m(void) { return tzr_tls_m; }

static uint64_t tzr_rand(uint64_t *state) {
    /* xorshift64*; the state is never zero */
    uint64_t x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static void tzr_start(void);
static tzr_g *tzr_pseudo(void);

static TZR_NOINLINE tzr_g *tzr_current(void) {
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->curg != NULL) return m->curg;
    return tzr_pseudo();
}

/* ---------------------------------------------------------------- stacks */

/* A stack is one private mapping: TZR_GUARD bytes of PROT_NONE at the low end, then stack_size
 * usable bytes. The OS commits pages only when they are touched. The guard is larger than the
 * largest frame the compiler allows (MAX_FRAME_BYTES = 64 KiB), so no frame can jump over it. */
static char *tzr_stack_map(size_t size) {
    size_t total = TZR_GUARD + size;
    void *map = mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (map == MAP_FAILED) return NULL;
    if (mprotect(map, TZR_GUARD, PROT_NONE) != 0) {
        munmap(map, total);
        return NULL;
    }
    return map;
}

/* Gives the touched pages of a cached stack back to the OS, except the top 16 KiB. */
static void tzr_stack_trim(tzr_g *g) {
    size_t keep = 16 * 1024;
    if (g->stack_size > keep) {
#if defined(__APPLE__)
        madvise(g->stack + TZR_GUARD, g->stack_size - keep, MADV_FREE);
#else
        madvise(g->stack + TZR_GUARD, g->stack_size - keep, MADV_DONTNEED);
#endif
    }
}

/* Whether a fault at `address` hit the guard of the tzroutine that this thread is running. Called
 * by the SIGSEGV/SIGBUS handler of src/runtime/stack.c on the faulting thread's alternate stack:
 * it reads only this thread's M and the G's fixed fields, so it is async-signal-safe. */
TZR_API int32_t tsuzuri_tz_stack_guard_hit(uintptr_t address) {
    tzr_m *m = tzr_tls_m;
    tzr_g *g = m ? m->curg : NULL;
    if (g == NULL || g->stack == NULL) return 0;
    uintptr_t low = (uintptr_t)g->stack;
    return address >= low && address < low + TZR_GUARD + 4096;
}

/* ---------------------------------------------------------------- run queues */

/* The local run queue is a ring that only its P's M pushes to (tail) and that thieves pop from
 * (head, with CAS); see Go's runqput/runqget/runqsteal for the same design. */
static void tzr_globrunqput(tzr_g *g) {
    g->link = NULL;
    if (tzr_sched.runq_tail) tzr_sched.runq_tail->link = g;
    else tzr_sched.runq_head = g;
    tzr_sched.runq_tail = g;
    atomic_fetch_add(&tzr_sched.runq_size, 1);
}

static tzr_g *tzr_globrunqget(tzr_p *p, int max);

static void tzr_runqput_slow(tzr_p *p, tzr_g *g, uint32_t head, uint32_t tail) {
    tzr_g *batch[TZR_RUNQ / 2 + 1];
    uint32_t n = (tail - head) / 2;
    for (uint32_t i = 0; i < n; i++) batch[i] = atomic_load_explicit(&p->runq[(head + i) % TZR_RUNQ], memory_order_relaxed);
    if (!atomic_compare_exchange_strong_explicit(&p->head, &head, head + n, memory_order_acq_rel, memory_order_relaxed)) {
        /* a thief took some; the ring has room now */
        uint32_t t = atomic_load_explicit(&p->tail, memory_order_relaxed);
        atomic_store_explicit(&p->runq[t % TZR_RUNQ], g, memory_order_relaxed);
        atomic_store_explicit(&p->tail, t + 1, memory_order_release);
        return;
    }
    batch[n] = g;
    pthread_mutex_lock(&tzr_sched.lock);
    for (uint32_t i = 0; i <= n; i++) tzr_globrunqput(batch[i]);
    pthread_mutex_unlock(&tzr_sched.lock);
}

static void tzr_runqput(tzr_p *p, tzr_g *g, int next) {
    if (next) {
        g = atomic_exchange_explicit(&p->runnext, g, memory_order_acq_rel);
        if (g == NULL) return;
    }
    uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
    if (tail - head < TZR_RUNQ) {
        atomic_store_explicit(&p->runq[tail % TZR_RUNQ], g, memory_order_relaxed);
        atomic_store_explicit(&p->tail, tail + 1, memory_order_release);
        return;
    }
    tzr_runqput_slow(p, g, head, tail);
}

static tzr_g *tzr_runqget(tzr_p *p) {
    tzr_g *next = atomic_load_explicit(&p->runnext, memory_order_relaxed);
    if (next != NULL && atomic_compare_exchange_strong_explicit(&p->runnext, &next, NULL, memory_order_acq_rel, memory_order_relaxed))
        return next;
    for (;;) {
        uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
        uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
        if (tail == head) return NULL;
        tzr_g *g = atomic_load_explicit(&p->runq[head % TZR_RUNQ], memory_order_relaxed);
        if (atomic_compare_exchange_weak_explicit(&p->head, &head, head + 1, memory_order_acq_rel, memory_order_relaxed)) return g;
    }
}

static int tzr_runq_empty(tzr_p *p) {
    uint32_t head = atomic_load_explicit(&p->head, memory_order_acquire);
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_acquire);
    return head == tail && atomic_load_explicit(&p->runnext, memory_order_acquire) == NULL;
}

/* Moves about half of victim's queue to p's queue and returns one G. */
static tzr_g *tzr_runqsteal(tzr_p *p, tzr_p *victim, int steal_next) {
    tzr_g *batch[TZR_RUNQ / 2];
    uint32_t n;
    for (;;) {
        uint32_t head = atomic_load_explicit(&victim->head, memory_order_acquire);
        uint32_t tail = atomic_load_explicit(&victim->tail, memory_order_acquire);
        n = tail - head;
        n = n - n / 2;
        if (n == 0) {
            if (steal_next) {
                tzr_g *next = atomic_load_explicit(&victim->runnext, memory_order_relaxed);
                if (next != NULL && atomic_compare_exchange_strong_explicit(&victim->runnext, &next, NULL, memory_order_acq_rel, memory_order_relaxed))
                    return next;
            }
            return NULL;
        }
        if (n > TZR_RUNQ / 2) continue; /* inconsistent snapshot */
        for (uint32_t i = 0; i < n; i++) batch[i] = atomic_load_explicit(&victim->runq[(head + i) % TZR_RUNQ], memory_order_relaxed);
        if (atomic_compare_exchange_weak_explicit(&victim->head, &head, head + n, memory_order_acq_rel, memory_order_relaxed)) break;
    }
    tzr_g *g = batch[--n];
    if (n == 0) return g;
    uint32_t tail = atomic_load_explicit(&p->tail, memory_order_relaxed);
    for (uint32_t i = 0; i < n; i++) atomic_store_explicit(&p->runq[(tail + i) % TZR_RUNQ], batch[i], memory_order_relaxed);
    atomic_store_explicit(&p->tail, tail + n, memory_order_release);
    return g;
}

static tzr_g *tzr_globrunqget(tzr_p *p, int max) {
    /* tzr_sched.lock held */
    int64_t size = atomic_load(&tzr_sched.runq_size);
    if (size == 0) return NULL;
    int64_t n = size / tzr_sched.nprocs + 1;
    if (n > size) n = size;
    if (max > 0 && n > max) n = max;
    if (n > TZR_RUNQ / 2) n = TZR_RUNQ / 2;
    tzr_g *g = tzr_sched.runq_head;
    tzr_sched.runq_head = g->link;
    if (tzr_sched.runq_head == NULL) tzr_sched.runq_tail = NULL;
    atomic_fetch_sub(&tzr_sched.runq_size, n);
    for (int64_t i = 1; i < n; i++) {
        tzr_g *more = tzr_sched.runq_head;
        tzr_sched.runq_head = more->link;
        if (tzr_sched.runq_head == NULL) tzr_sched.runq_tail = NULL;
        tzr_runqput(p, more, 0);
    }
    return g;
}

/* ---------------------------------------------------------------- Ps and Ms */

static tzr_p *tzr_pidleget(void) {
    tzr_p *p = tzr_sched.pidle;
    if (p) {
        tzr_sched.pidle = p->link;
        atomic_fetch_sub(&tzr_sched.npidle, 1);
    }
    return p;
}

static void tzr_pidleput(tzr_p *p) {
    p->link = tzr_sched.pidle;
    tzr_sched.pidle = p;
    atomic_fetch_add(&tzr_sched.npidle, 1);
}

static void *tzr_mstart(void *arg);

/* Starts an M with a P (an idle one when p is NULL). tzr_sched.lock not held. */
static void tzr_startm(tzr_p *p, int spinning) {
    pthread_mutex_lock(&tzr_sched.lock);
    if (p == NULL) {
        p = tzr_pidleget();
        if (p == NULL) {
            pthread_mutex_unlock(&tzr_sched.lock);
            if (spinning) atomic_fetch_sub(&tzr_sched.nmspinning, 1);
            return;
        }
    }
    tzr_m *m = tzr_sched.midle;
    if (m) {
        tzr_sched.midle = m->link;
        tzr_sched.nmidle--;
        m->nextp = p;
        m->spinning = spinning;
        pthread_mutex_unlock(&tzr_sched.lock);
        tzr_note_wake(&m->park);
        return;
    }
    m = calloc(1, sizeof *m);
    if (m == NULL) tzr_fatal("out of memory starting a scheduler thread");
    tzr_note_init(&m->park);
    m->nextp = p;
    m->spinning = spinning;
    m->rand = 0x9E3779B97F4A7C15ULL ^ (uint64_t)(uintptr_t)m;
    tzr_sched.mcount++;
    pthread_mutex_unlock(&tzr_sched.lock);
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_setstacksize(&attr, 256 * 1024); /* g0 runs only the scheduler */
    pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    if (pthread_create(&m->thread, &attr, tzr_mstart, m) != 0) tzr_fatal("cannot start a scheduler thread");
    pthread_attr_destroy(&attr);
}

static void tzr_wakep(void) {
    if (atomic_load(&tzr_sched.npidle) == 0) return;
    int expected = 0;
    if (atomic_load(&tzr_sched.nmspinning) != 0 || !atomic_compare_exchange_strong(&tzr_sched.nmspinning, &expected, 1)) return;
    tzr_startm(NULL, 1);
}

static void tzr_checkdead(void);

/* Parks the M until tzr_startm gives it a P. tzr_sched.lock held on entry, not on return. */
static void tzr_stopm(tzr_m *m) {
    m->link = tzr_sched.midle;
    tzr_sched.midle = m;
    tzr_sched.nmidle++;
    tzr_checkdead();
    pthread_mutex_unlock(&tzr_sched.lock);
    tzr_note_sleep(&m->park);
    m->p = m->nextp;
    m->nextp = NULL;
}

/* ---------------------------------------------------------------- deadlock */

static tzr_g *tzr_pseudo_list; /* every pseudo-G, linked through `link` */
static tzr_lock tzr_pseudo_lock;

static void tzr_report_deadlock(void) {
    /* tzr_sched.lock held */
    int64_t waiting = 0;
    char reasons[512] = "";
    tzr_lock_acquire(&tzr_sched.allgs_lock);
    for (tzr_g *g = tzr_sched.allgs; g; g = g->all_next) {
        if (atomic_load(&g->status) != TZR_G_WAITING) continue;
        waiting++;
        if (strlen(reasons) < 400 && g->wait_reason && !strstr(reasons, g->wait_reason)) {
            if (reasons[0]) strcat(reasons, ", ");
            strcat(reasons, g->wait_reason);
        }
    }
    tzr_lock_release(&tzr_sched.allgs_lock);
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g *g = tzr_pseudo_list; g; g = g->link) {
        if (atomic_load(&g->status) != TZR_G_WAITING) continue;
        if (strlen(reasons) < 400 && g->wait_reason && !strstr(reasons, g->wait_reason)) {
            if (reasons[0]) strcat(reasons, ", ");
            strcat(reasons, g->wait_reason);
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    fprintf(stderr, "Tsuzuri runtime: all tzroutines are asleep - deadlock! (%lld waiting%s%s)\n",
            (long long)(waiting + tzr_sched.npseudo_parked), reasons[0] ? ": " : "", reasons);
    fflush(stderr);
}

/* Called with tzr_sched.lock held whenever an M goes idle or a pseudo-G parks. */
static void tzr_checkdead(void) {
    if (tzr_sched.deadlocked) return;
    int running = tzr_sched.mcount - tzr_sched.nmidle + tzr_sched.npseudo_running;
    if (running > 0) return;
    if (atomic_load(&tzr_sched.runq_size) > 0) return;
    if (atomic_load(&tzr_sched.ntimers) > 0) return;
    if (atomic_load(&tzr_sched.ngoroutines) == 0 && tzr_sched.npseudo_parked == 0) return;
    for (int i = 0; i < tzr_sched.nprocs; i++)
        if (!tzr_runq_empty(tzr_sched.allp[i])) return;
    tzr_sched.deadlocked = 1;
    tzr_report_deadlock();
    /* Wake every parked pseudo-G with doomed = 1: its operation returns the deadlock status and the
     * generated code traps at that call site. With no pseudo-G parked, trap here. */
    int woke = 0;
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g *g = tzr_pseudo_list; g; g = g->link) {
        if (atomic_load(&g->status) == TZR_G_WAITING) {
            g->doomed = 1;
            atomic_store(&g->status, TZR_G_RUNNING);
            tzr_sched.npseudo_parked--;
            tzr_sched.npseudo_running++;
            tzr_note_wake(&g->note);
            woke++;
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    if (!woke) __builtin_trap();
}

/* ---------------------------------------------------------------- scheduling */

static void tzr_execute(tzr_m *m, tzr_g *g) {
    m->curg = g;
    m->p->schedtick++;
    atomic_store(&g->status, TZR_G_RUNNING);
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(g->tsan_fiber);
#endif
    tzr_switch(&m->g0_sp, g->sp);
    /* back on g0: the G parked, yielded or exited */
    tzr_g *prev = m->curg;
    m->curg = NULL;
    void (*after)(tzr_m *, tzr_g *) = m->after;
    m->after = NULL;
    if (after) after(m, prev);
}

static tzr_g *tzr_findrunnable(tzr_m *m) {
    for (;;) {
        tzr_p *p = m->p;
        tzr_g *g;
        if (p->schedtick % 61 == 0 && atomic_load(&tzr_sched.runq_size) > 0) {
            pthread_mutex_lock(&tzr_sched.lock);
            g = tzr_globrunqget(p, 1);
            pthread_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
        }
        if ((g = tzr_runqget(p))) return g;
        if (atomic_load(&tzr_sched.runq_size) > 0) {
            pthread_mutex_lock(&tzr_sched.lock);
            g = tzr_globrunqget(p, 0);
            pthread_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
        }
        /* steal, unless enough Ms are already spinning */
        int busy = tzr_sched.nprocs - atomic_load(&tzr_sched.npidle);
        if (m->spinning || 2 * atomic_load(&tzr_sched.nmspinning) < busy) {
            if (!m->spinning) {
                m->spinning = 1;
                atomic_fetch_add(&tzr_sched.nmspinning, 1);
            }
            for (int round = 0; round < 4; round++) {
                int start = (int)(tzr_rand(&m->rand) % (uint64_t)tzr_sched.nprocs);
                for (int i = 0; i < tzr_sched.nprocs; i++) {
                    tzr_p *victim = tzr_sched.allp[(start + i) % tzr_sched.nprocs];
                    if (victim == p) continue;
                    if ((g = tzr_runqsteal(p, victim, round == 3))) return g;
                }
            }
        }
        /* nothing: give the P back and sleep */
        pthread_mutex_lock(&tzr_sched.lock);
        if (atomic_load(&tzr_sched.runq_size) > 0) {
            g = tzr_globrunqget(p, 0);
            pthread_mutex_unlock(&tzr_sched.lock);
            if (g) return g;
            continue;
        }
        tzr_pidleput(p);
        m->p = NULL;
        pthread_mutex_unlock(&tzr_sched.lock);
        if (m->spinning) {
            m->spinning = 0;
            atomic_fetch_sub(&tzr_sched.nmspinning, 1);
            /* A G may have been queued after our last look and before nmspinning dropped: look
             * again, or nobody would wake for it. */
            for (int i = 0; i < tzr_sched.nprocs; i++) {
                if (!tzr_runq_empty(tzr_sched.allp[i]) || atomic_load(&tzr_sched.runq_size) > 0) {
                    pthread_mutex_lock(&tzr_sched.lock);
                    tzr_p *again = tzr_pidleget();
                    pthread_mutex_unlock(&tzr_sched.lock);
                    if (again) {
                        m->p = again;
                        m->spinning = 1;
                        atomic_fetch_add(&tzr_sched.nmspinning, 1);
                    }
                    break;
                }
            }
            if (m->p) continue;
        }
        pthread_mutex_lock(&tzr_sched.lock);
        tzr_stopm(m); /* returns with m->p set */
    }
}

static void tzr_schedule(tzr_m *m) {
    for (;;) {
        tzr_g *g = tzr_findrunnable(m);
        if (m->spinning) {
            m->spinning = 0;
            if (atomic_fetch_sub(&tzr_sched.nmspinning, 1) == 1) tzr_wakep();
        }
        tzr_execute(m, g);
    }
}

static void *tzr_mstart(void *arg) {
    tzr_m *m = arg;
    tzr_tls_m = m;
#if defined(TZR_TSAN)
    m->tsan_g0 = __tsan_get_current_fiber();
#endif
#if defined(TZ_STACK_GUARD)
    tsuzuri_stack_thread(); /* the alternate stack on which stack.c reports an overflow */
#endif
    m->p = m->nextp;
    m->nextp = NULL;
    tzr_schedule(m);
    return NULL;
}

/* A whole decimal number in [low, high] from the environment, or `fallback` for anything else. */
static long tzr_env(const char *name, long low, long high, long fallback) {
    const char *text = getenv(name);
    if (text == NULL || *text == '\0') return fallback;
    char *end = NULL;
    errno = 0;
    long n = strtol(text, &end, 10);
    if (errno != 0 || end == text || *end != '\0' || n < low || n > high) return fallback;
    return n;
}

static void tzr_init(void) {
    long cpus = sysconf(_SC_NPROCESSORS_ONLN);
    int nprocs = cpus < 1 ? 1 : (cpus > TZR_MAXPROCS ? TZR_MAXPROCS : (int)cpus);
    nprocs = (int)tzr_env("TSUZURI_MAXPROCS", 1, TZR_MAXPROCS, nprocs);
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    size_t stack = (size_t)tzr_env("TSUZURI_TZROUTINE_STACK_KIB", 64, 1048576, TZR_DEFAULT_STACK / 1024) * 1024;
    tzr_sched.stack_size = (stack + page - 1) / page * page;
    tzr_sched.nprocs = nprocs;
#if defined(TZ_STACK_GUARD)
    tsuzuri_stack_guard_hook = tsuzuri_tz_stack_guard_hit;
#endif
    for (int i = nprocs - 1; i >= 0; i--) {
        tzr_p *p = calloc(1, sizeof *p);
        if (p == NULL) tzr_fatal("out of memory starting the scheduler");
        p->id = i;
        tzr_sched.allp[i] = p;
        tzr_pidleput(p);
    }
}

static void tzr_start(void) { pthread_once(&tzr_sched.once, tzr_init); }

/* ---------------------------------------------------------------- pseudo-G */

static pthread_key_t tzr_pseudo_key;
static pthread_once_t tzr_pseudo_key_once = PTHREAD_ONCE_INIT;

/* A thread that ends takes its pseudo-G with it. It cannot be parked or inside a scope then: a
 * host thread returns from the export only after tsuzuri_tz_scope_leave. */
static void tzr_pseudo_destroy(void *arg) {
    tzr_g *g = arg;
    tzr_lock_acquire(&tzr_pseudo_lock);
    for (tzr_g **link = &tzr_pseudo_list; *link; link = &(*link)->link) {
        if (*link == g) {
            *link = g->link;
            break;
        }
    }
    tzr_lock_release(&tzr_pseudo_lock);
    pthread_mutex_lock(&tzr_sched.lock);
    tzr_sched.npseudo_running--;
    pthread_mutex_unlock(&tzr_sched.lock);
    pthread_mutex_destroy(&g->note.mutex);
    pthread_cond_destroy(&g->note.cond);
    free(g);
}

static void tzr_pseudo_key_init(void) {
    if (pthread_key_create(&tzr_pseudo_key, tzr_pseudo_destroy) != 0) tzr_fatal("cannot create a thread key");
}

static tzr_g *tzr_pseudo(void) {
    tzr_g *g = tzr_tls_pseudo;
    if (g) return g;
    tzr_start();
    pthread_once(&tzr_pseudo_key_once, tzr_pseudo_key_init);
    g = calloc(1, sizeof *g);
    if (g == NULL) tzr_fatal("out of memory");
    g->kind = TZR_KIND_THREAD;
    atomic_store(&g->status, TZR_G_RUNNING);
    g->rand = 0x2545F4914F6CDD1DULL ^ (uint64_t)(uintptr_t)g;
    g->id = atomic_fetch_add(&tzr_sched.next_id, 1) + 1;
    tzr_note_init(&g->note);
    tzr_lock_acquire(&tzr_pseudo_lock);
    g->link = tzr_pseudo_list;
    tzr_pseudo_list = g;
    tzr_lock_release(&tzr_pseudo_lock);
    pthread_mutex_lock(&tzr_sched.lock);
    tzr_sched.npseudo_running++;
    pthread_mutex_unlock(&tzr_sched.lock);
    tzr_tls_pseudo = g;
    pthread_setspecific(tzr_pseudo_key, g);
    return g;
}

/* ---------------------------------------------------------------- park and ready */

static void tzr_after_park(tzr_m *m, tzr_g *g) {
    (void)g;
    void (*unlock)(void *) = m->after_unlock;
    void *arg = m->after_arg;
    m->after_unlock = NULL;
    if (unlock) unlock(arg);
}

/* Parks the current G until tzr_ready. `unlock(arg)` runs once the G is off its stack (R1).
 * Returns 1 only for a pseudo-G that the deadlock detector woke. */
static int tzr_park(void (*unlock)(void *), void *arg, const char *reason) {
    tzr_g *g = tzr_current();
    g->wait_reason = reason;
    if (g->kind == TZR_KIND_THREAD) {
        pthread_mutex_lock(&tzr_sched.lock);
        atomic_store(&g->status, TZR_G_WAITING);
        tzr_sched.npseudo_running--;
        tzr_sched.npseudo_parked++;
        pthread_mutex_unlock(&tzr_sched.lock);
        if (unlock) unlock(arg);
        pthread_mutex_lock(&tzr_sched.lock);
        tzr_checkdead();
        pthread_mutex_unlock(&tzr_sched.lock);
        tzr_note_sleep(&g->note);
        return g->doomed;
    }
    tzr_m *m = tzr_this_m();
    atomic_store(&g->status, TZR_G_WAITING);
    m->after = tzr_after_park;
    m->after_unlock = unlock;
    m->after_arg = arg;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
    tzr_switch(&g->sp, m->g0_sp);
    return 0;
}

static void tzr_ready(tzr_g *g) {
    if (g->kind == TZR_KIND_THREAD) {
        pthread_mutex_lock(&tzr_sched.lock);
        if (atomic_load(&g->status) != TZR_G_WAITING) {
            pthread_mutex_unlock(&tzr_sched.lock);
            return; /* doomed already */
        }
        atomic_store(&g->status, TZR_G_RUNNING);
        tzr_sched.npseudo_parked--;
        tzr_sched.npseudo_running++;
        pthread_mutex_unlock(&tzr_sched.lock);
        tzr_note_wake(&g->note);
        return;
    }
    int expected = TZR_G_WAITING;
    if (!atomic_compare_exchange_strong(&g->status, &expected, TZR_G_RUNNABLE)) tzr_fatal("bad tzroutine status in ready");
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->p != NULL) {
        tzr_runqput(m->p, g, 1);
    } else {
        pthread_mutex_lock(&tzr_sched.lock);
        tzr_globrunqput(g);
        pthread_mutex_unlock(&tzr_sched.lock);
    }
    tzr_wakep();
}

static void tzr_after_yield(tzr_m *m, tzr_g *g) {
    (void)m;
    atomic_store(&g->status, TZR_G_RUNNABLE);
    pthread_mutex_lock(&tzr_sched.lock);
    tzr_globrunqput(g);
    pthread_mutex_unlock(&tzr_sched.lock);
}

TZR_API void tsuzuri_tz_yield(void) {
    tzr_m *m = tzr_this_m();
    if (m == NULL || m->curg == NULL) {
        sched_yield();
        return;
    }
    tzr_g *g = m->curg;
    m->after = tzr_after_yield;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
    tzr_switch(&g->sp, m->g0_sp);
}

/* ---------------------------------------------------------------- spawn and exit */

static void tzr_after_exit(tzr_m *m, tzr_g *g) {
    atomic_store(&g->status, TZR_G_DEAD);
    tzr_scope *scope = g->scope;
    g->scope = NULL;
    g->spawn_scope = NULL;
    g->code = NULL;
    g->env = NULL;
    /* keep the G and its stack for the next spawn */
    tzr_p *p = m->p;
    if (p->gfree_count < TZR_P_GFREE) {
        g->link = p->gfree;
        p->gfree = g;
        p->gfree_count++;
    } else {
        tzr_stack_trim(g);
        tzr_lock_acquire(&tzr_sched.gfree_lock);
        int keep = tzr_sched.gfree_count < TZR_STACK_CACHE;
        if (keep) {
            g->link = tzr_sched.gfree;
            tzr_sched.gfree = g;
            tzr_sched.gfree_count++;
        }
        tzr_lock_release(&tzr_sched.gfree_lock);
        if (!keep) {
            tzr_lock_acquire(&tzr_sched.allgs_lock);
            if (g->all_prev) g->all_prev->all_next = g->all_next;
            else tzr_sched.allgs = g->all_next;
            if (g->all_next) g->all_next->all_prev = g->all_prev;
            tzr_lock_release(&tzr_sched.allgs_lock);
            munmap(g->stack, TZR_GUARD + g->stack_size);
#if defined(TZR_TSAN)
            __tsan_destroy_fiber(g->tsan_fiber);
#endif
            free(g);
        }
    }
    tzr_g *waiter = NULL;
    tzr_lock_acquire(&scope->lock);
    if (--scope->live == 0) {
        waiter = scope->waiter;
        scope->waiter = NULL;
    }
    tzr_lock_release(&scope->lock);
    atomic_fetch_sub(&tzr_sched.ngoroutines, 1);
    if (waiter) tzr_ready(waiter);
}

static void tzr_gstart(void) {
    tzr_m *m = tzr_this_m();
    tzr_g *g = m->curg;
    g->code(g->env);
    m = tzr_this_m(); /* R2: the body may have moved this G to another M */
    m->after = tzr_after_exit;
#if defined(TZR_TSAN)
    TZR_TSAN_SWITCH(m->tsan_g0);
#endif
    tzr_switch(&g->sp, m->g0_sp);
    __builtin_trap();
}

static tzr_scope tzr_root_scope;

static tzr_g *tzr_galloc(void) {
    tzr_m *m = tzr_this_m();
    tzr_g *g = NULL;
    if (m && m->p && m->p->gfree) {
        g = m->p->gfree;
        m->p->gfree = g->link;
        m->p->gfree_count--;
        return g;
    }
    tzr_lock_acquire(&tzr_sched.gfree_lock);
    if (tzr_sched.gfree) {
        g = tzr_sched.gfree;
        tzr_sched.gfree = g->link;
        tzr_sched.gfree_count--;
    }
    tzr_lock_release(&tzr_sched.gfree_lock);
    if (g) return g;
    g = calloc(1, sizeof *g);
    if (g == NULL) return NULL;
    g->stack = tzr_stack_map(tzr_sched.stack_size);
    if (g->stack == NULL) {
        free(g);
        return NULL;
    }
    g->stack_size = tzr_sched.stack_size;
    g->kind = TZR_KIND_ROUTINE;
#if defined(TZR_TSAN)
    g->tsan_fiber = __tsan_create_fiber(0);
#endif
    g->rand = 0x9E3779B97F4A7C15ULL ^ (uint64_t)(uintptr_t)g;
    tzr_lock_acquire(&tzr_sched.allgs_lock);
    g->all_next = tzr_sched.allgs;
    g->all_prev = NULL;
    if (tzr_sched.allgs) tzr_sched.allgs->all_prev = g;
    tzr_sched.allgs = g;
    tzr_lock_release(&tzr_sched.allgs_lock);
    return g;
}

TZR_API void tsuzuri_tz_go(void (*code)(void *), void *env) {
    tzr_start();
    tzr_g *parent = tzr_current();
    tzr_scope *scope = parent->spawn_scope ? parent->spawn_scope : parent->scope;
    if (scope == NULL) scope = &tzr_root_scope;
    tzr_g *g = tzr_galloc();
    if (g == NULL) tzr_fatal("cannot allocate a tzroutine stack (out of memory or address space)");
    g->code = code;
    g->env = env;
    g->scope = scope;
    g->id = atomic_fetch_add(&tzr_sched.next_id, 1) + 1;
    g->wait_reason = NULL;
    g->sp = tzr_initial_frame(g->stack + TZR_GUARD + g->stack_size, tzr_gstart);
    tzr_lock_acquire(&scope->lock);
    scope->live++;
    tzr_lock_release(&scope->lock);
    atomic_fetch_add(&tzr_sched.ngoroutines, 1);
    atomic_store(&g->status, TZR_G_RUNNABLE);
    tzr_m *m = tzr_this_m();
    if (m != NULL && m->p != NULL) {
        tzr_runqput(m->p, g, 1);
    } else {
        pthread_mutex_lock(&tzr_sched.lock);
        tzr_globrunqput(g);
        pthread_mutex_unlock(&tzr_sched.lock);
    }
    tzr_wakep();
}

/* ---------------------------------------------------------------- scopes */

TZR_API void *tsuzuri_tz_scope_enter(void) {
    tzr_scope *scope = calloc(1, sizeof *scope);
    if (scope == NULL) tzr_fatal("out of memory");
    tzr_g *g = tzr_current();
    scope->outer = g->spawn_scope;
    g->spawn_scope = scope;
    return scope;
}

static void tzr_unlock_scope(void *arg) { tzr_lock_release(&((tzr_scope *)arg)->lock); }

/* Waits until every tzroutine spawned in `scope` (and their descendants) has finished. Returns 1
 * when the deadlock detector ended the wait. */
TZR_API int32_t tsuzuri_tz_scope_leave(void *opaque) {
    tzr_scope *scope = opaque;
    int doomed = 0;
    for (;;) {
        tzr_lock_acquire(&scope->lock);
        if (scope->live == 0) {
            tzr_lock_release(&scope->lock);
            break;
        }
        scope->waiter = tzr_current();
        if (tzr_park(tzr_unlock_scope, scope, "end of scope (waiting for tzroutines)")) {
            doomed = 1;
            break;
        }
    }
    tzr_current()->spawn_scope = scope->outer;
    if (!doomed) free(scope);
    return doomed;
}

/* Routine.count: the tzroutines that exist, counting the caller's thread as one (Go counts main). */
TZR_API int64_t tsuzuri_tz_count(void) { return atomic_load(&tzr_sched.ngoroutines) + 1; }

/* Routine.max_procs: the number of Ps (TSUZURI_MAXPROCS, or the online CPUs, at most 256). */
TZR_API int64_t tsuzuri_tz_max_procs(void) {
    tzr_start();
    return tzr_sched.nprocs;
}

/* Routine.cpu_count: the online CPUs, at least 1. */
TZR_API int64_t tsuzuri_tz_cpu_count(void) {
    long cpus = sysconf(_SC_NPROCESSORS_ONLN);
    return cpus < 1 ? 1 : cpus;
}

/* ---------------------------------------------------------------- parking lot (sync) */

/* Address-keyed waiting, like a futex: tzr_addr_wait parks while *addr == expected, tzr_addr_wake
 * wakes up to n waiters of addr. Mutex, RwMutex, WaitGroup, Once, Cond and Semaphore use it. */
#define TZR_BUCKETS 251
typedef struct tzr_waiter {
    tzr_g *g;
    void *addr;
    struct tzr_waiter *next;
} tzr_waiter;
static struct {
    tzr_lock lock;
    tzr_waiter *first, *last;
} tzr_buckets[TZR_BUCKETS];

static size_t tzr_bucket(void *addr) { return ((uintptr_t)addr >> 3) % TZR_BUCKETS; }
static void tzr_unlock_bucket(void *arg) { tzr_lock_release(&tzr_buckets[(size_t)(uintptr_t)arg].lock); }

/* Returns 1 for a doomed pseudo-G (deadlock). */
static int tzr_addr_wait(_Atomic uint64_t *addr, uint64_t expected, const char *reason) {
    size_t b = tzr_bucket((void *)addr);
    tzr_lock_acquire(&tzr_buckets[b].lock);
    if (atomic_load(addr) != expected) {
        tzr_lock_release(&tzr_buckets[b].lock);
        return 0;
    }
    tzr_waiter w = {tzr_current(), (void *)addr, NULL};
    if (tzr_buckets[b].last) tzr_buckets[b].last->next = &w;
    else tzr_buckets[b].first = &w;
    tzr_buckets[b].last = &w;
    return tzr_park(tzr_unlock_bucket, (void *)(uintptr_t)b, reason);
}

static int tzr_addr_wake(_Atomic uint64_t *addr, int n) {
    size_t b = tzr_bucket((void *)addr);
    tzr_g *woken[64];
    int count = 0;
    tzr_lock_acquire(&tzr_buckets[b].lock);
    tzr_waiter **link = &tzr_buckets[b].first;
    tzr_waiter *last = NULL;
    while (*link && (n < 0 || count < n) && count < 64) {
        tzr_waiter *w = *link;
        if (w->addr == (void *)addr) {
            *link = w->next;
            woken[count++] = w->g;
        } else {
            last = w;
            link = &w->next;
        }
    }
    if (*link == NULL) tzr_buckets[b].last = last;
    tzr_lock_release(&tzr_buckets[b].lock);
    for (int i = 0; i < count; i++) tzr_ready(woken[i]);
    return count == 64 && n < 0 ? 64 + tzr_addr_wake(addr, n) : count;
}

/* The parking lot for the sync types that TZ06 writes in C (RwMutex, Once, Cond, Semaphore). */
TZR_API int32_t tsuzuri_tz_addr_wait(_Atomic uint64_t *addr, uint64_t expected) {
    return tzr_addr_wait(addr, expected, "sync wait") ? 2 : 0;
}

TZR_API int64_t tsuzuri_tz_addr_wake(_Atomic uint64_t *addr, int64_t n) {
    return tzr_addr_wake(addr, n < 0 ? -1 : (int)(n > 1000000 ? 1000000 : n));
}

/* Mutex: the word is 0 free, 1 held, 2 held with waiters (Drepper's three states). */
TZR_API int32_t tsuzuri_tz_mutex_lock(_Atomic uint64_t *word) {
    uint64_t c = 0;
    if (atomic_compare_exchange_strong_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed)) return 0;
    for (int spin = 0; spin < 32; spin++) {
        c = 0;
        if (atomic_load_explicit(word, memory_order_relaxed) == 0
            && atomic_compare_exchange_weak_explicit(word, &c, 1, memory_order_acquire, memory_order_relaxed)) return 0;
    }
    while (atomic_exchange_explicit(word, 2, memory_order_acquire) != 0)
        if (tzr_addr_wait(word, 2, "Mutex lock")) return 2;
    return 0;
}

TZR_API void tsuzuri_tz_mutex_unlock(_Atomic uint64_t *word) {
    if (atomic_exchange_explicit(word, 0, memory_order_release) == 2) tzr_addr_wake(word, 1);
}

/* WaitGroup: the word is the counter. */
TZR_API int32_t tsuzuri_tz_waitgroup_add(_Atomic uint64_t *word, int64_t delta) {
    int64_t v = (int64_t)atomic_fetch_add(word, (uint64_t)delta) + delta;
    if (v < 0) return 1; /* the generated code traps: negative WaitGroup counter */
    if (v == 0) tzr_addr_wake(word, -1);
    return 0;
}

TZR_API int32_t tsuzuri_tz_waitgroup_wait(_Atomic uint64_t *word) {
    for (;;) {
        uint64_t v = atomic_load(word);
        if (v == 0) return 0;
        if (tzr_addr_wait(word, v, "WaitGroup.wait")) return 2;
    }
}

/* ---------------------------------------------------------------- timers */

typedef struct tzr_timer {
    int64_t when;
    tzr_g *g;
} tzr_timer;

/* One heap of timers under a tzr_lock, and one thread that sleeps until the earliest one. The
 * thread sleeps on its own mutex and condition variable (`kicked` prevents a lost wakeup), so the
 * heap lock is a tzr_lock that tzr_park can release (R4). */
static struct {
    tzr_lock lock;
    tzr_timer **heap;
    size_t count, capacity;
    int started;
    pthread_mutex_t wake_mutex;
    pthread_cond_t wake;
    int kicked;
} tzr_timers = {.wake_mutex = PTHREAD_MUTEX_INITIALIZER, .wake = PTHREAD_COND_INITIALIZER};

static void tzr_heap_up(size_t i) {
    while (i > 0) {
        size_t parent = (i - 1) / 2;
        if (tzr_timers.heap[parent]->when <= tzr_timers.heap[i]->when) break;
        tzr_timer *t = tzr_timers.heap[parent];
        tzr_timers.heap[parent] = tzr_timers.heap[i];
        tzr_timers.heap[i] = t;
        i = parent;
    }
}

static void tzr_heap_down(size_t i) {
    for (;;) {
        size_t l = 2 * i + 1, r = l + 1, m = i;
        if (l < tzr_timers.count && tzr_timers.heap[l]->when < tzr_timers.heap[m]->when) m = l;
        if (r < tzr_timers.count && tzr_timers.heap[r]->when < tzr_timers.heap[m]->when) m = r;
        if (m == i) return;
        tzr_timer *t = tzr_timers.heap[m];
        tzr_timers.heap[m] = tzr_timers.heap[i];
        tzr_timers.heap[i] = t;
        i = m;
    }
}

static void *tzr_timerproc(void *arg) {
    (void)arg;
    for (;;) {
        tzr_lock_acquire(&tzr_timers.lock);
        int64_t now = tzr_nanotime();
        while (tzr_timers.count > 0 && tzr_timers.heap[0]->when <= now) {
            tzr_timer *top = tzr_timers.heap[0];
            tzr_timers.heap[0] = tzr_timers.heap[--tzr_timers.count];
            tzr_heap_down(0);
            tzr_ready(top->g); /* the G may run and leave `top` (its stack frame) at once */
            atomic_fetch_sub(&tzr_sched.ntimers, 1);
        }
        int64_t next = tzr_timers.count > 0 ? tzr_timers.heap[0]->when : -1;
        tzr_lock_release(&tzr_timers.lock);
        pthread_mutex_lock(&tzr_timers.wake_mutex);
        if (!tzr_timers.kicked) {
            if (next < 0) {
                pthread_cond_wait(&tzr_timers.wake, &tzr_timers.wake_mutex);
            } else {
                struct timespec ts;
                clock_gettime(CLOCK_REALTIME, &ts);
                int64_t abs = (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec + (next - now);
                ts.tv_sec = abs / 1000000000;
                ts.tv_nsec = abs % 1000000000;
                pthread_cond_timedwait(&tzr_timers.wake, &tzr_timers.wake_mutex, &ts);
            }
        }
        tzr_timers.kicked = 0;
        pthread_mutex_unlock(&tzr_timers.wake_mutex);
    }
    return NULL;
}

static void tzr_unlock_timers(void *arg) {
    (void)arg;
    tzr_lock_release(&tzr_timers.lock);
}

TZR_API int32_t tsuzuri_tz_sleep(int64_t ns) {
    if (ns <= 0) {
        tsuzuri_tz_yield();
        return 0;
    }
    tzr_start();
    int64_t now = tzr_nanotime();
    tzr_timer t = {now > INT64_MAX - ns ? INT64_MAX : now + ns, tzr_current()};
    tzr_lock_acquire(&tzr_timers.lock);
    if (!tzr_timers.started) {
        pthread_t thread;
        pthread_attr_t attr;
        pthread_attr_init(&attr);
        pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
        if (pthread_create(&thread, &attr, tzr_timerproc, NULL) != 0) tzr_fatal("cannot start the timer thread");
        pthread_attr_destroy(&attr);
        tzr_timers.started = 1;
    }
    if (tzr_timers.count == tzr_timers.capacity) {
        size_t capacity = tzr_timers.capacity ? tzr_timers.capacity * 2 : 64;
        tzr_timer **heap = realloc(tzr_timers.heap, capacity * sizeof *heap);
        if (heap == NULL) tzr_fatal("out of memory");
        tzr_timers.heap = heap;
        tzr_timers.capacity = capacity;
    }
    tzr_timers.heap[tzr_timers.count] = &t;
    tzr_heap_up(tzr_timers.count++);
    atomic_fetch_add(&tzr_sched.ntimers, 1);
    if (tzr_timers.heap[0] == &t) {
        pthread_mutex_lock(&tzr_timers.wake_mutex);
        tzr_timers.kicked = 1;
        pthread_cond_signal(&tzr_timers.wake);
        pthread_mutex_unlock(&tzr_timers.wake_mutex);
    }
    return tzr_park(tzr_unlock_timers, NULL, "Time.sleep");
}

/* ---------------------------------------------------------------- channels */

struct tzr_sudog {
    tzr_g *g;
    tzr_sudog *next, *prev;
    void *elem;
    tzr_chan *c;
    int is_select;
    int success;
};

typedef struct {
    tzr_sudog *first, *last;
} tzr_waitq;

/* The block that Channel.make allocates: this header, then capacity * elem_size bytes. */
struct tzr_chan {
    tzr_lock lock;
    _Atomic uint64_t refs;
    uint64_t elem_size;
    uint64_t capacity;
    uint64_t count;
    uint64_t sendx;
    uint64_t recvx;
    uint64_t closed;
    tzr_waitq recvq;
    tzr_waitq sendq;
    uint64_t reserved[4];
};
_Static_assert(sizeof(struct tzr_chan) == 128, "channel header is 128 bytes");
#define TZR_CHAN_BUF(c, i) ((char *)(c) + sizeof(struct tzr_chan) + (i) * (c)->elem_size)

enum { TZR_OK = 0, TZR_CLOSED = 1, TZR_DEADLOCK = 2, TZR_WOULD_BLOCK = 3 };

static void tzr_enqueue(tzr_waitq *q, tzr_sudog *s) {
    s->next = NULL;
    s->prev = q->last;
    if (q->last) q->last->next = s;
    else q->first = s;
    q->last = s;
}

static void tzr_unlink(tzr_waitq *q, tzr_sudog *s) {
    if (s->prev) s->prev->next = s->next;
    else if (q->first == s) q->first = s->next;
    else return; /* not queued */
    if (s->next) s->next->prev = s->prev;
    else q->last = s->prev;
    s->next = s->prev = NULL;
}

/* Takes the first waiter that is not a select that another case already won. */
static tzr_sudog *tzr_dequeue(tzr_waitq *q) {
    for (;;) {
        tzr_sudog *s = q->first;
        if (s == NULL) return NULL;
        q->first = s->next;
        if (q->first) q->first->prev = NULL;
        else q->last = NULL;
        s->next = s->prev = NULL;
        if (s->is_select) {
            uint32_t expected = 0;
            if (!atomic_compare_exchange_strong(&s->g->select_done, &expected, 1)) continue;
        }
        return s;
    }
}

TZR_API void tsuzuri_tz_chan_init(tzr_chan *c, int64_t elem_size, int64_t capacity) {
    memset(c, 0, sizeof *c);
    atomic_store(&c->refs, 1);
    c->elem_size = (uint64_t)elem_size;
    c->capacity = (uint64_t)capacity;
}

TZR_API void tsuzuri_tz_chan_share(tzr_chan *c) { atomic_fetch_add_explicit(&c->refs, 1, memory_order_relaxed); }

/* Returns 1 when this was the last handle: the generated code then takes the buffered items with
 * tsuzuri_tz_chan_drain, drops them and frees the block. */
TZR_API int32_t tsuzuri_tz_chan_release(tzr_chan *c) {
    return atomic_fetch_sub_explicit(&c->refs, 1, memory_order_acq_rel) == 1;
}

TZR_API int32_t tsuzuri_tz_chan_drain(tzr_chan *c, void *out) {
    if (c->count == 0) return 0;
    memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
    c->recvx = (c->recvx + 1) % c->capacity;
    c->count--;
    return 1;
}

static void tzr_unlock_chan(void *arg) { tzr_lock_release(&((tzr_chan *)arg)->lock); }

static void tzr_send_to(tzr_chan *c, tzr_sudog *s, const void *elem) {
    /* c->lock held; s is a receiver */
    if (s->elem) memcpy(s->elem, elem, c->elem_size);
    s->success = 1;
    s->g->param = s;
}

static void tzr_recv_from(tzr_chan *c, tzr_sudog *s, void *out) {
    /* c->lock held; s is a sender and the buffer, if any, is full */
    if (c->capacity == 0) {
        memcpy(out, s->elem, c->elem_size);
    } else {
        memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
        memcpy(TZR_CHAN_BUF(c, c->recvx), s->elem, c->elem_size);
        c->recvx = (c->recvx + 1) % c->capacity;
        c->sendx = c->recvx;
    }
    s->success = 1;
    s->g->param = s;
}

/* block = 0 is try_send. Returns TZR_OK, TZR_CLOSED (the generated code traps for send),
 * TZR_WOULD_BLOCK (try only) or TZR_DEADLOCK. */
TZR_API int32_t tsuzuri_tz_chan_send(tzr_chan *c, const void *elem, int32_t block) {
    tzr_lock_acquire(&c->lock);
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    tzr_sudog *s = tzr_dequeue(&c->recvq);
    if (s) {
        tzr_send_to(c, s, elem);
        tzr_lock_release(&c->lock);
        tzr_ready(s->g);
        return TZR_OK;
    }
    if (c->count < c->capacity) {
        memcpy(TZR_CHAN_BUF(c, c->sendx), elem, c->elem_size);
        c->sendx = (c->sendx + 1) % c->capacity;
        c->count++;
        tzr_lock_release(&c->lock);
        return TZR_OK;
    }
    if (!block) {
        tzr_lock_release(&c->lock);
        return TZR_WOULD_BLOCK;
    }
    tzr_sudog me = {.g = tzr_current(), .elem = (void *)elem, .c = c};
    me.g->param = NULL;
    tzr_enqueue(&c->sendq, &me);
    if (tzr_park(tzr_unlock_chan, c, "Channel.send")) return TZR_DEADLOCK;
    return me.success ? TZR_OK : TZR_CLOSED;
}

/* Returns TZR_OK (out holds the item), TZR_CLOSED (closed and empty), TZR_WOULD_BLOCK or TZR_DEADLOCK. */
TZR_API int32_t tsuzuri_tz_chan_recv(tzr_chan *c, void *out, int32_t block) {
    tzr_lock_acquire(&c->lock);
    tzr_sudog *s = tzr_dequeue(&c->sendq);
    if (s) {
        tzr_recv_from(c, s, out);
        tzr_lock_release(&c->lock);
        tzr_ready(s->g);
        return TZR_OK;
    }
    if (c->count > 0) {
        memcpy(out, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
        c->recvx = (c->recvx + 1) % c->capacity;
        c->count--;
        tzr_lock_release(&c->lock);
        return TZR_OK;
    }
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    if (!block) {
        tzr_lock_release(&c->lock);
        return TZR_WOULD_BLOCK;
    }
    tzr_sudog me = {.g = tzr_current(), .elem = out, .c = c};
    me.g->param = NULL;
    tzr_enqueue(&c->recvq, &me);
    if (tzr_park(tzr_unlock_chan, c, "Channel.recv")) return TZR_DEADLOCK;
    return me.success ? TZR_OK : TZR_CLOSED;
}

/* Returns TZR_OK, or TZR_CLOSED when it was closed already (the generated code traps). */
TZR_API int32_t tsuzuri_tz_chan_close(tzr_chan *c) {
    tzr_g *wake[64];
    int n = 0;
    tzr_lock_acquire(&c->lock);
    if (c->closed) {
        tzr_lock_release(&c->lock);
        return TZR_CLOSED;
    }
    c->closed = 1;
    for (;;) {
        while (n < 64) {
            tzr_sudog *s = tzr_dequeue(&c->recvq);
            if (s == NULL) s = tzr_dequeue(&c->sendq);
            if (s == NULL) break;
            s->success = 0;
            s->g->param = s;
            wake[n++] = s->g;
        }
        if (n < 64) break;
        /* more than 64 waiters: wake this batch with the lock held (R3 allows it) and go on */
        for (int i = 0; i < n; i++) tzr_ready(wake[i]);
        n = 0;
    }
    tzr_lock_release(&c->lock);
    for (int i = 0; i < n; i++) tzr_ready(wake[i]);
    return TZR_OK;
}

TZR_API int64_t tsuzuri_tz_chan_capacity(tzr_chan *c) { return (int64_t)c->capacity; }

TZR_API int64_t tsuzuri_tz_chan_length(tzr_chan *c) {
    tzr_lock_acquire(&c->lock);
    int64_t n = (int64_t)c->count;
    tzr_lock_release(&c->lock);
    return n;
}

/* ---------------------------------------------------------------- select */

/* One case of a select, built by the generated code in a stack array. */
typedef struct tzr_scase {
    tzr_chan *c;
    void *elem;      /* send: the value; recv: where the item goes */
    int32_t send;    /* 1 send, 0 recv */
    int32_t pad;
    tzr_sudog sudog;  /* scratch for the runtime */
} tzr_scase;

_Static_assert(sizeof(tzr_sudog) == 48, "a sudog is 6 words");
_Static_assert(sizeof(tzr_scase) == 72, "the generated code lays out a case as { ptr, ptr, i32, i32, [6 x i64] }");

typedef struct {
    tzr_scase *cases;
    uint16_t *lockorder;
    int64_t n;
} tzr_selock;

static void tzr_sellock(tzr_scase *cases, uint16_t *lockorder, int64_t n) {
    tzr_chan *last = NULL;
    for (int64_t i = 0; i < n; i++) {
        tzr_chan *c = cases[lockorder[i]].c;
        if (c != last) tzr_lock_acquire(&c->lock);
        last = c;
    }
}

static void tzr_selunlock(tzr_scase *cases, uint16_t *lockorder, int64_t n) {
    /* in reverse, and once per channel */
    for (int64_t i = n - 1; i >= 0; i--) {
        tzr_chan *c = cases[lockorder[i]].c;
        if (i > 0 && cases[lockorder[i - 1]].c == c) continue;
        tzr_lock_release(&c->lock);
    }
}

static void tzr_selunlock_arg(void *arg) {
    tzr_selock *s = arg;
    tzr_selunlock(s->cases, s->lockorder, s->n);
}

/* Returns the index of the chosen case, or -1 when block is 0 and no case was ready (the
 * `default` arm). *ok is 1 when a recv case got an item and 0 when its channel was closed; a send
 * case on a closed channel returns with *ok = 0 and the generated code traps. Returns -2 for a
 * doomed pseudo-G. n == 0 with block parks forever. n <= 65535. */
TZR_API int64_t tsuzuri_tz_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok) {
    uint16_t pollorder[n > 0 ? n : 1], lockorder[n > 0 ? n : 1];
    tzr_g *g = tzr_current();
    *ok = 0;
    if (n == 0) {
        if (!block) return -1;
        return tzr_park(NULL, NULL, "select (no cases)") ? -2 : -1;
    }
    /* pollorder: uniform random permutation (Fisher-Yates) */
    for (int64_t i = 0; i < n; i++) {
        int64_t j = (int64_t)(tzr_rand(&g->rand) % (uint64_t)(i + 1));
        pollorder[i] = pollorder[j];
        pollorder[j] = (uint16_t)i;
    }
    /* lockorder: by channel address (insertion sort; n is small) */
    for (int64_t i = 0; i < n; i++) {
        int64_t j = i;
        while (j > 0 && (uintptr_t)cases[lockorder[j - 1]].c > (uintptr_t)cases[i].c) {
            lockorder[j] = lockorder[j - 1];
            j--;
        }
        lockorder[j] = (uint16_t)i;
    }
    tzr_sellock(cases, lockorder, n);
    /* pass 1: a ready case */
    for (int64_t k = 0; k < n; k++) {
        int64_t i = pollorder[k];
        tzr_scase *cs = &cases[i];
        tzr_chan *c = cs->c;
        if (cs->send) {
            if (c->closed) {
                tzr_selunlock(cases, lockorder, n);
                return i;
            }
            tzr_sudog *s = tzr_dequeue(&c->recvq);
            if (s) {
                tzr_send_to(c, s, cs->elem);
                tzr_selunlock(cases, lockorder, n);
                tzr_ready(s->g);
                *ok = 1;
                return i;
            }
            if (c->count < c->capacity) {
                memcpy(TZR_CHAN_BUF(c, c->sendx), cs->elem, c->elem_size);
                c->sendx = (c->sendx + 1) % c->capacity;
                c->count++;
                tzr_selunlock(cases, lockorder, n);
                *ok = 1;
                return i;
            }
        } else {
            tzr_sudog *s = tzr_dequeue(&c->sendq);
            if (s) {
                tzr_recv_from(c, s, cs->elem);
                tzr_selunlock(cases, lockorder, n);
                tzr_ready(s->g);
                *ok = 1;
                return i;
            }
            if (c->count > 0) {
                memcpy(cs->elem, TZR_CHAN_BUF(c, c->recvx), c->elem_size);
                c->recvx = (c->recvx + 1) % c->capacity;
                c->count--;
                tzr_selunlock(cases, lockorder, n);
                *ok = 1;
                return i;
            }
            if (c->closed) {
                tzr_selunlock(cases, lockorder, n);
                return i;
            }
        }
    }
    if (!block) {
        tzr_selunlock(cases, lockorder, n);
        return -1;
    }
    /* pass 2: wait on every case */
    atomic_store(&g->select_done, 0);
    g->param = NULL;
    for (int64_t k = 0; k < n; k++) {
        int64_t i = lockorder[k];
        tzr_scase *cs = &cases[i];
        cs->sudog = (tzr_sudog){.g = g, .elem = cs->elem, .c = cs->c, .is_select = 1};
        tzr_enqueue(cs->send ? &cs->c->sendq : &cs->c->recvq, &cs->sudog);
    }
    tzr_selock arg = {cases, lockorder, n};
    if (tzr_park(tzr_selunlock_arg, &arg, "select")) {
        tzr_sellock(cases, lockorder, n);
        for (int64_t i = 0; i < n; i++) tzr_unlink(cases[i].send ? &cases[i].c->sendq : &cases[i].c->recvq, &cases[i].sudog);
        tzr_selunlock(cases, lockorder, n);
        return -2;
    }
    /* pass 3: the waker set g->param to the winning sudog; leave the other queues */
    tzr_sellock(cases, lockorder, n);
    tzr_sudog *won = g->param;
    int64_t chosen = -1;
    for (int64_t i = 0; i < n; i++) {
        tzr_scase *cs = &cases[i];
        if (&cs->sudog == won) chosen = i;
        else tzr_unlink(cs->send ? &cs->c->sendq : &cs->c->recvq, &cs->sudog);
    }
    tzr_selunlock(cases, lockorder, n);
    if (chosen < 0) tzr_fatal("select woke without a case");
    *ok = won->success;
    return chosen;
}
```

### B-2 `tests/tzroutine_runtime.c`（そのまま置く）

引数でケースを 1 つ選ぶ（なければ全部）。`child-*` は `failures`・`stack_size` が自分を起こし直すときの入口。

```c
/* Tests of the tzroutine runtime (TZ01), run by tests/tzroutine_runtime.mjs. By hand:
 * clang -std=c11 -Wall -Wextra -Werror -O2 -pthread tests/tzroutine_runtime.c -o t && ./t [case] */
#include "../src/runtime/tzroutine.c"

#include <assert.h>
#include <signal.h>
#include <sys/wait.h>

#define CHECK(cond)                                                              \
    do {                                                                         \
        if (!(cond)) {                                                           \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #cond); \
            exit(1);                                                             \
        }                                                                        \
    } while (0)

static double seconds(void) { return (double)tzr_nanotime() / 1e9; }

static tzr_chan *chan_make(int64_t elem, int64_t cap) {
    tzr_chan *c = calloc(1, sizeof(tzr_chan) + (size_t)(elem * cap));
    tsuzuri_tz_chan_init(c, elem, cap);
    return c;
}

static void join(void *scope) { CHECK(tsuzuri_tz_scope_leave(scope) == 0); }

/* 1. spawn many */
static _Atomic int64_t counter;
static void incr(void *env) {
    atomic_fetch_add(&counter, (int64_t)(intptr_t)env);
}
static void test_spawn(void) {
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    for (int i = 0; i < 100000; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    join(scope);
    CHECK(atomic_load(&counter) == 100000);
    printf("spawn: 100000 tzroutines in %.3f s\n", seconds() - t);
}

/* 2. nested spawn: a tree of 2^15 routines */
static void tree(void *env) {
    intptr_t depth = (intptr_t)env;
    atomic_fetch_add(&counter, 1);
    if (depth > 0) {
        tsuzuri_tz_go(tree, (void *)(depth - 1));
        tsuzuri_tz_go(tree, (void *)(depth - 1));
    }
}
static void test_tree(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(tree, (void *)(intptr_t)14);
    join(scope);
    CHECK(atomic_load(&counter) == (1 << 15) - 1);
    printf("tree: %lld routines\n", (long long)atomic_load(&counter));
}

/* 3. unbuffered ping-pong */
typedef struct { tzr_chan *ping, *pong; int64_t rounds; } pingpong;
static void ponger(void *env) {
    pingpong *pp = env;
    for (int64_t i = 0; i < pp->rounds; i++) {
        int64_t v;
        CHECK(tsuzuri_tz_chan_recv(pp->ping, &v, 1) == TZR_OK);
        v++;
        CHECK(tsuzuri_tz_chan_send(pp->pong, &v, 1) == TZR_OK);
    }
}
static void pinger(void *env) {
    pingpong *pp = env;
    for (int64_t i = 0; i < pp->rounds; i++) {
        int64_t v = i, w;
        CHECK(tsuzuri_tz_chan_send(pp->ping, &v, 1) == TZR_OK);
        CHECK(tsuzuri_tz_chan_recv(pp->pong, &w, 1) == TZR_OK);
        CHECK(w == i + 1);
    }
}
static void test_pingpong(int64_t rounds, int main_thread) {
    pingpong pp = {chan_make(8, 0), chan_make(8, 0), rounds};
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    tsuzuri_tz_go(ponger, &pp);
    if (main_thread) pinger(&pp);
    else tsuzuri_tz_go(pinger, &pp);
    join(scope);
    double e = seconds() - t;
    printf("pingpong(%s): %lld round trips, %.0f ns each\n", main_thread ? "main" : "routines", (long long)rounds, e * 1e9 / (double)rounds);
    free(pp.ping);
    free(pp.pong);
}

/* 4. MPMC buffered */
typedef struct { tzr_chan *c; int64_t count; _Atomic int64_t *sum; tzr_chan *done; } mpmc;
static void producer(void *env) {
    mpmc *m = env;
    for (int64_t i = 1; i <= m->count; i++) CHECK(tsuzuri_tz_chan_send(m->c, &i, 1) == TZR_OK);
    int64_t one = 1;
    CHECK(tsuzuri_tz_chan_send(m->done, &one, 1) == TZR_OK);
}
static void consumer(void *env) {
    mpmc *m = env;
    int64_t v, local = 0;
    while (tsuzuri_tz_chan_recv(m->c, &v, 1) == TZR_OK) local += v;
    atomic_fetch_add(m->sum, local);
}
static void closer(void *env) {
    mpmc *m = env;
    int64_t v;
    for (int i = 0; i < 4; i++) CHECK(tsuzuri_tz_chan_recv(m->done, &v, 1) == TZR_OK);
    CHECK(tsuzuri_tz_chan_close(m->c) == TZR_OK);
    CHECK(tsuzuri_tz_chan_close(m->c) == TZR_CLOSED);
}
static void test_mpmc(int64_t cap) {
    _Atomic int64_t sum = 0;
    mpmc m = {chan_make(8, cap), 50000, &sum, chan_make(8, 4)};
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(producer, &m);
    for (int i = 0; i < 4; i++) tsuzuri_tz_go(consumer, &m);
    tsuzuri_tz_go(closer, &m);
    join(scope);
    CHECK(atomic_load(&sum) == 4 * (50000LL * 50001LL / 2));
    int64_t v;
    CHECK(tsuzuri_tz_chan_recv(m.c, &v, 1) == TZR_CLOSED);
    CHECK(tsuzuri_tz_chan_send(m.c, &v, 1) == TZR_CLOSED);
    printf("mpmc(cap %lld): ok\n", (long long)cap);
    free(m.c);
    free(m.done);
}

/* 5. select fairness and default */
static void feeder(void *env) {
    tzr_chan *c = env;
    int64_t v = 1;
    for (;;) {
        if (tsuzuri_tz_chan_send(c, &v, 1) != TZR_OK) return;
    }
}
static void test_select(void) {
    tzr_chan *a = chan_make(8, 0), *b = chan_make(8, 0), *quit = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(feeder, a);
    tsuzuri_tz_go(feeder, b);
    int64_t hits[2] = {0, 0};
    for (int i = 0; i < 20000; i++) {
        int64_t va = 0, vb = 0;
        tzr_scase cases[2] = {{.c = a, .elem = &va}, {.c = b, .elem = &vb}};
        int32_t ok;
        int64_t k = tsuzuri_tz_select(cases, 2, 1, &ok);
        CHECK(k == 0 || k == 1);
        CHECK(ok == 1);
        hits[k]++;
    }
    /* both feeders are always ready eventually; the choice must not starve either */
    CHECK(hits[0] > 4000 && hits[1] > 4000);
    /* default: nothing is ready on quit */
    int64_t q;
    tzr_scase none[1] = {{.c = quit, .elem = &q}};
    int32_t ok;
    CHECK(tsuzuri_tz_select(none, 1, 0, &ok) == -1);
    tsuzuri_tz_chan_close(a);
    tsuzuri_tz_chan_close(b);
    /* drain the feeders: they get CLOSED from their blocked send and return */
    join(scope);
    /* a closed channel is ready for receive with ok = 0 */
    int64_t v;
    tzr_scase closed[2] = {{.c = quit, .elem = &q}, {.c = a, .elem = &v}};
    CHECK(tsuzuri_tz_select(closed, 2, 1, &ok) == 1 && ok == 0);
    printf("select: a=%lld b=%lld\n", (long long)hits[0], (long long)hits[1]);
    free(a);
    free(b);
    free(quit);
}

/* 6. select send + recv waiting together, many rounds across routines */
typedef struct { tzr_chan *in, *out, *stop; int64_t n; } relay;
static void relayer(void *env) {
    relay *r = env;
    int64_t got = 0;
    for (;;) {
        int64_t v = 0, s = 0;
        tzr_scase cases[2] = {{.c = r->in, .elem = &v}, {.c = r->stop, .elem = &s}};
        int32_t ok;
        int64_t k = tsuzuri_tz_select(cases, 2, 1, &ok);
        if (k == 1) break;
        CHECK(ok);
        tzr_scase out[1] = {{.c = r->out, .elem = &v, .send = 1}};
        CHECK(tsuzuri_tz_select(out, 1, 1, &ok) == 0 && ok);
        got++;
    }
    CHECK(got == r->n);
}
static void test_relay(void) {
    relay r = {chan_make(8, 0), chan_make(8, 0), chan_make(8, 0), 100000};
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(relayer, &r);
    for (int64_t i = 0; i < r.n; i++) {
        int64_t v = i, w;
        CHECK(tsuzuri_tz_chan_send(r.in, &v, 1) == TZR_OK);
        CHECK(tsuzuri_tz_chan_recv(r.out, &w, 1) == TZR_OK && w == i);
    }
    CHECK(tsuzuri_tz_chan_close(r.stop) == TZR_OK);
    join(scope);
    printf("relay: ok\n");
    free(r.in);
    free(r.out);
    free(r.stop);
}

/* 7. mutex */
static _Atomic uint64_t mutex_word;
static int64_t plain;
static void locker(void *env) {
    (void)env;
    for (int i = 0; i < 20000; i++) {
        CHECK(tsuzuri_tz_mutex_lock(&mutex_word) == 0);
        plain++;
        if (i % 1000 == 0) tsuzuri_tz_yield();
        tsuzuri_tz_mutex_unlock(&mutex_word);
    }
}
static void test_mutex(void) {
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 16; i++) tsuzuri_tz_go(locker, NULL);
    join(scope);
    CHECK(plain == 16 * 20000);
    printf("mutex: ok\n");
}

/* 8. waitgroup */
static _Atomic uint64_t wg;
static void wg_worker(void *env) {
    atomic_fetch_add(&counter, (int64_t)(intptr_t)env);
    CHECK(tsuzuri_tz_waitgroup_add(&wg, -1) == 0);
}
static void wg_waiter(void *env) {
    (void)env;
    CHECK(tsuzuri_tz_waitgroup_wait(&wg) == 0);
    CHECK(atomic_load(&counter) == 1000);
}
static void test_waitgroup(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    CHECK(tsuzuri_tz_waitgroup_add(&wg, 1000) == 0);
    for (int i = 0; i < 3; i++) tsuzuri_tz_go(wg_waiter, NULL);
    for (int i = 0; i < 1000; i++) tsuzuri_tz_go(wg_worker, (void *)(intptr_t)1);
    CHECK(tsuzuri_tz_waitgroup_wait(&wg) == 0);
    CHECK(atomic_load(&counter) == 1000);
    join(scope);
    CHECK(tsuzuri_tz_waitgroup_add(&wg, -1) == 1); /* negative counter */
    atomic_store(&wg, 0);
    printf("waitgroup: ok\n");
}

/* 9. sleep */
static void sleeper(void *env) {
    CHECK(tsuzuri_tz_sleep((int64_t)(intptr_t)env) == 0);
    atomic_fetch_add(&counter, 1);
}
static void test_sleep(void) {
    atomic_store(&counter, 0);
    void *scope = tsuzuri_tz_scope_enter();
    double t = seconds();
    for (int i = 0; i < 2000; i++) tsuzuri_tz_go(sleeper, (void *)(intptr_t)20000000);
    join(scope);
    double e = seconds() - t;
    CHECK(atomic_load(&counter) == 2000);
    CHECK(e >= 0.020);
    printf("sleep: 2000 x 20 ms in %.3f s\n", e);
}

/* 10. external threads (pseudo-Gs) each with their own scope */
static void *external(void *arg) {
    (void)arg;
    test_pingpong(20000, 1);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 1000; i++) tsuzuri_tz_go(incr, (void *)(intptr_t)1);
    join(scope);
    return NULL;
}
static void test_external(void) {
    atomic_store(&counter, 0);
    pthread_t t[4];
    for (int i = 0; i < 4; i++) pthread_create(&t[i], NULL, external, NULL);
    for (int i = 0; i < 4; i++) pthread_join(t[i], NULL);
    CHECK(atomic_load(&counter) == 4000);
    printf("external threads: ok\n");
}

/* 11. deep recursion on a routine stack. The frame size depends on the compiler and the sanitizer,
 * so it is measured and the depth is chosen from a number of bytes. */
static uintptr_t deep_pad[2];
static int64_t deep(int64_t n) {
    volatile char pad[512];
    pad[n % 512] = (char)n; /* an index the compiler cannot know keeps the whole array */
    if (n < 2) deep_pad[n] = (uintptr_t)&pad[0];
    if (n == 0) return 0;
    return deep(n - 1) + 1 + (pad[n % 512] != (char)n);
}
static int64_t depth_for(int64_t bytes) {
    deep(1);
    uintptr_t frame = deep_pad[1] > deep_pad[0] ? deep_pad[1] - deep_pad[0] : deep_pad[0] - deep_pad[1];
    return bytes / (int64_t)(frame ? frame : 512);
}
static void deep_routine(void *env) { CHECK(deep((int64_t)(intptr_t)env) == (int64_t)(intptr_t)env); }
static void test_deep(void) {
    void *scope = tsuzuri_tz_scope_enter();
    int64_t depth = depth_for(900 * 1024); /* 900 KiB of the default 1 MiB stack */
    tsuzuri_tz_go(deep_routine, (void *)(intptr_t)depth);
    join(scope);
    printf("deep recursion: %lld frames: ok\n", (long long)depth);
}

/* 12. deadlock: main waits on a channel nobody sends on (run in a child) */
static void blocked(void *env) {
    int64_t v;
    tsuzuri_tz_chan_recv(env, &v, 1);
}
static const char *self_path;
/* A stack overflow ends a child by SIGSEGV or SIGBUS, or by SIGABRT after the report of stack.c. */
static int overflowed(int status) {
    return WIFSIGNALED(status) && (WTERMSIG(status) == SIGSEGV || WTERMSIG(status) == SIGBUS || WTERMSIG(status) == SIGABRT);
}
/* A fresh process (fork would copy a process whose scheduler threads do not exist in the child). */
static int run_child(const char *name) {
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        execl(self_path, self_path, name, (char *)NULL);
        _exit(127);
    }
    int status;
    waitpid(pid, &status, 0);
    return status;
}
static void deadlock_body(void) {
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(blocked, c);
    tsuzuri_tz_go(blocked, c);
    int64_t v;
    int32_t r = tsuzuri_tz_chan_recv(c, &v, 1);
    _exit(r == TZR_DEADLOCK ? 42 : 1);
    (void)scope;
}
static void deadlock_scope_body(void) {
    tzr_chan *c = chan_make(8, 0);
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(blocked, c);
    _exit(tsuzuri_tz_scope_leave(scope) == 1 ? 43 : 1);
}
static void overflow_routine(void *env) { (void)env; deep(1 << 20); }
static void overflow_body(void) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(overflow_routine, NULL);
    join(scope);
}
/* 14. TSUZURI_TZROUTINE_STACK_KIB sizes the stacks (a child per setting) */
static void stack_body(int64_t kib) {
    void *scope = tsuzuri_tz_scope_enter();
    tsuzuri_tz_go(deep_routine, (void *)(intptr_t)depth_for(kib * 1024));
    join(scope);
}
static int run_child_with(const char *name, const char *stack_kib) {
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        if (stack_kib) setenv("TSUZURI_TZROUTINE_STACK_KIB", stack_kib, 1);
        execl(self_path, self_path, name, (char *)NULL);
        _exit(127);
    }
    int status;
    waitpid(pid, &status, 0);
    return status;
}
static void test_stack_size(void) {
    int s = run_child_with("child-deep-100", "64"); /* 100 KiB of frames on a 64 KiB stack */
    CHECK(overflowed(s));
    s = run_child_with("child-deep-3500", "4096"); /* 3.5 MiB on a 4 MiB stack */
    CHECK(WIFEXITED(s) && WEXITSTATUS(s) == 0);
    s = run_child_with("child-deep-3500", NULL); /* the default 1 MiB is too small for it */
    CHECK(overflowed(s));
    const char *invalid[] = {"0", "abc", "2000000", "64k", ""};
    for (int i = 0; i < 5; i++) {
        s = run_child_with("child-deep-900", invalid[i]); /* each is ignored: the default 1 MiB */
        CHECK(WIFEXITED(s) && WEXITSTATUS(s) == 0);
    }
    printf("stack sizes: ok\n");
}

static void test_failures(void) {
    int s = run_child("child-deadlock");
    CHECK(WIFEXITED(s) && WEXITSTATUS(s) == 42);
    s = run_child("child-deadlock-scope");
    CHECK(WIFEXITED(s) && WEXITSTATUS(s) == 43);
    s = run_child("child-overflow");
    CHECK(overflowed(s));
    printf("deadlock and overflow: ok\n");
}

/* 13. callee-saved floating-point registers survive switches */
static double fp_work(int yield) {
    double a = 1.1, b = 2.2, c = 3.3, d = 4.4, e = 5.5, f = 6.6, g = 7.7, h = 8.8;
    for (int i = 0; i < 2000; i++) {
        a = a * 1.0000001 + b; b = b * 0.9999999 + c; c = c + d * 1e-9; d = d - e * 1e-9;
        e = e * 1.0000002; f = f + g * 1e-7; g = g * 0.9999998; h = h + a * 1e-12;
        if (yield) tsuzuri_tz_yield();
    }
    return a + b + c + d + e + f + g + h;
}
static double fp_expected;
static void fp_routine(void *env) {
    (void)env;
    CHECK(fp_work(1) == fp_expected);
}
static void test_fp(void) {
    fp_expected = fp_work(0);
    void *scope = tsuzuri_tz_scope_enter();
    for (int i = 0; i < 64; i++) tsuzuri_tz_go(fp_routine, NULL);
    join(scope);
    printf("fp registers: ok\n");
}

int main(int argc, char **argv) {
    const char *only = argc > 1 ? argv[1] : NULL;
    self_path = argv[0];
    if (only && strcmp(only, "child-deadlock") == 0) deadlock_body();
    if (only && strcmp(only, "child-deadlock-scope") == 0) deadlock_scope_body();
    if (only && strcmp(only, "child-overflow") == 0) overflow_body();
    if (only && strcmp(only, "child-deep-100") == 0) { stack_body(100); return 0; }
    if (only && strcmp(only, "child-deep-900") == 0) { stack_body(900); return 0; }
    if (only && strcmp(only, "child-deep-3500") == 0) { stack_body(3500); return 0; }
#define RUN(name, call) if (!only || strcmp(only, name) == 0) call
    RUN("spawn", test_spawn());
    RUN("tree", test_tree());
    RUN("pingpong", test_pingpong(200000, 1));
    RUN("pingpong", test_pingpong(200000, 0));
    RUN("mpmc", test_mpmc(0));
    RUN("mpmc", test_mpmc(1));
    RUN("mpmc", test_mpmc(64));
    RUN("select", test_select());
    RUN("relay", test_relay());
    RUN("mutex", test_mutex());
    RUN("waitgroup", test_waitgroup());
    RUN("sleep", test_sleep());
    RUN("external", test_external());
    RUN("deep", test_deep());
    RUN("fp", test_fp());
    RUN("stack_size", test_stack_size());
    RUN("failures", test_failures());
    printf("all passed\n");
    return 0;
}
```

### B-3 `tests/tzroutine_runtime.mjs`（そのまま置く）

```javascript
// The tzroutine runtime on its own (TZ01): builds tests/tzroutine_runtime.c, which includes
// src/runtime/tzroutine.c, at -O0 and -O3 and runs every case with 1, 2 and 4 Ps. TSUZURI_TSAN=1
// adds a ThreadSanitizer build and TSUZURI_ASAN=1 an AddressSanitizer and UBSan build (each case
// once, with 4 Ps). On an Apple silicon Mac with Rosetta it also runs the x86-64 build. A build with
// src/runtime/stack.c and -DTZ_STACK_GUARD checks that a tzroutine's overflow is reported.
//
// usage: node tests/tzroutine_runtime.mjs        (TSUZURI_CLANG names clang, default `clang`)
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-tzroutine-"));
const source = "tests/tzroutine_runtime.c";
const cases = ["spawn", "tree", "pingpong", "mpmc", "select", "relay", "mutex", "waitgroup", "sleep",
  "external", "deep", "fp", "stack_size", "failures"];
// The cases whose children end by a signal, which a sanitizer reports in its own way.
const signalled = new Set(["stack_size", "failures"]);

function execute(program, args, environment = {}, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 300_000, maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, ...environment },
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")} ${JSON.stringify(environment)}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function build(name, flags, extra = []) {
  const binary = join(temporary, name);
  execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", ...flags, "-pthread", source, ...extra, "-o", binary]);
  return binary;
}

let runs = 0;
try {
  for (const optimization of ["-O0", "-O3"]) {
    const binary = build(`tzroutine${optimization}`, [optimization]);
    for (const procs of ["1", "2", "4"]) {
      for (const name of cases) {
        execute(binary, [name], { TSUZURI_MAXPROCS: procs });
        runs++;
      }
    }
  }
  if (process.platform !== "win32") {
    // With stack.c, a tzroutine that overflows its stack is reported like any other thread.
    const guarded = build("tzroutine-guarded", ["-O2", "-DTZ_STACK_GUARD"], ["-x", "c", "src/runtime/stack.c"]);
    const overflow = execute(guarded, ["child-overflow"], {}, false);
    assert.equal(overflow.signal, "SIGABRT", `${overflow.status} ${overflow.signal}\n${overflow.stderr}`);
    assert.match(overflow.stderr, /^trap: stack overflow$/m);
    for (const name of cases) {
      execute(guarded, [name], { TSUZURI_MAXPROCS: "4" });
      runs++;
    }
  }
  if (process.platform === "darwin" && process.arch === "arm64" && spawnSync("arch", ["-x86_64", "/usr/bin/true"]).status === 0) {
    const binary = build("tzroutine-x86_64", ["-arch", "x86_64", "-O2"]);
    for (const name of cases) {
      execute("arch", ["-x86_64", binary, name], { TSUZURI_MAXPROCS: "4" });
      runs++;
    }
  }
  const sanitizers = [];
  if (process.env.TSUZURI_TSAN === "1") sanitizers.push(["tsan", ["-fsanitize=thread"], { TSAN_OPTIONS: "halt_on_error=1" }]);
  if (process.env.TSUZURI_ASAN === "1") sanitizers.push(["asan", ["-fsanitize=address,undefined"], { ASAN_OPTIONS: "detect_leaks=0:detect_stack_use_after_return=1", UBSAN_OPTIONS: "halt_on_error=1" }]);
  for (const [name, flags, environment] of sanitizers) {
    const binary = build(`tzroutine-${name}`, ["-O1", "-g", ...flags]);
    for (const test of cases.filter(test => !signalled.has(test))) {
      execute(binary, [test], { TSUZURI_MAXPROCS: "4", ...environment });
      runs++;
    }
  }
  console.log(`tzroutine runtime: ${runs} runs passed`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
```

### B-4 `src/runtime/stack.c` に足す 2 か所

`diff` の出力（左が HEAD `9a76d8ac`、右が変更後）。1 つ目は `tz_stack_count` の宣言の直後、2 つ目は `tz_stack_fault` の先頭（`address` を求めた直後）。

```diff
31a32,36
> /* TZ01: src/runtime/tzroutine.c sets this to tsuzuri_tz_stack_guard_hit before its first scheduler
>    thread starts, so that a fault in the guard of a tzroutine's own stack is an overflow too. */
> __attribute__((visibility("hidden")))
> int32_t (*volatile tsuzuri_stack_guard_hook)(uintptr_t);
> 
36a42,47
>     int32_t (*guard)(uintptr_t) = tsuzuri_stack_guard_hook;
>     if (guard != 0 && guard(address)) {
>         static const char message[] = "trap: stack overflow\n";
>         (void)!write(2, message, sizeof message - 1);
>         abort();
>     }
```

## C. Windows（Phase 2）の設計

**この節のコードは検査していない**（起票した手元に Windows がない）。Phase 2 の実装者は、CI の `windows.yml` で B-2 のテストを通すまでを Phase 2 の範囲とする（TZ01 の停止条件 S5）。

### C-1 写像

| POSIX の参照実装 | Windows |
| --- | --- |
| `tzr_lock`（`os_unfair_lock`・futex） | `SRWLOCK`（`AcquireSRWLockExclusive`／`ReleaseSRWLockExclusive`。8 バイト、0 で解放状態）。`_Static_assert(sizeof(SRWLOCK) <= 8)` |
| `tzr_note`（pthread の mutex + cond） | `SRWLOCK` + `CONDITION_VARIABLE` + `set`（`SleepConditionVariableSRW` を `set` が立つまで繰り返す） |
| `tzr_nanotime` | `QueryPerformanceCounter` / `QueryPerformanceFrequency`（`async.c` の Windows の分岐と同じ計算。オーバーフローしないよう秒と余りに分ける） |
| `pthread_create`（detach） | `CreateThread` → `CloseHandle`（`task-windows.h` の写像を使ってよい） |
| `pthread_once` | `InitOnceExecuteOnce` |
| `pthread_key_create`（擬似 G の解放） | `FlsAlloc(tzr_pseudo_destroy)` と `FlsSetValue`（Fiber のローカル領域のコールバックは、スレッドの終了でも呼ばれる） |
| `sysconf(_SC_NPROCESSORS_ONLN)` | `GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)` |
| `mmap` + `mprotect` のスタック | `CreateFiberEx(64 * 1024, reserve, FIBER_FLAG_FLOAT_SWITCH, tzr_fiber_start, g)`（OS がガードを置き、必要に応じて commit する） |
| `tzr_switch` | `SwitchToFiber`（M は最初に `ConvertThreadToFiberEx(NULL, FIBER_FLAG_FLOAT_SWITCH)` で g0 を Fiber にする） |
| `madvise` による返却 | しない（Fiber のスタックは OS が管理する。全体の空きリストが満杯なら `DeleteFiber`） |
| タイマーのスレッドの `pthread_cond_timedwait` | `SleepConditionVariableSRW(&wake, &lock, ms, 0)`（`ms` は残りのナノ秒を 1,000,000 で割って切り上げ。`INFINITE` は使わず、上限 1 日で繰り返す。`async.c` と同じ） |
| `stack.c` のフック | なし（`stack.c` は Unix だけ）。Windows のスタック溢れは既存どおり理由なしで終わる |

### C-2 切り替えの 5 か所の書き換え

参照実装が `tzr_switch` を呼ぶのは 5 か所（`tzr_execute` の g0 → G、`tzr_park`・`tsuzuri_tz_yield`・`tzr_gstart` の終わりの G → g0）。Phase 2 は、この 5 か所を次の 2 つのマクロに置き換え、POSIX では今の `tzr_switch` に展開する（POSIX の生成物は変わらない）。

```c
#if defined(_WIN32)
/* g0 -> G and G -> g0. The fiber saves its own registers; sp is unused. */
#define TZR_TO_G(m, g) SwitchToFiber((g)->fiber)
#define TZR_TO_G0(m, g) SwitchToFiber((m)->g0_fiber)
#else
#define TZR_TO_G(m, g) tzr_switch(&(m)->g0_sp, (g)->sp)
#define TZR_TO_G0(m, g) tzr_switch(&(g)->sp, (m)->g0_sp)
#endif
```

G の Fiber は、終わった G を再利用するため、開始関数の中で繰り返す:

```c
static VOID CALLBACK tzr_fiber_start(LPVOID arg) {
    tzr_g *g = arg;
    for (;;) {
        g->code(g->env);
        tzr_m *m = tzr_this_m(); /* R2: the body may have moved this G to another M */
        m->after = tzr_after_exit;
        TZR_TO_G0(m, g); /* returns only when tsuzuri_tz_go reuses this G with a new code */
    }
}
```

- `tsuzuri_tz_go` は、新しい G なら `CreateFiberEx`、再利用なら `code`・`env` を入れ替えるだけ（Fiber は前回の `TZR_TO_G0` の中で止まっていて、次の `TZR_TO_G` で `for` の先頭へ戻る）。POSIX の `tzr_initial_frame` は Windows では使わない。
- M の g0: `tzr_mstart` の先頭で `m->g0_fiber = ConvertThreadToFiberEx(NULL, FIBER_FLAG_FLOAT_SWITCH)`。失敗は `tzr_fatal`。
- 擬似 G は Fiber にしない（自分のスレッドを止めて待つだけ。`ConvertThreadToFiber` をホストのスレッドで呼ばない。ホストが Fiber を使っていても壊さない）。
- `FIBER_FLAG_FLOAT_SWITCH` は浮動小数点の状態（MXCSR・x87）を Fiber ごとに保つ（POSIX の `fp` のテストと同じ性質）。
- x64 の Windows の ABI では XMM6〜15 も callee-saved だが、Fiber が保存するので考えなくてよい。

## D. アルゴリズム

### D-1 待つ・起こす（park／ready）

G が待つ手順（`tzr_park(unlock, arg, reason)`、G 側）:

1. 待ちの条件をロック（例: チャンネルの `lock`）の下で確かめ、sudog などを待ち行列に入れる。
2. `g->status = WAITING`（この時点で、ロックを持っているので誰も G を見つけられない）。
3. `m->after = tzr_after_park`、`m->after_unlock = unlock`、`m->after_arg = arg`。
4. g0 へ切り替える（`tzr_switch(&g->sp, m->g0_sp)`）。**G の文脈はここで保存しきる。**
5. g0 側（`tzr_execute` の切り替えの直後）: `after(m, prev)` → `unlock(arg)`。ここで初めて、起こす側がロックを取って G を見つけられる。
6. 起こす側（`tzr_ready(g)`）: `WAITING` → `RUNNABLE` の compare-exchange、自分の P の `runnext` か全体へ入れて `tzr_wakep()`。
7. どこかの M の `tzr_execute` が G へ切り替える。G は 4 の切り替えの直後から続く（別の OS スレッドかもしれない。R2）。

擬似 G は 2 の代わりに `tzr_sched.lock` の下で `WAITING`・`npseudo_running--`・`npseudo_parked++`、続けて `unlock(arg)`、`tzr_checkdead()`、`tzr_note_sleep`。起こす側は同じロックの下で逆に数えて `tzr_note_wake`（`note` は 1 回分を覚えるので、`note_sleep` の前に起こされても失わない）。

### D-2 新しい G の初期の frame

- aarch64: `tzr_switch` が戻すのは、`sp` から 160 バイトの保存領域（d8〜d15、x19〜x28、x29、x30）。初期の frame は、スタックの上端 `top` から 160 バイト下に置き、x30 の位置（`frame[19]`）に `tzr_gstart`、x29（`frame[18]`）に 0（frame pointer の鎖の終わり。B10 のバックトレースが止まる）。`ret` のあと `sp == top`（16 の倍数）。
- x86-64: 保存領域は MXCSR と x87 の制御語（8 バイト）と 6 つのレジスタ（48 バイト）、その上に戻り先。戻り先は `top - 16`、偽の戻り先 0 を `top - 8` に置く。`ret` のあと `rsp == top - 8`（呼ばれた直後と同じ `rsp ≡ 8 (mod 16)`）。MXCSR `0x1F80`、x87 `0x037F`。

### D-3 デッドロックの判定

```
tzr_checkdead()（tzr_sched.lock を持って呼ぶ）:
  if deadlocked: return
  running = mcount - nmidle + npseudo_running      # 眠っていない M と、走っている擬似 G
  if running > 0: return
  if runq_size > 0 or いずれかの P の環が空でない: return
  if ntimers > 0: return                            # タイマーが誰かを起こす
  if ngoroutines == 0 and npseudo_parked == 0: return  # 誰も生きていない
  （TZ08 で足す: if nmsys > 0 or netpoll の待ち > 0: return）
  deadlocked = 1; 報告; 待っている擬似 G をすべて doomed で起こす; 1 つもなければ __builtin_trap()
```

- 呼ぶのは 2 か所: M が P を返して眠る直前（`tzr_stopm`）と、擬似 G が待つ直前（`tzr_park`）。最後の 1 人が眠る瞬間に必ずどちらかが走る。
- 「眠る途中の M」（P を返したが `nmidle++` の前）は `running` に数えられるので、判定は保守的（偽の判定を出さない側）に倒れる。
- タイマーのスレッドは、起こしてから `ntimers--` する（逆にすると、起こす前に判定が `ntimers == 0` を見て誤る）。

### D-4 チャンネル（`tzr_chan`）

| 状態 | `send` | `recv` |
| --- | --- | --- |
| 閉じている | `TZR_CLOSED`（トラップ） | 残りがあれば取る。なければ `TZR_CLOSED`（`None`） |
| 相手が待っている | 待つ受け手のスロットへ直接書いて起こす | 容量 0: 待つ送り手の値を直接取る。容量あり（満杯）: 先頭を取り、待つ送り手の値を末尾へ入れて起こす |
| バッファに空き／要素あり | 末尾へ入れる | 先頭を取る |
| どれでもない | `block` なら sudog を `sendq` に入れて待つ。起きて `success == 0` なら閉じられた（`TZR_CLOSED`） | `block` なら sudog を `recvq` に入れて待つ。起きて `success == 0` なら閉じられた |

- 要素はバイト列としてコピーする（所有権は生成コードが管理する。送った値の所有権はチャンネルへ、受けた値は受け手へ移る）。容量 0 の場合も `elem_size` のコピーは 1 回。
- 参照数 `refs`: `share` で増やし、`release` で減らす。0 になった後のブロックには、もう誰も触れない（待っている G は自分のハンドルを借りているので、その間は 0 にならない）。

### D-5 `select`

```
tsuzuri_tz_select(cases, n, block, ok):
  pollorder = 一様乱数の置換（Fisher–Yates）
  lockorder = チャンネルのアドレスの昇順（挿入整列）
  全チャンネルをロック（lockorder の順。同じチャンネルは 1 回）
  1 段目: pollorder の順に準備のできた腕を探し、見つかればその場で実行して全解放し、添字を返す
  block == 0 なら全解放して -1
  2 段目: 全腕の sudog（is_select = 1）を各待ち行列へ。select_done = 0。tzr_park（全解放は g0 で）
  起こした者: tzr_dequeue が select_done を 0 → 1 にできた sudog だけを勝ちにし、g->param に入れる
  3 段目: 全ロック、勝った腕以外の sudog を tzr_unlink、全解放、勝った腕の添字と success を返す
```

- 同じチャンネルが 2 つの腕にあってもよい（ロックは 1 回、sudog は腕ごと）。
- `tzr_dequeue` が勝てなかった sudog（ほかの腕がすでに勝った G）は行列から外して捨てる。3 段目の `tzr_unlink` は、外れた sudog に対して何もしない。

### D-6 spinning の手順（取りこぼしの防止）

- `tzr_wakep`: 空いた P があり、`nmspinning` が 0 のときだけ、0 → 1 の compare-exchange に勝った者が `tzr_startm(NULL, 1)`。
- spinning の M が G を見つけたら `nmspinning--`。それで 0 になったら `tzr_wakep()`（次の spinning を起こし、仕事が続く限り探す者を 1 人保つ）。
- spinning の M が何も見つけられなかったら: P を返し、`nmspinning--`、**そのあと**すべての P の環と全体をもう一度見る。何かあれば空いた P を取り直して spinning に戻る。こうしないと、「生成した側が `nmspinning != 0` を見て `wakep` をやめた」直後に spinning の M が眠り、誰もその G を走らせない。

### D-7 スタック

- 1 つの G = 1 つの mapping（ガード 128 KiB + 本体）。`mmap(PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON)` のあと下端を `mprotect(PROT_NONE)`。Linux の `MAP_GROWSDOWN` と `MAP_NORESERVE` は使わない（前者はガードの意味が変わり、後者は不要: 触れていないページは commit されない）。
- 予約の大きさは、生成コードの再帰の深さに合わせて選ぶ: 1 MiB は、1 段 512 バイトなら約 2,000 段。より深い再帰は `TSUZURI_TZROUTINE_STACK_KIB` で増やす（TZ02 の文書）。
- 空きリストの G のスタックには前回の内容が残る。G の開始は初期の frame を書き直すだけで、内容を消さない（スタックの未初期化の読み出しは生成コードにない）。
- 溢れの報告: `stack.c` の handler は sigaltstack の上で走り、`tsuzuri_tz_stack_guard_hit` は `tzr_tls_m` を直接読む（この関数だけは TLS を直接読んでよい: 信号を受けたスレッドの上で、切り替えをまたがないため）。ガードの範囲は `[stack, stack + 128 KiB + 4 KiB)`。
