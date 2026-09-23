# G07: LSP（言語サーバー）
| 項目 | 内容 |
|---|---|
| ID | G07 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | G02（複数エラーの同時報告） |
| 後続 | G05（フォーマッター連携）、G09（doc hover 拡張）、G03（警告表示拡張） |
| 状態 | todo |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/lib.rs`, `src/diagnostic.rs`, `src/check.rs`, `src/polymorph.rs`, `src/closures.rs`, `src/syntax.rs`, `tests/lsp.rs`, `tests/lsp_sessions.mjs`, `README.md`, `docs/architecture.md` |

## 目的

エディタから `tsuzuri lsp` を起動し、保存前のコードでも診断、hover、定義ジャンプ、document symbols を返せるようにする。Phase 1 は「編集しながら安全に型エラーを直せる」ことを目標にし、補完や rename は後続段階へ分ける。

最重要の制約は、LSP のためにコンパイラの意味を変えないこと、partial code でコンパイラが panic しないこと、既存の `Span`（UTF-8 バイト範囲）と複数診断（G02）をそのまま使うことである。

## 現状

- CLI は `src/main.rs` の `Action::{Check, Build, Run}`、`parse_arguments`、`main` で処理され、`lsp` サブコマンドはない。
- 診断は `src/diagnostic.rs` の `Diagnostic { code, message, span }` と `Span { start, end, source }`。`Diagnostic::json` は `json_string` を使って手書き JSON を出力し、`location` は UTF-8 バイト offset から 1 始まりの文字列 column を計算する。
- 現在の公開解析 API は `src/lib.rs` の `analyze` / `analyze_modules` で、戻り値は `Result<check::CheckedModule, Diagnostic>`。G02 後は複数診断を返す API が必要になる。
- `src/driver.rs` の `Project::load` は入力ファイルまたはディレクトリから同階層の `.tz` / `.tt` / `.tc` を読み、`Project::analyze` が `crate::analyze_modules` へ渡す。未保存バッファの overlay はない。
- `src/check.rs` の `CheckedModule`, `CheckedFunction`, `CheckedRecord`, `TypedExpr`, `TypedExprKind`, `Local` は型付き情報を持つ。`TypedExpr.span` と `Local.span` は LSP の hover / definition に使える。
- `check_modules` は型検査後に `polymorph::specialize`、`closures::lower`、`ownership::check` を実行してから `CheckedModule` を返す。`polymorph::specialize` と `closures::lower` は `TypedExprKind::GenericFunction` / `Method` / `Lambda` を書き換えるため、ソース位置に対応した型 index はこの前に採取する必要がある。
- 名前解決は `Checker::name` がローカル → 同一モジュール関数 → builtin の順で行い、レコード解決は `Names::record`。これらは非公開なので、LSP 用の semantic index には追加 API が必要。

## 仕様

### CLI

```text
tsuzuri lsp
```

- 標準入出力で LSP 3.17 の JSON-RPC framed message を扱う。
- `lsp` はビルドオプションを受け取らない。`tsuzuri lsp -O0`、`--target`、`-o` は `E2000` 相当の CLI エラーで拒否する。
- 通常の `--json` は LSP では使わない。LSP 内のエラーは JSON-RPC error response と `window/logMessage` で通知する。

### JSON-RPC と JSON 処理

- `Content-Length: <bytes>\r\n\r\n<json>` のみを必須対応する。`Content-Type` があれば無視してよい。
- JSON は新 crate を追加せず、Phase 1 では `src/json.rs` に最小 JSON parser / writer を実装する。
  - 理由: 既存の `diagnostic.rs` は手書き JSON を持ち、リポジトリ方針は「既定で新 crate なし」。
  - 対応型: object, array, string, number（整数 / 小数は文字列保持で十分）, bool, null。
  - 上限: 1 message 16 MiB、ネスト 128。超過は JSON-RPC parse error `-32700`。
- 文字列 escape は `diagnostic::json_string` と同等にし、LSP response の生成でも再利用する。

### 初期化と lifecycle

対応 request / notification:

| メソッド | Phase 1 の挙動 |
|---|---|
| `initialize` | capability を返す。`rootUri` / `workspaceFolders` があれば記録するが、実際の project は各 file の directory で決める |
| `initialized` | no-op |
| `shutdown` | `null` を返し、以降は `exit` 待ち |
| `exit` | shutdown 後なら 0、shutdown 前なら 1 で終了 |
| `textDocument/didOpen` | buffer を保存し、その directory project を解析予約 |
| `textDocument/didChange` | `TextDocumentSyncKind.Full` の全量同期のみ。version を更新し解析予約 |
| `textDocument/didClose` | overlay を削除し、disk source で再解析予約。最後に空 diagnostics を publish |
| `textDocument/hover` | position に最も狭く重なる typed node / local / declaration の型を返す |
| `textDocument/definition` | 関数、レコード、ローカルの定義位置へ `Location` を返す。未解決は `null` |
| `textDocument/documentSymbol` | module 内の record / function / class / instance / entry 相当を返す |
| `$/cancelRequest` | request id を cancelled set に入れる。解析開始前または結果返却前なら `-32800 Request cancelled` |

`completion`, `formatting`, `rename`, `semanticTokens` は Phase 1 では capability を返さない。

### position encoding

- `initialize` の client capabilities `general.positionEncodings` に `utf-8` が含まれる場合、server capabilities に `"positionEncoding": "utf-8"` を返し、以降の `Position.character` は UTF-8 byte offset とする。
- 含まれない場合は LSP 既定の UTF-16 code unit を使う。
- `Span` は UTF-8 byte offset のまま保持し、`PositionMapper` が byte ↔ LSP position を変換する。
- `diagnostic::location` は human / CLI の文字 column 用であり、LSP の UTF-16 変換には直接使わない。

### diagnostics

- G02 の複数診断 API を使い、同一 project の全 source を解析して file ごとに `textDocument/publishDiagnostics` を送る。
- severity は `Diagnostic` が error の場合 `1`。G03 後に warning が入る場合は `2`。
- `range` は `Span.start..Span.end`。`end == start` の場合も LSP の zero-width range として送る。
- `code` は既存の安定コード（例: `E1003`）を文字列で送る。
- `source` は `"tsuzuri"`。
- open buffer の内容は disk より優先する。project は対象 file の directory で、同 directory の `.tz` / `.tt` / `.tc` を読み、開いている同 directory の source buffer で上書きする。disk に未作成の `.tz` / `.tt` / `.tc` buffer も project に含める。
- `didClose` 後に disk file が存在しなければ project から外す。

### hover

返す Markdown 例:

````markdown
```tsuzuri
value: i64
```
````

- 式に重なる場合: `TypedExpr.ty.display(&records)`。
- ローカルに重なる場合: `Local.name: Type`。
- 関数名に重なる場合: `def module.name :: ...`。型は `CheckedFunction.signature` を `Type::display` 相当で表示する。
- レコード名に重なる場合: `record Module.Name { ... }`。
- 複数候補がある場合は最も狭い span、同じ幅なら local > expr > function > record を優先する。

### go-to-definition

- `TypedExprKind::Local(id)` は対応する `Local.span`。
- `TypedExprKind::Function(FunctionRef::User(id))` と `TypedExprKind::GenericFunction(id, _)` は `CheckedFunction.span`。
- `TypedExprKind::Record` そのものは値式なので、レコードリテラル名 / 型注釈名の definition は AST / check 時に別途 index する。
- builtin は definition を返さない。
- class method / instance method は Phase 1 では通常関数に下がった `CheckedFunction.span` へ飛ぶ。G09 / A06 後に docs / richer symbol と統合する。

### document symbols

- `DocumentSymbol[]` を返す。階層は module → declarations ではなく、file 内 top-level declarations の flat list でよい。
- `kind`: function = 12, struct(record) = 23, class = 5, method = 6, variable(entry) = 13。
- `range` は宣言全体、`selectionRange` は名前 span。
- declaration order は parser の順序を維持し、module list は `Project::load` と同じ file name sort。

### robustness

- LSP server は compiler error / syntax error を通常の diagnostics として返し、panic しない。
- JSON parse error、未知 method、壊れた URI、範囲外 position は JSON-RPC error response を返し、プロセスを落とさない。
- 解析が失敗しても直前の semantic index は hover / definition に使わない。古い成功結果を返すと誤誘導になるため、該当 project の semantic index は invalid にする。
- `panic = abort` などの特別な設定は追加しない。

## 設計

### 追加する主な型

`src/json.rs`:

```rust
pub enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

