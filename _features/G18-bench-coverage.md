# G18: ベンチマーク・カバレッジ・プロパティテスト

| 項目 | 内容 |
| --- | --- |
| ID | G18 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G06, (D07), (E08) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（予約語 `bench`。GUIDE D-30 の仮割り当ての確定と、識別子 `bench` を壊す変更）。Phase 2（カバレッジ）は承認不要。**承認済み（2026-10-08、D-41）**: 利用者の「G16、G18、G17、G13 の実装をすべて完遂して。…すべてのフェーズを完了させること」で D1 と Phase 3 の着手を承認 |
| 改善する劣位 | Rust 比: 開発ツールの成熟度（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）／追加: 利用者コードの性能測定・カバレッジ・プロパティテストを言語のツールで行えない |
| 手本にする既存実装 | 宣言と実行器の全体: G06 の `test` 宣言（`src/parser.rs` の `Parser::test_declaration`、`src/check.rs` の `CheckedTest` と `$test.<index>` 関数の合成、`src/llvm.rs` の `Entry::TestRunner`・`emit_test_runner`、`src/test_runner.rs` の `run_tests`・`run_with_timeout`・`build_runner`・`execute_test`、`src/runtime/test-runner.c`、`src/main.rs` の `Action::Test`・`run_test_action`）。任意の型を受ける builtin: `src/check.rs` の `Builtin::Unreachable`。計装の切り替え: `src/llvm.rs` の `Instrumentation`。CLI の E2E: `tests/test_runner.rs` の `cli_reports_json_filters_and_failures_without_main` |
| 主な影響ファイル | `src/lexer.rs`, `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/formatter.rs`, `src/semantic.rs`, `src/computation.rs`, `src/polymorph.rs`, `src/check.rs`, `src/llvm.rs`, `src/test_runner.rs`, `src/coverage.rs`（新規）, `src/runtime/test-runner.c`, `src/runtime/bench-runner.c`（新規）, `src/main.rs`, `std/Bench.tz`（新規）, `tests/test_runner.rs`, `tests/bench.rs`（新規）, `tests/coverage.rs`（新規）, `tests/fixtures/coverage/Calc.tz`（新規）, `tests/frontend.rs`, `docs/language.md`, `docs/architecture.md`, `_docs/tools/testing.md`, `_docs/tools/command-line.md`, `_docs/guides/performance.md`, `_docs/feature-status.md`, `_features/README.md`, `vsc/` の予約語文法（GUIDE §6.1） |

## 目的

`cargo bench`／criterion、BenchmarkDotNet、`cargo llvm-cov`／coverlet、QuickCheck／FsCheck／proptest に相当する機能を提供する。
AGENTS.md の「実測してから主張する」方針を、利用者のコードにも適用しやすくする。

- Phase 1: `bench` 宣言、`std/Bench.tz`、`tsuzuri bench`（native だけ）。準備の時間を除いた反復時間を、予熱と複数標本の中央値・最小・最大で報告する。合否の閾値は持たない。
- Phase 2: `tsuzuri test --coverage PATH`（native だけ）。Tsuzuri の source span に対応するカウンターを IR に挿入し、行と関数のカバレッジを lcov と文字の要約で出す。
- Phase 3: プロパティテスト（設計方針だけ）。

Phase 1 と Phase 2 は互いに独立して出荷できる。実装者は Phase 2 → Phase 1 の順（D1 の承認が先に得られれば Phase 1 から）に実装し、
Phase 3 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G06 が `_features/README.md` の状態欄で done であること（HEAD で `test` 宣言と `tsuzuri test` が動く）。確認: `grep -n "G06\|G18\|G19" _features/README.md`。
- D07・E08 は開始条件にしない（D10）。
- Phase 1 は D1 の承認後に着手する。Phase 2 は承認を待たない。
- G19（edition）が done なら、D1 の予約は新しい edition だけに限る（D1）。todo なら無条件に予約する。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（test 実行器の IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `--coverage` も `bench` も使わない既存の実行で、生成 IR・診断・`tests/test_runner.rs` の期待値が 1 byte でも変わる。
- `$bench.<index>` 関数の合成（D4）に、`FunctionDecl` や `TypedExprKind` の新しい variant が必要になった。
- `Bench.consume` の inline asm（D6）が native または wasm32 の `-O0`／`-O3` のどれかで compile できない。
- カウンターの `atomicrmw`（D8）が native の対象（arm64／x86-64 の macOS・Linux・Windows）で compile できない。
- カバレッジのために `Type`・`TypedExpr` の形、または `--coverage` なしの `emit_program` の出力を変える必要がある。
- 新しい crate、`unsafe`、既定の WASM import が必要になった（lcov と JSON は std と既存の `serde_json` で書く）。
- stack-depth の 3 テスト（`tests/polymorphism.rs::bounds_type_growing_polymorphic_recursion`、`parser::tests::bounds_recursive_and_flat_expression_depth`、
  `tests/computations.rs::bounds_nested_builder_expansion_not_just_source_syntax`）か `honors_the_exact_specialization_limit` が失敗する。
- 速度の閾値を持つテストを書きたくなった（AGENTS.md。書かない）。

## 現状（HEAD `f8dc655` で確認）

- 字句: `src/lexer.rs` の `Lexer::identifier` に `"test" => TokenKind::Test`。`bench` は予約語ではない。`docs/language.md` の `## ソースと宣言` に予約語の一覧、
  `## 言語内テスト` に test 宣言の仕様がある。
- 構文: `src/syntax.rs` の `Program::tests: Vec<TestDecl>`（`name`、`name_span`、`body`、`span`）。`src/parser.rs` の `Parser::program` のループが
  `self.at(&TokenKind::Test)` で `Parser::test_declaration` を呼ぶ。`test_declaration` は文字列リテラルの名前（UTF-16 と UTF-8）、`=`、`body_expression`、
  省略可能な `;` を読む。`TokenKind::Test` は `src/parser.rs` の 3 か所の token 集合と `src/parse_control.rs`、`src/formatter.rs` にも現れる。
- 検査: `src/check.rs` は `module.program.tests` を走査して `CheckedTest { module, name, index, function, span }` を作り、`FunctionDecl`
  （名前 `$test.<index>`、`Provenance::Generated`、`Visibility::Private`、引数なし）を合成する。std の test は E1018
  `embedded standard-library sources cannot declare tests`。
- 生成: `src/llvm.rs` の `Entry { Library, Console, TestRunner }`。`emit_test_runner(module, selected, wasm)` は `emit_selected` → `emit_program` を
  `Instrumentation::default()` で呼び、`reachable_functions` の根を選んだ test 関数にする。末尾に `@tsuzuri_test_count` と、`switch` で
  `call i8 @tz.fn.<qualified_name>()` を呼ぶ `@tsuzuri_test_run(i32)` を出す。`Instrumentation` は `traps`・`debug`・`cpu_dispatch`・`wasm_threads` だけ。
- 実行器: `src/test_runner.rs`（`use super::*` で driver の非公開 item を使う）。`run_tests` は `run_with_timeout` に 30 秒を渡す。`build_runner` は
  `tests.ll` を書き、`tool("TSUZURI_CLANG", "clang")` に `-x ir -Wno-override-module -O<n>`、`native_compile_args`、`src/runtime/test-runner.c`（`main.c`、
  `-x c -std=c11`）、`-lm`、必要なら task runtime を渡す。`execute_test` は index を引数に子プロセスを起動し、stdin・stdout・stderr を null にして 5 ms ごとに
  `try_wait` する。worker 数は `available_parallelism`・32・選択数の最小。`src/runtime/test-runner.c` は引数を検査し（不正なら終了コード 2）、
  `tsuzuri_test_run` の戻り値を終了コードにする。
- CLI: `src/main.rs` の `Action::Test`、`--list`・`--index`・`--filter`・`--json`・`-O0`〜`-O3`・`--target` の解析、`run_test_action`（`driver::TestOptions` を作る）。
- 通常のビルドの clang 引数は `src/driver.rs` の `build_complete` で組み立てる（`-x ir`、`-O<n>`、`-g`、target 別の引数）。`-fprofile` や
  `-fcoverage-mapping` を渡す箇所はない。runtime（`src/runtime/*.c`）に単調時計（`clock_gettime`・`mach_absolute_time`・`QueryPerformanceCounter`）はない。
- ベンチマークは `benchmarks/*.mjs` の外部スクリプトで、利用者のプロジェクト向けではない。結果の共通形式は PX01（todo）の JSON Lines、`schema: 1`。
- `std/`・`examples/`・`tests/`・`benchmarks/` の `.tz`／`.tc`／`.tt` に識別子 `bench` はない（`grep -rnw bench` が空）。
- カバレッジ・プロパティテスト・パラメーター化テストはない。

### 再現（検証済み）

`bench` は今は識別子で、予約語を識別子に使うと E0002 になる（`test` で確認）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-g18/ident /tmp/tz-g18/kw
printf 'let bench = 1\nbench + 1\n' > /tmp/tz-g18/ident/Main.tz
printf 'let test = 1\ntest + 1\n' > /tmp/tz-g18/kw/Main.tz
target/release/tsuzuri run /tmp/tz-g18/ident   # 2 を出力し、終了コード 0
target/release/tsuzuri run /tmp/tz-g18/kw      # Main.tz:1:5: error[E0002]: expected an identifier
```

clang の計装を Tsuzuri の IR に掛けた結果（D7 の根拠）。`-fcoverage-mapping` は C の frontend が mapping を作る機能で、`.ll` の入力には何も起きない。

```sh
mkdir -p /tmp/tz-g18/p && cat > /tmp/tz-g18/p/Main.tz <<'EOF'
def add :: i64 -> i64 -> i64 = \x y -> x + y

