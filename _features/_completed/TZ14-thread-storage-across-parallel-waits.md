# TZ14: spawn しないプログラムの `Parallel.*` でも、スレッドの記憶域を読むランタイムの IR 関数を `noinline` にする（`--allocator small` の cache と G の移動）

| 項目 | 内容 |
| --- | --- |
| ID | TZ14 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | TZ10, TZ11 |
| 後続 | – |
| 状態 | done（2026-11-06。手順 4 の測定で D1 を狭めた: 手伝いの tzroutine が待ちうるプログラムだけ） |
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
  手順 4 の測定の後、spawn しないプログラムでは手伝いの tzroutine が待ちうるときだけに狭めた（D1 の見直し。末尾の「手順 4 と D1 の見直し」）。
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
- [x] spawn せず `Parallel.*` を呼ぶネイティブのプログラムで、手伝いの tzroutine が待ちうる（`helpers_may_wait`）とき、`tz_small_realloc` が `noinline` になる（D1 の見直しで狭めた。待てないときは IR が `main` と同じ）。
- [x] spawn するプログラムと、tzroutine のランタイムを使わないプログラムの IR が修正の前後で同じ。
- [x] 手順 4 の測定が 1 % 以内で、`docs/benchmarks.md` に記録されている（最初の実装は `Vec` を伸ばす核で +17.1 % で停止条件に当たり、D1 を狭めた。狭めた後、その核と手伝いが待てないプログラムの IR は `main` と byte 単位で同じなので費用は 0（同じ IR なので測り直していない）。測定と判断は `docs/benchmarks.md` の TZ14 の節と末尾の「手順 4 と D1 の見直し」）。
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
- 見直し（2026-11-06）: 手順 4 で +17.1 % だったので、利用者から委ねられた権限で orchestrator が狭めた。spawn するプログラムは TZ10 のまま。spawn せず `Parallel.*` を呼ぶネイティブのプログラムは、手伝いの tzroutine が走らせるコードが待ちに届きうるときだけ `noinline` にし、そうでなければ IR を `main` と byte 単位で同じに保つ。理由: spawn しないプログラムの G は `tsuzuri_tz_parallel` の手伝いだけで（最上位の `Parallel.*` の呼び出し側は自分のスレッドを離れない擬似 G）、G が M を離れるのは待つときだけ（spawn しないプログラムには横取りの検査も SIGURG も無い）。待てないコールバックは 1 つの M の上で終わるので、その中で TLS の address を使い回しても安全である（末尾の「手順 4 と D1 の見直し」）。

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

- **範囲**（最初の実装 `7bc7b6b4` まで）: 手順 1・2・3・5。**手順 4 の測定は別に行う**（この作業の間、共有機（8 コア）で 7 つのエージェントが同時に build と test を走らせていて、load average が 20〜27 あり、計時に意味がないため）。
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
- **手順 4**: orchestrator が別に測り、+17.1 % で停止条件に当たった。D1 を狭めた（末尾の「手順 4 と D1 の見直し」）。
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
  4. 停止条件の「1 % を超えて遅くなる」は、この作業では判定していない（手順 4 は別に行った。結果と D1 の見直しは末尾）。チケットの手順 4 が挙げた `run-tasks.mjs` と `run-tzroutine.mjs` は、このままでは
     変更を測らない（`run-tasks.mjs` は `--allocator system` を名指し、program は spawn する。`run-tzroutine.mjs` の `benchmarks/tzroutine/Main.tz` は `Parallel.*` を呼ばず、tzroutine を spawn する）ので、測るのは下の手順のカーネルにする。
- **文書**: `docs/architecture.md`（tzroutine の「横取り（TZ10）」の `noinline` の段落に TZ14 の範囲と再現、`Small` の `tz_small_realloc` の段落に `noinline` になる条件）、
  `_features/_completed/TZ10-preemption.md`（「残したこと」）、`_features/README.md`（TZ14 の行と追加チケットの表のリンク）。言語リファレンス・`docs/language.md`・README は変更不要
  （`grep -rn "threadlocal\|スレッドの記憶域\|noinline\|tz_small_realloc"` で該当なし。`--allocator small` の説明（スレッドごとのキャッシュ、別のスレッドの解放）は、修正でそのとおりになる）。
