# TZ14: spawn しないプログラムの `Parallel.*` でも、スレッドの記憶域を読むランタイムの IR 関数を `noinline` にする（`--allocator small` の cache と G の移動）

| 項目 | 内容 |
| --- | --- |
| ID | TZ14 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | TZ10, TZ11 |
| 後続 | – |
| 状態 | todo |
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

- [ ] 手順 1 の調査結果がチケットに追記されている。
- [ ] spawn せず `Parallel.*` を呼ぶネイティブのプログラムで、`tz_small_realloc` が `noinline` になる。
- [ ] spawn するプログラムと、tzroutine のランタイムを使わないプログラムの IR が修正の前後で同じ。
- [ ] 手順 4 の測定が 1 % 以内で、`docs/benchmarks.md` に記録されている。
- [ ] T2 が成功する。

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
