# G19: 言語版（edition）と互換性・非推奨の管理

| 項目 | 内容 |
| --- | --- |
| ID | G19 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | E04, G09, (E10) |
| 後続 | E08（エラー種別の追加）, D09（UCD の更新） |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（manifest の `edition` の名前・値・既定）, D4（doc comment の `@deprecated` タグの意味と `W1005` の確定。GUIDE D-30 の仮割り当て）, D9（Phase 2 の `tsuzuri api-diff`） |
| 改善する劣位 | C/C++ 比: 長期運用の実績（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）のうち、仕組みで補える互換性の保証 |
| 手本にする既存実装 | manifest のキー: `src/package.rs` の `parse_manifest` の `"package"` 分岐（`name`・`version`）と `Line::error`（E0002）。ソースごとの属性の受け渡し: `src/driver.rs` の `SourceFile::package` → `src/lib.rs` の `SourceInput::origin` → `src/check.rs` の `ModuleInput::origin`。警告: `src/warnings.rs` の `unused_locals`・`unused_private` と `check_modules_indexed_all` での呼び出し。参照から定義と説明への対応: `src/semantic.rs`（`check::semantic`）の `collect`・`SemanticIndex::doc_for`（`tests/lsp.rs` が検査）。package graph のテスト: `tests/modules.rs` の `loads_and_protects_local_package_graphs`。CLI の警告テスト: `tests/warnings.rs` の `cli_caps_warnings_and_denies_before_touching_artifacts` |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/package.rs`, `src/driver.rs`, `src/lib.rs`, `src/lsp.rs`, `src/check.rs`, `src/polymorph.rs`, `src/computation.rs`, `src/warnings.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/stdlib.rs`, `tests/editions.rs`（新規）, `tests/warnings.rs`, `tests/chars.rs`・`tests/diagnostics.rs`・`tests/lists.rs`・`tests/strings.rs`・`tests/formatter.rs`・`tests/generic_records.rs`（呼び出しの機械的な更新）, `docs/language.md`, `docs/architecture.md`, `docs/stability.md`（新規）, `_docs/language-reference/modules-and-packages.md`, `_docs/tools/diagnostics.md`, `_docs/feature-status.md`, `_features/README.md`。Phase 2 は `src/docgen.rs`, `src/main.rs`, `tests/api_diff.rs`（新規） |

## 目的

C/C++ の規格の版、Rust の edition と安定性保証、C# の言語版のように、言語と標準ライブラリの変更で既存のコードが壊れないことを保証する枠組みを作る。
実績そのものは時間でしか得られないが、互換性の方針と検査の仕組みは今から整えられる。

Phase 1 は次の三つで、単独で出荷できる。

- manifest の `edition` と、ソースファイルごとの edition による予約語表の選択（異なる edition の package を同じ graph で結合できる）。
- doc comment の `@deprecated` タグと、非推奨の宣言の使用を知らせる警告 `W1005`。
- 安定性方針の文書 `docs/stability.md`（新規）。

Phase 2 は公開 API の差分検査 `tsuzuri api-diff`（D9）。実装者は Phase 1 だけを実装する。Phase 2 は D9 の承認後、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- E04（パッケージ）と G09（ドキュメントコメント）が `_features/README.md` の状態欄で done であること。
  確認: `grep -nE "^\| (E04|G09|E10) \|" _features/README.md` で E04・G09 が `done`。
- E10 は開始条件にしない。E10 が先に入っていても、`parse_manifest` の `"package"` 分岐の許可キーに `edition` を足すだけで衝突しない（E10 は `[dependencies]` を変える）。
- D1 が承認済みであること。承認前は手順 2 以降に着手しない。D4 の承認前は手順 7–9 に着手しない（手順 2–6 は D4 と独立に出荷できる）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- manifest の解析に crate（`toml` など）を足したくなった。または E10 が `parse_manifest` を別の構造（別ファイル、crate）へ移していて、`"package"` 分岐へキーを足す形にならない。
- `Edition` を `PackageId` に入れたくなった（`PackageId` は `Ord` で並べ替えと同一性に使う。D2）。
- lexer 以外（parser・checker・所有権・LLVM）で edition による分岐が必要になった。Phase 1 の edition の差は予約語だけである（D3）。
- 同じプログラムの IR が edition の値によって変わる。
- `SemanticIndex::doc_for` の対応（参照の `target` と宣言名の span の一致）が `def`・`record`・`union` のどれかで成り立たず、W1005 を semantic index から出せない（D5）。
- `@deprecated` がない project で `semantic::collect` が走る（W1005 の費用は `@deprecated` を書いた project だけが払う。D5）。
- 既存テストの期待値（診断コード・IR・警告）を変える必要がある。例外は `parse_manifest` の未知キーのメッセージ（`expected name or version` → `expected name, version, or edition`）で、HEAD にこの文言を検査するテストはない。
- stack-depth の 3 テスト（GUIDE §11.1 の既知の落とし穴、`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・`bounds_nested_builder_expansion_not_just_source_syntax`）が失敗する。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- `src/package.rs`: `Manifest { name, version, namespace, dependencies }`、`PackageId { name, root }`。`parse_manifest` の `[package]` は
  `name`・`version` だけを受け、他のキーは `cursor.error("unknown package key; expected name or version")`（E0002）。値の検査の失敗も
  `Line::error` の E0002 である（例: `dependency path must be a nonempty relative path`）。重複と空の値は `package fields must be nonempty and occur exactly once`。
- `src/driver.rs`: `SourceFile { path, relative_path, name, text, origin, package: Option<PackageId> }`。package の読み込みで
  `source.package = package.map(|package| package.id.clone())` を設定し、std は `stdlib::SOURCES` から `package: None` で足す。
  manifest なしの project は `package: None`。`Project::analyze_all` は `crate::SourceInput { path, text, origin }` を作る。`src/lsp.rs` も同じ形で `SourceInput` を作る。
- `src/lib.rs`: `analyze_inputs_indexed_all` が `parser::parse_with_source_all(input.text, id)` で解析し、`ModuleInput { name, program, origin }` を
  `check::check_modules_indexed_all` へ渡す。`analyze`・`analyze_modules*` は manifest なしの入口で、`SourceInput` を `ModuleOrigin::User`／`Std` で作る。
