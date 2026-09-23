# G05: フォーマッター（tsuzuri fmt）
| 項目 | 内容 |
|---|---|
| ID | G05 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | なし |
| 後続 | G07, G09 |
| 状態 | todo |
| 主な影響ファイル | `src/lexer.rs`, `src/parser.rs`, `src/syntax.rs`, `src/diagnostic.rs`, `src/driver.rs`, `src/main.rs`, `src/formatter.rs`, `tests/formatter.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

Tsuzuri の標準フォーマッター `tsuzuri fmt` を追加し、リポジトリ内外の `.tz` / `.tt` / `.tc` を
決定的かつ保守的に整形できるようにする。

初期段階では意味を変えないことを最優先し、コメントを保持し、idempotent であることを保証する。

## 現状

字句解析はコメントを捨てる。

| ファイル | 関数 | 現状 |
|---|---|---|
| `src/lexer.rs` | `Lexer::tokens` | `//` 行コメントを改行まで読み飛ばす |
| `src/lexer.rs` | `Lexer::comment` | `/* ... */` block comment を読み飛ばす |
| `src/lexer.rs` | `lex` | `Vec<Token>` だけを返し、空白 / コメント trivia を保持しない |
| `src/parser.rs` | `Parser::program` | token stream と元 source の gap を使って newline 判定をする |
| `src/parse_control.rs` | `body_expression`, `layout_block` | indentation-sensitive body を元 source の column で判定する |

CLI は `check`, `build`, `run` だけである。

```text
tsuzuri check source.tz|source.tt|source.tc|directory [--json]
tsuzuri [build] source.tz|source.tt|source.tc|directory [options]
tsuzuri run Main.tz|directory [-O0|-O1|-O2|-O3] [--cpu generic|native] [--json]
```

出力保護 / atomic replace は `src/driver.rs` の `build`, `protect_sources`, `TemporaryDirectory` に実装されているが、
formatter は source 自身を書き換えるため、build output protection とは別の安全策が必要である。

## 仕様

### CLI

```text
tsuzuri fmt [--check] <file|dir> [--json]
```

入力:

- `.tz`, `.tt`, `.tc` の通常ファイル。
- directory。直下の `.tz`, `.tt`, `.tc` をファイル名順に整形する。
- directory は再帰しない。現行 compiler の module loading と同じ。

出力:

- `--check` なし:
  - 変更が必要な file を atomic replace で更新する。
  - 変更なし file は触らない。
  - stdout には何も出さない。
  - warning / error は stderr。
- `--check`:
  - file を変更しない。
  - 整形差分が必要な file があれば exit code 1。
  - human: `path: not formatted`
  - JSON: `{"severity":"error","code":"E2000","message":"file is not formatted","path":"..."}`

exit code:

- 成功 / already formatted: `0`
- formatter 差分あり (`--check`) / source error / I/O error: `1`
- CLI 引数エラー: `2`

`fmt` は LLVM / Clang / wasm-ld を必要としない。

### formatting policy

G05 phase 1 は conservative formatter とする。

**phase 1 で行うこと:**

- BOM を保持する。
- 改行コードを保持する。
  - 入力が CRLF 優勢なら CRLF。
  - それ以外は LF。
- 末尾空白を削除する。
- file 末尾をちょうど 1 改行にする。
- token 間の明らかな horizontal whitespace を正規化する。
- indentation-sensitive body の indent を 4 spaces 単位にそろえる。
- `,`, `;`, `:`, `::`, `->`, `=>`, `=`, binary operators の前後 spacing を統一する。
- `record`, `class`, `instance`, `def`, `fn`, `let`, `if`, `match`, `for`, `while`, computation block の基本 layout を維持する。
- コメントの相対位置を保持する。
- 空白適用と postfix 呼び出し / index の意味を保持する。
  `Parser::space_argument` により、`f (1, 2)` は tuple 1 引数の空白適用、`f(1, 2)` は postfix call、
  `f [x]` は配列 1 引数、`f[x]` は index であり、formatter はこの separator を変えない。

**phase 1 で行わないこと:**

- 行幅に基づく自動改行。
- expression の再構成。
- record / array / match arms の多行化判断。
- コメント文面の reflow。
- 旧構文から新構文への変換。
- import / declaration の並べ替え（import は現状なし）。

