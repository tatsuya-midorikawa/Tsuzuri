# G02: 複数エラーの同時報告
| 項目 | 内容 |
|---|---|
| ID | G02 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | なし |
| 後続 | G03, G07 |
| 状態 | done |
| 主な影響ファイル | `src/diagnostic.rs`, `src/lexer.rs`, `src/parser.rs`, `src/lib.rs`, `src/check.rs`, `src/control.rs`, `src/computation.rs`, `src/polymorph.rs`, `src/closures.rs`, `src/ownership.rs`, `src/driver.rs`, `src/main.rs`, `tests/diagnostics.rs`, `tests/modules.rs`, `tests/e2e.mjs`, `docs/language.md`, `README.md` |

## 目的

1 回の `tsuzuri check` / `tsuzuri build` で最初の 1 件だけでなく、同じプロジェクト内の独立した複数エラーを
決定的な順序で報告する。

AI やエディタ連携では、1 件修正して再実行する反復が大きなボトルネックになる。G07 LSP も
複数診断を前提にした API を必要とする。

## 現状

すべての主要段階が `Result<_, Diagnostic>` で短絡している。

| 段 | 現在の入口 / 代表関数 | 現在の停止点 |
|---|---|---|
| lexer | `lexer::lex` | `E0001` / `E0003` を最初の 1 件で返す |
| parser | `parser::parse_with_source`, `Parser::program` | `?` で最初の構文エラー停止 |
| lib | `analyze`, `analyze_modules` | parser の `collect::<Result<Vec<_>, _>>()?` で停止 |
| check | `check::check_modules` | 名前収集、型解決、各関数本体、entry、recursion、polymorph、closures、ownership のどこかで停止 |
| Checker | `Checker::expression`, `composed_expression`, `value_expression` | 1 関数内の最初の型エラーで停止 |
| control | `Checker::control_expression`, `match_value`, `pattern_alternatives` | `match` / `for` / pattern の最初のエラーで停止 |
| computation | `computation::collect`, `expand` | `.tc` 種別違反や展開エラーで停止 |
| polymorph | `specialize`, `Specializer::request`, `instantiate`, `lower` | 制約 / 特殊化の最初のエラーで停止 |
| closures | `closures::lower`, `lower_expression` | lambda lowering 中の最初のエラーで停止 |
| ownership | `ownership::check`, `check_body` | 関数順に最初の move/loan エラーで停止 |
| driver/main | `Project::analyze`, `main::print_diagnostic` | 単一 `Diagnostic` だけ表示 |

既存 API:

```rust
pub fn analyze(source: &str) -> Result<check::CheckedModule, diagnostic::Diagnostic>
pub fn analyze_modules(sources: &[(&str, &str)]) -> Result<check::CheckedModule, diagnostic::Diagnostic>
pub fn check_modules(modules: &[(&str, &Program)]) -> Result<CheckedModule, Diagnostic>
```

既存 JSON 診断は 1 オブジェクトだけを stderr に出す。

```json
{"severity":"error","code":"E1003","message":"expected i64, found bool","path":"Main.tz","span":{"start":17,"end":25,"line":1,"column":18,"end_line":1,"end_column":26}}
```

## 仕様

### 利用者向け挙動

- `check`, `build`, `run` は、コンパイル段階で見つかった複数エラーを stderr に報告する。
- 終了コードは変更しない。
  - 成功: `0`
  - ソース / 型 / I/O / LLVM / 実行時エラー: `1`
  - CLI 引数エラー: `2`
- `--json` は stderr に **1 行 1 JSON オブジェクト**で診断を出す。
- human 出力は診断ごとに空行で区切る。
- エラー数は既定で 50 件まで表示する。
- 診断収集は表示 cap とは別に、unique diagnostic の hard resource limit まで継続する。
  既定値は `MAX_UNIQUE_DIAGNOSTICS = 1000`。
- hard limit に達しなかった場合、50 件を超えた分は正確な件数で表示専用 note を 1 件出す。
  - human: `error: 12 more errors not shown`
  - JSON: `{"severity":"note","message":"12 more errors not shown"}`
