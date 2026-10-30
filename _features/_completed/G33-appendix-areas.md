# G33 付録 C: 残り 10 領域（LEX・SYN・STR・MOD・TSK・DIA・CLI・ABI・STD・EDN）の棚卸しの手順と最初の 100 規則

本文: [G33-spec-rule-ids-conformance.md](G33-spec-rule-ids-conformance.md)。表の読み方（`LR/`・`⏎`・`(empty)`・`[名前]⏎`・ディレクティブの項目）と、行からファイルを作る手順は本文 §3 と [付録 A](G33-appendix.md)。すべての行を、`target/release/tsuzuri` で実機に当てて確かめた（付録 A.1）。

## C.0 共通の棚卸しの手順

領域を広げるときは本文 §8 の P1〜P6 に従う。候補の文の探し方は次のとおり。

1. 各領域の「読むページ」で、「この記事のポイント」の箇条書き・表の行・診断コード（`E…`・`W…`）を含む文・断定する文（「〜は…です」「〜できません」）を拾う。
2. 1 文 1 規則にする。英語で、`MUST`・`MUST NOT`・`MAY` のどれかを使い、診断コードは `(E1012)` のように括弧に入れる。主張が 2 つある文は 2 規則に分ける。
3. 最小のプログラム（1〜10 行）で実機に当てる。診断は `check --json`、値は `run -O0` と `-O3`、数値は `test` で 4 組（native/WASM × `-O0`/`-O3`）、CLI は実際のコマンド。
4. 文と実機の結果が一致したものだけを表に足す。食い違ったら足さずに報告する（本文の停止条件 2）。
5. 各領域の Phase 3 の完了条件は、この付録の 10 件が通ること。それ以降の追加に終わりの条件はない。

## C.1 LEX（字句。10 件）

**読むページ**: `LR/literals-and-strings/literals.md`（整数・小数・文字・エスケープ・桁区切り・接尾辞）、`LR/values-and-functions/tokens.md`（記号・コメント）、`LR/values-and-functions/keywords.md`（予約語と文脈キーワード）、`LR/compiler/diagnostics.md`（`E0001`）。**探す文**: リテラルの形と値、コメントの入れ子、予約語の表の各語（その語を識別子に使うと `E0002`）、`E0001` の条件（未終端・不正なエスケープ・不正な桁区切り）。**確かめ方**: 本体は 1〜3 行。誤りは `check-error E0001 line=N`、値は `stdout`。1 ファイルに字句の誤りを 2 つ入れない（`E0001` があると構文解析に進まない。DIA-005）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| LEX-001 | A block comment MUST be allowed to nest. | LR/values-and-functions/tokens.md | `/* outer /* inner */ still outer */⏎1 + 2` → `expect: stdout "3\n"` |
| LEX-002 | A leading zero MUST NOT select octal: `0123` is the decimal value 123. | LR/literals-and-strings/literals.md | `0123` → `expect: stdout "123\n"` |
| LEX-003 | A digit separator `_` MUST be placed between digits only (E0001 otherwise). | LR/literals-and-strings/literals.md | `let x = 1_⏎x` → `expect: check-error E0001 line=1` |
| LEX-004 | A line comment `//` MUST run to the end of the line. | docs/language.md §ソースと宣言 | `let x = 1 // + 2⏎x` → `expect: stdout "1\n"` |
| LEX-005 | A reserved word MUST NOT be used as an identifier. | LR/values-and-functions/keywords.md | `let for = 1⏎for` → `expect: check-error E0002 line=1` |
| LEX-006 | A contextual keyword such as `of` MUST remain usable as an identifier outside its syntax. | LR/values-and-functions/keywords.md | `let of = 3⏎of + 1` → `expect: stdout "4\n"` |
| LEX-007 | `not` MUST be an ordinary function value, not a reserved word. | LR/values-and-functions/keywords.md | `let f = not⏎f true` → `expect: stdout "false\n"` |
| LEX-008 | An unterminated string literal MUST be rejected (E0001). | LR/compiler/diagnostics.md | `let s = "abc⏎s` → `expect: check-error E0001 line=1` |
| LEX-009 | An unknown escape sequence in a string literal MUST be rejected (E0001). | LR/literals-and-strings/literals.md §注意点 | `let s = "a\qb"⏎s` → `expect: check-error E0001 line=1` |
| LEX-010 | A lone `}` outside an interpolation hole MUST be rejected (E0001). | LR/literals-and-strings/interpolated-strings.md | `$"a}"` → `expect: check-error E0001 line=1` |

## C.2 SYN（構文。10 件）