これにより、既存 corpus に対して `fmt(fmt(x)) == fmt(x)` を達成し、AST equality を保証する。

### 将来の full formatting rules

phase 1 は保守的だが、設計は次の規則へ拡張できるようにする。

| 項目 | 目標規則 |
|---|---|
| indentation | 4 spaces |
| body after `=` / `then` / `do` / `->` | 次行 body は +4 spaces |
| binary operators | 前後 1 space |
| unary operators | operand との間に不要 space を入れない。ただし `- 1` は `-1` へは変えない |
| function application | 既存の空白適用を維持。改行をまたぐ application は作らない |
| records | single-line は `{ x: 1, y: 2 }`、multi-line は field ごとに +4 |
| arrays/lists | single-line は `[1, 2]` / `[|1, 2|]`、multi-line は element ごとに +4 |
| match arms | `| pattern when guard -> expr`、multi-line body は `->` 後改行 +4 |
| `def` / `fn` pair | 間に空行 0 または 1。別 declaration group 間は 1 空行 |
| class / instance | method ごとに +4 |
| blank lines | 連続空行は最大 1 |

phase 1 で未実装の規則は docs に「まだ保守的」と明記する。

### semantic preservation

formatter は整形前後を parse し、AST が spans を除いて同一であることを確認する。

```rust
let before = parser::parse_with_source(source, 0)?;
let formatted = formatter::format(source, kind)?;
let after = parser::parse_with_source(&formatted, 0)?;
assert_ast_eq_ignoring_spans(&before, &after)?;
```

AST equality helper は test 専用ではなく formatter 内部にも使う。
formatter が AST equality に失敗した場合は `E2000` ではなく internal formatting failure として
`E2001` 相当の I/O ではない source tool errorを返す。既定案は `E2000`:

```text
formatter would change program semantics; please report a bug
```

ただし利用者の入力が parse error の場合は通常の parser diagnostic を返す。

### comment preservation

lexer に trivia-preserving mode を追加する。

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
}

#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TokenWithTrivia {
    pub token: Token,
    pub leading: Vec<Trivia>,
}

pub fn lex_with_trivia(source: &str) -> Result<Vec<TokenWithTrivia>, Diagnostic>;
```

通常 parser は既存 `lex` を使い、trivia を見ない。
`lex_with_trivia` は formatter 専用で、既存 parser の token 種類 / span を変えない。

line comment は改行を含めない。
newline は独立 trivia とする。
block comment は入れ子を保持し、元 text をそのまま保持する。
末尾空白削除は comment token の外側の whitespace trivia にだけ適用する。line comment / block comment の内部 text は
byte-for-byte で保持し、コメント内の末尾空白や CRLF を整形しない。

### CRLF / BOM

- BOM がある入力は、formatted output も先頭に BOM を持つ。
- CRLF が lone LF より多い場合、formatter の生成改行は CRLF。
  lone LF は「直前が `\r` ではない `\n`」だけを数える。CRLF 中の `\n` を LF と二重に数えない。
- mixed newline は、生成部分は majority newline に統一するが、コメント内部の text は保持する。
- final newline は majority newline。

### atomic replace

formatter は source file を更新するため、build の `protect_sources` は使わない。
新 helper:

```rust
fn replace_source_atomically(path: &Path, text: &[u8]) -> Result<(), Diagnostic>;
```

規則:

- `path` が regular file であることを確認する。
- symlink / directory / special file は拒否する。
- parent directory に一時ファイルを作る。
- 書き込み、flush、rename の順。
- 失敗時に元 file を変更しない。
- Unix では mode を元 file から引き継ぐ。
- hard link は formatter の性質上、rename により link を切る。既存 build と異なり source update が目的なので許容するが、docs に記す。

## 設計

### formatter module

新規 `src/formatter.rs`。

```rust
pub struct FormatOptions {
    pub check: bool,
}

pub struct FormatResult {
    pub changed: bool,
    pub formatted: String,
}

