# F15 付録（Phase 3）: `--report parallel` のテスト一式

[F15 本体](F15-sequential-equivalent-data-parallel.md)の Phase 3 のテストの全文。**fixture のソースは `check` を通ることを確かめた**（`Monoids` を使う `rules` は Phase 1 を入れたコンパイラで）。
**期待値は実装前に仕様から手で導いた想定出力**（オフセットと行・桁は fixture から Node で数えた。`./Main.tz` の `./` は `tsuzuri check Main.tz` の診断の表示と同じ）。実装して初めて確かめられるので、
実装者は最初に `parallel_report::build` を書く前に、このファイルの 3 つの fixture と `tests/parallel_report.rs` を写して赤を確かめる（手順 8）。期待値が実出力と違うとき、**まず実装の誤りを疑い**、仕様 §7 を読み直してから、
それでも仕様どおりの実出力が違うなら止めて報告する（実装に合わせて期待値を書き換えない）。

## L1. `tests/fixtures/parallel_report/basic/Main.tz`（新規。全文。484 バイト）

<!-- file: tests/fixtures/parallel_report/basic/Main.tz -->
```text
def square :: i64 -> i64
fn square value = value * value

def pure_total :: ref [i64] -> i64
fn pure_total values =
    let squares = Array.map square values
    Array.sum (ref squares)

def traced :: ref [i64] -> [i64]
fn traced values = Array.map (\value -> Debug.trace value) values

def doubled :: ref [i64] -> [i64]
fn doubled values = Parallel.map (\value -> value * 2) values

def shown :: ref [i64] -> [i64]
fn shown values = Parallel.map (\value -> Debug.trace value) values
```

## L2. `tests/fixtures/parallel_report/rules/Main.tz`（新規。全文。1,064 バイト）

提案する 4 件（2・14・17・20 行目）と、しない 4 件（浮動小数点の `Array.sum`＝5 行目、長さ 3 の配列リテラル＝8 行目、借用を捕捉するラムダ＝11 行目、関数値の引数＝23 行目）と、`site` の 3 件（26・29・32 行目）。

<!-- file: tests/fixtures/parallel_report/rules/Main.tz -->
```text
def total_ints :: ref [i64] -> i64
fn total_ints values = Array.sum values

def total_floats :: ref [f64] -> f64
fn total_floats values = Array.sum values

def small_literal :: unit -> i64
fn small_literal _unit = Array.sum (ref [1, 2, 3])

def lookup :: ref [i64] -> ref [i64] -> [i64]
fn lookup table indexes = Array.map (\index -> table[index]) indexes

def shifted :: i64 -> ref [i64] -> [i64]
fn shifted offset values = Array.map (\value -> value + offset) values

def lengths :: ref [string] -> [i64]
fn lengths texts = Array.map_ref String.length texts

def sums :: ref [Monoids.Sum<i64>] -> Monoids.Sum<i64>
fn sums values = Array.fold_monoid values

def apply :: (i64 -> i64) -> ref [i64] -> [i64]
fn apply transform values = Array.map transform values

def chunked :: ref [i64] -> i64
fn chunked values = Parallel.sum values

def folded :: ref [Monoids.Sum<i64>] -> Monoids.Sum<i64>
fn folded values = Parallel.fold values

def filled :: i64 -> [i64]
fn filled count = Parallel.init count (\index -> index * 2)

def main :: unit -> i32
fn main _unit = 0
```

## L3. `tests/fixtures/parallel_report/error/Main.tz`（新規。全文）

```text
let value: i64 = true
value
```

## L4. `tests/parallel_report.rs`（新規。全文）

