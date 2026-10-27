# G21 付録: AI ガイドパックの全文・検証記録・L1 の設計

本文は [G21-ai-guide-pack.md](G21-ai-guide-pack.md)。この付録の A〜C は、実装者が `guide/` に**そのまま置く**ファイルの全文（HEAD `a2313d19` のコンパイラで検証済み）。D 以降は、検証の記録と、L1・std 索引・他チケットへの依頼の設計。

## 付録 A: `guide/card.md`（ガイド L0。5,429 字。上限 6,000 字。ASCII だけ）

````markdown
# Tsuzuri guide, level 0

Tsuzuri is statically typed, uses ownership instead of a GC, and compiles through LLVM to native code or WASM. One file is one module: `Main.tz` is the entry, `Shapes.tz` is module `Shapes` (call `Shapes.area`). A project from `tsuzuri new` starts `Main.tz` with `namespace Name`; keep that line first. The programs below and every error code named here are checked against this compiler. More: `tsuzuri guide --list`.

## Commands
- `tsuzuri check .` type-checks. With `--json` each diagnostic is one JSON line on stderr: `severity`, `code`, `message`, `path`, `span` (`line`, `column`, `end_line`, `end_column`). Exit 1 on errors.
- `tsuzuri run .` builds and runs; `tsuzuri test .` runs `test "name" = assert (...)`; `tsuzuri fmt .` formats in place.

## A program
```tsuzuri run=24%2055%207
record Point { x: i64, y: i64 } deriving (Eq)

union Shape =
    | Circle of i64
    | Rect of i64 * i64

def area :: Shape -> i64 = \shape ->
    match shape with
    | Circle r -> 3 * r * r
    | Rect (w, h) -> w * h

def sum_to :: i64 -> i64 = \n ->
    let mut total = 0
    for i in 1 .. n do
        total = total + i
    total

def norm1 :: ref Point -> i64 = \p -> Int.abs p.x + Int.abs p.y

def main :: unit -> i32 = \() ->
    let p = Point { x: -3, y: 4 }
    let mut sum = 0
    for s in [Circle 2, Rect (3, 4)] do
        sum = sum + area s
    do! IO.writeln $"{sum} {sum_to 10} {norm1 p}"
    0

test "area" = assert (area (Circle 2) == 12)
```
- `def f :: A -> B -> C = \a b -> body`; call `f x y`. The last expression is the value (no `return`). Blocks are indented. Comments are `//`.
- `do! action` runs an effect statement; `let! line = IO.read_line ()` binds its result. `main :: unit -> i32` returns the exit code.
- `if c then a else b`, several lines: `elif`. `match v with | P -> e | _ -> d`. Lambda: `\x -> x + 1`. `let mut x = 0`, then `x = x + 1`.
- `for x in xs do`, `for i in a .. b do` (includes `b`), `while c do`, `break`, `continue`.
- Strings: `a + b`, `$"n = {n}"`, `s.length`, `to_string 5`. Arrays: `[1, 2]`, `xs[i]`, `xs.length`, `Array.map f xs`, slice `ref xs[1..3]` (half-open). Growable: `let mut v: Vec<i64> = Vec.empty()`, `v = Vec.push 1 v`.
- Errors are values: `Maybe<T>` (`Some x`, `None`), `Result<T, E>` (`Ok x`, `Error e`). No exceptions, no `?`.
- Std names are qualified (`Array.sum xs`, `Int.abs n`, `Math.sqrt x`). A def without parameters is called `f()`.

## Types and numbers
- `i8 i16 i32 i64 i128`, unsigned `i8u i16u i32u i64u i128u` (not `u8`), `f32 f64`, `bool char string unit`, `[T]`, `(A * B)`, `Maybe<T>`.
- A bare integer literal is `i32` and a float literal `f64`, unless the context says otherwise. No implicit conversion: `n as f64`. A literal beyond `i32` needs a type: `let big: i64 = 3000000000` (else E1009).
- Integer `+ - *` wrap around. `/` truncates toward zero. `/` or `%` by zero and `MIN / -1` trap. Floats are IEEE 754.
- `==` on a record or union needs `deriving (Eq)`.

## Ownership
```tsuzuri run=5
def len :: ref string -> i64 = \s -> s.length
def eat :: string -> i64 = \s -> s.length
def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def main :: unit -> i32 = \() ->
    let s = "hi"
    let mut n = len s
    bump n
    n = n + eat s
    do! IO.writeln n
    0
```
1. A `string`, `Vec`, or record/union holding one has one owner. `let b = a`, passing `a` to a by-value parameter, or returning it moves it; any later use of `a` is E1012. Numbers, `bool`, `char`, `ref T` and arrays of Copy values are Copy (an array copy is O(n)). Copy a string on purpose with `clone_string s`.
2. To keep using a value, borrow it: parameter `ref T` reads, `ref mut T` writes. `f x` borrows automatically; `ref x` and `ref mut x` also work. `deref r` reads, `deref r = v` assigns.
3. Assignment and `ref mut` need `let mut` (else E1014). While a `ref mut` lives, any other use of the value is E1014. Many `ref` may coexist.
4. A reference cannot outlive its owner: returning `ref local` is E1013. Return the owned value.
5. Values are freed once at scope end. There is no free or delete; sharing is explicit (`Rc`, `Arc`).