- hard limit に達したため収集を打ち切った場合、件数は下限として表示する。
  - human: `error: at least 950 more errors not shown`
  - JSON: `{"severity":"note","message":"at least 950 more errors not shown"}`
- この note は `Diagnostic` ではなく、診断コードを持たない表示メッセージとする。

### 決定的な順序

診断の並び順は次のキーで安定ソートする。

1. `span.source.unwrap_or(root_source_id)`
2. `span.start`
3. `span.end`
4. `code`
5. `message`

同じ診断が複数経路で発生した場合は、完全一致
`(source, start, end, code, message)` で dedup する。

### API 互換

既存 API は壊さない。

```rust
pub fn analyze(source: &str) -> Result<CheckedModule, Diagnostic>
pub fn analyze_modules(sources: &[(&str, &str)]) -> Result<CheckedModule, Diagnostic>
```

これらは新 API を呼び、エラーがある場合は決定的順序の先頭 1 件を返す。

新 API を追加する。

```rust
pub const MAX_REPORTED_ERRORS: usize = 50;
pub const MAX_UNIQUE_DIAGNOSTICS: usize = 1000;

#[derive(Clone, Debug)]
pub struct DiagnosticSet {
    pub diagnostics: Vec<Diagnostic>,
    pub omitted: usize,
    pub omitted_is_lower_bound: bool,
}

pub fn analyze_all(source: &str) -> Result<CheckedModule, DiagnosticSet>;

pub fn analyze_modules_all(
    sources: &[(&str, &str)],
) -> Result<CheckedModule, DiagnosticSet>;
```

E02（標準ライブラリ同梱）後は、origin を持つ `ModuleInput` が `check_modules` の内部入力になる。
G02 は E02 の前後どちらにも適用できるよう、次の形を併記して実装時の現在形に合わせる。

E02 前:

```rust
pub fn check_modules_all(
    modules: &[(&str, &Program)],
) -> Result<CheckedModule, DiagnosticSet>;
```

E02 後:

```rust
pub fn check_modules_all(
    modules: &[check::ModuleInput<'_>],
) -> Result<CheckedModule, DiagnosticSet>;

pub fn analyze_modules_with_std_all(
    user_sources: &[(&str, &str)],
    std_sources: &[(&str, &str)],
) -> Result<CheckedModule, DiagnosticSet>;
```

`Project::analyze_all` は、E02 後は `Project.sources` の ordered origin-bearing source list
（user source と embedded std source の両方を含み、仮想 path / origin を持つ）をそのまま渡す。
legacy wrappers (`analyze`, `analyze_modules`, `analyze_modules_with_std`, `Project::analyze`) は、
新 API の globally sorted diagnostics の先頭 1 件を返す。

`DiagnosticSet::diagnostics` には、globally sorted / deduplicated な先頭 50 件だけを入れる。
`omitted` は表示しない unique diagnostics の件数である。`omitted_is_lower_bound == false` なら正確な件数、
`true` なら hard resource limit に達したため下限である。

実装は次のどちらかにする。

1. すべての unique diagnostics を `BTreeSet<DiagnosticKey>` に集め、段階終了後に sort/dedup/cap する。
2. `BTreeSet<DiagnosticKey>` で unique count を最後まで数えつつ、表示用には上位 50 key だけを保持する。

51 件だけ集めて停止してはならない。parser recovery も、正確な omitted 件数を約束する限り 50 件到達では止めず、
hard resource limit 到達時だけ早期停止して `omitted_is_lower_bound = true` にする。

G03 で warning を追加するため、`DiagnosticSet` の名前は error 専用にしない。
ただし G02 時点では `diagnostics` は error だけである。

### parser recovery

構文解析は top-level 宣言境界で回復する。

回復境界は delimiter nesting depth 0 で、かつ user-facing column 1（`Parser::column(span) == 0`）にある
次の token だけとする。読み飛ばし中は `()`, `[]`, `[| |]`, `{}` の nesting depth を追跡し、
depth が 0 でない位置の keyword は top-level 境界にしない。

