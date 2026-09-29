# D07: 文字列補間と書式指定

| 項目 | 内容 |
| --- | --- |
| ID | D07 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | D01 |
| 後続 | D08, G18 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（接頭辞 `$"..."`・`u8$"..."` の新構文）, D9（段 B の `numeric.ll` 増分。数値表示を使う全プログラムの runtime IR が増える） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: C#／F# の `$"..."`、Rust の `format!` に相当する補間・書式指定がない |
| 手本にする既存実装 | 表示の呼び出しと部分文字列の組み立て: `TypedExprKind::StructuralDisplay`（`src/check.rs`、`TypedExpr::children`）と `FunctionEmitter::structural_display`・`display_element`（`src/llvm_display.rs`）、所有権の arm は `src/ownership.rs` の `E::StructuralDisplay(arguments)`。一回確保: `@tz.string.allocate`・`@tz.string.copy`（`src/runtime/string.ll`）と `@tz.display.join`（`src/runtime/display.ll`）。数値の文字化: `tz_soft_format`・`format_float`・`shortest`（`src/runtime/numeric.c`）と `numeric_kind`（`src/llvm.rs`）。入れ子の字句状態: `Lexer::comment` の `depth`。文字列の字句: `Lexer::string`・`Lexer::unicode_escape`・`Lexer::recover` |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/computation.rs`, `src/formatter.rs`, `src/check.rs`, `src/ownership.rs`, `src/polymorph.rs`, `src/closures.rs`, `src/recursion.rs`, `src/call_specialization.rs`, `src/warnings.rs`, `src/llvm.rs`, `src/llvm_display.rs`, `src/llvm_frame.rs`, `src/runtime/format.ll`（新規）, `.gitignore`, `src/runtime/numeric.c` と再生成する `numeric.ll`・`math.ll`（段 B）, `tests/string_interpolation.rs`（新規）, `tests/fixtures/string_interpolation/Main.tz`（新規）, `tests/features.mjs`, `vsc/syntaxes/tsuzuri.tmLanguage.json`, `vsc/src/test/grammar.test.ts`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/strings-and-characters.md`, `_docs/library-reference/formatting-and-parsing.md`, `_docs/guides/from-fsharp.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

値を埋め込んだ文字列を、連結と `to_string` の組み合わせではなく一つの式で書けるようにする。
埋め込んだ値は借用して表示し、所有値を消費しない。結果の文字列は一回だけ確保する。
桁揃え・符号・精度・基数などの書式指定を、ホストの locale や printf に依存せず native と WASM で同一の結果にする。

実装者は Phase 1 だけを実装する。Phase 1 は段 A（補間と `{{`／`}}`）と段 B（書式指定 `{expr:spec}`）に分かれ、
段 A だけでも出荷できる。段 B は D9 の承認後に着手する。Phase 2 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D01 が done であること。`_features/_completed/D01-display-parse-format.md` があり、`grep -n "D01" _features/README.md` の状態欄が
  done であることを確認する。HEAD では `Display.display` と `to_string` が動く（現状の再現を参照）。
- D1 が承認済みであること。承認前はどの手順にも着手しない。段 B（手順 8 以降の書式指定）は D9 の承認後に着手する。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（補間を使わない fixture の IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §0）。

- `TokenKind` または `ExprKind` の大きさが増える（新しい payload を `Box` にしても増える）。または stack-depth の 3 テスト
  （`tests/polymorphism.rs::bounds_type_growing_polymorphic_recursion`、`parser::tests::bounds_recursive_and_flat_expression_depth`、
  `tests/computations.rs::bounds_nested_builder_expansion_not_just_source_syntax`）が失敗する。上限や stack サイズを上げたくなった場合も同じ。
- `lex_with_trivia`（formatter が使う）が、穴の中の token の trivia を正しく作れない。
- 合成した `Display.display` のパス式を `Checker::expression` で型付けできない（利用者の `Display` という名前のモジュールに解決される、
  既存の経路が使えない）。`Display` クラスの宣言や `to_string` の意味を変える必要が出た。
- 穴の型が参照かどうかを、既存の `ref` 被演算子と同じ規則（E1015）で決められない。
- `numeric.c` の再生成（GUIDE §2.2 と手順 11 のコマンド）が Apple Clang 21・llvm-link 21 で失敗する。または再生成で既存関数の本体が変わる。
- 精度 4096 以下の参照テストで `tzrt_big` の上限（`LIMBS`）の trap が起きる。`LIMBS` は上げない。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。`$` を含むソースの E0001 が別の診断になることだけは除く（テスト計画）。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 字句: `Lexer::token` は `'`、`u8'`、`u8"`、ASCII 英字（`Lexer::identifier`）、数字、`"`（`Lexer::string(false)`）、それ以外
  （`Lexer::symbol`）の順に分岐する。`Lexer::symbol` の表に `$` はなく、`$` は E0001
  `unexpected character '$'; identifiers use ASCII` になる。`u8$"x"` も `u8` の識別子の後の `$` で同じ E0001 になる。
  したがって `$"`／`u8$"` は既存の token と衝突しない。
- `Lexer::string(utf8)` はエスケープ `\"`、`\\`、`\n`、`\r`、`\t`、`\0`、`\u{...}`、UTF-16 だけの `\uXXXX` を受ける。
  それ以外のエスケープは E0001 `invalid string escape`、生の改行は E0001 `use '\n' for a newline inside a string`、
  閉じない文字列は E0001 `unterminated string literal`。`{`／`}` は通常の文字で、`"{x} }{ {{"` は今も有効な文字列リテラル。
- `Lexer::recover` は `"`／`u8"` で始まる token の失敗後に行末まで読み飛ばす。`Lexer::comment` は入れ子の深さを `depth` で数える。
- 構文: `StringLiteral`（`Utf16(Vec<u16>)`／`Utf8(String)`）を `TokenKind::String` と `ExprKind::String` が持つ。`Parser::literal` が
  変換し、`Parser::primary` は `TokenKind::String(_)` を `literal` へ送る。`Parser::enter` は `MAX_NESTING`（128）を超えると E0002
  `syntax nesting exceeds 128` を返す。式の中の `:` は record literal の `{ field: value }`（波括弧の内側）と `let` の型注釈だけで使われる。
- 表示（D01）: `Display.display :: &'a -> string` は借用、`to_string :: Display<'a> => 'a -> string` は消費（`Builtin::ToString` の
  `signature`）。一時値は借用できず E1013、参照型の値をさらに借用すると `Display<ref string>` のインスタンスがなく E1005 になる。
  利用者は数値・文字列の `Display` を定義できない（`instance Display<i64>` は E1016、`tests/display_parse.rs`）。
- 構造表示: `TypedExprKind::StructuralDisplay(Vec<TypedExpr>)` は借用した値とメソッドの列を持ち、`FunctionEmitter::structural_display`
  が要素ごとに `display_element` でメソッドを呼び、`@tz.display.join` が長さを合計してから一回だけ `@tz.string.allocate` する。
- 文字列の runtime: `@tz.string.allocate` は長さが 2^53 − 1 を超えると trap し、長さ 0 では確保しない。`@tz.string.copy`、
  `@tz.string.from_ascii`、`@tz.utf8string.from_string`（孤立サロゲートで trap）、`@tz.utf8string.from_ascii`、`@tz.string.decode_utf16`
  がある。連結 `+` は `@tz.string.concat` で、断片が多いと中間文字列を確保する。
