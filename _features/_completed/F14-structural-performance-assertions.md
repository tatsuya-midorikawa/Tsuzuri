# F14: 構造的な性能表明（確保数のテスト用 API と `bench` の確保数）

| 項目 | 内容 |
| --- | --- |
| ID | F14 |
| 優先度 | P1 |
| 規模 | L（Phase 1: M、Phase 2: S、Phase 3: S）。種は M と見積もったが、`Alloc` の実装（ビルトイン・std・heap の連結・到達の制限・CLI）、`bench` の計数、42 件の契約を合わせると L になる。Phase は独立に出荷できる |
| 依存 | [F13](F13-custom-allocators.md)（done。`--allocator counting`・`src/runtime/heap-counting.ll`・`tsuzuri_alloc_stats`）、[G18](G18-bench-coverage.md)（done。`tsuzuri bench`・`src/runtime/bench-runner.c`）。Phase 3 は Phase 1 |
| 後続 | A21（契約の `test` モードが `Alloc.snapshot`／`since` で `@no_alloc` の主張を実行時に確かめられる）、G31（AI 作業コスト指標。確保数は決定的な指標）、確保を減らす性能チケット（退行が「確保数が増えた」というテスト失敗になる） |
| 状態 | done（Phase 1・2・3。下記の実装・検証記録を参照） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `main` の `9dd0fbbc`で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D10 はすべて承認済み |
| 改善する劣位 | 「確保しない」「償却で対数」といった構造的な性質は、時間では測れず（共有 CI に速度の合否の閾値を置かない。AGENTS.md）、HEAD にはテストで確かめる手段がない。Go の `testing.AllocsPerRun`・.NET の `GC.GetAllocatedBytesForCurrentThread`・Rust の `allocation-counter` crate（付録 F）に当たる手段がなく、`--allocator counting` は native 実行ファイルと `tsuzuri test` で使えない。最後の使用のコピー除去（A17）のような最適化は、退行しても結果が同じで時間だけが変わり、見つからない |
| 手本にする既存実装 | 計数 heap: `src/runtime/heap-counting.ll`、`src/llvm.rs` の `counted_base`・`emit_program_in`、`src/llvm_abi.rs` の `ALLOCATION_STATS_PROTOTYPE`。専用の組み込み関数: `Builtin::BenchNow`・`Builtin::GenSeed`（`src/check.rs`、`src/llvm.rs` の `emit_builtin`、`src/polymorph.rs` の `Checker::builtin` の `E1022`）。出力を限る到達解析: `src/llvm.rs` の `references`・`direct_references`・`close`・`program_reach`・`reaching`・`clock_error`・`clock_use`と、`tests/bench.rs` の `bench_now_outside_bench_runner_is_rejected`・`public_bench_helpers_stay_out_of_other_builds`・`clock_reached_from_entry_export_test_or_drop_is_rejected_by_name`。opt-in の std: `src/stdlib.rs` の `OPT_IN`、`std/Bench.tz`。bench: `src/test_runner.rs` の `run_benches_linked`・`execute_bench`・`parse_bench_output`、`src/runtime/bench-runner.c`、`src/main.rs` の `run_bench_action`。harness: `tests/test_runner.rs`、`tests/allocator.rs`・`tests/allocator.mjs` |
| 主な影響ファイル | Phase 1: （新規）`std/Alloc.tz`、`tests/alloc.rs`。変更: `src/runtime/heap-counting.ll`、`src/check.rs`、`src/polymorph.rs`、`src/llvm.rs`、`src/stdlib.rs`、`src/driver.rs`、`src/test_runner.rs`、`src/main.rs`、既存テスト 3 件（下の「既存テストへの影響」）、文書。Phase 2: `src/runtime/bench-runner.c`、`src/llvm.rs`、`src/test_runner.rs`、`src/main.rs`、`tests/bench.rs`。Phase 3: （新規）`tests/fixtures/alloc_contracts/Main.tz`、`tests/alloc_contracts.rs`。文書: 「ドキュメント」の表 |

付録（fixture・`std/Alloc.tz`・`heap-counting.ll` の全文、実測、出典、試作）は [F14-appendix.md](F14-appendix.md)。

## 実装者向け早見

- 作るもの: (1) std の `Alloc`（`Alloc.Stats`・`snapshot`・`since`・`measure`）。確保・再確保・解放の回数と要求バイト数を、テストの中で読む。(2) `--allocator counting` を native 実行ファイル・`tsuzuri run`・`tsuzuri test` で使えるようにし、`tsuzuri test` は `Alloc` に届くときだけ counting の heap を自動で連結する。(3) `tsuzuri bench --allocs`（1 反復あたりの `alloc_calls`・`alloc_bytes`）。(4) std の確保の契約 42 件（付録 A をそのまま置く）。
- 最初に読む（この順）: `src/runtime/heap-counting.ll`（105 行）→ `src/llvm.rs` の `emit_program_in` の `runtime_symbols` から `match allocator` まで → `std/Bench.tz` と `src/llvm.rs` の `clock_error` → `tests/bench.rs` の `bench_now_outside_bench_runner_is_rejected` → `tests/test_runner.rs` の先頭の関数。
- 最初に書くテスト: 手順 1.2 の `tests/alloc.rs`（`#[ignore = "F14 手順 N"]` つきで先に全部書き、対応する手順で外す）。Phase 3 の fixture は付録 A にそのまま書いてある（写して 4 構成で通す）。
- 触ってはいけない: `tsuzuri_alloc_stats` の構造体（32 バイト・4 フィールド・順序。`ALLOCATION_STATS_PROTOTYPE`）／`Alloc` を使わないプログラムの IR・object・`.wasm`（1 byte も変えない。手順 1.1 で保存した基準と `cmp` する）／`Bench.now` の `E1018` の文面と位置（`tests/bench.rs` の既存テストが見る）／WASM の import（既定は import なし）／`--allocator host`・`small` の挙動。
- 名前・型・値はすべてこのチケットで決めてある: `Alloc.Stats { allocs, reallocs, frees, bytes }`・`Alloc.snapshot`・`Alloc.since`・`Alloc.measure`・ビルトイン `Alloc.__counter`・`@tz.heap.counter`・`--allocs`・JSON のキー `alloc_calls`／`alloc_bytes`。発明しない。
- 診断コードは新しく足さない: 到達の制限は `E1018`、std の外からのビルトインは `E1022`、組み合わせの誤りは `E2000`、bench の失敗は `E2005`。
- Phase の完了の見分け方: Phase 1 = `tsuzuri test` で `Alloc` を使うテストが native・wasm32 の `-O0`／`-O3` で通り、`tsuzuri run --allocator counting` が `90` を出し、`--allocator` なしの `Alloc` つきの `build` が `E1018`。Phase 2 = `tsuzuri bench --allocs --json` の行に `alloc_calls`・`alloc_bytes` が付き、付けないと行は 1 byte も変わらない。Phase 3 = `cargo test --locked --test alloc_contracts` が 2 件通る。
- 迷ったら: 「決定事項」D1〜D10 → 最も保守的な選択（拒否）の順。止める条件は「着手条件と停止条件」。

## 目的

Tsuzuri は確保の場所を言語が約束する（`new` の有無・最後の使用の move・`Vec` の容量の倍増）が、約束どおりに動いているかをテストで確かめる手段がない。
最適化は、退行しても結果が同じで確保だけが増える。共有 CI に速度の合否の閾値は置けない（AGENTS.md）ので、その退行は見つからない。確保の回数は、単一スレッドなら決定的なので、時間の代わりに合否に使える。

HEAD で測った事実（付録 D）:

- `Vec.push` を 100 回すると、確保 1 回・再確保 5 回・要求 2,016 byte。既存のカウンターは再確保を数えず、`tsuzuri_alloc_stats` の `allocations` は 1 に見える（D.2）。
- ループの `xs = Array.update_at i v xs` は、その関数のどこかで `xs` を借用する（`Array.length (ref xs)`・補間の穴 `{xs[99]}`）だけで、更新ごとに配列全体を複製する（100 回の更新で確保 100 回・80,000 byte。借用しなければ 0）。規則どおり（`ownership.md`）だが、時間のテストでは見つけにくい（D.5）。
- `Array.sort` は n=100 で 318 回確保する（`Array.sort_in_place` は 0 回）。捕捉つきクロージャを `Array.map` に渡すと、wasm32 だけ環境の確保が 1 回増える（D.5）。

提供するもの: (1) テストの中で確保を数える `Alloc`。(2) そのために `--allocator counting` を native 実行ファイル・`run`・`test` で使えるようにする。(3) `tsuzuri bench --allocs`（1 反復あたりの確保数）。(4) std の構造的な性質 42 件の契約スイート。退行は「確保数が増えた」というテストの失敗になり、時間の閾値は使わない。

非目標: 確保の呼び出し位置の報告（G26 の facts `alloc_sites` の仕事）／リーク検出（`Stats` は差分だけで、`live`・`peak` は出さない）／時間の計測への使用（計数 heap は確保ごとに原子演算 4 回とヘッダーが付く。`bench` が別の実行器で数える理由）／std の実装の改善（`Array.sort` の確保。見つけた事実を付録 D に残すだけ）／Task をまたぐ独立した計数（カウンターはプロセス全体）。

## 着手条件と停止条件

### 着手条件