**読むページ**: `LR/values-and-functions/statements.md`・`functions.md`・`lambda-expressions.md`、`docs/language.md §アプリケーションのエントリーポイント`（`main` の署名・文の区切り・行の継続）、`LR/compiler/diagnostics.md`（`E0002`・`E2004`）。**探す文**: 文の区切り（改行と `;`）、ブロックの 2 つの形、行の継続（行頭の `|>`・`(`）、入れ子の上限、`main` の署名、旧構文の拒否。**確かめ方**: 構文の誤りは `check-error E0002 line=N`。入れ子の上限のように本体が長いものは、本体を 1 行に書く。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| SYN-001 | Top-level statements MUST be separable by a newline or by `;`. | docs/language.md §アプリケーションのエントリーポイント | `let a = 1; let b = 2⏎a + b` → `expect: stdout "3\n"` |
| SYN-002 | Statements inside a brace block MUST be separated by `;`. | docs/language.md §アプリケーションのエントリーポイント | `def f :: unit -> i64 = \() -> { let a = 1 a }⏎0` → `expect: check-error E0002 line=1` |
| SYN-003 | A brace block and an indented block MUST be equivalent. | LR/values-and-functions/statements.md | `let a = if true then { let x = 1; x + 1 } else 0⏎let b =⏎    if true then⏎        let x = 1⏎        x + 1⏎    else⏎        0⏎$"{a} {b}"` → `expect: stdout "2 2\n"` |
| SYN-004 | A line that starts with `\|>` MUST continue the previous line. | docs/language.md §アプリケーションのエントリーポイント | `def inc :: i64 -> i64 = \x -> x + 1⏎let r = 40⏎    \|> inc⏎    \|> inc⏎r` → `expect: stdout "42\n"` |
| SYN-005 | A line that starts with `(` MUST start a new expression, not continue the previous line. | docs/language.md §アプリケーションのエントリーポイント | `let x = 40⏎(x + 2)` → `expect: stdout "42\n"` |
| SYN-006 | The operand of `deref` MUST be a single term; `deref f x` MUST be rejected (E0002). | docs/language.md §呼び出し引数の暗黙の借用 | `def id :: ref i64 -> ref i64 = \x -> x⏎let n = 5⏎let v = deref id ref n⏎v` → `expect: check-error E0002 line=3` |
| SYN-007 | Expression nesting deeper than 128 levels MUST be rejected (E0002). | LR/values-and-functions/statements.md | `((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))` → `expect: check-error E0002 line=1` |
| SYN-008 | `fn main` without a preceding `def` signature MUST be rejected (E0002). | docs/language.md §アプリケーションのエントリーポイント | `fn main () = 0` → `expect: check-error E0002 line=1` |
| SYN-009 | A `main` with a signature other than `unit -> i32` or `[string] -> i32` MUST be rejected (E2004). | docs/language.md §アプリケーションのエントリーポイント | `def main :: unit -> i64 = \() -> 0` → `expect: check-error E2004 line=1` |
| SYN-010 | An old-style generic record `Pair 'a 'b` MUST be rejected (E0002); type arguments use `<...>`. | LR/types-and-type-inference/generics.md | `record Pair 'a 'b { first: 'a, second: 'b }⏎0` → `expect: check-error E0002 line=1` |

## C.3 STR（文字列。10 件）

**読むページ**: `LR/literals-and-strings/strings.md`・`interpolated-strings.md`、`LR/built-in-types-and-modules/string.md`・`utf8string.md`・`char.md`・`utf8char.md`、`docs/language.md §string と utf8string`。**探す文**: 長さの単位（`string` は UTF-16 コード単位、`utf8string` は UTF-8 バイト）、連結・比較・添字、補間（書式を含む）、エスケープ、`char`・`utf8char` の変換。**確かめ方**: `stdout`。非 ASCII の期待は、ディレクティブの JSON 文字列に直接書く（UTF-8）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| STR-001 | The length of a `string` MUST count UTF-16 code units. | LR/literals-and-strings/strings.md | `let text = "😀"⏎text.length` → `expect: stdout "2\n"` |
| STR-002 | The length of a `utf8string` MUST count UTF-8 bytes. | LR/literals-and-strings/strings.md | `let text = u8"é"⏎text.length` → `expect: stdout "2\n"` |
| STR-003 | There MUST be no implicit conversion between `string` and `utf8string`. | LR/literals-and-strings/strings.md | `let s: string = u8"a"⏎s` → `expect: check-error E1003 line=1` |
| STR-004 | `+` on two strings MUST concatenate them in order. | docs/language.md §string と utf8string | `let a = "ab"⏎let b = "cd"⏎a + b` → `expect: stdout "abcd\n"` |
| STR-005 | String equality MUST compare contents, not identity. | docs/language.md §string と utf8string | `let a = "a" + "b"⏎let b = "ab"⏎a == b` → `expect: stdout "true\n"` |
| STR-006 | Strings MUST compare lexicographically with `<`. | docs/language.md §string と utf8string | `$"{"apple" < "banana"} {"b" < "a"}"` → `expect: stdout "true false\n"` |
| STR-007 | An interpolation hole MUST borrow its value, so the value stays usable afterwards. | LR/literals-and-strings/interpolated-strings.md | `let s = "x"⏎let t = $"{s}{s}"⏎t + to_string s.length` → `expect: stdout "xx1\n"` |
| STR-008 | `{{` and `}}` MUST denote literal braces in an interpolated string. | LR/literals-and-strings/interpolated-strings.md | `$"{{x}}"` → `expect: stdout "{x}\n"` |
| STR-009 | An out-of-range string index MUST trap. | docs/language.md §string と utf8string | `let s = "ab"⏎let i = 5⏎s[i] as i64` → `expect: trap kind=bounds` |
| STR-010 | An escape such as `\n` MUST denote one character. | docs/language.md §string と utf8string | `let s = "a\tb\n"⏎s.length` → `expect: stdout "4\n"` |