- `src/lexer.rs`: `lex`・`lex_all`・`lex_with_trivia` は source だけを受ける。予約語は `Lexer::identifier` の固定の `match`（`"fn" => TokenKind::Fn` など）。
  呼び出し元は `src/parser.rs` の `parse_with_source_all`・`parse_source`、`src/check.rs` の `check_modules_indexed_all` のモジュール名の検査
  （`crate::lexer::lex(name)` で各要素が識別子一つになることを確かめる）、`src/formatter.rs` の `print_tokens`（`lex_with_trivia`）。
  `parse_with_source` の呼び出し元は `src/formatter.rs` の `format_source`（2 回）、`src/docgen.rs`、`src/stdlib.rs`。
- ドキュメントコメント: `///` を `TokenKind::DocComment` にし、`Parser::take_doc` が行を `\n` で結合して `Documentation { text, span }` を作る。
  宣言の `doc: Option<Documentation>`。`semantic::collect` は `SemanticIndex::document` で「宣言名の span → 説明」を記録し、関数の参照
  （`TypedExprKind::Function(FunctionRef::User(id))`・`GenericFunction`）の `target` を `functions[id].span`、record・union の型と構築の
  `target` を `type_target` で記録する。`tests/lsp.rs` は `index.doc_for(function.target.unwrap())` と record の説明を検査している。
  `semantic::collect` は `check_modules_indexed_all` の `semantic` 引数が `Some` のとき（LSP と `analyze_modules_with_semantics`）だけ走る。
- 警告は `W1001`–`W1004`。`check_modules_indexed_all` が `warnings::unused_locals`・`warnings::unused_private` などを集め、
  `(span.source, span.start)` で並べて `CheckedModule::warnings` に入れる。`--deny-warnings`（`src/main.rs`）で失敗にできる。
- 非推奨の仕組み、API の差分検査、semver の検査はない。`docs/stability.md` はない。GUIDE D-30 は「G19 の edition を導入した後は、
  新しい予約語を新しい edition でだけ予約する」とし、`W1005` を G19 に仮割り当てしている。
- `stdlib::is_reserved_module` は std のモジュール名（`RESERVED_MODULES`）を利用者のモジュール名として拒否する。std へのモジュールの追加は、
  同名の利用者モジュールを E1011 にする（D8 で安定性方針に書く）。

### 再現（検証済み）

各 case は `/tmp/tz-work-G19/<case>/` に置く。`a` は `Tsuzuri.toml`（`name = "a"`, `version = "0.1.0"`, `edition = "2026"`）と
`Main.tz`（トップレベルの `42`）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri run /tmp/tz-work-G19/a   # E0002: unknown package key; expected name or version（終了コード 1）
target/release/tsuzuri run /tmp/tz-work-G19/b   # 42（dyn と bench は今は識別子）
target/release/tsuzuri run /tmp/tz-work-G19/c   # 42（@deprecated は説明の文字列で、警告は出ない）
```

`b/Main.tz`:

```tsuzuri
let dyn = 40
let bench = 2
dyn + bench
```

`c/Main.tz`:

```tsuzuri
/// Old API.
/// @deprecated use add instead
def old :: i64 -> i64 = \x -> x + 1

old 41
```

## 仕様

### 前提とする他チケットのインターフェース

- E04（done）: `Project::load*` の package graph、`PackageId`、`parse_manifest`。package のソースは `SourceFile::package` が `Some`、
  manifest なしの root と std は `None`。依存 package のモジュール名は namespace で始まる（`LegacyLib.Old`）。
- G09（done）: `Documentation { text, span }` と宣言の `doc`。`///` の直後の空白一つは lexer が取り除く。
- E10（任意）: git 依存の package も自分の `Tsuzuri.toml` の `edition` を使う。E10 側の変更は要らない。
- 予約語を足すチケット（A14 の `dyn`、G18 の `bench`）: G19 より前に入った予約語は edition 2026 の一部になる（`Lexer::identifier` の
  `match` に直接足す）。G19 の後に入る予約語は `match` に足したうえで、`EDITION_KEYWORDS`（新規）へ予約を始める edition と一緒に登録する（D12）。

### 構文

manifest（新構文（実装後に有効。未検証））:

```text
package-section = "[package]" NEWLINE { package-field }
package-field   = ( "name" | "version" | "edition" ) "=" STRING NEWLINE
edition-name    = "2026"        (* Phase 1 で受ける値。Edition::SUPPORTED の名前 *)
```

- `edition` は省略できる。各キーは一回だけ（既存の `fields` の重複検査）。値は `Edition::SUPPORTED` の名前と完全一致する文字列だけを受ける。
- ソースの edition は次で決まる（D1）。

| ソース | edition |
| --- | --- |
| `edition` を書いた manifest の package | その値 |
| `edition` のない manifest の package | `2026`（`Edition::MANIFEST_DEFAULT`（新規）。edition を足しても変えない） |
| manifest のない project、`tsuzuri::analyze*` の入口 | `Edition::LATEST`（新規。Phase 1 は `2026`） |
| std（`stdlib::SOURCES`、`analyze_modules_with_std*` の std） | `Edition::LATEST` |
| `tsuzuri fmt` の対象 | 入力ディレクトリ（ファイル入力は親）の `Tsuzuri.toml` の edition。なければ `Edition::LATEST`（D11） |

doc comment の非推奨タグ（新構文（実装後に有効。未検証）。字句と構文は変えず、説明の本文の規約として解釈する）:

```text
deprecation = "@deprecated" [ " " note ]   (* 説明の行を trim_start した先頭。最初の該当行だけが有効 *)
note        = 行の残りを trim したもの（空でもよい）
```

- `@deprecatedX` はタグではない。二つ目以降の `@deprecated` 行は通常の説明である。`tsuzuri doc` と LSP hover は本文をそのまま表示する。
- W1005 の対象は Phase 1 では `def`（`export def`・`private def` を含み、`extern def` を除く）、`record`、`union`（D4）。
  `const`・`type`・`class`・class の method・`extern def` のタグは説明として表示されるだけで、警告は出ない。

### edition で変えてよいもの（D3）