```text
def
fn
record
class
instance
export
let
```

判定は parser 内の helper に集約する。

```rust
fn is_top_level_declaration_start(kind: &TokenKind) -> bool
```

後続チケットは新しい top-level keyword を追加したら、この helper を必ず拡張する。

| チケット | 追加 keyword | helper への追加 |
|---|---|---|
| E01 | `private` | `private def`, `private record`, `private union`, `private type` の開始 |
| A02 | `union` | `union` |
| A05 | `type` | `type` |
| D06 | `const` | `const` |
| E06 | `extern` | `extern` |
| G06 | `test` | `test` |

`and` は単独では回復境界にしない。`and` は直前の `rec` グループに依存するため、
先行宣言が壊れている状態でそこから再開すると cascade が増える。

トップレベル `let` は `Main.tz` の entry code になり得るため境界にする。
明示 `{ ... }` block 内の `let` は、たとえ column 1 にあっても delimiter depth が 0 ではないため境界にしない。

parser は宣言 1 件の解析でエラーになったら、該当エラーを記録し、次の境界まで token を読み飛ばす。
その後、次の宣言解析を続ける。

### lexer recovery

G02 の最初の実装では、parser recovery を有効にするために最小の recovering lexer を追加する。

新 API:

```rust
pub fn lex_all(source: &str) -> (Vec<Token>, Vec<Diagnostic>);
```

方針:

- `lex` は既存互換のため `Result<Vec<Token>, Diagnostic>` のまま。
- `lex_all` は可能な範囲で token を返し、エラーを `Vec<Diagnostic>` に積む。
- 不正文字は 1 文字分を読み飛ばす。
- 不正な数値 suffix は次の空白 / 記号まで読み飛ばす。
- 不正な string は次の改行または EOF まで読み飛ばす。
- 未終端 block comment は EOF で 1 件だけ報告し、`End` token を返す。
- source size 超過 `E0003` は構造全体が信用できないため、従来どおり 1 件で停止してよい。

通常の parser `parse_with_source` は `lex` を使い続ける。
`parse_with_source_all` だけが `lex_all` を使う。

### per-function checking

型検査は、可能な限り関数単位で継続する。

1. 全モジュールのモジュール名、record 名、function 名を先に収集する。
2. record layout と関数シグネチャを先に解決する。
3. シグネチャ解決に失敗した関数は **poisoned** とする。
4. poisoned 関数の本体は検査しない。
5. poisoned 関数を呼び出す側では、その呼び出しに起因するエラーを抑制する。
6. シグネチャが有効な関数は、他の関数の本体エラーに関係なく検査する。
7. entry code も 1 つの synthesized function として同じ扱いにする。

poisoning のために内部型を追加する。

```rust
enum FunctionSlot {
    Valid(Scheme),
    Poisoned { span: Span },
}

enum Type {
    // 既存 variant...
    Error,
}

enum TypedExprKind {
    // 既存 variant...
    Error,
}
```

`Type::Error` の規則:

- `Inference::unify` はどちらかが `Type::Error` なら成功として扱う。
- `Type::display` は `"an erroneous type"` を返すが、通常の利用者診断には出さない。
- `Type::{is_copy, needs_drop, contains_reference, carries_loans, can_capture, can_send, exportable}` は
  cascade を避ける保守的な値を返す。
  - `is_copy = true`
  - `needs_drop = false`
  - `contains_reference = false`
  - `carries_loans = false`
  - `can_capture = true`
  - `can_send = true`
  - `exportable = false`
- `validate_size` / `layout_size` は `Type::Error` を 0 または 8 bytes として扱い、追加診断を出さない。
- `llvm` に到達してはならない。`CheckedModule` を返すのは error 0 件のときだけ。

この design により、ある関数の signature / body failure が別関数の型検査を止めない。

`Type::Error` を追加する実装チェックリスト（GUIDE §6.3 の Type 追加時チェックリストに基づく）:

- `check.rs`
  - `Type::display`, `is_copy`, `needs_drop`, `contains_reference`, `contains_mutable_reference`,
    `carries_loans`, `can_capture`, `can_send`, `exportable`。
  - `layout_size`, `validate_size`, `record_size`（`Type::Error` を含む field で追加診断を出さない）。
  - `Checker::call_signature`（callee が `Type::Error` なら `count` 個の `Type::Error` parameter と `Type::Error` result を返す）。
  - `Checker::annotation` / `resolve_type`（poisoned signature 内の失敗を `Type::Error` に変換し、原因診断だけ残す）。
  - `Checker::value_expression` の call / field / index / cast / binary / unary branches
    （operand が `Type::Error` なら secondary diagnostics を出さず `TypedExprKind::Error` を伝播）。
  - `Checker::name` / `Checker::function`（poisoned function 参照を `TypedExprKind::Error` と `Type::Error` にする）。
- `polymorph.rs`
  - `map_type`, `substitute`, `bounded_type`, `variables`。
  - `Inference::resolve`, `Inference::unify`, `default_numeric`。
  - `require_concrete`（`Type::Error` は concrete 不足として扱わない）。
  - `Classes::validate`, `Classes::intrinsic`, `Checker::require`, `Checker::builtin`, `Checker::function`,
    `Checker::method`, `Specializer::request`, `expression_types`, `captures`。
- `ownership.rs` / `ownership_control.rs`
  - error 0 件の module だけが渡る前提を assert するか、`TypedExprKind::Error` を no-op として扱う。
- `closures.rs`, `recursion.rs`, `call_specialization.rs`
  - `TypedExpr::children` / `children_mut` に `TypedExprKind::Error` を追加し、walk で panic しない。
- `llvm.rs` / `llvm_control.rs` / `llvm_frame.rs`
  - `Type::Error` / `TypedExprKind::Error` が到達したら internal bug として明示的に error にする。
- A01/A02 後:
  - generic records の `Type::Record(id, args)` と unions の `Type::Union(id, args)` の traversal に
    `Type::Error` を含む型引数が混ざっても secondary diagnostics を出さない。

### ownership

所有権検査は関数ごとに独立して実行する。

既存:

```rust
pub fn check(module: &CheckedModule) -> Result<(), Diagnostic>
```

追加:

```rust
pub fn check_all(module: &CheckedModule) -> Vec<Diagnostic>;
```

`check` は `check_all` の先頭 1 件を返す互換 wrapper にする。
`check_all` は `check_body` を関数ごとに呼び、失敗しても次の関数へ進む。

関数本体に `Type::Error` が残る状態では ownership に渡さない。
G02 の `CheckedModule` は error 0 件の時だけ返るため、ownership に渡る module は通常どおり well-typed である。

### CLI 出力

`src/main.rs` の `print_diagnostic` を単体用として残し、集合用を追加する。

```rust
fn print_diagnostics(
    diagnostics: &[Diagnostic],
    omitted: usize,
    project: &Project,
    json: bool,
)
```

`Project::source_for(&Diagnostic)` を既存どおり使う。
CLI 引数エラーは project がないので従来どおり 1 件だけ表示する。
I/O 読み込みエラーも `Project::load` 前に発生するため従来どおり 1 件だけでよい。

## 設計

### 新しい内部補助型

```rust
pub struct DiagnosticSet {
    pub diagnostics: Vec<Diagnostic>,
    pub omitted: usize,
}

impl DiagnosticSet {
    pub fn new() -> Self;
    pub fn push(&mut self, diagnostic: Diagnostic);
    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>);
    pub fn sort_dedup_and_cap(&mut self);
    pub fn first(self) -> Option<Diagnostic>;
    pub fn is_empty(&self) -> bool;
}
```

`sort_dedup_and_cap` は source id が `None` の診断を root source として扱うため、
CLI 側では root id を渡せる overload を用意してもよい。
内部では `DiagnosticKey { source, start, end, code, message }` の `BTreeSet` を持ち、
表示用の `diagnostics` を生成する前に必ず unique 化する。hard limit 到達時だけ
`omitted_is_lower_bound` を true にする。

### parser API