- F13・G18 が `_features/README.md` で done（HEAD で done）。確認: `grep -nE "^\| (F13|G18) " _features/README.md`。
- 承認は済み（D-50）。新しい予約語・crate・`unsafe`・既定の WASM の import はない。
- GUIDE §2.3 の基準コマンド `cargo build --release --locked`・`cargo test --locked` が通る状態から始める（手順 1.1 で件数を控える）。
- 読むもの: GUIDE §6.5（組み込み関数）・§6.6（ランタイム）・§6.8（CLI）・§11.1、`docs/stability.md` の「既知の例外」（std へのモジュールの追加は `E1011` になりうる）、`_tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md` と `ownership.md`（付録 A の規則の出典）。
- Node 20 以降、clang・wasm-ld（`tsuzuri test --target wasm32` が使う）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `Alloc` を使わないプログラムの IR・object・`.wasm`（手順 1.1 の基準）が 1 byte でも変わった。`@tz.heap.counter` を `Alloc` に届くときだけ連結する入れ方に直す。直らなければ報告する。
2. `tests/allocator.mjs` の `tsuzuri_alloc_stats` の値（allocations・frees・live・peak）や構造体が変わった。ABI は変えない。`@tz.heap.counts` の添字の対応を直す。
3. 付録 A の fixture が 4 構成（native／wasm32 × `-O0`／`-O3`）のどれかで通らない。**期待値を書き換えて通さない。**実測と規則の食い違いを報告する。選択肢: A（std・コンパイラの退行として直す。別チケット）、B（その性質を契約から外し、理由を決定事項に書く）。
4. `-O0` と `-O3`、または native と wasm32 で、ポインターを含まない型の確保数が違う。
5. `Entry::TestRunner` に counting を連結すると、WASM の import が増える、または `with_wasm_heap_limit` の置換が効かなくなる。
6. `Bench.now` の `E1018` の文面・位置が変わる（`tests/bench.rs` の既存テストが失敗する）。到達解析の一般化が既存の振る舞いを変えている。
7. `Builtin` の `match` の網羅の誤り（`cargo check` が知らせる）が、手順 1.4 に書いた場所の外にも出た。機械的に直せるなら直して手順に追記し、意味の判断が要るなら止める。
8. 手順 2.4 の差分（2 反復 − 1 反復）が、決定的な本体で負になる、または 2 回の実行で値が変わる。
9. Windows の CI（`windows.yml`）が計数つきの native 実行ファイルやテスト実行器で落ちる（手元では確かめられない。結果を報告する）。
10. `unsafe`・新しい crate・新しい予約語・`tsuzuri_alloc_stats` の構造体の変更が必要に見える。同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

### なぜ `--allocator counting` は native 実行ファイルと `tsuzuri test` で使えないか

| # | 理由 | 場所 |
| --- | --- | --- |
| R1 | `build --allocator counting` は、出力が実行ファイル（と GPU カーネル）なら `E2000`。理由の文面は「ホストが `tsuzuri_alloc_stats` で数を読む」 | `src/driver.rs` の `BuildOptions::validate_allocator`。F13 の決定の追記 3「exe は数を読む相手がいないので `E2000`」 |
| R2 | `--allocator` は `build` だけが受ける。`run`・`test`・`check` は `E2000` `--allocator is only valid with build` | `src/main.rs` の `parse_arguments`（`allocator.is_some() && action != Action::Build`） |
| R3 | テスト実行器に allocator を渡す経路がない。`TestOptions`・`llvm::TestRunnerOptions` に欄がなく、`emit_test_runner_with` は `Instrumentation::default()`（`allocator` は `System`）で `emit_program` を呼ぶ | `src/test_runner.rs`、`src/llvm.rs` |
| R4 | 数を読む手段は C の ABI `tsuzuri_alloc_stats(ptr)` だけで、Tsuzuri のコードから読めない。実行ファイルとテスト実行器（`src/runtime/test-runner.c` の `main` は `tsuzuri_test_run(index)` を呼ぶだけ）には、読むホストがいない | `src/runtime/heap-counting.ll`、`src/runtime/test-runner.c` |
| R5 | 読めても数が足りない。カウンターは確保・解放・現在量・最大量で、再確保（`Vec.push` の容量の倍増）と要求バイトの累計がない | `src/runtime/heap-counting.ll`（`[4 x i64]`）、付録 D.2 |

native の実行ファイルで counting の heap が動くこと自体には障害がない。native の object（`--emit object --allocator counting`）は `tests/allocator.mjs` が C のホストと `-O0`／`-O3` で動かしており、
実行ファイルは同じ IR に `@main` が付くだけである。試作（付録 G）で、`run --allocator counting`・`build --allocator counting` の実行ファイル・`test --allocator counting`（native と wasm32、`-O0`／`-O3`）がすべて動いた。

### 再現コマンド（HEAD の `target/release/tsuzuri`）

```sh
mkdir -p target/tz-F14-try && printf 'let xs = new [i64](10, \\i -> i)\nArray.length (ref xs)\n' > target/tz-F14-try/Main.tz
target/release/tsuzuri build target/tz-F14-try --allocator counting -o target/tz-F14-try/x
# <command line>:1:1: error[E2000]: --allocator counting requires object, LLVM IR, header, or WebAssembly output; the host reads the counts with tsuzuri_alloc_stats   （終了コード 2）
target/release/tsuzuri test target/tz-F14-try --allocator counting
# <command line>:1:1: error[E2000]: --allocator is only valid with build   （終了コード 2）
target/release/tsuzuri build target/tz-F14-try --emit llvm --allocator counting -o target/tz-F14-try/x.ll && grep -n "tz.heap.counts =" target/tz-F14-try/x.ll
# @tz.heap.counts = internal global [4 x i64] zeroinitializer, align 8
```

### 関連する HEAD の事実

- `tsuzuri test` は native では**テストごとに別プロセス**で動く（`src/test_runner.rs` の `execute_test` が `tests.exe <index>` を起動）。wasm32 は `src/runtime/test-runner.mjs` が実行する。本設計は差分（スナップショットの引き算）で数えるので、開始時の値に依らない。
- `emit_program_in` は、生成した本体の `output` から `RuntimeSymbols::collect` で `@` の名前を集め、`runtime_symbols.prefix("tz.closure.")` のように**参照されたランタイムだけを連結する**（`src/runtime_cache.rs`）。heap の選択は末尾の `match allocator`。`Counting` は `counted_base(&heap)` で対象の heap の `@tz.alloc`・`@tz.free`・`@tz.realloc` を `.base` に改名し、`heap-counting.ll` を足す。
- `Bench.now` は `Builtin::BenchNow` で、`tsuzuri bench` の実行器以外が到達すると `E1018`（`clock_error`）。公開の補助関数が時計に届くだけでは根にしない（`program_reach`）。`tests/bench.rs` に 3 つの手本のテストがある。
- `tsuzuri bench` の `--json` の行は `type・index・module・name・status・workload・target・opt・cpu_mode・metric・unit・iterations・samples・median・min・max` の順（`_tsuzuri/language-reference/built-in-types-and-modules/bench.md`）。text の行の例（実測。値は機械で変わる。`iterations` は時間で決まるので実行ごとに変わる）: `bench 1 Main.map one array: median 30.887 ns (min 26.302 ns, max 33.624 ns; 3 samples of 524288 iterations)`。
- `docs/stability.md`「既知の例外」は、std へのモジュールの追加で同名の利用者のモジュールが `E1011` になることを、release notes で予告する扱いにしている。

## 変えてはいけない意味

- `Alloc` を名前に書かないプログラムの IR・object・`.wasm`・診断は 1 byte も変わらない（`Alloc` は opt-in の std モジュール。heap-counting は `@tz.heap.counter` に届くときか `--allocator counting` のときだけ連結する）。手順ごとに基準と `cmp` する。
- `--allocator counting` の既存の出力（object・LLVM IR・header・WASM）の `tsuzuri_alloc_stats` の値と構造体 `tsuzuri_allocation_stats { allocations, frees, live_bytes, peak_bytes }`（各 8 byte）は変えない。IR の本文は変わる（カウンターが 2 つ増える。IR は安定の対象外。`docs/stability.md`）。
- `--allocator system`・`host`・`small` の IR・挙動、`--trap-mode return` との併用の拒否は変えない。
- `Bench.now` の `E1018` の文面・位置・根の除外の振る舞いは変えない（到達解析を一般化するが、既存の 3 つのテストが緑のまま）。
- `tsuzuri bench` の既定の出力（`--allocs` なし）は、text も JSON も 1 byte も変えない。
- 診断コードの意味は変えない。`E1018`・`E1022`・`E2000`・`E2005` は既存の意味の中で使う。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `Alloc`（`Stats`・`snapshot`・`since`・`measure`）、計数 heap のカウンター追加、`tsuzuri test` の自動の counting、`--allocator counting` の実行ファイル・`run`・`test` への対応、到達の制限（`E1018`） | M | F13 | `Alloc` を使わないプログラムは何も変わらない。新機能は `Alloc` と `--allocator counting` を書いたときだけ働く |
| 2 | `tsuzuri bench --allocs`（JSON の `alloc_calls`・`alloc_bytes`、text） | S | Phase 1（`@tz.heap.counter` とカウンター） | `--allocs` を付けない `bench` の出力は 1 byte も変わらない |
| 3 | std の確保の契約 42 件（fixture と harness）、文書の表 | S | Phase 1 | テストと文書だけ。コンパイラの挙動を変えない |

Phase 1 の手順 1.1〜1.9 は 1 手順ずつ緑で、途中で止めても既定の挙動は変わらない（手順 1.5 までは `Alloc` が使えず、手順 1.6 で `tsuzuri test` の中だけで使えるようになり、手順 1.7 で CLI が開く）。

## 仕様

### `Alloc`（opt-in の std モジュール。ソースは付録 B）

`std/Alloc.tz` を足し、`Alloc` を名前に書いたプログラムだけが読み込む（`src/stdlib.rs` の `OPT_IN`）。

```tsuzuri
record Stats { allocs: i64, reallocs: i64, frees: i64, bytes: i64 }   // Alloc.Stats
def snapshot :: unit -> Stats                                          // Alloc.snapshot ()
def since :: Stats -> Stats                                            // Alloc.since start
def measure :: (unit -> 'a) -> Stats * 'a                              // Alloc.measure (\() -> ...)
```

- `Alloc.snapshot ()` は、プログラムの開始からの累計を `Stats` で返す。`Alloc.since start` は、`start`（それより前の `snapshot`）から今までの差を、フィールドごとの引き算で返す。`Alloc.measure f` は `f ()` を 1 回呼び、`(since start, 結果)` を返す。3 つとも確保しない。
- 窓（`snapshot` から `since` まで）は入れ子にできる（外の窓は内の窓を含む）。累計なので、`snapshot` を 2 回取って自分で引いてもよい。
- 数える出来事（`@tz.alloc`／`@tz.free`／`@tz.realloc` の呼び出し。`tsuzuri_alloc` も含む）:

| 出来事 | `allocs` | `reallocs` | `frees` | `bytes` |
| --- | --- | --- | --- | --- |
| `@tz.alloc(size)` | +1 | | | +size |
| `@tz.realloc(null, _, new)`（`new > 0`。`@tz.alloc` になる） | +1 | | | +new |
| `@tz.realloc(生きているブロック, _, new)`（`new > 0`。場所が変わってもよい） | | +1 | | +new |
| `@tz.free(ブロック)`、`@tz.realloc(ブロック, _, 0)` | | | +1 | |
| `@tz.free(null)` | （何もしない） | | | |

