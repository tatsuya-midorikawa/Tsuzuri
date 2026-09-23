# G03: 警告（未使用・到達不能・シャドーイング）
| 項目 | 内容 |
|---|---|
| ID | G03 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | G02, 弱依存 A03, 弱依存 E01（`W1002` の非公開判定） |
| 後続 | G07 |
| 状態 | todo |
| 主な影響ファイル | `src/diagnostic.rs`, `src/lib.rs`, `src/check.rs`, `src/control.rs`, `src/ownership.rs`, `src/main.rs`, `src/driver.rs`, `tests/warnings.rs`, `tests/e2e.mjs`, `docs/language.md`, `README.md` |

## 目的

コンパイルは成功するが意図しない可能性が高いコードを、安定した warning code で報告する。
最初の対象は未使用、到達不能 match 節、紛らわしいシャドーイングである。

G02 の複数診断基盤を使い、warning は error と同じ source span / JSON lines / 決定的順序で出す。
`--deny-warnings` を指定した場合だけ warning を失敗扱いにする。

## 現状

`Diagnostic` は error 専用の構造である。

```rust
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}
```

`Diagnostic::render` は常に `error[...]` を出し、`Diagnostic::json` は常に
`"severity":"error"` を出す。

`src/main.rs` は tool cleanup warning だけを `W2001` として ad-hoc に出す。

```rust
{"severity":"warning","code":"W2001","message":...}
```

言語警告の収集場所はまだない。

既存の型付き IR は warning の材料を持つ。

| 対象 | 既存の情報 |
|---|---|
| ローカル未使用 | `Local { id, name, span }`, `TypedExpr::children`, `ownership::count_uses`, `ownership::uses` |
| 捕捉 | `TypedExprKind::Lambda { captures, .. }`, `closures::free_locals` |
| guard 内使用 | `TypedMatchArm.guard`, `PatternStep::Test`, `TypedExpr::children` |
| generated names | 現状は `$match...`, `$iteration...`, `$recognizer...`, `$instance...`, `$fx...` などの名前で生成物を区別しているが、`closures::lower` は `$export` bridge と `arg{id}` parameter、`polymorph::intrinsic_function` は `$intrinsic` function と `arg{id}` parameter も作るため、warning 判定は名前推測ではなく明示 provenance が必要 |
| 到達不能 match 節 | A03 が `E1021` / `W1003` の解析を追加予定 |
| private 未使用 | E01 が `private` 可視性を追加予定。現時点の `export` は host ABI 指定であり、モジュール間可視性ではない |

## 仕様

### 診断コード

D-16 の割り当てに従う。

| コード | 意味 | 既定 severity | 既定有効 |
|---|---|---|---|
| `W1001` | 未使用のローカル束縛 | warning | 有効 |
| `W1002` | 未使用の非公開関数・型 | warning | 有効。ただし E01 の `private` metadata がない場合は出さない |
| `W1003` | 到達しない `match` 節 | warning | 有効。A03 の解析がある場合に出す |
| `W1004` | 同じスコープ内での紛らわしいシャドーイング | warning | 既定無効 |

`W2001` は既存の tool cleanup warning として維持し、言語 warning には使わない。

### severity

`Diagnostic` に severity を追加する。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}
```

互換のため `Diagnostic::new(code, message, span)` は `Severity::Error` を作る。
warning 用に `Diagnostic::warning(code, message, span)` を追加する。

`render` は severity に応じて `error[...]` / `warning[...]` を出す。
`json` は `"severity":"error"` / `"severity":"warning"` を出す。

### CLI

新 option:

```text
--deny-warnings
```

適用対象:

- `check`
- `build`
- `run`

挙動:

- warning だけなら、通常は exit code 0。
- `--deny-warnings` がある場合、warning が 1 件以上あれば exit code 1。
- `--deny-warnings` による失敗でも、warning の severity は JSON 上 `"warning"` のまま。
- 引数エラーではなくソース診断扱いなので exit code 1。
- `--deny-warnings` は `--json` と併用可。

G02 の cap 50 は error + warning の合計に適用する。
error がある場合、`--deny-warnings` の有無に関係なく exit code 1。

### 抑制規則

#### `_` prefix

次のローカル名は `W1001` / `W1004` の対象外。

```text
_
_name
__generated_for_user
```

`$` で始まる名前や `arg0` のような見た目から compiler-generated と推測してはならない。
G03 では function / binding に明示的な provenance を追加し、`User` の宣言位置だけを warning 対象にする。

#### std module

標準ライブラリ導入後（E02 / D07）、std 埋め込み module の warning は既定で利用者に出さない。

内部表現として `SourceFile { is_std: bool }` または `Program.source_kind` とは別の source origin を持つ。
G03 実装時点で std infrastructure がまだない場合は、抑制 hook だけ用意し、
通常ユーザー source には影響させない。

#### generated code

生成コードの抑制は名前ではなく明示 provenance で行う。

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    User,
    Generated(GeneratedKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratedKind {
    Entry,
    ComputationExpansion,
    PatternLowering,
    FxParameter,
    InstanceMethod,
    LambdaLift,
    BuiltinWrapper,
    ExportBridge,
    IntrinsicWrapper,
    TestRunner,
}
```