- **変更したファイル**: `src/llvm_preempt.rs`、`src/llvm.rs`、`tests/allocator.rs`、`tests/small_allocator.mjs`、`docs/architecture.md`、`_features/_completed/TZ10-preemption.md`、`_features/README.md`、このチケット。

### 手順 4 の測定（2026-11-06）

orchestrator が、負荷の無い Apple M3 で、`main`（`72f06862`）と `7bc7b6b4` の release のコンパイラを使って測った。核は `docs/benchmarks.md` の TZ14 の節にある
`flat_growth`（200 万要素の `Parallel.init` のコールバックが `Vec` を 64 回伸ばす。それを 100 回。出力 `12800000000`）で、`build -O3 --allocator small` の実行ファイル。
2 つの IR の違いは `@tz_small_realloc` の見出しの ` noinline` だけ。交互の実行（順を毎回入れ替える。Node の `process.hrtime` で 1 プロセスずつ）で、
変更前の中央値 2172.8 ms（15 回、最小 2070.5）に対し変更後 2544.5 ms（最小 2390.6）、+17.1 %。2 回目の 11 回でも 2179.0 ms と 2550.7 ms、+17.1 %。停止条件の 1 % を大きく超えた。
原因: インライン展開されると、LLVM は TLS の address（macOS では TLV の getter の呼び出し）をコールバックのループの外へ出す。それはまさに、待ちをまたぐと危険で、待ちが無ければ安全な使い回しである。

### レビューの指摘への対応（2026-11-06）

`9758ef96` の後、独立したレビューが 2 件を指摘した。どちらも、利用者から委ねられた権限で orchestrator が対応を決めた。

