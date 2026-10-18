# B10 付録: 実測・参照実装・詳細設計

[B10-trap-backtrace-and-domains.md](B10-trap-backtrace-and-domains.md) の付録。A 実測、B 参照実装の C と JS、C 安全性の議論、D `E1038`、E `std/Domain.tz` と `TrapKind`、F 後始末の表、G Phase 2 の codegen・経路・driver、H Phase 2 の変更表と手順。

## A. 実測（macOS arm64、Apple M1 Max、macOS 27.0.1、2026-10-18）

道具: Apple clang 21（`clang`）、Homebrew LLVM 23.1.1（`opt`・`clang`・`llvm-symbolizer`）、Node v20.19.6、rustc 1.98.1、`vsc/toolchain/zig/zig` 0.16.0。HEAD は `9dd0fbbc`（`a2313d19` との差は `docs/benchmarks.md` の 41 行の削除だけ）。Apple の `clang` は Homebrew の `opt` が書いた IR を読めない（同じ LLVM のものを使う）。

### A-1 chain（`tests/fixtures/trap_backtrace/Main.tz` と同じ。本文の「現状」）

`tsuzuri build <dir> --emit exe --trap-info -O0 -o exe-O0` の局所 symbol（`nm -n exe-O0`）:

```text
0000000100000548 t _tz.fn.Main.level3
00000001000005bc t _tz.fn.Main.level2
00000001000005f4 t _tz.fn.Main.level1
000000010000062c t _tz.fn.Main.main
0000000100000770 T _main
```

`-O3` の `nm -n exe-O3` は `_main`（`T`）と `_main.cold.1`（`t`）だけ。4 つの関数は `tsuzuri_main` へ inline され、`tsuzuri_main` も `_main` へ inline される。

### A-2 `backtrace()` と `dladdr`（frame pointer の有無）

`tz.trap.report` の中から呼ぶ小さな C（`tz_dump_backtrace`: `backtrace()` と `dladdr`）を、IR の `@tz.trap.report` の `done:` に差し込んだ実行ファイルで測った。

| 構成 | 結果 |
| --- | --- |
| 生成した関数に frame pointer の属性なし（`-O0`） | 3 フレーム: `tz_dump_backtrace`・`tz.trap.report`・`start`（dyld）。`tz.fn.Main.*` は 0 |
| 全 `define` に `"frame-pointer"="all"`（`-O0`） | 9 フレーム: `tz_dump_backtrace`・`tz.trap.report`・`tz.fn.Main.level3`・`level2`・`level1`・`tz.fn.Main.main`・`tsuzuri_main`・`main`（C）・`start` |
| 同（`-O3`） | 4 フレーム: `tz_dump_backtrace`・`tz.trap.report`・`tsuzuri_main`・`start` |
| module flag `!{i32 7, !"frame-pointer", i32 2}` と `clang -fno-omit-frame-pointer`（属性なし） | 効かない（3 フレームのまま） |

`dladdr` は macOS で局所 symbol の名前を返した（`sname=tz.fn.Main.level3`）。Linux の glibc の `dladdr` が動的 symbol しか引かないことは既知の挙動で、このマシンでは測っていない（D4）。

### A-3 報告の行と symbolizer

`src/runtime/backtrace.c`（B-1）を連結し、IR に A-5 の変更を入れた実行ファイルを `TSUZURI_TRAP_REPORT=report.txt` で走らせた（ASLR のため `<slide>` は実行ごとに違う）:

```text
$ TSUZURI_TRAP_REPORT=report.txt ./rep-exe
trap: integer division by zero at ./Main.tz:2:15
$ cat report.txt
tz-trap/1 2 0x2514000 0 0x1025144e4 0x10251456c 0x1025145c0 0x10251461c 0x102514774 0x102518d24 0x180d23e80
```

`<slide>` を引くと link 時のアドレスになり、`nm -n rep-exe` の `_tz.fn.Main.level3`（`0x1000004b0`）・`level2`（`0x100000540`）・`level1`（`0x100000594`）・`main`（`0x1000005e8`）・`_tsuzuri_main`（`0x100000748`）・`_main`（`0x100004d10`）のそれぞれの内側に入る（`0x1025144e4 − 0x2514000 = 0x1000004e4`）。symbolizer へ渡すのは `address − slide − 1`:

```text
$ llvm-symbolizer --obj=rep-exe --output-style=JSON --inlining --functions=linkage 0x1000004e3 0x10000056b 0x1000005bf 0x10000061b 0x100000773 0x100004d23 0x17e80fe7f
[{"Address":"0x1000004e3","ModuleName":"rep-exe","Symbol":[{"Column":0,"Discriminator":0,"FileName":"","FunctionName":"tz.fn.Main.level3","Line":0,"StartAddress":"0x100000548","StartFileName":"","StartLine":0}]},{"Address":"0x10000056b",…"FunctionName":"tz.fn.Main.level2"…},…]
```

関数名は順に `tz.fn.Main.level3`・`tz.fn.Main.level2`・`tz.fn.Main.level1`・`tz.fn.Main.main`・`tsuzuri_main`・`main`（C）・空（dyld の `start`）。出力は 1 つの JSON 配列で、要素は `Address`・`ModuleName`・`Symbol[]`。`Symbol` は `Column`・`Discriminator`・`FileName`・`FunctionName`・`Line`・`StartAddress`・`StartFileName`・`StartLine`。

### A-4 `-g`

`-g -O0` の実行ファイルは `<exe>.dwarf`（`dsymutil --flat`）を伴い、その `.dwarf` を `--obj` に渡すと `FileName` と `Line`・`Column` が入る。上の chain の 6 つの戻りアドレス（`FunctionName`, `FileName`, `Line`, `Column`）:

```text
0x1000004e3  tz.fn.Main.level3  ./Main.tz  0   15      ← 最も内側は Line 0（位置は主スパンが示すので付けない）
0x10000056b  tz.fn.Main.level2  ./Main.tz  5   15
0x1000005bf  tz.fn.Main.level1  ./Main.tz  8   15
0x10000061b  tz.fn.Main.main    ./Main.tz  11  19
0x100000773  tsuzuri_main       ./Main.tz  11  4       ← 名前の表で捨てる
```

`-g` を付けない実行ファイルは `FileName` が空で `Line` が 0。`-g -O3` では、`--inlining` を付けても `tsuzuri_main` の 1 つしか返らない（`Line 0`）。frame は空になる。

### A-5 frame pointer の属性の入れ方（検証済みの変換）

`tsuzuri build <dir> --emit llvm --trap-info -O0` の IR に次の変換を施し、`clang -O0 chain-fp.ll backtrace.c host.c -lm` で組んだものが A-3・A-4 の結果を出した。`-g` の IR は ` !dbg !N` の直前に属性が入る（`define internal i64 @tz.fn.Main.level3(i64 noundef %arg0, i32 %tz.context) nounwind "frame-pointer"="all" !dbg !333 {`）。

```python
import sys
source, target = sys.argv[1], sys.argv[2]
out = []
for line in open(source).read().split("\n"):
    if line.startswith("define "):
        index = line.index(" !dbg ") if " !dbg " in line else line.rindex(" {")
        line = line[:index] + ' "frame-pointer"="all"' + line[index:]
    out.append(line)
text = "\n".join(out)
head = "define internal void @tz.trap.report(i32 %site) cold nounwind"
text = text.replace(head, head + " noinline", 1)
marker = "done:\n  ret void\n}\ndefine internal void @tz.trap.write"
text = text.replace(marker, "done:\n  notail call void @tsuzuri_trap_backtrace(i32 %site)\n  ret void\n}\ndefine internal void @tz.trap.write", 1)
text += "\ndeclare void @tsuzuri_trap_backtrace(i32)\n"
open(target, "w").write(text)
```

Rust の `with_frame_pointers`・`instrument_with` は、この変換と同じ結果の IR を出す（`@tz.trap.report` は `noinline`、`done:` の先頭に `notail call`）。

### A-6 WASM（Node 20.19.6 の V8）

`tsuzuri build … --target wasm32 --trap-info -g -O0` の `error.stack`（`Error.stackTraceLimit = 33`）。名前は V8 が module 名を前に付ける（`artifact.`）。`-g` なしは名前がない:

```text
at artifact.tz.fn.Main.level3 (wasm://wasm/…:wasm-function[0]:0xa9)        # -g
at wasm://wasm/…:wasm-function[0]:0xa9                                       # -g なし
```

B-3 の JS は、`-g -O0` で `Main.level3`・`Main.level2`・`Main.level1`・`Main.entry`・`entry`（`tz_entry`）、`-g なし` で `index` と `offset` だけの 5 frame、`-g -O3` で `tz_entry.cold.1` と `tz_entry` が 1 つ（`entry`）にまとまることを確かめた。