pub fn parse(input: &str) -> Result<Json, JsonError>;
pub fn stringify(value: &Json) -> String;
```

`src/lsp.rs`:

```rust
pub fn serve<R: std::io::Read, W: std::io::Write>(
    input: R,
    output: W,
    stderr: impl std::io::Write,
) -> i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PositionEncoding { Utf8, Utf16 }

struct PositionMapper {
    line_starts: Vec<usize>,
    encoding: PositionEncoding,
}

struct OpenBuffer {
    uri: String,
    path: PathBuf,
    version: Option<i64>,
    text: String,
    mapper: PositionMapper,
}

struct Workspace {
    buffers: BTreeMap<PathBuf, OpenBuffer>,
    projects: BTreeMap<PathBuf, ProjectState>,
    cancelled: BTreeSet<JsonRpcId>,
}
```

`src/semantic.rs`（または `src/lsp.rs` 内 module）:

```rust
pub struct SemanticSnapshot {
    pub module: check::CheckedModule,
    pub files: Vec<SemanticFile>,
}

pub struct SemanticFile {
    pub path: PathBuf,
    pub symbols: Vec<DocumentSymbolData>,
    pub hovers: Vec<HoverEntry>,
    pub definitions: Vec<DefinitionEntry>,
}

pub struct HoverEntry {
    pub span: Span,
    pub priority: u8,
    pub markdown: String,
}