1. **major: `helpers_may_wait` の関数の分け方の穴で、手伝いが待てないプログラムの `tz_small_realloc` が `noinline` になる（+14〜17 %）**
   - 指摘: ランタイムの IR の断片のうち `src/runtime/character.ll`・`display.ll`・`debug.ll`・`recursive.ll` は末尾に改行が無く、次の断片がその最後の `}` と同じ行で始まる（`}define internal void @tz.string.copy(...) nounwind {`、`}@tz.debug.newline = ...`）。
     `definitions` は `define ` で始まる行だけを見出しにし、`runtime_definitions` は `}` の後に注釈が続く形（`}; PM08: ...`）しか閉じなかったので、`@tz.string.copy` などは定義に入らず、その呼び出しが宣言された関数の呼び出し（待ち）に見えた。
     `Char` を表示するプログラムなどで、手伝いが待てないのに ` noinline` が付き、IR が `main` と違った（D1 の見直しの「待てなければ IR は `main` と byte 単位で同じ」に反する）。誤りは安全な側で、不健全ではない。
   - 再現（直す前の `9758ef96` の dev のコンパイラ）: レビューの `strjoin2`（`Parallel.init` のコールバックが `Vec` を 64 回伸ばして文字列を連結し、別の `export def letter` が `(to_string 'x').length` を返す）を `build --emit llvm --allocator small --no-cache` で出すと、
     `main` の出力と違うのは `tz_small_realloc` の見出しの ` noinline` の 1 行だけで、754 行目が `}define internal void @tz.string.copy(...)` だった。`letter` の無い `strplain` は `main` と同じ。
   - 失敗するテストを先に: `src/llvm_preempt.rs` の単体テスト `runtime_pieces_after_a_closing_brace_are_functions_of_their_own`。`}define`（2 回。2 回目は `@llvm.threadlocal.address` を読む `@tz.cache` の見出し）・`}@` の大域変数・`}%` の型・`}` と空白の後の注釈・`}declare`・`}attributes` の後にそれぞれ関数を置き、
     チャンクがそのすべてを呼ぶ IR で、(a) 分けた行をつなぐと元の IR、(b) 9 個の関数がすべて見つかり、それぞれが自分の `}` で閉じ、自分の印の行を持つ、(c) 待ちが無いので `helper_wait` は `None` で、IR はそのまま、
     (d) 7 個のランタイムの関数のどれに `tsuzuri_tz_yield` の呼び出しを入れても、その行が理由になり、`}` の後にある `@tz.cache` の見出しに ` noinline` が付く、を確かめる。期待値は IR の文法（`}` の後の top-level の形は前の関数に属さない）と D1 の規則から書き、コンパイラの出力は写していない。
     直す前の `runtime_definitions` では (b) で失敗した（見つかる関数が `@tz.text`・`@tz.integer.digits`・`@tz.fn.M.run` の 3 個だけ）。
   - 修正: `runtime_definitions(ir)` は行と定義を返す。`}` で始まり、空白の後に top-level の形（`TOP_LEVEL`: `define `・`declare `・`@`・`%`・`$`・`!`・`;`・`attributes `・`source_filename `・`target `・`module `・`uselistorder`）が続く行を、`}` と空白の後で 2 つに分け、そのうえで `definitions`（`}` だけの行で閉じる）を使う。
     分けた行は `ir` の切れ端なので、つなぐと `ir` に戻り、` noinline` を足さないときの出力は byte 単位で同じ。前の実装の `definitions_closed_by` は要らなくなったので消し、`definitions` を `main` の形に戻した。
     関数の本体の中で `}` から始まる行は閉じ括弧だけなので（`grep -n '^}.' src/runtime/*.ll` は該当なし。生成した IR の命令は字下げされる）、分けても本体の行を落とさない。top-level の形でないものが続く行は分けず、前のとおり次の関数を前の関数に含める（保守的な側）。
     ランタイムの `.ll` は変えていない（改行を足すと、それを含むすべてのプログラムの IR が変わる。停止条件の 1 つ目）。`with_preemption_checks` は `definitions` のままで、TZ10 の検査の位置は変わらない。
   - 修正後の証拠: `strjoin2` の IR は `-O0`（既定）と `-O3` の `--emit llvm --allocator small` で `main` と byte 単位で同じで、` noinline` が無い（`}define` の行はそのまま残る）。`strplain` も同じ。
     レビューが測った核（`docs/benchmarks.md` の `flat_growth` の `grown` に文字列の連結、`main` に `to_string 'x'` の表示を足したもの）も `-O0`・`-O3` で `main` と同じ。
     待つ版は、`}define` の行があっても ` noinline` が付く（`strjoin2` のコールバックに入れ子の `Parallel.init 8192` を足したもの、`Routine.yield ()` を足したもの）。
   - IR の比較: `main` のコンパイラ（`/Users/tmidorikawa/pj/Tsuzuri/target/debug/tsuzuri`）と修正後の dev のコンパイラで、`tests/fixtures/*` と `examples/*` の `Main.tz` を持つ 180 の project を `-O0` の `build --emit llvm --no-cache`、既定の確保器と `--allocator small` で出した（360 個）。
     333 個が byte 単位で同じ、12 個は前後で同じ診断で build できない（前の比較と同じ 6 project の 2 通り）、15 個が違う。違う 15 個は前の比較と同じ組で、どれも `--allocator small` の `tz_small_realloc` の見出しの ` noinline` の 1 行だけが違う。
     それぞれの手伝いが待ちうる理由（修正後の `helper_wait` が返す行。チャンクから直接の呼び出しで届く関数の中の行）:

     | project | `helper_wait` の行 | 待ちうる理由 |
     | --- | --- | --- |
     | `arena`・`trap_return_sync` | `call void %destroy(ptr %env)` | ポインターを通す呼び出し（`closure.ll` のクロージャの環境の drop） |
     | `dyn_dispatch` | `call void %destroy(ptr %data)` | ポインターを通す呼び出し（`@tz.dyn.drop` の `dyn` の値の drop） |
     | `concurrency_threads`・`rc`・`recursive_types` | `call void %action(ptr %node, ptr %pending)` | ポインターを通す呼び出し（`recursive.ll` の再帰型の節の drop） |
     | `parallel` | `%v38 = call i64 %v36(i64 %v33, ptr %v37, i1 false)` | クロージャの呼び出し |
     | `mutable_slices` | `%v49 = call %tz.closure %v47(i64 %v37, ptr %v48, i1 false)` | クロージャの呼び出し |
     | `strings` | `%v36 = call %tz.utf8string %v34(i64 %v31, ptr %v35, i1 false)` | クロージャの呼び出し |
     | `bindings_threads`・`data_channel_threads` | `%v3 = call i32 @tsuzuri_channel_close(ptr %v2, i32 1)` | tzroutine のランタイムの関数（`Channel`） |
     | `host_imports`・`user_drop` | `call void @tsuzuri_tz_blocking_enter()` | extern の呼び出し（ブロッキング呼び出しの包み） |
     | `parallel_report/basic` | `%written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)` | `NEVER_WAITS` に無い宣言された関数（`write`） |
     | `trap_return` | `call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.4, ptr %v27, i64 %v15)` | 入れ子の `Parallel.*`（本当に待つ） |

     15 個のうち 5 個（`concurrency_threads`・`parallel_report/basic`・`rc`・`recursive_types`・`user_drop`）は IR に `}` の後に断片が続く行を 1 つ持ち、修正後は関数が 1 つ多く見つかるが、理由の行は前の比較と同じ。