## C.4 MOD（モジュール。10 件。9 件がディレクトリ形）

**読むページ**: `LR/organizing-tsuzuri/modules.md`・`namespaces.md`・`using-declarations.md`・`access-control.md`・`packages.md`。**探す文**: モジュール名とファイル名の対応、`Module.member`、`namespace` と `using`、`private`、サブディレクトリと `::`、`.tt` の規則。**確かめ方**: 複数ファイルのディレクトリ形。診断が補助ファイルに出るときは `file=` を付ける（MOD-008）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| MOD-001 | A module file name MUST start with an uppercase letter (E1011). | LR/organizing-tsuzuri/modules.md | `[Main.tz]⏎0⏎[point.tz]⏎def one :: i64 = 1` → `expect: check-error E1011` |
| MOD-002 | A public member MUST be reachable as `Module.member` from another module. | LR/organizing-tsuzuri/modules.md | `[Main.tz]⏎Geometry.area 3 4⏎[Geometry.tz]⏎def area :: i64 -> i64 -> i64 = \w h -> w * h` → `expect: stdout "12\n"` |
| MOD-003 | A `private` member MUST NOT be reachable from another module (E1022). | LR/organizing-tsuzuri/access-control.md | `[Main.tz]⏎Geometry.secret 3⏎[Geometry.tz]⏎private def secret :: i64 -> i64 = \x -> x` → `expect: check-error E1022 line=1` |
| MOD-004 | A public function MUST NOT expose a private type of its module (E1022). | LR/organizing-tsuzuri/access-control.md | `[Main.tz]⏎0⏎[Shapes.tz]⏎private record Hidden { x: i64 }⏎⏎def make :: unit -> Hidden = \() -> Hidden { x: 1 }` → `expect: check-error E1022` |
| MOD-005 | A subdirectory MUST extend the module name with `::` when no namespace is declared. | LR/organizing-tsuzuri/namespaces.md | `[Main.tz]⏎Geometry::Point.area 3 4⏎[Geometry/Point.tz]⏎def area :: i64 -> i64 -> i64 = \w h -> w * h` → `expect: stdout "12\n"` |
| MOD-006 | `using` MUST allow a module of the named namespace to be referenced without the namespace. | LR/organizing-tsuzuri/using-declarations.md | `[Main.tz]⏎namespace App⏎using Lib⏎⏎Point.area 3 4⏎[Lib/Point.tz]⏎namespace Lib⏎⏎def area :: i64 -> i64 -> i64 = \w h -> w * h` → `expect: stdout "12\n"` |
| MOD-007 | `using` of an unknown namespace MUST be rejected (E1011). | LR/organizing-tsuzuri/using-declarations.md | `namespace App⏎using Nowhere⏎⏎0` → `expect: check-error E1011 line=2` |
| MOD-008 | A top-level expression MUST be accepted only in `Main.tz` (E2004 elsewhere). | LR/values-and-functions/values.md | `[Main.tz]⏎0⏎[Lib.tz]⏎def one :: i64 = 1⏎1 + 1` → `expect: check-error E2004 line=2 file=Lib.tz` |
| MOD-009 | A `def` in a `.tt` file MUST be rejected unless it belongs to a class (E1018). | LR/organizing-tsuzuri/modules.md | `[Main.tz]⏎0⏎[Traits.tt]⏎def one :: i64 = 1` → `expect: check-error E1018` |
| MOD-010 | A module name that is also a record name in the same file MUST be referenced by the module name alone. | LR/organizing-tsuzuri/modules.md | `[Main.tz]⏎let p = Point { x: 3 }⏎Point.double p⏎[Point.tz]⏎record Point { x: i64 }⏎⏎def double :: Point -> i64 = \p -> p.x * 2` → `expect: stdout "6\n"` |

## C.5 TSK（タスク。10 件）