- 変えてよい: 予約語の追加、前の edition で非推奨にした構文の削除、警告の既定の有効・無効。
- 変えてはいけない: 両方の edition が受理するプログラムの意味（型、値、評価順序、所有権・借用、数値・トラップ、layout・ABI、IR）。
  edition の差分は「旧 edition で受理するプログラムを新 edition で拒否する」か「旧 edition で拒否するプログラムを新 edition で受理する」形に限る。
- Phase 1 の edition は `2026` だけで、差分はない。std は全 edition で共有する（D8）。

### 名前と予約語（D6）

- 予約語の判定はソースファイルの edition で行う。モジュール名の要素の検査（`check_modules_indexed_all` の `crate::lexer::lex(name)`）も、
  そのモジュールの edition を使う。
- raw identifier はない。新 edition の package からは、旧 edition の package が新しい予約語と同じ名前で公開した宣言を参照できない（E0002）。
  旧 edition の package 自体は変わらず compile できる。

### 意味・IR への影響

edition と W1005 は字句と警告だけに作用する。評価順序・所有権・借用・数値・トラップ・native と WASM の差を変えない。
IR は edition の値と `@deprecated` の有無で byte 単位で同じで、WASM の import も増えない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E0002 | `[package]` の未知のキー | `unknown package key; expected name, version, or edition` | その行 |
| E0002 | `edition` の値が `Edition::SUPPORTED` にない | `unsupported edition '<value>'; supported editions: 2026` | その行 |
| E0002 | `edition` の値が文字列でない、空、重複 | 既存の `Line::string` のメッセージ、`package fields must be nonempty and occur exactly once` | その行 |
| E0002 | ソースの edition の予約語を識別子の位置に書いた | 既存の parser のメッセージ（変えない） | その token |
| E1011 | モジュール名の要素がそのモジュールの edition で予約語 | 既存のメッセージ（変えない） | 既存どおり |
| W1005 | 別 package（std を含む）の非推奨の `def`・`record`・`union` の参照 | `'<Qualified.name>' is deprecated: <note>`。note が空なら `'<Qualified.name>' is deprecated; see its documentation for a replacement` | 参照の span |

- `supported editions:` の一覧は `Edition::SUPPORTED` を `, ` で結合する。`<Qualified.name>` は宣言のモジュール名と宣言名（`LegacyLib.Old.old`）。
- W1005 を出す参照は `semantic::collect` が `target` を記録するもの: 関数の参照、record の構築式、union の case の構築式（手順 8 で追加）、
  宣言の署名・field・型別名・const の型注釈。match の pattern は対象外（D5）。
- W1005 は既定で有効で、`--deny-warnings` で失敗になる。同じ package（manifest なしの project 全体を含む）の中の参照には出さない。

### 資源上限

- 新しい上限はない。manifest は既存の 1 MiB の中。W1005 は既存の警告と同じ表示 50 件・収集 1000 件の上限（`MAX_REPORTED_ERRORS`・`MAX_UNIQUE_DIAGNOSTICS`）に従う。
- W1005 の検出のための `semantic::collect` は、`@deprecated` を一つ以上含む project でだけ走る（D5）。

### 例

package graph `/tmp/tz-work-G19/e/`（HEAD で `run app` が `42` を出すことを確認済み。警告は実装後の期待）。
`legacy-lib/Tsuzuri.toml` は `name = "legacy-lib"`・`version = "0.1.0"`、`app/Tsuzuri.toml` は `name = "app"`・`version = "0.1.0"` と
`[dependencies]` の `legacy-lib = { path = "../legacy-lib" }`。

`legacy-lib/Old.tz`:

```tsuzuri
/// @deprecated use LegacyLib.Old.next instead
def old :: i64 -> i64 = \x -> x + 1

def next :: i64 -> i64 = \x -> x + 1

/// @deprecated use a tuple
record Pair { left: i64, right: i64 }

/// @deprecated use bool
union Light = Red | Green

def inside :: i64 -> i64 = \x -> old x
```

`app/Main.tz`:

```tsuzuri
let pair = LegacyLib.Old.Pair { left: 1, right: 2 }
let light = LegacyLib.Old.Red
let bonus = match light with
    | LegacyLib.Old.Red -> 0
    | LegacyLib.Old.Green -> 1
LegacyLib.Old.old 39 + pair.right + bonus
```

実装後の期待（列は行頭からの文字数で数えた参照の開始位置）:

| 位置 | W1005 のメッセージ | 参照の種類 |
| --- | --- | --- |
| `app/Main.tz:1:12` | `'LegacyLib.Old.Pair' is deprecated: use a tuple` | record の構築式 |
| `app/Main.tz:2:13` | `'LegacyLib.Old.Light' is deprecated: use bool` | union の case の構築式 |
| `app/Main.tz:6:1` | `'LegacyLib.Old.old' is deprecated: use LegacyLib.Old.next instead` | 関数の参照 |

- `check app` と `run app` は終了コード 0 で、`run` は `42`。`legacy-lib` の `inside` の `old x` は同じ package なので警告しない。
  4–5 行目の pattern は警告しない。`run app --deny-warnings` は終了コード 1 で、何も実行しない。
- 「再現」の case `c`（manifest なし、`old` は同じ project）は実装後も警告を出さない。
- manifest に `edition = "2026"` を足した `a` は `42`（新構文（実装後に有効。未検証））。`edition = "2030"` は
  E0002 `unsupported edition '2030'; supported editions: 2026`。

### Phase 2（設計方針）

`tsuzuri api-diff <old> <new> [--json]`（D9。承認まで着手しない）。両方の入力を `Project::load_for_docs` で読み、`src/docgen.rs` の
`render_project` と同じ宣言の走査で「完全修飾名 → 署名の文字列」の表を作って比べる。breaking は公開宣言の削除、署名の変更（型・制約・region）、
union の case の追加（網羅的な match を壊す）、record の field の追加（構築式を壊す）、既定のない class method の追加。
宣言の追加、説明の変更、`@deprecated` の追加は non-breaking。1 変更 1 行で出力し、breaking があれば終了コード 1。E10 の公開前検査に使う。

## 設計

### データ構造