pub fn format_source(
    path: &str,
    source: &str,
    kind: SourceKind,
) -> Result<FormatResult, Diagnostic>;
```

`path` は diagnostics / comments debug 用。AST には入れない。

### phase 1 algorithm

1. `lex_with_trivia(source)`。
2. `parser::parse_with_source(source, 0)` で before AST。
3. token + trivia を sequential に pretty print。
4. generated text を `parser::parse_with_source(&formatted, 0)`。
5. `ast_eq_ignoring_spans(before, after)`。
6. `format_source(formatted)` をもう一度走らせ、同じ text になることを assert（debug/test）。

Pretty print は AST ではなく token stream ベースから始める。
理由:

- コメント位置を保持しやすい。
- phase 1 は whitespace 正規化だけなので AST printer より安全。
- indentation-sensitive parser の意味を変えにくい。

ただし semantic preservation は AST で検査する。

### token spacing rules (phase 1)

`Formatter` は直前 token と現在 token の pair で separator を決める。

separator:

- `Newline(indent)`。
- `Space`.
- `None`.

separator は grammar-aware に分類する。特に `LeftParen` / `LeftBracket` / `LeftList` の前は、
元 source で token が隣接していたか空白があったかを保持する。

| 元の形 | AST / parser 上の意味 | formatter |
|---|---|---|
| `f(1, 2)` | postfix call、2 引数 | 隣接を保持 |
| `f (1, 2)` | 空白適用、tuple 1 引数 | 1 space を保持 |
| `f[x]` | index | 隣接を保持 |
| `f [x]` | 空白適用、array 1 引数 | 1 space を保持 |
| `value.field` | field access | 隣接を保持 |

この分類は trivia token の gap だけでなく、parser の `space_argument` / `postfix` と同じ条件
（newline なし、前 token end と current token start の隣接/非隣接）で行う。

基本:

- `(`, `[`, `[|`, `{` の直後は space なし。
- `)`, `]`, `|]`, `}` の直前は space なし。
- `,` / `;` の前は space なし、後ろは space 1 または newline。
- `:` は record field / type annotation で後ろ 1 space、前なし。
- `::`, `->`, `=>`, `=`, binary operators は前後 1 space。
- field access `.` は前後 space なし。
- range `..` は前後 1 space。
- unary prefix `&`, `&mut`, `*`, `!`, `~` は後ろ space なし。ただし `&mut` は内部 1 space。
- keyword pairs:
  - `if <expr> then`
  - `while <expr> do`
  - `for <pat> in <expr> do`
  - `match <expr> with`

layout decisions:

- 既存に newline がある場所は newline を保持する。
- `record` / `class` / `instance` body の braces 内 newline は保持し、indent を正規化する。
- `=` / `then` / `do` / `->` の後に既存 newline がある場合、次 line indent を +4 にする。
- 既存 single-line expression は single-line のまま。
- grammar-aware layout stack を持つ。
  - explicit `{ ... }` block / record / class / instance body。
  - indentation body after `=`, `then`, `do`, `->`。
  - `match` arm after `|` and `->`。
  - `else` / `elif` alignment。`Parser::conditional_else` と同じく、浅い indent の `else` は外側に属する。
- column は layout の意味に影響するため、既存 newline の indent を変更した後は必ず AST equality で検証する。

### AST equality ignoring spans

`syntax.rs` の AST structs に `PartialEq` を derivation すると span まで比較してしまう。
次のどちらかを採用する。

既定案:

```rust
pub(crate) fn ast_fingerprint(program: &Program) -> String
```

fingerprint は Debug-like な構造出力だが、すべての `Span` と `depth` を除く。
`Ident` は `text` だけ、`Token` は使わない。

代替案:

- AST structs に custom `eq_ignoring_spans` helper を実装する。

fingerprint の方が M 規模に収まる。

### 前提とする他チケットのインターフェース

なし。

### 他チケットへの提供インターフェース

- G07 LSP は `formatter::format_source` を document formatting に使える。
- G09 doc comments は `lex_with_trivia` の trivia model を再利用する。
- G06 test declaration を追加する場合、formatter の token spacing table に `test` を追加する。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer | `lex_with_trivia`, `Trivia`, `TokenWithTrivia` を追加。既存 `lex` は互換維持 |
| parser | 変更なし。semantic preservation の再 parse に使う |
| check / polymorph / control / closures / ownership | 変更なし |
| llvm / runtime | 変更なし |
| driver | source enumeration helper を fmt でも再利用するか、新 helper を追加 |
| main | `fmt` subcommand, `--check`, JSON diagnostic |
| tests | formatter corpus / idempotence / AST equality |

## 実装手順

1. **trivia-preserving lexer を追加する。**
   - `TriviaKind`, `Trivia`, `TokenWithTrivia` を追加。
   - `lex_with_trivia` は既存 token と同じ span / kind を返す。
   - 既存 `lex` tests が通ること。
   - 新 test:
     - line comment が `LineComment`。
     - nested block comment が `BlockComment`。
     - BOM が token trivia ではなく formatter metadata として保持される。

2. **formatter module skeleton を追加する。**
   - `format_source` を追加。
   - まず入力をそのまま返す実装にして parse before/after と idempotence harness を作る。
   - `ast_fingerprint` を追加。
   - 確認: corpus 全ファイルで identity formatter が通る。

3. **末尾空白 / final newline / newline style を実装する。**
   - comment token 外の whitespace trivia だけ末尾空白削除。
   - final newline 1 つ。
   - CRLF majority preservation。LF count は lone LF のみ。
   - 確認: BOM + CRLF file の test。

4. **horizontal spacing を実装する。**
   - token pair table を作る。
   - `(`/`[`/`[|` の前の separator を application / postfix / index に分類して保持する。
   - single-line constructs の spacing を正規化する。ただし semantic separator は変えない。
   - コメント前後の空白を保持し、コメントを token に吸着しすぎない。コメント内部 text は byte-for-byte で保持する。
   - 確認:
     - `let x=1+2` -> `let x = 1 + 2`
     - `Point{x:1,y:2}` は parser 上 record ではない可能性があるため、既存 token spacing と AST equality で確認。
     - `f (1, 2)` と `f(1, 2)`、`f [x]` と `f[x]` の AST が整形前後でそれぞれ一致すること。

5. **indentation phase 1 を実装する。**
   - 既存 newline を保持し、grammar-aware layout stack で次 line の基準 indent を計算する。
   - explicit `{}`、indentation body、match arm、`else` / `elif` を別 frame として扱う。
   - `{` 後 newline は +4、`}` は -4。
   - `=` / `then` / `do` / `->` 後 newline は +4。
   - match arm `|` は current match block indent に合わせる。
   - `else` は対応する `if` frame に alignment し、内側 block の `else` と外側 `else` を取り違えない。
   - 確認: `docs/language.md` の examples と `tests/fixtures/control`。

6. **CLI `fmt` を追加する。**
   - `Action::Fmt`。
   - `fmt` は build options を受け付けない。
   - `--check` は fmt 専用。
   - `--json` は既存と共有。
   - `--help` を更新。

7. **file / directory traversal を実装する。**
   - file は 1 file。
   - dir は直下 `.tz/.tt/.tc` を filename sort。
   - subdir は探索しない。
   - unsupported extension は `E2000`。

8. **atomic replace を実装する。**
   - symlink / directory / special file を拒否。
   - temp file + rename。
   - `--check` では書かない。
   - 確認: 既存 file preserved on formatter failure。

9. **corpus test を追加する。**
   - `glob` 相当を Rust std で実装し、repo の `.tz/.tt/.tc` を列挙する。
   - `tests/fixtures`, `examples`, `benchmarks` を含む。
   - 各 file:
     - `fmt1 = format(source)`
     - `fmt2 = format(fmt1)`
     - `fmt1 == fmt2`
     - AST fingerprint before == after

## テスト計画

### Rust tests

新規 `tests/formatter.rs`。

単体:

```text
let x=1+2
x
```

期待:

```text
let x = 1 + 2
x
```

コメント:

```text
let x = 1 // keep
/* keep
   block */
x
```

期待:

- コメント text が byte-for-byte 残る。
- 末尾空白削除は comment 外だけに適用され、comment 内の trailing spaces は残る。
- AST equality。

CRLF/BOM:

- 入力 `\u{feff}let x=1\r\nx\r\n`。
- 出力も BOM + CRLF。
- mixed newline で `\r\n` の `\n` を LF と二重計上しない。

semantic whitespace golden tests:

- `f (1, 2)` と `f(1, 2)`。
- `f [x]` と `f[x]`。
- `value . field` は既存 parser で field access ではないため、formatter が勝手に `value.field` へ変えないこと。

nested indentation golden tests:

- `if ... then` の中の `if ... then ... else ...`。
- `match` arm body の中の `let` / nested `match`。
- computation expression の `for` / `while` / `if` body。

idempotence:

- hand-written cases。
- corpus all `.tz/.tt/.tc`。

rejection:

- parse error file は `E0001` / `E0002` を返し、file を変更しない。
- symlink は `E2001` または `E2003` 相当で拒否。

### CLI tests

`tests/e2e.mjs`。

- `tsuzuri fmt --check already_formatted.tz` -> 0。
- unformatted temp file:
  - `tsuzuri fmt --check file` -> 1、file unchanged。
  - `tsuzuri fmt file` -> 0、file changed。
  - second `tsuzuri fmt --check file` -> 0。
- directory:
  - `.tz`, `.tt`, `.tc` だけ整形。
  - nested subdir は無視。
- JSON:
  - `--check --json` の not formatted が JSON object。

### build/test validation

formatter は codegen を変えないが、corpus AST equality と既存 tests を通す。

```sh
cargo test --locked --test formatter
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
```

`cargo test --locked --test formatter` は GUIDE §3 の通り `--test <ファイル名>` を使い、
出力の `running N tests` が 0 でないことを確認する。Node E2E の直前には
`cargo build --release --locked` を実行する。

## ドキュメント

- `README.md` CLI 表に `tsuzuri fmt [--check] <file|dir>` を追加する。
- `docs/language.md` の source 節に formatter の保守的 phase 1 と comment preservation を追記する。
- `docs/architecture.md` に trivia lexer と semantic preservation check を追記する。

## 受け入れ条件

- [ ] `tsuzuri fmt` subcommand がある。
- [ ] `--check` が file を変更しない。
- [ ] file / directory 入力に対応し、directory は直下のみ。
- [ ] comments が保持される。
- [ ] trailing whitespace removal は comment 外の whitespace trivia にだけ適用され、comment token text は byte-for-byte で保持される。
- [ ] BOM / CRLF が保持される。
- [ ] CRLF majority 判定は lone LF だけを LF として数える。
- [ ] `f (1, 2)` / `f(1, 2)` と `f [x]` / `f[x]` の意味を変えない。
- [ ] grammar-aware layout stack により nested body / match arm / else の indent を壊さない。
- [ ] formatter output が parse でき、AST fingerprint が一致する。
- [ ] `fmt(fmt(x)) == fmt(x)`。
- [ ] repo 内すべての `.tz/.tt/.tc` corpus test が通る。
- [ ] source update は atomic replace。
- [ ] parse / formatting failure で file を変更しない。
- [ ] 既存 `lex` / parser / compiler behavior が変わらない。

## 落とし穴

- コメントを捨てる既存 `lex` を formatter に使うと、コメント消失という破壊的変更になる。
- AST printer だけで始めると、phase 1 の範囲を超えて構造を変えやすい。
- 空白適用は改行をまたがないため、勝手な改行は意味を変える。
- `(` / `[` の前の空白も意味を持つ。`f (1, 2)` を `f(1, 2)` にしたり、`f [x]` を `f[x]` にしたりしない。
- indentation-sensitive body は column が意味を持つ。既存 newline の indent 変更は AST equality で必ず検査する。
- `else` の column は `Parser::conditional_else` の結合先を変える。layout stack なしの単純 indent 補正は危険。
- CRLF を LF に変えるだけでも利用者には大きな差分になる。phase 1 では newline style を保持する。
- `fmt` は source file を更新するため、build の `protect_sources` と同じ「source overwrite 拒否」は使えない。

## 対象外

- line width based wrapping。
- import / declarations sorting。
- doc comment formatting。
- formatter configuration file。
- partial range formatting。
- stdin/stdout formatting。
- semantic rewrite / modernization。

## 未決事項

- **AST equality の実装方式。** 既定案は span/depth を除いた fingerprint。
- **hard link の扱い。** 既定案は rename により link を切ることを許容し、docs に明記する。
- **phase 1 の record literal spacing。** `Name {}` が computation expression と曖昧なため、AST equality を最終判定にする。
- **台帳の見直し提案:** なし。