**読むページ**: `LR/async-tasks-and-lazy/task.md`・`async.md`・`lazy.md`、`docs/language.md §タスク`・`§非同期計算（Async）`。**探す文**: `task { … }`・`Task.run`・`Task.parallel`・`Task.scope`、`Atomic`、捕捉と送信（`E1012`・`E1013`）、結果の順序。**注意**: 並行の結果は、実行のたびに変わらない性質（合計・順序）だけを規則にする。実行順に依存する出力は載せない。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| TSK-001 | `Task.run` MUST run a task to completion and return its result. | LR/async-tasks-and-lazy/task.md | `def next :: i64 -> Task<i64> = \value -> task { return value + 1 }⏎⏎let work = task {⏎    let! first = next 19⏎    let! second = next 21⏎    return first + second⏎}⏎Task.run work` → `expect: stdout "42\n"` |
| TSK-002 | A `Task` MUST NOT be Copy, so running it twice MUST be rejected (E1012). | LR/async-tasks-and-lazy/task.md | `let work = task { return 1 }⏎let a = Task.run work⏎let b = Task.run work⏎a + b` → `expect: check-error E1012 line=3` |
| TSK-003 | A reference MUST NOT be sent into a task from outside (E1013). | LR/async-tasks-and-lazy/task.md | `let text = "coffee"⏎let r = ref text⏎let work = task { return String.length r }⏎Task.run work` → `expect: check-error E1013 line=3` |
| TSK-004 | `Task.parallel` MUST return its results in the order of the input array. | LR/async-tasks-and-lazy/task.md | `let jobs = new [Task<i64>](4, \index -> task { return index * index })⏎let results = Task.run (Task.parallel jobs)⏎$"{results}"` → `expect: stdout "[0, 1, 4, 9]\n"` |
| TSK-005 | A task that is dropped without running MUST NOT execute its body. | LR/async-tasks-and-lazy/task.md | `let _unused = task {⏎    assert false⏎    0⏎}⏎42` → `expect: stdout "42\n"` |
| TSK-006 | An owned value MAY be moved into a task. | LR/async-tasks-and-lazy/task.md | `let text = "coffee"⏎let work = task { return String.length text }⏎Task.run work` → `expect: stdout "6\n"` |
| TSK-007 | An empty `Task.parallel` MUST yield an empty result array. | LR/async-tasks-and-lazy/task.md | `let jobs: [Task<i64>] = []⏎Array.length ref (Task.run (Task.parallel jobs))` → `expect: stdout "0\n"` |
| TSK-008 | Tasks sharing an `Atomic` through `Task.scope` MUST produce the same total on every run. | LR/async-tasks-and-lazy/task.md | `let hits = Atomic.create 0i64⏎let _seen = Task.scope (ref hits) 8 (shared -> index -> Atomic.fetch_add shared (index + 1))⏎Atomic.load (ref hits)` → `expect: stdout "36\n"` |
| TSK-009 | A `Task.parallel_results` that contains an `Error` MUST return the `Error` of the smallest input index. | LR/async-tasks-and-lazy/task.md | `def job :: i64 -> Task<Result<i64, string>> = \value -> task {⏎    if value >= 2 { return Result.Error ("stopped " + to_string value) }⏎    else { return Result.Ok (value * 2) }⏎}⏎⏎let jobs = new [Task<Result<i64, string>>](4, \index -> job index)⏎match Task.run (Task.parallel_results jobs) with⏎\| Ok _ -> "ok"⏎\| Error e -> e` → `expect: stdout "stopped 2\n"` |
| TSK-010 | A `Task<T>` value MUST NOT be used where a `T` is expected (E1003). | LR/async-tasks-and-lazy/task.md | `let work = task { return 1 }⏎let n: i64 = work⏎n` → `expect: check-error E1003 line=2` |

## C.6 DIA（診断。10 件。8 件が cli モード）