## Pitfalls
- `pair.0` is E0002: `match pair with | (a, _) -> a`.
- `x += 1` is E0002: `x = x + 1`.
- `p.x = 1` and `xs[i] = v` are E1012 (fields and elements are immutable): `{ p with x = 1 }`, `xs = Array.update_at i v xs`.
- `IO.writeln s` as a statement is E1003 (`expected unit, found IO<unit>`): `do! IO.writeln s`.
- `for i in 0 .. xs.length` compiles, then traps: `..` includes the end. Use `0 .. xs.length - 1`.
- `else if` on a new line is E0002: `elif`.
- `n + 1.5` and `"a" + 1` are E1003: `n as f64 + 1.5`, `$"a{n}"`.
- `new type to with test do in` are reserved words (E0002): rename.
- `length xs`, `abs n` are E1002/E1003: `xs.length`, `Int.abs n` (`abs` is f64 only).
- `Display.to_string x` is E1002: `to_string x`, or `Display.display x`.
- `u8`, `usize`, `String`, `Option`, `True` are E1004/E1002: `i8u`, `i64`, `string`, `Maybe`, `true`.
- `Vec.empty ()` is E1006: `Vec.empty()`. `main` must return `i32` (E1003).

## Workflow
1. Write a little, then `tsuzuri check . --json`. Do not write 100 lines blind.
2. Fix the first diagnostic by `code`, `span.line` and `message`, then check again; later ones are often consequences.
3. Add a `test` for each function; `tsuzuri test .` must pass. Run `tsuzuri fmt .` last.
4. Unsure of a rule: `tsuzuri guide --level 1` or `--topic NAME`.
````

数字（`wc -m`）: 5,429。Phase 3・4 と他チケットが足す行（付録 H）の予算は 571 字。

## 付録 B: `guide/AGENTS.md.tmpl`（2,348 字。上限 3,500 字）

置換する語は `{{name}}`・`{{namespace}}`・`{{edition}}`・`{{version}}` の 4 つだけ（仕様の「テンプレート」）。

````markdown
# AGENTS.md

This is the Tsuzuri project `{{name}}` (namespace `{{namespace}}`, edition {{edition}}, compiler {{version}}).
Tsuzuri is not in your training data. Do not guess its syntax or its standard library: read the guide, check your code, and trust the diagnostics.

## Start here
1. Run `tsuzuri guide` and read all of it (about 6 KB). It matches the installed compiler, and every example in it was checked.
2. For more: `tsuzuri guide --level 1`, or one chapter with `tsuzuri guide --topic NAME` (`tsuzuri guide --list` shows the names).
3. Read `Main.tz` and the module you are about to change.

## Commands (run in the project root)
- `tsuzuri check . --json` type-checks. Diagnostics are JSON lines on stderr (`code`, `message`, `path`, `span.line`). Run it after every edit.
- `tsuzuri test .` runs every `test "name" = ...`. It must pass before you finish.
- `tsuzuri run .` builds and runs `main`.
- `tsuzuri fmt .` formats in place; `tsuzuri fmt --check .` only verifies.

## Rules
- Make the smallest change that does the task. Match the names and style around it.
- Fix the first diagnostic first, then check again; the later ones are often consequences.
- Add a `test` next to every function you add or change.
- Never make a diagnostic disappear by deleting a test, loosening a type, or copying values to dodge the borrow checker (copying an array costs O(n)). Borrow with `ref` or `ref mut`.
- Never invent std functions or modules. An unknown name (E1002) means look it up, not guess.
- Do not edit `Tsuzuri.lock` or `.tsuzuri/` by hand, and do not add a dependency unless asked.
- Run `tsuzuri fmt .` last, and keep unrelated formatting changes out of the diff.

## Layout
- `Tsuzuri.toml`: name, version, `edition`, `namespace`. Do not change `edition`.
- `Main.tz`: the entry point, `main :: unit -> i32`. Another file `Name.tz` is module `Name`; call it as `Name.f`.
- `.tsuzuri/`: build output, ignored by git.

## Frequent mistakes
- There is no `pair.0`, `x += 1`, `else if` on a new line, or early `return`: use `match`, `x = x + 1`, `elif`, `if ... else`.
- Effects need `do!`: `do! IO.writeln "hi"`. `for i in a .. b` includes `b`.
- A `string`, `Vec`, or record holding one moves when passed by value; pass `ref x` to keep using it.

## Project notes
Add project rules below: architecture, naming, what not to touch.
````

## 付録 C: `guide/llms.txt.tmpl`（1,002 字。上限 1,500 字）

````markdown
# {{name}}

