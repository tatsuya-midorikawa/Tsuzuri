# A08: char（Unicode スカラー）型

> この提案の UTF-8 string に関する記述は旧仕様です。
> 現行の [文字列仕様](../docs/language.md#string-と-utf8string) では string は孤立サロゲートも保持する UTF-16、
> utf8string は妥当な UTF-8 です。char を追加する際はスカラー値とコード単位を区別し、
> string の索引・列挙（i16u）と utf8string の索引・列挙（ubyte）を暗黙に変更しないでください。

| 項目 | 内容 |
|---|---|
| ID | A08 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02, (B01) |
| 後続 | D02 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/numeric.rs`, `src/main.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/types_ownership.rs`, `tests/control.rs`, `tests/primitives.mjs`, `tests/fixtures/*`, `std/Char.tz` または E02 の builtin std 登録 |

## 目的

Unicode スカラー値を表す `char` 型を追加する。文字列は引き続き UTF-8 バイト列であり、`string[index]` と `for ... in string` は D02 まで ubyte 単位のまま維持する。A08 の core は、単一 Unicode スカラーの値型、リテラル、Copy/Eq/Ord、ASCII 分類、整数変換を提供することに限定する。`Display`/`Parse` は D01 がある場合だけ、`Hash`/`Default` は A07 がある場合だけ追加する。

GUIDE §9 D-12 に従い、LLVM 表現は `i32`、算術型ではなく、整数変換は `as` ではなく `Char` モジュール関数で行う。

## 現状

- `src/lexer.rs` は `'` を常に `TokenKind::TypeVariable` の開始として扱い、`'a'` は `TypeVariable("a")` の後に余分な `'` として字句エラーになる。
- `src/syntax.rs` の `TokenKind`/`ExprKind`/`PatternKind` に char literal はない。
- `src/numeric.rs::primitive` は `bool`, `unit`, `string`, 数値だけを返す。
- `src/check.rs::Type` に `Char` はない。`Type::is_scalar` は numeric または bool。
- `src/polymorph.rs::Classes::intrinsic` の `Eq` は scalar/string/unit、`Ord` は numeric のみ。`char` の Copy/Eq/Ord は未実装。
- `src/control.rs::pattern_alternatives` は literal pattern を `Eq` constraint と `Binary(Equal)` へ下げる。char literal が `TypedExprKind::Char` になれば同じ仕組みを使える。
- `src/llvm_control.rs::switch_cases` は `Integer`, `Bool`, `Unit` だけを switch/table 化する。char を加える必要がある。
- `src/llvm.rs::llvm_type` は char がない。`console_main` は数値/bool/string/unit と software numeric だけを出力する。

## 仕様

### 型

```text
char ::= Unicode scalar value
range ::= U+0000..U+D7FF | U+E000..U+10FFFF
```

- 値域は Rust/Unicode の scalar value と同じで surrogate `U+D800..U+DFFF` を含まない。
- LLVM 表現は `i32`。上位ビットは常に 0、値域チェック済み。
- `char` は A08 core で Copy, Eq, Ord。D01 が実装済みなら D01 側の `Display`/`Parse` 対象に加える。A07 が実装済みなら A07 側の `Hash`/`Default` 対象に加える。
- `char` は Numeric ではない。`'a' as i32u`、`65i32u as char` は `E1005` または `E1003` で拒否する。
- export ABI には含めない。`export def f :: char` または `char` 引数は `E1008`。

### 字句・リテラル

```text
char-literal ::=
    "'" char-body "'"

char-body ::=
    non-quote-non-backslash-non-newline Unicode scalar
  | "\\" ("'" | "\\" | "n" | "r" | "t" | "0")
  | "\\u{" hex-digit{1,6} "}"
```

有効例:

```text
'a'
'あ'
'\n'
'\''
'\\'
'\u{1F600}'
```

字句解析の disambiguation:

1. `Lexer::tokens` が `'` を見たら、まず char literal として読めるか試す。
2. 開始 `'` の直後から、通常 scalar 1 個または escape 1 個を読む。
3. その直後に閉じる `'` があれば `TokenKind::Char(u32)` を返す。
4. one-scalar form に失敗したら、開始 `'` の直後から ASCII type-variable identifier（ASCII alphabetic で始まり ASCII alphanumeric/`_` が続く）を消費する。消費できなければ `E0001`。
5. その identifier の直後に `'` が **即座に** 続く場合だけ、`'ab'` のような malformed multi-scalar char として `E0001 "a char literal contains exactly one Unicode scalar"` を出す。空白や別 token を越えて後方の quote を探さない。
6. 直後に quote がなければ `TokenKind::TypeVariable(identifier)` を返す。
7. `''`、`'ab'`、`'\u{}'`、`'\u{D800}'`、`'\u{110000}'`、改行を含む char literal は `E0001`。

この規則により:

| 入力 | token |
|---|---|
| `'a` | `TypeVariable("a")` |
| `'a'` | `Char(0x61)` |
| `'abc` | `TypeVariable("abc")` |
| `'ab'` | `TypeVariable("ab")` 後の stray quote ではなく、char literal として試した結果 `E0001` にする |

最後の `"'ab'"` は利用者の意図が char literal の誤りである可能性が高いため、分かりやすい `E0001 "a char literal contains exactly one Unicode scalar"` を出す。

### 標準関数

E02 の std module 機構で `Char` モジュールを追加する。実装形態は std source でも builtin でもよいが、呼び出しは常に `Char.name`。

| 関数 | 型 | 意味 |
|---|---|---|
| `Char.to_u32` | `char -> i32u` | scalar value を返す |
| `Char.of_u32_unchecked` | `i32u -> char` | valid scalar なら char、範囲外/surrogate は trap |
| `Char.of_u32` | `i32u -> Option<char>` | B01 完了後。invalid は `None` |
| `Char.is_ascii_digit` | `char -> bool` | `0`..`9` |
| `Char.is_ascii_alphabetic` | `char -> bool` | `A`..`Z` or `a`..`z` |
| `Char.is_ascii_lower` | `char -> bool` | `a`..`z` |
| `Char.is_ascii_upper` | `char -> bool` | `A`..`Z` |
| `Char.to_ascii_upper` | `char -> char` | ASCII lower だけ upper、その他 unchanged |
| `Char.to_ascii_lower` | `char -> char` | ASCII upper だけ lower、その他 unchanged |

B01 が未完了の環境では `Char.of_u32` は提供しない。`Char.of_u32_unchecked` だけを提供し、B01 実装時に safe 版を追加する。

### パターン・match

- char literal は constant pattern として使える。
- `match c with | 'a' -> ... | '\n' -> ... | _ -> ...` は `Eq<char>` による通常比較。
- `llvm_control::switch_cases` は `Type::Char` を `i32` switch/table 化対象に加える。
- 範囲・文字クラス pattern は導入しない。

### コンソールと表示クラス

- `console_main` は entry result `char` を UTF-8 に encode して改行付きで出力する。`unit` と違い出力する。
- D01 が既に実装済みのブランチで A08 を入れる場合は、D01 の `Display<char>`/`Parse<char>` を同時に登録する。D01 が未実装なら A08 は `Display`/`Parse` を追加せず、console output と `Char` module だけを実装する。
- D01 の `Display<char>` は同じ UTF-8 encoding を使い、quote しない単一文字の string を返す。derived Display の quote は A07 側。
- invalid scalar は型検査/変換で作れないため、LLVM 側で default replacement character に置換しない。

### 文字列との関係

- `string.length` は引き続き byte length。
- `text[index]` は引き続き `ubyte`。
- `for b in text do ...` は引き続き UTF-8 bytes。
- char iteration、grapheme cluster、Unicode case folding/full classification は D02 以降。

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub enum TokenKind {
    // ...
    Char(u32),
}

pub enum ExprKind {
    // ...
    Char(u32),
}
```

`PatternKind` は literal expression を使うため追加不要。

`src/check.rs`:

```rust
pub enum Type {
    // ...
    Char,
}

pub enum TypedExprKind {
    // ...
    Char(u32),
}
```

`Type` methods:

| method | 変更 |
|---|---|
| `display` | `"char"` |
| `is_scalar` | `char` を含める。ただし `is_numeric` は含めない |
| `is_copy` | true |
| `needs_drop` | false |
| `contains_reference`/`carries_loans` | false |
| `can_capture`/`can_send` | true |
| `exportable` | false |

`src/numeric.rs::primitive` は名前が数値に限らない実装なので、`"char" => Type::Char` を追加する。`NUMERIC_NAMES` には追加しない。

### Lexer

`Lexer::tokens` の `'` branch を `char_or_type_variable(start)` helper へ置き換える。

```rust
fn char_or_type_variable(&mut self, start: usize) -> Result<TokenKind, Diagnostic>
```

helper は char literal candidate を読むときに `self.position` を一時変数で進め、成功時だけ commit する。失敗したら同じ開始位置から ASCII type-variable identifier を読む。identifier の直後が quote なら malformed multi-scalar char として `E0001`、直後が quote でなければ type variable として commit する。空白・コメント・別 token を越えて後方の quote を scan しない。

既存 string escape の Unicode scalar 検査は `char::from_u32` を使っており surrogate を拒否できる。同じ helper を抽出して string/char で共有する。

### Parser wiring

char literal token を追加したら、少なくとも次の parser 分岐を更新する。

| ファイル + 関数 | 変更 |
|---|---|
| `src/parser.rs::Parser::primary` | `TokenKind::Char(_)` を `Integer`/`Float`/`String` と同じ literal token として `literal()` に渡す |
| `src/parser.rs::Parser::space_argument` | 空白適用の開始 token に `TokenKind::Char(_)` を追加 |
| `src/parser.rs::Parser::literal` | `TokenKind::Char(value) => ExprKind::Char(value)` |
| `src/parse_control.rs::pattern_atom` | literal pattern の token set に `TokenKind::Char(_)` を追加 |
| `src/parse_control.rs::named_pattern` | active pattern の argument/payload pattern 開始 token set に `TokenKind::Char(_)` を追加 |
| `src/parse_control.rs::guard_predicate` | 関数ガード predicate 判定で literal 開始 token に `TokenKind::Char(_)` を追加 |

### 型検査

- `Parser::literal` に `TokenKind::Char(value) => ExprKind::Char(value)`。
- `Checker::value_expression` に `ExprKind::Char(value) => (TypedExprKind::Char(value), Type::Char)`。
- `Checker::Cast` は既存 `Numeric` requirement により char casts を拒否する。テストで固定。
- `Classes::intrinsic`:
  - `Eq`: `ty.is_scalar() || string/unit` の `is_scalar` に char を含めれば Eq 対象。
  - `Ord`: `ty.is_numeric() || *ty == Type::Char`。
  - `Copy`, `Capture`, `Send` は既存 Type method で true。
  - `Display`/`Parse` は D01 実装時だけ D01 側で char を含める。
  - `Hash`/`Default` は A07 実装時だけ A07 側で char を含める。

### LLVM

`llvm_type(Type::Char)` は `i32`。

`FunctionEmitter::expression_mode`:

```rust
TypedExprKind::Char(value) => value.to_string()
```

`FunctionEmitter::binary` は char Eq/Ord を integer と同じ `icmp` で下げる。符号なし順序として扱う:

| operator | LLVM |
|---|---|
| `==`/`!=` | `icmp eq/ne i32` |
| `< <= > >=` | `icmp ult/ule/ugt/uge i32` |

`Char.of_u32_unchecked` lowering:

```llvm
%le_max = icmp ule i32 %x, 1114111
%sur_hi = icmp uge i32 %x, 55296
%sur_lo = icmp ule i32 %x, 57343
%is_sur = and i1 %sur_hi, %sur_lo
%valid = and i1 %le_max, xor i1 %is_sur, true
br i1 %valid, label %ok, label %trap
```

`console_main` char:

- `validate_main` は `Type::Char` を許可する。
- `console_main` の `buffered = matches!(...)` に `Type::Char` を追加し、`runtime/console.ll` の `@tz.console.write` が確実に連結されるようにする。
- entry-type 診断文は `"number, bool, char, unit, or string"` に更新する。
- entry `i32 %result` を UTF-8 bytes へ encode:
  - `< 0x80`: 1 byte
  - `< 0x800`: 2 bytes
  - `< 0x10000`: 3 bytes
  - otherwise: 4 bytes
- entry block に `[4 x i8] alloca` を置き、`@tz.console.write(ptr, i64)` で bytes を出し、続けて newline `\0A` も書くか、5 byte buffer に newline を含める。
- native console only。WASM library/exe に import は追加しない。

### 変更ステージ

| ステージ | 変更 |
|---|---|
| lexer | `TokenKind::Char`, `char_or_type_variable`, escape helper |
| parser | char literal を `ExprKind::Char`。`Parser::primary`, `space_argument`, `literal`, `parse_control.rs::pattern_atom`, `named_pattern`, `guard_predicate` を更新 |
| syntax | `TokenKind::Char`, `ExprKind::Char` |
| computation::expand | `ExprKind::Char` は子なし。match に追加 |
| check | `Type::Char`, `TypedExprKind::Char`, Type methods, primitive name, value typing |
| polymorph | `map_type` 変更なし。A08 core では `Classes::intrinsic` に char Eq/Ord/Copy/Capture/Send hooks。D01/A07 がある場合だけ Display/Parse/Hash/Default hooks |
| control | literal pattern は既存。Char token を literal として parse できるようにする |
| closures / recursion / ownership | `TypedExprKind::Char` は子なし・Copy。children/walk 系に漏れがないか確認 |
| call_specialization | 変更なし |
| llvm | `llvm_type`, constants, comparisons, builtin Char functions, console output |
| llvm_control | `switch_cases` と `constant_result` に `Type::Char`/`TypedExprKind::Char` |
| llvm_frame | char は scalar 8 byte layout扱い。変更なしまたは stack_size に明示 |
| runtime | 追加なし。console.ll の `@tz.console.write` を再利用 |
| driver/main | help/docs の entry result に char を追加 |

### 前提とする他チケットのインターフェース

- E02: `Char` module を標準ライブラリまたは builtin namespace として予約し、`Char.to_u32` 形式で解決できる。
- B01: `Option<char>` が利用可能なら `Char.of_u32` を追加する。B01 未完了では `Char.of_u32_unchecked` のみ。
- D01: D01 が完了済みなら `Display<char>`/`Parse<char>` を登録する。D01 未完了のブランチで A08 だけを実装する場合、`console_main` の char 出力と `Char` module だけを入れ、Display/Parse class instance は D01 側で有効化する。

### 他チケットへの提供インターフェース

- D02 は `char` と `Char.to_u32` を使って string の Unicode scalar iteration API を実装できる。ただし A08 では `for in string` を変えない。
- A07 は `char` が存在する場合、derived Display の quoted-char builtin 入力、`Hash`/`Default` の primitive component として扱える。

## 実装手順

1. **字句・構文**
   - `TokenKind::Char(u32)` と lexer helper を追加。
   - `Parser::primary`, `space_argument`, `literal`, `parse_control.rs::pattern_atom`, `named_pattern`, `guard_predicate` に Char を追加。
   - 確認: lexer tests for `'a'`, `'a`, `'\''`, `'\\'`, `'\u{1F600}'`, malformed。
2. **型検査**
   - `Type::Char`, `ExprKind::Char`, `TypedExprKind::Char` を追加。
   - Type methods と `numeric::primitive("char")` を更新。
   - `TypedExpr::children`/`children_mut` は子なし variant として fallback でもよいが、明示テストで walk 漏れを防ぐ。
   - 確認: `def f :: char\nfn f = 'あ'` が通り、`'a' as i32u` が拒否。
3. **型クラス**
   - A08 core として Eq/Ord/Copy/Capture/Send の char intrinsic を追加。
   - D01 が同じブランチにある場合だけ Display/Parse、A07 が同じブランチにある場合だけ Hash/Default も追加する。
   - 確認: `'a' == 'a'`, `'a' < 'b'`。D01 ありなら `Display.display (&'a')` も確認。
4. **Char module**
   - E02 の方式で `Char.to_u32`, `Char.of_u32_unchecked`, ASCII helpers を登録。
   - B01 がある場合だけ `Char.of_u32` を追加。
   - 確認: invalid scalar unchecked は native/WASM で trap。
5. **LLVM**
   - `llvm_type`, `expression_mode`, comparisons, `console_main`, `llvm_control::switch_cases` を更新。
   - 確認: IR に `switch i32`、char constants `i32 97`、WASM imports 空。
6. **E2E/docs**
   - fixtures と docs を追加。
   - native/WASM × `-O0`/`-O3`。

## テスト計画

### 受理

```text
def newline :: char
fn newline = '\n'

def smile :: char
fn smile = '\u{1F600}'

def compare :: bool
fn compare = 'A' < 'a' && 'a' == '\u{61}'
```

```text
def classify :: char -> i64
fn classify c =
    match c with
    | '0' -> 0
    | 'A' -> 1
    | '\n' -> 2
    | _ -> 3
```

```text
def upper :: char -> char
fn upper c = Char.to_ascii_upper c
```

### 拒否

| 入力 | 期待 |
|---|---|
| `''` | `E0001` |
| `'ab'` | `E0001` |
| `'\u{}'` | `E0001` |
| `'\u{D800}'` | `E0001` |
| `'\u{110000}'` | `E0001` |
| `def f :: i32u\nfn f = 'a' as i32u` | `E1005` |
| `def f :: char\nfn f = 65i32u as char` | `E1005` |
| `export def f :: char\nfn f = 'a'` | `E1008` |
| `def f :: i64\nfn f = 'a' + 'b'` | `E1005` |

### E2E

GUIDE §3 に従い、Node E2E の直前に必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

- `export def code_a :: i32u` returns `Char.to_u32 'A'` -> 65.
- `export def ascii :: i64` combines digit/upper/lower predicates into checksum.
- `export def match_char :: i64 -> i64` maps input through `Char.of_u32_unchecked` then match; JS reference validates scalar range.
- Native console fixture returns `'あ'`; `tsuzuri run` output bytes equal UTF-8 `E3 81 82 0A`.
- Native char console の LLVM IR は `Type::Char` が `buffered` path に入り、`runtime/console.ll` 由来の `@tz.console.write` 定義/宣言連結経路をちょうど 1 系統だけ含むことを検査する。
- WASM `Char.of_u32_unchecked 0xD800` traps with `WebAssembly.RuntimeError`.
- IR deterministic and imports empty.

## ドキュメント

- `docs/language.md`
  - 型表に `char`。
  - リテラル/escape と type variable disambiguation。
  - 文字列が byte-indexed のままであること。
  - `Char` module API と B01 依存の `of_u32`。
  - export ABI 非対応。
- `docs/architecture.md`
  - `Type::Char` は LLVM `i32`、valid scalar invariant は型検査/unchecked conversion guard が保証。
  - WASM imports なし。
- `README.md`
  - 実装済み型一覧と console result に char を追加。

## 受け入れ条件

- [ ] `'a'`, escapes, Unicode scalar char literals が lex/parse/typecheck できる。
- [ ] `'a` type variable は従来通り動く。
- [ ] surrogate/out-of-range/multi-scalar char literal は `E0001`。
- [ ] `char` は A08 core で Copy/Eq/Ord、Numeric ではない。
- [ ] D01 が同じブランチにある場合だけ `Display`/`Parse` for char が追加される。
- [ ] A07 が同じブランチにある場合だけ `Hash`/`Default` for char が追加される。
- [ ] `Char.to_u32`, `Char.of_u32_unchecked`, ASCII helpers が動く。
- [ ] B01 完了時だけ `Char.of_u32 : i32u -> Option<char>` が提供される。
- [ ] match char が `switch i32` に lower されるケースを持つ。
- [ ] native console が UTF-8 で char result を出力し、WASM は imports を増やさない。
- [ ] `console_main` の entry-type 診断文が `char` を含み、char console IR が `@tz.console.write` 連結経路を重複させない。
- [ ] native/WASM × `-O0`/`-O3`、IR 決定性。

## 落とし穴

- `char` を `NUMERIC_NAMES` に入れない。`as` cast や Numeric constraint が誤って通る。
- `Type::is_scalar` に入れる場合、`exportable` は別に false にする。
- `'\u{D800}'` は `u32 <= 0x10FFFF` でも Unicode scalar ではない。
- `for in string` を char iteration に変えない。既存コードの byte semantics を破壊する。
- char console output に `printf("%c")` を使うと非 ASCII が壊れる。必ず UTF-8 encode して `@tz.console.write`。
- WASM 用にホストの Unicode API を import しない。

## 対象外

- Unicode grapheme cluster、正規化、full case folding、locale-aware classification。
- string の char indexing/iteration。D02。
- char ranges/pattern classes。
- char の export ABI。E05 までは公開しない。
- `char` と整数の `as` cast。

## 未決事項

- **既定案: B01 未完了では `Char.of_u32_unchecked` のみ。** `Option` を独自に作らず、B01 完了後に safe API を追加する。
- **既定案: `Type::Char` を `is_scalar` に含めるが `is_numeric` には含めない。** Eq intrinsic の既存条件を再利用しつつ Numeric を拒否する。
- **既定案: console は char result を表示対象に含める。** export ABI は変更しない。