- `bytes` は要求バイト数の累計（`@tz.alloc` は `size`、再確保は新しい大きさ）。16 バイトのヘッダーは含めない。解放は引かない（累計であり、同時に生きている量ではない）。PX01 の `alloc_bytes` と同じ定義。
- 値は `i64`。カウンターは 64 ビットの符号なしで、`i64` として返す（2^63 を超えると負になるが、実際には起きない）。
- **閉包に注意**: `Alloc.measure` の閉包が捕捉した所有値は、閉包の中では一意でない。`Alloc.measure (\() -> Array.update_at 0 5 xs)` は `xs` を複製する（`allocs 1`）が、`snapshot`／`since` なら 0。所有値を消費する操作は `snapshot`／`since` で測る。`Alloc.measure` の閉包の環境は呼び出しの前に作られるので窓の外。
- 決定性: 単一スレッドなら、同じプログラムは常に同じ値になる（時刻・アドレス・順序に依らない）。`Task`・`Parallel` のスレッドの確保も、プロセス全体のカウンターに入る（実行器の内部の確保を含み、回数は実装の詳細）。
- ビルトイン `Alloc.__counter :: i64 -> i64` は `std/Alloc.tz` だけが呼べる（付録 B）。0 `allocs`、1 `reallocs`、2 `frees`、3 `bytes`、4 現在量、5 最大量。5 を超えるとトラップ（`@tz.heap.counter`）。std の外から呼ぶと `E1022`。

### どこで `Alloc` が使えるか（決定表）

`Alloc.snapshot`・`since`・`measure` の本体は `@tz.heap.counter` を呼ぶ。出力ごとの扱い:

| 出力 | `--allocator` | `Alloc` に到達 | 結果 |
| --- | --- | --- | --- |
| `tsuzuri test`（native・wasm32・`--coverage`・`-g`） | なし／`counting` | する | counting の heap を**自動で連結**する。受理 |
| `build`・`run`（実行ファイル）、`build --emit object\|llvm\|shared`、WASM | `counting` | する | 受理 |
| 同上 | なし／`system` | する | `E1018`（下の文面） |
| 同上 | `host`・`small` | する | `E2000`（`Alloc needs the counting allocator; …`） |
| `tsuzuri bench`（時間の実行器） | — | する | `E1018`（助言は `--allocs`） |
| `tsuzuri bench --allocs` の計数用の実行器 | （内部で `counting`） | する | 受理（本体が `Alloc` を使う場合） |
| `script`・`repl` | — | する | `E1018`（これらは `--allocator` を受けない） |
| `tsuzuri check`・`doc`・`lsp` | — | — | 型検査だけ。到達の検査は IR を作るときだけ（`Bench.now` と同じ） |

**根の除外**（`Bench.now` と同じ規則）: 根を指定しない出力（Library・Console。`export`・入口・公開の利用者関数が根）では、`Alloc` に届く公開の補助関数は、`export` でも入口でもなければ根にしない。
テスト専用の補助関数を公開で書いても、通常のビルドは失敗せず、counting も連結されない。根の候補が `Alloc` に届かないプログラムでは、到達の計算も出力も変わらない。

### CLI（`--allocator counting`）

| コマンド | 結果 |
| --- | --- |
| `build`（実行ファイル・object・llvm・header・shared・wasm32／wasm64） | 受理。実行ファイルは**新規**（HEAD は `E2000`） |
| `build --emit wgsl\|wgsl-relaxed\|spirv\|spirv-relaxed` | `E2000` `--allocator counting requires executable, object, shared library, LLVM IR, header, or WebAssembly output` |
| `build --trap-mode return`、bindings 出力 | 既存の `E2000`（変えない） |
| `run` | **新規に受理**（実行ファイルを counting でビルドして実行する） |
| `test`（`--target wasm32\|wasm64`・`--coverage`・`-g` を含む） | **新規に受理**（`Alloc` を使わなくても runner に counting を連結する） |
| `bench`・`check`・`doc`・`fmt`・`script`・`repl`・`lsp` | 既存の `E2000` `--allocator is only valid with build`（変えない） |
| `run`・`test` の `--allocator system\|host\|small` | 既存の `E2000` `--allocator is only valid with build`（変えない。`test`／`run` は `counting` だけを受ける） |

終了コードと出力は既存と同じ（コマンドラインの誤りは 2）。`tsuzuri test --allocator counting` は、`Alloc` を使わないテストを counting の heap で動かす（動作の確認用）。リークの検査はしない。

### 診断

新しいコードは足さない。

| コード | kind（G22 以降） | 条件 | メッセージ（英語） | 位置 |
| --- | --- | --- | --- | --- |
| `E1018` | `alloc_needs_counting_allocator` | `Alloc` に、counting でない出力（上の表）が到達する | 下の文面 | 起点から `Alloc` への最短の経路で、最後に通る利用者の関数の中の、次へ向かう式（`Bench.now` の `clock_error` と同じ規則） |
| `E1022` | （既存） | std の `Alloc` モジュールの外から `Alloc.__counter` を呼ぶ | `the allocation counters are private to the standard Alloc module; read them with Alloc.snapshot or Alloc.since` | 呼び出しの式 |
| `E2000` | （既存） | `Alloc` に届き、`--allocator host` か `small` | `Alloc needs the counting allocator; remove --allocator host or --allocator small, or build with --allocator counting` | なし（`Span::default()`） |
| `E2000` | （既存） | `build --emit <カーネル> --allocator counting` | 上の CLI の表の文面 | なし |
| `E2000` | （既存） | `--allocs` を `bench` 以外に付ける | `--allocs is only valid with bench` | なし（コマンドライン） |
| `E2005` | （既存） | `--allocs` の計数が失敗した bench（トラップ・時間切れ・負の数） | `benchmark 'Main.name' allocation run: <理由>` | その bench の宣言（既存の失敗と同じ） |

`E1018` の文面の型（`RULE`・`ADVICE` の 2 つの定数と、`clock_error` の 2 つの形）:

```text
RULE   = Alloc reads the counters of the counting allocator, which only tsuzuri test and builds with --allocator counting link
ADVICE = build with --allocator counting, or call it only from tests and from functions that only tests use
ADVICE（bench の実行器のとき）= count the allocations of a bench with tsuzuri bench --allocs instead
利用者の関数を通るとき: {RULE}, but {root} reaches it through `{name}`; {ADVICE}
通らないとき:          {RULE}, but {root} reads it; {ADVICE}
```

`{root}` は次の 5 つの形のどれか（`clock_error` の `root_name` と同じ）。`{name}` は `Module.name`。

```text
the top-level code
test "<name>"
export `<Module.name>`
`<Module.name>`
the Drop instance of `<Type>`
```
例（想定出力。位置は例）:

```text
Main.tz:7:18: error[E1018]: Alloc reads the counters of the counting allocator, which only tsuzuri test and builds with --allocator counting link, but the top-level code reaches it through `Main.report`; build with --allocator counting, or call it only from tests and from functions that only tests use
Main.tz:4:30: error[E1018]: Alloc reads the counters of the counting allocator, which only tsuzuri test and builds with --allocator counting link, but export `Main.total_allocs` reads it; build with --allocator counting, or call it only from tests and from functions that only tests use
Main.tz:9:34: error[E1018]: Alloc reads the counters of the counting allocator, which only tsuzuri test and builds with --allocator counting link, but the top-level code reaches it through `Main.helper`; count the allocations of a bench with tsuzuri bench --allocs instead   （tsuzuri bench の実行器）
Main.tz:3:17: error[E1022]: the allocation counters are private to the standard Alloc module; read them with Alloc.snapshot or Alloc.since
<command line>:1:1: error[E2000]: --allocator counting requires executable, object, shared library, LLVM IR, header, or WebAssembly output
```

G22 が入ったら、`E1018` に `kind: "alloc_needs_counting_allocator"`、`help`（`build with --allocator counting`）を足す。HEAD にはこれらがないので付けない。

### `tsuzuri bench --allocs`

- フラグ `--allocs`（`bench` だけ。付けない `bench` の出力は 1 byte も変わらない）。付けると、bench ごとに、時間の実行器の後で、**別の計数用の実行器**（counting の heap・`-DTSUZURI_BENCH_ALLOCS` の C）を 1 回走らせる。時間の実行器は変えない（計数が時間を乱さない）。
- 計数用の実行器は、bench `index` について次を行う。(1) 予熱として 1 反復で `tsuzuri_bench_sample(index, 1)` を呼ぶ。(2) カウンターを読み、1 反復で呼び、カウンターを読む（増分 A）。(3) 2 反復で呼び、カウンターを読む（増分 B）。**1 反復あたり = B − A**（`Bench.with_input` の準備と解放は A にも B にも入って消える）。`alloc_calls` は `allocs + reallocs` の B − A、`alloc_bytes` は `bytes` の B − A。
- 反復回数（時間で決まる）に依らず決定的。本体が反復ごとに同じ確保をするなら、1 反復の値と一致する。償却される構造（反復をまたいで伸びる `Vec`）では「2 回目の反復の分」になる。
- 本体が Alloc を使うなら、計数用の実行器では受理する（上の表）。
- JSON: 既存のキーの後ろ（`max` の後）に `"alloc_calls":<整数>,"alloc_bytes":<整数>` を足す。失敗した bench と `summary` の行は変えない。text: 既存の行の末尾に `, alloc_calls <n>, alloc_bytes <n> per iteration` を足す。

```text
bench 1 Main.one array of ten: median 30.284 ns (min 29.261 ns, max 32.408 ns; 3 samples of 524288 iterations), alloc_calls 1, alloc_bytes 80 per iteration
{"type":"bench","index":1,"module":"Main","name":"one array of ten","status":"passed","workload":"Main.one array of ten","target":"native","opt":"O3","cpu_mode":"generic","metric":"wall_time","unit":"ms","iterations":524288,"samples":[…],"median":…,"min":…,"max":…,"alloc_calls":1,"alloc_bytes":80}
```

（想定出力。付録のソース `bench2` の 5 件の期待値: `nothing allocated` 0／0、`one array of ten` 1／80、`map over a shared input` 1／80、`sum over a shared input` 0／0、`five pushes` 2／96。）
計数用の実行器は LLVM が確保を消せない形（原子的な加算を伴う呼び出し）で作るので、`alloc_calls` は言語の規則による確保の数で、時間の実行器で LLVM が確保ごと消した本体（`five pushes` の中央値が 0.18 ns）の確保も数える。

### std の確保の契約（Phase 3）

fixture は付録 A の 42 件。形の規則:

- 1 テストは窓 1〜2 個。窓の中には測る操作だけを書き、`Debug.print`・文字列補間・`ref` の借用を測る対象に足さない（窓の中の確保や、最後の使用の判定を変えるため）。
- 期待値は、各テストの直前の `// Rule:` に書いた規則（付録 E の出典）から決める。コンパイラの出力を写さない。
- `bytes` は、要素の型にポインターを含まない（`i64`・`i32`・UTF-16 の文字列）ものだけを検査する（`expect`）。ポインターの大きさに依るもの（再帰 union のノード・`Rc` のブロック・`Vec<[i64]>` の要素）は回数だけ（`expect_events`）。
- native と wasm32、`-O0` と `-O3` のすべてで同じ値。最適化で変わる性質は入れない（HEAD の 42 件に `-O3` だけの性質はない。付録 D.3）。
- 入れないもの: `Array.sort` の確保（規則がない）、捕捉つきクロージャの環境（target に依る）、`Map`・`Set`・`HashMap`・`Task`・`Parallel`。
- 値を変える変更（std の実装の変更）は、その変更と同じ PR で fixture と文書の表を直す。

### 互換性

- edition・構文・予約語の変更はない（`Alloc` は文脈で読む std モジュールの名前）。`docs/stability.md` の「std へのモジュールの追加」に当たり、`Alloc` の名前の利用者のモジュールは `E1011` になる（release notes で予告）。HEAD の `tests/`・`examples/`・`std/` に `Alloc` の名前のモジュールはない（確認: `grep -rn "\bAlloc\b" tests examples std --include=*.tz`）。
- 公開 ABI: `tsuzuri_alloc_stats`・`tsuzuri_allocation_stats` は変えない。新しい公開記号は足さない（`@tz.heap.counter`・`@tsuzuri_bench_counts` は実行器の内部）。
- IR・WASM の import: `Alloc` を使わないプログラムは変わらない。counting の IR は、`@tz.heap.counts` が 6 要素になり、`@tz.heap.counter` が増え、alloc／realloc／free が 1 回あたり原子演算 1 回増える（`bytes` の加算）。WASM の import は増えない（`WebAssembly.Module.imports` は空のまま）。
- 診断: `E1018`・`E1022`・`E2000`・`E2005` の意味は変えない。
- A19／A20（todo）: `Alloc.__counter` の効果は `debug`（D-53。プロセス全体の状態を読む試験用の補助）。`alloc` ではない（確保しない）。実装時に、効果の表に 1 行足す。

## 設計

### データ構造

```rust
// src/llvm.rs
pub struct TestRunnerOptions<'a> {
    // 既存の wasm, memory64, coverage, seed, debug_output, debug, f16 に足す
    /// `tsuzuri test --allocator counting`: link the counting heap even if no test reads the counters (F14).
    pub allocator: Allocator,                 // Default は System（derive(Default) のまま）
}

/// What a function body reads that only some outputs can provide (G18 D3, F14).
#[derive(Clone, Copy, Default)]
struct Restricted {
    clock: bool,                              // Bench.now: the clock that only the bench runner defines
    counter: bool,                            // Alloc.__counter: the counters of the counting heap
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum RestrictedKind { Clock, Counter }

// src/test_runner.rs
pub struct TestOptions {
    // 既存の欄に足す
    pub allocator: llvm::Allocator,           // `--allocator counting`。Default は System
}
pub struct BenchOptions {
    pub allocs: bool,                         // `--allocs`。Default は false
}
pub struct BenchResult {
    pub alloc_calls: Option<u64>,             // `--allocs` で成功した bench だけ Some
    pub alloc_bytes: Option<u64>,
}
// src/main.rs の Arguments
bench_allocs: bool,
```

### 新規ファイルと関数

| ファイル | 内容 |
| --- | --- |
| `std/Alloc.tz`（新規） | 付録 B |
| `tests/alloc.rs`（新規） | Phase 1・2 の Rust テスト（「テスト計画」） |
| `tests/fixtures/alloc_contracts/Main.tz`（新規） | 付録 A |
| `tests/alloc_contracts.rs`（新規） | 契約の harness |
| `src/llvm.rs` `emit_bench_allocation_runner`（新規） | 計数用の bench 実行器。`emit_bench_runner_with` と同じ。`Instrumentation { f16, allocator: Allocator::Counting, ..Default::default() }` で `emit_program` を呼ぶ |
| `src/llvm.rs` `restricted_error`（`clock_error` の改名と一般化） | 下の「到達の制限」 |
| `src/test_runner.rs` `execute_bench_allocations`（新規） | 計数用の実行器を 1 回走らせる |

### 計数 heap（`src/runtime/heap-counting.ll`。付録 C が全文）

`@tz.heap.counts` を `[6 x i64]`（0 allocs、1 reallocs、2 frees、3 bytes、4 live、5 peak）にし、`@tz.heap.change(slot, delta, requested)`（原子演算 4 回: 出来事 1・bytes・live・peak）、`@tz.heap.counter(index)`（5 を超えるとトラップ）を足す。`tsuzuri_alloc_stats` は `@tz.heap.counter` を呼んで同じ 4 つの値を書く。

### `emit_program_in` の変更（番号の順に）

`src/llvm.rs` の `emit_program_in` で、`let mut runtime_symbols = RuntimeSymbols::collect(&output);` の直後に次を足す（`allocator` は関数の先頭の `let allocator = instrumentation.allocator;`）。

1. `let reads_counters = runtime_symbols.contains("tz.heap.counter");`
2. `reads_counters` かつ `allocator` が `Host`／`Small` → `E2000`（上の表の文面、`Span::default()`）。
3. `reads_counters` かつ `allocator == System` かつ `entry != Entry::TestRunner` → `return Err(restricted_error(module, &roots, RestrictedKind::Counter, entry))`（`E1018`）。
4. `let allocator = if reads_counters { Allocator::Counting } else { allocator };`（以降の `allocator` はこれ。2・3 を通ると、ここで `Counting` になるのは「明示の counting」か「テスト実行器の自動」だけ）。
5. heap を連結する条件（`runtime_symbols.prefix("tz.string.") || … || runtime_symbols.prefix("tz.realloc")`）に `|| reads_counters` を足す（`Alloc` だけを使うテストでも heap と counting を連結するため）。
6. ABI の弱い定義（`uses_host_abi(module) || … || instrumentation.allocator == Allocator::Counting`）は**変えない**（`instrumentation.allocator` は明示の指定のまま）。
7. `Entry::BenchRunner` の組み立て（`tsuzuri_bench_sample` を定義する所）で、`instrumentation.allocator == Allocator::Counting` のとき、`@tsuzuri_bench_count` の後に `@tsuzuri_bench_counts(ptr %out)`（`@tz.heap.counter` を 0・1・3 で呼び、`%out` の 3 つの `i64` に書く）を足す（Phase 2。IR の全文は付録 H）。

`emit_test_runner_with` は `Instrumentation { …, allocator: options.allocator, ..Instrumentation::default() }` にする。`emit_builtin` に `Builtin::AllocCounter` の arm（`define internal i64 {symbol}(i64 %index) nounwind { … %count = call i64 @tz.heap.counter(i64 %index) … ret i64 %count }`）を `Builtin::GenSeed` の前に足す。

### 到達の制限（`src/llvm.rs`。`Bench.now` の機構の一般化）

1. `Restricted`・`RestrictedKind` を `references` の近くに足す（上のデータ構造。`Restricted::any(self) -> bool`・`has(self, kind) -> bool`）。
2. `references`・`direct_references` の `clock: &mut bool` を `restricted: &mut Restricted` にする。`direct_references` は、`Builtin::BenchNow` なら `restricted.clock = true`、`Builtin::AllocCounter` なら `restricted.counter = true`。
3. `close` は `references` の後 `if restricted.any()` で `clocks`（改名: `restricted_functions`）に入れる。`program_reach`・`reaching` は名前を合わせるだけで、**どちらを読む関数も同じに扱う**（根の除外は両方に効く）。`reaching` の `&mut false` は `&mut Restricted::default()`。
4. `clock_error(module, roots)` を `restricted_error(module, roots, kind, entry)` に改名する。幅優先の探索は `references` の後 `restricted.has(kind)` で終わる。`clock_use(expression, module, target)` に `kind` を足し、`direct_references` の結果の `restricted.has(kind)` を `clock` の代わりに見る。
5. 文面は `kind` で選ぶ: `Clock` は HEAD の `RULE`・`ADVICE` の文字列をそのまま（1 文字も変えない）。`Counter` は上の診断の表の文面（`Entry::BenchRunner` のときだけ助言が違う）。
6. 呼び出しは 2 か所: 既存の `if entry != Entry::BenchRunner && intrinsics.contains("declare i64 @tsuzuri_bench_now()") { return Err(restricted_error(module, &roots, RestrictedKind::Clock, entry)); }` と、上の手順 3。

### CLI と driver の変更

| ファイル | 関数 | 変更 |
| --- | --- | --- |
| `src/driver.rs` | `BuildOptions::validate_allocator` | counting の拒否を `self.emit.is_kernel()` だけにし、文面を `--allocator counting requires executable, object, shared library, LLVM IR, header, or WebAssembly output` にする |
| `src/main.rs` | `parse_arguments` | `allocator.is_some() && action != Action::Build` を、`Run`・`Test` は値が `Allocator::Counting` のときだけ許す形にする。拒否の文面は変えない（`--allocator is only valid with build`）。`--allocs` を足す（`bench_allocs`。`bench` 以外は `--allocs is only valid with bench`）。HELP の `--allocator` の行に `(counting also with run and test)`、bench の行に `[--allocs]` |
| `src/main.rs` | テストの実行（`driver::TestOptions { … }` を作る所） | `allocator: arguments.options.allocator`（`BenchOptions` には `allocs: arguments.bench_allocs`） |
| `src/test_runner.rs` | `TestOptions`・`Default` | `allocator` 欄 |
| `src/test_runner.rs` | `build_runner`・`build_debug_runner` | `llvm::TestRunnerOptions { allocator: options.allocator, … }`（`build_runner` の構造体の書き方は全欄を書くので `allocator` を足す。`build_debug_runner` は `..Default` つき） |
| `src/test_runner.rs` | `run_benches_linked` | 手順 2.3 |
| `src/main.rs` | `run_bench_action` | 手順 2.4 |

### bench の計数の流れ（`run_benches_linked`、`--allocs` のとき）