> Tsuzuri project (namespace {{namespace}}, edition {{edition}}). Tsuzuri is statically typed, uses ownership instead of a GC, and compiles through LLVM to native code or WASM. It is not in LLM training data: read the guide before you write code.

## Read first
- [AGENTS.md](AGENTS.md): rules and commands for AI agents working in this project.
- `tsuzuri guide`: the language card for the installed compiler (about 6 KB, every example checked). Run it; it is not a file.

## More from the compiler
- `tsuzuri guide --level 1`: the full guide (types, type classes, modules, tasks, the std map).
- `tsuzuri guide --list`: chapter names for `tsuzuri guide --topic NAME`.
- `tsuzuri check . --json`: diagnostics as JSON lines on stderr.

## Source
- [Tsuzuri.toml](Tsuzuri.toml): package manifest.
- [Main.tz](Main.tz): entry point.

## Optional
- [Language reference](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/main/_tsuzuri/language-reference/index.md): the full manual (Japanese).
````

`llms.txt` の形式は <https://llmstxt.org> の規約（H1・引用の要約・H2 の節・リンクの箇条書き・`Optional` 節）に合わせた。リンクの先はプロジェクトの相対パスで、コマンドは項目の説明に書く。

## 付録 D: 検証の記録（HEAD `a2313d19` 相当の `target/release/tsuzuri`、2026-10-18）

作業ディレクトリは `target/tz-G21/`（`/tmp` は使わない）。`run` は共有のビルドキャッシュが他のプロセスにロックされると 1 分以上待つので、**手元の確認は `export TSUZURI_CACHE_DIR=$PWD/target/tz-G21/cache` を付ける**（付けない `run` が 1m49s、付けた `run` が 0.24〜0.8 s）。

| 確認 | コマンド | 結果 |
| --- | --- | --- |
| `tsuzuri new` | `tsuzuri new target/tz-G21/hello-world --namespace Acme::Hello` | `Created …/Tsuzuri.toml`、`…/Main.tz`、`…/.gitignore` の 3 行。`check` は無出力で終了コード 0、`run` は `Hello, Tsuzuri!`、`test` は `ok 1 - Main adds numbers` と `1 passed; 0 failed; 0 ignored` |
| 現状の `tsuzuri guide` | `tsuzuri guide` | `guide:1:1: error[E2001]: cannot inspect source 'guide': No such file or directory (os error 2)`、終了コード 1（`build` の省略形として入力を探す） |
| L0 の例 1（付録 A の「A program」） | `node scripts/check-guide.mjs`（試作） | `check` 成功、`run -O0`・`-O3` が `24 55 7`、`test` が通る |
| L0 の例 2（「Ownership」） | 同上 | `check` 成功、`run -O0`・`-O3` が `5` |
| 試作の出力 | `node target/tz-G21/check-guide-proto.mjs target/tz-G21/guide` | `Guide: 2 checked examples, 4 native runs (O0/O3), 1 test projects.` |
| 文字数 | `wc -m guide/*` 相当 | `card.md` 5,429、`AGENTS.md.tmpl` 2,348、`llms.txt.tmpl` 1,002。非 ASCII 文字は 0 |
| 落とし穴の主張 | 付録 E の 23 項目（予約語 7 語を展開して 29 件）を `check` | 全件の先頭の診断のコードが表と一致（`all pitfall claims hold`） |
| `check --json` の出力先 | `check e1012 --json 2>/dev/null` と `2>&1 >/dev/null` | 診断は **stderr** の JSON 1 行（`severity`・`code`・`message`・`path`・`span`）。stdout は空。`test --json` は stdout に 1 行 1 JSON |
| 引数 `.` のときの `path` | `cd e1012 && tsuzuri check . --json` | `"path":"./Main.tz"`（`check Main.tz` も同じ）。作業ディレクトリの名前が出ない |

`check --json` の実出力（E1012。`target/tz-G21/e1012/Main.tz` の 6 行目 `let m = consume text`）:

```json
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'text'","path":"e1012/Main.tz","span":{"start":148,"end":152,"line":6,"column":21,"end_line":6,"end_column":25}}
```

## 付録 E: 落とし穴の検証表（`tests/fixtures/guide/pitfalls.txt` の全文）

形式: `## <ID> <snippet> => <コード>[,<コード>…]` の行のあとに、次の `## ` の行までがソース（そのまま `Main.tz` に書く）。`<snippet>` は L0 の `card.md` の**バッククォートの中に現れる文字列**で、同じ行に各コードも現れなければならない（`tests/guide.rs::pitfall_claims_hold_and_appear_in_the_card`）。ソースに `{word}` があれば、`<snippet>` の空白区切りの語ごとに 1 回ずつ置き換えて検査する。コードは**先頭の診断**の `code`。HEAD で 23 項目（29 件）すべてが一致した。