- 数値表示: builtin の表示関数は 128 bytes の buffer で `tz_soft_format(out, input, kind)` を呼び、`@tz.string.from_ascii` に渡す
  （`kind` は `numeric_kind(ty)`）。浮動小数点は `shortest`（Dragon4）で最短往復表示。多倍長 `tzrt_big` は `LIMBS` 1400 語で、
  `tz_soft_parse` の注記どおり 4096 桁と指数範囲を収める。精度指定の変換はない。
- runtime の連結: `emit_target` の末尾が、生成 IR に `@tz.display.` があれば `display.ll`、`@tz.string.`・`@tz.alloc` などがあれば
  `string.ll` と `utf8string.ll`、`@tz_soft_` があれば `numeric.ll` 全体を連結する。
- formatter: `print_tokens` は各 token の source 片をそのまま出し、token 間の空白を `spacing` で決める。
- LSP: `src/lsp.rs` は hover・definition・documentSymbol だけで semantic tokens はない（G12 Phase 4 の予定）。VS Code の色分けは
  `vsc/syntaxes/tsuzuri.tmLanguage.json` の `strings` と `escape` で、単体テストは `vsc/src/test/grammar.test.ts`。
- `docs/language.md` の「表示と解析」は「文字列補間、任意の書式指定は未対応」と書く。D01 の補間の案（move）は D3 で借用に改める。

### 再現（検証済み）

各ケースを別ディレクトリの `Main.tz` に置き、`target/release/tsuzuri check <dir>` を実行する。

```tsuzuri
export def f :: string
fn f = $"x"
```

```tsuzuri
export def f :: i64 -> i64
fn f x =
    let text = Display.display (&(x + 1))
    text.length
```

```tsuzuri
def show :: &string -> string
fn show name = Display.display name + Display.display (&name)
export def f :: i64
fn f =
    let text = "ab"
    (show (&text)).length
```

```text
dollar/Main.tz:2:8: error[E0001]: unexpected character '$'; identifiers use ASCII
temp/Main.tz:3:34: error[E1013]: borrow requires a local place; bind the temporary with 'let' first
refparam/Main.tz:2:39: error[E1005]: no instance for Display<ref string>; define an instance or use a supported type
```

1 番目は `$` の位置、2 番目は `&` の位置、3 番目は 2 番目の `Display.display` の位置。3 番目の前半 `Display.display name` は受理される。

## 仕様

### 前提とする他チケットのインターフェース

- D01（done）: `Display<'a> { def display :: &'a -> string }`、数値・bool・unit・string・utf8string・char・utf8char の組み込み
  インスタンス、tuple・配列・リストの構造表示、`to_string`（GUIDE D-11）。数値の表示は `tz_soft_format`。本チケットはどれも変えない。
- D09（todo）は前提にしない。幅の単位は Unicode スカラー数に決める（D10）。

### 構文

新構文（実装後に有効。未検証）。

```text
interpolated = [ "u8" ] "$" '"' { text | "{{" | "}}" | hole } '"'
text         = 通常の文字列リテラルと同じ文字とエスケープ（'{' '}' を除く）
hole         = "{" expression [ ":" spec ] "}"
spec         = [ [ fill ] align ] [ "+" ] [ width ] [ "." precision ] [ type ]
fill         = '{' '}' '"' '\' CR LF 以外の Unicode スカラー 1 個
align        = "<" | ">" | "^"
width        = "1".."9" { "0".."9" }          （1 以上 4096 以下）
precision    = "0" | "1".."9" { "0".."9" }    （0 以上 4096 以下）
type         = "x" | "X" | "o" | "b" | "e" | "f"
```

- `$` と `"`、`u8` と `$` の間に空白は置けない（置くと今と同じ E0001）。`u8$"` の text は `u8"..."` と同じく UTF-8 のエスケープ規則。
- 穴は 1 行に収め、改行とコメントを含めない。穴の式には文字列リテラルや別の補間リテラルを書ける（`$"a{f "x"}b"`、`$"{$"{x}"}"`）。
- 穴の式の最上位（`(`、`[`、リスト括弧、`{` の内側でない位置）の `:` が書式指定を始める。`::` は始めない。record literal や
  型注釈の `:` は括弧・波括弧の内側にある。
- 穴を持たない `$"..."` は、同じ内容の通常の文字列リテラルと同じ `TokenKind::String` になる（`$"a{{b}}"` は `"a{b}"` と同一）。
- 1 リテラルの穴は 1024 個まで（D7）。

### 型規則

- `$"..."` は `string`、`u8$"..."` は `utf8string`。
- 穴 `{e}` で `e : T`。T が `ref U`／`ref mut U` なら表示する型は U、それ以外は T。表示する型に `Display` が要る（既存の E1005）。
  型変数の場合は `Display.display` を直接書いたときと同じ制約規則（宣言に `Display<'a> =>` が要る）。
- 書式指定の適用範囲（表示する型で判定する）:

| 指定 | 受ける型 | それ以外 |
| --- | --- | --- |
| fill・align・width | すべて | — |
| `+` | `Type::Integer`、`Type::Binary`、`Type::Decimal` | E1003 |
| precision（`e`・`f` を含む） | `Type::Binary`、`Type::Decimal` | E1003 |
| `x`・`X`・`o`・`b` | `Type::Integer` | E1003 |

- `+`・precision・type を含む指定では、穴の型付け直後に表示する型が具体的な数値型であること。型変数・未確定は E1003。
- precision だけ（`.2`）は `.2f` と同じ。`e`・`f` は precision が必須、`x`・`X`・`o`・`b` は precision 不可（どちらも字句で E0001）。

### 評価順序・所有権・借用

- 左から右。各穴の式はちょうど一回評価する。穴 i の値を評価して表示文字列を作ってから穴 i+1 を評価する。
  全穴の後に結果を一回確保して書き込み、その後に穴の表示文字列の解放と一時値の drop を穴の順に行う。
- 穴の値は消費しない。場所（ローカル・フィールド・参照外し）は共有借用、`ref U` の値はそのまま、`ref mut U` の値は共有の再借用
  （`reborrow_operand` と同じ木）、それ以外の一時値は隠れた領域へ置いて借用し、表示後に drop する。
  `to_string` と違い、非 Copy のローカルは穴の後も使える（D3）。
- 借用は補間式の全体の間続く（関数の引数と同じ）。`$"{x}{bump (&mut x)}"` は既存の E1014。
- 利用者の `Display` インスタンスは穴ごとに一回だけ呼ばれる。

### 数値・トラップ・native と WASM の差

- 表示はすべて生成 IR と `numeric.c` で行い、libc・locale・ホストの浮動小数点表示を使わない。native と WASM で同一の結果。
- 書式指定のない穴の文字列は `Display.display` の結果と同一（数値は D-11 の最短往復表示）。
- trap: 結果の長さが 2^53 − 1 単位を超える（`@tz.string.allocate`、utf8 は `@tz.utf8string.from_string` と確保）、`u8$` の穴の
  表示文字列が孤立サロゲートを含む（`@tz.utf8string.from_string`）、利用者の `Display` の trap。