**読むページ**: `LR/compiler/diagnostics.md`（表示形式・JSON・複数エラー・エラーコードの表・警告）。**探す文**: 診断の形式（text と JSON の全キー）、順序と上限（画面 50 件・収集 1000 件）、段階（字句 → 構文 → 型 → 所有権）、警告の扱い。**棚卸しの量**: エラーコードの表は 47 種（`grep -o "E[0-9]\{4\}\|W[0-9]\{4\}" _tsuzuri/language-reference/compiler/diagnostics.md | sort -u | wc -l` が 47。`src/` の `"E…"` リテラルの種類も 47）。コード 1 つにつき、そのコードを出す最小のプログラムを `check-error CODE` で 1 つ足していく。**注意**: バイト位置・列を含む期待は、本体だけで値が決まる（ディレクティブを末尾に置く理由）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| DIA-001 | `check --json` MUST write one JSON object per diagnostic on stderr, with the keys `severity`, `code`, `message`, `path` and `span` in that order. | LR/compiler/diagnostics.md | `let a = "hello"⏎let b = a⏎let c = a⏎c` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "{\"severity\":\"error\",\"code\":\"E1012\",\"message\":\"use of moved or partially moved value 'a'\",\"path\":\"" ⏎ expect: stderr-has "\",\"span\":{\"start\":34,\"end\":35,\"line\":3,\"column\":9,\"end_line\":3,\"end_column\":10}}" ⏎ expect: stdout-empty ⏎ expect: stderr-lines 1` |
| DIA-002 | A span MUST carry byte offsets `start` and `end` and 1-based `line`, `column`, `end_line` and `end_column`. | LR/compiler/diagnostics.md | `let x: i64 = "s"⏎x` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "\"span\":{\"start\":13,\"end\":16,\"line\":1,\"column\":14,\"end_line\":1,\"end_column\":17}"` |
| DIA-003 | A warning MUST NOT make `check` fail: the exit code MUST be 0. | LR/compiler/diagnostics.md | `def f :: unit -> i64 = \() -> {⏎    let unused = 1;⏎    2⏎}⏎f ()` → `cli: check $DIR --json ⏎ expect: exit 0 ⏎ expect: stderr-has "\"severity\":\"warning\",\"code\":\"W1001\""` |
| DIA-004 | An unused local MUST be reported as warning W1001 whose message names the fix of prefixing the name with `_`. | LR/compiler/diagnostics.md | `def f :: unit -> i64 = \() -> {⏎    let unused = 1;⏎    2⏎}⏎f ()` → `expect: check-warning W1001 line=2` |
| DIA-005 | A lexical error MUST stop the compiler before parsing, so no syntax error is reported in the same run. | LR/compiler/diagnostics.md | `let a = "x⏎let = = 1` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "\"code\":\"E0001\"" ⏎ expect: stderr-lacks "\"code\":\"E0002\"" ⏎ expect: stderr-lines 1` |
| DIA-006 | A syntax error MUST stop the compiler before type checking, so no type error is reported in the same run. | LR/compiler/diagnostics.md | `let = = 1⏎let x: i64 = "s"` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "\"code\":\"E0002\"" ⏎ expect: stderr-lacks "\"code\":\"E1003\"" ⏎ expect: stderr-lines 1` |
| DIA-007 | The human format MUST be `path:line:column: severity[code]: message` followed by the source line and a caret line. | LR/compiler/diagnostics.md | `let x: i64 = "s"⏎x` → `cli: check $DIR ⏎ expect: exit 1 ⏎ expect: stderr-has "Main.tz:1:14: error[E1003]: " ⏎ expect: stderr-has "\n  1 \| let x: i64 = \"s\"\n    \|              ^"` |
| DIA-008 | A successful `check` MUST print nothing. | LR/compiler/usage.md | `1` → `cli: check $DIR ⏎ expect: exit 0 ⏎ expect: stdout-empty ⏎ expect: stderr-empty` |
| DIA-009 | A diagnostic about a name MUST point at that name, not at the start of the statement. | docs/language.md §診断 | `let a = "hello"⏎let b = a⏎let c = a⏎c` → `expect: check-error E1012 line=3` |
| DIA-010 | A warning in `--json` output MUST use the severity `warning`. | LR/compiler/diagnostics.md | `def f :: unit -> i64 = \() -> {⏎    let unused = 1;⏎    2⏎}⏎f ()` → `cli: check $DIR --json ⏎ expect: exit 0 ⏎ expect: stderr-has "{\"severity\":\"warning\",\"code\":\"W1001\",\"message\":\"unused local 'unused'" ⏎ expect: stderr-lines 1` |

## C.7 CLI（コマンド。10 件。cli モード 10 件（`deterministic` 1 件を含む））