```text
## P01 pair.0 => E0002
def first :: (i64 * i64) -> i64 = \pair -> pair.0

def main :: unit -> i32 = \() -> 0
## P02 x += 1 => E0002
def main :: unit -> i32 = \() ->
    let mut x = 1
    x += 1
    0
## P03 p.x = 1 => E1012
record P { x: i64 }

def main :: unit -> i32 = \() ->
    let mut p = P { x: 0 }
    p.x = 1
    0
## P04 xs[i] = v => E1012
def main :: unit -> i32 = \() ->
    let mut xs = [1, 2]
    xs[0] = 5
    0
## P05 IO.writeln s => E1003
def main :: unit -> i32 = \() ->
    IO.writeln "x"
    0
## P06 else if => E0002
def sign :: i64 -> i64 = \n ->
    if n < 0 then
        -1
    else if n == 0 then
        0
    else
        1

def main :: unit -> i32 = \() -> 0
## P07 n + 1.5 => E1003
def grow :: i64 -> f64 = \n -> n + 1.5

def main :: unit -> i32 = \() -> 0
## P08 "a" + 1 => E1003
def label :: i64 -> string = \n -> "a" + n

def main :: unit -> i32 = \() -> 0
## P09 new type to with test do in => E0002
def main :: unit -> i32 = \() ->
    let {word} = 1
    0
## P10 length xs => E1002
def size :: ref [i64] -> i64 = \xs -> length xs

def main :: unit -> i32 = \() -> 0
## P11 abs n => E1003
def mag :: i64 -> i64 = \n -> abs n

def main :: unit -> i32 = \() -> 0
## P12 Display.to_string x => E1002
def main :: unit -> i32 = \() ->
    let s = Display.to_string 5
    0
## P13 u8 => E1004
def main :: unit -> i32 = \() ->
    let a: u8 = 5
    0
## P14 usize => E1004
def main :: unit -> i32 = \() ->
    let a: usize = 5
    0
## P15 String => E1004
def main :: unit -> i32 = \() ->
    let a: String = "x"
    0
## P16 Option => E1004
def main :: unit -> i32 = \() ->
    let a: Option<i64> = None
    0
## P17 True => E1002
def main :: unit -> i32 = \() ->
    let a: bool = True
    0
## P18 Vec.empty () => E1006
def main :: unit -> i32 = \() ->
    let mut v: Vec<i64> = Vec.empty ()
    0
## P19 main => E1003
def main :: unit -> i32 = \() ->
    let r = 4i64
    r
## R01 E1012 => E1012
def eat :: string -> i64 = \s -> s.length

def main :: unit -> i32 = \() ->
    let s = "hi"
    let a = eat s
    let b = eat s
    0
## R02 E1014 => E1014
def bump :: ref mut i64 -> unit = \r -> deref r = deref r + 1

def main :: unit -> i32 = \() ->
    let mut c = 1
    let r = ref mut c
    let seen = c
    bump r
    0
## R03 let mut => E1014
def main :: unit -> i32 = \() ->
    let c = 0
    c = c + 1
    0
## R04 ref local => E1013
def first_word :: unit -> ref string = \() ->
    let s = "hello world"
    ref s

def main :: unit -> i32 = \() -> 0
```

`P05`・`P19` のように snippet が文の中の語だけのものは、card の同じ行に `E1003` があることだけを確かめる（`IO.writeln s` の行は `E1003` を含む）。`R01`〜`R04` の snippet は card の規則 1〜4 の行に現れるコード名で、card のその行にコードが書かれていること（`E1012`・`E1014`・`E1013`）を確かめる。実行時に trap する `for i in 0 .. xs.length`（`E2005`）は `check` では通るので、`scripts/check-guide.mjs` が `run` して `trap:` を含む標準エラーと終了コード 1 を確かめる（付録 G）。

## 付録 F: L1 の章立て・要点・文字数の配分（本文は実装者が書く）