```rust
//! `tsuzuri check --report parallel` (F15 Phase 3). The fixtures are in tests/fixtures/parallel_report/.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parallel_report")
        .join(name)
}

fn run(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap()
}

fn report(directory: &Path, json: bool) -> String {
    let mut arguments = vec!["check", "Main.tz", "--report", "parallel"];
    if json {
        arguments.push("--json");
    }
    let output = run(directory, &arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

const BASIC_TEXT: &str = "parallel report: 2 suggestions, 2 sites
./Main.tz:6:19: suggestion: Array.map can be Parallel.map: the callback is pure
  fix (maybe): replace 'Array.map' with 'Parallel.map'
./Main.tz:7:5: suggestion: Array.sum can be Parallel.sum: integer sums are the same in any order
  fix (maybe): replace 'Array.sum' with 'Parallel.sum'
./Main.tz:13:21: site: Parallel.map callback is pure (parallel_safe: true)
./Main.tz:16:19: site: Parallel.map callback is not pure (parallel_safe: false): the callback reaches 'Debug.__print_string' (effect: debug)
";

const BASIC_JSON: &str = r#"{"schema":"tz-report-parallel/1","suggestions":2,"sites":2,"truncated":0}
{"kind":"suggestion","call":"Array.map","replacement":"Parallel.map","parallel_safe":true,"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"fixes":[{"title":"replace 'Array.map' with 'Parallel.map'","applicability":"maybe","edits":[{"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"text":"Parallel.map"}]}]}
{"kind":"suggestion","call":"Array.sum","replacement":"Parallel.sum","parallel_safe":true,"path":"./Main.tz","span":{"start":162,"end":171,"line":7,"column":5,"end_line":7,"end_column":14},"fixes":[{"title":"replace 'Array.sum' with 'Parallel.sum'","applicability":"maybe","edits":[{"path":"./Main.tz","span":{"start":162,"end":171,"line":7,"column":5,"end_line":7,"end_column":14},"text":"Parallel.sum"}]}]}
{"kind":"site","call":"Parallel.map","parallel_safe":true,"path":"./Main.tz","span":{"start":341,"end":382,"line":13,"column":21,"end_line":13,"end_column":62}}
{"kind":"site","call":"Parallel.map","parallel_safe":false,"reason":"the callback reaches 'Debug.__print_string' (effect: debug)","path":"./Main.tz","span":{"start":434,"end":483,"line":16,"column":19,"end_line":16,"end_column":68}}
"#;

const RULES_TEXT: &str = "parallel report: 4 suggestions, 3 sites
./Main.tz:2:24: suggestion: Array.sum can be Parallel.sum: integer sums are the same in any order
  fix (maybe): replace 'Array.sum' with 'Parallel.sum'
./Main.tz:14:28: suggestion: Array.map can be Parallel.map: the callback is pure
  fix (maybe): replace 'Array.map' with 'Parallel.map'
./Main.tz:17:20: suggestion: Array.map_ref can be Parallel.map_ref: the callback is pure
  fix (maybe): replace 'Array.map_ref' with 'Parallel.map_ref'
./Main.tz:20:18: suggestion: Array.fold_monoid can be Parallel.fold: the result is the same by the laws of Monoid
  fix (maybe): replace 'Array.fold_monoid' with 'Parallel.fold'
./Main.tz:26:21: site: Parallel.sum callback is pure (parallel_safe: true)
./Main.tz:29:20: site: Parallel.fold callback is pure (parallel_safe: true)
./Main.tz:32:19: site: Parallel.init callback is pure (parallel_safe: true)
";

#[test]
fn the_basic_report_in_text_and_json() {
    assert_eq!(report(&fixture("basic"), false), BASIC_TEXT);
    assert_eq!(report(&fixture("basic"), true), BASIC_JSON);
}

#[test]
fn only_the_replaceable_calls_are_suggested() {
    assert_eq!(report(&fixture("rules"), false), RULES_TEXT);
}

#[test]
fn the_report_is_deterministic() {
    for name in ["basic", "rules"] {
        for json in [false, true] {
            assert_eq!(
                report(&fixture(name), json),
                report(&fixture(name), json),
                "{name} {json}"
            );
        }
    }
}

#[test]
fn without_the_flag_the_output_is_unchanged() {
    for name in ["basic", "rules"] {
        let output = run(&fixture(name), &["check", "Main.tz"]);
        assert!(output.status.success(), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        assert!(output.stderr.is_empty(), "{name}");
    }
}

#[test]
fn an_erroneous_program_prints_no_report() {
    let output = run(
        &fixture("error"),
        &["check", "Main.tz", "--report", "parallel"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn the_flag_is_checked() {
    let directory = fixture("basic");
    for (arguments, message) in [
        (
            vec!["build", "Main.tz", "--report", "parallel"],
            "--report is only valid with check",
        ),
        (
            vec!["check", "Main.tz", "--report"],
            "--report requires a report name; supported: parallel",
        ),
        (
            vec!["check", "Main.tz", "--report", "copies"],
            "unknown report 'copies' for --report; supported: parallel",
        ),
        (
            vec!["check", "Main.tz", "--report", "parallel", "--report", "parallel"],
            "report parallel specified more than once",
        ),
    ] {
        let output = run(&directory, &arguments);
        assert!(!output.status.success(), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{arguments:?}\n{stderr}");
    }
}

#[test]
fn the_suggested_edits_compile_and_leave_no_suggestions() {
    let mut fixed = fs::read_to_string(fixture("rules").join("Main.tz")).unwrap();
    let mut edits = Vec::new();
    for line in report(&fixture("rules"), true).lines().skip(1) {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        if value["kind"] == "suggestion" {
            let edit = &value["fixes"][0]["edits"][0];
            edits.push((
                edit["span"]["start"].as_u64().unwrap() as usize,
                edit["span"]["end"].as_u64().unwrap() as usize,
                edit["text"].as_str().unwrap().to_string(),
            ));
        }
    }
    assert_eq!(edits.len(), 4);
    edits.sort_by(|left, right| right.0.cmp(&left.0));
    for (start, end, text) in edits {
        fixed.replace_range(start..end, &text);
    }
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parallel_report_fixed");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("Main.tz"), &fixed).unwrap();
    let json = report(&directory, true);
    assert_eq!(
        json.lines().next().unwrap(),
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":7,"truncated":0}"#
    );
}

#[test]
fn the_entries_stop_at_one_thousand() {
    let mut source = String::new();
    for index in 0..1001 {
        source.push_str(&format!(
            "def f{index} :: i64 -> [i64]\nfn f{index} count = Parallel.init count (\\item -> item)\n\n"
        ));
    }
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parallel_report_limit");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("Main.tz"), &source).unwrap();
    let json = report(&directory, true);
    assert_eq!(
        json.lines().next().unwrap(),
        r#"{"schema":"tz-report-parallel/1","suggestions":0,"sites":1000,"truncated":1}"#
    );
    assert_eq!(json.lines().count(), 1001);
}
```