1. 時間の実行器を作り、全 bench を既存どおり測る。
2. 続けて `llvm::emit_bench_allocation_runner(module, &indices, f16)` → `compile_native_runner`（`NativeRunner { stem: "bench-allocs", kind: "bench", entry: bench-runner.c, defines: &["-DTSUZURI_BENCH_ALLOCS"], optimization: options.optimization, … }`）で計数用の実行器を作る。
3. 時間が成功した bench ごとに `execute_bench_allocations(runner, position, BENCH_TIMEOUT)` を呼び、標準出力の `alloc_calls <n>\nalloc_bytes <n>\n`（この 2 行だけ。形が違えば `Err("malformed allocation output")`）を読む。終了コード 3 は `negative duration`、5 は `returned a negative allocation count`。
4. 失敗は `BenchResult.failure = Some("allocation run: <理由>")` にする（時間の結果は捨てる）。

`bench-runner.c` の `-DTSUZURI_BENCH_ALLOCS` 版の `main`（既存の `main` は `#else` の側に残し、`parse`・`tsuzuri_bench_now` は共有する）は付録 H。引数は `INDEX` 1 つ。終了コードは 2 が使い方の誤り、3 が負の標本、4 が出力の誤り、5 が負の計数。

### 段ごとの変更表（Phase 1）

| 段 | ファイル | 関数・型 | 変更 |
| --- | --- | --- | --- |
| 1.3 | `src/runtime/heap-counting.ll` | 全体 | 付録 C に置き換える |
| 1.4 | `src/check.rs` | `Builtin`・`Builtin::ALL`・`Builtin::name`（`"Alloc.__counter"`）・`Builtin::scheme`（`(vec![Concrete(Type::I64)], Concrete(Type::I64), Vec::new())`） | `AllocCounter` を足す（`BenchConsume` の後ろに並べる） |
| 1.4 | `src/polymorph.rs` | `Checker::builtin` | `Builtin::AllocCounter` を `Alloc` の std モジュールの外から呼ぶと `E1022`（`Builtin::GenSeed` の規則と同じ形） |
| 1.4 | `std/Alloc.tz`・`src/stdlib.rs` | `SOURCES`・`RESERVED_MODULES`・`OPT_IN` | 付録 B。`OptIn { module: "Alloc", names: &["Alloc"], aliases: &[], uses: &[] }` |
| 1.4 | `src/llvm.rs` | `emit_builtin` | `Builtin::AllocCounter` の arm |
| 1.5 | `src/llvm.rs` | `references`・`direct_references`・`close`・`program_reach`・`reaching`・`clock_error`→`restricted_error`・`clock_use`・`emit_program_in` の手順 1〜4 | 上の「到達の制限」と「`emit_program_in` の変更」（1〜3） |
| 1.6 | `src/llvm.rs` | `emit_program_in`（4・5）・`TestRunnerOptions`・`emit_test_runner_with` | counting の自動の連結 |
| 1.7 | `src/driver.rs`・`src/test_runner.rs`・`src/main.rs` | 上の CLI の表 | 実行ファイル・`run`・`test` |

## 前提とする他チケットのインターフェース

- F13（done）: `llvm::Allocator::{System, Host, Counting, Small}`・`heap-counting.ll`・`tsuzuri_alloc_stats`・`header_with_allocator`。変えるのは `heap-counting.ll` の内部と `validate_allocator` の条件だけ。
- G18（done）: `bench`・`Bench.of`・`Bench.with_input`・`Bench.now`・`Bench.consume`・`run_benches_linked`。
- PX01（done）: 指標の名前 `alloc_calls`（count。確保と、null でも大きさ 0 でもない再確保の回数）・`alloc_bytes`（bytes。要求バイトの累計）。`tsuzuri bench --allocs` の JSON のキーに同じ名前を使う（種の `allocs` から変えた。決定事項 D5）。`benchmarks/` の harness はまだこれを読まない。
- A19・A20（todo）: 組み込み関数の効果の表が入ったら、`Alloc.__counter` を `debug` にする。それまでは何もしない（表がない）。`@no_alloc` の検査（A20）は静的で、`Alloc` は実行時の確認。互いに依存しない。
- A21（todo）: 契約の `test` モードが `Alloc.snapshot`／`since` を使える。未実装でも本チケットは完結する。
- G22・G23（todo）: `E1018` に `kind`（`alloc_needs_counting_allocator`）・`help` を足す。G23 があれば S1 の手順で kind を登録して `of` で出し、G22 だけなら `with_kind`、どちらもなければ足さない（D-51）。
- G34（todo）: `Alloc` のページの `test` つきの例は、G34 の doc-test と `check-docs.mjs` のどちらでも実行できる形で書く。

## 実装手順

### Phase 1

#### 手順 1.1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p target/tz-F14-before && cp target/release/tsuzuri target/tz-F14-before/tsuzuri-before`。次の 4 つのプロジェクトを、`target/tz-F14-before/tsuzuri-before build <dir> --emit llvm --target <native|wasm32> -O0 --no-cache -o target/tz-F14-before/<名前>-<target>.ll` で出す: `tests/fixtures/last_use`・`tests/fixtures/map_set`・`tests/fixtures/channel`・`examples/point`（8 ファイル。以後の「IR が変わらない」の基準）。`node tests/allocator.mjs target/release/tsuzuri` が `allocator: ok` であることを確かめる。「現状」の再現コマンドを実行し、出力が一致することを確かめる。`grep -rn "\bAlloc\b" tests examples std --include=*.tz` が 0 件であることを確かめる。
- 確認: すべて通る。8 つの基準 IR ができている。

#### 手順 1.2: テスト先行（`tests/alloc.rs` を `#[ignore]` つきで全部書く）

- 変更: `tests/alloc.rs`（新規）。
- 内容: 「テスト計画」の Phase 1 の Rust テスト 12 件を、本体まで書く。各テストに `#[ignore = "F14 手順 1.N"]`（N は有効にする手順）を付ける。`tests/allocator.rs` の `options`・`emit`・`fixture` の形、`tests/bench.rs` の `project`・`tsuzuri`・`clean` の形を写す。
- 確認: `cargo test --locked --test alloc` が `0 passed; 12 ignored`。`cargo test --locked` が手順 1.1 と同じ件数（＋12 ignored）で通る。

#### 手順 1.3: 計数 heap にカウンターを足す

- 変更: `src/runtime/heap-counting.ll`、`tests/alloc.rs`（`counting_heap_keeps_the_stats_abi_and_adds_two_counters` の `ignore` を外す）。
- 内容: 付録 C の全文に置き換える。
- 確認: `cargo test --locked --test allocator`（9 件）と `--test alloc` の該当テストが通る。`cargo build --release --locked && node tests/allocator.mjs target/release/tsuzuri` が `allocator: ok`（`tsuzuri_alloc_stats` の値は手順 1.1 と同じ）。手順 1.1 の 8 つの IR が `cmp` で一致する（`Alloc` を使わない）。

#### 手順 1.4: ビルトイン・std モジュール・`E1022`

- 変更: `src/check.rs`・`src/polymorph.rs`・`src/llvm.rs`（`emit_builtin` の arm）・`std/Alloc.tz`（新規）・`src/stdlib.rs`、`tests/alloc.rs`（`alloc_is_an_opt_in_reserved_std_module`・`alloc_counter_is_private_to_the_std_module` の `ignore` を外す）。
- 内容: 「段ごとの変更表」の 1.4 のとおり。`Builtin::AllocCounter` を足したら `cargo check` が示す `match` を直す（停止条件 7）。`std/Alloc.tz` は付録 B のまま。
- 確認: `cargo test --locked`（`src/stdlib.rs` の `opt_in_modules_are_reached_only_through_their_names`・`every_std_module_type_checks_when_named` を含む）が通る。手順 1.1 の 8 つの IR が `cmp` で一致する。

#### 手順 1.5: 到達の制限（`E1018`）

- 変更: `src/llvm.rs`、`tests/alloc.rs`（4 件: `alloc_outside_tests_and_counting_builds_is_rejected`・`public_alloc_helpers_stay_out_of_other_builds`・`alloc_reached_from_entry_export_or_drop_is_rejected_by_name`・`alloc_with_host_or_small_allocator_is_rejected`）。
- 内容: 「到達の制限」の 1〜6 と、「`emit_program_in` の変更」の 1〜3。`Bench.now` の既存の挙動を変えない。
- 確認: `cargo test --locked --test bench` が手順 1.1 と同じ件数で通る（`Bench.now` の 3 件を含む）。新しい 4 件が通る。`cargo test --locked` が通る。手順 1.1 の IR が一致する。

#### 手順 1.6: テスト実行器の counting

- 変更: `src/llvm.rs`（`emit_program_in` の 4・5、`TestRunnerOptions` の `allocator` 欄、`emit_test_runner_with`）、`src/test_runner.rs`（`TestOptions` に `allocator` 欄。`build_runner`・`build_debug_runner` がそれを `llvm::TestRunnerOptions` に渡す。CLI からの値は手順 1.7）、`tests/alloc.rs`（`test_runner_links_the_counting_heap_only_when_a_selected_test_reaches_alloc`・`cli_test_counts_events_on_native_and_wasm32` の `ignore` を外す）。
- 内容: 「`emit_program_in` の変更」の 4・5。
- 確認: `cargo build --release --locked` の後、`target/release/tsuzuri test <Alloc を使う 3 件のテストのプロジェクト> -O0`・`-O3`・`--target wasm32 -O0`・`--target wasm32 -O3` が `3 passed; 0 failed; 0 ignored`。`cargo test --locked` が通る。手順 1.1 の IR が一致する。

#### 手順 1.7: CLI（実行ファイル・`run`・`test`）

- 変更: `src/driver.rs`（`validate_allocator`）、`src/main.rs`（`parse_arguments`・`TestOptions` の構築・HELP・テスト `parses_allocator_selection`）、`src/test_runner.rs`（値の配線）、`tests/allocator.rs`（`allocator_options_reject_unsupported_outputs`）、`tests/allocator.mjs`、`tests/alloc.rs`（`cli_run_and_build_executable_with_counting`・`cli_build_without_counting_rejects_alloc`・`cli_test_with_allocator_counting_runs_programs_that_do_not_use_alloc` の `ignore` を外す）。
- 内容: 「CLI と driver の変更」の表。既存テストの更新は「既存テストへの影響」の 3 件だけ。
- 確認: `cargo test --locked` が通る（`parses_allocator_selection` は新しい期待値）。`node tests/allocator.mjs target/release/tsuzuri` が `allocator: ok`。`tsuzuri run <fixture> --allocator counting` が `90`。`tsuzuri build … --allocator counting -o x && ./x` が `90`。`--allocator` なしの `build` が `E1018`・終了コード 1。

#### 手順 1.8: 文書（Phase 1）

