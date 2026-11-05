# TZ14: spawn しないプログラムの `Parallel.*` でも、スレッドの記憶域を読むランタイムの IR 関数を `noinline` にする（`--allocator small` の cache と G の移動）

| 項目 | 内容 |
| --- | --- |
| ID | TZ14 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | TZ10, TZ11 |
| 後続 | – |
| 状態 | done（2026-11-06。手順 4 の測定は別に行う） |
| 起票 | 2026-11-05（TZ10 の作業中に発見。TZ10 のチケットの「残したこと」の続き。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（言語の意味は変えない。生成 IR の属性だけを変える） |
| 改善する劣位 | 潜在的な不具合（失敗の再現は未確認）: spawn しないネイティブのプログラムでも、`Parallel.*` の helper の tzroutine は待つと別のスレッドで再開しうる。そのとき、`--allocator small` の `tz_small_realloc`（`@llvm.threadlocal.address` で自分のスレッドの cache を読む）がインライン展開された関数は、前のスレッドの cache の address を使い続けうる。TZ10 は spawn するプログラムだけを `noinline` で塞いだ |
| 手本にする既存実装 | `src/llvm_preempt.rs` の `with_preemption_checks` の `thread_bound`（`@llvm.threadlocal.address` を読む、ソース由来でない関数に `noinline` を足す）と、その単体テスト（`@tz.cache` の 3 通り。852〜897 行） |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_preempt.rs`, `tests/allocator.rs`（無ければ `tests/preemption.rs`）, `docs/architecture.md`, `_features/_completed/TZ10-preemption.md`, `_features/README.md` |

## 目的

tzroutine が別のスレッドへ移りうるすべてのネイティブのプログラムで、スレッドの記憶域（TLS）の address を呼び出しをまたいで使い回す最適化を起こさない。
LLVM は `@llvm.threadlocal.address` の結果を、関数の中では同じスレッドの上だと仮定して使い回してよい（intrinsic は `memory(none)` で、関数の途中でスレッドが変わることを知らない）。
G が待ちで別の M へ移ると、この仮定が破れ、size class の allocator が別のスレッドの cache を同期なしに書き換えうる（データ競合とヒープの破壊）。

## 着手条件と停止条件

### 着手条件

- TZ10・TZ11 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- tzroutine のランタイムを使わないプログラム（`Parallel.*` も spawn も無い）の IR が、修正の前後で byte 単位で変わる。
- spawn するプログラムの IR が、修正の前後で byte 単位で変わる（TZ10 の挙動は変えない）。
- `noinline` を足した `Parallel.*` のプログラムの性能が、手順 4 の測定で 1 % を超えて遅くなる（AGENTS.md: 測定して報告する。CI に閾値は置かない）。
- 手順 1 の調査で、helper の G が移る経路のほかに、spawn しないプログラムの pseudo-G（`tzr_current` が返す、P を持たないスレッドの G）が別のスレッドへ移る経路が見つかった。

## 現状（HEAD `5ee6b0f2` で確認）

- `src/llvm.rs:1494` の `spawning = spawns(module) || (!wasm && scheduler_parallel(module))` は、spawn するか、`Parallel.*`（`Routine.scope` を除く）を呼ぶネイティブのプログラムで真になる。
  `with_stack_probes`・`with_blocking_externs` はこの `spawning` で動く。
- `src/llvm.rs:1513〜1520` の `with_preemption_checks` は `!wasm && spawns(module)` のときだけ動く。コメント:
  「A program whose only tzroutines are the helpers of `Parallel.*` has nothing that waits for them: its other threads hold no P.」
  これは横取りの検査が要らない理由で、G が移らない理由ではない。
- `src/llvm_preempt.rs:588〜632` の `with_preemption_checks` は、検査を入れるのと同じ走査で、ソース由来でない関数（ランタイムの IR）のうち本文が
  `@llvm.threadlocal.address` を含み `noinline` でないものに ` noinline` を足す（`thread_bound`）。`with_preemption_checks` が動かないプログラムでは、この `noinline` も付かない。
- `@llvm.threadlocal.address` を使うランタイムの IR は `src/runtime/heap-small.ll` だけ（`grep -c threadlocal src/runtime/*.ll`）。その中で `noinline` でないのは
  `tz_small_realloc`（`--allocator small`）である。`tz_small_alloc`・`tz_small_free` は最初から `noinline`。
- `src/runtime/tzroutine.c:2931` の `tsuzuri_tz_parallel` は、呼び出し側のほかに `min(n, P) - 1`（呼び出し側が pseudo-G なら `min(n, P)`）個の helper を `tzr_spawn` で
  tzroutine として起こし、呼び出し側は `tzr_addr_wait` で待つ。helper が実行するコールバックの中で `Parallel.*` をもう一度呼ぶと、helper は G として
  `tzr_addr_wait` で待ち、起こされると別の M で再開しうる。入れ子の `Parallel.init` は spawn しないプログラムでビルドでき、実行できる（`Parallel.init 4` の中で
  `Parallel.init 2` を呼ぶプログラムを `--allocator small -O3` で確かめた）。
- `Parallel.*` のコールバックは `Atomic` と `Mutex` を捕捉できない（`E1005`。[Parallel](../../_tsuzuri/language-reference/built-in-types-and-modules/parallel.md)）ので、helper が待つ経路は、
  入れ子の `Parallel.*` など、コールバックの中で待ちを起こす操作に限られる。
- [TZ10](TZ10-preemption.md) の「着手時の再検討」4 は、検査を spawn するプログラムに限り、spawn しないプログラムの IR を 1 byte も変えないこと（TZ10 の S1。`tests/preemption.rs` と `examples/*` の IR の比較）を決めた。
  このチケットは、その範囲のうち `--allocator small` の `Parallel.*` のプログラムだけを、意図して変える（D1）。
- [TZ10](TZ10-preemption.md) の「着手時の再検討」9 と「残したこと」は、この危険が TZ10 の検査より前からの tzroutine の待ちにもあったこと、spawn するプログラムでだけ `noinline` にしたことを記録している。

### 観察（失敗はまだ再現していない）

入れ子の `Parallel.init` の前後で `Vec.add` する関数を `--allocator small -O3 --emit llvm` で出し、`opt -O3`（Homebrew LLVM 21）で最適化すると、
`tz_small_realloc` は `@"tz.fn.Vec.add.$mono.2"` へインライン展開され（その関数が `@llvm.threadlocal.address` を 3 回呼ぶ）、`Vec.add` は
コールバックへは展開されなかった。この例では、TLS の address が `tsuzuri_tz_parallel` の呼び出しをまたいで使われていない。危険は、
`Vec.add` か `tz.realloc` がコールバックへさらに展開されたときに起きる（インライン展開の判断しだいで、正しさが決まる）。

## 仕様

- spawn するか、`Parallel.*` を呼ぶネイティブのプログラム（`src/llvm.rs` の `spawning`）では、`@llvm.threadlocal.address` を使うランタイムの IR 関数は `noinline` である。
- spawn するプログラムの IR は今と同じ（TZ10 の `with_preemption_checks` がすでに付ける）。
- tzroutine のランタイムを使わないプログラムと WebAssembly の IR は今と同じ。
- spawn せず `Parallel.*` を呼ぶプログラムでも、既定の allocator と `--allocator host` の IR は今と同じ（TLS を IR で読む関数が無い）。変わるのは `--allocator small` の `tz_small_realloc` の見出しの ` noinline` だけで、`tests/preemption.rs` の S1 の比較（`grep -n allocator tests/preemption.rs` で、比べる組み合わせを確かめる）は通る。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/llvm_preempt.rs` | `with_thread_storage_noinline`（新規、`pub`） | `thread_bound` の走査と ` noinline` の付与だけを行う関数。`with_preemption_checks` はこれを使うか、同じ helper を共有する |
| `src/llvm.rs` | `emit_program` の 1513〜1520 行 | `!wasm && spawns(module)` なら今の `with_preemption_checks`。そうでなく `!wasm && spawning` なら `with_thread_storage_noinline` |
| `src/llvm.rs` | 1513 行のコメント | 横取りの検査は要らないが、helper が移るので TLS の関数は `noinline` にする、と書き直す |
| `tests/` | 新しいテスト | 下の「テスト計画」 |
| `docs/architecture.md` | TZ10 の `noinline` の段落 | 適用する範囲を `spawning` に広げたことを書く |
| `_features/_completed/TZ10-preemption.md` | 「残したこと」 | TZ14 で塞いだことを書く |

### アルゴリズム

`with_thread_storage_noinline(ir)` は、`with_preemption_checks` の関数の分割（`functions`）と、`emitted_from_source` でない関数の本文に
`@llvm.threadlocal.address` があり見出しに ` noinline` が無いものを集める部分を使い、見出しの最後の ` {` の前に ` noinline` を入れる。
対象が無ければ `ir` をそのまま返す（byte 単位で同一）。

## 実装手順

### 手順 1: 経路の確認と再現の試み

- 変更: なし（`target/scratch/tz14/` の試作だけ）。
- 内容: 次を調べ、結果をこのチケットの末尾に追記する。
  1. spawn しないプログラムで G が別の M へ移る経路の一覧（`tzr_addr_wait`・`tzr_park` を G として呼ぶ箇所を `src/runtime/tzroutine.c` で探し、`Parallel.*` の
     コールバックの中から届くものを選ぶ）。
  2. 失敗の再現の試み: `tz_small_realloc` を `alwaysinline` にした IR（試作。commit しない）で、入れ子の `Parallel.init` の前後で確保するプログラムを
     `-O3` でビルドし、最適化後の IR で TLS の address が `tsuzuri_tz_parallel` をまたいで使われることを確かめる。可能なら `TSUZURI_TSAN=1` 相当の
     ThreadSanitizer のビルドで競合を報告させる。
- 確認: 経路の一覧と、再現できたかどうか（できなければ、できなかった理由）を追記した。停止条件の 4 つ目に当たれば停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `src/llvm_preempt.rs` の単体テスト、`tests/allocator.rs`（`--allocator small` の IR を見るテストがあるファイル。無ければ `tests/preemption.rs`）。
- 内容:
  - 単体テスト: `with_thread_storage_noinline` が、`@llvm.threadlocal.address` を読む `@tz.cache` に ` noinline` を足し、ソース由来の関数（`@tz.fn.…`）には足さず、
    対象が無い IR を同一のまま返す（既存の 852〜897 行のテストと同じ形）。
  - 統合テスト `parallel_programs_keep_thread_storage_out_of_callbacks`: spawn せず `Parallel.init` を呼ぶプログラムを `--allocator small` の native の library として出力し、
    `define internal ptr @tz_small_realloc(` の見出しに ` noinline` があることを確かめる。比較として、`Parallel.*` を使わないプログラムには付かないこと、spawn するプログラムでは今と同じに付くことも確かめる。
- 確認: 統合テストの `Parallel.init` の場合だけが失敗する。

### 手順 3: `with_thread_storage_noinline` を入れる

- 変更: `src/llvm_preempt.rs`、`src/llvm.rs`。
- 確認: 手順 2 のテストが成功する。`cargo test --locked --test preemption` と `--test allocator` が全件成功する。
  `tests/fixtures/` の spawn するプログラムと、tzroutine のランタイムを使わないプログラムの `--emit llvm` が、修正の前後で `cmp` で一致する。

### 手順 4: 性能の確認

- 内容: `Parallel.*` を使うベンチマーク（`benchmarks/run-tasks.mjs`、`benchmarks/run-tzroutine.mjs` の該当カーネル、`docs/benchmarks.md` の `Parallel` の行）を、
  `--allocator small` で修正の前後に交互に実行する（GUIDE §14）。
- 確認: 差が 1 % 以内。超えたら停止する。結果を `docs/benchmarks.md` に記録する。

### 手順 5: ドキュメントと最終確認

- `docs/architecture.md`（`grep -n "threadlocal" docs/architecture.md`）と TZ10 のチケットの「残したこと」を更新する。
- T2。`node tests/features.mjs target/debug/tsuzuri` を `TSUZURI_TEST_ALLOCATOR=small` で、`Parallel.*` を使う suite について。

## テスト計画

### Rust テスト

- `src/llvm_preempt.rs`: `with_thread_storage_noinline` の単体テスト（新規）。
- `tests/allocator.rs` か `tests/preemption.rs`: `parallel_programs_keep_thread_storage_out_of_callbacks`（新規。3 通りの比較）。

### E2E

- `TSUZURI_TEST_ALLOCATOR=small node tests/features.mjs target/debug/tsuzuri <Parallel の suite>`（native の `-O0`・`-O3`）。
- 手順 1 で再現できたら、その program を `tests/tzroutine.mjs` か `tests/features.mjs` の回帰に足す。

### 既存テストへの影響

- `Parallel.*` を呼び spawn しないプログラムの、`--allocator small` の IR の `tz_small_realloc` の見出しだけが変わる。IR の期待値を固定しているテストがあれば
  （`grep -rn "tz_small_realloc" tests`）、その行を直す。

### 性能

- 手順 4。

## ドキュメント

- `_tsuzuri/language-reference/`: 変更不要（利用者から見える振る舞いは変わらない。`--allocator small` のページは TLS の扱いを書いていない）。
- `docs/architecture.md`: TZ10 の `noinline` の適用範囲。
- `_features/_completed/TZ10-preemption.md`: 「残したこと」。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [x] 手順 1 の調査結果がチケットに追記されている。
- [x] spawn せず `Parallel.*` を呼ぶネイティブのプログラムで、`tz_small_realloc` が `noinline` になる。
- [x] spawn するプログラムと、tzroutine のランタイムを使わないプログラムの IR が修正の前後で同じ。
- [ ] 手順 4 の測定が 1 % 以内で、`docs/benchmarks.md` に記録されている（別に行う。既存の `Parallel` のベンチマークのカーネルは、修正の前後で `-O3` の object が byte 単位で同じ）。
- [x] T2 が成功する。

## 落とし穴

- `with_preemption_checks` の全体を `spawning` で動かすと、spawn しないプログラムにも横取りの検査が入る。TZ10 は検査を spawn するプログラムに限っている（TZ10 の D1・S1）。
  `noinline` の部分だけを分ける。
- `--allocator host`・既定の allocator は TLS を IR で読まない。`--allocator small` の行で確かめないと、テストが何も確かめない。
- WebAssembly の JSPI の tzroutine は 1 本のスレッドの上を交代に走るので、対象外（`!wasm`）。

## 対象外

- C で書いたランタイム（`tzroutine.c` など）の TLS。C のランタイムは生成コードへインライン展開されない（bitcode の LTO はホストの責任。TZ10 の「Phase 2 は実行ファイルだけ」）。
- `Parallel.*` のコールバックに横取りの検査を入れること。

## 決定事項

### D1: `noinline` だけを `spawning` に広げる

- 決定: TLS を読むランタイムの IR 関数の `noinline` を、`spawns` から `spawning`（spawn するか `Parallel.*` を呼ぶネイティブのプログラム）に広げる。横取りの検査は今のまま spawn するプログラムに限る。
- 理由: G が移るのは helper の tzroutine があるすべてのプログラムで、横取りの要否とは別の条件である。`noinline` は `tz_small_realloc` だけに効き、確保の伸長のたびの呼び出しの費用しか足さない。
- 状態: 既定案（実装者はこの案に従う）。手順 4 の測定で 1 % を超えたら見直す。

## 手順 1 の調査結果（2026-11-06）

### spawn しないプログラムで G が別の M へ移る経路

spawn しないネイティブのプログラムで tzroutine として走るのは、`tsuzuri_tz_parallel`（`src/runtime/tzroutine.c`）が `tzr_spawn` で起こす手伝いの G（`tzr_parallel_helper`）だけです。
手伝いの G が `tzr_park`（`TZR_KIND_ROUTINE` の道。`tzr_switch_out` で g0 へ戻る）か `tsuzuri_tz_yield`（`tzr_after_yield` で大域の実行待ちへ）で止まると、
起こした側の P の実行待ち（`tzr_ready` の `tzr_runqput`）か大域の実行待ちに入り、盗みを含めてどの M でも再開します。コールバックの中から届くものは次のとおりです
（spawn しないプログラムでコンパイルでき、IR に該当の呼び出しがあることを、下の 3 つと入れ子の `Parallel.init` で確かめました）。

| コールバックの中の操作 | ランタイムの待ち |
| --- | --- |
| 2 チャンク以上の入れ子の `Parallel.*`（4,097 要素以上） | `tsuzuri_tz_parallel` → `tzr_addr_wait(&job.remaining, …, "Parallel")` → `tzr_park` |
| `Routine.yield ()` | `tsuzuri_tz_yield` → `tzr_switch_out`（`tzr_after_yield`） |
| `Time.sleep d` | `tsuzuri_tz_sleep` → `tzr_park(tzr_unlock_timers, …)` |
| `Channel.send`・`recv`・`select`、`DataChannel` | `tzr_park`（`Channel.send`・`Channel.recv`・`select`、`tzr_unlock_dchan`） |
| 共有した同期の型（`Arc` で共有した `Mutex`、`WaitGroup.wait`・`RwMutex`・`Once`・`Cond`・`Semaphore`） | `tzr_addr_wait` → `tzr_park` |
| `Net` の待ち | `tzr_park(tzr_netwait_commit, …)` |

移らないもの: ブロッキング呼び出し（`tsuzuri_tz_blocking_enter`・`_exit`。「the G never leaves its M inside the call」）、extern のコールバック（`tsuzuri_tz_callback_enter` が G を M に固定する）、
`Routine.with_locked_thread` の中の待ち（`tzr_park_locked` が同じ M で眠る）。spawn しないプログラムには横取りの検査も信号もありません（`with_preemption_checks` と
`async_preempt` はどちらも `spawns` が条件）。

擬似 G（`tzr_pseudo`。スレッドごとに `tzr_tls_pseudo` に 1 つ）は移りません。`tzr_park` の `TZR_KIND_THREAD` の道は自分の `note` の上でそのスレッドが眠り、
`tsuzuri_tz_yield` は `m == NULL || m->curg == NULL` で `tzr_os_yield` するだけ、`tsuzuri_tz_blocking_enter`・`_exit` と `tsuzuri_tz_callback_enter` も何もしません。
したがって停止条件の 4 つ目（擬似 G が移る経路）には当たりません。

### 失敗の再現

`alwaysinline` にしない今のコンパイラの出力のままで再現しました（`tz_small_realloc` を `alwaysinline` にした試作も作りましたが、最適化後の `@tz.parallel.chunk.1` は 1 行も変わりませんでした）。

- program: 外側の `Parallel.init count (\i -> grown i)` の `grown i` が、`Vec.add` を 40 回、`Parallel.init 8192 (\j -> j + i)`（2 チャンク）、`Vec.add` を 40 回して、
  要素の和を返す（`tests/small_allocator.mjs` の `nested`。期待値は `1640 + 40 * (33550336 + 8192 i)` の和で、`count = 40000` で 315818049600000。Python で `n = 200`・`2000` を
  直接足した値と式が一致することを確かめた）。
- IR: `--allocator small --emit llvm` を Clang 21 の `opt -O3`（`-mtriple=arm64-apple-macosx14.0.0`）で最適化すると、`tz_small_realloc` が外側のチャンク `@tz.parallel.chunk.1` へ
  インライン展開され、`%1 = tail call ptr @llvm.threadlocal.address.p0(ptr @tz_small_tc)` がチャンクのループの preheader（`b1.lr.ph`）へ出され、ループの本体の最初の `Vec.add` が
  `load ptr, ptr %1` で cache を読み、同じループに `call i32 @tsuzuri_tz_parallel(...)` がある。2 回目からの繰り返しは、前の繰り返しの待ちの後で、最初の M の cache を使う。
- 機械語: 同じことを実行ファイル（Clang 21 と Homebrew Clang 23、`-O3`）で確かめた。TLV の getter の呼び出し（`blr x8`）はループの前に 1 回で、結果を callee-saved の `x19` に置き、
  ループの中は `ldr x8, [x19]`、ループの中に `bl _tsuzuri_tz_parallel` がある。
- 実行: 変更前のコンパイラの `build -O3 --allocator small` の実行ファイルは、`TSUZURI_MAXPROCS=8` で 10 回中 10 回 SIGSEGV（Clang 21・23 とも）。`-O0` と既定の allocator は
  すべて正しい値。C のホストと IR から作った版では、`n = 40000` で P 4 が 5 回中 3 回、P 8・16 が 5 回中 5 回 SIGSEGV（`n = 8192`・`20000` で P 4 以下は 0 回。外側のチャンクが少ないと
  手伝いが少ない）。`tzroutine.c` の写しに数を足した計測では、P 8 の 1 回の実行で手伝いの G の park が約 15,700 回、別の M での再開が 263〜2,778 回あった。
- ThreadSanitizer: Homebrew Clang 23 の `-fsanitize=thread`（関数に `sanitize_thread`、`tzroutine.c` は `TZR_TSAN`）で、P 8 の 1 回の実行が 11 件のデータ競合
  （`tz.parallel.chunk.1` のインライン展開された fast path と、別のスレッドの `tz.fn.Vec.add.$mono.2` が同じ cache を読み書き）を報告し、`tz.parallel.chunk.1` の SEGV で止まった。
  macOS の Homebrew LLVM 21 の TSan は起動しない（`targets.yml` の注記）ので、Clang 23 で確かめた。
- `tz_small_realloc` の見出しに ` noinline` を足しただけの IR（変更後のコンパイラの出力と byte 単位で同じ）では、同じ条件の実行が 5 回中 0 回の失敗、TSan の 2 回の実行が 0 件の報告で、正しい値を返した。

## 実装と検証（2026-11-06）

- **範囲**: 手順 1・2・3・5。**手順 4 の測定は別に行う**（この作業の間、共有機（8 コア）で 7 つのエージェントが同時に build と test を走らせていて、load average が 20〜27 あり、計時に意味がないため）。
  代わりに、既存の `Parallel` のベンチマークのカーネルの機械語が変わらないことを確かめた（下の「手順 4 に代わる確認」）。
- **手順 1**: 上の「手順 1 の調査結果」。G が移る経路は手伝いの G の待ちだけで、擬似 G は移らない（停止条件 4 に当たらない）。失敗は、変更前のコンパイラの出力のままで、`-O3` の `--allocator small` の SIGSEGV と TSan のデータ競合として再現した。
- **手順 2（失敗するテストを先に）**: `src/llvm_preempt.rs` の単体テスト `thread_storage_stays_out_of_line_without_the_checks`（`@tz.cache` に ` noinline` を足し、`noinline` 済みの関数、ソース由来の `@tz.fn.M.spin`
  と `@"tz.fn.M.spin.cpu.avx2"` には足さない。`with_preemption_checks` の出力から検査の行と `@tz.preempt.poll` の定義を除いたものと同じ。対象の無い IR は同一）、`tests/allocator.rs` の
  `parallel_programs_keep_thread_storage_out_of_callbacks`（`tz_small_realloc` の見出しを、`src/runtime/heap-small.ll` の見出しと、その ` {` の前に ` noinline` を入れたものと比べる。
  tzroutine の無い版（`Parallel.init` を `Array.init` にしたもの）・spawn する版・spawn しない `Parallel.*` の版の順。既定の allocator と `--allocator host` の IR に
  `@llvm.threadlocal.address` が無いことも確かめる）、`tests/small_allocator.mjs` の `nested`（手順 1 の program を `--allocator small` の object にし、`-O0`・`-O3` × 既定の P・`TSUZURI_MAXPROCS=8`
  で実行して、値と、切り出したブロックがすべてキャッシュか中央へ戻ったことを確かめる）。変更前の動作（関数は `ir` をそのまま返す仮のもの）で、統合テストは spawn しない `Parallel.*` の
  比較（最後の assert）だけで失敗し、単体テストは最初の比較で失敗した。変更前のコンパイラ（`72f06862`）の `node tests/small_allocator.mjs` は `nested -O3` の実行が信号で終わって失敗した（`-O0` は成功）。
- **手順 3**: `src/llvm_preempt.rs` に `with_thread_storage_noinline` を足し、`with_preemption_checks` と、対象の見出しを集める `thread_bound` と ` noinline` を書き込む `push_line` を共有した
  （`with_preemption_checks` の出力は変わらない。下の IR の比較）。`src/llvm.rs` の `emit_program` は、`spawns(module)` を 1 回だけ求め、WebAssembly では何もせず、spawn するなら
  `with_preemption_checks`、そうでなく `spawning`（`Parallel.*` を呼ぶネイティブのプログラム）なら `with_thread_storage_noinline` を呼ぶ。コメントを、検査が要らない理由と `noinline` が要る理由に分けて書き直した。
- **IR の比較（手順 3 の確認）**: 変更前（`72f06862`。`src/` は `5ee6b0f2` と同じ）と変更後の dev のコンパイラで、`tests/fixtures/` の `Main.tz` を持つすべての project、`examples/*`、
  `benchmarks/tasks`・`tzroutine`・`matrix`、手順 1 の試作の 159 個を、既定の native・`--allocator small` の native・`--target wasm32` の 3 通りで `build --emit llvm --no-cache` した（467 個。
  build できない 16 個は前後で同じ）。437 個が byte 単位で同じ: 既定の native の 159 個すべて、wasm32 の 149 個すべて、`--allocator small` のうち spawn する 18 個（TZ10 の `noinline` が前から付いている）と
  `Parallel.*` を呼ばない 111 個（tzroutine のランタイムを使わない 106 個、`Channel`・同期の型・タイマーだけを使う 5 個）。変わった 30 個は、spawn せず `Parallel.*` を呼ぶプログラムの
  `--allocator small` の IR のちょうど全部で、どれも `define internal ptr @tz_small_realloc(` の見出しに ` noinline` が付いた 1 行だけが違う。`-O3 --emit llvm` も既定と同じ IR だった。
- **手順 4**: 手順 4 の測定は別に行う（計時の手順は末尾の「手順 4 の測定の手順（未実施）」。`docs/benchmarks.md` はまだ変えていない）。
- **手順 4 に代わる確認（計時ではない）**: 既存のハーネスで `Parallel.*` を測るものは、どれも変更の影響を受けない。`benchmarks/run-tasks.mjs`（`--data-parallel` を含む）は `--allocator system` を
  名指しし、program は `parallel_batch` の `Routine.scope` で spawn するので IR が前後で同じ。`benchmarks/run-matrix.mjs` も既定の allocator で IR が同じ。`--allocator small` でも、
  `run-tasks.mjs` の `portable()` と同じく `parallel_batch` を除いた spawn しない `benchmarks/tasks/Main.tz` と `benchmarks/matrix` は、`build --emit object -O3 --allocator small` の object が
  前後で byte 単位で同じだった（`tz_small_realloc` がカーネルから呼ばれないので、` noinline` は機械語を変えない）。機械語が変わるのは、`Parallel.*` のコールバック（とそこへ展開される関数）が
  `Vec`・文字列などを伸ばすプログラムだけで、`tz_small_realloc` の fast path がインライン展開から呼び出しになる。その計時が手順 4 で残る（`Vec.add` を 64 回する `Parallel.init` などの、
  前後で交互の測定）。
- **手順 5**: 文書（下の「文書」）と T2（下の「実行したコマンドと結果」）。
- **実行したコマンドと結果**: `cargo fmt --all -- --check`（成功）、`cargo clippy --locked --all-targets -- -D warnings`（警告なし）、`cargo test --locked --test allocator --test preemption`（10 件・6 件すべて成功）、
  `cargo test --locked --lib preempt`（8 件成功）、`cargo nextest run --locked --no-fail-fast`（2936 件すべて成功、4 件 skip）、`node tests/small_allocator.mjs target/debug/tsuzuri`（単体テストの O0・O3・ASan + UBSan・TSan、
  `grow`・`primitives`・`threaded`・`nested`（`-O0`・`-O3` × 既定の P・P 8）、2 つの object の共有。すべて成功）、
  `TSUZURI_TEST_ALLOCATOR=small node tests/features.mjs target/debug/tsuzuri <suite>`（native と WASM の `-O0`・`-O3`。`Parallel.*` を呼び spawn しない fixture の suite: `parallel` 36 件・`parallel_equivalence` 48 件・`pure_parallel` 12 件・`matrix` 529 件・`mutable_slices` 46 件・`monoid` 90 件・`arena` 27 件・`const_statics` 52 件・`dyn_dispatch` 55 件・`rc` 37 件・`record_layout` 225 件・`recursive_types` 23 件・`value_layout` 969 件、すべて成功。IR が変わった 30 個のうち `features.mjs` の suite であるもの全部）、
  `node tests/features.mjs target/debug/tsuzuri <suite>`（既定の allocator の `parallel` 36 件・`parallel_scheduler` 62 件・`pure_parallel` 12 件、すべて成功）、`node scripts/check-guide-lite.mjs`。言語リファレンスのページは変えていないので `check-docs` は対象のページが無い。
- **決定（チケットを細かくしたもの）**:
  1. `with_thread_storage_noinline` は `pub(crate)` にした（設計は `pub`）。`with_preemption_checks` と同じ可視性で、呼ぶのは `src/llvm.rs` と単体テストだけ。統合テストは `llvm::emit_with_options` を通す。
  2. 統合テストは `tests/allocator.rs` に置いた（`--allocator small` の IR を見るテストがあるファイル）。見出しは `"probe-stack"="inline-asm"` を除いてから比べる（tzroutine のあるプログラムには
     TZ02 の probe が付き、Windows では付かない）。期待値は `heap-small.ll` の見出しと仕様の「` {` の前に ` noinline`」から作り、コンパイラの出力は写していない。
  3. 再現した program を回帰にした（テスト計画の「手順 1 で再現できたら」）。置き場所は `tests/tzroutine.mjs`・`tests/features.mjs` でなく `tests/small_allocator.mjs` にした:
     `--allocator small` を end to end で build する suite で（`targets.yml` が走らせる）、`features.mjs` の suite の `options` は WASM の build にも渡るので `--allocator small` を名指せない。
     既存の `threaded` の program は spawn する（`heap_cycles` の `tzroutine`）ので TZ10 で塞がっており、spawn しない別の program `nested` にした。P 8 は、CI の 3〜4 コアの機械でも
     手伝いの数を増やすため（失敗は確率的なので、確定的な検査は Rust の IR のテスト）。
  4. 停止条件の「1 % を超えて遅くなる」は、この作業では判定していない（手順 4 は別に行う）。チケットの手順 4 が挙げた `run-tasks.mjs` と `run-tzroutine.mjs` は、このままでは
     変更を測らない（`run-tasks.mjs` は `--allocator system` を名指し、program は spawn する。`run-tzroutine.mjs` の `benchmarks/tzroutine/Main.tz` は `Parallel.*` を呼ばず、tzroutine を spawn する）ので、測るのは下の手順のカーネルにする。
- **文書**: `docs/architecture.md`（tzroutine の「横取り（TZ10）」の `noinline` の段落に TZ14 の範囲と再現、`Small` の `tz_small_realloc` の段落に `noinline` になる条件）、
  `_features/_completed/TZ10-preemption.md`（「残したこと」）、`_features/README.md`（TZ14 の行と追加チケットの表のリンク）。言語リファレンス・`docs/language.md`・README は変更不要
  （`grep -rn "threadlocal\|スレッドの記憶域\|noinline\|tz_small_realloc"` で該当なし。`--allocator small` の説明（スレッドごとのキャッシュ、別のスレッドの解放）は、修正でそのとおりになる）。
- **変更したファイル**: `src/llvm_preempt.rs`、`src/llvm.rs`、`tests/allocator.rs`、`tests/small_allocator.mjs`、`docs/architecture.md`、`_features/_completed/TZ10-preemption.md`、`_features/README.md`、このチケット。

### 手順 4 の測定の手順（未実施）

変更前（`72f06862`）と変更後の release のコンパイラで、`Parallel.init` のコールバックが `Vec` を伸ばす spawn しないカーネルを `-O3 --allocator small` で build し、交互に実行します
（GUIDE §14。順を毎回入れ替える。どちらも `128000000` を出力する。手順 1 の program（`tests/small_allocator.mjs` の `nested`）も同じ形で測れる）。コマンドはこの worktree の root で実行します。

```sh
git worktree add target/wt/tz14-base 72f06862
(cd target/wt/tz14-base && node scripts/slot.mjs -- cargo build --release --locked)
node scripts/slot.mjs -- cargo build --release --locked
mkdir -p target/scratch/tz14-bench/growth
cat > target/scratch/tz14-bench/growth/Main.tz <<'TZ'
export def flat_growth :: own i64 -> i64
fn flat_growth count =
    let outer = Parallel.init count (\i -> grown i)
    Array.sum (ref outer)

def grown :: i64 -> i64
fn grown i =
    let mut values: Vec<i64> = Vec.empty()
    for k in 1 ..= 64 do
        values = Vec.add (k + i) values
    Vec.length (ref values)

def main :: own unit -> i32 = \() ->
    do! IO.write_line (flat_growth 2000000)
    0
TZ
target/wt/tz14-base/target/release/tsuzuri build target/scratch/tz14-bench/growth -O3 --allocator small --no-cache -o target/scratch/tz14-bench/base
target/release/tsuzuri build target/scratch/tz14-bench/growth -O3 --allocator small --no-cache -o target/scratch/tz14-bench/head
for i in $(seq 1 15); do
  if [ $((i % 2)) = 1 ]; then order="base head"; else order="head base"; fi
  for c in $order; do /usr/bin/time -p target/scratch/tz14-bench/$c 2>&1 >/dev/null | awk -v c=$c '/^real/ { print c, $2 }'; done
done
```

既存の `Parallel` のカーネル（`run-tasks.mjs --data-parallel` など）は、上の「手順 4 に代わる確認」のとおり IR か `-O3` の機械語が前後で同じなので、測っても差は雑音です。
