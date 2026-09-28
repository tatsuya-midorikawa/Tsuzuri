# A08: char（UTF-16 コード単位）型と utf8char（Unicode スカラー）型

> 現行の [文字列仕様](../docs/language.md#string-と-utf8string) に合わせ、
> string 向けの char と utf8string 向けの utf8char を別の型として追加します。
> string の索引・列挙（i16u）と utf8string の索引・列挙（ubyte）は変更しません。

| 項目 | 内容 |
| --- | --- |
| ID | A08 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02, (B01) |
| 後続 | D02 |
| 状態 | done |
| 主な影響ファイル（実装時） | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/numeric.rs`, `src/main.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/types_ownership.rs`, `tests/control.rs`, `tests/primitives.mjs`, `tests/strings.rs`, `tests/strings.mjs`, `tests/fixtures/*`, `std/Char.tz`, `std/Utf8Char.tz` または E02 の builtin std 登録 |

## 目的

現行の `string`（UTF-16）と互換性を持つ `char` 型を追加する。`char` は UTF-16 の1コード単位で、孤立サロゲートを含む全16-bit値を保持する。`utf8string`（妥当な UTF-8）向けには、単一 Unicode スカラー値を表す別の型 `utf8char` を追加する。`utf8char` は UTF-8 の1バイトではなく、符号化すると1～4バイトになる値である。

A08 の core は、両型の値、リテラル、Copy/Eq/Ord、ASCII 分類、整数との明示的な変換を提供することに限定する。`Display`/`Parse` は D01 がある場合だけ、`Hash`/`Default` は A07 がある場合だけ追加する。既存の索引・列挙は `string` が `i16u`、`utf8string` が `ubyte` のまま維持する。

LLVM 表現は `char` が `i16`、`utf8char` が `i32`。どちらも算術型ではなく、整数変換は `as` ではなく `Char`／`Utf8Char` モジュール関数で行う。GUIDE §9 D-12 の旧定義との相違は「未決事項」に記録する。

## 現状

- 現行の `string` は孤立サロゲートも保持する UTF-16 コード単位列、`utf8string` は妥当な UTF-8 バイト列で、暗黙変換はない。
- `src/lexer.rs` の `'` は型変数の開始に使われる。`'a` を維持しつつ `'a'` と `u8'a'` を区別する必要がある。
- `src/syntax.rs`、`src/check.rs`、`src/numeric.rs::primitive` に両文字型のリテラル・型を追加する必要がある。
- `src/polymorph.rs::Classes::intrinsic` に両型の Copy/Eq/Ord 等を追加する。文字列の既存インスタンスは変更しない。
- `src/control.rs::pattern_alternatives` は literal pattern を `Eq` constraint と `Binary(Equal)` へ下げる。両文字型も同じ仕組みを使う。
- `src/llvm_control.rs::switch_cases`、`src/llvm.rs::llvm_type`、`console_main` は両型への対応が必要。
- 実装着手時に各関数の現状と依存チケットの完了状況を再確認する。以下は実装予定の契約であり、実装済みの仕様ではない。

## 仕様

### 型

```text
char ::= UTF-16 code unit
range ::= 0x0000..0xFFFF

utf8char ::= Unicode scalar value
range ::= U+0000..U+D7FF | U+E000..U+10FFFF
```

- `char` は `string` の各コード単位を損失なく保持し、`0xD800..0xDFFF` も有効。補助平面のスカラーはサロゲートペア、すなわち2個の `char` に対応し、1個の `char` には収まらない。
- `utf8char` の値域は Rust/Unicode の scalar value と同じで、surrogate `U+D800..U+DFFF` を含まない。UTF-8 バイトや書記素クラスタとは別物。
- LLVM 表現は `char` が `i16`、`utf8char` が `i32`。`utf8char` は値域チェック済みで、未使用の上位ビットは常に 0。
- 両型は A08 core で Copy, Eq, Ord。`char` の Ord は符号なしコード単位順、`utf8char` はスカラー値順。同じ型同士だけ比較できる。
- D01 が実装済みなら両型を `Display`/`Parse` 対象に、A07 が実装済みなら `Hash`/`Default` 対象に加える。Default はそれぞれ `'\0'`／`u8'\0'`。
- 両型は Numeric ではなく、整数型の別名でもない。整数との `as`、両型間の `as`、算術演算は `E1005` または `E1003` で拒否する。
- 型注釈によるリテラルの切り替え、両型間や文字列との暗黙変換は導入しない。
- 両型とも export ABI には含めない。結果または引数が `char`／`utf8char` の `export def` は `E1008`。

### 字句・リテラル

```text
char-literal ::=
    "'" char-body "'"

utf8char-literal ::=
    "u8'" utf8char-body "'"

char-body ::=
    BMP-scalar
  | common-escape
  | "\\u" hex-digit{4}
  | "\\u{" hex-digit{1,6} "}"

utf8char-body ::=
    Unicode-scalar
  | common-escape
  | "\\u{" hex-digit{1,6} "}"

common-escape ::=
    "\\" ("'" | "\\" | "n" | "r" | "t" | "0")
```

直接記述する scalar は quote、backslash、改行（CR/LF）を除く。`char` の Unicode escape は `0xFFFF` 以下のコード単位を受理し、surrogate も許す。`utf8char` の Unicode escape は妥当なスカラーだけを受理する。4桁の `\uXXXX` は現行 `string` と同じく `char` だけが対応する。

有効例（型はリテラルの表記で決まり、型注釈では変わらない）:

```text
'a'
'あ'
'\n'
'\''
'\\'
'\uD800'
'\u{DFFF}'

u8'a'
u8'あ'
u8'\n'
u8'\''
u8'\\'
u8'\u{1F600}'
```

字句解析の disambiguation:

1. `Lexer::tokens` が隣接した `u8'` を見たら utf8char literal を読む。妥当な scalar 1 個または escape 1 個と閉じる `'` があれば `TokenKind::Utf8Char(u32)`、それ以外は `E0001`。型変数にはフォールバックしない。
2. `'` を見たら、まず通常 scalar 1 個または escape 1 個と、その直後の閉じる `'` を試す。1 UTF-16 コード単位で表せれば `TokenKind::Char(u16)`、補助平面など範囲外なら `E0001`。サロゲートペアへの自動分割や切り捨てはしない。
3. この形に一致しなければ、開始 `'` の直後から ASCII type-variable identifier（ASCII alphabetic で始まり ASCII alphanumeric/`_` が続く）を消費する。消費できなければ `E0001`。
4. その identifier の直後に `'` が **即座に** 続く場合だけ、`'ab'` のような malformed char として `E0001 "a char literal contains exactly one UTF-16 code unit"` を出す。空白・コメント・別 token を越えて後方の quote を探さない。
5. 直後に quote がなければ `TokenKind::TypeVariable(identifier)` を返す。
6. 空、複数要素、不正 escape、改行を含むリテラルは両型とも `E0001`。`utf8char` の複数要素の診断は `"a utf8char literal contains exactly one Unicode scalar"`。
7. `u8` 単独や通常の識別子、既存の `u8"..."` の字句解析は変更しない。

この規則により:

| 入力 | token |
| --- | --- |
| `'a` | `TypeVariable("a")` |
| `'a'` | `Char(0x61)` |
| `u8'a'` | `Utf8Char(0x61)` |
| `'abc` | `TypeVariable("abc")` |
| `'ab'` | `TypeVariable("ab")` 後の stray quote ではなく、char literal の誤りとして `E0001` |
| `'\u{D800}'` | `Char(0xD800)` |
| `'\u{1F600}'` | `E0001`（2 UTF-16 コード単位が必要） |
| `u8'\u{1F600}'` | `Utf8Char(0x1F600)` |
| `u8'\u{D800}'` | `E0001`（スカラーではない） |

`'a -> 'b` のような既存の型変数列を壊さず、離れた quote を文字リテラルの終端として扱わない。

### 標準関数

E02 の std module 機構で `Char` と `Utf8Char` モジュールを提供し、両方の名前空間を予約する。実装形態は std source でも builtin でもよいが、呼び出しは常に `Char.name`／`Utf8Char.name`。

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `Char.to_u16` | `char -> i16u` | UTF-16 コード単位を返す。surrogate もそのまま |
| `Char.of_u16` | `i16u -> char` | 全16-bit値で成功。surrogate を拒否せず、trap も Option も不要 |
| `Utf8Char.to_u32` | `utf8char -> i32u` | scalar value を返す |
| `Utf8Char.of_u32_unchecked` | `i32u -> utf8char` | valid scalar なら utf8char、範囲外/surrogate は trap |
| `Utf8Char.of_u32` | `i32u -> Option<utf8char>` | B01 完了後。invalid は `None` |

| 関数名 | Char の型 | Utf8Char の型 | 意味 |
| --- | --- | --- | --- |
| `is_ascii_digit` | `char -> bool` | `utf8char -> bool` | `0`..`9` |
| `is_ascii_alphabetic` | `char -> bool` | `utf8char -> bool` | `A`..`Z` or `a`..`z` |
| `is_ascii_lower` | `char -> bool` | `utf8char -> bool` | `a`..`z` |
| `is_ascii_upper` | `char -> bool` | `utf8char -> bool` | `A`..`Z` |
| `to_ascii_upper` | `char -> char` | `utf8char -> utf8char` | ASCII lower だけ upper、その他 unchanged |
| `to_ascii_lower` | `char -> char` | `utf8char -> utf8char` | ASCII upper だけ lower、その他 unchanged |

非 ASCII の分類は false、大小変換は元の値を返す。`char` の surrogate も保持し、分類や大小変換だけで trap しない。

B01 が未完了の環境では `Utf8Char.of_u32` は提供せず、`Utf8Char.of_u32_unchecked` だけを提供する。`Char.of_u16` は全域関数なので B01 に依存しない。旧案の scalar 用 `Char.to_u32`／`of_u32_unchecked`／`of_u32` は `Utf8Char` 側へ移す。

### パターン・match

- 両型のリテラルは constant pattern として使える。
- `match c with | 'a' -> ... | '\n' -> ... | _ -> ...` は `Eq<char>`、`u8'a'` 等のパターンは `Eq<utf8char>` による通常比較。両型の混在は型エラー。
- `llvm_control::switch_cases` は `Type::Char` を `i16`、`Type::Utf8Char` を `i32` switch/table 化対象に加える。
- 範囲・文字クラス pattern は導入しない。

### コンソールと表示クラス

- `console_main` は entry result `char`／`utf8char` を UTF-8 に encode して改行付きで出力する。NUL も保持し、`unit` と違い出力する。
- `char` が surrogate なら、現行 `string` のコンソール出力と同じく trap。単独のコード単位を勝手に補完せず、replacement character に置換しない。値として保持できることと UTF-8 に出力できることは別の契約。
- `utf8char` は常に妥当な scalar なので1～4バイトで出力できる。
- D01 が既に実装済みなら、両型の `Display`/`Parse` を同時に登録する。D01 が未実装なら console output と両モジュールだけを実装する。
- `Display<char>` は quote しない1コード単位の **string（UTF-16）** を返し、surrogate も損失なく保持する。`Display<utf8char>` もクラスの戻り値は string のままで、BMP は1コード単位、補助平面はサロゲートペアの2コード単位へ符号化する。derived Display の quote は A07 側。
- `Parse<char>` は入力 string がちょうど1コード単位なら surrogate も含め `Some`、それ以外は `None`。
- `Parse<utf8char>` は入力 string がちょうど1スカラー（非 surrogate の1コード単位、または妥当なサロゲートペア）なら `Some`。空、孤立サロゲート、複数スカラーは `None`。文字向け Parse は数値向けの ASCII 限定処理を流用しない。
- Parse は空白を除去せず、quote や escape を解釈しない。両型とも `Parse(Display(value))` で元の値へ戻る。

### 文字列との関係

| 契約 | string / char | utf8string / utf8char |
| --- | --- | --- |
| 文字列の値モデル | UTF-16 コード単位列。孤立サロゲート可 | 妥当な UTF-8 バイト列 |
| 文字型との対応 | 1コード単位が1 char。補助平面は2 char | 1スカラーが1 utf8char、符号化すると1～4バイト |
| `.length : i64` | 引き続き UTF-16 コード単位数 | 引き続き UTF-8 バイト数 |
| `text[index]` | 引き続き `i16u` | 引き続き `ubyte` |
| `for value in text` | 引き続き `i16u` のコード単位列挙 | 引き続き `ubyte` のバイト列挙 |

`Char.of_u16 text[index]` で string の索引結果を損失なく char にでき、`Char.to_u16` で元のコード単位に戻せる。一方、utf8string の任意の1バイトは utf8char ではない。スカラーの取得には UTF-8 の復号が必要で、バイトを暗黙に utf8char として扱わない。

文字型を返す索引・列挙 API、両文字型間の符号化変換やサロゲートペアの分解・合成 API、grapheme cluster、Unicode case folding/full classification は D02 以降。A08 は既存の文字列操作の型・単位を変更しない。

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub enum TokenKind {
    // ...
  Char(u16),
  Utf8Char(u32),
}

pub enum ExprKind {
    // ...
  Char(u16),
  Utf8Char(u32),
}
```

`PatternKind` は literal expression を使うため追加不要。

`src/check.rs`:

```rust
pub enum Type {
    // ...
    Char,
  Utf8Char,
}

pub enum TypedExprKind {
    // ...
  Char(u16),
  Utf8Char(u32),
}
```

`Type` methods:

| method | 変更 |
| --- | --- |
| `display` | `"char"`／`"utf8char"` |
| `is_scalar` | 両型を含める。ただし `is_numeric` は含めない。ここでの scalar はコンパイラの値分類であり Unicode scalar の保証ではない |
| `is_copy` | true |
| `needs_drop` | false |
| `contains_reference`/`carries_loans` | false |
| `can_capture`/`can_send` | true |
| `exportable` | false |

`src/numeric.rs::primitive` は名前が数値に限らない実装なので、`"char" => Type::Char`、`"utf8char" => Type::Utf8Char` を追加する。どちらも `NUMERIC_NAMES` には追加しない。

### Lexer

`Lexer::tokens` の `'` branch を `char_or_type_variable(start)` helper へ置き換える。隣接した `u8'` は既存の `u8"` と同様に識別子の走査より先に認識し、utf8char literal 専用の経路へ渡す。

```rust
fn char_or_type_variable(&mut self, start: usize) -> Result<TokenKind, Diagnostic>
```

helper は char literal candidate を読むときに `self.position` を一時変数で進め、成功時だけ commit する。リテラルの形に一致しなければ同じ開始位置から ASCII type-variable identifier を読む。identifier の直後が quote なら malformed char として `E0001`、直後が quote でなければ type variable として commit する。空白・コメント・別 token を越えて後方の quote を scan しない。リテラルの形に一致した後の範囲違反は型変数に読み替えず診断する。

escape の16進数読み取りは既存 string／utf8string と共有できるが、値域検査は分ける。`char` は `u16` の全域を許し、Rust の `char::from_u32` で検査してはいけない。`utf8char` は `char::from_u32` 相当の scalar 検査を使う。string 用の補助平面から2コード単位への展開を char literal に流用しない。

### Parser wiring

両文字型の literal token を追加したら、少なくとも次の parser 分岐を更新する。

| ファイル + 関数 | 変更 |
| --- | --- |
| `src/parser.rs::Parser::primary` | `TokenKind::Char(_)`／`Utf8Char(_)` を既存の literal token と同じく `literal()` に渡す |
| `src/parser.rs::Parser::space_argument` | 空白適用の開始 token に両型を追加 |
| `src/parser.rs::Parser::literal` | `TokenKind::Char(value) => ExprKind::Char(value)`、Utf8Char も同様 |
| `src/parse_control.rs::pattern_atom` | literal pattern の token set に両型を追加 |
| `src/parse_control.rs::named_pattern` | active pattern の argument/payload pattern 開始 token set に両型を追加 |
| `src/parse_control.rs::guard_predicate` | 関数ガード predicate 判定で literal 開始 token に両型を追加 |

### 型検査

- `Checker::value_expression` に `ExprKind::Char(value) => (TypedExprKind::Char(value), Type::Char)` と Utf8Char の対応を追加。
- `Checker::Cast` は既存 `Numeric` requirement により両文字型の casts を拒否する。相互の暗黙変換・型注釈による切り替えも拒否する。
- `Classes::intrinsic`:
  - `Eq`: 既存の `is_scalar` 条件に両型を含める。
  - `Ord`: 既存の対象に `Type::Char` と `Type::Utf8Char` を追加。string の Ord 等は維持する。
  - `Copy`, `Capture`, `Send` は既存 Type method で true。
  - `Display`/`Parse` は D01 実装時だけ両型を含め、コード単位とスカラーの違いを保持する。
  - `Hash`/`Default` は A07 実装時だけ両型を含める。char の surrogate も有効な値として扱う。

### LLVM

`llvm_type(Type::Char)` は `i16`、`llvm_type(Type::Utf8Char)` は `i32`。

`FunctionEmitter::expression_mode`:

```rust
TypedExprKind::Char(value) => value.to_string(),
TypedExprKind::Utf8Char(value) => value.to_string(),
```

`FunctionEmitter::binary` は両型の Eq/Ord を integer と同じ `icmp` で下げる。型ごとの幅で符号なし順序として扱う:

| operator | char | utf8char |
| --- | --- | --- |
| `==`/`!=` | `icmp eq/ne i16` | `icmp eq/ne i32` |
| `< <= > >=` | `icmp ult/ule/ugt/uge i16` | `icmp ult/ule/ugt/uge i32` |

`Char.to_u16`／`Char.of_u16` は同じ `i16` 値をそのまま使い、surrogate 検査や trap を入れない。

`Utf8Char.of_u32_unchecked` lowering:

```llvm
%le_max = icmp ule i32 %x, 1114111
%sur_hi = icmp uge i32 %x, 55296
%sur_lo = icmp ule i32 %x, 57343
%is_sur = and i1 %sur_hi, %sur_lo
%not_sur = xor i1 %is_sur, true
%valid = and i1 %le_max, %not_sur
br i1 %valid, label %ok, label %trap
```

`Utf8Char.of_u32` は同じ条件で `Some`／`None` を選び、invalid 入力で trap しない。

`console_main` の両文字型:

- `validate_main` は `Type::Char`／`Type::Utf8Char` を許可する。
- `console_main` の `buffered = matches!(...)` に両型を追加し、`runtime/console.ll` の `@tz.console.write` が重複なく連結されるようにする。
- entry-type 診断文は `"number, bool, char, utf8char, unit, string, or utf8string"` に更新し、既存の文字列型も維持する。
- char の entry `i16 %result` は surrogate なら trap、それ以外は `i32` へゼロ拡張する。utf8char の entry `i32 %result` は scalar invariant を利用する。
- 検査済み scalar を UTF-8 bytes へ encode:
  - `< 0x80`: 1 byte
  - `< 0x800`: 2 bytes
  - `< 0x10000`: 3 bytes
  - otherwise: 4 bytes
- entry block に最大4バイト分と newline `\0A` の5 byte buffer を置き、実際の長さで `@tz.console.write(ptr, i64)` を呼ぶ。char も utf8char も同じ出力経路を使う。
- native console only。WASM library/exe に import は追加しない。

### 変更ステージ

| ステージ | 変更 |
| --- | --- |
| lexer | `TokenKind::Char`／`Utf8Char`、型変数との区別、符号化ごとの escape 検査 |
| parser | 両型の literal。`Parser::primary`, `space_argument`, `literal`, `parse_control.rs::pattern_atom`, `named_pattern`, `guard_predicate` を更新 |
| syntax | `TokenKind`／`ExprKind` に `Char(u16)`／`Utf8Char(u32)` |
| computation::expand | 両型は子なし。match に追加 |
| check | `Type`／`TypedExprKind` の両型、Type methods、primitive name、value typing |
| polymorph | 両型の Eq/Ord/Copy/Capture/Send hooks。D01/A07 がある場合だけ Display/Parse/Hash/Default hooks |
| control | 既存の literal pattern の仕組みで両型を扱う |
| closures / recursion / ownership | 両型は子なし・Copy。children/walk 系に漏れがないか確認 |
| call_specialization | 変更なし |
| llvm | `i16`／`i32`、constants、comparisons、Char／Utf8Char functions、console output |
| llvm_control | `switch_cases` と `constant_result` に両型。char は `i16`、utf8char は `i32` |
| llvm_frame | 既存の scalar 配置規則に従い、両型の幅・load/store・コレクション要素の配置を確認 |
| runtime | 既存の console write と文字列の符号化処理を再利用。ホスト Unicode API は追加しない |
| driver/main | help/docs の entry result に両型を追加 |

### 前提とする他チケットのインターフェース

- E02: `Char`／`Utf8Char` module を標準ライブラリまたは builtin namespace として予約し、`Char.to_u16`／`Utf8Char.to_u32` 形式で解決できる。
- B01: `Option<utf8char>` が利用可能なら `Utf8Char.of_u32` を追加する。B01 未完了では unchecked 版のみ。`Char.of_u16` は B01 に依存しない。
- D01: 完了済みなら両型の `Display`/`Parse` を登録する。Display の戻り値・Parse の入力は現行の UTF-16 string のままで、utf8string へ変更しない。

### 他チケットへの提供インターフェース

- D02 は string のコード単位に `char`／`Char.to_u16`、utf8string の復号済み scalar に `utf8char`／`Utf8Char.to_u32` を使える。A08 では既存の索引・`for in` を変えない。
- A07 は両型を derived Display の quoted literal、`Hash`/`Default` の primitive component として扱える。char の surrogate は escape で表し、utf8char は `u8'...'` と区別する。

## 実装手順

1. **字句・構文**: `TokenKind::Char(u16)`／`Utf8Char(u32)`、lexer helper、Parser wiring の各分岐を追加する。型変数、両リテラル、escape、char の surrogate 受理・補助平面拒否、utf8char の補助平面受理・surrogate 拒否を確認する。
2. **型検査**: `Type`／`ExprKind`／`TypedExprKind` に両型を追加し、Type methods と `numeric::primitive` を更新する。子なし variant の children/walk、整数 cast・算術・混在比較・暗黙変換・export の拒否を確認する。
3. **型クラス**: 両型の Eq/Ord/Copy/Capture/Send intrinsic を追加する。D01 があれば Display/Parse、A07 があれば Hash/Default も追加する。等値・符号なし順序と、D01 ありなら surrogate／サロゲートペアも含む Display/Parse の往復を確認する。
4. **Char / Utf8Char modules**: E02 の方式で整数変換と ASCII helpers を登録し、B01 がある場合だけ `Utf8Char.of_u32` を追加する。`Char.of_u16` の全域性、invalid scalar の unchecked 版による native/WASM trap、safe 版の None を確認する。
5. **LLVM**: `llvm_type`、`expression_mode`、comparisons、`console_main`、`llvm_control::switch_cases` を更新する。char は `switch i16`／定数 `i16 97`、utf8char は `switch i32`／定数 `i32 97`、WASM imports 空、char console の surrogate trap を確認する。
6. **E2E/docs**: fixtures と docs を追加し、既存の UTF-16 string／UTF-8 utf8string の索引・列挙・長さを回帰検査する。native/WASM × `-O0`/`-O3`、IR 決定性、runtime 連結の重複なしを確認する。

## テスト計画

### 受理

```text
def newline :: char = '\n'

def surrogate :: char = '\uD800'

def smile :: utf8char = u8'\u{1F600}'

def compare :: bool = 'A' < 'a' && 'a' == '\u{61}'

def compare_utf8 :: bool = u8'A' < u8'a' && u8'a' == u8'\u{61}'
```

```text
def classify :: char -> i64 = \c ->
    match c with
    | '0' -> 0
    | 'A' -> 1
    | '\n' -> 2
    | '\uD800' -> 3
    | _ -> 4

  def classify_utf8 :: utf8char -> i64 = \c ->
    match c with
    | u8'A' -> 1
    | u8'\u{1F600}' -> 2
    | _ -> 0
```

```text
def upper :: char -> char = \c -> Char.to_ascii_upper c

def upper_utf8 :: utf8char -> utf8char = \c -> Utf8Char.to_ascii_upper c

def surrogate_code :: i16u = Char.to_u16 (Char.of_u16 55296i16u)
```

### 拒否

| 入力 | 期待 |
| --- | --- |
| `''`、`u8''` | `E0001` |
| `'ab'`、`u8'ab'` | `E0001` |
| `'\u{}'`、`u8'\u{}'` | `E0001` |
| `'\u{1F600}'`、`'\uD83D\uDE00'` | `E0001`（char は1コード単位のみ） |
| `u8'\u{D800}'`、`u8'\u{DFFF}'` | `E0001`（utf8char は surrogate 不可） |
| `u8'\u0041'` | `E0001`（utf8char は braced Unicode escape のみ） |
| `'\u{110000}'`、`u8'\u{110000}'` | `E0001` |
| 型変数として成立しない未閉鎖リテラル、不正 escape、リテラル内の改行 | `E0001` |
| `def f :: i16u\nfn f = 'a' as i16u` | `E1005` |
| `def f :: char\nfn f = 65i16u as char` | `E1005` |
| `def f :: i32u\nfn f = u8'a' as i32u` | `E1005` |
| `def f :: utf8char\nfn f = 65i32u as utf8char` | `E1005` |
| `def f :: utf8char\nfn f = 'a' as utf8char` | `E1005` または `E1003` |
| `def f :: char\nfn f = u8'a'`、逆方向の型注釈 | `E1003` |
| `'a' == u8'a'`、char の match に utf8char pattern | `E1003` |
| `export def f :: char\nfn f = 'a'`、utf8char の結果、両型の引数 | `E1008` |
| `'a' + 'b'`、`u8'a' + u8'b'` | `E1005` |

### E2E

GUIDE §3 に従い、Node E2E の直前に必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

- `export def code_a :: i16u` returns `Char.to_u16 'A'` -> 65。utf8char 版は `i32u` で `Utf8Char.to_u32 u8'A'` -> 65。
- 両モジュールの ASCII 分類・大小変換を整数チェックサムで検査。非 ASCII、char の surrogate、utf8char の補助平面が変わらないことも確認する。
- char は `Char.of_u16`／`to_u16` を全65536コード単位で往復させ、JS の UTF-16 コード単位を参照値にする。match の surrogate 分岐と符号なし順序も確認する。
- utf8char は `0`、`0x7F`、`0x80`、`0x7FF`、`0x800`、`0xD7FF`、`0xE000`、`0xFFFF`、`0x10000`、`0x10FFFF` を受理し、`0xD800`、`0xDFFF`、`0x110000`、`0xFFFFFFFF` を拒否する。JS reference は scalar range を検査する。
- invalid scalar の `Utf8Char.of_u32_unchecked` は native 子プロセスで異常終了、WASM で `WebAssembly.RuntimeError`。B01 ありなら safe 版は同じ入力で None。char の surrogate 構築は trap しない。
- Native console fixture の `'あ'` と `u8'あ'` はともに UTF-8 `E3 81 82 0A`、`u8'\u{1F600}'` は `F0 9F 98 80 0A`。NUL を含め符号化長の境界もバイト列で照合する。
- Native console の `'\uD800'`／`'\uDFFF'` は現行 string と同じく trap し、置換文字を出さない。
- Native console の LLVM IR は両型が `buffered` path に入り、`runtime/console.ll` 由来の `@tz.console.write` の連結経路をちょうど1系統だけ含むことを検査する。
- D01 ありなら `Display<char>` は surrogate を保持し Parse で戻る。補助平面の string に対して `Parse<char>` は None、`Parse<utf8char>` は Some。後者は孤立サロゲート、空、複数スカラーで None。
- 既存の `"\u{1F600}"` は長さ2・索引/列挙 `i16u`、`u8"\u{1F600}"` は長さ4・索引/列挙 `ubyte` のまま。既存の string／utf8string テストも維持する。
- native/WASM × `-O0`/`-O3`。IR は決定的、char の `switch i16` と utf8char の `switch i32` を検査し、WASM imports は空。

## ドキュメント

以下の言語仕様・設計台帳・実装済み一覧を両文字型の実装に同期済み。

- `docs/language.md`
  - 型表に char（UTF-16 コード単位）と utf8char（Unicode スカラー）。
  - `'...'`／`u8'...'`、escape の値域、type variable disambiguation。
  - string の索引・列挙は `i16u`、utf8string は `ubyte` のままであること。
  - `Char`／`Utf8Char` module API、後者の `of_u32` の B01 依存、Utf8Char 名前空間の予約。
  - Display/Parse とコンソールでの surrogate の扱い、暗黙変換・export ABI 非対応。
- `docs/architecture.md`
  - `Type::Char` は LLVM `i16` で全ビットパターン有効、`Type::Utf8Char` は LLVM `i32` で valid scalar invariant を保証。
  - literal と変換の型別検査、char console の surrogate guard、両型の switch/table lowering。
  - WASM imports なし。
- `README.md`
  - 実装完了後に両型を実装済み型一覧と console result に追加。
- `_features/GUIDE.md`／`_features/README.md` と関連チケット
  - GUIDE §9 D-12、D-07 のモジュール一覧、D-11／D-15／D-20 の文字型参照と A08 の一覧を本設計へ同期。
  - D01／D02／A07 の char を単一 Unicode スカラーとみなす箇所を、用途に応じて char／utf8char に分ける。

## 受け入れ条件

- [x] char は全 UTF-16 コード単位を保持し、utf8char は妥当な Unicode スカラーだけを保持する別の型。
- [x] `'a'`／`u8'a'`、各型の escapes、Unicode リテラルが lex/parse/typecheck できる。
- [x] `'a` type variable は従来通り動く。
- [x] char の surrogate literal を受理し、補助平面や複数コード単位は `E0001`。
- [x] utf8char の補助平面 literal を受理し、surrogate・範囲外・複数スカラーは `E0001`。
- [x] 両型は A08 core で Copy/Eq/Ord、Numeric ではなく、相互の暗黙変換・cast・export ABI は不可。
- [x] D01 の両型の `Display`/`Parse` を実装し、UTF-16 string を使って値が往復する。
- [x] A07 未実装のため `Hash`/`Default` はまだ追加しない。
- [x] `Char.to_u16`／`of_u16` は全コード単位を往復し、`Utf8Char.to_u32`／`of_u32_unchecked` は scalar invariant を保証する。
- [x] 両型の ASCII helpers が動き、非 ASCII や char の surrogate を壊さない。
- [x] B01 の `Option` を使う `Utf8Char.of_u32` と、全域の `Char.of_u16` を提供する。
- [x] char は `switch i16`、utf8char は `switch i32` に lower されるケースを持つ。
- [x] native console は UTF-8 で両型を出力し、char の surrogate では trap。WASM imports は増やさない。
- [x] `console_main` の entry-type 診断文が両型と既存文字列型を含み、console IR が `@tz.console.write` 連結経路を重複させない。
- [x] string／utf8string の既存の長さ・索引・列挙の単位と型を変更しない。
- [x] native/WASM × `-O0`/`-O3`、IR 決定性。

## 落とし穴

- 両型を `NUMERIC_NAMES` に入れない。`as` cast や Numeric constraint が誤って通る。
- `Type::is_scalar` に入れる場合、`exportable` は別に false にする。
- char を Rust の `char` や Unicode scalar と同一視しない。`'\u{D800}'` は有効な char だが、`u8'\u{D800}'` は無効。
- utf8char を `ubyte` と同一視しない。UTF-8 の途中のバイトは単独のスカラーではない。
- `for in string` を char／scalar iteration に、`for in utf8string` を utf8char iteration に変えない。既存コードの型・コード単位／バイトの意味を破壊する。
- char の Display が surrogate を保持することと、コンソールがそれを trap することを混同しない。暗黙の U+FFFD 置換は禁止。
- 両型の console output に `printf("%c")` を使うと非 ASCII が壊れる。必ず検査・UTF-8 encode して `@tz.console.write`。
- WASM 用にホストの Unicode API を import しない。

## 対象外

- Unicode grapheme cluster、正規化、full case folding、locale-aware classification。
- string／utf8string の文字型を返す indexing/iteration と、両文字型間の符号化変換・サロゲートペア操作 API。D02。
- 両文字型の ranges/pattern classes。
- 両文字型の export ABI。E05 までは公開しない。
- 両文字型と整数、char と utf8char の `as` cast、暗黙変換。

## 未決事項

- **解決（2026-09-27）:** GUIDE D-12 とモジュール一覧を両文字型へ同期し、実装済み。型付き定数は既存の `TypedExprKind::Int` を再利用して型で区別する。`tests/chars.rs` と `tests/features.mjs ... chars` が全コード単位、スカラー境界、Display/Parse、console、IR を検証する。
- **既定案: utf8char literal は `u8'...'`。** 現行の `u8"..."` と対応させ、型注釈による切り替えは導入しない。
- **既定案: B01 未完了では Utf8Char の整数入力は unchecked 版のみ。** `Option` を独自に作らず、B01 完了後に safe API を追加する。`Char.of_u16` は全域関数。
- **既定案: 両型を `is_scalar` に含めるが `is_numeric` には含めない。** Eq intrinsic の既存条件を再利用しつつ Numeric を拒否する。
- **既定案: console は両型を表示対象に含める。** char の surrogate は現行 string と同じく trap。export ABI は変更しない。