## L5. `src/main.rs` の単体テスト（`parses_warn_implicit_copy` の直後）

```rust
    #[test]
    fn parses_report_parallel() {
        let arguments = parse(&["check", "Main.tz", "--report", "parallel"]).unwrap();
        assert!(arguments.report_parallel);
        assert!(!parse(&["check", "Main.tz"]).unwrap().report_parallel);
        for (values, message) in [
            (
                vec!["check", "Main.tz", "--report"],
                "--report requires a report name; supported: parallel",
            ),
            (
                vec!["check", "Main.tz", "--report", "copies"],
                "unknown report 'copies' for --report; supported: parallel",
            ),
            (
                vec!["check", "Main.tz", "--report", "parallel", "--report", "parallel"],
                "report parallel specified more than once",
            ),
            (
                vec!["build", "Main.tz", "--report", "parallel"],
                "--report is only valid with check",
            ),
        ] {
            assert_eq!(parse(&values).unwrap_err(), message, "{values:?}");
        }
    }
```

## L6. 実装者が見直す点（想定出力が実出力と違いうる場所）

1. `Parallel.sum values`（26 行目）の `site` の `span` は呼び出し式の全体（`Parallel.sum values`）。`Call` の `TypedExpr.span` が呼び出しの先頭から最後の引数の終わりまでであること（`src/parser.rs` の `left.span.through(...)`）は確かめた。`Parallel(..)` の式の `span` も同じ `expression.span`（`call_kind` を呼ぶ `finish_expression` の引数）。
2. `Parallel.sum`・`Parallel.fold` の `site` は、std の本体の `Parallel(ParallelReduce, ..)` の `args[1]`（`Closure(id, [])`）を根にする。`Closure` の `captures` が空であること、`reachable_effect` が `Monoid.combine` の instance へ届き `Ok(None)` になることを、最初に `dbg!` で確かめる。
3. `lookup`（11 行目）が提案されないのは、ラムダが `ref [i64]` の `table` を捕捉する（`Type::Reference` に届く）ため。`apply`（23 行目）が提案されないのは、`transform` が局所変数の関数値（手順 4 の 4 行目）のため。どちらも、Phase 3 が「提案しないほうへ倒す」規則。
4. 1,001 件のテストは、`site` の整列が `(path, start, end, kind)` の昇順で、先頭の 1,000 件を残すことを確かめる。関数の数が多いので、`MAX_NODES_PER_FUNCTION` ではなくエントリの上限だけが効く。