```rust
pub fn parse_with_source_all(source: &str, source_id: usize) -> Result<Program, Vec<Diagnostic>>;
```

戻り値の意味:

- エラー 0 件: `Ok(Program)`
- エラーあり: recovery 後に `Program` 断片があっても、型検査へは進めず `Err(diagnostics)` を返す

parser recovery の目的は「同じファイル内の構文エラーを複数出す」ことであり、
壊れた AST を後段へ渡して型 cascade を出すことではない。

### check API

`check_modules` は互換 wrapper とする。

```rust
pub fn check_modules(modules: &[(&str, &Program)]) -> Result<CheckedModule, Diagnostic> {
    check_modules_all(modules).map_err(|set| set.first().expect("nonempty"))
}

pub fn check_modules_all(modules: &[(&str, &Program)]) -> Result<CheckedModule, DiagnosticSet>;
```

E02 後の実装では同じ wrapper 方針で、引数型だけ `&[ModuleInput<'_>]` に変える。
`analyze_modules_with_std_all(user, std)` は user と std を E02 の規則どおり連結し、
source id / origin / virtual path を保った `ModuleInput` 配列を `check_modules_all` に渡す。

`check_modules_all` は次の段階で診断を集める。

1. module 名検査。
2. `.tt` / `.tc` 種別検査（`computation::collect_all`）。
3. record 宣言・field 型解決。
4. function 名登録。
5. signature 解決と poison 登録。
6. active pattern signature 検査。
7. function body 型検査（valid signature のみ）。
8. entry 型検査。
9. recursion check。
10. polymorph specialize。
11. closures lower。
12. ownership check。

後段に進む条件:

- module 名 / record 名 / function 名など、名前空間構築そのものが壊れている場合は、
  後段で大量 cascade になるため、その段階の診断を返して停止する。
- シグネチャ単位の failure は poison で局所化できるため、同じ段階内では継続する。
- body 型検査 failure はその関数だけで止め、他関数へ進む。
- `polymorph::specialize` / `closures::lower` / `ownership::check_all` は、
  型検査 error が 0 件の場合だけ実行する。

### cascade suppression

抑制規則:

- poisoned function 参照は追加診断を出さず `Type::Error` を返す。
- `Type::Error` を含む call / field / index / binary / cast は、すでに原因があるとみなし追加診断を出さない。
- 関数本体で最初に出た原因診断は残す。
- 同じ span で同じ code/message は dedup する。
- 不正 module 名で source id が不明な場合は、`Span::default().in_source(source_index)` を使う。

### 前提とする他チケットのインターフェース

なし。

### 他チケットへの提供インターフェース

- G03 は `DiagnosticSet` と CLI の複数行 JSON 出力を warning 表示に再利用する。
- G07 は `analyze_modules_all` を LSP diagnostics の入口にする。
- A03 が match exhaustiveness で `E1021` / `W1003` を出す場合、`DiagnosticSet` へ追加する。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer | `lex_all` を追加。既存 `lex` は互換維持 |
| parser | `parse_with_source_all` と top-level recovery を追加。既存 parser は互換維持 |
| check | `check_modules_all`, poison, `Type::Error`, `TypedExprKind::Error` を追加 |
| computation | `collect_all` / `expand` の診断を `DiagnosticSet` に集約できるようにする。展開は body 単位で失敗を局所化 |
| control | `match_value` / `pattern_alternatives` は関数内では従来どおり first error。関数境界で継続 |
| polymorph | 型検査 error が 0 件の場合だけ実行。必要に応じて `specialize_all` は追加しない |
| closures | polymorph 成功後だけ実行。変更最小 |
| ownership | `check_all` を追加し、関数単位で継続 |
| llvm | 変更なし。error 0 件の `CheckedModule` だけを受け取る |
| runtime | 変更なし |
| driver | `Project::analyze_all` を追加 |
| main | 複数診断の表示、JSON lines、cap note を追加 |

## 実装手順