**読むページ**: `LR/compiler/usage.md`・`option.md`、`target/release/tsuzuri --help`（使い方の行。サブコマンドは `lsp`・`check`・`doc`・`fmt`・`test`・`bench`・`build`・`run`・`repl`・`script`・`new`・`fetch`・`publish`・`api-diff`・`bindgen`・`toolchain info`）。**探す文**: サブコマンドごとの終了コード・出力の形・オプションの拒否（`E2000`）。**確かめ方**: cli モード。作業ディレクトリ `$DIR` の下だけに書く。**注意**: 出力の文言は互換性の対象外（`docs/stability.md`）。照合するのは、安定として保つもの（診断のコード・JSON のキー・終了コード・`api-diff` の出力の形）と、文書が文言まで約束しているものだけ。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| CLI-001 | An unknown option MUST be rejected with E2000 and exit code 2. | LR/compiler/option.md | `0` → `cli: check $DIR --bogus ⏎ expect: exit 2 ⏎ expect: stderr-has "error[E2000]: unknown option '--bogus'"` |
| CLI-002 | `run` MUST print the value of the final expression of `Main.tz` and exit with 0. | docs/language.md §アプリケーションのエントリーポイント | `let x = 40⏎x + 2` → `cli: run $DIR ⏎ expect: exit 0 ⏎ expect: stdout "42\n"` |
| CLI-003 | A program that exits with a non-zero code MUST make `run` fail with E2005 and exit code 1. | LR/compiler/usage.md | `def main :: unit -> i32 = \() -> 7` → `cli: run $DIR ⏎ expect: exit 1 ⏎ expect: stderr-has "error[E2005]: program exited with code 7"` |
| CLI-004 | `fmt --check` MUST exit with 1 and name the file when a source is not formatted. | LR/compiler/usage.md | `let   x=1⏎x` → `cli: fmt --check $DIR ⏎ expect: exit 1 ⏎ expect: stderr-has "Main.tz: not formatted"` |
| CLI-005 | `fmt --check` MUST exit with 0 and print nothing when every source is formatted. | LR/compiler/usage.md | `let x = 1⏎x` → `cli: fmt --check $DIR ⏎ expect: exit 0 ⏎ expect: stdout-empty ⏎ expect: stderr-empty` |
| CLI-006 | `test --list` MUST print `index module name` lines for each test and MUST NOT run them. | LR/built-in-types-and-modules/test.md | `test "a" = assert false⏎test "b" = ()` → `cli: test $DIR --list ⏎ expect: exit 0 ⏎ expect: stdout "0 Main.a\n1 Main.b\n"` |
| CLI-007 | `test --filter` with no match MUST exit with 0 and report every test as ignored. | LR/built-in-types-and-modules/test.md | `test "a" = ()⏎test "b" = ()` → `cli: test $DIR --filter zzz ⏎ expect: exit 0 ⏎ expect: stdout-has "0 passed; 0 failed; 2 ignored"` |
| CLI-008 | `new` MUST create `Tsuzuri.toml`, `Main.tz` and `.gitignore`. | LR/compiler/usage.md | `(empty)` → `cli: new $DIR/app ⏎ expect: exit 0 ⏎ expect: stdout-has "Tsuzuri.toml" ⏎ expect: stdout-has "Main.tz" ⏎ expect: stdout-has ".gitignore" ⏎ expect: file-has app/Tsuzuri.toml "name = \"app\"" ⏎ expect: file-has app/Main.tz "" ⏎ expect: file-has app/.gitignore ""` |
| CLI-009 | `build --emit llvm` MUST produce byte-identical IR for the same input. | docs/architecture.md §不変条件 | `def add :: i64 -> i64 -> i64 = \a b -> a + b⏎export def total :: i64 -> i64 = \n -> add n 1⏎0` → `cli: build $DIR --emit llvm -o $OUT ⏎ expect: deterministic` |
| CLI-010 | `api-diff` MUST exit with 1 when a public declaration was removed. | LR/compiler/usage.md | `[old/Lib.tz]⏎def one :: i64 = 1⏎def two :: i64 = 2⏎[new/Lib.tz]⏎def one :: i64 = 1` → `cli: api-diff $DIR/old $DIR/new ⏎ expect: exit 1 ⏎ expect: stdout-has "two"` |

## C.8 ABI（公開 ABI。10 件。8 件が cli モード）