### A-7 Linux・Windows

このマシンでは実行できなかった（Docker の daemon は動いているが、コンテナの起動が終わらなかった）。代わりに、同梱の `vsc/toolchain/zig/zig cc -target <t> -std=c11 -Wall -Wextra -c` で、`backtrace.c`（B-1）が `aarch64-linux-gnu`・`x86_64-linux-gnu`・`aarch64-linux-musl`・`x86_64-linux-musl` で、`trap.c`（B-2 の後）が上の 4 つと `x86_64-windows-gnu`・`aarch64-windows-gnu` で、警告なしにコンパイルできることを確かめた。**Linux での実行（frame pointer の鎖、`dl_iterate_phdr` の slide）は未測定**で、実装者が S2 の条件で確かめる。

### A-8 決定性

`examples/hello`・`currying`・`functional` を、`identity` の 8 種（exe・object・object+trap-info・llvm・llvm+trap-info・wasm+trap-info・llvm+return・object+return）で 2 回出力して `diff -r` が空（39 ファイル）。object と exe はこのマシンで byte 単位で同じ。`identity` は `target/tz-b10/identity.sh` に次のとおり置く（`sh target/tz-b10/identity.sh <出力先>`）:

```sh
identity() {
  out=$1; mkdir -p "$out"
  for p in hello currying functional; do
    build="target/release/tsuzuri build examples/$p --no-cache"
    $build -o "$out/$p.exe" || return 1
    $build --emit object -o "$out/$p.o" || return 1
    $build --emit object --trap-info -o "$out/$p.trap.o" || return 1
    $build --emit llvm -o "$out/$p.ll" || return 1
    $build --emit llvm --trap-info -o "$out/$p.trap.ll" || return 1
    $build --target wasm32 --trap-info -o "$out/$p.wasm" || return 1
    $build --emit llvm --trap-mode return -o "$out/$p.ret.ll" || return 1
    $build --emit object --trap-mode return -o "$out/$p.ret.o" || return 1
  done
}
identity "$1"
```

### A-9 費用（M1 Max、`-O1`、1 スレッド）

| 測ったもの | 結果 |
| --- | --- |
| 追跡確保の確保と解放の組（境界の中。48 byte を 256 個ずつ回す） | 約 55〜58 ns |
| 同（境界の外。`tsuzuri_tracked_*` を呼ぶが frame がない） | 約 22 ns |
| 素の `malloc`/`free` | 約 19〜21 ns |
| 空のドメイン 1 回（`setjmp`） | 約 370 ns。即トラップ 1 回は約 740 ns |
| 同（`sigsetjmp(env, 0)`） | 約 40 ns。即トラップ 1 回は約 82 ns |
| 子プロセス方式の下限（`fork` して即 `_exit`、`waitpid`。5,000 回の平均、2 回測定） | 約 2.7〜3.6 ms（`exec`・値の受け渡しは含まない） |

## B. 参照実装

### B-1 `src/runtime/backtrace.c`（そのまま置く。`-std=c11 -Wall -Wextra -Werror` で警告なし）

```c
/* Crash report of a trap (B10 Phase 1): the frames that led to it, for native executables built with
   --trap-info (build, run). Linked into executables only, never into objects, so a host that links an
   object keeps its own process. The generated @tz.trap.report calls tsuzuri_trap_backtrace(site) after
   it wrote the site's message. Nothing is written unless TSUZURI_TRAP_REPORT names a file; `tsuzuri run`
   sets it. The code walks the chain of frame pointers: every function that Tsuzuri generates keeps one in
   such a build ("frame-pointer"="all"), and a frame record is the saved frame pointer followed by the
   return address on arm64 and x86-64. Every record is checked to be mapped before it is read. */
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#if defined(__APPLE__)
#include <mach-o/dyld.h>
#else
#include <link.h>
#endif

enum { TZ_BACKTRACE_FRAMES = 32 };

#if !defined(__APPLE__)
static int tz_backtrace_first_image(struct dl_phdr_info *info, size_t size, void *data) {
    (void)size;
    *(uintptr_t *)data = (uintptr_t)info->dlpi_addr;
    return 1;
}
#endif

/* The distance between where the executable is loaded and where it was linked. */
static uintptr_t tz_backtrace_slide(void) {
#if defined(__APPLE__)
    return (uintptr_t)_dyld_get_image_vmaddr_slide(0);
#else
    uintptr_t slide = 0;
    dl_iterate_phdr(tz_backtrace_first_image, &slide);
    return slide;
#endif
}

/* Whether the `length` bytes at `address` are mapped, so that reading them cannot fault. */
static int tz_backtrace_readable(uintptr_t address, size_t length) {
    long page = sysconf(_SC_PAGESIZE);
    if (page <= 0) return 0;
    uintptr_t mask = (uintptr_t)page - 1;
    uintptr_t start = address & ~mask;
    uintptr_t end = (address + length + mask) & ~mask;
    char vector[2];
    if (end <= start || (end - start) / (uintptr_t)page > sizeof vector) return 0;
    return mincore((void *)start, (size_t)(end - start), (void *)vector) == 0;
}

/* Appends one line `tz-trap/1 <site> <slide> <truncated> <address>...` (hexadecimal with 0x, the
   innermost frame first) to the file of TSUZURI_TRAP_REPORT. */
__attribute__((noinline, visibility("hidden")))
void tsuzuri_trap_backtrace(uint32_t site) {
    const char *path = getenv("TSUZURI_TRAP_REPORT");
    if (!path || !*path) return;
    uintptr_t frames[TZ_BACKTRACE_FRAMES];
    unsigned count = 0;
    int truncated = 0;
    int skipped = 0;
    uintptr_t *record = (uintptr_t *)__builtin_frame_address(0);
    while (record && ((uintptr_t)record & (sizeof(uintptr_t) - 1)) == 0
           && tz_backtrace_readable((uintptr_t)record, 2 * sizeof(uintptr_t))) {
        uintptr_t next = record[0];
        uintptr_t address = record[1];
        if (address == 0) break;
        /* The first return address is into @tz.trap.report itself. */
        if (!skipped) skipped = 1;
        else if (count < TZ_BACKTRACE_FRAMES) frames[count++] = address;
        else {
            truncated = 1;
            break;
        }
        /* A stack grows down, so the record of a caller lies above its callee's. */
        if (next <= (uintptr_t)record) break;
        record = (uintptr_t *)next;
    }
    char line[1024];
    int length = snprintf(line, sizeof line, "tz-trap/1 %u 0x%lx %d", (unsigned)site,
                          (unsigned long)tz_backtrace_slide(), truncated);
    for (unsigned index = 0; index < count && length > 0 && length < (int)sizeof line - 24; ++index) {
        length += snprintf(line + length, sizeof line - (size_t)length, " 0x%lx", (unsigned long)frames[index]);
    }
    if (length <= 0 || length >= (int)sizeof line - 1) return;
    line[length++] = '\n';
    int descriptor = open(path, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (descriptor < 0) return;
    (void)!write(descriptor, line, (size_t)length);
    (void)close(descriptor);
}
```

### B-2 `src/runtime/trap.c` の差分と `tests/domain_runtime.c`

`src/runtime/trap.c`（HEAD は 328 行）への差分（`diff -u`）。当てたあと `domain_runtime.c`（18,041 checks）と E14 の `tests/trap_boundary_runtime.c`（26,897 checks）の両方が `-O0`・`-O3`・ASan+UBSan・TSan で通る（`-std=c11 -Wall -Wextra -Werror`、このマシンで確認）。