1. **`DiagnosticSet` を追加する。**
   - `src/diagnostic.rs` に構造体と sort/dedup/cap helper を追加する。
   - 既存 `Diagnostic::new`, `render`, `json` は変更しない。
   - 確認: `cargo test --locked --test diagnostics`。GUIDE §3 の通り `--test <ファイル名>` を使い、
     `running N tests` の N が 0 でないことを確認する。

2. **lexer recovery を追加する。**
   - `lexer::lex_all` を追加する。
   - `lex` は `lex_all` を呼んで、診断があれば先頭を返す形にしてもよい。
   - source size 超過だけは token stream を返さない。
   - 確認:
     - `@ @` で `E0001` が 2 件。
     - `"unterminated\n@` で string error と `@` error が出る。
     - 既存 `lexer` tests が通る。

3. **parser recovery を追加する。**
   - `parse_with_source_all` を追加する。
   - `Parser::program` の top-level loop で宣言解析 1 件を helper に分ける。
   - エラー時に `recover_top_level_boundary` を呼ぶ。この helper は delimiter nesting depth を追跡し、
     depth 0 かつ column 1 の `is_top_level_declaration_start` にだけ resync する。
   - `Span::source` をすべて source id に設定する。
   - 確認:
     ```text
     fn bad = 
     fn also_bad = 
     record R { x: i64 }
     ```
     で parser error が 2 件以上、record 以降へ recovery すること。
   - `{` で始まる explicit block の内部に column 1 の `let` を置いた入力を用意し、
     その `let` に resync しないことを確認する。

4. **`analyze_all` / `analyze_modules_all` を追加する。**
   - `src/lib.rs` に新 API を追加する。
   - 既存 `analyze` / `analyze_modules` は先頭 error を返す wrapper にする。
   - E02 が既に実装済みなら、`analyze_modules_with_std_all(user_sources, std_sources)` と
     `check_modules_all(&[ModuleInput])` の形で追加し、`Project::analyze_all` は ordered origin-bearing source list を使う。
   - parse error がある source が 1 つでもあれば、後段へ進まず parser 診断集合を返す。
   - 確認: 既存 tests の `analyze(...).unwrap_err()` がそのまま動く。

5. **signature poison を導入する。**
   - `check_modules_all` の signature 解決段階で `FunctionSlot` を使う。
   - invalid signature の関数名は名前空間に残すが、`Scheme` は `Poisoned` にする。
   - `Checker::name` / `Checker::function` が poisoned id を見たら `Type::Error` / `TypedExprKind::Error` を返す。
   - 確認:
     - `def bad :: Missing -> i64` と、別関数の `true + 1` が同時に出る。
     - `bad 1` を呼ぶ関数で、`bad` 由来の cascade が出ない。

6. **関数本体の型検査を継続する。**
   - `for function_declarations` loop で 1 関数ごとに `Result` を捕捉する。
   - 失敗した body は module へ入れないか `Type::Error` body にし、後段へ進まない。
   - body error が 1 件以上あれば、polymorph へ進まず診断集合を返す。
   - 確認:
     - `fn a = true + 1`
     - `fn b = "x" + 1`
     - `fn c = 42`
     - で `a` と `b` の型エラーが両方出る。

7. **ownership を関数単位で継続する。**
   - `ownership::check_all` を追加する。
   - `ownership::check` は wrapper にする。
   - G02 の `check_modules_all` は polymorph/closures 成功後に `check_all` を呼ぶ。
   - 確認:
     - 2 つの関数で move-after-use がある場合、`E1012` が 2 件出る。

8. **CLI を複数診断対応にする。**
   - `Project::analyze_all` を追加する。
   - `main` の compile flow は `analyze_all` を使う。
   - `check` は analyze errors だけ出して exit 1。
   - `build` / `run` は analyze errors がある場合、LLVM / native 実行へ進まない。
   - `--json` は 1 行 1 JSON。
   - 確認: `tests/e2e.mjs` に JSON lines test を追加する。

9. **cap と deterministic ordering を実装する。**
   - 50 件を超えるエラーを作る fixture を Rust test で生成する。
   - 2 回実行して同じ順序であることを確認する。
   - hard limit 未満では `omitted` note が exact な human / JSON になることを確認する。
   - hard limit 到達時だけ `at least N more errors not shown` になることを確認する。