- 幅: 表示文字列の Unicode スカラー数（string の孤立サロゲートは 1）が width 未満なら fill で埋める。既定の揃えは数値型が右、
  それ以外が左。`^` は不足分を左に floor(n/2)、右に残り。幅以上ならそのまま。
- 基数: 負数は `-` と絶対値（`i128` の最小値も正しい）。`0x` などの接頭辞なし。0 は `0`。`X` は大文字 `A`–`F`。
- `+`: 非負（`+0`、`+inf` を含む）に `+` を付ける。`-0.0` は `-0`（精度付きなら `-0.00`）。NaN は常に `nan`。
- `.Pf`: 正確な値を小数点以下 P 桁へ一回だけ最近接・偶数丸めする。P = 0 は小数点を出さない（`2.5` は `2`、`3.5` は `4`）。
- `.Pe`: 正確な値を有効数字 P + 1 桁へ一回だけ最近接・偶数丸めし、`d.ddd` と `e` と指数（負なら `-`、先頭の 0 なし）を出す。
  0 は `0.00e0`。丸めで桁が上がれば指数を 1 増やす（`9.96` の `.1e` は `1.0e1`）。
- inf・nan は精度に関係なく `inf`・`-inf`・`nan`。
- decimal（`d32`・`d64`・`d128`）は係数と指数が表す正確な十進値を同じ規則で丸める。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E0001 | 穴の外の単独の `}` | `a single '}' in an interpolated string must be written '}}'` | その `}` |
| E0001 | 穴が `"`・行末・ファイル末尾までに閉じない | `unterminated interpolation hole; close it with '}'` | `{` から現在位置 |
| E0001 | 穴の中の改行 | `an interpolation hole must stay on one line; bind the value with 'let' first` | `{` から改行まで |
| E0001 | 穴の中の `//`・`/*` | `comments are not allowed inside an interpolation hole` | コメントの先頭 2 文字 |
| E0001 | 書式指定の文法違反 | `invalid format spec '<spec>'; expected [[fill]align][+][width][.precision][type]` | `:` の次から `}` の前 |
| E0001 | width・precision が範囲外か先頭 0 | `format width and precision are at most 4096 and have no leading zeros; write '0>' to pad with zeros` | 同上 |
| E0001 | precision と `x`・`X`・`o`・`b` | `precision does not apply to integer format '<type>'; remove '.<precision>'` | 同上 |
| E0001 | precision のない `e`・`f` | `format type '<type>' needs a precision such as '.2<type>'` | 同上 |
| E0001 | 補間リテラル自体が閉じない | `unterminated string literal`（既存） | `$` から現在位置 |
| E0002 | 空の穴（空白だけも含む） | `an interpolation hole needs an expression; write '{{' for a literal brace` | `{` から `}` |
| E0002 | 1025 個目の穴 | `an interpolated string has at most 1024 holes; split it into several strings` | その穴の `{` |
| E0002 | 入れ子が 128 を超える | `syntax nesting exceeds 128`（既存の `Parser::enter`） | 現在の token |
| E1005 | 表示する型に `Display` がない | `no instance for Display<{型}>; define an instance or use a supported type`（既存） | 穴の式 |
| E1003 | 書式指定が表示する型に合わない | `format spec '<spec>' does not apply to <型>; <理由>`。理由は `'+' needs a number`、`precision needs a floating-point or decimal number`、`'x', 'X', 'o' and 'b' need an integer` | 書式指定 |
| E1003 | 数値の書式指定で型が具体的でない | `format spec '<spec>' needs a concrete numeric type, found <型>; annotate the value's type` | 書式指定 |
| E1012・E1014 | move・借用の規則違反 | 既存のメッセージ | 既存の位置 |

`<spec>` は `:` を含まない source の書式指定、`<型>` は `Type::display` の表示。

### 資源上限

- 入れ子: `Parser::enter` の 128（E0002）。字句の穴の stack は heap の `Vec` で再帰しない。
- 穴の数: 1 リテラルあたり 1024（E0002）。長さの合計は 2^53 − 1 以下の値を 1025 個足しても `i64` で溢れない。
- width・precision: 4096（E0001）。f128 の最小非正規化数の分母 2^16494 と 10^4096（約 13,607 bit）は `LIMBS`（44,800 bit）に収まる。

### 例

新構文（実装後に有効。未検証）。

```tsuzuri
export def greet :: i64
fn greet =
    let name = "Tsuzuri"
    let count = 3
    let text = $"hello {name}, {count + 1} times {{ok}}"
    assert (text == "hello Tsuzuri, 4 times {ok}")
    name.length + text.length
```

期待値は 34（`name` は穴の後も使え、`text` は 27 単位）。書式指定の結果（新構文。実装後に有効。未検証）:

| 式 | 結果 | 式 | 結果 |
| --- | --- | --- | --- |
| `$"{42:x}"` | `2a` | `$"{0.125:.2}"` | `0.12` |
| `$"{-255:X}"` | `-FF` | `$"{0.375:.2}"` | `0.38` |
| `$"{5:b}"`・`$"{8:o}"` | `101`・`10` | `$"{2.5:.0}"` | `2` |
| `$"{7:>4}"`・`$"{7:0>4}"` | `   7`・`0007` | `$"{1234.5:.2e}"` | `1.23e3` |
| `$"{"ab":*^5}"` | `*ab**` | `$"{0.1:.20}"` | `0.10000000000000000555` |
| `$"{1:+}"` | `+1` | `$"{-0.0:.2}"` | `-0.00` |

拒否: `$"{}"` は E0002、`$"a}b"`・`$"{x"`・`$"{7:.2x}"`・`$"{7:08}"`・`$"{x:e}"` は E0001、`$"{"s":x}"`・`$"{1.5:x}"` は E1003、
`Display` のない record の穴は E1005、`$"{x}{bump (&mut x)}"` は E1014。

### Phase 2（設計方針）

`Format` 型クラス（利用者型の書式指定）と、検証済みの書式を関数の引数として渡す仕組み。書記素クラスター単位の幅（D09 の後）。

## 設計

### データ構造

```rust
// src/syntax.rs（すべて新規）
pub enum TokenKind {
    // ...
    InterpolationStart(Box<InterpolationPiece>),  // `$"text{`、`u8$"text{`
    InterpolationMiddle(Box<InterpolationPiece>), // `}text{`、`:spec}text{`
    InterpolationEnd(Box<InterpolationPiece>),    // `}text"`、`:spec}text"`
}
pub struct InterpolationPiece {
    pub text: StringLiteral,      // `$"` は Utf16、`u8$"` は Utf8
    pub spec: Option<FormatSpec>, // この token が閉じる穴の書式指定。Start は常に None
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    pub fill: char, pub align: Option<FormatAlign>, pub plus: bool,
    pub width: u16, pub precision: Option<u16>, pub kind: Option<FormatKind>, pub span: Span,
}
pub enum FormatAlign { Left, Right, Center }
pub enum FormatKind { LowerHex, UpperHex, Octal, Binary, Exponent, Fixed }
pub enum ExprKind { /* ... */ Interpolated(Box<Interpolation>) }
pub struct Interpolation { pub texts: Vec<StringLiteral>, pub holes: Vec<InterpolationHole> } // texts は holes + 1 個
pub struct InterpolationHole { pub value: Expr, pub spec: Option<FormatSpec> }

// src/check.rs（すべて新規）
pub enum TypedExprKind { /* ... */ Interpolated(Box<TypedInterpolation>) }
pub struct TypedInterpolation { pub texts: Vec<StringLiteral>, pub holes: Vec<TypedHole> }
pub struct TypedHole {
    pub operand: TypedExpr,         // `ref U` の値。`temporary` なら所有する U の値
    pub temporary: bool,
    pub method: Option<TypedExpr>,  // `ref U -> string` の `Display.display`。None は直接コピーと数値書式
    pub spec: Option<FormatSpec>,
}

// src/lexer.rs
struct Lexer<'a> { source: &'a str, position: usize, holes: Vec<OpenHole> } // holes は新規
struct OpenHole { utf8: bool, depth: u32, start: usize }                     // 新規
```