```diff
@@ -3,13 +3,30 @@
    allocation made since the call is freed, and the host gets status 1. Tsuzuri code never
    unwinds, so no destructor is skipped; the runtime releases its own locks before any callback
    runs, and the one lock that Tsuzuri code holds while it runs, Mutex.with_lock's, is released
-   by the boundary through `tsuzuri_sync_hooks` (F10). */
+   by the boundary through `tsuzuri_sync_hooks` (F10). A domain (B10, `Domain.run`) is the same
+   frame for the call of a pure closure; it nests inside a boundary, an item, or another domain. */
+#if defined(__linux__) && !defined(_GNU_SOURCE)
+#define _GNU_SOURCE
+#endif
 #include <pthread.h>
 #include <setjmp.h>
 #include <stddef.h>
 #include <stdint.h>
 #include <stdlib.h>
 #include <string.h>
+
+/* A trap leaves a boundary or a domain by a jump that must not touch the signal mask: the setjmp
+   of macOS saves it with a system call (370 ns for an empty domain against 40 ns, measured), and
+   Windows has no sigsetjmp. */
+#if defined(_WIN32)
+typedef jmp_buf tz_jump;
+#define TZ_SETJMP(env) setjmp(env)
+#define TZ_LONGJMP(env) longjmp(env, 1)
+#else
+typedef sigjmp_buf tz_jump;
+#define TZ_SETJMP(env) sigsetjmp(env, 0)
+#define TZ_LONGJMP(env) siglongjmp(env, 1)
+#endif
 
 #ifndef TZ_TRAP_API
 #if defined(_WIN32)
@@ -85,7 +102,7 @@
 };
 
 struct tz_frame {
-    jmp_buf env;
+    tz_jump env;
     struct tz_boundary *owner;
     struct tz_frame *outer;
     volatile uint32_t site;
@@ -100,7 +117,7 @@
     if (!frame) return;
     frame->site = site;
     frame->kind = kind;
-    longjmp(frame->env, 1);
+    TZ_LONGJMP(frame->env);
 }
 
 /* The boundary a Task.parallel group submitted from this thread belongs to, or NULL. */
@@ -138,7 +155,7 @@
     frame.kind = 0;
     tz_frame = &frame;
     int32_t trapped = 0;
-    if (setjmp(frame.env) == 0) {
+    if (TZ_SETJMP(frame.env) == 0) {
         if (run_result) *failed = run_result(context, index);
         else run(context, index);
     } else {
@@ -179,7 +196,7 @@
     frame.kind = 0;
     tz_frame = &frame;
     int32_t trapped = 0;
-    if (setjmp(frame.env) == 0) thunk(argument);
+    if (TZ_SETJMP(frame.env) == 0) thunk(argument);
     else trapped = 1;
     tz_frame = NULL;
     if (trapped && tsuzuri_sync_hooks.abandon) tsuzuri_sync_hooks.abandon();
@@ -252,6 +269,63 @@
     if (block->next) block->next->prev = block->prev;
     --owner->count;
     return block;
+}
+
+/* B10: what a finished domain leaves behind (the result of its closure) becomes the property of
+   the boundary or domain that encloses it, so that a later trap there frees it; with none around,
+   the blocks are plain heap blocks of the program. */
+static void tz_domain_hand_over(struct tz_boundary *domain, struct tz_boundary *outer) {
+    struct tz_block *block = domain->blocks;
+    domain->blocks = NULL;
+    while (block) {
+        struct tz_block *next = block->next;
+        if (outer) {
+            pthread_mutex_lock(&outer->lock);
+            struct tz_block *moved = tz_block_track(outer, block->pointer);
+            pthread_mutex_unlock(&outer->lock);
+            if (!moved) abort();
+        }
+        TZ_TRAP_FREE(block);
+        block = next;
+    }
+    TZ_TRAP_FREE(domain->buckets);
+    domain->buckets = NULL;
+}
+
+/* B10: 0: the closure finished (the blocks that are left belong to the caller), 1: it trapped
+   (`trap` is filled and every block allocated inside is freed). Unlike tsuzuri_boundary_run it
+   nests: a domain may run inside a boundary, a worker item, or another domain, and a trap goes to
+   the innermost frame. It never calls tsuzuri_sync_hooks.abandon: a lock that the caller holds
+   is not the domain's to release (a pure closure takes none). */
+TZ_TRAP_API
+int32_t tsuzuri_domain_run(void (*thunk)(void *), void *argument, tsuzuri_trap_info *trap) {
+    /* Off the stack, for the reason given in tsuzuri_boundary_run. */
+    struct tz_boundary *volatile domain = TZ_TRAP_STATE_MALLOC(sizeof *domain);
+    if (!domain) abort();
+    pthread_mutex_init(&domain->lock, NULL);
+    domain->blocks = NULL;
+    domain->buckets = NULL;
+    domain->capacity = 0;
+    domain->count = 0;
+    struct tz_frame frame;
+    frame.owner = domain;
+    frame.outer = tz_frame;
+    frame.site = 0;
+    frame.kind = 0;
+    tz_frame = &frame;
+    int32_t trapped = 0;
+    if (TZ_SETJMP(frame.env) == 0) thunk(argument);
+    else trapped = 1;
+    tz_frame = frame.outer;
+    if (trapped) tz_boundary_release(domain, 1);
+    else tz_domain_hand_over(domain, frame.outer ? frame.outer->owner : NULL);
+    pthread_mutex_destroy(&domain->lock);
+    TZ_TRAP_STATE_FREE(domain);
+    if (trapped && trap) {
+        trap->site = frame.site;
+        trap->kind = frame.kind;
+    }
+    return trapped;
 }
 
 TZ_TRAP_API
@@ -271,16 +345,22 @@
     return pointer;
 }
 
-TZ_TRAP_API
-void tsuzuri_tracked_free(void *pointer) {
-    if (!pointer) return;
-    struct tz_boundary *owner = tz_frame ? tz_frame->owner : NULL;
-    if (owner) {
+/* B10: the tracking block of `pointer` in the innermost frame that tracks it, removed from its owner. */
+static struct tz_block *tz_untrack_chain(void *pointer) {
+    for (struct tz_frame *frame = tz_frame; frame; frame = frame->outer) {
+        struct tz_boundary *owner = frame->owner;
         pthread_mutex_lock(&owner->lock);
         struct tz_block *block = tz_block_untrack(owner, pointer);
         pthread_mutex_unlock(&owner->lock);
-        TZ_TRAP_FREE(block);
+        if (block) return block;
     }
+    return NULL;
+}
+
+TZ_TRAP_API
+void tsuzuri_tracked_free(void *pointer) {
+    if (!pointer) return;
+    if (tz_frame) TZ_TRAP_FREE(tz_untrack_chain(pointer));
     TZ_TRAP_FREE(pointer);
 }
 
@@ -293,6 +373,16 @@
     }
     struct tz_boundary *owner = tz_frame ? tz_frame->owner : NULL;
     if (!owner) return TZ_TRAP_REALLOC(pointer, size);
+    /* B10: the block belongs to the innermost owner that tracks it; one that nobody tracks joins the innermost. */
+    for (struct tz_frame *frame = tz_frame; frame; frame = frame->outer) {
+        pthread_mutex_lock(&frame->owner->lock);
+        int tracked = frame->owner->capacity != 0 && *tz_block_slot(frame->owner, pointer) != NULL;
+        pthread_mutex_unlock(&frame->owner->lock);
+        if (tracked) {
+            owner = frame->owner;
+            break;
+        }
+    }
     pthread_mutex_lock(&owner->lock);
     struct tz_block *block = owner->capacity ? *tz_block_slot(owner, pointer) : NULL;
     int added = block == NULL;
```

`tests/domain_runtime.c`（`tests/trap_return.mjs` が `unit-O0`・`unit-O3`・`unit-asan`・`unit-tsan` と同じ 4 つのフラグで組み、`/^domain runtime: \d+ checks passed\n$/` を照合する。トポロジーの引数はない）:

```c
/* Unit tests of the failure domain (B10 Phase 2): src/runtime/trap.c with every allocation counted, so
   each case ends with the number of live blocks. Usage: domain_runtime */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#undef NDEBUG
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static atomic_long live;

static void *counted_malloc(size_t size) {
    void *pointer = malloc(size);
    if (pointer) atomic_fetch_add(&live, 1);
    return pointer;
}

static void counted_free(void *pointer) {
    if (!pointer) return;
    atomic_fetch_sub(&live, 1);
    free(pointer);
}

static void *counted_realloc(void *pointer, size_t size) {
    return realloc(pointer, size);
}

#define TZ_TRAP_MALLOC counted_malloc
#define TZ_TRAP_FREE counted_free
#define TZ_TRAP_REALLOC counted_realloc
#define TZ_TRAP_API
#include "../src/runtime/trap.c"

static atomic_uint checks;
#define CHECK(condition) do { assert(condition); atomic_fetch_add(&checks, 1); } while (0)

struct work {
    long count;
    void *result;
    void *outer_block;
    int trap;
};

/* Allocates `count` blocks, frees all but the first, which stays as the result. */
static void survivor_domain(void *argument) {
    struct work *work = argument;
    for (long index = 0; index < work->count; ++index) {
        unsigned char *block = tsuzuri_tracked_malloc(16 + (size_t)index);
        assert(block && (uintptr_t)block % 16 == 0);
        memset(block, 0x5a, 16 + (size_t)index);
        if (index == 0) work->result = block;
        else tsuzuri_tracked_free(block);
    }
}

/* Allocates, frees a third, grows one, then traps with nothing released. */
static void trapping_domain(void *argument) {
    struct work *work = argument;
    for (long index = 0; index < work->count; ++index) {
        void *block = tsuzuri_tracked_malloc(32 + (size_t)index);
        assert(block);
        if (index % 3 == 0) tsuzuri_tracked_free(block);
    }
    unsigned char *grown = tsuzuri_tracked_malloc(8);
    memset(grown, 0x33, 8);
    grown = tsuzuri_tracked_realloc(grown, 4096);
    assert(grown && grown[7] == 0x33);
    tsuzuri_trap_raise(42, 3);
    assert(!"a trap inside a domain must not return");
}

static void host_boundary(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(64);
    struct work inner = { .count = 50 };
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(survivor_domain, &inner, &trap) == 0);
    CHECK(tsuzuri_domain_run(trapping_domain, &inner, &trap) == 1);
    CHECK(trap.site == 42 && trap.kind == 3);
    /* The block of the host and the survivor of the first domain are intact (a sanitizer checks). */
    memset(work->outer_block, 1, 64);
    memset(inner.result, 2, 16);
    if (work->trap) tsuzuri_trap_raise(7, 1);
    tsuzuri_tracked_free(inner.result);
}

static void free_outer_in_domain(void *argument) {
    struct work *work = argument;
    tsuzuri_tracked_free(work->outer_block);
    work->outer_block = NULL;
    tsuzuri_trap_raise(9, 2);
}

static void host_cross_free(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(64);
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(free_outer_in_domain, work, &trap) == 1);
    CHECK(trap.site == 9 && work->outer_block == NULL);
}

static void realloc_outer_in_domain(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_realloc(work->outer_block, 4096);
    assert(work->outer_block);
    memset(work->outer_block, 4, 4096);
    tsuzuri_trap_raise(10, 2);
}

static void host_cross_realloc(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(8);
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(realloc_outer_in_domain, work, &trap) == 1);
    memset(work->outer_block, 5, 4096);
    if (work->trap) tsuzuri_trap_raise(8, 1);
}

static void nested_outer(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(32);
    struct work inner = { .count = 10 };
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(survivor_domain, &inner, &trap) == 0);
    CHECK(tsuzuri_domain_run(trapping_domain, &inner, &trap) == 1);
    if (work->trap) tsuzuri_trap_raise(11, 4);
    work->result = inner.result;
}

static void reenter_boundary(void *argument) {
    (void)argument;
    CHECK(0);
}

static void domain_with_boundary_inside(void *argument) {
    struct work *work = argument;
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_boundary_run(reenter_boundary, work, &trap) == 2);
    work->result = tsuzuri_tracked_malloc(24);
}

static void *thread_main(void *argument) {
    (void)argument;
    for (int round = 0; round < 2000; ++round) {
        struct work work = { .count = 20 };
        tsuzuri_trap_info trap = { 0, 0 };
        if (round % 2 == 0) {
            CHECK(tsuzuri_domain_run(survivor_domain, &work, &trap) == 0);
            tsuzuri_tracked_free(work.result);
        } else {
            CHECK(tsuzuri_domain_run(trapping_domain, &work, &trap) == 1);
        }
    }
    return NULL;
}

int main(void) {
    tsuzuri_trap_info trap = { 0, 0 };
    struct work work = { .count = 100 };

    /* A trap outside every frame returns. */
    tsuzuri_trap_raise(1, 1);

    /* A finished domain leaves its result, and only that. */
    CHECK(tsuzuri_domain_run(survivor_domain, &work, &trap) == 0);
    CHECK(work.result && live == 1);
    tsuzuri_tracked_free(work.result);
    CHECK(live == 0);

    /* A trapped domain leaves nothing, and says where. */
    CHECK(tsuzuri_domain_run(trapping_domain, &work, &trap) == 1);
    CHECK(trap.site == 42 && trap.kind == 3 && live == 0);

    /* Alternating, 10,000 times. */
    for (int round = 0; round < 10000; ++round) {
        struct work again = { .count = 5 + round % 17 };
        if (round % 2 == 0) {
            CHECK(tsuzuri_domain_run(survivor_domain, &again, &trap) == 0);
            tsuzuri_tracked_free(again.result);
        } else {
            CHECK(tsuzuri_domain_run(trapping_domain, &again, &trap) == 1);
        }
    }
    CHECK(live == 0);

    /* Inside a host boundary: the domain's trap does not touch the host's blocks; the host's own trap frees all. */
    struct work host = { .trap = 0 };
    CHECK(tsuzuri_boundary_run(host_boundary, &host, &trap) == 0);
    CHECK(live == 1);
    tsuzuri_tracked_free(host.outer_block);
    CHECK(live == 0);
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_boundary_run(host_boundary, &host, &trap) == 1 && trap.site == 7);
    CHECK(live == 0);

    /* A domain frees a block of the boundary around it, then traps: no double free later. */
    host = (struct work){ 0 };
    CHECK(tsuzuri_boundary_run(host_cross_free, &host, &trap) == 0);
    CHECK(live == 0);

    /* A domain grows a block of the boundary around it, then traps: the boundary still owns it. */
    host = (struct work){ .trap = 0 };
    CHECK(tsuzuri_boundary_run(host_cross_realloc, &host, &trap) == 0);
    CHECK(live == 1);
    tsuzuri_tracked_free(host.outer_block);
    CHECK(live == 0);
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_boundary_run(host_cross_realloc, &host, &trap) == 1 && trap.site == 8);
    CHECK(live == 0);

    /* Domains inside a domain. */
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_domain_run(nested_outer, &host, &trap) == 1 && trap.site == 11);
    CHECK(live == 0);
    host = (struct work){ .trap = 0 };
    CHECK(tsuzuri_domain_run(nested_outer, &host, &trap) == 0);
    CHECK(live == 2);
    tsuzuri_tracked_free(host.outer_block);
    tsuzuri_tracked_free(host.result);
    CHECK(live == 0);

    /* A host boundary cannot open inside a domain. */
    host = (struct work){ 0 };
    CHECK(tsuzuri_domain_run(domain_with_boundary_inside, &host, &trap) == 0);
    tsuzuri_tracked_free(host.result);
    CHECK(live == 0);

    /* Threads keep their own frames. */
    pthread_t threads[4];
    for (int index = 0; index < 4; ++index) assert(pthread_create(&threads[index], NULL, thread_main, NULL) == 0);
    for (int index = 0; index < 4; ++index) assert(pthread_join(threads[index], NULL) == 0);
    CHECK(live == 0);

    printf("domain runtime: %u checks passed\n", atomic_load(&checks));
    return 0;
}
```

### B-3 WASM の JS（`src/runtime/trap-boundary.mjs`）

先頭の `describe` の前に次を足し、`createBoundary` を `{ imports = {}, sites = [], frames = false } = {}` にして、`call` を次の形にする（`frames` が `false` の経路は今までと同じ）。Node 20.19.6 で、`tests/fixtures/trap_boundary` の `tz_div(7n, 0n)` が `-g` で `[{"function":"Main.div","index":0,"offset":219},{"function":"div","index":2,"offset":306}]`、`-g` なしで `[{"index":0,"offset":219},{"index":2,"offset":306}]` になることを確かめた。

```js
const FRAME = /^\s*at (?:(.*?) \()?wasm:\/\/wasm\/[^:]*:wasm-function\[(\d+)\]:0x([0-9a-f]+)\)?$/;
const SUFFIX = /\.(?:\$mono\.\d+|cold\.\d+|cpu\.[a-z0-9_]+)$/;

// The table of 1.3 for a linkage name: the name to show, or undefined.
function displayName(linkage) {
  let rest;
  if (linkage.startsWith("tz.fn.")) rest = linkage.slice("tz.fn.".length);
  else if (linkage.startsWith("tz_") && linkage.length > 3) rest = linkage.slice(3);
  else return /^tz\.specialized\.\d+$/.test(linkage) ? linkage : undefined;
  while (SUFFIX.test(rest)) rest = rest.replace(SUFFIX, "");
  return rest === "" ? undefined : rest;
}

// The frames of the stack of a WebAssembly.RuntimeError, innermost first.
function wasmFrames(error) {
  const frames = [];
  for (const line of String(error.stack).split("\n")) {
    const match = FRAME.exec(line);
    if (!match) continue;
    const linkage = match[1] === undefined ? undefined : /^[^.]*\.(tz[._].*)$/.exec(match[1])?.[1];
    const name = linkage === undefined ? undefined : displayName(linkage);
    const frame = { index: Number(match[2]), offset: parseInt(match[3], 16) };
    if (name === undefined) {
      frames.push(frame);
      continue;
    }
    const cold = linkage.includes(".cold.");
    const previous = frames[frames.length - 1];
    if (previous?.function === name && (cold || previous.cold)) {
      if (cold) continue;
      frames.pop();
    }
    frames.push({ function: name, ...frame, cold });
  }
  return frames.map(({ cold, ...frame }) => frame);
}
```