L1 は 4 章（`guide/language.md`・`ownership.md`・`pitfalls.md`・`workflow.md`）。D-56 の上限 20,000 字を、章の上限で配る（テストが章ごとにも検査する）。各章は「節 → 要点（含める事実）→ 出典（日本語の言語リファレンスの正のページ。英語に要約する）→ 検証する例」の順。**例はすべて ` ```tsuzuri ` で、`check`（`run=` は実行）を通す。** 出典に書いていない事実は書かず、書く前に `tsuzuri check`／`repl` で確かめる（付録 A の作り方と同じ）。

| 章（ファイル） | 上限 | 節と配分 |
| --- | --- | --- |
| `language` | 8,500 | 1 モジュール・名前空間・パッケージ 1,300／2 宣言（record・union・型別名・定数・`deriving`・`test`）1,300／3 制御とパターン 1,000／4 ジェネリクスと型クラス 1,500／5 コンピュテーション式と効果 1,100／6 Task と並列 800／7 文字列・文字・数値 700／8 std の地図 800 |
| `ownership` | 4,500 | 1 Copy と非 Copy の表 1,000／2 ムーブ 700／3 借用 1,100／4 寿命と region 700／5 共有と解放（`Rc`・`Arc`・`Arena`・`Drop`）600／6 性能（複製の費用・`W1006`・バッファの再利用）400 |
| `pitfalls` | 4,000 | 「誤り → コード → 正しい書き方」を 1 項目 1 行で 30 項目以上。付録 E の 23 項目を含め、増やした項目は付録 E の形式で `pitfalls.txt` に足す（`pitfalls.md` の全項目が検証表にあることをテストが検査する）。Phase 4 で G23 の `hints::TABLE` が出荷済みなら、この章の表を生成物に替える（付録 H） |
| `workflow` | 2,990 | 1 プロジェクトとコマンド 700／2 診断の読み方（JSON のキー、コードの族 `E0xxx`＝構文・`E10xx`＝型・所有権・`E2xxx`＝ツール、最初の 1 件から直す）900／3 テスト（`test`・`Test.is_true`・`Gen.for_all`・`--filter`・`--list`）500／4 性能とデバッグ（`bench`・`--trap-info`・トラップ `E2005`・`--warn implicit-copy`）500／5 詰まったとき 390 |

節ごとの要点と出典:

**language**

1. モジュール・名前空間・パッケージ: 1 ファイル 1 モジュール（先頭が大文字）。`namespace A::B`・`using A::B`・フォルダーが名前空間の一部になる規則・`export`／`private`。`Tsuzuri.toml` の `[package]`（`name`・`version`・`edition`・`namespace`）と `[dependencies]`（`path`・`git`＋`rev`・`version`）、`tsuzuri fetch`。2 ファイルの例（`project=`・`file=` の fence）。出典: `_tsuzuri/language-reference/organizing-tsuzuri/` の `namespaces.md`・`modules.md`・`packages.md`・`using-declarations.md`・`access-control.md`。
2. 宣言: record の構築・フィールド・更新 `{ r with f = v }`、union（`of A * B`）、型別名、`const`、`deriving (Eq, Ord, Display, Hash, Default)`、`test`・`bench`。`Maybe`／`Result` の主な関数（`Maybe.default_value`・`Maybe.map`・`Result.map_error` ほか、`std/Maybe.tc`・`std/Result.tc` の署名から）。出典: `built-in-types-and-modules/record.md`・`union.md`・`maybe.md`・`result.md`、`types-and-type-inference/alias.md`。
3. 制御とパターン: `match` の形（タプル・リテラル・`when` ガード・`as`・`_`）、網羅検査 `E1021`、`for ... to`／`downto`（`i32` のカウンタ）、`while`、`break`／`continue`、`if` が式であること。出典: `pattern-matching/`・`loops-and-conditionals/`。
4. ジェネリクスと型クラス: `'a`、制約 `Add<'a> => ...`、組み込みのクラス、`class`・`instance`（`.tt` ファイル）、`deriving`、`dyn`。例は `examples/polymorphism`。出典: `types-and-type-inference/type-classes.md`・`generics.md`・`constraints.md`、`values-and-functions/generics-functions.md`。
5. コンピュテーション式と効果: `do!`・`let!`・`return`、`IO`・`Async` と `.tc` ビルダー、`Maybe`／`Result` のビルダーの使い方。出典: `computation-expressions/computation-expressions.md`、`built-in-types-and-modules/io.md`。
6. Task と並列: `task { ... }`、`Task.run`、`Task.parallel`、`Parallel.map`、値を Task へ move する規則。出典: `async-tasks-and-lazy/task.md`、`built-in-types-and-modules/parallel.md`。
7. 文字列・文字・数値: `string`（UTF-16 単位）と `utf8string`、`char`、補間 `$"..."`、`to_string`、数値の接尾辞（`i64` は `l`、`i64u` は `ul`。全部は表 `basic-types.md`）、`as` の規則。出典: `literals-and-strings/`、`types-and-type-inference/basic-types.md`・`cast.md`。
8. std の地図: 53 の予約モジュールのうち、よく使う 30 前後を 1 行ずつ（「`Array` — 配列。`map`・`fold`・`sort`…」の形）。全体の索引は Phase 3 の `tsuzuri guide std` を指す（Phase 3 までは「後で足す」と書かない。Phase 1 の版は地図を 800 字で持つ）。

**ownership**

1. Copy と非 Copy の表: `i*`・`f*`・`bool`・`char`・`ref T`・全フィールドが Copy の record／union・要素が Copy の配列は Copy。`string`・`Vec`・`Map` などと、それを含む record／union は非 Copy。配列の暗黙の複製は O(n)。出典: `ownership-and-memory/ownership.md`。
2. ムーブ: `E1012` の例（付録 A と別の形。条件分岐の合流・ループ・部分ムーブ）、直し方 3 つ（借用にする、`clone_string`、使う順序を替える）。
3. 借用: `ref`／`ref mut`／`deref`、呼び出しでの自動の借用、スライス `ref xs[a..b]`・`ref mut xs[a..]`、`E1014` の 2 つの形（`let mut` がない、借用が生きている）。出典: `ownership-and-memory/borrowing.md`。
4. 寿命と region: `E1013`、引数から導いた参照を返す書き方（`ref {r}` の region）、struct の中の参照が書けないこと。出典: `ownership-and-memory/lifetimes.md`。
5. 共有と解放: `Rc`・`Arc`（`Rc.share`）、`Arena` とハンドル、`Drop`、スコープの終わりの解放、トラップでは巻き戻さないこと。出典: `built-in-types-and-modules/rc.md`・`arena.md`、`ownership-and-memory/drop.md`。
6. 性能: 暗黙の複製の費用、`--warn implicit-copy`（`W1006`）、`xs = Array.update_at i v xs` と `Vec.push` の再束縛でバッファを再利用すること。出典: `ownership-and-memory/stack-and-heap.md`、`built-in-types-and-modules/array.md`・`vec.md`。

**pitfalls**: 付録 E の 23 項目に加えて、HEAD で `check` して確かめた誤りを足す。候補（実測が要る）: 整数の `abs`、`xs[1..3]`（`E0002`: 借用でなければならない）、`let add = \x y -> x + y` の単独（`E1015`）、`Some` なしの `Maybe`、`==` の `Eq` なし（`E1005`）、`return`、`#` のコメント、`new` の使用、`fn name () = ...`、`u8"..."` の形、`String.length s` の呼び出し規約。