## テスト計画

### Rust tests

新規 `tests/diagnostics.rs` を追加する。

受理:

```rust
#[test]
fn analyze_keeps_legacy_first_error() {
    let error = analyze("fn a() -> i64 { true }\nfn b() -> i64 { false }").unwrap_err();
    assert_eq!(error.code, "E1003");
}
```

複数 parser error:

```text
fn a =
record R { x: i64 }
fn b =
```

期待:

- `analyze_all` は `Err(set)`。
- `set.diagnostics.len() >= 2`。
- すべて `E0002`。
- source id は `Some(0)`。

複数関数 body error:

```text
def a :: i64
fn a = true

def b :: i64
fn b = "text"
```

期待:

- `E1003` が 2 件。
- offset 順。

signature poison と cascade suppression:

```text
def bad :: Missing -> i64
fn bad x = x

def caller :: i64
fn caller = bad 1

def independent :: i64
fn independent = true
```

期待:

- `Missing` の `E1004`。
- `independent` の `E1003`。
- `caller` の `bad 1` から `E1006` / `E1003` などを出さない。

ownership per function:

```text
def consume :: string -> unit
fn consume s = ()

def a :: unit
fn a = { let s = "a"; consume s; consume s }

def b :: unit
fn b = { let s = "b"; consume s; consume s }
```

期待:

- `E1012` が 2 件。

cap:

- 60 個の `def fN :: i64` と `fn fN = true` の組を生成。
- 表示される診断は 50 件。
- `omitted == 10`。
- hard limit を小さくした test-only 設定で `omitted_is_lower_bound == true` と
  `at least N more errors not shown` を確認する。

Type::Error propagation:

- poisoned value を call / field / index / cast / binary / unary / annotation / class constraint / generic function reference に流し、
  原因診断以外の secondary diagnostics が出ないことを確認する。
- `Type::Error` を含む array / list / tuple / function / reference / record field 相当の traversal で panic しないことを確認する。
- A01/A02 後は generic record / union の型引数に `Type::Error` が混ざるケースを追加する。

### CLI / JSON tests

`tests/e2e.mjs` に追加。

```js
const result = cli(["check", invalid, "--json"], { success: false });
const lines = result.stderr.trim().split("\n").map(JSON.parse);
assert(lines.length >= 2);
assert(lines.every((line) => line.severity === "error" || line.severity === "note"));
```

Node E2E を走らせる直前は GUIDE §3 に従い必ず release binary を更新する。

```sh
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
```

human:

- stderr に複数の `error[E...]` が出る。
- 空行区切りで読みやすい。

### 既存挙動維持

- 既存 `tests/*.rs` は `analyze` / `analyze_modules` の first error API を使い続けられる。
- 既存 JSON single diagnostic tests は、1 件だけの入力では同じ JSON object を返す。
- exit code は既存と同じ。

## ドキュメント

- `docs/language.md` の「診断」を更新する。
  - 「検査は最初のエラーで停止します」を「可能な範囲で複数エラーを報告します」に変更。
  - cap 50 と note を明記。
  - `--json` は JSON lines であることを明記。
- `README.md` の CLI 節に複数診断の例を追加する。
- `docs/architecture.md` のフロントエンド不変条件に、diagnostic ordering と source id ordering を追記する。

## 受け入れ条件

- [x] `analyze` / `analyze_modules` の public signature が変わらない。
- [x] `analyze_all` / `analyze_modules_all` が追加される。
- [x] `tsuzuri check --json` が複数エラーを JSON lines として出す。
- [x] human 出力が複数診断を安定順で出す。
- [x] parser は top-level 宣言境界で recovery する。
- [x] parser recovery は delimiter nesting depth 0 の境界にだけ resync し、explicit block 内の column-1 `let` を top-level と誤認しない。
- [x] 関数 A の body error が関数 B の検査を止めない。
- [x] signature failure の関数は poisoned になり、caller 側 cascade が抑制される。
- [x] `Type::Error` が全 Type traversal / property / call branch を通っても secondary diagnostics や panic を出さない。
- [x] ownership error が関数ごとに複数報告される。
- [x] 診断は source id / offset で決定的に並ぶ。
- [x] 50 件 cap と exact omitted note、および hard limit 時の lower-bound note が動く。
- [x] exit code は既存仕様と同じ。
- [x] 既存 tests が通る。