```rust
// src/syntax.rs（新規）
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edition(pub(crate) u16);

impl Edition {
    pub const E2026: Edition = Edition(2026);
    pub const LATEST: Edition = Edition::E2026;
    pub const MANIFEST_DEFAULT: Edition = Edition::E2026;
    pub const SUPPORTED: &'static [Edition] = &[Edition::E2026];
    pub fn parse(text: &str) -> Option<Edition>; // SUPPORTED の中で to_string() が一致するもの
}
impl std::fmt::Display for Edition {} // 年の数字だけ（"2026"）

// src/lexer.rs
pub const EDITION_KEYWORDS: &[(&str, Edition)] = &[]; // （新規）語と予約を始める edition。Phase 1 は空
fn reserved_in(word: &str, edition: Edition, gated: &[(&str, Edition)]) -> bool; // （新規）
struct Lexer<'a> { source: &'a str, position: usize, edition: Edition } // edition は新規
pub fn lex(source: &str, edition: Edition) -> Result<Vec<Token>, Diagnostic>;
pub fn lex_all(source: &str, edition: Edition) -> (Vec<Token>, Vec<Diagnostic>);
pub fn lex_with_trivia(source: &str, edition: Edition) -> Result<Vec<TokenWithTrivia>, Diagnostic>;

// src/package.rs
pub struct Manifest { /* 既存 */ pub edition: Edition } // edition は新規

// src/driver.rs
pub struct SourceFile { /* 既存 */ pub edition: Edition } // edition は新規

// src/lib.rs と src/check.rs（どちらも既存の field の後ろに足す）
pub struct SourceInput<'a> { /* 既存 */ pub edition: Edition, pub package: Option<&'a str> }
pub struct ModuleInput<'a> { /* 既存 */ pub edition: Edition, pub package: Option<&'a str> }

// src/warnings.rs（新規）
pub(super) struct Deprecation { pub name: String, pub note: String, pub module: usize }
pub(super) fn deprecation_note(doc: &Documentation) -> Option<&str>;
pub(super) fn deprecations(modules: &[ModuleInput<'_>]) -> BTreeMap<(Option<usize>, usize), Deprecation>;
pub(super) fn deprecated_uses(
    modules: &[ModuleInput<'_>],
    index: &semantic::SemanticIndex,
    deprecations: &BTreeMap<(Option<usize>, usize), Deprecation>,
) -> Vec<Diagnostic>;
```

- `package` は `PackageId::name`（manifest の `name`）で、manifest なしの root・std・`analyze*` の入口は `None`。同じ package かどうかは
  「両方 `ModuleOrigin::User` で `package` が等しい」で判定する（`None == None` は manifest なしの一つの project）。
- `deprecations` のキーは宣言名の span の `(source, start)`。`Span` の `Ord` に頼らない。`module` は `modules` の添字で、
  `analyze_inputs_indexed_all` はソース ID と `modules` の添字を一致させている（解析に失敗すれば型検査へ進まない）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 型 | `src/syntax.rs` | `Edition`（新規） | データ構造のとおり |
| 字句 | `src/lexer.rs` | `lex`, `lex_all`, `lex_with_trivia`, `tokenize`, `Lexer` | `edition` 引数を受けて `Lexer::edition` へ渡す。既定値を持つ別の入口は作らない |
| 字句 | `src/lexer.rs` | `Lexer::identifier`, `EDITION_KEYWORDS`, `reserved_in` | 語を切り出した直後に `reserved_in(word, self.edition, EDITION_KEYWORDS)` が false なら `TokenKind::Ident` を返し、既存の `match` を通さない |
| 構文 | `src/parser.rs` | `parse_with_source`, `parse_with_source_all`, `parse_source` | `edition` 引数を足して lexer へ渡す。`parse(source)` は `Edition::LATEST` |
| manifest | `src/package.rs` | `Manifest`, `parse_manifest` | `"package"` 分岐の許可キーに `edition`。値は `Edition::parse`、失敗は `cursor.error` の E0002。省略は `Edition::MANIFEST_DEFAULT` |
| 読み込み | `src/driver.rs` | `SourceFile`, `SourceFile::read`, package のソースを集める処理（`source.package = ...` の行）, std の `SourceFile`, manifest の `SourceFile` | `edition`。package のソースは `package.manifest.edition`、それ以外は `Edition::LATEST` |
| 読み込み | `src/driver.rs` | `Project::analyze_all` | `SourceInput` に `edition: source.edition` と `package: source.package.as_ref().map(\|id\| id.name.as_str())` |
| 読み込み | `src/driver.rs` | `read_manifest`（新規）, `format_sources` | package 読み込みの manifest 検査（通常ファイル、symlink は E1011、`parse_manifest`）を `read_manifest` に切り出し、`format_sources` でも使う（D11） |
| 入口 | `src/lib.rs` | `SourceInput`, `analyze_modules_with_std_all`, `analyze_modules_with_semantics`, `analyze_inputs_indexed_all` | 入口は `Edition::LATEST` と `package: None`。解析に `input.edition`、`ModuleInput` に `edition`・`package` |
| LSP | `src/lsp.rs` | `SourceInput` を作る箇所 | `Project::analyze_all` と同じ 2 field |
| 検査 | `src/check.rs` | `ModuleInput`, 単一モジュールの入口（`check_modules(&[ModuleInput { .. }])`） | field の追加。単一モジュールの入口は `Edition::LATEST`・`None` |
| 検査 | `src/check.rs` | `check_modules_indexed_all` | モジュール名の検査を `crate::lexer::lex(name, module.edition)` に。`diagnostics.check()?` の後の `semantic` の処理で W1005 を集める（アルゴリズム） |
| 検査 | `src/check.rs`, `src/polymorph.rs` | `for &ModuleInput { .. } in modules` の分解（check 2 か所、polymorph 2 か所） | `..` がなければ足す。`src/computation.rs` は既に `..` |
| 警告 | `src/warnings.rs` | `Deprecation`, `deprecation_note`, `deprecations`, `deprecated_uses`（新規）、テストの `ModuleInput` リテラル | アルゴリズムのとおり |
| LSP 索引 | `src/semantic.rs` | `collect` の式の `target` の `match` | `TypedExprKind::Construct { .. } => type_target(&expression.ty, &types)` を足す（union の case の構築。hover と定義へ移動も union を指すようになる） |
| 整形 | `src/formatter.rs` | `format_source`, `print_tokens` | `edition` 引数。`parse_with_source` 2 回と `lex_with_trivia` に渡す |
| 文書生成 | `src/docgen.rs` | `render_project` | `parse_with_source(&source.text, id, source.edition)` |
| std | `src/stdlib.rs` | `parse_with_source(text, id)` の呼び出し | `Edition::LATEST` |
| 生成 | `src/llvm*.rs`, `src/ownership.rs` ほか | — | 変更なし |