`FormatSpec.fill` の既定は `' '`、`width` 0 は幅なし。`FormatSpec` は `Copy` で、typed tree へそのまま渡す。

### 段ごとの変更

GUIDE §6.1・§6.2・§6.6 のチェックリストを HEAD に当てはめた一覧。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 字句 | `src/syntax.rs` | `TokenKind` ほか上記 | 3 variant と payload。payload は `Box` |
| 字句 | `src/lexer.rs` | `tokenize`、`Lexer::token` | `holes` を空で初期化。`$"`・`u8$"` の判定を `u8'`・識別子の分岐より前に置き `interpolated(utf8)`（新規）へ。穴の中の CR・LF・`//`・`/*` は E0001 |
| 字句 | `src/lexer.rs` | `Lexer::string`、`string_unit`（新規） | 1 文字分のエスケープ処理を `string_unit` へ切り出し、`string` と `interpolated` で共有する（IR・診断は不変） |
| 字句 | `src/lexer.rs` | `Lexer::symbol`、`interpolation_piece`・`format_spec`（新規） | 穴があるとき `(`・`[`・リスト括弧・`{` で `depth` を増やし閉じで減らす。`depth` 0 の `}` は `interpolation_piece`、`depth` 0 の `:`（`::` でない）は `format_spec` の後に `}` を要求して `interpolation_piece` |
| 字句 | `src/lexer.rs` | `Lexer::tokens`、`Lexer::recover` | 末尾で穴が残れば E0001。穴の中の失敗は `holes` を空にして行末まで読み飛ばす |
| 構文 | `src/syntax.rs` | `ExprKind`、`Interpolation`、`InterpolationHole` | variant を 1 つ（`Box`） |
| 構文 | `src/parser.rs` | `Parser::primary`、`Parser::interpolation`（新規） | `TokenKind::InterpolationStart(_) => return self.interpolation()`。本体は `interpolation` に置く |
| 構文 | `src/parser.rs`、`src/parse_control.rs` | `TokenKind::String(_)` を式・引数の開始として並べる `matches!`・match（parser.rs は `primary` 以外に 1 か所、parse_control.rs は 3 か所） | `TokenKind::InterpolationStart(_)` を並べる。`Middle`・`End` は開始にしない。モジュール名などで `TokenKind::String(StringLiteral::..)` を読む箇所は変えない |
| ビルダー | `src/computation.rs` | `expand` | `ExprKind::String(_)` と同じ葉の列に入れず、各穴の `value` を再帰で展開する |
| 整形 | `src/formatter.rs` | `spacing`、`Layout`、`Canonical` | `Start`・`Middle` の後と `Middle`・`End` の前は空白なし。3 token は `Layout` の段付けを変えない。AST の走査は穴を辿る |
| 検査 | `src/check.rs` | `value_expression`、`Checker::interpolation`・`hole_place`・`check_spec`（新規） | 下の「アルゴリズム」。`value_expression` の arm は 1 行で呼ぶだけにする |
| 検査 | `src/check.rs` | `TypedExprKind`、`TypedExpr::children`・`children_mut` | 穴ごとに `operand`、`method` の順で返す（`StructuralDisplay` の arm の隣） |
| 走査 | `src/polymorph.rs`・`src/closures.rs`・`src/recursion.rs` | `walk`・`expression_types`・`Specializer::lower`、`free_locals`・`lower_expression`、`references` | `children` を使う箇所は変更なし。variant を列挙する match があれば `StructuralDisplay` と同じ扱いの arm を足す |
| 所有権 | `src/ownership.rs` | `E::StructuralDisplay(arguments)` の arm がある評価関数 | 新しい arm。穴の順に `operand`、`method` を `self.eval(.., Use::Consume, &during)?` |
| 特殊化 | `src/call_specialization.rs` | `may_mutate` ほか `StructuralDisplay` を並べる match、`all_children` | `StructuralDisplay` と同じ列 |
| 警告 | `src/warnings.rs` | `StructuralDisplay(_) => {}` を含む match | `{}` の列には入れず子を辿る（穴の中の未使用などを警告する） |
| 生成 | `src/llvm.rs` | `expression_mode` | `TypedExprKind::Interpolated(value) => self.interpolation(value, &expression.ty)` |
| 生成 | `src/llvm_display.rs` | `FunctionEmitter::interpolation`・`hole_text`・`format_capacity`（新規） | 下の「生成 IR とランタイム」 |
| 生成 | `src/llvm_frame.rs` | `TypedExprKind::String(text) if !text.is_empty()` の arm を持つ関数 | その arm が数える内容を読み、`Interpolated` の各 text と一時値に同じ扱いを足す |
| 生成 | `src/llvm.rs` | `emit_target` | 出力に `@tz.format.` があれば `runtime/format.ll` を連結する。`@tz.string.` の判定より前に置く |
| runtime | `src/runtime/format.ll`（新規）、`.gitignore` | `@tz.format.pad`（新規） | `!src/runtime/format.ll` を足す |
| runtime | `src/runtime/numeric.c`（段 B） | `tz_soft_format_spec`（新規） | 再生成で `numeric.ll`・`math.ll` を更新 |
| 文法 | `vsc/syntaxes/tsuzuri.tmLanguage.json`、`vsc/src/test/grammar.test.ts` | `strings` | 下の「VS Code の文法」 |
| LSP | `src/lsp.rs`、`src/semantic.rs` | — | 変更なし。穴の token は実際の source 位置を持つので hover・definition は既存の経路で動く。semantic tokens は G12 |

### 生成 IR とランタイム

`FunctionEmitter::interpolation` は穴ごとに `hole_text` で `%tz.string`（`u8$` は `%tz.utf8string`）を得て、長さを `add i64` で合計し、
一回だけ確保して `@tz.string.copy`（utf8 は `@tz.utf8string.copy`）で書き込む。text は `string_constant` の定数から直接コピーし、確保しない。
`$"a{x}b"`（`x : i64`）の形:

```llvm
%h0 = call %tz.string @...(ptr %x.slot)            ; display_element と同じ呼び出し
%n0 = extractvalue %tz.string %h0, 1
%total = add i64 %n0, 2                            ; text の単位数の合計は定数
%r = call %tz.string @tz.string.allocate(i64 %total)
%out = extractvalue %tz.string %r, 0
call void @tz.string.copy(ptr %out, ptr @.str.a, i64 1)
%at1 = getelementptr inbounds i16, ptr %out, i64 1
%d0 = extractvalue %tz.string %h0, 0
call void @tz.string.copy(ptr %at1, ptr %d0, i64 %n0)
; ... 残りの text。最後に drop_value(&Type::String, %h0)、一時値の drop_value
```