**workflow**: 出典は `compiler/usage.md`・`compiler/diagnostics.md`・`built-in-types-and-modules/test.md`・`bench.md`。診断の JSON の実出力は付録 D の形を使う（コードと位置だけを引く。メッセージの文言は固定しない）。

## 付録 G: `scripts/check-guide.mjs` の骨子（試作で確かめた形）

`scripts/check-docs.mjs`（115 行）は**関数を export していない**（読み込むと `_tsuzuri` 全体を検査して終わるトップレベルのスクリプト）ので、「抽出の関数を再利用」できない。本チケットは `check-docs.mjs` を変えず、fence の抽出（約 30 行）を `check-guide.mjs` へ写し、写した旨をコメントに書く（決定 D7）。試作（`target/tz-G21/check-guide-proto.mjs`。`check-docs.mjs` の 54〜90 行と同じ意味）は、付録 A の 2 つの例を通した。実装すべき追加の振る舞い:

```js
// 骨子。入力: ガイドのディレクトリ（既定は <repo>/guide。環境変数 TSUZURI_GUIDE_EXAMPLES があればそれ）
// 1. *.md と *.tmpl を読む。fence ```tsuzuri [project=P] [file=F] [run=R] を check-docs.mjs と同じ規則で集める。
// 2. 各プロジェクトを一時ディレクトリへ書き、`tsuzuri check <Main.tz>`。run= があれば -O0 と -O3 で run して trim が R と一致。
//    `test "` を含めば `tsuzuri test <dir>`。
// 3. 実行時に trap する主張（付録 E の「for i in 0 .. xs.length」）: 小さなプログラムを `run` し、終了コード 1 と
//    stderr に "trap:" を含むことを確かめる（コンパイラの出力に依存する固定の期待。メッセージ全文は比べない）。
// 4. `tsuzuri guide --level 0`・`--level 1`・`--topic <各章>` の出力の長さ（Unicode のコードポイント数）を D-56 の上限と比べる。
// 5. 失敗は assert で投げ、終了コード 1。成功は 1 行: `Guide: N checked examples, M native runs (O0/O3), K test projects.`
```

環境変数 `TSUZURI_GUIDE_EXAMPLES`（D-55）は**このスクリプトだけが読む**（コンパイラは読まない）。`tests/guide.rs::the_checker_rejects_a_broken_example` が、壊れた例を持つ `tests/fixtures/guide_broken/`（`card.md` に ` ```tsuzuri ` の fence を 1 つ置き、中身が型エラーのソース）を指して実行し、終了コードが 0 でないことを確かめる。

## 付録 H: 他チケットへの依頼（guide の行の差し替え）

ガイドは「今のコンパイラの振る舞いだけ」を書く。下のチケットは、機能を出荷する同じ変更で、次の行を足す（予算は L0 の余り 571 字の内側）。

| 出荷するチケット | 直すファイル・位置 | 足す・替える文（英語。そのまま） | 増える字数 |
| --- | --- | --- | --- |
| G21 Phase 3 | `guide/card.md` の Workflow の手順 4 | `4. Unsure of a rule: tsuzuri guide --level 1 or --topic NAME. Std API: tsuzuri guide std, then tsuzuri guide std Array.` | 約 +55 |
| G23（`tsuzuri explain`） | `guide/card.md` の Workflow に手順 5 | `5. Unknown code: tsuzuri explain E1012.` | 約 +45 |
| G24（`tsuzuri fix`） | `guide/card.md` の Workflow に手順 6 | `6. Mechanical fixes: tsuzuri fix . (--dry-run shows the diff first).` | 約 +70 |
| A22（`..<`・`..=`） | `card.md` の `for i in a .. b` の行と落とし穴の行、`AGENTS.md.tmpl` の `.. includes b` | `for i in a ..< b do` は `b` を含まない、`a ..= b` は含む。`xs.length` までの添字は `0 ..< xs.length`。`..` は書かない | 約 +60 |
| A18／A19（`own`・`pure`） | `card.md` の所有権の規則 1・2 | `own` の引数の書き方と `pure def` を 1 行ずつ | 約 +120 |
| 合計 | | | 約 +350（余り 571 字に収まる） |