```js
    call(name, ...args) {
      const exports = current().exports;
      if (typeof exports[name] !== "function") throw new TypeError(`unknown export ${name}`);
      hostError = null;
      const limit = Error.stackTraceLimit;
      if (frames) Error.stackTraceLimit = Math.max(limit, 33);
      try {
        return { ok: true, value: exports[name](...args) };
      } catch (error) {
        instance = null;
        if (error === hostError) throw error;
        if (error instanceof WebAssembly.RuntimeError) {
          const trap = describe("trap", exports.tsuzuri_trap_site?.() ?? 0, sites);
          if (frames) trap.frames = wasmFrames(error);
          return { ok: false, trap };
        }
        if (error instanceof RangeError || error?.name === "InternalError") {
          return { ok: false, trap: { reason: "stack", site: 0 } };
        }
        throw error;
      } finally {
        if (frames) Error.stackTraceLimit = limit;
      }
    },
```

### B-4 `tests/domain_host.c`（想定。実装後に書いた形のまま通ることを確かめる）

`tests/fixtures/domain_host/Main.tz`:

```tsuzuri
export def risky :: i64 -> i64 -> i64
fn risky count divisor =
    let values = new [i64](count, \i -> i * 3 + 1)
    let text = "buffer" + to_string count
    values[count - 1] / divisor + text.length

export def guarded :: i64 -> i64 -> i64
fn guarded count divisor =
    match Domain.run (\() -> risky count divisor) with
    | Result.Ok value -> value
    | Result.Error failure -> 0 - failure.kind
```

`tests/domain.mjs` は、これを `--emit llvm --trap-mode return`（`-O0`・`-O3`）で出し、`clang -std=c11 -Wno-override-module -pthread tests/domain_host.c <ir> -o host`（ASan 版もある）で組んで走らせる。ホストは次の形（`trap.c` を数える確保で取り込む。試作の `real_host.c` が同じ形で、本物の `tz_risky`・`tz_make` に対して 10,000 回と入れ子で通った）:

```c
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
/* … counted_malloc / counted_free / counted_realloc と `#define TZ_TRAP_*` は domain_runtime.c と同じ … */
#include "../src/runtime/trap.c"

int64_t tz_risky(int64_t count, int64_t divisor);
int64_t tz_guarded(int64_t count, int64_t divisor);

static void nested(void *argument) {
    int64_t *value = argument;
    *value = tz_guarded(1000, 7) + tz_guarded(1000, 0);      /* a domain inside a host boundary */
    tz_risky(10, 0);                                         /* outside every domain: the boundary traps */
}

int main(void) {
    for (int round = 0; round < 10000; ++round) {
        int64_t divisor = round % 2 == 0 ? 7 : 0;
        int64_t expected = divisor == 0 ? -1 : 2998 / 7 + 10;
        assert(tz_guarded(1000, divisor) == expected);       /* kind 1: integer division by zero */
    }
    assert(live == 0);
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t value = 0;
    assert(tsuzuri_boundary_run(nested, &value, &trap) == 1 && trap.kind == 1);
    assert(value == 2998 / 7 + 10 - 1 && live == 0);
    printf("domain host: ok, live %ld\n", (long)live);
    return 0;
}
```

## C. `Domain` の安全性の議論

主張: トラップした `Domain.run` の後のプログラムは、呼び出しの前と同じ状態（呼び出しが確保した block と、捨てられる stack の frame を除く）にある。

| 前提 | 担保するもの |
| --- | --- |
| P1 本体は純粋（外界・時計・乱数・並行・共有可変状態・GPU・extern・`dyn`・利用者の `Drop` に触れない。`ref mut` を取らない） | A19 の検査（規則 3）。本体から到達する関数もすべて調べる（A19 の幅優先の到達可能性）。`DomainRun` 自体は不純（入れ子の拒否） |
| P2 捕捉は平たいデータ（関数値・`Rc`・`Arc`・`Task`・`Drop` 型・`dyn`・排他借用を持つ型は捕捉できない） | 規則 2（`own_violation`）。関数値の捕捉を許すと、A19 が到達する本体を静的に決められなくなるので許さない |
| P3 結果は平たいデータ | 規則 4。結果が領域の内部の状態を指すことがない（新しく確保した block を所有するだけ） |
| P4 領域の中の確保はすべて領域の owner に載る | `domain` のとき `heap_native_tracked`（`@tz.alloc` → `tsuzuri_tracked_malloc`）。`free`・`realloc` は frame の鎖（`tz_untrack_chain`）で owner を探す |
| P5 トラップの `siglongjmp` が飛び越す frame は、後始末を持たない | Tsuzuri のコードに巻き戻しはなく、P1・P2 により、`Drop`・`finally`・ロック・`Rc` の計数を持つ値が領域の中で作られない |

起こりうること: (1) 領域の中で確保した block は、トラップで owner の list ごと `free` される。成功すると、残った block（結果が所有するもの）は外側の owner へ `tz_domain_hand_over` で引き継がれ、外側がなければ素の block になる（後の `tsuzuri_tracked_free` は frame がなければ素の `free`）。(2) 閉包の環境は外の block で、`apply_value(…, borrowed = true)` が読むだけ（呼び出しごとに捕捉した値の写しが領域の中に作られる）。トラップしても環境は無傷で、`drop_value` が領域の外で 1 回解放する。(3) 確保失敗（`malloc` が `NULL`）は `@tz.alloc` が返ったあとにトラップするので、`owner->lock` を持ったままトラップすることはない。(4) 領域は呼び出したスレッドの TLS の frame に属し、純粋な本体は別のスレッドへ仕事を出さないので、領域の block を触るのは 1 スレッドだけ。`Task.parallel` の item の中の `Domain.run` は、item の frame（共有 owner）の上に自分の frame を積む（`domain_runtime.c` の 4 スレッドの確認）。

領域が閉じ込めないもの: スタック枯渇（`stack.c` が `abort()` する）、停止しない計算、確保量（制限がない。トラップするまで増える）、領域の外のトラップ。**A19 の純粋性の検査に穴が見つかったとき（S10）**は、この議論が成り立たない。穴を直すまで Phase 2 を出さない。

費用（付録 A-9）: 1 回あたり、`tz_boundary` の確保・mutex の初期化と破棄・`sigsetjmp`（空のドメイン 1 回で約 40 ns）と、領域の中の確保 1 回ごとの追跡（約 +35 ns）、成功時の引き継ぎ（外側があれば block ごとに追跡の移し替え。O(確保した block 数)）。大きな配列を捕捉すると、呼び出しごとに写しが作られる（`ref` で渡す・領域の中で作るほうが速い）。

## D. `E1038` の診断の一覧

コード `E1038`（D-55: B10、`Domain` の使い方の誤り）。メッセージは英語・小文字始まり・ピリオドなし・直し方つき。位置は主スパンだけ（`related`・`fixes` はない。直し方が文脈次第で、機械的に直せないため）。G22 が入ったら `help` に次の文を付け、`kind` を `.with_kind(…)` で付ける。

| 規則 | kind | メッセージ | 主スパン | help（G22 後） |
| --- | --- | --- | --- | --- |
| 0 | `domain_not_called_directly` | `'Domain.run' must be called directly with its closure; write 'Domain.run (\() -> ...)'` | その式 | `call it with a lambda at once` |
| 1 | `domain_argument_not_closure` | `the argument of 'Domain.run' must be a lambda or the name of a pure def; write 'Domain.run (\() -> ...)'` | 実引数 | `write the computation as a lambda here` |
| 2 | `domain_capture_not_plain` | `the closure passed to 'Domain.run' captures '{name}', which holds {what}; capture plain data or compute the value inside the closure` | 実引数のラムダ | `pass plain data, or build the value inside the closure` |
| 3 | `domain_body_not_pure` | `the closure passed to 'Domain.run' must be pure: it {reason}{chain}; move that work out of the closure` | A19 の `Impurity.span` | `call only pure functions inside the closure` |
| 4 | `domain_result_not_plain` | `the result of 'Domain.run' must be plain data; '{type}' holds {what}` | 呼び出し式 | `return plain data from the closure` |
| 5 | `domain_target_unsupported` | `'Domain.run' needs a native POSIX build with the system allocator: {why}` | 最初の呼び出し | `build for a native POSIX target` |

`{what}`・`{reason}`・`{chain}` は A19 の診断と同じ語彙（`{what}`: `a reference`・`an 'Rc' or 'Arc'`・`a value with a user Drop`・`an extern handle`・`a function, 'dyn', or a task`・`a type that is not plain data`。`{reason}`: `calls the extern function '{name}'`・`reads or writes the outside world ('{builtin}')`・`prints debug output`・`reads the clock, a random source, or a benchmark counter ('{builtin}')`・`starts or waits for a task or a parallel operation ('{builtin}')`・`uses shared mutable state ('{builtin}')`・`uses the GPU ('{builtin}')`・`uses dyn dispatch`・`holds a value whose type has a user Drop ('{type}')`・`cannot be proven pure`・加えて B10 が足す `runs a failure domain ('Domain.run')`。`{chain}`: ` (through 'A' -> 'B')`）。`{why}`: `WebAssembly cannot return from a trap into the module`・`--freestanding has no trap runtime`・`the allocator is not the system allocator`・`Windows is not supported`・`--emit llvm needs --trap-mode return`。

例（想定出力。位置の行は目安で、テストはコードとメッセージの部分文字列を見る）:

```tsuzuri
def risky :: unit -> i64
fn risky _ = 1 / 0