`hole_text` の分岐:

- `method` が None で書式指定がない: 表示する型が literal と同じ文字列型。operand の data と length をそのまま使い、確保も解放もしない。
- `method` が Some: `display_element` と同じく `comparison_values(method, &[operand])` を呼ぶ。`temporary` なら先に `spill` した slot を渡し、
  結果を組み立てた後に `drop_value` する。
- 数値書式（段 B）: `@tz.alloc(format_capacity)` の buffer に `tz_soft_format_spec(buffer, value_ptr, numeric_kind(ty), flags, precision)`
  を書き、`@tz.string.from_ascii` で文字列にして buffer を `@tz.free` する。
- width があれば `@tz.format.pad(%tz.string text, i64 width, i32 fill, i32 align)` を通す。入力を消費し、足りていれば同じ値を返す。
  `@tz.string.decode_utf16` でスカラー数を数え、不足分だけ `@tz.string.allocate` して fill（補助面なら 2 単位）と本文を書き、入力を `@tz.free` する。
- `u8$` で直接コピーでない穴は、UTF-16 の結果を `@tz.utf8string.from_string` で変換して元を drop する。

`format_capacity` は 16 + precision + 整数部の最大桁数（f16 5、f32 39、f64 309、f128 4933、d32 97、d64 385、d128 6145）、整数型は 16 + bit 数。
`flags` は bit 0 が `+`、bit 4–7 が形式（0 表示、1 `f`、2 `e`、3 `x`、4 `X`、5 `o`、6 `b`）。`align` は 0 左、1 右、2 中央。
`format.ll` は `@tz.string.*` を呼ぶので、`emit_target` で `string.ll` の判定より前に連結する。

### アルゴリズム

`Checker::interpolation`（穴ごと。値系の小さな dispatcher から呼ぶ非再帰の helper）:

1. `e` を `self.expression` で型付けし、型 T を推論状態で解決する。
2. T が `Type::Reference(U, false)` なら operand は `e`。`Type::Reference(U, true)` なら `reborrow_operand(e)` を `TypedExprKind::Borrow(.., false)`
   で包む。`hole_place(&e)`（ownership.rs の place 判定と同じ規則: ローカル、場所のフィールド、参照外し）なら `Borrow(e, false)`。
   それ以外は `temporary = true` で operand は `e`。T が未確定の推論変数なら参照でないとみなす。
3. U（または T）が `Type::String` で literal が UTF-16、`Type::Utf8String` で UTF-8、または数値書式なら `method = None`。
   それ以外は `Provenance::Generated` の `Display.display` パス式（`ExprKind::Field`）を `self.expression` で型付けし、
   `Inference::unify` で `ref U -> string` と単一化して `Some`。
4. `check_spec` が表の規則で書式指定を検査する（E1003）。

`tz_soft_format_spec`（段 B）: 整数は絶対値を基数で割って桁を作る。`f`・`e` は `decode` した正確な値を分子・分母の `tzrt_big` で持ち、
`magnitude` で先頭桁の位置 k を求め、`shortest` と同じく 1 桁ずつ（`compare`・`subtract`・`multiply_small`）必要な桁数を出す。
最後に余り r と分母 d の 2r と d を比べて最近接・偶数に丸め、繰り上がりは digit buffer 上で伝える（全桁 9 なら先頭に 1、`e` は指数 + 1）。
f128 の `.4096f` は約 9,000 桁 × 1,400 語の演算になり遅いが上限内で終わる（性能の主張はしない）。

### VS Code の文法

`strings.patterns` の先頭に `string.interpolated.tsuzuri`（begin `(?:u8)?\$"`、end `"`）を足す。中は `{{`・`}}` を
`constant.character.escape.tsuzuri`、`#escape`、穴 `meta.embedded.interpolation.tsuzuri`（begin `\{`、end `(?::[^{}"]*)?\}`、
patterns は `$self`）。波括弧を含む穴で色が早く閉じる制限は許容する（TextMate は深さを数えない）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、補間を使わない fixture の IR を保存する。
- 確認: 次がすべて成功する。stack-depth と特殊化上限の 4 テストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-d07
for s in display_parse strings chars; do
  target/release/tsuzuri build tests/fixtures/$s --emit llvm -o /tmp/tz-d07/$s-before.ll
  target/release/tsuzuri build tests/fixtures/$s --emit llvm -O3 -o /tmp/tz-d07/$s-before-O3.ll
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
```

### 手順 2: 字句（段 A）

- 変更: `src/syntax.rs` の `TokenKind`・`InterpolationPiece`、`src/lexer.rs` の `Lexer`・`tokenize`・`token`・`string`・`string_unit`・
  `interpolated`・`symbol`・`interpolation_piece`・`tokens`・`recover`、`tests/string_interpolation.rs`（新規）。
- 内容: 「段ごとの変更」の字句の行。書式指定（`format_spec`）はまだ作らず、穴の最上位の `:` は通常の `Colon` token のまま。
  テストファイルに `tests/display_parse.rs` の `accepts`・`rejects` を写し、`tsuzuri::lexer::lex` で token 列を得る `kinds`（新規）を足す。
- 確認: `cargo test --locked --test string_interpolation` が `2 passed`。`cargo test --locked --lib lexer` と `cargo test --locked --test strings`
  が成功する。

### 手順 3: 構文木・parser・整形

- 変更: `ExprKind::Interpolated` と `Interpolation`・`InterpolationHole`、`Parser::primary`・`Parser::interpolation`、`TokenKind::String(_)` を
  並べる箇所（`grep -n "TokenKind::String(_)" src/parser.rs src/parse_control.rs`）、`computation.rs` の `expand`、`formatter.rs`。
- 内容: `interpolation` は先頭で `self.enter()?`、終わりで `self.nesting -= 1`。穴ごとに `self.expression(0, true)` を呼び、次の token が
  `InterpolationMiddle`／`InterpolationEnd` でなければ既存の `self.error` 形式で E0002。空の穴と 1025 個目の穴は「診断」の E0002。
  深さは穴の深さの最大 + 1 で `Parser::make` へ渡す。check.rs の match が網羅的で compile できない場合は、手順 4 と一つの変更にする。
- 確認: `cargo test --locked --test string_interpolation` が `3 passed`。手順 1 の stack-depth 3 テストが成功する。
  `cargo test --locked --test computations` と `cargo test --locked --test borrowed_records` が成功する（formatter の既存テスト）。

### 手順 4: 型検査・所有権・走査

- 変更: `TypedExprKind::Interpolated`・`TypedInterpolation`・`TypedHole`、`TypedExpr::children`・`children_mut`、`value_expression`、
  `Checker::interpolation`・`hole_place`、「段ごとの変更」の走査・所有権・特殊化・警告の行。
- 内容: 「アルゴリズム」の 1–3。`check_spec` はまだない。`llvm.rs` の arm は手順 5 で足す（この手順のテストは `analyze` だけ）。
- 確認: `cargo test --locked --test string_interpolation` が `5 passed`。`cargo test --locked --test display_parse --test ownership` が成功する。

### 手順 5: 生成（段 A）

- 変更: `llvm.rs` の `expression_mode`、`llvm_display.rs` の `interpolation`・`hole_text`、`llvm_frame.rs`。
- 内容: 「生成 IR とランタイム」の書式指定以外。runtime ファイルは足さない。
- 確認: `cargo test --locked --test string_interpolation` が `7 passed`。`cargo test --locked` が成功する。

### 手順 6: E2E（段 A）と VS Code の文法

- 変更: `tests/fixtures/string_interpolation/Main.tz`（新規）、`tests/features.mjs` の `suites.string_interpolation`（新規）、
  `vsc/syntaxes/tsuzuri.tmLanguage.json`、`vsc/src/test/grammar.test.ts`。
- 内容: 「E2E」の段 A の case。文法テストは既存の test に `$"a{x}b{{"` と `u8$"{y}"` の行を足し、`string.interpolated.tsuzuri` と
  `meta.embedded.interpolation.tsuzuri` の scope を確かめる。
- 確認: 次が成功する。suite は native と WASM の `-O0`／`-O3` で全 case が一致し、`live == 0`、WASM の import が空。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri string_interpolation
(cd vsc && npm run test:unit)
```