pub struct DefinitionEntry {
    pub use_span: Span,
    pub target_span: Span,
}
```

### G02 に期待するインターフェース

前提とする他チケットのインターフェース:

```rust
pub struct DiagnosticSet {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

pub fn analyze_modules_report(
    sources: &[(&str, &str)],
) -> Result<check::CheckedModule, DiagnosticSet>;
```

G02 の実際の名前が異なる場合、G07 実装時に wrapper を用意してよい。ただし LSP は最初の 1 件だけに戻してはならない。

他チケットへの提供インターフェース:

- G05 は `tsuzuri lsp` の `textDocument/formatting` capability を後から有効化できる。
- G09 は hover に doc comment を追加するため `HoverEntry` 生成箇所へ docs markdown を差し込める。
- G03 は warning diagnostics を同じ `publishDiagnostics` に載せられる。

### semantic index を作る時点

semantic index は `check_modules` 内で次の時点に作る。

1. 各関数 body を `checker.expression` / `checker.finish` で型付き化した直後。
2. `recursion::check` は済ませてもよい。
3. `polymorph::specialize` より前。
4. `closures::lower` より前。

理由: `polymorph::specialize` は `Specializer::lower` で `GenericFunction` / `Method` を concrete function に置換し、`closures::lower` は `TypedExprKind::Lambda` を `Closure` に置換して `$lambda` / `$task` 関数を追加する。hover / definition は利用者のソースに近い抽象 body を見せるべきであり、lower 後の `$mono` / `$lambda` 名へ飛ばしてはいけない。

実装は既存 `check_modules` を壊さず、内部 helper を分ける。

```rust
pub struct CheckOutput {
    pub lowered: CheckedModule,
    pub semantic: SemanticSnapshot,
}

pub fn check_modules_with_semantics(
    modules: &[(&str, &Program)],
) -> Result<CheckOutput, DiagnosticSet>;
```

`check_modules` は `check_modules_with_semantics(...).map(|out| out.lowered)` へ委譲する。

### project overlay

`src/driver.rs` に LSP 専用の安全な loader を追加する。

```rust
impl Project {
    pub fn load_with_overlays(
        input: &Path,
        overlays: &BTreeMap<PathBuf, String>,
    ) -> Result<Self, SourceError>;
}
```

- `Project::load` の既存動作は変更しない。
- overlays は canonicalize できない未作成 file もあり得るため、project directory への所属確認は親 directory の canonicalize と path normalization で行う。
- `SourceFile { path, name, text }` は overlay text を使う。
- overlay の source size も `MAX_SOURCE_BYTES` を超えたら `E0003`。

### コンパイラ段階ごとの変更

| 段階 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | 変更なし |
| check | semantic index 生成 API を追加。`Checker::bind`, `Checker::name`, `TypedExpr` 走査から hover / definition を採取 |
| polymorph | 変更なし。ただし index は `polymorph::specialize` 前に採取 |
| control | 変更なし。`TypedExpr::children` により match / loop 内も index する |
| closures | 変更なし。ただし index は `closures::lower` 前に採取 |
| ownership | 変更なし。所有権エラーも diagnostics に出る |
| llvm / runtime | 変更なし |
| driver | overlay loader を追加 |
| main | `Action::Lsp` と help を追加 |

## 実装手順

1. **JSON / framing**
   - `src/json.rs` を追加し、parser / writer / `Content-Length` reader を実装。
   - 確認: `cargo test --locked --lib json`（`src/json.rs` の単体テスト。N > 0 を確認）。壊れた header、ネスト上限、escape roundtrip を検査。
2. **CLI 起動**
   - `src/main.rs` に `Action::Lsp` と help を追加。
   - `tsuzuri lsp` は `lsp::serve(stdin, stdout, stderr)` を呼ぶ。
   - 確認: `parse_arguments` tests に `lsp` と不正 option を追加。
3. **PositionMapper**
   - UTF-8 / UTF-16 の byte ↔ position を実装。
   - CRLF は `\r\n` を 1 行区切りとして扱う。LSP position は 0 始まり。
   - 確認: 日本語、絵文字、サロゲート相当、行末、EOF zero-width の単体テスト。
4. **overlay project loader**
   - `Project::load_with_overlays` を追加。
   - 確認: disk source + unsaved replacement + unsaved new `.tz` + didClose 相当を Rust test。
5. **semantic snapshot**
   - `check_modules_with_semantics` を追加し、pre-specialize body から hover / definition / document symbols を作る。
   - `TypedExpr::children` を使い、追加漏れを避ける。`Local.id` → `Local.span` map を関数ごとに作る。
   - 確認: `tests/lsp.rs` で local/function/record hover と definition の source id を検査。
6. **LSP lifecycle / diagnostics**
   - initialize/shutdown/open/change/close/publishDiagnostics を実装。
   - debounce は 200ms を既定にし、同じ project の予約をまとめる。実装は `std::thread` + `mpsc` でよい。
   - 確認: `tests/lsp_sessions.mjs` で scripted session を流し、publishDiagnostics の code/range を照合。
7. **hover / definition / symbols**
   - request id ごとに response を返す。cancelled id は `-32800`。
   - 確認: scripted session で hover markdown、definition URI/range、document symbols を golden JSON と比較。
8. **docs**
   - README の CLI 表に `tsuzuri lsp` を追加。
   - `docs/architecture.md` に semantic index の採取位置と LSP validation command を追記。

## テスト計画

- Rust:
  - `tests/lsp.rs`
    - `PositionMapper` UTF-8 / UTF-16。
    - `Project::load_with_overlays` が open buffer を disk より優先する。
    - semantic index が `polymorph::specialize` 前の型を返し、`$mono` / `$lambda` に飛ばない。
    - syntax error でも panic せず diagnostics を返す。
  - `src/json.rs` unit tests。
- Node:
  - `tests/lsp_sessions.mjs target/release/tsuzuri`
    - initialize → didOpen invalid → diagnostics `E1003`。
    - didChange valid → diagnostics empty。
    - hover on local / function / record。
    - definition on local / function。
    - documentSymbol order。
    - `$/cancelRequest`。
    - malformed JSON-RPC request returns parse error and process continues.
- 既存回帰:
  - `cargo test --locked`
  - LSP は codegen しないため native/WASM E2E は不要。ただし check API を変えるため `tests/modules.rs`, `tests/polymorphism.rs`, `tests/computations.rs` は通す。

## ドキュメント

- `README.md`:
  - CLI 表に `tsuzuri lsp`。
  - LSP は stdio JSON-RPC で editor が起動すること、手動実行例はデバッグ用途であることを書く。
- `docs/architecture.md`:
  - check pipeline に「semantic index は単相化・closure lowering 前に採取」と記載。
  - LSP の position encoding 方針を記載。

## 受け入れ条件

- [ ] `tsuzuri lsp` が initialize/shutdown/exit を LSP 形式で完了する。
- [ ] didOpen/didChange/didClose の full sync で diagnostics が更新される。
- [ ] unsaved buffer が disk source より優先される。
- [ ] hover が position の型を返し、日本語を含む source でも range がずれない。
- [ ] definition が関数、レコード、ローカルへ飛ぶ。
- [ ] document symbols が決定的な順序で返る。
- [ ] partial / invalid code で panic しない。
- [ ] 新 crate を追加していない。追加する場合は未決事項に理由を追記し、人間レビューを受ける。
- [ ] `cargo fmt --all -- --check`、`cargo test --locked`、`node tests/lsp_sessions.mjs target/release/tsuzuri` が通る。

## 落とし穴

- LSP range は 0 始まり、既存 `diagnostic::location` は 1 始まり。混同しない。
- UTF-16 fallback を省くと多くの editor で hover / diagnostics がずれる。
- lower 後の `CheckedModule` だけを index すると `$lambda` / `$builtin` / `$mono` へ飛ぶ。
- 古い成功 snapshot を syntax error 中に返すと、利用者に誤った型情報を見せる。
- LSP server の stdout にログを書いてはいけない。ログは stderr または `window/logMessage`。
- `didChange` は Phase 1 では full sync のみ。incremental range edit を受けたら JSON-RPC error にするか、capability を出さない。

## 対象外

- completion、formatting、rename、semantic tokens。
- workspace-wide symbol search。
- `.tz` 以外の import / package graph（E03 / E04）。
- LSP over TCP / socket。
- editor extension の配布。

## 未決事項

- **G02 API 名**: 本チケットでは `DiagnosticSet` / `analyze_modules_report` を仮定した。実際の G02 実装名が異なる場合は薄い adapter を作る。
- **JSON crate**: 既定案は新 crate なしの最小 parser。人間が保守性を優先するなら `serde_json` 追加を検討できるが、依存追加の理由と MSRV / binary size 影響を PR に記録する。
- **debounce 時間**: 既定 200ms。大規模 project で遅い場合は設定追加を G11 / 将来の workspace 設定へ回す。
- **position encoding**: 既定案は `utf-8` を優先し、なければ UTF-16。UTF-8 のみ実装は不可。