`CheckedFunction` では GUIDE D-22 の `origin: FunctionOrigin` の `provenance` フィールドとして持たせる（独立したフィールドを
別に作らない。`FunctionOrigin` が未導入なら D-22 の定義どおりに導入する）。`Local` と pattern binding の計画
（`PatternStep::Bind` / `TypedMatchArm` 内の binding）には `provenance: Provenance` を追加する。
lint は `Provenance::User` の宣言 site だけを見る。

現行実装で `Generated` にする代表例:

- `$match...`
- `$iteration...`
- `$recognizer...`
- `$fx...`
- `$enumerable`
- `$entry`
- `$active...`
- `$instance.<id>.<method>`
- `$lambda` / `$task` / `$builtin` module の synthetic function
- `closures::lower` が作る `$export` bridge とその `arg{id}` parameter
- `polymorph::intrinsic_function` が作る `$intrinsic` function とその `arg{id}` parameter
- computation expression expansion が作る `$` names

warning は利用者が書いた span に紐づく `Provenance::User` の宣言だけに出す。

### W1001: 未使用のローカル束縛

対象:

- 関数引数。
- `let` / block binding。
- pattern binding。
- `for` の loop local。
- `fx` の pattern から生成された user-visible binding。

対象外:

- `_` prefix。
- `$` generated name。
- `mut` かどうかは無関係。
- `let _ = expr` は expr の評価意図が明確なので warning なし。

「使用」と数えるもの:

- 値として読む。
- move する。**moved-only uses count as uses**。
- `&x` / `&mut x` で借用する。
- field / index / length の receiver として使う。
- closure / task / lambda に捕捉される。
- match guard で使う。
- pattern guard / `PatternStep::Test` の中で使う。
- `return` / block result に現れる。

使用と数えないもの:

- binding 自身の定義位置。
- shadowing された後の同名別 local。
- compiler-generated projection binding。

診断 span:

- binding 名の `Local.span`。

メッセージ:

```text
unused local '<name>'; prefix it with '_' to silence this warning
```

### W1002: 未使用の非公開関数・型

対象:

- E01 の `private` function。
- E01 の `private` record / union / type alias。
- 将来の std private helper。

対象外:

- `export def`。これは host ABI entry なので、Tsuzuri 内で未使用でも warning なし。
- public module function / public type。Tsuzuri では `export` なしでも他モジュールから参照できるため、E01 なしでは非公開扱いにしない。
- instance method。型クラス dispatch で間接的に参照されるため、この warning では扱わない。
- active pattern function。pattern から参照される別 namespace を持つため、専用到達性で扱う。
- `$instance`, `$lambda`, `$builtin` synthetic function。

E01 が未実装の場合、W1002 は発生しない。
これはチケット依存表にはない弱依存であり、未決事項に記録する。

到達性 root:

- すべての public callable。
- すべての public type（record / union / type alias）。
- `entry`。
- `export def`。
- instance implementations（型クラス dispatch から参照されるため、private でも root 扱いにする）。
- active pattern の実装関数（pattern namespace から参照されるため root 扱い）。
- G06 の test declarations と test runner が選択する callable。
- public callable / type / tests / instance / active pattern から辿れる private callable / type。

使用 edge:

- `TypedExprKind::Function(FunctionRef::User(id))`。
- `GenericFunction(id, _)`。
- `Method` / class implementation から concrete method id に解決される参照。
- type annotation / field / signature / generic instantiation に現れる private type。
- export wrapper からの参照。
- test runner（G06）からの test helper 参照。

warning は「この full root set から到達不能な private node」にだけ出す。
public node は未参照でも API とみなし warning 対象外。

診断 span:

- 宣言名の span。

メッセージ:

```text
unused private function '<Module.name>'; remove it or make it public
unused private type '<Module.Name>'; remove it or make it public
```

### W1003: 到達しない match 節

対象:

- `match value with | ...` の arm。
- function guard の arm。
- `for` / `fx` pattern mismatch 用に合成された single-arm match は対象外。

A03 が exhaustiveness / usefulness 解析を導入している場合、その結果を warning として受け取る。
G03 単独では、既存の単純な明らかに到達不能な場合だけ実装してよい。

G03 単独 phase 1 の最小検出:

- 先行 arm が guard なし wildcard / `otherwise` である。
- その後の arm はすべて到達不能。
- 先行 OR pattern の中に wildcard があり guard がない。

診断 span:

- 到達不能 arm の `MatchArm.span`。

メッセージ:

```text
unreachable match arm; a previous pattern already matches these values
```

### W1004: 紛らわしいシャドーイング

既定無効。G03 では infrastructure と tests だけ追加し、CLI option は導入しない。
有効化は将来 `--warn-shadowing` または設定ファイルで行う。

定義:

- 同じ lexical scope 内で、既存の user binding と同じ名前を再束縛した場合。
- ただし Tsuzuri は shadowing を許す言語なので、既定では noise を避ける。

対象外:

- inner block での一般的な shadowing。
- `_` prefix。
- pattern OR の同名束縛（仕様上同一 binding）。
- generated `$` names。

phase 1 では `WarningOptions { shadowing: bool }` を内部に持ち、テストからだけ true にできるようにする。

## 設計

### warning の収集場所

G02 後の `CheckedModule` に warning channel を持たせる。

```rust
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
    pub warnings: Vec<Diagnostic>,
}
```

A03 が先に `CheckedModule.warnings: Vec<Diagnostic>` を導入している場合:

- 同じ field 名と型を使う。
- A03 の warning を上書きせず extend する。
- field が `pub(crate)` なら G03 で `pub` にするか accessor を追加する。

A03 が別名 channel を導入している場合:

- 既定案は `CheckedModule.warnings` に統一する。
- 既存 field は削除せず、一時的な変換 layer を置いて段階的に整理する。

### public API

G02 の `DiagnosticSet` を warning 対応にする。

```rust
pub struct Analysis {
    pub module: CheckedModule,
    pub diagnostics: Vec<Diagnostic>, // warnings only on success
}
```

ただし G02 で `Result<CheckedModule, DiagnosticSet>` を採用している場合、互換を壊さないため次を追加する。

```rust
pub fn analyze_with_diagnostics(source: &str) -> Result<(CheckedModule, Vec<Diagnostic>), DiagnosticSet>;
pub fn analyze_modules_with_diagnostics(
    sources: &[(&str, &str)],
) -> Result<(CheckedModule, Vec<Diagnostic>), DiagnosticSet>;
```

既存 `analyze` は warning を無視して `CheckedModule` だけ返す。
CLI は `*_with_diagnostics` を使う。

### warning pass

`src/check.rs` に `warnings` submodule を追加するか、`src/warnings.rs` を追加する。

推奨:

```rust
pub(crate) struct WarningOptions {
    pub shadowing: bool,
    pub std_source_ids: BTreeSet<usize>,
}

pub(crate) fn collect_warnings(
    module: &CheckedModule,
    options: &WarningOptions,
) -> Vec<Diagnostic>;
```

ただし shadowing は型検査中の lexical scope 情報が必要なので、次の 2 系統に分ける。

1. 型検査中にしか分からない warning:
   - W1004 shadowing。
2. 型付き IR 後で分かる warning:
   - W1001 local usage。
   - W1002 private reachability。
   - W1003 simple unreachable arms / A03 results。

### W1001 algorithm

関数ごとに `BTreeMap<usize, LocalInfo>` を作る。

```rust
struct LocalInfo {
    local: Local,
    user_visible: bool,
    used: bool,
}
```

収集:

- `CheckedFunction.parameters`
- `TypedExprKind::Block.bindings`
- `TypedExprKind::Lambda.parameters`
- `TypedExprKind::ForRange.local`
- `TypedExprKind::ForEach.local`
- `TypedExprKind::Match.local` は `$match` なので対象外。
- `TypedMatchArm.alternatives.bindings` の `Local`。
- `PatternStep::Bind` の `Local` は `provenance == User` なら対象。

使用判定:

- `TypedExprKind::Local(id)` を見たら used。
- `Lambda.captures` を used。
- `Closure(_, captures)` 内の expression を通常走査。
- `FunctionRef::User` は W1002 のみ。
- `Assign(place, value)` の place 側の local も used。
- `Borrow`, `Dereference`, `Field`, `Index`, `Length`, `StringLength` は children 走査で used。

`TypedExpr::children` は `_ => Vec::new()` fallback があるため、新しい `TypedExprKind` 追加時に漏れやすい。
W1001 pass では `children` に頼るだけでなく、`match` で全 variant を明示する helper を作り、
コンパイル時に variant 追加が見えるようにする。

### W1002 algorithm

E01 後の `CheckedFunction` / `CheckedRecord` に visibility がある前提。

追加想定:

```rust
pub enum Visibility { Public, Private }

pub struct CheckedFunction {
    pub visibility: Visibility,
    pub provenance: Provenance,
    // existing fields...
}

pub struct CheckedRecord {
    pub visibility: Visibility,
    pub provenance: Provenance,
    // existing fields...
}

pub struct Local {
    // existing fields...
    pub provenance: Provenance,
}
```

G03 実装時に E01 が未導入なら、W1002 の pass は空の Vec を返す。

到達性:

- root set は「すべての public callable/type、entry、exports、instance implementations、active patterns、G06 tests」。
- root から function body / signatures / type annotations / record fields / class implementations を辿る。
- warning 対象は `visibility == Private && provenance == User && !reachable` の callable/type だけ。
- instance method と active pattern implementation は root なので、private でも W1002 にしない。
- generated callable/type/local は `provenance != User` なので W1002 にしない。

### W1003 algorithm

A03 が `match` usefulness result を提供する場合:

```rust
pub struct MatchUsefulnessWarning {
    pub span: Span,
    pub reason: MatchUsefulnessReason,
}
```

G03 はこれを `Diagnostic::warning("W1003", ..., span)` に変換する。

A03 が未導入の場合、simple pass:

- `TypedExprKind::Match { arms, .. }` を走査。
- guard なしで alternatives に unconditional alternative がある arm を見つける。
- unconditional alternative は `steps.is_empty()` か `PatternStep::Test(Bool(true))` のみ。
- その後の arm を W1003 にする。

generated single-arm match は `local.name` が `$iteration` / `$fx` / `$recognizer` 由来なら除外する。

### 前提とする他チケットのインターフェース

- G02:
  - `DiagnosticSet`。
  - 複数 diagnostics の deterministic sort/dedup/cap。
  - CLI JSON lines。
- A03（弱依存）:
  - match usefulness / exhaustiveness の解析結果を warning channel へ追加できること。
  - A03 が先に `CheckedModule.warnings` を導入した場合、G03 はその field を再利用する。
- E01（明示依存ではないが W1002 に必要）:
  - private visibility metadata。未実装の場合 W1002 は no-op。

### 他チケットへの提供インターフェース

- G07 は `Diagnostic.severity` と warning JSON を LSP severity に写像する。
- A03 は `W1003` を G03 の warning channel へ出せる。
- G06 は test declaration の未使用 helper を W1002 で誤検出しないよう、test runner root を到達性 root に追加できる。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | shadowing warning のための追加 span は不要。変更なし |
| check | `Diagnostic.severity`, `CheckedModule.warnings`, warning collection を追加 |
| computation | generated `$` names を warning 対象外にする。展開の source span は保持 |
| control | W1003 simple pass / A03 warning 連携 |
| polymorph | monomorphized synthetic functions を W1002 対象外にする |
| closures | `$lambda`, `$task`, `$builtin` synthetic functions を W1002 対象外にする |
| ownership | W1001 の usage で moved-only uses を used と数える。既存 move 検査の意味は変更しない |
| llvm | 変更なし |
| runtime | 変更なし |
| driver | warning を含む analysis API を CLI へ渡す |
| main | `--deny-warnings`, severity-aware render/json |

## 実装手順

1. **`Diagnostic` に severity を追加する。**
   - `Severity` enum を追加。
   - `Diagnostic::new` は error。
   - `Diagnostic::warning` を追加。
   - `render` / `json` を severity-aware にする。
   - 既存 tests を error 期待のまま通す。

2. **CLI に `--deny-warnings` を追加する。**
   - `Arguments { deny_warnings: bool }` を追加。
   - `parse_arguments` に `--deny-warnings` を追加。
   - `rejects_ambiguous_or_unused_arguments` に重複指定や build-only 誤判定がないことを確認する。
   - `--deny-warnings` は check/build/run すべてで有効。