### 手順 7: 段 A の最終確認

- 変更: なし（失敗があれば直す）。
- 内容: 手順 1 の IR と比べ、補間を使わないプログラムの IR が byte 単位で同一であることを確かめる。
- 確認: `diff` がすべて空。`cargo test --locked` と `node tests/features.mjs target/release/tsuzuri` が成功する。
  段 A の文書（「ドキュメント」の書式指定以外）を更新し、`node scripts/check-docs.mjs` が成功する。

```sh
for s in display_parse strings chars; do
  target/release/tsuzuri build tests/fixtures/$s --emit llvm -o /tmp/tz-d07/$s-after.ll
  target/release/tsuzuri build tests/fixtures/$s --emit llvm -O3 -o /tmp/tz-d07/$s-after-O3.ll
  diff /tmp/tz-d07/$s-before.ll /tmp/tz-d07/$s-after.ll && diff /tmp/tz-d07/$s-before-O3.ll /tmp/tz-d07/$s-after-O3.ll
done
```

### 手順 8: 書式指定の字句（段 B。D9 の承認後）

- 変更: `FormatSpec`・`FormatAlign`・`FormatKind`、`Lexer::symbol` の `:` 分岐、`format_spec`（新規）、`InterpolationPiece::spec`。
- 内容: 構文の `spec` を左から読む。fill は「2 文字目が align なら 1 文字目」。E0001 の 4 メッセージ。
- 確認: `cargo test --locked --test string_interpolation` が `8 passed`。

### 手順 9: 書式指定の型検査

- 変更: `check_spec`（新規）と `Checker::interpolation` の手順 3・4。
- 内容: 「型規則」の表と E1003 の 2 メッセージ。数値書式の穴は `method = None`。
- 確認: `cargo test --locked --test string_interpolation` が `9 passed`。

### 手順 10: 幅と揃え

- 変更: `src/runtime/format.ll`（新規）の `@tz.format.pad`、`.gitignore`、`emit_target`、`hole_text` の width 分岐。
- 内容: 「生成 IR とランタイム」の pad。`emit_target` では `@tz.string.` の判定より前に連結する。
- 確認: `cargo test --locked` が成功する。fixture に `spec_padding` を足し、手順 6 の suite が成功する。

### 手順 11: `tz_soft_format_spec`

- 変更: `src/runtime/numeric.c`、再生成した `src/runtime/numeric.ll`・`src/runtime/math.ll`。
- 内容: 「アルゴリズム」の桁生成。static な lookup table は置かない。`Globals::FIRST_METADATA` が両 `.ll` の metadata 範囲より上にあることを確かめる。
- 確認: 次の再生成が成功し、`git diff --stat src/runtime` が `numeric.c`・`numeric.ll`・`math.ll` だけを示す。`cargo test --locked --test display_parse`
  が成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
TSUZURI_CLANG=/usr/bin/clang python3 src/runtime/generate.py
TSUZURI_CLANG=/usr/bin/clang TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link python3 src/runtime/generate_math.py
```

### 手順 12: 数値書式の生成と E2E

- 変更: `hole_text` の数値分岐、`format_capacity`、fixture と `suites.string_interpolation` の段 B の case、`tests/string_interpolation.rs`。
- 内容: `flags`・`numeric_kind`・`format_capacity` を「生成 IR とランタイム」どおりに渡す。
- 確認: `cargo test --locked --test string_interpolation` が `10 passed`。手順 6 の suite が native と WASM の `-O0`／`-O3` で成功する。

### 手順 13: 文書と最終確認

- 変更: 「ドキュメント」の全ファイル。
- 内容: 手順 7 の IR 比較を再実行する（段 B 後は `numeric.ll` 由来の定義だけが差分。利用者関数と他の runtime は同一）。
- 確認: `cargo test --locked`、`node tests/features.mjs target/release/tsuzuri`、`(cd vsc && npm run test:unit)`、`node scripts/check-docs.mjs` が成功する。
  手順 1 の 4 テストが成功する。

## テスト計画

### Rust テスト

`tests/string_interpolation.rs`（新規）。`accepts` は IR を両 target で 2 回出して一致を確かめ、`@printf`・`@memcmp`・`@strtod`・`@strtof`・
`@snprintf` がないこと、`declare` が重複しないことを見る（`tests/display_parse.rs` と同じ）。

| テスト | 手順 | 確かめること |
| --- | --- | --- |
| `lexes_pieces_and_nested_holes` | 2 | `$"a{x}b"` が Start("a")・Ident・End("b")。`$"a{{b}}"` が `TokenKind::String`（"a{b}"）。`u8$"é{x}"` の text が Utf8。`$"{f "q"}"`、`$"{$"{x}"}"`、`$"{ {a: 1}.a }"` の token 列。`"{x}"` は今までどおり。`$ "x"` は E0001 |
| `rejects_malformed_interpolation_text` | 2 | `$"a}b"`、`$"{x"`、ファイル末尾の `$"{x`、穴の中の改行・`//`・`/*`、`$"\q{x}"` が E0001。`lex_all` で誤りの次の行が正しく読める |
| `parses_holes_and_bounds_nesting` | 3 | `$"{}"`・`$"{ }"` と 1025 穴が E0002、1024 穴は受理。入れ子 129 段が E0002、100 段は受理。`format_source("Main.tz", .., SourceKind::Code)` が `$"a{ x + 1 }b"` を `$"a{x + 1}b"` にし、2 回目は不変。`$"a{{b}}"` は byte 単位で保たれる |
| `holes_borrow_evaluate_once_and_keep_owners` | 4 | 非 Copy のローカルを穴の後で使う、`ref`・`ref mut` 引数、フィールド、一時値、0 引数の def、`Display<'a> =>` の多相関数、入れ子、`u8$` の string・utf8string の穴、利用者の `Display` がすべて `analyze` を通る |
| `rejects_holes_without_display_or_with_conflicts` | 4 | `Display` のない record は E1005、`$"{x}{bump (&mut x)}"` は E1014、`to_string s` の後の `$"{s}"` は E1012。制約のない多相関数の穴は、同じ値に `Display.display` を直接書いたプログラムと同じコード |
| `emits_one_allocation_and_deterministic_ir` | 5 | `fn f s n = $"a{s}b{n}c"`（`&string -> i64 -> string`）の関数本体に `@tz.string.allocate(` が 1 回、`@tz.string.concat` と `@tz.string.new` が 0 回 |
| `plain_literals_keep_their_ir` | 5 | `$"abc"` と `"abc"`、`$"a{{b}}"` と `"a{b}"`、`u8$"é"` と `u8"é"` のプログラムの IR が一致する |
| `lexes_format_specs` | 8 | `{x:*^+10.3e}`・`{x:>4}`・`{x:0>4}`・`{x:<<}`・`{x:.2}`・`{x:X}` の `FormatSpec`。`.2x`・`08`・`e`・`f`・`4097`・`.4097`・`z`・`+-` が E0001 |
| `checks_specs_against_types` | 9 | `{"s":x}`・`{1.5:x}`・`{"s":+}`・`{7:.2}`・多相の `{value:x}` が E1003。`{"s":>4}`・利用者 record の `{r:>8}`・`{1.5:+.2e}`・`{7:+}` を受理 |
| `spec_ir_uses_soft_format_and_pad` | 12 | 数値書式があれば `@tz_soft_format_spec`、width があれば `@tz.format.pad` が IR にあり、書式指定のないプログラムにはどちらもない |