### 生成 IR とランタイム

変更なし。runtime、`%tz.*` の型、WASM import、cache の key の構成は変えない。manifest の `edition` を変えたときに build cache を
再利用しないことは手順 5 で確かめる。手順 10 で IR が不変であることを確かめる。

### アルゴリズム

```text
reserved_in(word, edition, gated) = gated のどの (w, since) も「w == word かつ since > edition」でない

deprecation_note(doc):
    if !doc.text.contains("@deprecated") { return None }
    for line in doc.text.lines():
        rest = line.trim_start().strip_prefix("@deprecated")?
        if rest.is_empty() || rest.starts_with(' ') { return Some(rest.trim()) }
    None

deprecations(modules):
    for (id, module) in modules の添字つき:
        program.functions・records・unions（union は name.provenance == Generated を除く）の各 decl:
            note = deprecation_note(decl.doc?)?
            map[(decl.name.span.source, decl.name.span.start)] =
                Deprecation { name: module.name + "." + decl.name.text, note, module: id }

deprecated_uses(modules, index, map):
    for entry in index.entries（semantic::collect が位置順に並べ済み）:
        target = entry.target?; target == entry.span なら次へ（宣言そのもの）
        d = map.get((target.source, target.start))?
        user = modules[entry.span.source?]; user.origin != User なら次へ
        owner = modules[d.module]; owner.origin == User かつ owner.package == user.package なら次へ
        (entry.span.source, start, end) が既出なら次へ
        W1005 を entry.span に出す

check_modules_indexed_all（diagnostics.check()? の後）:
    deprecated = warnings::deprecations(modules)
    semantic が Some(index) なら: *index = semantic::collect(..); deprecated が空でなければ deprecated_uses(modules, index, ..)
    semantic が None で deprecated が空でなければ: 局所の index = semantic::collect(..) で同じことをする
```

`deprecated_uses` の結果は既存の `warnings.sort_by_key` で他の警告と位置順に並ぶ。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。package graph のテストは GUIDE §11.1 のとおり、テストごとに別の一時 root を使う。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「例」の package graph を `/tmp/tz-work-G19/e/` に作り、IR を保存する。
- 確認: `run` が `42`。IR が 4 ファイルできる。stack-depth の 3 テストがそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-work-G19/e/app
for t in native wasm32; do for o in -O0 -O3; do
  target/release/tsuzuri build /tmp/tz-work-G19/e/app --target $t --emit llvm $o -o /tmp/tz-work-G19/before-$t$o.ll