3. **warning channel を追加する。**
   - `CheckedModule { warnings }` を追加。
   - module を構築する全箇所（tests helper を含む）で `warnings: Vec::new()` を埋める。
   - A03 の channel が先にある場合は統合する。

4. **W1001 pass を実装する。**
   - `warnings::collect_unused_locals(function)` を作る。
   - `Local.id` ベースで使用を数える。
   - capture / guard / moved-only use を used にする。
   - `_` prefix と `provenance != User` を抑制する。名前や span から generated code を推測しない。
   - tests:
     - `let x = 1; 42` -> W1001。
     - `let _x = 1; 42` -> warning なし。
     - `let s = "x"; consume s` -> warning なし。
     - `let x = 1; match 0 with | n when x > 0 -> n | _ -> 0` -> warning なし。
     - closure capture -> warning なし。

5. **W1003 simple pass を実装する。**
   - A03 がない場合でも wildcard 後の arm を検出する。
   - A03 がある場合は A03 の result を優先する。
   - tests:
     - `match x with | _ -> 0 | 1 -> 1` -> W1003。
     - `match x with | _ when flag -> 0 | 1 -> 1` -> warning なし。

6. **provenance と W1002 pass を実装 / no-op にする。**
   - `CheckedFunction`, `CheckedRecord`（A02/A05 後は union / type alias）、`Local` に `Provenance` を追加する。
   - parser/checker が利用者宣言から作るものは `User`、computation expansion / pattern lowering / closure lifting /
     builtin wrapper / export bridge / intrinsic wrapper / test runner が作るものは `Generated(kind)` にする。
   - E01 visibility がある場合だけ private reachability を実装する。
   - E01 がない場合、関数を追加しても常に empty を返す。
   - no-op でも tests に「export なし通常関数は W1002 にならない」を入れる。
   - E01 がある場合は、root set に public callable/type、entry、exports、instance implementations、
     active patterns、G06 tests をすべて入れ、そこから到達不能な private user node だけを W1002 にする。

7. **warning を analysis result へ流す。**
   - `check_modules_all` 成功時に warnings を module に入れる。
   - CLI は success module の warnings を表示する。
   - error がある場合でも、同じ解析段階で安全に得られた warnings を混ぜるかは避ける。
     既定案: error がある場合は warning を出さない。error 修正後に warning を出す。

8. **`--deny-warnings` の exit code を実装する。**
   - warning だけの `check` は通常 0、deny あり 1。
   - build/run は warning だけなら通常は成果物を生成/実行する。
   - deny ありなら build/run は codegen 前に止め、成果物を変更しない。

9. **docs を更新する。**
   - warning code 表。
   - `_` prefix 抑制。
   - `--deny-warnings`。
   - JSON severity。

## テスト計画

### Rust tests

新規 `tests/warnings.rs`。

補助:

```rust
fn warnings(source: &str) -> Vec<Diagnostic> {
    let (module, diagnostics) = tsuzuri::analyze_with_diagnostics(source).unwrap();
    assert!(module.warnings.iter().all(|d| d.severity == Severity::Warning));
    diagnostics
}
```

W1001:

```text
def f :: i64
fn f =
    let unused = 1
    42
```

期待:

- `W1001` 1 件。
- message に `unused local 'unused'`。
- span は `unused` 名。

抑制:

```text
fn f = { let _unused = 1; 42 }
```

期待: warning なし。

使用例:

```text
def consume :: string -> unit
fn consume s = ()
def f :: unit
fn f = { let text = "x"; consume text }
```

期待: `text` は move で使用済み、warning なし。

guard-only:

```text
def f :: i64
fn f =
    let threshold = 10
    match 20 with
    | n when n > threshold -> n
    | _ -> 0
```

期待: `threshold` warning なし。

capture:

```text
def f :: i64
fn f =
    let offset = 1
    let g = x -> x + offset
    g 41
```

期待: `offset` warning なし。

generated code:

- computation expression fixture を使い、`Generated(ComputationExpansion)` binding に warning が出ないこと。
- `fx (x, y) -> x` では user binding `y` は W1001、内部 `Generated(FxParameter)` はなし。
- `$export` bridge、`$intrinsic` wrapper、`$builtin` wrapper の `arg0` に W1001/W1002 が出ないこと。

W1002 roots:

- private helper が public function から呼ばれる場合は warning なし。
- private helper が G06 test からだけ呼ばれる場合も warning なし。
- private instance method / active pattern implementation は root 扱いで warning なし。
- private function/type が full root set から到達不能な場合だけ W1002。