2. **nit: 受け入れ条件の 2 つ目が、D1 の見直しの後の規則と合わない**: 「手伝いの tzroutine が待ちうる（`helpers_may_wait`）とき」と書き直した。上の「実装」の「関数の分け方」にも、レビューの後の分け方を足した。

- 実行したコマンドと結果: `cargo fmt --all -- --check`（成功）、`cargo clippy --locked --all-targets -- -D warnings`（警告なし）、`cargo test --locked --lib preempt`（10 件成功。新しい単体テストを含む）、
  `cargo nextest run --locked --no-fail-fast`（2938 件すべて成功、4 件 skip）、`node tests/small_allocator.mjs target/debug/tsuzuri`（単体テストの O0・O3・ASan + UBSan・TSan、`grow`・`primitives`・`threaded`・`nested`（`-O0`・`-O3` × 既定の P・P 8、IR の ` noinline`）、2 つの object の共有。成功）、
  `TSUZURI_TEST_ALLOCATOR=small node tests/features.mjs target/debug/tsuzuri <suite>`（native と WASM の `-O0`・`-O3`。`parallel` 36 件・`parallel_equivalence` 48 件・`pure_parallel` 12 件・`recursive_types` 23 件・`rc` 37 件、すべて成功。後の 2 つは `}` の後に断片が続く行を持つ）、
  上の IR の比較、`node scripts/check-guide-lite.mjs`（88 件の参照、失敗 0）・`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 ブロック、古いもの 0）・`node scripts/check-cli-docs.mjs`（4 件、失敗 0）。
  言語リファレンスのページは変えていないので `check-docs` の対象は無い。文書: このチケット（受け入れ条件、「実装」の「関数の分け方」、この節）、`docs/architecture.md`（`helpers_may_wait` の段落に行の分け方）、`docs/benchmarks.md`（TZ14 の節にレビューの核と修正後の IR）。
- 決定:
  1. 分ける行を `}` の後に top-level の形が続く行に限った。`}` の後に何か続く行をすべて分けると、本体の途中で関数を閉じる誤りが（今の IR には無いが）不健全な側に出うる。限っておけば、知らない形は前のとおり次の関数を前の関数に含め、保守的な側に倒れる。
  2. 行の分け方は `runtime_definitions`（`with_thread_storage_noinline` だけが使う）に閉じ込め、`with_preemption_checks` の `definitions` と `src/llvm.rs` の `with_stack_probes`（TZ02）の見出しの判定には入れなかった。spawn するプログラムの IR を変えないため（停止条件の 2 つ目）。
  3. `_features/README.md` の TZ14 の行は変えていない。行の要約（待ちうるときだけ ` noinline`、ほかのプログラムの IR は変わらない）は、この修正でそのとおりになった。
  4. 費用は測り直していない。修正後、レビューの核の IR が `-O0`・`-O3` で `main` と同じなので、機械語も同じである。
- 残したこと（前からのことで、TZ14 では直さない）: ランタイムの IR の断片の連結の改行の欠けは、`with_preemption_checks` の `definitions`（TZ10 の検査の位置）と `with_stack_probes`（TZ02。`define ` で始まる行だけを見出しとみるので、`}define` の後の `@tz.string.copy` に probe が付かない。`strjoin2` の IR で確かめた）にも効く。
  直すにはランタイムの `.ll` か連結に改行を足すことになり、それを含むすべてのプログラムの IR が変わるので、TZ14 の範囲の外とした（チケットは起票していない）。

## 手順 4 と D1 の見直し（2026-11-06）

### 決定

利用者から委ねられた権限で、orchestrator は費用を受け入れずに D1 を狭めた（上の D1 の「見直し」）。spawn するネイティブのプログラムは TZ10 のまま（`with_preemption_checks`）。
spawn せず `Parallel.*` を呼ぶネイティブのプログラムは、手伝いの tzroutine が待ちうるとき、つまり手伝いが走らせるコードが待ちに届きうるときだけ `noinline` にし、そうでなければ IR を `main` と byte 単位で同じに保つ。

### 実装

`src/llvm_preempt.rs` の `with_thread_storage_noinline` は、`thread_bound` が空か `helpers_may_wait` が偽なら IR をそのまま返す。`helpers_may_wait`（理由の行を返す `helper_wait`）は IR の保守的な解析で、迷えば真と答える（保守的すぎても速さを失うだけで、待ちを見落とすとデータ競合になる）。

- 入口: `@tsuzuri_tz_parallel` のすべての呼び出しの第 1 引数（チャンク）。それが定義された関数の定数でないか、`@tsuzuri_tz_parallel` が直接の呼び出し以外で現れれば（`store` など）真。
- 入口から、定義された関数への直接の呼び出しをたどる。届く関数に、ポインター（レジスター）を通す呼び出し、インラインのアセンブリ、`invoke`・`callbr`、読めない呼び出し、`@tsuzuri_tz_parallel` の呼び出し（入れ子）、
  宣言された関数のうち `@llvm.*` の intrinsic と `NEVER_WAITS`（`malloc`・`calloc`・`realloc`・`free`・`posix_memalign`・`aligned_alloc`・`memcpy`・`memmove`・`memset`・`memcmp`）でないものの呼び出しがあれば真。
  ほかの `tsuzuri_*` のランタイムの関数はすべて待ちとみなす。trap と abort の関数は一覧に入れていない（見た fixture のコールバックは `@llvm.trap` で trap し、`--trap-mode return` などのランタイムの関数は待ちとみなす）。
- 関数の分け方: `definitions` は `}` だけの行で関数を閉じるが、ランタイムの IR の断片が閉じ括弧と同じ行で始まることがある（`}; PM08: ...`。`src/runtime/integer.ll` の 1 行目が前の断片の `}` の後に続く）。
  そのままだと `@tz.integer.digits` が前の関数に呑まれ、整数を表示するコールバックが宣言された関数を呼ぶように見えて真になった（`value_layout`）。`with_thread_storage_noinline` は、`}` の後に注釈だけが続く行でも閉じる `runtime_definitions` を使う。
  レビューの後、`}` の後に注釈以外の top-level の形（`}define ...`・`}@...`・`}declare ...` など）が続く行も分けるようにした（上の「実装と検証」の「レビューの指摘への対応（2026-11-06）」の 1）。
  `with_preemption_checks` は TZ10 の検査の位置を変えないよう `definitions` のまま（この連結の改行の欠けは前からある。直すと全プログラムの IR が変わるので、このチケットでは直さない）。

### 確かめた前提

- 擬似 G は別のスレッドで再開しない: `tzr_park` の `TZR_KIND_THREAD` の道は自分の `note` の上でそのスレッドが眠り、`tzr_ready` はその `note` を起こすだけ。`tsuzuri_tz_yield` は `m == NULL || m->curg == NULL` で `tzr_os_yield` するだけ（`src/runtime/tzroutine.c`）。
- spawn しないプログラムの手伝いは横取りで止まらない: `with_preemption_checks` は `spawns(module)` のときだけ（`src/llvm.rs` の `emit_program`）、`with_generated_section`（`tz_text`）と `-DTZ_ASYNC_PREEMPT` は `async_preempt`（`llvm::spawns(module)` が条件。`src/driver.rs`）のときだけで、
  `TZR_ASYNC`（SIGURG の handler と sysmon の `tzr_async_kick`）は `TZ_ASYNC_PREEMPT` のときだけ定義される。`tsuzuri_tz_preempt_check` を呼ぶのは検査の `@tz.preempt.poll` だけ。
- 手伝いの G が M を離れるのは待つときだけ: G から g0 へ戻るのは `tzr_switch_out`（`tzr_park` の `TZR_KIND_ROUTINE` の道（`lockedm` が無いとき）と `tsuzuri_tz_yield`。後者は `Routine.yield`・`tsuzuri_tz_preempt_check`・SIGURG の再開から）と G の終わり（`tzr_after_exit`）だけ。
  ブロッキング呼び出し（`tsuzuri_tz_blocking_enter`・`_exit`）は P を手放すが G は M に残り、`tsuzuri_tz_callback_enter` は G を M に固定する。
- spawn しないプログラムで生成コードを G の上で走らせるランタイムの入口は `tsuzuri_tz_parallel` だけ: `tzr_spawn` を呼ぶのは `tsuzuri_tz_go`（`Routine.go`・`tzroutine { }`・`WaitGroup.go`、`ErrGroup.go` は `WaitGroup.go` を使う）、`tsuzuri_tz_parallel`、
  `tsuzuri_tz_scope_run`（`Routine.scope`）、タイマーの `after_func`（`Time.after_func`・`Context.after_func`）で、`tsuzuri_tz_parallel` 以外はどれも `Builtin::spawns`（`RoutineGo`・`WaitGroupGo`・`RoutineScope`、`Time.sleep` 以外のタイマー、`Context`）か `RoutineGo` の式で `spawns(module)` を真にする。
- 前提の外（前からのことで、TZ14 は変えない）: 最上位の `Parallel.*` の呼び出し側が擬似 G なのは、実行ファイルとホストのスレッドから呼ぶとき。spawn する別の Tsuzuri の object を同じ実行ファイルにリンクし、その tzroutine のブロッキングの extern 呼び出しの中から、
  extern を持たない（したがって `tsuzuri_tz_callback_enter` の包みが無い）この object の export を呼ぶと、`tzr_current` はその G を返す。その G は P を手放している（`tzr_entersyscall`）ので、そこでの待ちはランタイムが前から扱っていない道である。

### 検証（2026-11-06）

- 単体テスト（`src/llvm_preempt.rs`）: `thread_storage_stays_out_of_line_without_the_checks` を、待ちうるチャンク（`tsuzuri_tz_yield` を呼ぶ関数を呼ぶ）を `@tsuzuri_tz_parallel` に渡す IR にした。
  `helpers_that_cannot_wait_keep_the_ir_of_main`: 確保・`llvm.memset`・`malloc`・`free`、`}; ...` の後の関数を呼ぶチャンクは偽で IR が同一。入れ子の `Parallel.*` に届く関数、ポインターを通す呼び出し、インラインのアセンブリ、未知の外部関数、`invoke`、
  `@tsuzuri_tz_parallel` の `store`、定数でない第 1 引数（`%chunk`）、定義の無い第 1 引数はどれも真で、その理由の行と、`@tz.cache` の見出しの ` noinline` を確かめる。
- `tests/allocator.rs` の `parallel_programs_keep_thread_storage_out_of_callbacks`: 期待値は `heap-small.ll` の見出しと規則から作る。tzroutine の無い版と、コールバックが `Vec` を伸ばすだけの `FLAT_GROWTH` は見出しのまま、
  入れ子の `Parallel.init` の `NESTED_GROWTH` は ` noinline`、spawn する 2 つの版（`FLAT_GROWTH`・`NESTED_GROWTH` に `tzroutine { }`）は ` noinline`（TZ10）。
- `tests/small_allocator.mjs` の `nested`: 実行の回帰はそのまま。その IR の `tz_small_realloc` の見出しが ` noinline` であることを足した。
- IR の比較: `main`（`src/` を `72f06862` に戻して build した dev のコンパイラ）と変更後の dev のコンパイラで、`tests/fixtures/` と `examples/` の `Main.tz` を持つ 180 の project を `-O0` の `build --emit llvm --no-cache`、既定の確保器と `--allocator small` で出した（360 個）。
  build できない 12 個（`bindgen/basic`・`bindgen/extended`・`domain`・`domain_host`・`llvm_handoff/dead_os`・`parallel_report/error` の 2 通り）は前後で同じ診断。333 個が byte 単位で同じ（既定の確保器のすべて、spawn する 4 つ（`examples/tzroutines`・`concurrency`・`parallel_scheduler`・`tzroutine_wasm`）、
  手伝いが待てない 11 個（`const_statics`・`cpu_dispatch`・`debug_info`・`heap_threads`・`matrix`・`monoid`・`parallel_equivalence`・`parallel_report/rules`・`pure_parallel`・`record_layout`・`value_layout`）を含む）。
  違う 15 個は、どれも spawn せず `Parallel.*` を呼び手伝いが待ちうる `--allocator small` の IR で、`tz_small_realloc` の見出しの ` noinline` の 1 行だけが違う。理由（`helper_wait` の行）:
  `arena`・`dyn_dispatch`・`trap_return_sync`（drop の `%destroy` を通す呼び出し）、`concurrency_threads`・`rc`・`recursive_types`（`%action` を通す呼び出し）、`parallel`・`mutable_slices`・`strings`（クロージャを通す呼び出し）、
  `bindings_threads`・`data_channel_threads`（`tsuzuri_channel_close`）、`host_imports`・`user_drop`（extern の `tsuzuri_tz_blocking_enter`）、`parallel_report/basic`（`write`）、`trap_return`（入れ子の `Parallel.init`）。
  手順 4 の核は `-O0` と `-O3` の `--emit llvm --allocator small` で `main` と同じ。
- 実行したコマンドと結果: 下の「最終確認」。

### 最終確認（2026-11-06）

`cargo fmt --all -- --check`（成功）、`cargo clippy --locked --all-targets -- -D warnings`（警告なし）、`cargo test --locked --lib preempt`（9 件成功）、`cargo test --locked --test allocator --test preemption`（10 件・6 件成功）、
`cargo nextest run --locked --no-fail-fast`（2937 件のうち 2936 件成功、4 件 skip。`formatter::corpus_is_idempotent_and_semantically_unchanged` が、同時に走ったほかのテストが木の中のファイルを消した `NotFound` で 1 回失敗し、単独の再実行で成功）、
`node tests/small_allocator.mjs target/debug/tsuzuri`（単体テストの O0・O3・ASan + UBSan・TSan、`grow`・`primitives`・`threaded`・`nested`（`-O0`・`-O3` × 既定の P・P 8、IR の ` noinline`）、2 つの object の共有。すべて成功）、
`TSUZURI_TEST_ALLOCATOR=small node tests/features.mjs target/debug/tsuzuri <suite>`（`parallel` 36 件・`parallel_equivalence` 48 件・`pure_parallel` 12 件、すべて成功）、上の IR の比較、
`node scripts/check-guide-lite.mjs`・`node scripts/gen-docs.mjs --check`・`node scripts/check-cli-docs.mjs`（成功）。言語リファレンスのページは変えていない（利用者から見える振る舞いは変わらない）。計時はしていない（測定は上の orchestrator のもの）。
文書: `docs/architecture.md`（TZ10 の `noinline` の段落と `Small` の `tz_small_realloc`）、`docs/benchmarks.md`（TZ14 の節）、`_features/_completed/TZ10-preemption.md`（「残したこと」）、`_features/README.md`（TZ14 の行）。