### E2E

fixture `tests/fixtures/string_interpolation/Main.tz`（新規）、suite `string_interpolation`（`tests/features.mjs`、GUIDE §7.2）。
文字列の比較は fixture の `digest`（新規。各コード単位 u で `h = (h * 131 + u) % 1000000007`）と、同じ式の JavaScript 版で行う。
期待値は現在のコンパイラーの出力から作らない。浮動小数点の参照は `DataView` で取り出した仮数と指数から BigInt で正確に最近接・偶数丸めする
helper（新規）で作る。`Number.prototype.toFixed` は同点を大きい方へ丸めるので参照に使わない。

段 A の case:

- `basic_holes` → 34n。`borrowed_holes` → 15n（`$"{s}-{p.name}-{s}"` は `abc-xy-abc`、その後 `s`・`p.name` の長さを足す）。
- `evaluation_order` → 123003n（`ref mut` の記録に桁を足して返す `step` を 3 つの穴で呼び、記録 123 × 1000 + 結果の長さ 3）。
- `temporaries` [n]、n ∈ {-5n, 0n, 41n} → `digest` of `${n + 1n}|abc|42`（一時値・`clone_string`・0 引数の def）。
- `utf8_holes` → 11n（`u8$"é{s8}😀{s16}{c}"`、`u8"ü"`・`"ß"`・`'x'` の byte 数 2 + 2 + 4 + 2 + 1）。
- `nested_literals` → 1（`$"<{$"[{x}]"}>" == "<[42]>"`）。`display_instance` → 利用者 `Display` の record で `<label>` の長さ 7n。
- `owned_loop` [n]、n ∈ {0n, 1n, 1000n} → 0 から n − 1 の各 i について `$"{i}:{name}"`（name は `"ab"`）の長さの合計。JavaScript で計算。
- traps: `trap_utf8_surrogate`（`u8$"{s}"`、s は `"\uD800"`）、`trap_display`（`Display` 本体が `unreachable ()`）。

段 B の case（手順 10・12 で足す）:

- `spec_integer` [v]、v ∈ {i64 最小、-255n、-1n、0n、1n、42n、i64 最大} → `$"{v:x}|{v:X}|{v:o}|{v:b}|{v:+}|{v:>25}|{v:*^25}|{v:0>25}"` の digest。
  参照は BigInt の `toString(16)`・`toString(8)`・`toString(2)`、`padStart`、中央は左に floor。`0>` は符号の前に 0 を埋める。
  `spec_i128_min` は `$"{m:x}"`（m は `i128` の最小値）の digest で、参照は `(-(1n << 127n)).toString(16)`。
- `spec_f64` [x]、x ∈ {0, -0, 0.1, 0.125, 0.375, 2.5, 3.5, 1e21, 1e-7, 5e-324, 1.7976931348623157e308, Infinity, -Infinity, NaN} →
  `$"{x:.0}|{x:.2}|{x:.17}|{x:+.3}|{x:.0e}|{x:.3e}|{x:.16e}|{x:>12.1}"` の digest。`spec_f32` [x] は同じ x の `Math.fround` で同じ式。
- `spec_wide` [k]、k ∈ {0n..4n} → `0.1f16`、`0.1f128`、`0.125d32`、`2.5d64`、`1e-30d128` の `.3`・`.20`・`.2e` の digest。
  二進は 1/10 を 11 bit・113 bit の仮数へ最近接・偶数丸めした値から、decimal は係数と指数から参照を作る。
- `spec_padding` → `$"{"é😀":*>5}|{"\uD800":3}|{"ab":^6}|{r:>8}"` の digest（`***é😀`、`\uD800` と空白 2 つ、`  ab  `、右揃えの label）。
- `inspect(ir)`: `@tz_soft_format_spec` と `@tz\.format\.pad` があり、`@printf|@snprintf|@strtod|@strtof` がない。

すべての case を native と WASM の `-O0`／`-O3` で実行し、native は `live == 0`、WASM は import が空であることを harness が確かめる。

### 既存テストへの影響

なし。`$` の直後に `"` を置いたソースで E0001 を期待する既存テストがあれば、それだけは `$` の後に `"` を置かない形へ直す（停止条件の例外）。
段 B は `numeric.ll` を変えるので、生成 IR 全体を固定値で比べる既存テストがあれば停止して報告する。

### 性能

閾値は置かない。`emits_one_allocation_and_deterministic_ir` が一回確保と中間文字列の不在を IR で確かめる。時間の比較は記録しない。

## ドキュメント

- `docs/language.md`: 「string と utf8string」に補間リテラルの構文・エスケープ・`{{`／`}}`、「表示と解析」の未対応の記述から
  「文字列補間、任意の書式指定」を消し、穴の借用・評価順序・書式指定の表を足す。「診断」の表は変えない（コードは既存）。