## L7. reason の表

本体の §7.2 の `reason`（`site` の `parallel_safe: false`）は次の 8 つの文のどれか（閉じた表。`{effect}` は D-53 の効果の名前（`io`・`os`・`net`・`task`・`atomic`・`rc`・`extern`・`debug`・`user_drop`・`gpu`）か、本チケットの報告だけの 4 語 `dyn`・`unresolved`・`unknown_call`・`analysis_limit`。後者は `effects` には入れない）。本体が「§7.2 の表」と書くのはこの表。

| 原因（A19 の `Effect` ほか） | `reason` | `{effect}` |
| --- | --- | --- |
| `Host(name)`（`extern`） | `the callback calls the extern function '{name}' (effect: extern)` | `extern` |
| `Builtin(name, _)`（`name` は `Builtin::name()`。例 `Debug.__print_string`） | `the callback reaches '{name}' (effect: {effect})` | `name` の `.` の前で決める: `Debug`→`debug`、`IO`・`Async`→`io`、`Os`・`Bench`・`Gen`→`os`、`Net`→`net`、`Task`・`Parallel`→`task`、`Atomic`・`Mutex`・`Channel`・`Arena`→`atomic`、`Rc`・`Arc`→`rc`、`Gpu`→`gpu`、`Dyn`→`dyn` |
| `Concurrency`（`TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel` の式） | `the callback starts a task or a parallel operation (effect: task)` | `task` |
| `Dyn` | `the callback uses dyn dispatch (effect: dyn)` | `dyn` |
| `UserDrop(ty)` | `the callback holds a value whose type '{ty}' has a user Drop (effect: user_drop)` | `user_drop` |
| `Unresolved` | `the callback cannot be proven pure (effect: unresolved)` | `unresolved` |
| 未知の関数値（設計 3 の手順 4 の 1 行目・4 行目に当たる場合） | `the callback is or captures a function value that is not known (effect: unknown_call)` | `unknown_call` |
| 解析の上限（`Err(LimitExceeded)`） | `the analysis limit was reached (effect: analysis_limit)` | `analysis_limit` |

## L8. `src/parallel_report.rs` の型と関数の宣言

本体の「設計 3」が指す宣言の全文（`src/copies.rs` を手本にする。`src/lib.rs` に `pub mod parallel_report;`）。

```rust
pub const MAX_ENTRIES: usize = 1000;
pub const SMALL_INPUT: u64 = 4096;
const MAX_NODES_PER_FUNCTION: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind { Suggestion, Site }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub call: &'static str,            // "Array.map" の形
    pub replacement: Option<&'static str>,
    pub function: usize,               // 呼び出しを含む関数の `CheckedModule::functions` の添字（Phase 4 が使う）
    pub span: Span,                    // suggestion は呼び出しの名前の範囲、site は呼び出し式の全体
    pub parallel_safe: bool,
    pub reason: Option<String>,        // site で parallel_safe == false のとき
    pub why: Option<&'static str>,     // suggestion のとき
}

pub struct Report { pub entries: Vec<Entry>, pub truncated: usize }

/// The source files that a report reads: the text for the suggestion condition and the path for the output.
pub trait Sources {
    fn text(&self, span: Span) -> &str;
    fn path(&self, span: Span) -> String;
}

/// 成功した `check` の `CheckedModule` から報告を作る（型の文脈は `module.types()`）。`sources` は提案のテキストの条件に使う。
pub fn build(module: &CheckedModule, sources: &dyn Sources) -> Report;
pub fn render_text(report: &Report, sources: &dyn Sources) -> String;
pub fn render_json(report: &Report, sources: &dyn Sources) -> String;
```