どのチケットも、足したあとに `node scripts/check-guide.mjs` と `cargo test --locked --test guide` を実行し、`card.md` が 6,000 字を超えないことを確かめる。

## 付録 I: std 索引（Phase 3）の実測

HEAD で数えた（`std/*` を正規表現で数えた概算。実装者は手順 14 で実装した関数の出力で数え直す）:

| 項目 | 値 | 数え方 |
| --- | --- | --- |
| std のソース | 47 ファイル | `grep -c 'include_str!("../std/' src/stdlib.rs` |
| 予約モジュール名 | 53 | `RESERVED_MODULES`（ソースを持たない `Int`・`Simd`・`Rc`・`Arc`・`FixedArray`・`Dyn` を含む） |
| opt-in モジュール | 15 | `OPT_IN`（Arena・Regex・Unicode・Json・Cbor・Bench・Gen・Async・Matrix・Net・Atomic・Mutex・MatrixView・Tensor・Channel） |
| 公開の `def` 行（全部） | 559 行・31,424 字 | `^(export )?def ` で `private` でない行 |
| 小文字で始まる公開の `def`（ビルダーの `Bind`・`Return`… を除く） | 517 行。`Module.` を付けた 1 行の合計 30,262 字（コア 276 行 15,689 字、opt-in 241 行 14,573 字） | 同上 |
| 型の宣言（record・union・class・type・extern type）の行 | 64 | `^(export )?(record\|union\|class\|type\|extern type) ` |
| 組み込み関数 `Builtin::ALL` | 246 個（`#[cfg(test)]` の 3 個を除くと 243） | `src/check.rs` の `Builtin::name` の腕 |
| 利用者向けの組み込み関数 | 193 個（`.` を持つ 181、持たない 12: `sqrt`・`floor`・`ceil`・`abs`・`to_float`・`to_int`・`assert`・`clone_string`・`unreachable`・`to_string`・`not`・`ignore`） | 内部の 50 個（名前に `__` を含む 47 個と `$builtin.*` の 3 個）と test 用 3 個を除く |
| 組み込みの内訳 | `Math` 32・`Int` 26・`Simd` 20・`Vec` 17・`Atomic` 11・`Array` 9・`Rc` 9・`Arc` 9・`List` 7・`Utf8String` 6・`String` 6・`Parallel` 5・`Task` 4・`Channel` 4・`Utf8Char` 3・`Owned` 3・`Mutex` 3・`Char` 2・`Bench` 2・`FixedArray` 1・`Dyn` 1・`Seq` 1 | 同上 |
| 見積もり | `std-all` は 30,262 字（std）＋ 約 10,600 字（組み込み 193 行 × 約 55 字）＋ 型の宣言 約 3,000 字 ≒ 約 44,000 字 | 上限 45,000 字にほとんど余裕がない。**溢れたときの規則**（仕様）が要る |

std の関数の署名は、`def name :: ...` の 1 行（`ref`・`ref mut`・値渡し）。std は `&T` と `ref T` が混在する（`Maybe.tc` は `&(Maybe<'a>)`）が、`docgen::type_text` は構文の `Reference` を `ref T` に正規化して出すので、索引は常に `ref` で出る。

## 付録 J: std 索引（Phase 3）の規則

本文の「std の索引（Phase 3）」の詳細。元データは (1) `stdlib::SOURCES` を `parser::parse_with_source` で読んだ公開の宣言、(2) `Builtin::ALL`（`#[cfg(test)]` を除く）のうち利用者向けのもの（名前に `__` を含まず、`$` で始まらない。付録 I の 193 個）。