- `docs/architecture.md`: 表示の段落（`runtime/display.ll` の説明）に `Interpolated` の一回確保、`runtime/format.ll`、`tz_soft_format_spec` を足す。
- `_docs/language-reference/strings-and-characters.md`: 「エスケープ」の後に「補間」節。
- `_docs/library-reference/formatting-and-parsing.md`: 「数値の表示形式」の後に「書式指定」節（段 B）。
- `_docs/guides/from-fsharp.md`: 「構文の対応」に `$"..."` の行（F# の `$"..."` と同じ形、書式は `{x:.2}` で `%` 形式は使わない）。
- `_docs/feature-status.md` の D07 の行と `_features/README.md` の D07 の状態。段 A だけで出荷する場合は「段 A 完了」と書く。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/language-reference/strings-and-characters.md _docs/library-reference/formatting-and-parsing.md _docs/guides/from-fsharp.md _docs/feature-status.md`。

## 受け入れ条件

- [ ] `$"..."`・`u8$"..."` が「仕様」どおりに字句解析・型付けされ、「診断」の全行を Rust テストが確かめる。
- [ ] 穴の値を消費せず、左から右へ一回ずつ評価し、結果を一回だけ確保する（Rust テストと E2E）。
- [ ] 書式指定の結果が独立した参照と native・WASM × `-O0`／`-O3` で一致し、浮動小数点の精度指定が二重丸めなしで丸められる。
- [ ] 補間を使わないプログラムの IR が段 A で byte 単位で変わらない。段 B の差分は `numeric.ll` 由来の定義だけ。
- [ ] `live == 0`、WASM の import が空、stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功する。
- [ ] VS Code の文法テストが補間の scope を確かめる。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `u8$"` の判定を `Lexer::token` の英字の分岐より後に置くと `u8` が識別子になる。`$"` と一緒に `u8'`・`u8"` の判定の並びへ置く。
- text の中の `{{` は常に波括弧 1 個。record literal で始まる穴は `{ {a: 1}.a }` と空白を入れる。テストに入れる。
- 穴の中の失敗で `holes` を空にしないと、以後の `}` と `"` を誤読して大量の誤診断が出る。`lex_all` の回復テストで確かめる。
- `depth` は `Lexer::symbol` の最長一致（`[|`・`|]`・`>=>`）の後に数える。`::` を書式指定と誤認しない。
- `Parser::interpolation` の早期 return で `nesting` を戻し忘れると、深さ 128 より前に E0002 が出る（過去に起きた不具合）。深さテストで確かめる。
- 直接コピーは literal と穴の文字列型が同じときだけ。`$"{u8"x"}"` は表示（UTF-16 への変換）を通す。
- 一時値と穴の文字列は結果へコピーした後に drop する。先に drop すると解放済みの領域を読む。
- `format.ll` を `@tz.string.` の判定より後に連結すると `string.ll` が連結されず link で失敗する。
- `numeric.c` を変えたら両方の生成器を Apple Clang 21 と llvm-link 21 で実行し、`.ll` を手で直さない。static な表は wasm32 `-O0` で壊れた実績がある。
- 穴の中の識別子は実際の source 位置を持つ。`Provenance::Generated` は合成した `Display.display` だけに付け、利用者の式には付けない。

## 対象外

- locale 書式、桁区切り、通貨、printf 互換の `%d` 形式、実行時に組み立てた書式文字列。
- `#`（`0x` などの接頭辞）、符号を考慮する `0` フラグ、大文字の `E`、`-`・空白の符号指定。
- 書記素クラスター単位の幅（D09 の後）、`Format` 型クラス（Phase 2）、semantic tokens（G12）、TextMate での波括弧の深さの追跡。

## 決定事項

### D1: 接頭辞

- 決定: `$"..."` が `string`、`u8$"..."` が `utf8string`。穴のない補間リテラルは通常の文字列リテラルと同じ token。
- 理由: C#／F# と同じ形。`$` は HEAD で E0001 なので既存のプログラムの意味を変えない（現状の再現）。`f"..."` は `f "..."` の関数適用と
  区別できず既存の意味を変える。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: 字句の表現

- 決定: `InterpolationStart`・`InterpolationMiddle`・`InterpolationEnd` の 3 token と、穴の中の通常の token 列。`Lexer` は穴の stack を持つ。
- 理由: 穴の token が実際の source 位置を持ち、診断・formatter（`print_tokens` は token の source 片を出す）・LSP をそのまま使える。
  token に token 列を入れる案は、部分 lexer と位置の付け替えが要る。
- 状態: 既定案（実装者はこの案に従う）

### D3: 穴は借用する

- 決定: 穴の値を消費しない。場所は共有借用、参照はそのまま、一時値は隠れた領域で drop する。
- 理由: 元のチケットの仕様どおり。D01 の「string interpolation（フェーズ 2）」の move 案は完了済みチケットの設計メモで、GUIDE D-11 には
  補間の規定がない。借用なら `$"{name}"` の後も `name` を使える。
- 状態: 既定案（実装者はこの案に従う）

### D4: 型付けを checker で行う

- 決定: parser は `ExprKind::Interpolated` を作るだけで、穴の分類と `Display.display` の合成は `Checker::interpolation` で行う。
- 理由: parser の脱糖（`Display.display e`）は、一時値で E1003 `expected ref an undetermined type, found i64` になり（`Display.display (x + 1)` で確認）、
  0 引数の def と局所変数を名前解決なしで区別できない。型付け後なら参照・場所・一時値を区別でき、穴の式の位置で診断できる。
- 状態: 既定案（実装者はこの案に従う）

### D5: 一回確保と直接コピー

- 決定: 長さを合計して一回だけ確保し、text は定数から、同じ文字列型の穴は元の buffer から直接コピーする。段 A は runtime を足さない。
- 理由: 中間文字列を作らず、補間を使わないプログラムの IR を変えない。`Display<string>` は組み込みで利用者が上書きできない（E1016）ので結果は同一。
- 状態: 既定案（実装者はこの案に従う）

### D6: 書式指定の文法

- 決定: `[[fill]align][+][width][.precision][type]`。precision だけは `f`、`e`・`f` は precision 必須、既定の揃えは数値が右・他は左。
- 理由: Rust の `format!` の部分集合で読み方が一意。`e`・`f` の精度なしの意味（最短桁）は別の実装が要るので入れない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 上限

- 決定: 入れ子 128（`Parser::enter`）、穴 1024 個、width・precision 4096、穴は 1 行でコメントなし。
- 理由: 長さの合計が溢れ検査なしの `add i64` で済み、`LIMBS` の範囲に収まる。1 行の穴は layout に依存する parser と formatter を守る。
- 状態: 既定案（実装者はこの案に従う）

### D8: 評価と解放の順序

- 決定: 穴の順に評価と表示を交互に行い、結果の書き込み後に穴の順で解放する。借用は補間式の全体の間続く。
- 理由: 関数引数と同じ規則で ownership.rs の既存の評価（`E::StructuralDisplay` と同じ形）に乗る。
- 状態: 既定案（実装者はこの案に従う）

### D9: 段 B の runtime

- 決定: 数値書式は `numeric.c` の `tz_soft_format_spec`（`tzrt_big` と `shortest` の helper を共有）、揃えは新しい `runtime/format.ll`。
- 理由: 正確な丸めに多倍長が要り、別ファイルにすると helper が重複する。ただし `numeric.ll` は `@tz_soft_` を使う全プログラムへ丸ごと
  連結されるので、補間を使わないプログラムの runtime IR も増える。
- 状態: 要承認（承認前は段 B（手順 8 以降）に着手しない）

### D10: 幅の単位

- 決定: Unicode スカラー数。string の孤立サロゲートは 1 と数える。
- 理由: string と utf8string で同じ数になり、Unicode データの表が要らない。書記素クラスター単位は D09 の後で再検討する。
- 状態: 既定案（実装者はこの案に従う）

### D11: `u8$` の穴

- 決定: utf8string の穴は直接コピー、それ以外は UTF-16 の表示を `@tz.utf8string.from_string` で変換する（孤立サロゲートで trap）。
- 理由: `Display` は `string` を返すので変換が要り、`Utf8String.from_string` と同じ trap の規則になる。
- 状態: 既定案（実装者はこの案に従う）