done; done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
```

### 手順 2: `Edition` と lexer の gate

- 変更: `src/syntax.rs`（`Edition`）、`src/lexer.rs`（`lex`・`lex_all`・`lex_with_trivia`・`tokenize`・`Lexer`・`Lexer::identifier`・
  `EDITION_KEYWORDS`・`reserved_in`）、lexer の呼び出し元（この手順では `Edition::LATEST` を渡す。手順 4・5 で置き換える）、
  `tests/chars.rs`・`tests/diagnostics.rs`・`tests/lists.rs`・`tests/strings.rs`・`tests/formatter.rs` の呼び出し（`Edition::LATEST` を足すだけ）。
- 内容: データ構造とアルゴリズムのとおり。lexer の単体テスト `editions_parse_and_gate_keywords`（新規）を足す。
- 確認: `cargo test --locked --lib lexer` の N に新しいテストが含まれて成功する。
  `cargo test --locked --test chars --test diagnostics --test lists --test strings --test formatter` と `cargo test --locked` が成功する。

### 手順 3: manifest の `edition`

- 変更: `src/package.rs` の `Manifest`・`parse_manifest` とその単体テスト。
- 内容: 未知キーのメッセージを `unknown package key; expected name, version, or edition` にする。`edition` は `fields` に入れて重複を検査し、
  最後に `Edition::parse` で変換する（失敗は `unsupported edition '<value>'; supported editions: 2026`、E0002、その行の span）。
  単体テスト `parses_edition_and_defaults_missing_edition`（新規）。
- 確認: `cargo test --locked --lib package` の N に新しいテストが含まれて成功する。

### 手順 4: ソースごとの edition の受け渡し（共有変更）

- 変更: 「段ごとの変更」の読み込み（`read_manifest` を除く）・入口・LSP・検査・文書生成・std の行、`src/parser.rs` の 3 関数、
  `tests/generic_records.rs` の `parse_with_source` の呼び出し。
- 内容: `SourceFile`・`SourceInput`・`ModuleInput` に field を足し、コンパイラが指す全箇所を埋める。モジュール名の検査は `module.edition`。
- 確認: `cargo test --locked --test modules --test lsp --test generic_records` と `cargo test --locked` が成功する。stack-depth の 3 テストが成功する。

### 手順 5: `tsuzuri fmt` の edition

- 変更: `src/driver.rs` の `read_manifest`（新規）・package の読み込み・`format_sources`、`src/formatter.rs` の `format_source`・`print_tokens`。
- 内容: D11。package の読み込みの manifest 検査をそのまま `read_manifest` へ移す（診断の文言を変えない）。
  あわせて build cache の key が `project.manifests` の本文を含むことを確かめる（`src/driver.rs` の `project.sources.iter().chain(&project.manifests)` の走査）。
- 確認: `cargo test --locked --test formatter --test modules` が成功する。cache の key が manifest を含まなければ停止して報告する。

### 手順 6: `tests/editions.rs`

- 変更: `tests/editions.rs`（新規）。
- 内容: テスト計画の 4 テスト。一時 root の作り方は `tests/modules.rs` の `loads_and_protects_local_package_graphs` を写す。
- 確認: `cargo test --locked --test editions` が `4 passed`。`grep -n "Edition::LATEST" src/*.rs` の結果が受け入れ条件の一覧と一致する。

### 手順 7: `@deprecated` の読み取り

- 変更: `src/warnings.rs` の `Deprecation`・`deprecation_note`・`deprecations`（新規）とテストの `ModuleInput` リテラル。
- 内容: アルゴリズムのとおり。単体テスト `deprecation_note_reads_the_first_tag_line`（新規）。
- 確認: `cargo test --locked --lib warnings` の N に新しいテストが含まれて成功する。

### 手順 8: union の case の構築の target

- 変更: `src/semantic.rs` の `collect`、`tests/lsp.rs`。
- 内容: `TypedExprKind::Construct { .. } => type_target(&expression.ty, &types)`。テスト `union_case_constructions_target_their_union`（新規）。
- 確認: `cargo test --locked --test lsp` が既存 + 1 件で成功する。既存の LSP テストの期待が変わるなら停止する。

### 手順 9: W1005

- 変更: `src/warnings.rs` の `deprecated_uses`、`src/check.rs` の `check_modules_indexed_all`、`tests/warnings.rs`。
- 内容: アルゴリズムのとおり。テスト計画の `warns_on_deprecated_uses_from_other_packages_only` と `warns_on_deprecated_std_declarations`。
- 確認: `cargo test --locked --test warnings` が `9 passed`。`cargo test --locked` が成功する。

### 手順 10: CLI と IR の不変

- 変更: `tests/warnings.rs`（`deny_warnings_fails_on_deprecated_uses`）。
- 内容: release build で手順 1 の IR を出し直して比べる。
- 確認: `cargo test --locked --test warnings` が `10 passed`。次の `cmp` が 4 回とも差分なし。`check` は W1005 を 3 件出して終了コード 0。

```sh
cargo build --release --locked
for t in native wasm32; do for o in -O0 -O3; do
  target/release/tsuzuri build /tmp/tz-work-G19/e/app --target $t --emit llvm $o -o /tmp/tz-work-G19/after-$t$o.ll
  cmp /tmp/tz-work-G19/before-$t$o.ll /tmp/tz-work-G19/after-$t$o.ll
done; done
target/release/tsuzuri check /tmp/tz-work-G19/e/app
```

### 手順 11: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/stability.md _docs/language-reference/modules-and-packages.md _docs/tools/diagnostics.md _docs/feature-status.md` が成功する。

### 手順 12: 最終確認

- 確認: GUIDE §3 の全体の検証（`cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`）が成功し、
  手順 1 の stack-depth の 3 テストが成功する。GUIDE §10 の完了の定義を満たす。

## テスト計画

### Rust テスト

- `src/lexer.rs` の `editions_parse_and_gate_keywords`: `Edition::parse("2026") == Some(Edition::E2026)`、`"2030"`・`" 2026"`・`""` は `None`、
  `Edition::E2026.to_string() == "2026"`。合成表 `[("then", Edition(2027))]` で `reserved_in("then", Edition(2026), ..)` が false、
  `Edition(2027)` と `Edition(2028)` で true、`reserved_in("fn", Edition(2026), ..)` が true。`EDITION_KEYWORDS` の各 edition が `Edition::SUPPORTED` にある。
- `src/package.rs` の `parses_edition_and_defaults_missing_edition`: `edition = "2026"` → `Edition::E2026`、省略 → `Edition::MANIFEST_DEFAULT`。
  E0002 とメッセージ: `edition = "2030"`（unsupported）、`edition = 2026`（文字列でない）、`edition = ""` と二重の `edition`（既存の文言）、
  `license = "MIT"`（`unknown package key; expected name, version, or edition`）。
- `tests/editions.rs`（新規、4 件）:
  - `manifest_edition_selects_the_source_edition`: app（`edition = "2026"`）と依存（`edition` なし）の graph で、全 `SourceFile::edition` が
    `Edition::E2026`、std は `Edition::LATEST`。manifest なしの project は `Edition::LATEST`。`analyze_all` が成功する。
  - `rejects_unknown_and_unsupported_editions`: `Project` の読み込みで上の E0002 が manifest のパスに付く。
  - `edition_keywords_are_per_package`: `EDITION_KEYWORDS` の各 `(word, since)` と `since` より古い各 edition `e` について、
    依存（`e`）が `def <word> :: i64 = 1` を持つ graph は app（`since`）から別の関数を呼べて成功し、app が `let <word> = 1` を書くと E0002。
    Phase 1 は表が空なので繰り返しは 0 回で、表の整合（各語が `lex(word, since)` で `Ident` でない）だけを検査する。
  - `fmt_uses_the_manifest_edition`: `tsuzuri fmt --check` が `edition = "2030"` の package で E0002 を manifest に出してファイルを変えない。
    `edition = "2026"` では通常どおり整形する。
- `src/warnings.rs` の `deprecation_note_reads_the_first_tag_line`: `"Old.\n@deprecated use new"` → `Some("use new")`、`"@deprecated"` → `Some("")`、
  `"  @deprecated  spaced  "` → `Some("spaced")`、`"@deprecatedX"` → `None`、`"@deprecated a\n@deprecated b"` → `Some("a")`、`"text"` → `None`。
- `tests/lsp.rs` の `union_case_constructions_target_their_union`: `union Color = Red | Green` と `let c = Red` の `Red` の entry の `target` が union の名前の span。
- `tests/warnings.rs`（既存 7 件 + 3 件）:
  - `warns_on_deprecated_uses_from_other_packages_only`: 「例」の graph で、W1005 がちょうど 3 件（行・列・メッセージは「例」の表）。
    `legacy-lib/Old.tz` への警告はない。
  - `warns_on_deprecated_std_declarations`: `analyze_modules_with_std_all` に std `std/Legacy.tz`（`@deprecated use Legacy.next` の `old`、
    note なしの `@deprecated` の `record`、`@deprecated` 付きの `const`）と利用者 `Main` を渡す。`old` と record の参照に
    1 件ずつ（note なしは `see its documentation for a replacement` の文言）、const には 0 件。利用者の別モジュールで
    `@deprecated` を付けた `def` を `Main` から呼んでも 0 件（同じ package）。
  - `deny_warnings_fails_on_deprecated_uses`: `check app --deny-warnings` が終了コード 1 で stderr に `warning[W1005]`、`--json` の行が
    `"severity":"warning"` と `"code":"W1005"` を持つ。`run app` は `42`。

### E2E

コード生成を変えないので `tests/*.mjs` の suite は足さない。手順 10 の native・wasm32 × `-O0`・`-O3` の IR の一致で代える。

### 既存テストへの影響

- 期待値の変更はない。`lex`・`lex_all`・`lex_with_trivia`・`parse_with_source` の呼び出しに `Edition::LATEST` を足すだけの機械的な変更がある
  （`tests/chars.rs`・`tests/diagnostics.rs`・`tests/lists.rs`・`tests/strings.rs`・`tests/formatter.rs`・`tests/generic_records.rs`、`src/lexer.rs` と `src/warnings.rs` の単体テスト）。
- `tests/warnings.rs` の既存 7 件、`tests/lsp.rs` の既存テストは変えずに成功する。

### 性能

`@deprecated` を含まない project で増えるのは、全宣言の説明への `contains("@deprecated")` だけである。閾値は設けない。
比べたい場合は `/usr/bin/time -l target/release/tsuzuri check <大きな project>` を before と after で 9 回ずつ測り、中央値を報告する。

## ドキュメント

- `docs/language.md`: 「ソースと宣言」の予約語の段落に「予約語はソースの edition で決まる。edition 2026 は上の一覧」を足す。
  「ドキュメントコメント」に `@deprecated` の規則と W1005。「ローカルパッケージ」の manifest の例に `edition = "2026"` と edition の決まり方の表。
  「診断」の表に `W1005`。
- `docs/architecture.md`: モジュール表の `src/lexer.rs` の行（edition の gate）、パッケージの段落（`Manifest::edition` → `SourceFile` → `SourceInput` → lexer）、
  警告の段落（W1005 は `semantic::collect` の参照と説明の対応から出す）。
- `docs/stability.md`（新規）: D7 の節立てと内容。
- `_docs/language-reference/modules-and-packages.md`: 「manifest の文法と制限」に `edition`、「ローカルパッケージ」に edition の決まり方。
- `_docs/tools/diagnostics.md`: 「警告」の表に `W1005`。
- `_docs/feature-status.md` の G19 の行と `_features/README.md` の状態欄（Phase 1 完了、Phase 2 は未着手）。
- GUIDE §9: 承認済みなら `W1005` を D-30 から D-16 へ移す。§6.1 に「G19 以後の予約語は `EDITION_KEYWORDS` にも登録する」を足す。

## 受け入れ条件

- [ ] manifest の `edition = "2026"` を受け、省略時は 2026、manifest なしと std は `Edition::LATEST` になる。未知・未対応の値は E0002。
- [ ] 全ソースが自分の edition で字句解析され、`EDITION_KEYWORDS` の語は予約を始める edition より古いソースで識別子になる（単体テストと `tests/editions.rs`）。
- [ ] `grep -n "Edition::LATEST" src/*.rs` の使用箇所が、`src/syntax.rs` の定義、`parse`、`src/lib.rs` の入口、`src/driver.rs` の std・manifest なし・`format_sources` の既定、
      `src/check.rs` の単一モジュールの入口、`src/stdlib.rs` と単体テストだけである。
- [ ] 別 package と std の非推奨の `def`・`record`・`union` の参照が W1005 になり、同じ package の参照はならない。`--deny-warnings` で失敗する。
- [ ] `@deprecated` がない project では `semantic::collect` を追加で呼ばない（`check_modules_indexed_all` の分岐で確認）。
- [ ] 「例」の graph の IR が native・wasm32 × `-O0`・`-O3` で手順 1 と一致する。
- [ ] `docs/stability.md` があり、D7 の内容を満たす。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 既定の edition を持つ lexer・parser の入口を足すと、旧 edition の package を最新の予約語表で読む誤りが compile を通る。入口は edition を必須にし、
  既定を使うのは manifest なしの入口（`parse`、`analyze*`）だけにする。受け入れ条件の grep で確かめる。
- `reserved_in` の判定は既存の予約語の `match` より前に置く。後ろに置くと gate が効かない。
- `SourceFile` は `SourceFile::read` のほかに LSP の overlay、std、manifest の 4 か所でリテラルを作る。driver は package のソースで `package` を
  後から上書きしているので、`edition` も同じ場所で上書きする。
- `semantic::collect` の `entries` は宣言そのもの（`target == span`）と局所変数も含む。除かないと宣言の位置に W1005 が出る。
- `type_entry` は std のモジュールの宣言にも走る。参照側の origin で絞らないと std 内部の参照が警告になる。
- 同じ span に複数の entry がありうる。`(source, start, end)` で重複を除く。
- 依存 package の中の非推奨 API の使用も警告になる（全 package は `ModuleOrigin::User`。W1001 と同じ扱い）。依存が API を非推奨にすると、
  利用者の `--deny-warnings` の build が失敗しうる。`docs/stability.md` に「警告は互換性の保証の対象外」と書く。
- 説明の中の code fence の行でも、最初の `@deprecated` 行はタグとして読む。文書に書く。
- `tsuzuri fmt` はファイル入力で上位の `Tsuzuri.toml` を探さない（D11）。package の下位ディレクトリのファイルは `Edition::LATEST` で読むので、
  将来の edition では新しい予約語を識別子に使うファイルが E0002 になる（書き込まないので安全側）。文書に書く。
- `vsc/` の文法は edition を知らず、全 edition の予約語を強調する。Phase 1 は変更しない。
- `edition = 2026`（引用符なし）は `Line::string` の E0002 になる。既存の `name`・`version` と同じ扱いで、テストで固定する。

## 対象外

- raw identifier（予約語をエスケープする構文）と移行ツール `tsuzuri fix --edition`（D6・D10）。
- 依存 package の警告の抑制（`--cap-lints` 相当）と、利用者が W1005 を局所的に抑制する構文。
- `const`・`type`・`class`・class の method・`extern def` の非推奨と、pattern の中の参照の W1005（D4・D5）。
- edition による std の名前の隠蔽と、edition ごとの union の case の追加（D8）。構文の非推奨の仕組み（それを最初に使う edition のチケットで作る）。
- 言語の標準化団体・規格書、LTS 版の運用体制、semver の自動検査（Phase 2 の `api-diff` 以外）。

## 決定事項

### D1: edition の名前・値・既定

- 決定: manifest の `[package]` のキー `edition`。値は年の文字列で、Phase 1 は `"2026"` だけ。`edition` のない manifest は 2026 に固定し、
  将来も変えない。manifest のない project、`analyze*` の入口、std は `Edition::LATEST`。周期は定めず、必要なときに D12 の手順で足す。
- 理由: HEAD の manifest はすべて `edition` を持たないので、その意味が compiler の更新で変わってはならない（旧案の「最新」を manifest に適用すると、
  新しい予約語で既存 package が壊れる）。manifest のない project はスクリプト・テスト・文書の例で、compiler に追従させる（旧案の「最新」をここで維持する）。
- 状態: 要承認（承認前は手順 2 以降に着手しない）

### D2: edition の表現と受け渡し

- 決定: `Edition(u16)` の newtype を `src/syntax.rs` に置き、`SourceFile`・`SourceInput`・`ModuleInput` に field で持たせる。`PackageId` には入れない。
  lexer と parser の入口は edition を必須の引数にする。
- 理由: enum にすると単体テストで架空の edition（`Edition(2027)`）を作れず、gate を検査できない。`PackageId` は並べ替えと同一性に使う。
  必須の引数にすれば、呼び出し元の漏れを compiler が見つける。
- 状態: 既定案（実装者はこの案に従う）

### D3: edition で変えてよいもの

- 決定: 「仕様」の「edition で変えてよいもの」のとおり。予約語・前の edition で非推奨にした構文の削除・警告の既定だけを変え、両方が受理するプログラムの意味は変えない。
- 理由: 異なる edition の package は一つの IR に結合される。意味が edition で変わると、package の境界で数値・所有権の規則が混在する。
- 状態: 既定案（実装者はこの案に従う）

### D4: 非推奨の印と W1005

- 決定: doc comment の `@deprecated [note]` 行（新しい構文を足さない）。Phase 1 の対象は `def`（`extern def` を除く）・`record`・`union`。
  使用は警告 `W1005`（GUIDE D-30 の仮割り当て）で、既定で有効。メッセージは「診断」の表のとおり。
- 理由: Tsuzuri には属性の構文がなく、`deprecated` 修飾子は予約語の追加になる。説明に置けば `tsuzuri doc` と hover にそのまま出る。
  三種類は `semantic::collect` が参照の target を記録する宣言で、新しい解決の経路が要らない。
- 状態: 要承認（承認前は手順 7–9 に着手しない。W1005 の確定と、説明が警告に影響するという G09 の規則の変更を含む）

### D5: W1005 の範囲と費用

- 決定: 参照が `ModuleOrigin::User` のモジュールにあり、宣言が std か別 package にあるときだけ出す。同じ span は一度だけ。
  検出は `semantic::collect` の結果を使い、`@deprecated` がない project では collect を追加で呼ばない。pattern の中の参照は対象外。
- 理由: 局所的に抑制する構文がないので、非推奨にした API を移行期間に自分の package の中で使うと、自分の `--deny-warnings` の build が失敗してしまう。
  警告の読み手は API の利用者である。collect の再利用で LSP の hover と同じ解決結果を使える。
- 状態: 既定案（実装者はこの案に従う）

### D6: 予約語と名前の到達性

- 決定: raw identifier を作らない。予約語とモジュール名の検査はソースの edition で行う。新 edition の package から、新しい予約語と同じ名前の旧 package の宣言は参照できない。
- 理由: raw identifier は新しい構文で、二つ目の edition ができるまで必要にならない。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: 最初の新しい予約語を edition に入れるとき、raw identifier の要否を人間が判断する。

### D7: 安定性方針の文書

- 決定: `docs/stability.md` に次を書く。安定: edition ごとの構文と意味、std の公開 API（削除しない）、C ABI の `tz_`／`tsuzuri_` シンボルと header、
  WASM の import 名、診断コード・severity・JSON のキー。安定しない: LLVM IR の形、内部シンボル、cache の形式、診断の文言、警告の有無
  （新しい警告は増えうる）。あわせて edition の表（D1）、非推奨の手順（`@deprecated` → 削除は edition でだけ）、D8 の例外を書く。
- 理由: 旧チケットの安定対象の一覧を保ち、`--deny-warnings` と診断の文言が保証の外であることを明示する。
- 状態: 既定案（実装者はこの案に従う）

### D8: std と edition

- 決定: std は `Edition::LATEST` で一度だけ検査し、全 edition で共有する。Phase 1 では std の公開 API を削除しない。edition による std の名前の隠蔽と
  union の case の追加は、必要になったとき別チケット（要承認）で作る。std へのモジュールの追加は edition と無関係に同名の利用者モジュールを
  E1011 にするので、`docs/stability.md` に既知の例外として書き、release notes で予告する。
- 理由: std を edition ごとに複製すると、型・instance が edition の数だけ分かれ、package の間で値を渡せない。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2 の `tsuzuri api-diff`

- 決定: 「仕様」の「Phase 2（設計方針）」のとおり。docgen の宣言の走査を共有し、比較は署名の文字列で行う。
- 理由: 新しい CLI コマンドと出力形式は公開の約束になる。Phase 1 と独立に出荷できる。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D10: 移行ツール

- 決定: `tsuzuri fix --edition` は G19 では作らない。
- 理由: Phase 1 の edition の差はない。将来の差は予約語で、公開名を改名すると依存側が壊れるため、raw identifier なしに自動移行できるのは局所名だけである。最初の edition の差を作るチケットで判断する。
- 状態: 既定案（実装者はこの案に従う）

### D11: `tsuzuri fmt` の edition

- 決定: 入力ディレクトリ（ファイル入力は親）の `Tsuzuri.toml` を `read_manifest` で読み、その edition を使う。なければ `Edition::LATEST`。上位は探さない。
- 理由: fmt は project を読み込まない。上位を探さない規則は root の決め方（ファイル入力は親を root にする）と同じである。
- 状態: 既定案（実装者はこの案に従う）

### D12: 新しい edition の作り方

- 決定: 新しい edition は人間の承認を得て一つの変更で作る。名前は作成年以上で最新の edition より大きい最小の年。`Edition` の定数、`SUPPORTED`、
  `LATEST`、`EDITION_KEYWORDS`、`docs/stability.md` の表、`tests/editions.rs` を同時に更新する。`MANIFEST_DEFAULT` は変えない。
  リリースに含めた edition は凍結し、差分を足さない。
- 理由: edition の数を抑え、予約語を足すチケットが年号を決めなくて済む。
- 状態: 既定案（実装者はこの案に従う。個々の edition の作成は要承認）