- 変更: 「ドキュメント」の Phase 1 の行。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/alloc.md _tsuzuri/language-reference/compiler/option.md _tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md _tsuzuri/language-reference/compiler/diagnostics.md`。

#### 手順 1.9: 全体の確認

- 内容: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、`cargo build --release --locked && node tests/allocator.mjs target/release/tsuzuri`、`node tests/features.mjs target/release/tsuzuri`（既存の suite が変わらない）、`node tests/parallel_frontend.mjs target/release/tsuzuri`、`sh scripts/check-runtime-includes.sh`（`.ll` を足していないので件数は同じ）。手順 1.1 の IR が一致する。native と wasm32 の `-O0`／`-O3` の確認は手順 1.6 のとおり。
- 確認: すべて通る。完了報告（GUIDE §13.2）を末尾に書く。

### Phase 2

#### 手順 2.1: テスト先行

- 変更: `tests/bench.rs`（`#[ignore = "F14 手順 2.N"]` つきで「テスト計画」の Phase 2 の 4 件）。
- 確認: `cargo test --locked --test bench` が既存の件数で通り、新しい 4 件が ignored。

#### 手順 2.2: 計数用の実行器（IR と C）

- 変更: `src/llvm.rs`（`emit_bench_allocation_runner`・`@tsuzuri_bench_counts`）、`src/runtime/bench-runner.c`、`tests/bench.rs`（`bench_allocation_runner_ir_has_the_counts_function_and_the_default_runner_is_unchanged`）。
- 内容: 「`emit_program_in` の変更」の 7 と、`bench-runner.c` の `-DTSUZURI_BENCH_ALLOCS` 版。`emit_bench_runner`・`emit_bench_runner_with` は変えない。
- 確認: `cargo test --locked` が通る。`emit_bench_runner` の IR は手順 1.1 の基準（`tests/bench.rs` の既存テストの文字列）と同じ。

#### 手順 2.3: 計数の実行（`run_benches_linked`）

- 変更: `src/test_runner.rs`（`BenchOptions.allocs`・`BenchResult.alloc_calls`・`alloc_bytes`・`run_benches_linked`・`execute_bench_allocations`）。
- 内容: 「bench の計数の流れ」。`BenchResult` を作る 2 か所に `alloc_calls: None, alloc_bytes: None` を足す（`allocs` なしの結果は変わらない）。
- 確認: `cargo test --locked` が通る。

#### 手順 2.4: CLI と出力

- 変更: `src/main.rs`（`parse_arguments`・`run_bench_action`・HELP・`parses_bench_coverage_and_seed_options`）、`tests/bench.rs`（`bench_allocs_*` の `ignore` を外す）。
- 内容: `--allocs`。JSON は `max` の後に `,"alloc_calls":{},"alloc_bytes":{}`、text は行末に `, alloc_calls {}, alloc_bytes {} per iteration`。`--allocs` がないときの出力のコードは触らない。
- 確認: 付録の `bench2`（5 件）で `target/release/tsuzuri bench <dir> --allocs --samples 3 --json` を 2 回実行し、`alloc_calls`／`alloc_bytes` が 0／0、1／80、1／80、0／0、2／96 で、2 回とも同じ。`--allocs` なしの出力に `alloc_` が出ない。

#### 手順 2.5: 文書と全体