- **モジュール**: (1) のモジュール名（`stdlib::module_name(path)`）と (2) の名前の `.` の前（組み込み関数の名前に現れるモジュール名。`Int`・`Simd`・`Rc`・`Arc`・`FixedArray`・`Dyn`・`Task` の類を含む。実装は名前を `.` で割って集める）の和集合。`.` のない 12 個の組み込み関数は疑似モジュール `prelude` に入れる。名前の ASCII 昇順（`prelude` は最後）。`opt-in` は `stdlib::is_opt_in(module)`（`pub(crate)`。同じクレートなので呼べる）。
- **関数の行**: `Module.name :: <署名>`。署名は `docgen::signature("def", name, regions, constraints, params, result)` の戻り値を `" :: "` で割った後ろ（`def {name}{regions}` の `{regions}` は `Module.name{regions}` の形で残す）。`"\n    "` は `" "` に直す。除外: `private`、名前が大文字で始まる（ビルダーの `Bind`・`Return` ほか）、`__` を含む。`extern def` も同じ形。
- **クラスのメソッドの行**: `program.classes` の各 `ClassDecl` `c` の `methods`（`SignatureDecl`）のうち `visibility` が `Private` でないものを、`{c.name.text}.{m.name.text} :: <署名>` の 1 行にして、そのソースのモジュールの `functions` に足す（`defaults` は既定の本体で、署名は `methods` にあるので行は増えない）。署名は関数の行と同じ `docgen::signature` で作るが、`constraints` の先頭にクラス自身の制約を足す: `ConstraintExpr { name: ConstraintName::Class(c.name.clone()), ty: TypeExpr { kind: TypeExprKind::Variable(c.variable.text.clone()), span: c.variable.span } }` に `m.constraints.clone()` を続ける（`c.variable.text` は `'` を含まないので、`type_text` が `'s` と書く）。例: `class Ord<'s> => Text<'s>` の `def find :: ref 's -> ref 's -> Maybe<i64>` は `Text.find :: Text<'s> => ref 's -> ref 's -> Maybe<i64>`（スーパークラスは行に出さない。型の行の `class` 行にだけ出る）。`functions` の数・整列・重複の除去は関数の行と同じ。
- **組み込み関数の署名**: `Builtin::scheme()` を `builtin_signature(&BuiltinScheme) -> String` で `(C1<'a>, C2<'b>) => P1 -> P2 -> R` にする（制約が 1 つなら括弧なし、0 なら `=> ` ごと省く。`Var("a")` は `'a`）。`BuiltinType` の描画は全 variant を wildcard なしで書く:

| variant | 描画 |
| --- | --- |
| `Var(v)` | `'v` |
| `Concrete(ty)` | `ty.display(&TypeContext { records: &[], unions: &[] })`（`Concrete` は原始型だけで、プロジェクトの record の id を持たない） |
| `Std { name, args, .. }` | `Name<args>`（引数なしは `Name`） |
| `Array(t)`・`ArrayView(t)`・`List(t)`・`Vec(t)`・`Task(t)` | `[t]`・`[t..]`・`[|t|]`・`Vec<t>`・`Task<t>` |
| `Tuple(ts)` | `(a * b)` |
| `Shared(t, kind)` | `kind.name()` に `<t>` を付けた形（`SharedKind::name()` が `Rc`・`Rc.Weak`・`Arc`・`Arc.Weak` を返す） |
| `Reference(t, false)`・`Reference(t, true)` | `ref t`・`ref mut t` |
| `Function(ps, r)` | `(p1 -> p2 -> r)`（引数なしは `(fn() -> r)`） |
| `UnsignedOf(t)`・`WidenOf(t)` | `Unsigned<t>`・`Wide<t>` |
| `SimdLane(t, Some(n))`・`SimdLane(t, None)`・`SimdMask(t)` | `Simd<t, n>`・`Simd<t, _>`・`Mask<t>` |
| `FixedArrayOf(t)` | `[t; N]` |

- **型の行**（モジュールの先頭。名前順）: record は `record Name<'a> { f: T, g: U }`（`stdlib::opaque_record("Module.Name")` が真なら `record Name<'a> (opaque)`）、union は `union Name<'a> = A | B of T`、型別名は `type Name = T`、定数は `const Module.NAME: T`、class は `class Name<'a> { m1, m2 }`。
- **1 モジュールのページ**（`std MODULE`）: 1 行目 `# MODULE`、2 行目に凡例 `ref T borrows (read), ref mut T borrows (write), a plain T moves (or copies if Copy). Opt-in modules load when named.`、空行、型の行、関数の行（行の文字列の昇順）。6,000 字を超えるときは、関数の行を昇順の末尾から削り、最後に `... N more: tsuzuri guide --topic std-all` を足して 6,000 字以内にする。
- **一覧**（`std`）: 1 行目 `# std modules`、モジュールごとに `format!("{:<12} {:>4} functions{}\n", name, count, if opt_in { "  (opt-in)" } else { "" })`、最後に `Details: tsuzuri guide std NAME. Everything: tsuzuri guide --topic std-all.`。`count` は関数の行の数（型の行を含まない）。
- **全体**（`std-all`）: `# std index` と凡例、各モジュールを `## MODULE (N)` と型・関数の行。45,000 字を超えるときの**溢れの規則**: opt-in モジュールを大きい順（関数の行の文字数の降順、同じなら名前の昇順）に、節の中身を 1 行 `MODULE: N functions; run 'tsuzuri guide std MODULE'` に置き換え、収まるまで続ける。opt-in をすべて置き換えても収まらなければ、残りのモジュールにも同じ規則を（同じ順で）続ける。
- 不明なモジュール名は `E2000`（`guide_unknown_std_module`）。大文字小文字は区別する。