W1003:

```text
def f :: i64 -> i64
fn f x =
    match x with
    | _ -> 0
    | 1 -> 1
```

期待: `W1003`。

W1004 internal option:

- `WarningOptions { shadowing: true }` の unit test で同一 scope shadowing を検出する。
- CLI ではまだ出ないことを確認する。

### CLI tests

`tests/e2e.mjs` に追加。

- warning only:
  - `tsuzuri check Main.tz --json`
  - status 0。
  - stderr lines に `"severity":"warning","code":"W1001"`。
- deny:
  - `tsuzuri check Main.tz --deny-warnings --json`
  - status 1。
  - stderr は同じ warning JSON。
- build output protection:
  - 既存 output を作っておき、deny あり build が失敗した場合に output が変更されないこと。

### 既存 tests

- `cargo test --locked --test warnings`
- `cargo test --locked`
- `cargo build --release --locked`
- `node tests/e2e.mjs target/release/tsuzuri`

`cargo test --locked --test warnings` は GUIDE §3 の通り `--test <ファイル名>` を使い、
出力の `running N tests` が 0 でないことを確認する。Node E2E の直前には
`cargo build --release --locked` を実行する。

warning 追加により既存 CLI tests の stderr が変わる場合は、fixture 側で `_` prefix を付けるか、
その warning が正しいか確認して期待値を更新する。

## ドキュメント

- `docs/language.md` の「診断」に warning code 表を追加する。
- `_` prefix 抑制規則を「式と評価順序」または「診断」に追記する。
- `README.md` の CLI option 表に `--deny-warnings` を追加する。
- `docs/architecture.md` に warning collection の段階と、generated `$` names を警告対象外にする不変条件を追記する。

## 受け入れ条件

- [ ] `Diagnostic` が severity を持ち、既存 error JSON が維持される。
- [ ] `W1001` が user-visible unused local を検出する。
- [ ] move-only use、capture、guard-only use が使用として数えられる。
- [ ] `_` prefix と `$` generated names が warning を抑制する。
- [ ] warning 判定は名前や span ではなく `Provenance::User` / `Generated(kind)` に基づく。
- [ ] std source の warning を抑制する hook がある。
- [ ] `W1003` が少なくとも wildcard 後の到達不能 arm を検出する。
- [ ] `W1002` は E01 visibility がない場合 no-op、ある場合 full root set から到達不能な private user node にだけ出る。
- [ ] `W1004` は内部 option あり、CLI 既定では出ない。
- [ ] `--deny-warnings` で warning-only compile が exit code 1 になる。
- [ ] JSON の warning は `"severity":"warning"`。
- [ ] 既存 tests が通る。

## 落とし穴

- `export` は host ABI であり public/private ではない。E01 なしで `export` なし関数を W1002 にしない。
- `TypedExpr::children` の `_ => Vec::new()` に頼ると、新 variant の使用漏れで false positive が出る。
- pattern guard だけの使用を未使用と判定しない。
- moved-only use を未使用と判定しない。
- computation expansion の `$` names に warning を出すと利用者が修正不能になる。
- generated code を `$` name や `arg0` name で推測すると、`$export` / `$intrinsic` / 将来の生成物を漏らす。
  必ず provenance を持たせる。
- instance method と active pattern implementation は class / pattern namespace から使われるため、単純な function reachability で W1002 にしない。
- warning があるだけで build output を止めるのは `--deny-warnings` のときだけ。

## 対象外

- warning suppression attribute / pragma。
- config file。
- `--warn-shadowing` CLI option。
- unreachable code 一般（`return` / `break` 後など）。break/continue は B03。
- exhaustiveness error `E1021` の実装（A03）。
- lint auto-fix。

## 未決事項

- **W1002 と E01 の依存。** チケット一覧では G03 は E01 に依存していないが、D-16 の W1002 は
  「非公開」を前提にする。既定案は E01 がなければ W1002 no-op にする。
- **warning を error と同時に出すか。** 既定案は error がある compile では warning を出さない。
  型が壊れた状態の warning は false positive になりやすいため。
- **W1004 の有効化手段。** 既定案は内部 option のみ。CLI 既定無効。
- **反映済み:** E01 を弱依存として README とメタ情報に追記した。E01 未完了の間は本文どおり `W1002` を出さず（pass は空の Vec を返す）、E01 完了後に `private` の情報を使って有効にする。