def main :: Array<string> -> i32
fn main args =
    let a = Domain.run risky
    let b = Domain.run (\() -> { Debug.print "x"; 1 })
    let log = \text -> Debug.print text
    let c = Domain.run (\() -> { log "x"; 1 })
    0
```

```text
Main.tz:6:24: error[E1038]: the argument of 'Domain.run' must be a lambda or the name of a pure def; write 'Domain.run (\() -> ...)'
Main.tz:7:34: error[E1038]: the closure passed to 'Domain.run' must be pure: it prints debug output; move that work out of the closure
Main.tz:9:24: error[E1038]: the closure passed to 'Domain.run' captures 'log', which holds a function, 'dyn', or a task; capture plain data or compute the value inside the closure
```

## E. `std/Domain.tz` と `TrapKind` の番号

`std/Domain.tz` の全文は、本文の「Phase 2: 設計」の枠のとおり。`Failure.kind` の値は `TrapKind` の数（`src/trap.rs` の `#[repr(u32)]`、`tsuzuri_trap_info.kind`、`.trap.json` の `site.kind` の並び。`site = 1 + n × 16 + kind`）:

| kind | `TrapKind` | `description()`（`.trap.json` の `kind`） | `Domain.run` の中で起きる例 |
| --- | --- | --- | --- |
| 0 | `Assert` | assertion failed | `assert` が偽 |
| 1 | `IntegerDivisionByZero` | integer division by zero | `x / 0` |
| 2 | `IntegerDivisionOverflow` | integer division overflow | `min / -1` |
| 3 | `BoundsCheck` | index out of bounds | `xs[5]`（長さ 3） |
| 4 | `AllocationSize` | allocation size overflow | 巨大な長さの配列 |
| 5 | `AllocationFailure` | allocation failed | `malloc` が `NULL` |
| 6 | `RangeStepZero` | range step is zero | 刻みが 0 の範囲 |
| 7 | `MatchFailure` | non-exhaustive match | 網羅されない `match`、`unreachable` |
| 8 | `PatternMismatch` | pattern mismatch | 失敗するパターン束縛 |
| 9 | `StringConcatOverflow` | string length overflow | 文字列の連結の長さの溢れ |
| 10 | `NumericRuntime` | numeric runtime trap | 数値の runtime 補助関数のトラップ |
| 11 | `WasmRuntime` | wasm runtime trap | WASM だけ（`Domain` は native なので起きない） |
| 12 | `Encoding` | invalid Unicode encoding | 不正な符号化の復号 |
| 13 | `DebugOutput` | debug output failed | 純粋な本体では起きない |
| 14 | `StackOverflow` | stack overflow | **閉じ込めない**（`stack.c` が `abort()`）。`Failure.kind` には現れない |
| 15 | `Overflow` | unhandled OverflowException: arithmetic operation resulted in an overflow | `@checked` の算術の溢れ（`try` の外） |

## F. 後始末の表（Phase 3。測った事実だけ）

トラップのとき何が走り、何が走らないか。測った行は「測定」欄に方法を書いた。E14 の仕様から取った行（このチケットでは測っていない）は「E14」と書いた。

| 事象 | 境界なし（プロセスが終わる） | `Domain.run` の中 | `tsuzuri_try_*` の中 | 測定 |
| --- | --- | --- | --- | --- |
| 利用者の `Drop` | 走らない | 本体が `Drop` 型を持てない（A19） | 走らない | `Drop` を持つ束縛のあと `/ 0` で、`drop` の出力がない（HEAD、`run`） |
| `try … finally` | 走らない（`try` は `OverflowException` だけを扱う字句的な構文） | 走らない | 走らない | `finally` で `Debug.print "finally ran"` する `try` の本体で `x / 0`: `trap:` の行と `E2005` だけで `finally ran` は出ない（HEAD、`run`、`-O0`） |
| 標準出力 `IO.write_line` | トラップの前に書いた行は残る | 本体が書けない（不純） | 残る | stdout をファイルとパイプへ向けて、トラップの前の行が出る（`IO.write_line` は毎回 `fflush`） |
| `File` の書き込み | 書いた分は残る（1 回ごとに `write`） | 本体が書けない | 残る | 同様（`write` を直接呼ぶ） |
| ロック（`Mutex.with_lock`） | プロセスごと終わる | 本体が取れない | 境界が `tsuzuri_sync_hooks.abandon` で解放する | E14 |
| ヒープ | OS が回収する | 領域の中で確保した block をすべて `free` する。成功なら結果の block が残る | 境界の中で確保した block をすべて `free` する。成功なら結果はホストの物 | `domain_runtime.c` と `trap_return.mjs`（`live == 0`） |
| `Task.parallel` の仲間 | プロセスごと終わる | 本体が使えない | worker のトラップは投入した呼び出しの status 1 に集約される | E14 |
| stack | プロセスごと終わる | 捨てられる（`siglongjmp`） | 捨てられる | `domain_runtime.c` |

## G. Phase 2 の codegen・経路・driver の詳細

### G-1 `emit_domain_run`（`src/llvm_domain.rs`）

`emit_typed_builtin`（`src/llvm.rs`）が `Builtin::DomainRun` のとき呼ぶ。`%arg0` が閉包（所有）。`a = instance.types[0]`、`T = emitter.ty(&a)`。`parallel_result_tasks`（`src/llvm_task.rs`）をそのまま手本にする。

1. `let function_type = Type::function(vec![Type::Unit], a.clone());`。補助関数の名前は `@tz.domain.thunk.<mangled_type(&a, self.module)>`（`parallel_result_tasks` の `@tz.task.item.results.<…>` と同じ作り方）。
2. `if let Some(marks) = &mut self.globals.traps { marks.sources.entry(name.clone()).or_insert((self.current_span, false)); }`。
3. 補助関数: `FunctionEmitter::new(self.module, self.function, self.function_id, self.builtins, self.intrinsics, self.globals, self.specializations)` を作り、`emitter.current_span = self.current_span;`。`%context` は `{ %tz.closure, T }` を指す `ptr`。`getelementptr { %tz.closure, T }, ptr %context, i32 0, i32 0` から閉包を `load` し、`emitter.apply_value(&closure, &function_type, Some((&Type::Unit, "0")), true).0` の値を、`i32 1` の slot へ `store`。`ret void`。`let definition = emitter.auxiliary(&format!("void {name}(ptr %context)")); self.intrinsics.insert(definition);`。unit の値は `"0"`、型は `i8`（HEAD の `TypedExprKind::Unit => "0"`）。
4. 呼び出し側（`self`）: `self.allocas.push(format!("{context} = alloca {{ %tz.closure, {T} }}, align 16"))`、`{info} = alloca {{ i32, i32 }}`。`%arg0` を `context` の field 0 へ `store`。`let status = self.value(format!("call i32 @tsuzuri_domain_run(ptr {name}, ptr {context}, ptr {info})"));` と `self.intrinsics.insert("declare i32 @tsuzuri_domain_run(ptr, ptr, ptr)".into())`。
5. `Result` の組み立て: `parallel_result_tasks` と同じく `declaration = &self.module.unions[*result_id]` の `cases` から `"Ok"`・`"Error"` の位置を引き、`let output = self.spill(result, "zeroinitializer");`、`success = icmp eq i32 {status}, 0`、`ok`・`error`・`done` の label。`ok`: field 1 を `load` し `self.construct_value(result, ok_case, Some((value, self.ty(a))))` を `output` へ `store`。`error`: `info` の 2 つの `i32` を `load` して `zext i32 … to i64`、`{ i64, i64 }` に `insertvalue` し、`construct_value(result, error_case, Some((tuple, "{ i64, i64 }")))` を `output` へ `store`（タプル `(i64 * i64)` の LLVM 型は `self.ty(&Type::Tuple(vec![Type::I64, Type::I64]))` で確かめる。違うなら、その文字列を使う）。
6. `done` の後で、**1 回だけ** `self.drop_value(&function_type, "%arg0")`（成功・失敗の両方がここへ合流する）。`apply_value` の `borrowed = true` は環境を消費しない。`false` にすると環境の block を領域の中で解放して捕捉した値を本体へ渡すため、トラップで外の block を失う（付録 C）。`output` を `load` して値とする。