**読むページ**: `docs/language.md §公開 ABI`（型の対応表）、`LR/compiler/native-interop.md`、`LR/compiler/webassembly.md`。**探す文**: 表の各行（Tsuzuri の型 → C の型）、`export def` の名前 `tz_<name>`、拒否される引数（`E1008`）、ヘッダーの枠。**確かめ方**: `build $DIR --emit header -o $DIR/out.h` と `file-has`・`file-lacks`。WASM と JavaScript のグルーは対象外（本文の対象外）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| ABI-001 | An exported function MUST be declared in the header as `tz_<name>` with C types. | docs/language.md §公開 ABI | `export def add :: i64 -> i64 -> i64 = \a b -> a + b⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "int64_t tz_add(int64_t arg0, int64_t arg1);"` |
| ABI-002 | A function that is not exported MUST NOT appear in the header. | docs/language.md §公開 ABI | `export def add :: i64 -> i64 -> i64 = \a b -> a + b⏎def hidden :: i64 -> i64 = \a -> a⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-lacks out.h "hidden"` |
| ABI-003 | A `unit` result MUST map to `void`. | LR/types-and-type-inference/unit-type.md | `export def touch :: i64 -> unit = \_a -> ()⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "void tz_touch(int64_t arg0);"` |
| ABI-004 | An exported function MUST NOT take a record parameter (E1008). | docs/language.md §公開 ABI | `record R { a: i64, s: string }⏎export def f :: R -> i64 = \r -> r.a⏎0` → `expect: check-error E1008 line=2` |
| ABI-005 | An exported function MUST NOT take an owned `string` parameter (E1008). | docs/language.md §公開 ABI | `export def f :: string -> i64 = \s -> s.length⏎0` → `expect: check-error E1008 line=1` |
| ABI-006 | A `bool` MUST map to `int32_t` in the header. | docs/language.md §公開 ABI | `export def positive :: i64 -> bool = \n -> n > 0⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "int32_t tz_positive(int64_t arg0);"` |
| ABI-007 | An `f64` MUST map to `double` in the header. | docs/language.md §公開 ABI | `export def half :: f64 -> f64 = \x -> x / 2.0⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "double tz_half(double arg0);"` |
| ABI-008 | A narrow integer such as `i8` MUST travel as a normalized 32-bit value. | docs/language.md §公開 ABI | `export def neg :: i8 -> i8 = \x -> -x⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "int32_t tz_neg(int32_t arg0);"` |
| ABI-009 | The header MUST be a C header usable from C++: `#pragma once`, `<stdint.h>` and an `extern "C"` guard. | docs/language.md §公開 ABI | `export def one :: i64 -> i64 = \x -> x⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "#pragma once" ⏎ expect: file-has out.h "#include <stdint.h>" ⏎ expect: file-has out.h "extern \"C\" {"` |
| ABI-010 | An `i64u` MUST map to `uint64_t` and a narrow unsigned integer such as `i16u` MUST map to `uint32_t` in the header. | docs/language.md §公開 ABI | `export def big :: i64u -> i64u = \x -> x⏎export def narrow :: i16u -> i16u = \x -> x⏎0` → `cli: build $DIR --emit header -o $DIR/out.h ⏎ expect: exit 0 ⏎ expect: file-has out.h "uint64_t tz_big(uint64_t arg0);" ⏎ expect: file-has out.h "uint32_t tz_narrow(uint32_t arg0);"` |

## C.9 STD（標準ライブラリ。10 件）

**読むページ**: `LR/built-in-types-and-modules/*.md`（52 ページ。`std/*.tz` は 43 モジュール）。**探す文**: 各ページの関数の一覧の表の行 1 つにつき 1 規則（戻り値・失敗時の値・引数を変えないこと）。**優先順位**: 利用頻度の高い順（`Array`・`String`・`Maybe`・`Result`・`Vec`・`Map`・`Int`・`Math`）。**確かめ方**: `stdout` か `check-error`。配列や文字列を読むだけの関数には `ref` を付ける（`Array.sum (ref xs)`）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| STD-001 | `Array.sum` MUST add the elements of an array. | LR/built-in-types-and-modules/array.md | `let xs = [10, 20, 30]⏎Array.sum (ref xs)` → `expect: stdout "60\n"` |
| STD-002 | `Array.sort` MUST return an ascending copy and MUST leave its argument unchanged. | LR/built-in-types-and-modules/array.md | `let xs = [5, 2, 9]⏎$"{Array.sort (ref xs)} {xs}"` → `expect: stdout "[2, 5, 9] [5, 2, 9]\n"` |
| STD-003 | `Array.get` MUST return `None` for an out-of-range index instead of trapping. | LR/built-in-types-and-modules/array.md | `let xs = [10, 20, 30]⏎let a = match Array.get 1 (ref xs) with⏎\| Some v -> to_string v⏎\| None -> "none"⏎let b = match Array.get 99 (ref xs) with⏎\| Some v -> to_string v⏎\| None -> "none"⏎$"{a} {b}"` → `expect: stdout "20 none\n"` |
| STD-004 | `Array.update_at` MUST return an updated copy and MUST leave the original array unchanged. | LR/built-in-types-and-modules/array.md | `let original = [1, 2, 3]⏎let modified = Array.update_at 1 20 original⏎$"{original} {modified}"` → `expect: stdout "[1, 2, 3] [1, 20, 3]\n"` |
| STD-005 | `Int.abs` of the minimum signed value MUST return the minimum value unchanged. | LR/built-in-types-and-modules/int.md §選択と絶対値 | `def absolute :: i32 -> i32 = \x -> Int.abs x⏎⏎$"{absolute (-5)} {absolute (-2147483648)}"` → `expect: stdout "5 -2147483648\n"` |
| STD-006 | `Int.checked_add` MUST return `None` on overflow and `Some` of the sum otherwise. | LR/exception-handling/exception-handling.md | `def show :: i32 -> i32 -> string = \x y ->⏎    match Int.checked_add x y with⏎    \| Some value -> to_string value⏎    \| None -> "none"⏎⏎show 20 22 + " " + show 2147483647 1` → `expect: stdout "42 none\n"` |
| STD-007 | `Array.binary_search` MUST return the index of a present element in a sorted array. | LR/built-in-types-and-modules/array.md | `let scores = [5, 2, 9, 1, 8]⏎let ascending = Array.sort (ref scores)⏎match Array.binary_search (ref 5) (ref ascending) with⏎\| Some i -> to_string i⏎\| None -> "not found"` → `expect: stdout "2\n"` |
| STD-008 | `Array.product` MUST multiply the elements of an array. | LR/built-in-types-and-modules/array.md | `let factors = [1, 2, 3, 4]⏎Array.product (ref factors)` → `expect: stdout "24\n"` |
| STD-009 | `Parse.parse` MUST return `Some` for the text of a number and `None` otherwise. | LR/types-and-type-inference/cast.md | `let good = "42"⏎let bad = "4x2"⏎let a: Maybe<i64> = Parse.parse (ref good)⏎let b: Maybe<i64> = Parse.parse (ref bad)⏎let sa = match a with⏎\| Some v -> to_string v⏎\| None -> "none"⏎let sb = match b with⏎\| Some v -> to_string v⏎\| None -> "none"⏎$"{sa} {sb}"` → `expect: stdout "42 none\n"` |
| STD-010 | `Vec.push` MUST append and `for` over a `Vec` MUST visit the elements in insertion order. | LR/loops-and-conditionals/for-in.md | `let values = Vec.push 40 (Vec.push 20 (Vec.empty()))⏎let mut text = ""⏎for item in values do⏎    text = text + to_string item⏎text` → `expect: stdout "2040\n"` |