## 落とし穴

- 壊れた AST を型検査へ渡すと cascade が爆発する。parser recovery は parser 診断収集専用にする。
- function name collection 前に body を検査すると、宣言順依存の診断になる。
- poisoned function を `unknown function` として扱うと、原因診断のほかに caller cascade が出る。
- `Type::Error` を LLVM へ到達させてはいけない。
- `BTreeMap` / `BTreeSet` を維持し、HashMap の反復順に診断順を依存させない。
- `--json` で複数 JSON object を 1 つの配列にしない。既定は JSON lines。
- 50 件だけ収集して止めると exact omitted count を出せない。表示 cap と収集 hard limit を分離する。
- parser recovery で delimiter depth を見ないと、explicit block 内の column-1 `let` へ誤って resync する。
- cap note に新しい診断コードを割り当てない。台帳未登録コードを増やさないため、表示専用 note にする。

## 対象外

- warning の導入（G03）。
- match exhaustiveness（A03）。
- LSP server（G07）。
- 1 関数内での完全な型エラー recovery。G02 は関数境界の継続を主目的にする。
- LLVM / runtime の複数エラー化。LLVM tool failure は従来どおり 1 件。
- 実行時 trap の複数報告（G04）。

## 未決事項

- **`Type::Error` の公開可視性。** 既定案は `pub enum Type` に variant を追加するが、
  `CheckedModule` が成功時だけ返るため、外部利用者が通常観測することはない。
- **cap の設定変更手段。** 既定案は const 50 固定。CLI option は追加しない。
  内部 hard limit は `MAX_UNIQUE_DIAGNOSTICS = 1000` とし、test-only で小さくできる helper を用意する。
- **lexer recovery の範囲。** 既定案は parser recovery に必要な最小限。不正 UTF-8 は `driver::read_source` で止まるため複数化しない。
- **台帳の見直し提案:** なし。

### 実装時の判断（G02）

- 表示する `DiagnosticSet` と、最大 1000 件を保持する内部 `Diagnostics` を分離した。
  順序付きキーで重複除去後に表示 50 件へ絞り、省略数と下限フラグを公開する。CLI の上限変更オプションは追加しない。
  小さいテスト専用上限ではなく、実際の 1000 件を超える入力で下限 note まで検査する。
- lex／通常の parser API は先頭エラーの形を保ち、解析の既存 API は新しい集合の先頭へ委譲する。
  `lex_all` は回復して複数字句診断を集めるが、そのファイルを parser へは渡さず、削除 token 由来の構文 cascade を防ぐ。
- parser は宣言ごとに Result を捕捉し、消費済み delimiter も含めて境界を探索する。
  不正な fn／def を定義名集合へ早期登録しない。構文エラーがあれば部分 AST の signature pairing や型検査をしない。
- `FunctionSlot` と別の Scheme 表現を並立させず、Error 返却型の Scheme を poison の唯一の表現とした。
  `Type::Error`／`TypedExprKind::Error` と checker の taint によって、派生式や active recognizer の二次診断を抑制する。
  有効な関数の第一の原因診断は残し、他関数・entry の検査を続ける。完全な関数内回復は対象外。
- 所有権の最終検査だけでなく、単相化に先行する Copy 推論のエラーも集約する。
  推論を二重実行せず結果を特殊化へ渡し、`ownership::check`／`check_all` も公開 API にした。
- CLI は解析エラー時に生成・実行へ進まず、human は空行区切り、JSON は診断／省略 note の一行ごとのオブジェクトとする。
  引数・I/O・LLVM ツール・実行時の単一エラー形式と終了コードは維持する。