### G-2 `emit_domain_message` と `describe`

`emit_domain_message`（`Builtin::DomainMessage`、`%arg0: i64`）: `@tz.trap.text(i32 %site, ptr %length) -> ptr` を呼び（宣言は書かない。定義は `instrument_with` が足す）、`@tz.string.from_utf8(ptr, i64) -> %tz.string`（`src/runtime/string_scalar.ll`・`string_v128.ll`。`string` を使う IR にはあり、`from_utf8` を呼ぶ IR では `runtime_symbols.contains("tz.string.from_utf8")` が CPU 版の runtime も呼び込む）へ `{ptr, length}` を渡して返す。site が表にないときは長さ 0（空文字列）。

`instrument_with` の `describe`（IR に `call ptr @tz.trap.text(` があるときだけ。ないとき出力は変わらない）: 各 `TrapSite` について `@tz.trap.text.<id> = private unnamed_addr constant [N x i8] c"…"`（`site.text(sources)` の UTF-8 を `\XX` で。既存の `@tz.trap.message.<id>` と同じ書き方）、`define internal ptr @tz.trap.text(i32 %site, ptr %length) cold nounwind` が `switch i32 %site` で定数のアドレスと長さ（`store i64 N, ptr %length`）を返し、既定は `store i64 0, ptr %length` と `ret ptr null`。`TrapSite::text` は `message` から `trap: ` を除いたもの（`sources` が空なら `kind.description()` だけ）。`instrument_with` は `sources` が空でも `raise` なら `E2000`（`trap information requires a source map`）にしない（`tsuzuri test`・`bench` の実行ファイルは `sources` を持たない）。`managed` の除外に `!name.starts_with("@tz.domain.thunk.")` を足す（C から呼ばれる補助関数は `%tz.context` を取らない。`@tz.task.item.`・`@tz.parallel.chunk.` と同じ扱い）。

### G-3 経路の切り替えと driver

`let domain = !crate::check::domain_calls(module).is_empty();`（WASM では見ない）。`domain_calls` は `check` の子モジュール `domain_rules` の `pub(crate)` 関数で、`check.rs` が `pub(crate) use domain_rules::domain_calls;` で公開する。

`src/llvm.rs`: `emit_trap_return` の本体を `emit_trap_mode(module, options, sources, debug, mode)` へ移す。

```rust
struct TrapMode { cpu_dispatch: bool, levels: u8, wrappers: bool, executable_roots: bool, backtrace: bool }
// emit_trap_return(module, options, sources, debug, cpu_dispatch)
//   = emit_trap_mode(module, options, sources, debug,
//       TrapMode { cpu_dispatch, levels: cpu::host_levels(), wrappers: true, executable_roots: false, backtrace: false })
```

`emit_trap_mode` の本体は今の `emit_trap_return` のまま（`Instrumentation { traps: true, trap_return: true, multiversion: cpu_dispatch, cpu_dispatch: trusted_array, executable_roots, .. }`、`traps::instrument_with(…, InstrumentOptions { wasm: false, raise: true, backtrace })`、`cpu::multiversion(…, levels, f16)`）で、`wrappers` のときだけ `host_abi::try_wrappers` を足す。`emit_trap_return` の出力は 1 byte も変わらない（手順 12 の `identity` の `.ret.ll`・`.ret.o`）。

- `emit_native`: `emit_wasm` の判定の直後に `if domain { return emit_trap_mode(module, options, sources, debug, TrapMode { cpu_dispatch: true, levels, wrappers: false, executable_roots, backtrace }); }`（`backtrace` は Phase 1 の式）。
- `emit_test_runner_with`・`emit_bench_runner_with`: `if domain && !wasm { … }` で `emit_program` を `Instrumentation { traps: true, trap_return: true, ..今までの値 }` で呼び、`traps::instrument_with(ir, module, marks.unwrap(), sources, InstrumentOptions { wasm: false, raise: true, backtrace: false })` の `.ir` を返す。`sources` は `options.debug` があればその `TrapSource`、なければ `&[]`。
- driver（`build_complete`）: `options.validate()` の後、`runtime_objects` を決める前に `let domain = !crate::check::domain_calls(module).is_empty();`。`domain` のとき、(1) `E1038` の target 検査（2.3。位置は `domain_calls(module)[0]`）、(2) `runtime_objects = false`（`emit_program` が `traps`・`trap_return` と外部 runtime object の併用を `E2000` にするため）、(3) `trap_runtime` を `(…今の式…) || (domain && options.target == Target::Native && matches!(options.emit, Emit::Executable | Emit::Object | Emit::Shared))` にする、(4) 実行ファイルのリンクの `-pthread` の条件 `(task_runtime || net_runtime)` に `|| trap_runtime` を足す（通常のリンクと `dwarf_sidecar` のリンクの 2 か所）。
- `src/test_runner.rs`: `compile_native_runner` は `domain`（`check::domain_calls(module)` が空でない）のとき、`sources` に `("trap.c", include_str!("runtime/trap.c").to_owned())` を足す（ほかの runtime と同じく別の翻訳単位で、`-x c -std=c11 -c -pthread`）。`build_runner` は `wasm && domain` のとき `E1038`（`'Domain.run' needs a native POSIX build with the system allocator: WebAssembly cannot return from a trap into the module`）。

### G-4 `backtrace.c` のリンク（Phase 1。`src/driver.rs`）

```rust
// stack_runtime の隣
let backtrace_runtime = options.emit == Emit::Executable
    && text.contains("declare void @tsuzuri_trap_backtrace(");
// stack_source・stack_object の隣
let backtrace_source = temporary.path.join("backtrace.c");
let mut backtrace_object = temporary.path.join("backtrace.o");
if backtrace_runtime {
    fs::write(&backtrace_source, include_str!("runtime/backtrace.c"))
        .map_err(|error| io_error("write backtrace runtime", &backtrace_source, error))?;
}
// `if stack_runtime && options.debug_info { … }` の複製。名前だけ "backtrace-debug"・include_str!("runtime/backtrace.c")・"backtrace.c" にして、
// backtrace_object = compile_runtime_object(…) か、キャッシュなしなら runtime.arg(&backtrace_source).arg("-o").arg(&backtrace_object)
// 通常のリンク: `if stack_runtime && dwarf_sidecar.is_none() { … }` の直後
if backtrace_runtime && dwarf_sidecar.is_none() {
    if options.debug_info { clang.args(["-x", "none"]).arg(&backtrace_object); }
    else { clang.args(["-x", "c", "-std=c11"]).arg(&backtrace_source); }
}
// dwarf_sidecar のリンク: `if stack_runtime { linker.arg(&stack_object).arg("-pthread"); }` の直後
if backtrace_runtime { linker.arg(&backtrace_object); }
```

## H. Phase 2 の変更表と手順の詳細

### H-1 変更表