- 変更: 「ドキュメント」の Phase 2 の行。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/bench.md`、手順 1.9 のコマンド。

### Phase 3

#### 手順 3.1: fixture と harness

- 変更: `tests/fixtures/alloc_contracts/Main.tz`（付録 A をそのまま）、`tests/alloc_contracts.rs`（新規）。
- 内容: 「テスト計画」の Phase 3。`target/release/tsuzuri check tests/fixtures/alloc_contracts` が通ることを先に確かめる。
- 確認: `cargo test --locked --test alloc_contracts` が `2 passed`。件数の確認（42）が入っている。

#### 手順 3.2: 文書と全体

- 変更: 「ドキュメント」の Phase 3 の行。
- 確認: 手順 1.9 のコマンド、`node tests/parallel_frontend.mjs target/release/tsuzuri`（新しい fixture も `check` される）。

## テスト計画

### Phase 1: `tests/alloc.rs`（新規。Rust。`tests/allocator.rs`・`tests/bench.rs` の形）

| 関数 | 入力 | 期待値 |
| --- | --- | --- |
| `alloc_is_an_opt_in_reserved_std_module` | `tsuzuri::stdlib::is_reserved_module("Alloc")`。`Alloc` を書かない 1 行のプログラムと書くプログラムの `stdlib::sources_for([…])` | 予約されている。書かないと `std/Alloc.tz` は入らず、書くと入る |
| `alloc_counter_is_private_to_the_std_module` | `let n = Alloc.__counter 0\nn` を `analyze` | `E1022` |
| `counting_heap_keeps_the_stats_abi_and_adds_two_counters` | `llvm::emit_with_options`（`tests/allocator.rs` の `options(wasm, Allocator::Counting)`。native と wasm） | IR に `@tz.heap.counts = internal global [6 x i64]`・`define internal i64 @tz.heap.counter(i64 %index)`・`define void @tsuzuri_alloc_stats(ptr %stats)`・`getelementptr inbounds i8, ptr %stats, i64 24` が各 1 回。`include_str!("../src/runtime/heap-counting.ll")` を含む |
| `test_runner_links_the_counting_heap_only_when_a_selected_test_reaches_alloc` | 2 つのテスト（`Alloc.snapshot` を使う／使わない）の `analyze`、`emit_test_runner(&module, &[0], wasm)`・`&[1]`（native と wasm） | `[0]` は counting の heap（上のテキスト）を含み、`call i64 @tz.heap.counter(` がある。`[1]` は含まず、`Alloc` のない同じ module の IR と一致する |
| `alloc_outside_tests_and_counting_builds_is_rejected` | `export def n :: i64\nfn n = (Alloc.snapshot ()).allocs` と `let s = Alloc.snapshot ()\ns.allocs`。`emit_target(…, Library, wasm)`・`emit(…, Console)`・`emit_bench_runner`。`emit_with_options(EmitOptions { allocator: Allocator::Counting, entry: Library, … })` | 前 3 つは `E1018`（メッセージに `Alloc reads the counters of the counting allocator`）。bench 実行器の助言は `tsuzuri bench --allocs`。最後は成功し、IR が決定的 |
| `public_alloc_helpers_stay_out_of_other_builds` | 公開の `def count_events :: unit -> i64`（`Alloc` を呼ぶ）と、それを呼ぶ `test`、`Debug.print "hi"` の入口。`private def` に直した版も作る | Console・Library の `emit` が成功し、IR に `tz.heap.counter` がない。`private` 版と IR が 1 byte も同じ |
| `alloc_reached_from_entry_export_or_drop_is_rejected_by_name` | 入口が `report` を呼ぶ（`report` は `Alloc.measure` を使う）、`export def`、`Drop` の instance | `E1018`。メッセージに `the top-level code reaches it through `Main.report``、`export `Main.total_allocs` reads it`、`the Drop instance of` が出る。位置は `Span::default()` でない |
| `alloc_with_host_or_small_allocator_is_rejected` | `emit_with_options(…, Allocator::Host)`・`Small` で `Alloc` を使う export | `E2000`（`Alloc needs the counting allocator`） |
| `cli_test_counts_events_on_native_and_wasm32` | 3 件のテスト（`Array.map`＝1／0／0／80、`Array.update_at` を `snapshot`／`since`＝0、`Alloc.measure`＝1）の一時 project。`tsuzuri test <dir> -O0`・`-O3`・`--target wasm32 -O0`・`-O3` | 4 回とも終了コード 0、`3 passed; 0 failed; 0 ignored` |
| `cli_run_and_build_executable_with_counting` | 付録 D.7 の `Main.tz`（`used.bytes + Array.length (ref ys)`）。`tsuzuri run <dir> --allocator counting`（`-O0`・`-O3`）と、`build … --allocator counting -o <exe>` の実行 | 標準出力が `90`、終了コード 0 |
| `cli_build_without_counting_rejects_alloc` | 同じ project を `--allocator` なしで `build` と `run` | 終了コード 1、標準エラーに `E1018` と `--allocator counting` |
| `cli_test_with_allocator_counting_runs_programs_that_do_not_use_alloc` | `Alloc` を使わない 2 件の簡単なテストの一時 project に `test --allocator counting`、`test --allocator host` | 前者は通る（`2 passed`）。後者は終了コード 2 と `--allocator is only valid with build` |

### 既存テストへの影響（Phase 1）

| テスト | 変更 |
| --- | --- |
| `src/main.rs` の `parses_allocator_selection` | `build Main.tz --allocator counting` を「拒否」の表から外し、受理（`Allocator::Counting`）の確認に移す。`run Main.tz --allocator counting`・`test Main.tz --allocator counting` を受理に加える。`build --emit wgsl --allocator counting` の拒否（文面は新しいもの）を足す。`run\|check\|test … --allocator system` の拒否（文面そのまま）は変えない |
| `tests/allocator.rs` の `allocator_options_reject_unsupported_outputs` | `for allocator in [Host, Counting]` の `Emit::Executable` の拒否を `Host` だけにし、`Counting` の `Emit::Executable` は `validate().unwrap()` にする。`Emit::Executable` の拒否の文言は `Host` のものを使う |
| `tests/allocator.mjs` の A の 6 番目の `rejects(["build", abi, "--allocator", "counting", …], …)` | `--emit wgsl` の拒否（新しい文面）に変える。`abi` は入口のない fixture（実行ファイルは `E2004`）なので、実行ファイルの成功はここでは確かめず、`tests/alloc.rs` の `cli_run_and_build_executable_with_counting` が確かめる |
| それ以外 | 変更なし。根拠: `Alloc` を使わないプログラムの IR は 1 byte も変わらず（手順ごとに基準と `cmp`）、`--allocator system\|host\|small` の経路は触らない。`tests/bench.rs` の `Bench.now` の 3 件は到達解析の一般化の後も同じ文面で通る |

### Phase 2: `tests/bench.rs`（既存のファイルに足す）

| 関数 | 入力 | 期待値 |
| --- | --- | --- |
| `bench_allocation_runner_ir_has_the_counts_function_and_the_default_runner_is_unchanged` | `emit_bench_allocation_runner`・`emit_bench_runner` | 計数用は `define void @tsuzuri_bench_counts(ptr %out)` と counting の heap を含み、`emit_bench_runner` の IR に `tsuzuri_bench_counts`・`tz.heap.` がない |
| `bench_allocs_reports_deterministic_per_iteration_counts` | 付録の `bench2` の 5 件。`bench <dir> --allocs --json --samples 3` を 2 回 | 各行の `alloc_calls`／`alloc_bytes` が 0／0、1／80、1／80、0／0、2／96（添字 0〜4）。2 回の実行で一致 |
| `bench_allocs_is_opt_in_and_keeps_the_json_keys` | 同じ project を `--allocs` なしで `--json` | 行のキーの順が HEAD と同じ（`type` から `max`）で、`alloc_` を含まない。text の行は `alloc_calls` を含まない |
| `bench_allocs_rejects_other_commands_and_reports_failures` | `tsuzuri run <dir> --allocs`・`test … --allocs`、トラップする bench に `--allocs` | 前 2 つは終了コード 2 と `--allocs is only valid with bench`。トラップする bench は `status: failed`（時間の失敗）で `alloc_` なし |
| `src/main.rs` の `parses_bench_coverage_and_seed_options` | `bench Main.tz --allocs` | `bench_allocs == true`。`bench` 以外では `--allocs is only valid with bench` |

### Phase 3: `tests/alloc_contracts.rs`（新規。Rust）

| 関数 | 入力 | 期待値 |
| --- | --- | --- |
| `std_allocation_contracts_hold_on_native_and_wasm32_at_o0_and_o3` | `tests/fixtures/alloc_contracts/Main.tz` を読み、`tsuzuri::analyze`。`target ∈ {Native, Wasm32}` × `optimization ∈ {0, 3}` で `tsuzuri::driver::run_tests(&module, &TestOptions { target, optimization, ..Default::default() })` | 4 回とも `results.len() == 42` で、全件 `failure.is_none()`。失敗があれば、失敗したテスト名と `output`（native の `got/want` の行）を `panic!` に含める |
| `a_changed_expectation_fails_the_contract` | 付録 A の `expect` の定義と、`Vec.push` 100 回の 1 件（期待の `reallocs` を 6 にしたもの）だけの source | native の `-O0` で `results[0].failure.is_some()`（退行が「確保数が違う」というテストの失敗になる）。`output` に `reallocs 5/6` を含む |

### Phase 3 の確認（手動。実装者）

付録 A の 42 件の名前が付録 E の 42 行と 1 対 1 であること（`grep -c '^test "' tests/fixtures/alloc_contracts/Main.tz` が 42）。

## ドキュメント

| ページ | 更新（Phase） |
| --- | --- |
| `_tsuzuri/language-reference/built-in-types-and-modules/alloc.md`（新規） | `Alloc` の API・数える出来事の表・使える出力の決定表・閉包の注意・`E1018` の例・`test` つきの使用例・std の確保の契約の表（付録 E から）・既知の事実（`Array.sort`・捕捉つきクロージャ・借用された局所）（1・3） |
| `_tsuzuri/language-reference/index.md` | 「Bench」の次に `- [Alloc](./built-in-types-and-modules/alloc.md)`（1） |
| `_tsuzuri/language-reference/compiler/option.md` | `--allocator` の行に、`counting` が実行ファイル・`run`・`test` で使えること。`--allocs`。`--allocator counting` の拒否の行を新しい文面に（1・2） |
| `_tsuzuri/language-reference/ownership-and-memory/stack-and-heap.md` | `--allocator` の節に `counting` の対象と `Alloc`（1） |
| `_tsuzuri/language-reference/compiler/native-interop.md` | `--allocator counting` の段落に「Tsuzuri のコードからは `Alloc` で読む」と、`bytes`・再確保の数え方（1） |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | `E1018`・`E1022`・`E2000` の行に新しい条件（1） |
| `_tsuzuri/language-reference/compiler/usage.md` | `### test` に `--allocator counting`、`### bench` に `--allocs`（1・2） |
| `_tsuzuri/language-reference/built-in-types-and-modules/bench.md` | `--allocs` の行、JSON のキー、text の形、計数の定義、LLVM が消した確保も数えること（2） |
| `_tsuzuri/language-reference/built-in-types-and-modules/test.md` | 確保数の表明（`alloc.md` へのリンク）（1） |
| `docs/language.md` | 「ホスト提供の allocator」の `counting` の段落（実行ファイルとの併用の拒否を消す）、`tsuzuri bench` の段落（1・2） |
| `docs/architecture.md` | 「allocator の選択（F13）」に自動の counting と 6 つのカウンター。`program_reach` の `Bench.now` の段落を `Restricted` の一般化に合わせる。opt-in の std モジュールの一覧に `Alloc`（1） |
| `README.md` | CLI の表の `--allocator` の行、`tsuzuri bench` に `--allocs`、std モジュールの表に `Alloc`、検証コマンドに `tests/alloc.rs`・`tests/alloc_contracts.rs`（1・2・3） |
| `_features/README.md`・`_features/GUIDE.md` | 状態の更新は統括者。D-55 に追加の名前 `--allocs` を求める |

検証: `node scripts/check-docs.mjs <変更したページ>`（リンク・見出し・すべての例の `tsuzuri check`・`run=` の native `-O0`／`-O3`・`test` つきの例の `tsuzuri test`）。新しい例は `test "…" = …` の形で書き、`E1018` の例は `text` ブロック（実行しない）にする。

## 受け入れ条件

### Phase 1

- [x] 手順 1.1 の 8 つの IR（`Alloc` を使わない）が、すべての手順の後で `cmp` で一致する。
- [x] `tsuzuri test` が、`Alloc` を使うテストを native・wasm32 の `-O0`／`-O3` で通す。`Alloc` を使わないテストの実行器に counting の heap が入らない（`test --allocator counting` を除く）。
- [x] `tsuzuri run --allocator counting`・`build --allocator counting`（実行ファイル）が `90` を出す。`--allocator` なしの `Alloc` つきの `build`・`run` が `E1018`。公開の補助関数が `Alloc` に届くだけでは通常のビルドは失敗しない。
- [x] `tsuzuri_alloc_stats` の値と構造体が変わらない（`node tests/allocator.mjs` が `allocator: ok`）。WASM の import が増えない。
- [x] `Bench.now` の `E1018` の既存テストが同じ文面で通る。`cargo test --locked`・`fmt`・`clippy` が通る。
- [x] 「ドキュメント」の Phase 1 の行を更新し、`node scripts/check-docs.mjs <ページ>` が通る。

### Phase 2

- [x] `tsuzuri bench --allocs --json` の各行に `alloc_calls`・`alloc_bytes` が付き、付録の `bench2` の値（0／0、1／80、1／80、0／0、2／96）になり、2 回の実行で同じ。
- [x] `--allocs` を付けない `bench` の text・JSON が 1 byte も変わらない。`run`・`test` に `--allocs` を付けると終了コード 2。

### Phase 3

- [x] 42 件の契約が native と wasm32 の `-O0`／`-O3` で通る（`cargo test --locked --test alloc_contracts`）。期待値を書き換えていない。
- [x] 契約の表が `alloc.md` にあり、付録 E と一致する。

## 落とし穴

| 落とし穴 | 検出・回避 |
| --- | --- |
| 所有値を消費する操作を `Alloc.measure` の閉包の中で測ると、捕捉した値が複製されて 1 回余分に数える（`Array.update_at 0 5 xs` が `allocs 1`） | 消費する操作は `snapshot`／`since`（付録 D.7）。ドキュメントに書く |
| 窓の中で `Debug.print`・文字列補間・`Array.length (ref xs)` を使うと、確保が窓に入る／最後の使用の判定が変わる（ループの `Array.update_at` が 100 回複製される） | 窓の中は測る操作だけ。補間や借用は窓の外（付録 A の書き方）。付録 D.5 |
| `ref "literal"` は書けない（`E1013`）。`{s.length}` の補間は `E1013`（D13 が直す） | 文字列は `let` に束縛してから `ref`。長さは `String.length (ref s)` |
| `new [1, 2, 3]` の要素は `i32`（整数の既定）。`bytes` が 12 になる | 契約は `1i64` を明示（付録 A） |
| 捕捉つきクロージャを std の関数へ渡すと、wasm32 は環境を確保する（native は確保しない）。ポインターを含む型の `bytes` は target で違う | 契約に入れない。`expect_events` を使う |
| wasm32 の `-O3` で文字列を使うテストを含む実行器は、clang の警告 `loop not unrolled` を 1 行出す（HEAD から。終了コード 0） | F14 の不具合ではない。harness は `messages` を見ない |
| `bench --allocs` の `alloc_calls` は、時間の実行器で LLVM が確保ごと消した本体も数える（`five pushes` は 0.18 ns で、確保は 2） | 文書に書く。0.2 ns 前後の bench は消えたと読む |
| 計数 heap は遅い（確保ごとに原子演算 4 回と 16 バイトのヘッダー）。速度の測定に使う | 使わない。`bench` は別の実行器で数える（決定事項 D5） |
| `emit_test_runner_with` の `TestRunnerOptions` を全欄で作る所は `src/test_runner.rs` の `build_runner` だけ。欄を足すとコンパイルエラー | `..Default::default()` を使う既存のテストは壊れない |
| 到達解析を一般化するとき、`Clock` の文面を 1 文字でも変えると `tests/bench.rs` が落ちる | `RULE`・`ADVICE` の文字列を移すだけにして、`cargo test --locked --test bench` を毎回実行 |
| `Array.sort` は n に比例して確保する（n=100 で 318 回）。契約に入れていないので、直しても落ちない | 直すときは契約に足す（`Array.sort` の規則を決めてから） |

## 対象外

- 確保の位置の報告（G26 の facts `alloc_sites`）、リーク検出、`live`・`peak` の公開。
- `--allocator counting` を使った `tsuzuri bench` の時間の計測、`bench` の `--allocator`。
- `Map`・`Set`・`HashMap`・`Task`・`Parallel` の確保の契約。`Map`・`Set` は C12 が確保の回数を約束せず（検索が確保しないことだけを約束する）、`insert` の確保はチャンクの分割・追加と容量の倍化で変わるので、意図して入れない。要るなら、C12 の後に実測を取り直す別のチケットが足す。
- `Array.sort` の確保の改善、wasm32 の捕捉つきクロージャの環境、借用された局所のコピー除去の改善（見つけた事実を付録 D.5 に残した）。
- 終了時の確保数の自動の報告（環境変数・フラグ）。
- WASM の `tsuzuri bench`（`bench` は native だけ）。
- `script`・`repl` での `Alloc`。

## 決定事項

### D1: API は `Stats`・`snapshot`・`since`・`measure`。ビルトインは `Alloc.__counter` 1 つ

- 決定: 付録 B のとおり。種の `measure` に `snapshot`・`since` を足す。ビルトインは添字を取る 1 つ（`i64 -> i64`）。
- 理由: 閉包を渡す `measure` だけでは、所有値を消費する操作（`Array.update_at 0 5 xs`）が閉包の中で複製され測れない（付録 D.7。`allocs 1`）。ビルトインが 1 つなら、A19・A20 の効果の表の行も 1 つで済む。
- 却下: `measure` だけ（測れない）。4 つのビルトイン（分類が 4 行）。`Stats` を配列で返す（確保が窓に入る）。グローバルな `Alloc.reset`（窓が入れ子にできない）。
- 状態: 承認済み。

### D2: カウンターは `allocs`・`reallocs`・`frees`・`bytes`

- 決定: 4 つ。`live`・`peak` は内部（`tsuzuri_alloc_stats` の ABI）に残し、`Stats` に出さない。
- 理由: `Vec.push` を 100 回しても `allocs` は 1 で、再確保（倍増の 5 回）が見えない（付録 D.2）。種の 3 つでは「`Vec.push` は償却で確保が対数」を書けない。`bytes` は「余計な確保がない」を書くのに要る。`live`・`peak` は差分として意味が弱い。
- 却下: 種の 3 つ。`reallocs` を `allocs` に足す（「確保 1 回・再確保 5 回」を区別できない。bench の `alloc_calls` は PX01 に合わせて足し算する）。`live`・`peak` を出す（YAGNI）。
- 状態: 承認済み。

### D3: `Alloc` はテストと明示の counting だけ（`E1018`）。`tsuzuri test` は自動

- 決定: 「どこで `Alloc` が使えるか」の決定表のとおり。到達の制限と根の除外は `Bench.now` の機構を一般化する。
- 理由: 計数 heap は遅い。公開の補助関数が `Alloc` を呼ぶだけで、ライブラリや実行ファイルが黙って遅い heap になるのは、AGENTS.md の「自動の選択は文書化したフォールバックを持つ」に反する。テストの実行器は成果物でないので自動でよい。`Bench.now` が同じ問題を同じ形で解いている。
- 却下: どこでも自動で連結する（黙った性能の退行）。`test` でも `--allocator counting` を必須にする（フラグなしで失敗する）。`Alloc` を書ける場所を構文で限る（補助関数が書けない）。
- 状態: 承認済み。

### D4: `--allocator counting` は `build`（実行ファイル）・`run`・`test` が受ける

- 決定: CLI の表のとおり。`run`・`test` は `counting` だけを受け、ほかの値は既存の文面で拒否する。
- 理由: D-55 が「`--allocator counting` の `test` への対応」を F14 に割り当てている。`run` は `Alloc` を使う小さなプログラムを試す入口で、`E1018` の助言が実行できる必要がある。
- 却下: `test` だけ（`run` の助言が実行できない）。`host|small` を `test` に開く（用途がない）。新しいフラグ（D-55 の名前が増える）。
- 状態: 承認済み。

### D5: `bench --allocs`。別の実行器・2 反復 − 1 反復・キーは `alloc_calls`／`alloc_bytes`

- 決定: 「`tsuzuri bench --allocs`」の節のとおり。キー名は PX01 の指標に合わせる（種の `allocs` を変えた）。
- 理由: 計数は時間を乱す（原子演算とヘッダー）ので、時間の実行器は変えず別の実行器で数える。反復回数は時間で決まるので、「1 反復あたり」は 2 反復と 1 反復の差（準備と解放が消え、決定的）。`--allocs` は opt-in で、既定の出力と時間を変えない。
- 却下: 常に数える（ビルドが 2 倍になり、既定の出力が変わる）。時間の実行器に計数を入れる。実際の反復回数で割る（割り切れず非決定的）。キー名 `allocs`（PX01 と別の名前が増える）。
- 状態: 承認済み。追加が必要な名前: CLI `--allocs`（`bench` だけ）。

### D6: 計数 heap の内部は 6 つのカウンターと `@tz.heap.counter`

- 決定: 付録 C。`tsuzuri_alloc_stats` の ABI は変えない。
- 理由: 公開の ABI は安定の対象（`docs/stability.md`）。`@tz.heap.counter` 1 つを、`Alloc.__counter`・`tsuzuri_alloc_stats`・`@tsuzuri_bench_counts` が共有する。
- 却下: `tsuzuri_allocation_stats` に欄を足す（ホストの 32 byte の領域を超えて書く ABI の破壊）。新しい公開記号。
- 状態: 承認済み。

### D7: 契約は 1 つの fixture・42 件・規則のコメントつき・4 構成

- 決定: 付録 A。`expect`（4 つ）と `expect_events`（回数だけ）。harness は Rust から `run_tests` を 4 構成で呼ぶ。
- 理由: 規則から期待値を決めた（付録 E）ので、退行は規則の違反として読める。ポインターの大きさに依る `bytes` を検査しないので、1 つの fixture が native と wasm32 で通る。
- 却下: `.mjs` の suite（`tests/features.mjs` の `cases` は戻り値だけで、数が見えない）。target ごとの期待値の表。各テストを別ファイルにする（42 回のビルド）。
- 状態: 承認済み。

### D8: 契約に入れないもの

- 決定: `Array.sort`（規則がない。n=100 で 318 回）、捕捉つきクロージャの環境（wasm32 で +1）、`Map`・`Set`・`HashMap`・`Task`・`Parallel`。
- 理由: 契約は「規則から決められ、全構成で同じ」ものだけ。規則のない現状の値を固定すると、改善が契約の失敗になる。
- 却下: 現状の値を固定する（コンパイラの出力を写す禁止に反する）。
- 状態: 承認済み。

### D9: 新しい診断コードは足さない

- 決定: `E1018`・`E1022`・`E2000`・`E2005` を既存の意味の中で使う。
- 理由: `Bench.now`・`Gen.__seed` と同じ形で表せる。D-55 の予約に追記が要らない。
- 却下: `E1040` の新しいコード（利用者が見る区別が増えない）。
- 状態: 承認済み。

### D10: `Alloc` は std の予約モジュール名。効果は `debug`

- 決定: `RESERVED_MODULES` に `Alloc` を足す。A19・A20 の効果の表に入ったら `debug`。
- 理由: opt-in の std モジュールを予約するのが既存の方式（`docs/stability.md` の既知の例外）。`Alloc.__counter` はプロセス全体の状態を読む試験用の補助で、`Debug.print` と同じ分類が自然。`alloc` は「確保しうる」で、読むだけの関数は確保しない。
- 却下: 別の名前（D-55 が `Alloc` を割り当て済み）。名前を予約しない（`Alloc.tz` を持つ利用者との衝突が検出できない）。
- 状態: 承認済み。

## 実装・検証記録（41f80bc7 基準）

全 3 Phase を実装し、以下を確認してから本チケットと付録を `_completed/` へ移動した。
共有の `_features/README.md`・`GUIDE.md` の状態とリンクの更新は、指定どおり統括者に委ねる。

- **Phase 1**: `Alloc.Stats`・`snapshot`・`since`・`measure`、std 専用の `Alloc.__counter`、
  到達解析による自動計数、明示 counting の `build`・`run`・`test`。
  `tests/alloc.rs` は 13 件。native/wasm32 の O0/O3、coverage、デバッグ実行器、
  private・公開 helper・入口・export・Drop・非対応 allocator の診断を検査した。
  `Alloc` は D-59 の `subject(&["Stats"])` で分類し、資源ハンドルの例外にはしていない。
- **Phase 2**: 時間用とは別の counting 実行器による `bench --allocs`。`tests/bench.rs` は 14 件。
  5 組の期待値 `(0,0), (1,80), (1,80), (0,0), (2,96)` を O0/O3 でそれぞれ 2 回確認した。
  既定の JSON/text の形とキー順、指定先の制限、失敗と負の計数差分も検査した。
- **Phase 3**: 42 契約 × native/wasm32 × O0/O3 = **168 件成功**。
  Vec の期待する再確保を 5 回から 6 回へ変えた変異体が `reallocs 5/6` で失敗することも確認した。
- `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。
  `cargo nextest run --locked --test-threads 4` は **1739 passed / 2 skipped**。
  全重作業を共有 slot 内で実行し、スタック・資源上限・依存 crate・unsafe は増やしていない。
- 現在の dev バイナリで `allocator.mjs`、全 `features.mjs`（**19669 cases**）、
  `parallel_frontend.mjs` が成功。`check-runtime-includes.sh` は **58 files**。
- 初回 IDE CI の予約モジュール整合テストが `Alloc` の不足を検出した。
  隔離 worktree の `vsc/src/core.ts` の一覧も同期し、既存の **15 unit tests**・型検査・lint が成功した。
  元の root の release ファイルは変更していない。
- 変更した reference **10 pages / 396 links / 32 examples / 32 native runs / 7 test projects** が成功。
  最後の prose 調整後に Alloc の **1 page / 9 links / 2 examples / 2 test projects** も再検証した。
- `last_use`・`map_set`・`channel`・`point` の既定 heap の **8 IR が byte-identical**。
  追加で O0/O3 の native object・WASM **16 成果物も byte-identical**。
  明示 counting の内部 IR は 6 counters に変わるが、旧 `tsuzuri_alloc_stats` の
  **32-byte・4-field ABI と値**は allocator suite で維持を確認した。

### 最新契約との整合

実装前の README 整合コミットは `b100231`。起票時の付録と現在の差は次のとおり。
確保数の期待値は 46 の表明窓すべてで変更していない。

- D-59 に合わせ fixture の helper の対象 `Stats` を最後にし、
  `String.contains needle text`、現在の `Array.rev`、値を受ける `Array.filter` の述語に合わせた。
- `"abcdefghij"` の反復には境界の `"j" + "ab"` に `"jab"` がある。
  JavaScript の独立した検索でも確認し、旧引数順に由来する補助表明 `!found` だけを `found` に直した。
- bench の差分は unsigned で計算し、引く前の比較で負の結果を拒否する。付録の signed-overflow UB は採用していない。
- 現在の README の T2 に従い dev build を使用した。速度の主張はしていない。
  共有 cache の metadata/open 停止を避ける専用 `TSUZURI_CACHE_DIR` と、
  UNIX socket 長制約と子の cwd 変更を満たす repo 内の短い絶対 `TMPDIR` を使用した。
- A19 との統合では `AllocCounter` を `Category::Debug` とし、理由は「確保数を読む」とする。
  `alloc` 効果ではない（D10）。A19 担当者と確認済み。

## 付録

[F14-appendix.md](F14-appendix.md): A 契約の fixture（全文）、B `std/Alloc.tz`、C `heap-counting.ll`、D HEAD での実測、E 契約 42 件の出典、F 外部の出典、G 試作。