let base = 40
let step: i64 = 2
add base step
EOF
target/release/tsuzuri build /tmp/tz-g18/p --emit llvm -o /tmp/tz-g18/p.ll
target/release/tsuzuri build /tmp/tz-g18/p --emit llvm -g -o /tmp/tz-g18/pg.ll
cd /tmp/tz-g18 && L=/opt/homebrew/opt/llvm@21/bin
$L/clang -c -fprofile-instr-generate -fcoverage-mapping p.ll -o a.o
$L/llvm-objdump -h a.o | grep -icE "prf|cov"   # 0（計装も mapping もない）
$L/clang -c -fprofile-generate p.ll -o b.o
$L/llvm-objdump -h b.o | grep -iE "prf|cov"    # __llvm_prf_cnts などはあるが __llvm_covmap はない
$L/clang -c --coverage pg.ll -o c.o && ls *.gcno   # Main.gcno（DWARF の行による gcov）
```

`/opt/homebrew/opt/llvm@21/bin` に `llvm-cov` と `llvm-profdata` はあるが、既定の `clang` は Apple clang 21.0.0 で、版の組が一致しない。

## 仕様

### 前提とする他チケットのインターフェース

- G06（done）: 上の「現状」の `test` 宣言・`CheckedTest`・`emit_test_runner`・`build_runner`・`execute_test`・CLI をそのまま使う。
- PX01（todo）: 結果レコードの欄名と意味（`workload`、`target`、`opt`、`cpu_mode`、`metric: "wall_time"`、`unit: "ms"`、`samples`、`median`、`min`、`max`。
  `wall_time` は「1 回の kernel 呼び出し。反復回数で割った値」）だけを借りる。`benchmarks/metrics.mjs` には依存しない（D9）。
- G19（todo）: done になっていれば、edition ごとの予約語表（G19 の「lexer の予約語表を edition で切り替える」）に `bench` を新しい edition の語として登録する。
- D07・E08: 使わない（D10）。

### 構文

新構文（実装後に有効。未検証）。`test` 宣言と同じ位置・同じ規則（`export`／`private`／`rec`／`and` を付けられない、同名は index で区別）。

```text
BenchDecl := "bench" StringLiteral "=" BodyExpression [";"]
```

- `.tz` と `.tc` のトップレベルだけ。`.tt` は test と同じ E1018、std は E1018。
- `bench` は予約語になる（D1）。以後 `bench` を識別子に使うと E0002 `expected an identifier`（`test` と同じ既存の経路）。
- CLI（新規）: `tsuzuri bench source.tz|directory [--list] [--filter TEXT] [--index N] [--json] [--samples N] [-O0|-O1|-O2|-O3] [--target native]`。
  既定は `-O3`、`--samples 11`（D5）。`tsuzuri test ... --coverage PATH`（新規。Phase 2）。

### 型規則

- 本体の型は `i64 -> i64`。反復回数 `n`（1 以上）を受け、準備を除いた `n` 回分の経過 ns を返す関数値である。違えば E1003（`test` 本体が `unit` でないときと同じ。
  HEAD で `test "x" = 1` は `error[E1003]: expected unit, found i32`（D-34 の前は `found i64`）を本体の位置に出す）。
- 通常は std の関数で作る。新 API（実装後に有効。未検証）:

```tsuzuri
// std/Bench.tz（新規）
def with :: (unit -> 'a) -> (ref 'a -> 'b) -> i64 -> i64
fn with setup run iterations =
    let value = setup ()
    let start = Bench.now ()
    let mut index = 0
    while index < iterations do
        Bench.consume (run (ref value))
        index <- index + 1
    Bench.now () - start

def of :: (unit -> 'b) -> i64 -> i64
fn of run iterations = with (\_ -> ()) (\_ -> run ()) iterations
```

- 組み込み（新規 `Builtin`。`Math.sqrt` と `std/Math.tz` の併存と同じ形）:
  - `Bench.now :: unit -> i64` — 任意の原点からの単調時計の ns。`tsuzuri bench` の実行器の中だけで使える（D3）。
  - `Bench.consume :: 'a -> unit` — 値を受け取り（move）、最適化器に「この値とそれが指すメモリは観測される」と扱わせてから、通常どおり drop する。
    返り値・副作用・評価順序は変えない（D6）。どの entry・target でも使える。

### 評価順序・所有権・借用

- `with` は標本ごとに 1 回 `setup` を呼び、その値を `ref` で `run` に `n` 回貸す。`run` の結果は毎回 `Bench.consume` へ move され、その場で drop される。
  `value` は `with` の終わりで drop される。準備と drop の時間は計測に入らない（最後の drop は `Bench.now ()` の後）。
- `run` が `setup` の値を変更する計測は Phase 1 では書けない（`ref` のため）。書き換えたい場合は `of` の中で毎回作る。
- `Bench.consume` の引数は通常の引数と同じく左から評価し、所有権を移す。Copy 型は複製が渡る。

### 数値・トラップ・native と WASM の差

- `Bench.now` の時計: macOS は `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`、Linux などの POSIX は `clock_gettime(CLOCK_MONOTONIC)`、Windows は
  `QueryPerformanceCounter` を `QueryPerformanceFrequency` で ns に直す（`c / f * 1e9 + c % f * 1e9 / f` の 64 bit 演算。Rust `std::time::Instant` と同じ源）。
- 反復あたりの時間は Rust 側で `ns as f64 / iterations as f64 / 1e6`（ms）。`n` 回の和を 1 回の差で測るので、時計の分解能の誤差は `1 / n` に縮む。
- 本体のトラップは標本の失敗（E2005）で、他の bench は続ける。WASM は Phase 1 で対象外（E2000）。`Bench.consume` の下げ方は wasm32 でも compile できることを確かめる。
- カバレッジのカウンターは `atomicrmw add ... monotonic` で数え、並列 task の中でも数え落とさない。u64 の和は飽和加算にする。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E0002 | 予約語 `bench` を識別子に使った | `expected an identifier`（既存） | その token |
| E0002 | `bench` の後が文字列リテラルでない | `expected a string literal bench name` | その token |
| E0002 | UTF-16 の名前が不正な surrogate を含む | `bench names must contain valid Unicode scalars` | 名前 |
| E0002 | 名前の後に `=` がない | `expected '=' after the bench name` | 次の token |
| E1003 | 本体の型が `i64 -> i64` でない | 既存の型不一致の文（`expected i64 -> i64, found unit` など） | 本体 |
| E1018 | std が bench を宣言した | `embedded standard-library sources cannot declare benchmarks` | 名前 |
| E1018 | `.tt` が bench を宣言した | test と同じ既存の文（`src/computation.rs` の同じ経路） | 名前 |
| E1018 | `tsuzuri bench` 以外で生成した IR が `Bench.now` に到達する | `Bench.now runs only under tsuzuri bench; call Bench.with or Bench.now from a bench declaration` | なし（`Span::default()`） |
| E2000 | `tsuzuri bench --target wasm32` | `tsuzuri bench supports only the native target` | なし |
| E2000 | `--samples` が 1〜1000 の整数でない | `bench samples must be an integer between 1 and 1000` | なし |
| E2000 | `--samples` を bench 以外、`--coverage` を test 以外で指定した | `--filter` を check に付けたときと同じ既存の経路と形の文 | なし |
| E2000 | `test --coverage` と `--target wasm32` | `test coverage supports only the native target` | なし |
| E2000 | `test --coverage` と `--list` | `coverage cannot be combined with --list` | なし |
| E2000 | `tsuzuri bench --index` が範囲外 | `bench index is out of range; refresh the bench list` | なし |
| E2001 | lcov を書けない | 既存の `io_error("write coverage report", path, error)` | なし |
| E2003 | `--coverage` の出力先がソース | 既存の `protect_source` の文 | なし |
| E2005 | bench の子プロセスがトラップ・異常終了・時間切れ・負の時間を返した | `benchmark '<Module>.<name>' <reason>`（reason は `trapped or exited with code N`、`timed out after 300000ms`、`returned a negative duration`） | 名前 |

### 資源上限

- `--samples` は 1〜1000。反復回数は倍々で決め、上限 `2^30`。1 標本の目標は 10 ms（`BENCH_SAMPLE_NS`（新規）= 10,000,000）。
- bench 1 件（1 プロセス）の時間切れは 300 秒（`BENCH_TIMEOUT`（新規））。bench は並列に走らせない（互いの計測を乱すため）。
- カウンターの数は region の数（ソースの大きさに比例し、E0003 の上限で抑えられる）。

### 例

Phase 1。新構文（実装後に有効。未検証）。

```tsuzuri
def total :: ref [i64] -> i64
fn total values =
    let mut sum = 0
    let mut index = 0
    while index < values.length do
        sum <- sum + values[index]
        index <- index + 1
    sum

bench "total 1e6" = Bench.with (\_ -> new [i64](1000000, \i -> i)) (\values -> total values)
bench "fixed cost" = Bench.of (\_ -> 42)
bench "bad" = ()              // E1003: expected i64 -> i64, found unit
```

Phase 2 の対象。既存の構文だけで、`tsuzuri test` で検証済み（`ok 1 - Calc positive`、`1 passed; 0 failed; 0 ignored`）。

```tsuzuri
def classify :: i64 -> i64
fn classify x =
    if x > 0 then
        1
    else
        0

def unused :: i64 -> i64
fn unused x = x + 1

test "positive" = Test.is_true (classify 5 == 1)
```

### Phase 3: プロパティテスト（設計方針）

- `Test.property` と生成器 `Gen<'a>`、縮小（shrinking）、失敗時の seed の表示。乱数は E08 の決定的 PRNG を使う。E08 と D07 の完了後に別チケットとして詳細化する。
- 見直し（2026-10-08）: 利用者の依頼（D-41）で Phase 3 も実装した。E08（`Random.pcg`）は done。設計は決定事項 D13〜D18、結果は「実装と検証」にある。入口は `Test.property` ではなく opt-in の std `Gen` の `Gen.for_all` にした（D13）。

## 設計

### データ構造

```rust
// src/syntax.rs
pub struct Program { /* 既存 */ pub benches: Vec<BenchDecl> }      // （新規 field）
pub struct BenchDecl { pub name: String, pub name_span: Span, pub body: Expr, pub span: Span } // （新規。TestDecl と同じ形）

// src/check.rs
pub struct CheckedModule { /* 既存 */ pub benches: Vec<CheckedBench> } // （新規 field）
pub struct CheckedBench { pub module: String, pub name: String, pub index: usize, pub function: usize, pub span: Span } // （新規）
pub enum Builtin { /* 既存 */ BenchNow, BenchConsume }                 // （新規 variant）

// src/llvm.rs
pub enum Entry { Library, Console, TestRunner, BenchRunner }           // BenchRunner（新規）
struct Instrumentation<'a> { /* 既存 */ coverage: Option<&'a crate::coverage::CoveragePlan> } // （新規 field）

// src/coverage.rs（新規）
pub struct CoveragePlan {
    pub regions: Vec<(usize, usize)>,          // span の開始・終了 offset。昇順。添字がカウンター番号
    index: BTreeMap<(usize, usize), usize>,    // span → region
    pub points: Vec<(usize, usize)>,           // 式の開始 offset → そこで最も内側の region
    pub functions: Vec<(String, usize)>,       // FN 名（Module.name）、本体の region
}

// src/test_runner.rs
pub struct TestOptions { /* 既存 */ pub coverage: Option<PathBuf> }   // （新規 field）
pub struct BenchOptions { pub target: Target, pub optimization: u8, pub filter: Option<String>, pub indices: Vec<usize>, pub samples: usize } // （新規）
pub struct BenchResult { pub case: CheckedBench, pub failure: Option<String>, pub iterations: u64, pub samples_ms: Vec<f64> } // （新規）
pub struct BenchReport { pub results: Vec<BenchResult>, pub ignored: usize, pub messages: Vec<String> } // （新規）
```

`$bench.<index>` の合成（D4）: test と同じ `FunctionDecl`（`Provenance::Generated`、`Visibility::Private`）で、引数は `$iterations: i64` 一つ、結果は `i64`。本体は
`{ let $case: i64 -> i64 = <利用者の本体>; $case $iterations }` に当たる `ExprKind::Block { bindings, result }`（result は `ExprKind::Call(Name($case), [Name($iterations)])`）。
`$` で始まる名前は利用者が書けないので、利用者の名前を隠さない。`Binding` と `Parameter` の field は、parser が `let step: i64 = 2` と `fn f (x: i64)` から作る値と同じにする。

### 段ごとの変更

GUIDE §6.1（予約語）・§6.4（トップレベル宣言）・§6.5（組み込み関数）・§6.6（ランタイム）の手順を HEAD に当てはめた一覧。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 字句 | `src/lexer.rs`・`src/syntax.rs` | `Lexer::identifier`、`TokenKind` | `"bench" => TokenKind::Bench`（新規） |
| 構文 | `src/syntax.rs` | `Program`、`BenchDecl`（新規） | `benches` field |
| 構文 | `src/parser.rs` | `Parser::program`、`Parser::test_declaration`、`TokenKind::Test` を含む 3 つの token 集合 | `self.at(&TokenKind::Bench)` の分岐。`test_declaration` の本体を `named_declaration(&mut self, kind: &str)`（新規）へ移し、診断文の `test` を `kind` にする（test の文は不変）。3 集合に `TokenKind::Bench` |
| 構文 | `src/parse_control.rs` | `TokenKind::Test` を含む集合 | `TokenKind::Bench` を足す |
| 整形 | `src/formatter.rs` | `program.tests` の loop、`TokenKind::Test` を含む集合 | `bench "name" = body` を test と同じ規則で出す |
| LSP | `src/semantic.rs` | `for test in &program.tests` | 同じ登録を benches に |
| 種別 | `src/computation.rs` | `program.tests.first()` の `.or_else` 連鎖 | `program.benches.first()` を足す |
| 検査 | `src/check.rs` | `module.program.tests` の loop（`CheckedTest` と `$test.<index>`） | 同じ loop を benches に。std は E1018。`$bench.<index>` を合成（D4） |
| 検査 | `src/check.rs` | `Builtin`、`Builtin::ALL`（配列長）、`name`、`signature` | `BenchNow`（`Bench.now`、`vec![Concrete(Type::Unit)]` → `Concrete(Type::I64)` 相当）、`BenchConsume`（`Bench.consume`、`Unreachable` と逆向きの `a()` → `Type::Unit`） |
| 特殊化 | `src/polymorph.rs` | `.tests` を読む箇所 | benches の関数も同じ扱い（特殊化の根） |
| 生成 | `src/llvm.rs` | `Entry`、`emit_bench_runner`（新規）、`emit_program` の `tests` 引数 | 根を bench 関数にし、`@tsuzuri_bench_count`・`@tsuzuri_bench_sample` を出す |
| 生成 | `src/llvm.rs` | `emit_builtin` | `Bench.now` と `Bench.consume` の下げ方（生成 IR の節） |
| 生成 | `src/llvm.rs` | `emit_program` の末尾（`@tsuzuri_task_parallel(` を探す箇所の近く） | `entry != Entry::BenchRunner` かつ出力に `@tsuzuri_bench_now(` があれば E1018 |
| 実行器 | `src/test_runner.rs` | `build_runner`、`run_benches`・`execute_bench`（新規） | `build_runner` を「IR 文字列と C の main を受ける」形に分け、test と bench で共有 |
| 実行器 | `src/runtime/bench-runner.c`（新規） | `main`、`tsuzuri_bench_now` | 時計・較正・予熱・標本（アルゴリズム） |
| CLI | `src/main.rs` | `Action::Bench`（新規）、引数解析、`run_bench_action`（新規）、使い方の文 | `-O` の既定を 3、`--samples` |
| std | `std/Bench.tz`（新規）、std を `include_str!` で埋め込む一覧 | `with`、`of` | `grep -rn "std/Test.tz" src` で埋め込みの箇所を見つけて並べる |
| 計装 | `src/coverage.rs`（新規） | `plan`、`merge`、`render_lcov`、`render_summary` | 型付きの本体を歩いて region と point を作る（アルゴリズム） |
| 計装 | `src/llvm.rs` | `Instrumentation`、`emit_test_runner`、`FunctionEmitter`（関数の入口）、`TypedExprKind::If`・`Match`・`While` を下ろす arm | `coverage_increment(span)`（新規）。`grep -n "TypedExprKind::If\b\|TypedExprKind::Match\b\|TypedExprKind::While\b" src/llvm.rs` の 5 か所のうち、枝の block を作る箇所すべて。`src/llvm_frame.rs` は変更なし |
| 計装 | `src/runtime/test-runner.c`、`src/test_runner.rs` | `main`、`execute_test`、`run_with_timeout`、`TestReport` | `-DTSUZURI_COVERAGE` のときだけカウンターを書く。合算と報告 |
| CLI | `src/main.rs` | 引数解析、`run_test_action` | `--coverage PATH`、要約の出力 |

### 生成 IR とランタイム

`emit_bench_runner(module, selected)` が末尾に出す形（`emit_test_runner` と同じ組み立て）:

```llvm
define i32 @tsuzuri_bench_count() {
entry:
  ret i32 2
}
define i64 @tsuzuri_bench_sample(i32 %index, i64 %iterations) {
entry:
  switch i32 %index, label %bad [
    i32 0, label %bench0
    i32 1, label %bench1
  ]
bad:
  ret i64 -1
bench0:
  %result0 = call i64 @tz.fn.<qualified_name>(i64 %iterations)
  ret i64 %result0
bench1:
  %result1 = call i64 @tz.fn.<qualified_name>(i64 %iterations)
  ret i64 %result1
}
```

`Bench.now` は `call i64 @tsuzuri_bench_now()` と、使われたときだけ一度の `declare i64 @tsuzuri_bench_now()`。`Bench.consume` の値 `%v`（型 `T = llvm_type(ty)`）は次の形に下げ、
その後で既存の `drop_value` を呼ぶ（Rust の `core::hint::black_box` と同じ障壁）。

```llvm
  %slot = alloca T
  store T %v, ptr %slot
  call void asm sideeffect "", "r,~{memory}"(ptr %slot)
```

カバレッジ（`--coverage` のときだけ）。region `K` に入る block の先頭で数え、末尾に大域変数を出す。

```llvm
  %tz.cov.7 = atomicrmw add ptr getelementptr inbounds (i64, ptr @tsuzuri_coverage_counters, i64 K), i64 1 monotonic
@tsuzuri_coverage_counters = global [N x i64] zeroinitializer
@tsuzuri_coverage_count = constant i64 N
```

`src/runtime/test-runner.c` は `#ifdef TSUZURI_COVERAGE` の中で `extern uint64_t tsuzuri_coverage_counters[]` と `extern const uint64_t tsuzuri_coverage_count` を宣言し、
`tsuzuri_test_run` が 0 を返したときだけ、環境変数 `TSUZURI_COVERAGE_FILE` の path へ `fwrite` で `count` 個の u64（native の byte 順）を書く。書けなければ終了コード 3。
`src/runtime/bench-runner.c` は `tsuzuri_bench_now` を定義する（Linux だけ include の前に `#define _POSIX_C_SOURCE 200809L`。macOS で定義すると
`clock_gettime_nsec_np` が見えなくなる）。

### アルゴリズム

bench の子プロセス（引数 `index samples target_ns`。不正なら終了コード 2）:

```text
n = 1
loop: t = sample(index, n); if t < 0: exit 3
      if t >= target_ns or n >= 2^30: break
      n = n * 2
sample(index, n) を 1 回捨てる（予熱）
print "iterations <n>"
repeat samples: t = sample(index, n); if t < 0: exit 3; print "sample <t>"
```

親（`run_benches`）: bench を index 順に 1 件ずつ起動し、stdout を読み、行の形・件数（`1 + samples`）を厳密に検査する。反復あたり ms の中央値（奇数は中央、偶数は中央 2 つの平均）・最小・最大を計算する。
文字の出力は中央値の大きさで単位（ns・us・ms・s、小数 3 桁）を選び、最小・最大も同じ単位で出す。

カバレッジの計画（`coverage::plan`）: 利用者のソース（path が `std/` で始まらない `TrapSource`）にある関数の型付き本体を、`TypedExpr::children` で歩く。
test と bench の関数（`module.tests[i].function`、`module.benches[i].function`）の本体の span の内側は除く。region は関数本体、`If` の then と else、
`Match` の各 arm の本体、`While` の本体。span で重複を除き、(開始, 終了) の昇順に番号を付ける（特殊化した複製は同じカウンターを共有する）。各式の開始 offset を、
そのとき最も内側の region と組にして point にする。

報告（`render_lcov`）: ファイルは path の昇順。行 L の数は「L にある point の region の数の最大値」。point のない行は出さない。FN は `Provenance::Generated` でない
名前付き関数だけ、行は本体の region の開始行。std と test・bench の中身は出さない。

```text
TN:
SF:<絶対 path>
FN:<行>,<Module.name>
FNDA:<数>,<Module.name>
FNF:<数>
FNH:<数>
DA:<行>,<数>
LF:<数>
LH:<数>
end_of_record
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る
（GUIDE §3.1）。手順 2〜5 が Phase 2、手順 6〜11 が Phase 1（D1 の承認後）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、「再現」の `/tmp/tz-g18/p` の IR と、「例」の Phase 2 の `Calc.tz` の test 結果を保存する。
- 確認: 次がすべて成功する。`test` は `1 passed; 0 failed; 0 ignored`、stack-depth と特殊化上限の 4 テストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-g18/p --emit llvm -o /tmp/tz-g18/before.ll
target/release/tsuzuri build /tmp/tz-g18/p --emit llvm -O3 -o /tmp/tz-g18/before-O3.ll
target/release/tsuzuri test /tmp/tz-g18/cov
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: カバレッジの計画（生成はまだ変えない）

- 変更: `src/coverage.rs`（新規）、crate の module 一覧（`src/lib.rs`）。
- 内容: `CoveragePlan`、`plan(module, sources)`、`CoveragePlan::region(span)`、`merge(counts, bytes)`（長さ `8 * N` 以外は `Err`、飽和加算）、
  `render_lcov`、`render_summary`（アルゴリズムの節）。単体テスト 4 件（テスト計画）。
- 確認: `cargo test --locked --lib coverage::` が `running 4 tests`、4 passed。

### 手順 3: カウンターを生成する

- 変更: `src/llvm.rs` の `Instrumentation`、`emit_test_runner`（引数 `coverage: Option<&CoveragePlan>` を足す。既存の呼び出しは `None`）、
  `FunctionEmitter` の関数の入口と枝の block、`emit_program` の末尾（大域変数）。
- 内容: `coverage_increment(span)`（新規）は plan に span があるときだけ `atomicrmw` を出す。`None` のときは 1 文字も出力を変えない。
- 確認: `cargo build --release --locked` の後、手順 1 の 2 つの `build --emit llvm` を `after.ll`・`after-O3.ll` へ出し、`cmp` がどちらも無出力。
  `cargo test --locked --test test_runner` が `running 7 tests`、7 passed。

### 手順 4: 実行器と CLI の `--coverage`

- 変更: `src/runtime/test-runner.c`、`src/test_runner.rs`（`TestOptions::coverage`、`Default`、`build_runner`、`execute_test`、`run_with_timeout`、`TestReport`）、
  `src/main.rs`（引数解析、`run_test_action`）。
- 内容: `--coverage` のとき clang に `-DTSUZURI_COVERAGE` を渡し、各 test に `TSUZURI_COVERAGE_FILE=<temp>/coverage-<index>.bin` を渡す。成功した test のファイルだけを
  `merge` し、lcov を PATH に書く（先に `protect_source` で検査）。要約は test の要約行の後に出す。
- 確認: 手順 1 の `Calc.tz`（test 1 件）で次を実行し、`coverage: 2/4 lines (50.0%), 1/2 functions` が出て、lcov が `DA:3,1`・`DA:4,1`・`DA:6,0`・`DA:9,0` を含む。

```sh
cargo build --release --locked
target/release/tsuzuri test /tmp/tz-g18/cov --coverage /tmp/tz-g18/cov.info && cat /tmp/tz-g18/cov.info
```

### 手順 5: カバレッジの E2E

- 変更: `tests/fixtures/coverage/Calc.tz`（新規）、`tests/coverage.rs`（新規）。
- 内容: テスト計画の 5 件。各テストは fixture を自分の temp root へ copy する（E03 の再帰的なソース探索）。
- 確認: `cargo test --locked --test coverage` が `running 5 tests`、5 passed。`cargo test --locked` が成功する。

### 手順 6: 予約語 `bench` と構文（D1 の承認後）

- 変更: 段ごとの変更の字句・構文・整形・LSP・種別の行、`tests/frontend.rs`、`tests/bench.rs`（新規）。
- 内容: `named_declaration` へ移した後も test の診断文と `parser_keeps_tests_separate_and_preserves_names_and_spans` の期待は不変。
  `tests/frontend.rs` に「以前は識別子だった `bench` が E0002 になる」テストを足す（GUIDE §6.1）。
- 確認: テスト計画の 1 と、2 のうち E0002 の 3 例だけを書く（E1003 の例は手順 7 で足す）。`cargo test --locked --test bench` が `running 2 tests`。`cargo test --locked` と stack-depth の 3 テストが成功する。

### 手順 7: 検査と builtin

- 変更: `src/check.rs`（benches の loop、`CheckedBench`、`$bench.<index>`、`Builtin`）、`src/polymorph.rs`。
- 内容: D4 の合成。`Bench.now` と `Bench.consume` の signature。
- 確認: `cargo test --locked --test bench` の 1・2 が本体の型（E1003）まで通る。`cargo test --locked honors_the_exact_specialization_limit` が成功する。

### 手順 8: 生成（builtin・E1018・bench 実行器の IR）

- 変更: `src/llvm.rs`（`Entry::BenchRunner`、`emit_bench_runner`、`emit_builtin`、`emit_program` の末尾）。
- 内容: 生成 IR の節の形。`$bench` 関数の `define` 行が `i64 (i64)` であることを `--emit llvm` の出力で確かめてから `@tsuzuri_bench_sample` を書く。
- 確認: `cargo test --locked --test bench` が `running 5 tests`（1〜5）。手順 3 の `cmp` が無出力のまま。

### 手順 9: `std/Bench.tz`

- 変更: `std/Bench.tz`（新規）、std の埋め込みの一覧（`grep -rn "std/Test.tz" src` で見つかる箇所）。
- 内容: 型規則の節の `with` と `of`。`with` の中の `ref value`・`while`・`<-` が通らなければ、同じ意味の最小の書き換えにとどめ、意味（標本ごとに 1 回の準備、
  `n` 回の `consume`、準備を含まない時間）を変えない。
- 確認: `cargo test --locked --test bench` が 5 passed。`cargo test --locked --lib` が成功する（std の検査）。

### 手順 10: bench の実行器

- 変更: `src/runtime/bench-runner.c`（新規）、`src/test_runner.rs`（`build_runner` の分割、`BenchOptions`・`BenchResult`・`BenchReport`・`run_benches`・`execute_bench`・
  統計と出力の解析）、`src/driver.rs` の `mod test_runner` の再公開。
- 内容: アルゴリズムの節。stdout は `Stdio::piped()` にし、別 thread で読み切る。時間切れは `execute_test` と同じく kill と wait。
- 確認: `cargo test --locked --lib test_runner::` に単体テスト 3 件が足され、既存の `runner_rejects_malformed_indices_and_reaps_timed_out_children` と合わせて 4 passed。

### 手順 11: CLI `tsuzuri bench` と E2E

- 変更: `src/main.rs`（`Action::Bench`、`--samples`、`run_bench_action`、使い方）、`tests/bench.rs`。
- 内容: D5 と D9 の出力。失敗した bench ごとに E2005 を stderr へ出し、1 件でも失敗すれば終了コード 1。
- 確認: `cargo test --locked --test bench` が `running 7 tests`、7 passed。`cargo build --release --locked` の後、例の `total 1e6` を含む scratch で
  `target/release/tsuzuri bench /tmp/tz-g18/bench --samples 3` が 2 行の結果と `2 benchmarks; 0 failed; 0 ignored` を出す（`bad` は除いておく）。

### 手順 12: 文書と `vsc/`

- 変更: 「ドキュメント」の表のファイル、`vsc/` の予約語文法（`grep -rnw test vsc/syntaxes` で位置を見つける）。
- 確認: `node scripts/check-docs.mjs _docs/tools/testing.md _docs/tools/command-line.md _docs/guides/performance.md` が成功し、`git diff --check` が無出力。

### 手順 13: 最終確認

- 確認: GUIDE §2.3 の基準コマンド（`cargo fmt --check`、`cargo clippy`、`cargo test --locked`）が成功し、手順 1 の stack-depth 3 件と特殊化上限が成功し、手順 3 の `cmp` が無出力。

## テスト計画

### Rust テスト

速度の閾値は書かない。時間の値は「正」「`min <= median <= max`」「件数」だけを検査する。

`src/coverage.rs` の単体テスト:

1. `plan_is_sorted_deduplicated_and_skips_std_and_tests`: 同じ module から 2 回作った plan が等しく、region が昇順、std と test 本体の span がない。
2. `lines_take_the_max_region_count`: 1 行に count 2 と 0 の point があれば 2。point のない行は出ない。
3. `merge_rejects_wrong_length_and_saturates`: 長さ 7 byte は `Err`。`u64::MAX` に 1 を足すと `u64::MAX`。
4. `lcov_is_sorted_and_exact`: 2 ファイルの plan から、path の昇順・行の昇順の完全一致の文字列。

`tests/coverage.rs`（fixture は次の 13 行。期待値は手で数えた値で、コンパイラの出力から作らない）:

```tsuzuri
def classify :: i64 -> i64
fn classify x =
    if x > 0 then
        1
    else
        0

def unused :: i64 -> i64
fn unused x = x + 1

test "positive" = Test.is_true (classify 5 == 1)
test "zero" = Test.is_true (classify 0 == 0)
test "fails" = Test.is_true (classify 7 == 0)
```

数え方: 成功する 2 件で `classify` は 2 回、then と else は 1 回ずつ。`fails` はトラップするので除く（D12）。`unused` は 0。

1. `coverage_reports_exact_lcov_at_o0_and_o3`: `-O0` と `-O3` で終了コード 1（`fails`）、lcov が次と完全一致（`SF` は copy した `Calc.tz` の絶対 path）。

```text
TN:
SF:<root>/Calc.tz
FN:3,Calc.classify
FN:9,Calc.unused
FNDA:2,Calc.classify
FNDA:0,Calc.unused
FNF:2
FNH:1
DA:3,2
DA:4,1
DA:6,1
DA:9,0
LF:4
LH:3
end_of_record
```

2. `coverage_json_and_text_summaries`: 文字は `coverage: 3/4 lines (75.0%), 1/2 functions; excluded 1 failed test`（`; excluded ...` は除いた件数が 1 以上のときだけ）。`--json` の最後の行が
   `{"type":"coverage","lines":{"hit":3,"total":4},"functions":{"hit":1,"total":2},"excluded_failed":1,"files":[...]}`。
3. `coverage_counts_parallel_work_exactly`: 1,000 要素の `Parallel.map` の lambda の中の `if` の then が 1,000（引数の形は `_docs/library-reference/` の
   `Parallel.map` の例に合わせる）。atomic でなければ数え落とす。
4. `coverage_rejects_wasm_list_and_source_output`: `--target wasm32` と `--list` は E2000、出力先が `Calc.tz` なら E2003 でファイルは不変。
5. `runner_ir_without_coverage_has_no_counters`: `emit_test_runner(.., None)` の IR に `tsuzuri_coverage` と `atomicrmw` がない。

`tests/bench.rs`:

1. `parses_formats_and_indexes_bench_declarations`: `Program::benches` の件数・名前・`name_span`、`fmt` の往復、同名 2 件の index が 0 と 1。
2. `rejects_bench_identifiers_and_malformed_declarations`: `let bench = 1` → E0002、`bench 1 = ...` → E0002（`expected a string literal bench name`）、
   `bench "a" 1` → E0002、`bench "a" = ()` → E1003。
3. `normal_and_test_emission_exclude_benches`: bench 宣言を足しても Console と TestRunner の IR が byte 単位で同じ。
4. `bench_now_outside_bench_runner_is_rejected`: `Main.tz` の `Bench.now ()` を build すると E1018。`tsuzuri test` で test が呼んでも E1018。
5. `consume_lowers_to_an_asm_barrier_on_native_and_wasm`: IR に `call void asm sideeffect "", "r,~{memory}"(ptr` があり、native と wasm32 × `-O0`／`-O3` の
   `build` が成功し、native の `run` が `Bench.consume` の前後で同じ値（例: `Bench.consume 41` の後に `42`）を出す。
6. `cli_runs_lists_filters_and_reports_failures`: `total`・`fixed cost`・`{ assert false; 0 }` を返す `traps` の 3 件で `--json --samples 3`。終了コード 1、成功 2 行は
   `samples` が 3 件、`iterations` が 2 の冪で `2^30` 以下、`0 < min <= median <= max`、`metric` が `wall_time`、`unit` が `ms`、`opt` が `O3`。失敗行は `status: failed`、
   stderr に E2005。`--list` が `0 <Module>.total` の形、`--filter total` の要約が `2 ignored`、`--index 9` が E2000。
7. `cli_rejects_wasm_and_invalid_samples`: `--target wasm32`、`--samples 0`、`--samples 1001`、`test --samples 3`、`bench --coverage x` が E2000。

`src/test_runner.rs` の単体テスト: `bench_statistics_use_median_min_max`（`[3, 1, 2]` → 2、`[4, 1, 3, 2]` → 2.5）、`bench_output_is_parsed_strictly`
（`iterations` 行の欠落、標本数の不一致、数でない値、負の値を拒否）、`bench_runner_rejects_malformed_arguments`（C の main が終了コード 2）。

### E2E

- 両機能とも native だけ（D11）。`tests/coverage.rs` は `-O0`／`-O3`、`tests/bench.rs` は既定の `-O3` と `-O0` の 1 件。`Bench.consume` だけは wasm32 の build も確かめる。
- 既存の `tests/*.mjs` の suite は変えない（生成の共有部を変えないため）。WASM の import は増えない（bench 実行器は native だけ）。

### 既存テストへの影響

- `tests/frontend.rs` の予約語の一覧を検査するテストがあれば、`bench` の追加だけが正当な変化。それ以外の期待値は変わらない。
- `TestOptions` の field 追加に伴い、`TestOptions { .. }` を作る箇所（`src/main.rs` の `run_test_action`）を更新する。期待値は変わらない。

### 性能

- `--coverage` なしの IR は不変（手順 3）。`tsuzuri bench` の結果の数値は合否に使わない（AGENTS.md）。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/language.md` | `## ソースと宣言` | 予約語の一覧に `bench` |
| `docs/language.md` | `## 言語内テスト` | bench 宣言、本体の型、`Bench.with`・`Bench.of`・`Bench.now`・`Bench.consume`、`--coverage` |
| `docs/architecture.md` | test 実行器の説明 | bench 実行器（子プロセス、較正、標本）とカバレッジの計装（region、カウンター、合算） |
| `_docs/tools/testing.md` | `## カバレッジ`（新規。`## JSON と終了コード` の後） | `--coverage PATH`、lcov と要約、失敗した test の除外、native だけ |
| `_docs/tools/command-line.md` | コマンドの一覧 | `bench INPUT`、`--samples`、`--coverage` |
| `_docs/guides/performance.md` | `## 再現可能な測定` | `tsuzuri bench` の使い方、`Bench.consume` が要る理由、閾値を持たないこと |
| `_docs/feature-status.md`・`_features/README.md` | G18 の行 | 実装した Phase と状態 |

GUIDE D-30 の `bench`（予約語）と `Bench`（std）の行を D-15・D-07 へ移すのは、D1 の承認に含めて人間が行う。

## 受け入れ条件

- [ ] Phase 2: `tsuzuri test --coverage PATH` が fixture の lcov を `-O0`／`-O3` で完全一致に出し、要約と JSON の行を出す。
- [ ] Phase 2: `--coverage` なしの IR と test の出力が手順 1 のベースラインと同じ。
- [ ] Phase 1（D1 の承認後）: `bench` 宣言を `tsuzuri bench` で計測し、反復回数と中央値・最小・最大を文字と JSON Lines で出す。閾値を持たない。
- [ ] Phase 1: 通常ビルドと `tsuzuri test` の IR が bench 宣言の有無で変わらない。`Bench.now` の誤用は E1018。
- [ ] `tests/coverage.rs`・`tests/bench.rs`・単体テストが成功し、`running N tests` の N がテスト計画の件数と一致する。
- [ ] stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `-fprofile-instr-generate -fcoverage-mapping` を `.ll` に渡しても何も起きない（「再現」）。LLVM の coverage mapping を自作しない（D7）。
- `llvm@21` の `llvm-profdata`／`llvm-cov` は Apple clang の raw profile と版が合わないことがある。本チケットでは使わない。
- `-std=c11` の glibc は `_POSIX_C_SOURCE` なしで `clock_gettime` を宣言しない。macOS で `_POSIX_C_SOURCE` を定義すると `clock_gettime_nsec_np` が消える。
- test の worker は並列に動く。カバレッジのファイルは index ごとに分け、1 つのファイルを共有しない。
- トラップした test は `fwrite` の前に終わる。数を 0 として足さず、除いたことを要約に書く（D12）。
- `build_runner` の分割で test の IR や clang の引数が変わると、手順 3 の比較か `tests/test_runner.rs` が落ちる。引数の順序も保つ。
- bench の stdout を pipe にしたまま `try_wait` だけ回すと、出力が pipe の容量を超えたときに止まる。読み取り thread を先に始める。
- bench を並列に走らせない。test の worker の仕組みを流用しない。
- 特殊化・lambda の持ち上げで span が変わると、plan の region と生成時の span が合わず数が 0 になる。E2E の 1 がこれを検出する。
- 各テストは自分の temp root を使う（E03）。`cargo test --locked <filter>` は 0 件でも成功する。
- `bench` の予約で `_docs` の実行可能な例が壊れることがある。`node scripts/check-docs.mjs` で確かめ、識別子を改名する。

## 対象外

- WASM の bench とカバレッジ（D11 の方針だけ）。分岐の lcov（`BRDA`）、`&&`／`||` の region、MC/DC、HTML の報告、LLVM の coverage mapping。
- 前回の結果との比較、CI での速度の合否判定、分散・継続的な保存サービス。`tsuzuri bench --cpu native`。
- PX01 の完全なレコード（`schema`・`run_id`・`commit`・`host` の全欄）への変換（PX01 の `benchmarks/metrics.mjs` の側）。
- パラメーター化テスト。Phase 3（プロパティテスト）は 2026-10-08 に実装した（D13〜D18）。

## 決定事項

### D1: 構文と予約語

- 決定: 予約語 `bench`（GUIDE D-30 の仮割り当て）と `bench "name" = body`。識別子 `bench` は E0002 になる破壊的変更で、移行は改名（例: `benchmark`）。
  リポジトリの `.tz`／`.tc`／`.tt` に識別子 `bench` はない（「現状」）。G19 が先に done なら新しい edition だけで予約する。
- 理由: 代替を比べた。文脈キーワード（トップレベルで `bench` の後に文字列と `=` が続くときだけ）は壊さないが、`TokenKind::Test` を含む parser の 3 集合と
  `src/parse_control.rs` がトークンの種類で宣言の境界を決めており、各所に 3 token の先読みが要る。`.tb` の新しいソース種別は探索・E1018 の表・LSP・整形・`vsc/` に
  広がる。std の `Bench.run` を `main` から呼ぶ形は `--list`・`--filter`・プロセスの隔離ができない。test の修飾子は test の意味（合否）と混ざる。
- 状態: 承認済み（2026-10-08、D-41）。G19 は todo のままなので、`bench` は無条件に予約した（edition による切り替えはない）。

### D2: 本体の型と std の API

- 決定: 本体は `i64 -> i64`（反復回数 → 準備を除く ns）。`Bench.with :: (unit -> 'a) -> (ref 'a -> 'b) -> i64 -> i64` と `Bench.of :: (unit -> 'b) -> i64 -> i64` を std に置く。
- 理由: record や新しい型を作らずに、準備の分離と独自の計時（`Bench.now` を直接使う本体）の両方を表せる。実行器は `i64 (i64)` の関数だけを呼べばよい。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: `Bench.with` は `Bench.with_input` にした。`with` は `match ... with`・`try ... with`・`{ r with f = v }` の予約語で、`def with` も `Bench.with` も E0002 になる（`union`・`new` のような例外を parser・formatter・LSP・`vsc/` の文法に足すより、名前を変える方が影響が小さい）。名前は criterion の `bench_with_input`（入力を全反復で参照として共有する）に合わせた。`Bench.now` は引数なしの組み込み（`Math.pi()` と同じ `fn() -> i64`）で、`Bench.now()` と呼ぶ（`Bench.now ()` は空白のため E1006）。std の `Bench` は D-40 の opt-in モジュール（`stdlib::OPT_IN`）にし、識別子 `Bench` を書かないプログラムの型検査と IR は変えない。`Bench.of` は `with_input` を経由せず直接ループを書く（計測するループに余分な closure 呼び出しを入れないため）。`<-` は代入ではないので `=` で書いた。

### D3: `Bench.now` を使える範囲

- 決定: `tsuzuri_bench_now` を定義するのは bench 実行器の C だけ。他の entry の IR が参照すれば E1018（位置なし）。
- 理由: 通常ビルドの runtime に時計を足さずに済み、リンクエラーより早く分かりやすい。汎用の時計は E08 の `Time` の役目。
- 状態: 既定案（実装者はこの案に従う）

### D4: `$bench.<index>` の合成

- 決定: 設計の「データ構造」の形（引数 `$iterations`、型注釈付きの `$case` 束縛、`ExprKind::Call`）。std の bench は E1018。
- 理由: 型の不一致が test と同じ E1003 で本体に出る。`$` 名は利用者の名前を隠さない。実行器の IR が test と同じ直接呼び出しで済む。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: 合成は設計どおり（`check::bench_function`）。`FunctionOrigin.bench: Option<usize>` を足し、補助関数へ伝播させる（カバレッジの除外にも使う）。`polymorph::specialize` は bench 関数の要求を、ほかのすべての特殊化と Drop の固定点が終わった後に出す（先に出すと、通常ビルドとテストビルドの `$mono.N` が bench の数だけずれた）。その後半で初めて見つかった Drop 型の drop 関数は `CheckedModule.bench_drops` に記録し、`reachable_functions` は bench 実行器のときだけ根にする（bench だけが持つ型の drop glue が通常の成果物に出ていた）。ただし `$lambda.N`・`$instance.N` は関数の総数から付く名前なので、test を足したときと同じく、bench の宣言と opt-in の `Bench` モジュールの読み込みで番号がずれる。`tests/bench.rs` の 3 は「生成名の番号を出現順に振り直して一致」と「ラムダ・instance のないプログラムで byte 一致」を確かめる（bench を宣言しないプログラムは base と byte 一致。§実装と検証）。

### D5: 実行と統計

- 決定: 既定 `-O3`（`-O0`〜`-O3` を指定可）、`--samples 11`（1〜1000）、1 標本 10 ms を目標に反復回数を 1 から倍々（上限 `2^30`）、予熱 1 標本、bench ごとに別プロセスで
  順に実行、時間切れ 300 秒。反復あたりの中央値・最小・最大を出す。MAD は出さない。
- 理由: PX01 の `wall_time`（9 標本以上、中央値・最小・最大）に揃える。`-O0` の計測は利用者の判断を誤らせる。倍々の較正が予熱も兼ねる。
- 状態: 既定案（実装者はこの案に従う）

### D6: `Bench.consume` の意味と下げ方

- 決定: 値を move で受け、stack slot に store し、`asm sideeffect "", "r,~{memory}"` に slot の pointer を渡してから drop する。volatile store だけの形は使わない。
- 理由: volatile store は値の計算を残すが、pointer の先の書き込みは DSE で消えうる。memory clobber 付きの asm は Rust の `black_box` と同じく、到達できるメモリの読み書きを仮定させる。
  wasm32 で compile できない場合は停止条件。
- 状態: 既定案（実装者はこの案に従う）

### D7: カバレッジの方式

- 決定: Tsuzuri 自身の計装（region ごとの u64 カウンター、Rust で lcov を書く）。LLVM の source-based coverage と gcov は使わない。
- 理由: `-fcoverage-mapping` は `.ll` に効かず（「再現」）、mapping は版に依存する形式を自作する必要がある。`--coverage`（gcov）は `.gcno` を作るが行単位で、
  `if c then a else b` の 1 行を分けられず、DWARF の行の欠け（match の節）と `-O3` の最適化に左右され、`llvm-cov gcov` の発見と版合わせが要る。自前の計装は外部 tool なしで
  決定的で、元のチケットの既定案とも一致する。
- 状態: 既定案（実装者はこの案に従う）

### D8: region と行の規則

- 決定: region は関数本体、`If` の両枝、`Match` の各 arm、`While` の本体。行の数は point の region の最大値。関数は FN／FNDA、行は DA／LF／LH。std と test・bench の本体は除く。
  到達しない関数は 0。インスタンス化されない汎用関数の本体は、型付きの本体がなければ出さない。
- 理由: 枝の block の入口だけで数えれば、生成の変更は 1 か所の helper と 5 か所の呼び出しで済み、1 行の `if` も llvm-cov と同じく実行されたと数えられる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: region の鍵は `(source, start, end, 種類)` にした（複数ファイルのプロジェクトと、同じ span の別の構文を分けるため）。`else` のない `if` は then の span を持つ `()` を else に生成するので、その else は region にも point にもしない。`for`（`ForRange`・`ForEach`）の本体と `try ... with` のハンドラーも region にした（ループの本体と、例外のときだけ走るハンドラーが外側の回数で数えられないように）。持ち上げたラムダと `task`（module `$lambda`・`$task`）は自分の本体を region にする（並列の lambda の中の行が外側の 1 回で数えられないように）。test と bench の除外は span の包含ではなく `FunctionOrigin.test`（Phase 1 で `bench` も）で行い、テスト本体の中のラムダも除く。`match` のパターンとガードの式は point にしない（腕が選ばれる前に走り、回数が定まらない）。数える関数では `lookup_match` の定数表を使わない（腕ごとの block がないため）。組み込み・case・export のラッパー（module `$builtin`・`$intrinsic`・`$to_string`・`$case`・`$export`）は数えない。

### D9: 出力形式

- 決定: bench は文字（`bench <index> <Module>.<name>: median <v> <unit> (min <v> <unit>, max <v> <unit>; <S> samples of <N> iterations)` と
  `<B> benchmarks; <F> failed; <I> ignored`）と `--json` の JSON Lines（`"type":"bench"` の行は PX01 の欄名 `workload`・`target`・`opt`・`cpu_mode: "generic"`・`metric`・`unit`・
  `samples`・`median`・`min`・`max` と `index`・`module`・`name`・`status`・`iterations`、最後に `"type":"summary"`）。カバレッジは lcov（PATH）と要約（文字または JSON の 1 行）。
- 理由: PX01 の変換を欄の追加だけで済ませつつ、利用者のプロジェクトで知りえない `commit` や `rustc` を CLI に持ち込まない。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: 文字の行の `<index>` は `--list`・`--index` と同じ 0 始まり。要約の `<B>` は実行した件数で、1 件なら `1 benchmark`。`samples` は各標本の 1 回あたりの ms の配列。失敗した bench が標準エラーに書いた内容（最後の 64 KiB）は、その行の下（JSON は `output`）に出す。bench の実行器の C の終了コードは、引数の誤りが 2、負の時間が 3、標準出力の書き込みの失敗が 4。

### D10: D07 と E08

- 決定: 依存欄の `(D07)`・`(E08)` は Phase 1・2 の開始条件にしない。Phase 3 だけが E08 の PRNG を要する。
- 理由: 名前の文字列は既存の文字列リテラル、時計は bench 実行器の C で足りる。
- 状態: 既定案（実装者はこの案に従う）

### D11: target

- 決定: Phase 1・2 は native だけ。WASM は後続の方針として、bench の成果物だけに Node の import（`tsuzuri_bench_now`）を足し、カバレッジはカウンターを export した
  メモリから Node が読む。どちらも既定の WASM には import を足さない（D-18）。
- 理由: 時計の import は D-18 の opt-in の扱いが要り、Node の runner の変更も伴う。native で形式と意味を固めてから広げる。
- 状態: 既定案（実装者はこの案に従う）

### D12: 失敗した test のカバレッジ

- 決定: 成功した test のカウンターだけを合算し、除いた件数を要約に出す。
- 理由: トラップは `fwrite` の前にプロセスを終わらせる。signal handler での書き出しは async-signal-safe の制約と OS 差が大きい。
- 状態: 既定案（実装者はこの案に従う）

### D13: プロパティテストの置き場所と API（Phase 3）

- 決定: 生成器と実行を opt-in の std モジュール `Gen`（D-40、`stdlib::OPT_IN`）に置く。実行は `Gen.for_all :: Display<'a> => Gen<'a> -> ('a -> bool) -> unit`（100 件）、`Gen.for_all_cases :: Display<'a> => i64 -> Gen<'a> -> ('a -> bool) -> unit`、`Gen.check :: (ref 'a -> string) -> i64 -> Gen<'a> -> ('a -> bool) -> unit`。`Test.property` は作らない。生成器は `Gen.i64()`・`Gen.i32()`・`Gen.range`・`Gen.bool()`・`Gen.f64()`・`Gen.char()`・`Gen.unicode_char()`・`Gen.string()`・`Gen.string_of`・`Gen.array`・`Gen.array_up_to`・`Gen.maybe`・`Gen.result`、組み合わせは `Gen.constant`・`Gen.map`・`Gen.bind`・`Gen.pair`・`Gen.one_of`・`Gen.element`。`Gen<'a>` は不透明（`stdlib::opaque_record`）。
- 理由: `Test` は常に読み込む std モジュールなので、そこから `Gen` を使うと `Test` を書くすべてのプログラムが `Gen` の型検査を負う（D-40 に反する。`Test` を opt-in にしても `uses` で同じ）。生成器と実行を 1 つの opt-in モジュールに置けば、性質を書かないプログラムの型検査と IR は変わらない。名前は QuickCheck の `forAll` に合わせた。コンパイラへの追加は seed の組み込み 1 つ（D15）と実行器の出力の読み取り（D16）だけで、生成・縮小・報告は std の Tsuzuri で書いた。
- 状態: 承認済み（2026-10-08、D-41）

### D14: 生成と縮小の方式（選択の列）

- 決定: Hypothesis 型の統合縮小。生成器はすべての乱択を上限付きの `i64u` の「選択」として記録しながら値を作り（上限 0 の選択は記録しない）、選択が小さいほど単純な値になるように作る（整数は「0 に最も近い値からの向き」と「距離」、配列は要素の前ごとの「続けるか」、`maybe`・`result`・`one_of`・`element` は先頭が最も単純）。縮小は記録を短く・小さくした候補を再生し（記録が尽きたら 0）、性質が失敗し続け、短長辞書順でより単純な記録だけを採用する。手順は「連続した 8〜1 個の選択の削除」「各選択の 0 への置き換えと二分探索」を変化がなくなるまで繰り返し、性質の実行は 1 つの反例につき最大 10,000 回。
- 理由: QuickCheck 型の型ごとの縮小関数は `map` を通すと縮小できない。Hedgehog 型の rose tree は遅延の子を持つ再帰的な木が要り、Tsuzuri の record（再帰は union のノードだけ）で表しにくい。選択の列は関数値を持つ record 1 つと配列だけで書け、`map`・`bind` を通しても縮み、決定的。
- 状態: 承認済み（2026-10-08、D-41）

### D15: seed、件数、上書き

- 決定: 既定の seed は時刻に依存しない固定値 `0x9E3779B97F4A7C15`（`11400714819323198485`、`llvm::DEFAULT_PROPERTY_SEED`）。i 番目（0 始まり）の値は `Random.pcg seed i` の stream で選ぶ。既定の件数は 100、`Gen.for_all_cases n` で性質ごとに変える。上書きは `tsuzuri test --seed N`（0〜18446744073709551615、test だけ。2 回目と範囲外は E2000）。環境変数は足さない。seed は std 専用の組み込み `Gen.__seed()`（`fn() -> i64u`。`Gen` 以外からは E1022）で、`emit_builtin` がコンパイル時の定数を返す。
- 理由: 値の番号ごとに独立した stream なので、`--filter` で 1 件だけ動かしても同じ値になる。テスト名から seed を派生させないのは、性質を別のテストへ移しても結果を変えないためと、テスト名を std へ渡す仕組みを要さないため。純粋なコードは環境変数を読めない（IO が要る）ので、実行時ではなくコンパイル時の定数にした。CLI 1 つで再現の手順が完結する（GUIDE §6.8）。
- 状態: 承認済み（2026-10-08、D-41）

### D16: 失敗の報告

- 決定: 反例を見つけたら縮小し、`property failed at case N of M (seed S)`・`counterexample: X`・`shrunk K times from Y` の 3 行を `Debug.print` で標準エラーへ書き、`assert false` でトラップする。`tsuzuri test` は子の標準エラーを読み取りスレッドで読み（末尾 64 KiB）、失敗したテストだけ失敗理由の下に字下げして出す（JSON は失敗行の `output`）。WASM は、`Gen` の関数を含むプログラムのテストモジュールだけを `debug_output` 付きで出し、`runtime/test-runner.mjs` は `tsuzuri_debug.write` だけを import として許して `writeSync(2, ...)` で書く。性質の中のトラップは縮小せず、そのまま失敗する。
- 理由: テストは子プロセスなので、親が子の出力を読むのが最も単純で、ほかの言語の test 実行器（失敗したテストの出力だけを出す）とも同じ。`Gen` を使わないプログラムの WASM のテストモジュールは import なしのまま（D-18）。トラップは捕まえられない（例外は `@checked` だけ）ので、条件は `bool` で返す。
- 状態: 承認済み（2026-10-08、D-41）

### D17: 反例の表示

- 決定: `Gen.for_all` は `Display<'a>` を要求し、`Display` のない型（`Maybe`・`Result`・`deriving (Display)` のない record など）には表示関数を受ける `Gen.check` を使う。
- 理由: std の `Maybe`・`Result` に `Display` の instance を足すと、利用者が書ける `instance Display<Maybe<i64>>` と重なり、既存のコードを壊す（確かめた）。
- 状態: 承認済み（2026-10-08、D-41）

### D18: 既定の生成器の範囲

- 決定: `Gen.i64()`・`Gen.i32()` は全範囲で、探索ではビット長を一様に選ぶ（小さい値が多い）。`Gen.range` はほぼ一様（剰余による偏りは範囲が 2^63 に近いときだけ）。`Gen.f64()` は整数部 52 ビットまでと 20 ビットの小数部の有限値で、NaN・無限大・`-0.0` は作らない。`Gen.char()` は印字 ASCII、`Gen.unicode_char()` はサロゲート以外の UTF-16 コード単位（どちらも `'a'` へ縮む）。配列と文字列の既定の上限は 32。
- 理由: 既定の生成器は単純な値へ縮む順序を持つ必要があり、NaN や無限大は `x == x` のような自然な性質を壊す。全ビットパターンの浮動小数点は後続の課題。
- 状態: 承認済み（2026-10-08、D-41）

## 実装と検証（2026-10-08）

HEAD `1182045`（`Phase7-5`）から、利用者の依頼（GUIDE D-41）に従って Phase 2 → Phase 1 → Phase 3 の順にすべて実装した。

### Phase 2: `tsuzuri test --coverage PATH`

- `src/coverage.rs`（新規）: `plan`（region と point）、`merge`（長さ `8 * N` 以外は `Err`、飽和加算）、`files`（行は point の region の最大値、FN は本体の region の開始行）、`render_lcov`、`totals`・`render_summary`。単体テスト 4 件。
- 生成: `llvm::emit_test_runner_covered(module, selected, plan)`（native だけ）が `Instrumentation.coverage` を `Globals.coverage` に置き、`FunctionEmitter::cover(kind, span)` が region の block の先頭で `atomicrmw add ptr getelementptr inbounds ([N x i64], ptr @tsuzuri_coverage_counters, i64 0, i64 K), i64 1 monotonic` を出す。呼び出しは関数の入口（`emit`）、`if` の 3 つの下げ方（`emit_tail`・`expression_mode`・`llvm_frame` の `frame_inner`）の両枝、`loop_body`（`while`・範囲 `for`・`for each` の共通部）、`match_expression` の腕、`try_expression` のハンドラー。末尾に `@tsuzuri_coverage_counters = global [N x i64] zeroinitializer` と `@tsuzuri_coverage_count = constant i64 N`。計画のない出力は変わらない（`counted` が偽なら `cover` は何も出さない）。
- 実行器: `src/runtime/test-runner.c` は `-DTSUZURI_COVERAGE` のときだけ、テストが 0 を返したらカウンターを `TSUZURI_COVERAGE_FILE` へ書く（失敗は終了コード 3。Windows は `_CRT_SECURE_NO_WARNINGS`）。`src/test_runner.rs` は `TestOptions.coverage: bool`、`TestReport.coverage: Option<CoverageRun>`、`Runner.coverage`（カウンターのファイルの置き場）、`merge_coverage`、`check_coverage_output`（E2003）、`write_coverage`（`cache::real_path` の絶対 path で lcov を書く。E2001）。`build_runner` は引数 `coverage` を足しただけで、`None` のときの IR と clang の引数の順序は変わらない。
- CLI: `--coverage PATH`（test だけ。2 回目、`--target wasm32/wasm64`、`--list` は終了コード 2 の E2000）、要約はテストの要約行の後（`--json` は最後の行 `{"type":"coverage",...}`）。lcov はテストが失敗しても書き、終了コードは 1 のまま。
- 見直し（2026-10-08）: チケットの `TestOptions { coverage: Option<PathBuf> }` は `bool` にした（lcov を書くにはプロジェクトのソースが要るので、出力は `driver::write_coverage(project, run, path)` が行う）。E2E の `coverage_counts_parallel_work_exactly` は 1,000 要素ではなく 100,000 要素にした（`Parallel.map` は 4,096 要素ごとのチャンクなので、1,000 要素は 1 スレッドで終わり、原子性を試せない）。
- 検証: `cargo test --locked --lib coverage::` 4 passed、`cargo test --locked --test coverage` 5 passed（lcov の完全一致は `-O0`／`-O3`、並列の 100,000 回）、`cargo test --locked --test test_runner` 8 passed、`--lib test_runner` 2 passed、GUIDE §3.1 の 4 件 passed、`cargo clippy --locked --all-targets -- -D warnings` 成功、`node scripts/check-docs.mjs`（test.md・usage.md・option.md）成功。
- 生成 IR の不変（base `1182045` の release と比較）: examples と tests/fixtures の全プロジェクトの `build --emit llvm` を native／wasm32 × `-O0`／`-O3` で 324 組比較し、すべて byte 単位で一致。テスト実行器の IR（`emit_test_runner`、native と wasm32）も、base の木で同じ dump を作って 4 プロジェクト 8 組が一致。

### Phase 1: `bench` 宣言、`std/Bench.tz`、`tsuzuri bench`

- 字句・構文: `TokenKind::Bench`（予約語 `bench`。G19 は todo なので無条件）、`Program.benches: Vec<BenchDecl>`、`Parser::named_declaration(kind)`（test の診断文は不変）、`bench` を token 集合（`is_top_level_declaration_start`・文の終わり 2 か所・`parse_control`・formatter）に追加。formatter・LSP の document symbol（`bench "名前"`）・`computation.rs` の `.tt` の E1018 も test と同じ扱い。`lsp::KEYWORDS`（43 語）と `vsc/` の文法・`editor.ts`・`core.ts`・snippet に `bench`。
- 検査: `CheckedBench`、`CheckedModule.benches`、`$bench.<index>`（D4）、std の bench は E1018 `embedded standard-library sources cannot declare benchmarks`。組み込み `Bench.now`（`fn() -> i64`）と `Bench.consume`（`'a -> unit`）。
- 生成: `Entry::BenchRunner`、`llvm::emit_bench_runner`、`@tsuzuri_bench_count`・`@tsuzuri_bench_sample`。`Bench.now` は `declare i64 @tsuzuri_bench_now()` を intrinsics に登録し、bench 実行器以外の出力にあれば E1018。`Bench.consume` は alloca・store・`call void asm sideeffect "", "r,~{memory}"(ptr ...)`・`drop_value`（native と wasm32 の `-O0`／`-O3` で build でき、wasm の import は増えない。`-O3` の arm64 の機械語で、配列の書き込みと store が残ることを確かめた）。
- 実行器: `src/runtime/bench-runner.c`（新規。時計・較正・予熱・標本）。`src/test_runner.rs` の `build_runner` の native 部分を `compile_native_runner`（C の入口、`-D`、必要なランタイム、リンク入力）に分けて test と bench で共有した（test の IR と clang の引数の順序は不変）。`BenchOptions`・`BenchResult`（`statistics`）・`BenchReport`・`run_benches(_linked)`・`execute_bench`・`parse_bench_output`・`run_captured`（stdout と stderr を読み取りスレッドで読み切る）。
- CLI: `tsuzuri bench`（`Project::load_for_tests` と `analyze_all` を test と同じ分岐で使う）、`--samples N`（bench だけ。1〜1000）、`--list`・`--filter`・`--index`・`--json`・`-O`・`--target native`・リンク入力。`--filter`・`--list`・`--index` の誤用の文は `only valid with test or bench` に変えた。`bench` の `--cpu` と `--target wasm32/wasm64` は E2000。
- 予約: std モジュール `Bench`（`RESERVED_MODULES` は 44 件。`stdlib::tests::reserves_the_d07_table` の期待値を 43 から 44 に更新した。新しい予約モジュールに伴う正当な変化）。
- 検証: `cargo test --locked --test bench` 7 passed、`--test frontend` 4 passed（`bench_became_a_reserved_word` を追加）、`--lib` 118 passed（`test_runner::` の bench 単体テスト 3 件を含む）、`--test coverage` 5 passed、`--test test_runner` 8 passed、`--test formatter` 16 passed。`node scripts/check-docs.mjs`（bench.md・keywords.md・usage.md・option.md・diagnostics.md・strategy.md・index.md・test.md）成功。

### Phase 3: プロパティテスト（`std/Gen.tz`、`tsuzuri test --seed`）

- std: `std/Gen.tz`（新規。opt-in、`RESERVED_MODULES` は 45 件、`opaque_record` に `Gen.Gen`）。生成器・組み合わせ・`attempt`（生成と性質の実行）・`shrink`・`describe`・`check`・`for_all`・`for_all_cases` を Tsuzuri で書いた（D13・D14）。`Source` は `(Random.Pcg * [i64u] * bool * Vec<i64u>)` の組で、`match` で分解して受け渡す（非 Copy の record の更新は元の束縛を読めないため）。
- コンパイラ: 組み込み `Gen.__seed`（`Builtin::GenSeed`、`fn() -> i64u`、`Gen` 以外は E1022）、`Globals.seed`・`Instrumentation.seed`、`llvm::TestRunnerOptions`（`wasm`・`memory64`・`coverage`・`seed`・`debug_output`）と `emit_test_runner_with`、`llvm::DEFAULT_PROPERTY_SEED`。`emit_test_runner_for`・`emit_test_runner_covered` はその wrapper にした（出力は不変）。
- 実行器: `execute_test` を `run_captured`（Phase 1 の bench と共有。標準出力は捨て、標準エラーの末尾 64 KiB を読み切る）に切り替え、`TestResult.output`（失敗したテストだけ）を足した。CLI は失敗理由の下に字下げして出し、JSON は失敗行の `output`。`runtime/test-runner.mjs` は `tsuzuri_debug.write` だけを import として許し、`Gen` を含むプログラムの WASM のテストモジュールだけが `debug_output` 付きになる（D16）。
- CLI: `tsuzuri test --seed N`（`TestOptions.seed`）。test 以外、2 回目、範囲外は E2000。
- 見直し（2026-10-08）: 失敗したテストの標準エラーを表示するように変えた（これまでは捨てていた）。標準エラーに書く失敗したテストの報告に行が増えるが、成功したテストの出力、失敗理由、JSON の既存の欄は変わらない。`Debug.print` を使うテストの WASM の出力は、`Gen` を使わないプログラムでは従来どおり何も書かない。
- 検証: `cargo test --locked --test property` 4 passed（20 個の失敗する性質の反例を手で求めた最も単純な値と完全一致で照合、2 回の実行・`-O3`・wasm32 で報告が一致、`--seed 7` と最大の seed、`--seed` の誤用、`Gen.__seed` と `Gen` の構築・フィールド参照の E1022、opt-in）。`tests/test_runner.rs` に `failed_tests_show_the_end_of_their_stderr` を追加（9 passed）。`src/main.rs` に `parses_bench_coverage_and_seed_options`。`node scripts/check-docs.mjs`（gen.md・test.md・index.md・random.md・usage.md・option.md）成功。