| ファイル | 変更 |
| --- | --- |
| `src/runtime/trap.c` | B-2 の差分（`sigsetjmp`、`tz_jump`・`TZ_SETJMP`・`TZ_LONGJMP`、`tsuzuri_domain_run`、`tz_domain_hand_over`、`tz_untrack_chain`、`tsuzuri_tracked_free`・`tsuzuri_tracked_realloc` の変更、先頭の Linux 用の `_GNU_SOURCE`）。ほかは変えない |
| `std/Domain.tz`（新規） | 本文の全文 |
| `src/stdlib.rs` | `SOURCES` に `("std/Domain.tz", include_str!("../std/Domain.tz"))`（`Dir.tz` と `Env.tz` の間）、`RESERVED_MODULES` の末尾に `"Domain"`、`OPT_IN` の末尾に `OptIn { module: "Domain", names: &["Domain"], aliases: &[], uses: &[] }`。単体テスト `RESERVED_MODULES.len()` の期待値 53 を 54 に。`loaded("let r = Domain.run (\\() -> 1)")` が `["Domain"]` になるテストを足す |
| `vsc/src/core.ts` | `libraryModules` の文字列の末尾（`Channel` の後）に ` Domain` |
| `src/check.rs` | `Builtin::DomainRun`・`Builtin::DomainMessage`（enum・`ALL`（`&'static [Self]` に 2 つ足す）・`name`・`signature`）。`#[path = "domain_rules.rs"] mod domain_rules;` と `pub(crate) use domain_rules::domain_calls;`。`validate_cpu_functions` の直後に `diagnostics.extend(domain_rules::validate(&module));` |
| `src/polymorph.rs` | `Checker::builtin` に `DomainRun`・`DomainMessage` の `E1022`（`self.module == "Domain" && self.names.origin(self.module) == ModuleOrigin::Std` 以外では `the failure-domain primitives are private to the standard Domain module; call Domain.run`）。`Gpu` の隣の書き方 |
| `src/domain_rules.rs`（新規） | `domain_calls`・`validate`（本文 B1・B2） |
| `src/pure_rules.rs`（A19 の新規ファイル） | A19 の関数の切り出し 3 つと `Category::Domain`（本文「前提とする他チケットのインターフェース」） |
| `src/llvm_domain.rs`（新規） | `emit_domain_run`・`emit_domain_message`（G-1・G-2）。`src/llvm.rs` に `#[path = "llvm_domain.rs"] mod domain;`（`llvm_task.rs` と同じ接続） |
| `src/llvm.rs` | `emit_typed_builtin` の分岐と `emit_builtin` の列挙に 2 つの `Builtin`。`emit_trap_return` → `emit_trap_mode`、`emit_native`・`emit_test_runner_with`・`emit_bench_runner_with` の Domain の経路（G-3） |
| `src/llvm_traps.rs` | `describe`、`sources` が空でも `raise` なら通す、`managed` の除外（G-2） |
| `src/trap.rs` | `TrapSite::text` |
| `src/driver.rs`・`src/test_runner.rs` | G-3 |
| `tests/trap_return.mjs` | `unit(...)` の 4 行に並べて `tests/domain_runtime.c` を 4 つのフラグで組む 4 行 |

### H-2 手順 9〜17 の詳細

共通確認（C）は本文のとおり。

**手順 9: `trap.c`（C）テスト先行**
- 変更: `src/runtime/trap.c`、`tests/domain_runtime.c`（新規）、`tests/trap_return.mjs`。
- 内容: B-2 の `domain_runtime.c` を `tests/domain_runtime.c` に置き、`tsuzuri_domain_run` がないのでコンパイルに失敗することを確かめる。B-2 の差分を `trap.c` に当てる。`tests/trap_return.mjs` の `unit-O0`〜`unit-tsan` の 4 行の隣に、同じ 4 つのフラグで `tests/domain_runtime.c` を組み、出力が `/^domain runtime: \d+ checks passed\n$/` であることを確かめる 4 行を足す。
- 確認: `clang -std=c11 -Wall -Wextra -Werror -O0 -pthread tests/domain_runtime.c -o target/tz-b10/domain_runtime && target/tz-b10/domain_runtime` が `domain runtime: 18041 checks passed`（`-O3`・ASan+UBSan・TSan も同じ）。`node tests/trap_return.mjs target/release/tsuzuri`（exit 0。E14 の C の確認は `26897 checks passed`）、`node tests/trap_return_sync.mjs target/release/tsuzuri`、`node tests/tasks.mjs target/release/tsuzuri`。`vsc/toolchain/zig/zig cc -target x86_64-windows-gnu -std=c11 -Wall -Wextra -c src/runtime/trap.c -o target/tz-b10/trap-win.o` が警告 0（`x86_64-linux-gnu`・`aarch64-linux-musl` も）。C。

**手順 10: `Domain` の std と組み込み関数（検査まで。codegen は手順 13）**
- 変更: `std/Domain.tz`（新規）、`src/stdlib.rs`、`vsc/src/core.ts`、`src/check.rs`、`src/polymorph.rs`、`src/llvm.rs`（`emit_typed_builtin` の分岐に `Builtin::DomainRun | Builtin::DomainMessage => unreachable!("B10 step 13")`、`emit_builtin` の列挙）。
- 内容: H-1 の該当行と本文 B3。`tests/fixtures/domain/Main.tz`（新規）は 2.1 の最初の例（`average`・`describe`・`main`）に、`tsuzuri test` 用の `test` 宣言 2 つ（2.1 の 2 つ目の例と、成功する `Domain.run`）を足したもの。
- 確認: `cargo test --locked --lib stdlib`（件数は手順 1 と同じ）、`cd vsc && npm test`（`core.test.ts` が `RESERVED_MODULES` と `libraryModules` の一致を確かめる）。`cargo build --release --locked` のあと `target/release/tsuzuri check tests/fixtures/domain` が通る。C。

**手順 11: `E1038`（呼び出し地点の検査）テスト先行**
- 変更: `src/domain_rules.rs`（新規）、`src/check.rs`（`mod`・`pub(crate) use`・`validate` の呼び出し）、`src/pure_rules.rs`（A19 の関数の切り出し 3 つ）、`tests/domain.rs`（新規）。
- 内容: 先に `tests/domain.rs` に `domain_accepts_lambdas_pure_defs_and_plain_captures`・`domain_rejects_each_rule_with_e1038` を書いて失敗させ、B1・B2 と A19 の切り出しを実装する。`src/pure_rules.rs` の単体テストの個数（不純 91・純粋 157）を直す。
- 確認: `cargo test --locked --test domain`（N = 2）、`cargo test --locked --test pure_functions`（A19 の出力が変わらない）、`cargo test --locked --lib pure_rules`。C。

**手順 12: `describe`・`TrapSite::text`・`emit_trap_mode`（出力不変）**
- 変更: `src/trap.rs`（`TrapSite::text`）、`src/llvm_traps.rs`（G-2）、`src/llvm.rs`（`emit_trap_return` の本体を `emit_trap_mode` へ。G-3）。
- 確認: `identity` が差分なし（`.ret.ll`・`.ret.o` を含む。**これが不変の証拠**）、`cargo test --locked --test trap_locations`、`node tests/trap_return.mjs target/release/tsuzuri`。C。

**手順 13: `Domain.run` の codegen と経路の切り替え**
- 変更: `src/llvm_domain.rs`（新規）、`src/llvm.rs`（`emit_typed_builtin`・`emit_native`・`emit_test_runner_with`・`emit_bench_runner_with`）。
- 内容: G-1・G-2・G-3 のとおり。`unreachable!` を置き換える。テスト先行: `tests/domain.rs::domain_ir_is_deterministic_and_marks_the_runtime`（同じ入力で IR が一致、`@tsuzuri_domain_run` を 1 つ呼び、`Domain` のないプログラムの IR に `tsuzuri_domain_run`・`tsuzuri_tracked_` がない）。
- 確認: `cargo test --locked --test domain`（N = 3）、`identity` が差分なし。C。

**手順 14: driver・test runner・E2E**
- 変更: `src/driver.rs`、`src/test_runner.rs`、`tests/domain.mjs`・`tests/domain_host.c`・`tests/fixtures/domain_host/Main.tz`（新規。B-4）。
- 内容: G-3 の driver と test runner。`tests/domain.rs` に `domain_target_errors`（WASM・`--freestanding`・`--allocator counting`・`--emit llvm` の `E1038`）と `programs_without_domain_keep_their_ir` を足す。`tests/domain.mjs`（`tests/trap_return.mjs` の書き方）は、`-O0`・`-O3` で `tests/fixtures/domain` の `run`（`average 9`・`failed (kind 1)`）、`tsuzuri test`（`kind == 3` のテストが通る）、B-4 のホスト（ASan 版も）。
- 確認: `node tests/domain.mjs target/release/tsuzuri`（exit 0。`live == 0`）、`cargo test --locked --test domain`（N = 5）、`node tests/trap_return.mjs`・`trap_return_sync.mjs`・`tasks.mjs`、`cargo test --locked --test test_runner`。C。

**手順 15: 文書（Phase 2）** — 本文「ドキュメント」の Phase 2 の行。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/domain.md _tsuzuri/language-reference/compiler/diagnostics.md`。

**手順 16: Phase 2 の最終確認** — GUIDE §3.1 の回帰 4 コマンド、`cargo test --locked`、`node tests/domain.mjs`・`trap_return.mjs`・`trap_return_sync.mjs`・`tasks.mjs`・`e2e.mjs`、`TSUZURI_TSAN=1 node tests/trap_return_sync.mjs`、`sh scripts/check-runtime-includes.sh`。受け入れ条件の Phase 2 がすべて ✓。

**手順 17（Phase 3）: 後始末の表と facts との対応（文書だけ）** — 付録 F の表を `docs/language.md`「トラップ位置」と `exception-handling.md` に写し、`E2005` の `trap.kind` が `.trap.json` の `kind` と同じ文字列であることを `docs/language.md` に書く。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/exception-handling/exception-handling.md`。