## C.10 EDN（edition。10 件。3 件が cli モード）

**読むページ**: `LR/organizing-tsuzuri/packages.md`（マニフェストと言語版）、`docs/stability.md`（edition の節）、`LR/values-and-functions/keywords.md`（edition ごとの予約語）。**探す文**: マニフェストの検証（`E0002`）、edition の既定、対応する edition、予約語。**注意**: A22（edition 2027）が入ると EDN-002・EDN-003 の期待（`supported editions: 2026`）が変わる。直すのは A22 の実装者（本文「前提とする他チケットのインターフェース」）。マニフェストは `Tsuzuri.toml` として本体に含める（`// edition:` は使わない）。

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| EDN-001 | A manifest without `edition` MUST be accepted and use the latest edition. | LR/organizing-tsuzuri/packages.md | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"` → `expect: check-ok` |
| EDN-002 | `edition = "2026"` MUST be accepted. | LR/organizing-tsuzuri/packages.md | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎edition = "2026"` → `expect: check-ok` |
| EDN-003 | An unsupported edition MUST be rejected with E0002 and the message MUST list the supported editions. | LR/organizing-tsuzuri/packages.md §言語版（edition） | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎edition = "2030"` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "\"code\":\"E0002\",\"message\":\"unsupported edition '2030'; supported editions: 2026\""` |
| EDN-004 | An unknown key in `[package]` MUST be rejected with E0002. | LR/organizing-tsuzuri/packages.md | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎bogus = 1` → `expect: check-error E0002` |
| EDN-005 | `tsuzuri new` MUST write `edition = "2026"` into the manifest. | LR/compiler/usage.md | `(empty)` → `cli: new $DIR/app ⏎ expect: exit 0 ⏎ expect: file-has app/Tsuzuri.toml "edition = \"2026\""` |
| EDN-006 | The reserved words of edition 2026 MUST NOT be usable as identifiers. | LR/values-and-functions/keywords.md | `[Main.tz]⏎let task = 1⏎task⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎edition = "2026"` → `expect: check-error E0002 line=1` |
| EDN-007 | Input without a manifest MUST use the latest edition. | docs/stability.md | `let task = 1⏎task` → `expect: check-error E0002 line=1` |
| EDN-008 | The edition MUST be a string: an integer value MUST be rejected with E0002. | LR/organizing-tsuzuri/packages.md | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎edition = 2026` → `expect: check-error E0002` |
| EDN-009 | A manifest without `[package]` MUST be rejected. | LR/organizing-tsuzuri/packages.md | `[Main.tz]⏎1⏎[Tsuzuri.toml]⏎[dependencies]` → `expect: check-error E0002` |
| EDN-010 | An unsupported edition MUST be rejected with E0002 before the sources are read, so no source diagnostic is reported in the same run. | LR/organizing-tsuzuri/packages.md §言語版（edition） | `[Main.tz]⏎let x: i64 = "s"⏎x⏎[Tsuzuri.toml]⏎[package]⏎name = "demo"⏎version = "0.1.0"⏎edition = "2025"` → `cli: check $DIR --json ⏎ expect: exit 1 ⏎ expect: stderr-has "\"code\":\"E0002\"" ⏎ expect: stderr-lacks "\"code\":\"E1003\"" ⏎ expect: stderr-lines 1` |
