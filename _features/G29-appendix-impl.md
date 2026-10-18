# G29 付録 2/2（MCP サーバー: 実装・テスト・測定）

[G29-mcp-server.md](G29-mcp-server.md) の付録。付録 E〜H を置く。付録 A〜D は [G29-appendix.md](G29-appendix.md)。

## 付録 E: Rust の骨格

名前と型を決めるためのもので、**コンパイルは確かめていない**。実装で直してよいのは、コンパイラが指摘する細部だけで、名前・公開の範囲・意味は変えない。

```rust
// src/mcp/mod.rs
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub mod backend;
mod check;
mod path;
mod rpc;
pub mod schema;
mod session;
mod tools;
pub use backend::{Backend, Cancel, Capabilities, ProjectBackend, ToolError, ToolOutput, ToolRequest};

/// 新しい順。先頭は、交渉できない要求への応答でもある。
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PENDING: usize = 64;
pub const READER_QUEUE: usize = 8;
pub const MAX_TEXT_CHARS: usize = 200_000;
pub const MAX_STRUCTURED_BYTES: usize = 400_000;

#[derive(Debug)]
pub struct Fatal(pub String); // E2010 のメッセージ

pub struct Options {
    pub root: PathBuf,           // cache::real_path 済みのディレクトリ
    pub kb_dir: Option<PathBuf>, // 既定は <root>/target/tz/kb
}

/// stdio の入口。`input` は別スレッドで読み、`output` は呼び出したスレッドだけが書く。
pub fn serve(options: Options, input: impl Read + Send + 'static, output: impl Write) -> Result<(), Fatal> {
    serve_with(Arc::new(ProjectBackend::new(&options)), input, output)
}
/// 任意の `Backend` で動かす入口（試験用）。
pub fn serve_with(backend: Arc<dyn Backend>, input: impl Read + Send + 'static, output: impl Write) -> Result<(), Fatal> {
    session::run(backend, input, output)
}
```

```rust
// src/mcp/backend.rs
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub check: bool, pub guide: bool, pub explain: bool, pub kb: bool,
    pub impact: bool, pub find: bool, pub fix: bool, pub diagram: bool,
}
impl Capabilities {
    pub fn none() -> Self { Self::default() }
    pub fn all() -> Self { Self { check: true, guide: true, explain: true, kb: true, impact: true, find: true, fix: true, diagram: true } }
    /// ツール名 → 必要な機能。ここに無い名前は常に false。
    pub fn allows(&self, tool: &str) -> bool {
        match tool {
            "tz_check" => self.check,
            "tz_guide" => self.guide,
            "tz_explain" => self.explain,
            "tz_symbol" | "tz_brief" | "tz_search" | "tz_callers" => self.kb,
            "tz_impact" => self.impact,
            "tz_find" => self.find,
            "tz_fix_plan" => self.fix,
            "tz_diagram" => self.diagram,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn new() -> Self { Self::default() }
    pub fn cancel(&self) { self.0.store(true, Ordering::Relaxed) }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::Relaxed) }
}

/// 検証済みの引数。既定値は適用されていない（ツールが `unwrap_or` で補う）。
pub struct ToolRequest<'a> { pub name: &'a str, pub arguments: &'a Value, pub cancel: &'a Cancel }
/// `structured` は object。`truncated` は無くてよい（`shape` が足す）。
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput { pub text: String, pub structured: Value }
/// `isError: true` の本文になる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError(pub String);

pub trait Backend: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError>;
    /// Phase 3。`Ok(None)` は「その資源はない」（`-32002`）。
    fn read_resource(&self, _uri: &str, _cancel: &Cancel) -> Result<Option<String>, ToolError> { Ok(None) }
}

pub struct ProjectBackend { root: std::path::PathBuf, kb_dir: std::path::PathBuf /* Phase 2: results: Mutex<ResultCache> */ }
impl ProjectBackend {
    pub fn new(options: &super::Options) -> Self;
}
impl Backend for ProjectBackend {
    fn capabilities(&self) -> Capabilities { Capabilities { check: true, ..Capabilities::none() } } // 各手順が true を足す
    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError> {
        match request.name {
            "tz_check" => super::check::run(self, request),
            name => Err(ToolError(format!("tool '{name}' is not available in this build"))),
        }
    }
}
```

```rust
// src/mcp/rpc.rs
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
pub const RESOURCE_NOT_FOUND: i64 = -32002; // 2025-06-18 の資源の誤り
pub const TOO_MANY_PENDING: i64 = -32000;   // 実装定義

pub enum Line { Eof, Bytes(Vec<u8>), TooLong }
/// 空の行は読み飛ばす。`\n` の前の `\r` は取り除く。`MAX_LINE_BYTES` を超えたら次の `\n` まで読み捨てて `TooLong`。
pub fn read_line(reader: &mut impl std::io::BufRead) -> std::io::Result<Line>;

pub enum Incoming {
    Request { id: Value, method: String, params: Value }, // params は無ければ Value::Null、あれば object
    Notification { method: String, params: Value },
    Ignore,                                               // クライアントの応答、params が object でない通知
    Invalid { id: Value, code: i64, message: String },    // id は取れなければ Value::Null
}
pub fn parse(line: &[u8]) -> Incoming;                    // 仕様の分類の表の順

pub fn result_message(id: &Value, result: Value) -> Value;
pub fn error_message(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value;
pub fn write_line(output: &mut impl std::io::Write, message: &Value) -> std::io::Result<()>; // to_string + '\n' + flush
```

```rust
// src/mcp/schema.rs（pub。tests/mcp.rs が structuredContent の検証に使う）
pub const SUPPORTED_KEYWORDS: [&str; 16] = [
    "type", "properties", "required", "additionalProperties", "items", "enum", "const",
    "minimum", "maximum", "minLength", "maxLength", "minItems", "maxItems",
    "description", "default", "title",
];
/// 最初の違反だけ。`Err("/path: must be a string")` の形。根の pointer は ""。
pub fn validate(schema: &Value, value: &Value) -> Result<(), String>;
/// `properties` と `items` をたどり、SUPPORTED_KEYWORDS にないキーワードの一覧を返す。
pub fn keywords_outside_the_subset(schema: &Value) -> Vec<String>;
```

```rust
// src/mcp/tools.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version { V2024_11_05, V2025_03_26, V2025_06_18 }
impl Version {
    pub fn parse(text: &str) -> Option<Version>;
    pub fn as_str(self) -> &'static str;
    pub fn titles(self) -> bool { self >= Version::V2025_06_18 }      // title・outputSchema・structuredContent
    pub fn annotations(self) -> bool { self >= Version::V2025_03_26 }
}
pub struct Registry { tools: Vec<(String, Value)> } // include_str!("tools.json") を解析
impl Registry {
    pub fn load() -> Registry;                                     // 解析に失敗したら panic（単体テストで保証）
    pub fn list(&self, version: Version, caps: Capabilities) -> Vec<Value>;
    pub fn get(&self, name: &str, caps: Capabilities) -> Option<&Value>; // 定義の object
}
pub fn cap_text(text: String) -> (String, bool);
pub fn shape(version: Version, output: ToolOutput) -> Value;       // 本文の「結果の組み立て」
```

```rust
// src/mcp/session.rs
enum Event { Line(Line), ReadFailed(String), Done { id: Value, outcome: Outcome } }
enum Outcome { Result(Value), ProtocolError { code: i64, message: String }, Cancelled }
struct Job { id: Value, cancel: Cancel, work: Work }
enum Work { Tool { name: String, arguments: Value }, Resource { uri: String } }
pub(crate) fn run(backend: Arc<dyn Backend>, input: impl Read + Send + 'static, output: impl Write) -> Result<(), Fatal>;
fn run_job(backend: &dyn Backend, registry: &Registry, version: Version, job: &Job) -> Outcome; // 「tools/call」の表の 1〜6
```

```rust
// src/mcp/path.rs
pub fn resolve_path(root: &Path, arg: &str) -> Result<PathBuf, ToolError>;            // 「パスの閉じ込め」の表
pub fn relative_text(root: &Path, path: &Path) -> String;                              // 根の下なら相対（/ 区切り）、それ以外は "."
pub fn scrub(message: &str, root: &Path) -> String;                                    // 根の絶対パスを消す
// src/mcp/check.rs
pub(crate) fn run(backend: &ProjectBackend, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError>;
```

`tests/mcp.rs` の補助（手順 2）:

```rust
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;
use serde_json::{Value, json};
use tsuzuri::mcp::{Backend, Cancel, Capabilities, ToolError, ToolOutput, ToolRequest, serve_with};

/// `mpsc` で包んだ入力。送り手を drop すると EOF になる。
struct ChannelReader { rx: mpsc::Receiver<Vec<u8>>, buffer: Vec<u8>, position: usize }
impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.position == self.buffer.len() {
            match self.rx.recv() { Ok(chunk) => { self.buffer = chunk; self.position = 0; } Err(_) => return Ok(0) }
        }
        let count = (self.buffer.len() - self.position).min(out.len());
        out[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
/// サーバーの出力。`recv` が 1 行ずつ取り出す。
#[derive(Clone, Default)]
struct Shared(Arc<(Mutex<Vec<u8>>, Condvar)>);
impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let (lock, changed) = &*self.0;
        lock.lock().unwrap().extend_from_slice(bytes);
        changed.notify_all();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
struct Client { to_server: Option<mpsc::Sender<Vec<u8>>>, output: Shared, server: Option<std::thread::JoinHandle<Result<(), tsuzuri::mcp::Fatal>>> }
impl Client {
    fn start(backend: Arc<dyn Backend>) -> Client { /* channel を作り、serve_with を別スレッドで動かす */ todo!() }
    fn send(&self, message: Value) { /* to_string + "\n" を送る */ }
    fn send_raw(&self, bytes: &[u8]) { /* そのまま送る */ }
    /// 次の出力の行を Value で返す。10 秒待っても来なければ panic（テストの失敗の検出だけ）。
    fn recv(&self) -> Value { todo!() }
    fn request(&self, id: i64, method: &str, params: Value) -> Value { self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})); self.recv() }
    /// 入力を閉じ、サーバーの終了を待つ。
    fn finish(self) -> Result<(), tsuzuri::mcp::Fatal> { todo!() }
}
/// `tool-examples.json`（付録 D）の結果を返す。`gate` があれば、その開くまで `call` が止まる。
struct FakeBackend { caps: Capabilities, calls: Mutex<Vec<String>>, gate: Option<Arc<(Mutex<bool>, Condvar)>> }
```

## 付録 F: クライアントの設定と公式 SDK での確認

### F.1 設定の例（2026-10-18 に各製品の文書で確かめた）

`tsuzuri` が `PATH` にあること。**サーバーのカレントディレクトリがプロジェクトの根でない**クライアントでは `--root` に絶対パス（VS Code は `${workspaceFolder}`）を渡す。

| クライアント | ファイル・コマンド |
| --- | --- |
| VS Code（ワークスペース） | `.vscode/mcp.json`（トップは `servers`。`type`・`command`・`args`・`cwd`・`env` が使え、`${workspaceFolder}` が展開される） |
| VS Code・Copilot CLI・Claude Code（共有） | ワークスペースの根の `.mcp.json`（トップは `mcpServers`） |
| Copilot CLI（利用者） | `copilot mcp add tsuzuri -- tsuzuri mcp`、または `~/.copilot/mcp-config.json`（`mcpServers`。`"type": "local"`・`"tools": ["*"]`） |
| Claude Code | `claude mcp add --transport stdio tsuzuri -- tsuzuri mcp` |
| Cursor | `.cursor/mcp.json`（プロジェクト）か `~/.cursor/mcp.json`（`mcpServers`） |

```json
{
  "servers": {
    "tsuzuri": {
      "type": "stdio",
      "command": "tsuzuri",
      "args": ["mcp", "--root", "${workspaceFolder}"]
    }
  }
}
```

```json
{
  "mcpServers": {
    "tsuzuri": {
      "command": "tsuzuri",
      "args": ["mcp"]
    }
  }
}
```

### F.2 公式 SDK のクライアントでの確認

`@modelcontextprotocol/sdk` 1.32.1（TypeScript SDK。Node 22.12 以上。ここでは `npx --yes --package=node@24 node` を使った）のクライアントは、`tools/list` の応答を厳密に解析し、`callTool` が返す `structuredContent` を `outputSchema` で検証して、合わなければ例外にする（付録 G の対照実験で確認）。次の手順は Phase 1・3 の完了の確認で、**リポジトリの外（例: `target/tz-G29-sdk/`）で行い、リポジトリに依存を足さない**。

```sh
mkdir -p target/tz-G29-sdk && cd target/tz-G29-sdk
printf '{"name":"tz-sdk","private":true,"type":"module"}\n' > package.json
npm install --no-audit --no-fund @modelcontextprotocol/sdk@1.32.1
# conformance.mjs を下の内容で作る
npx --yes --package=node@24 node conformance.mjs /path/to/a/tsuzuri/project
```

```js
// conformance.mjs
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const root = process.argv[2];
const transport = new StdioClientTransport({ command: 'tsuzuri', args: ['mcp', '--root', root] });
const client = new Client({ name: 'tz-conformance', version: '0.0.0' }, { capabilities: {} });
const calls = {
  tz_check: {}, tz_guide: {}, tz_brief: {}, tz_search: { query: 'a' },
  tz_explain: { query: 'E1012' }, tz_fix_plan: {}, tz_diagram: { kind: 'module' },
};
try {
  await client.connect(transport);
  console.log('server', JSON.stringify(client.getServerVersion()), JSON.stringify(client.getServerCapabilities()));
  console.log('ping', JSON.stringify(await client.ping()));
  const { tools } = await client.listTools();
  console.log('tools', tools.map(tool => tool.name).join(' '));
  for (const tool of tools) {
    if (!calls[tool.name]) continue;
    const result = await client.callTool({ name: tool.name, arguments: calls[tool.name] });
    console.log(tool.name, result.isError ? 'isError' : 'ok', result.structuredContent ? 'structured' : 'text-only');
  }
  if (client.getServerCapabilities()?.resources) {
    console.log('resources', (await client.listResources()).resources.map(r => r.uri).join(' '));
    console.log('templates', (await client.listResourceTemplates()).resourceTemplates.map(r => r.uriTemplate).join(' '));
  }
  console.log('DONE');
} catch (error) {
  console.log('FAILED', error?.message ?? error);
  process.exitCode = 1;
} finally {
  await client.close().catch(() => {});
  process.exit(process.exitCode ?? 0);
}
```

期待: `FAILED` が出ず、最後が `DONE`、終了コード 0。`tz_check` は `structured`。

## 付録 G: 測定と再現（HEAD `a2313d19`、2026-10-18。作業ツリー `9dd0fbbc`）

**G.1 `tz_check` の元になる出力**（`Main.tz` は 9 行。`/tmp/tz-G29-bad/`）:

```text
def consume :: string -> i64 = \text -> text.length

def run :: string -> i64 = \text ->
    let first = consume text
    let second = consume text
    first + second

let spare = 1
run "abc"
```

`tsuzuri check Main.tz --json` の標準エラーは付録 C.2 の `diagnostics[0]` と同じ内容（`path` だけが `./Main.tz`）で、終了コード 1。`tsuzuri check Main.tz` の標準エラーは `./Main.tz:5:26: error[E1012]: use of moved or partially moved value 'text'` と、引用 2 行（`  5 |     let second = consume text` と、`^` の行）。

警告の例（`/tmp/tz-G29-warn/Main.tz`。5 行）:

```text
def scale :: i64 -> i64 = \n ->
    let spare = 1
    n * 2

scale 21
```

`tsuzuri check Main.tz --json` は終了コード 0 で、標準エラーに `{"severity":"warning","code":"W1001","message":"unused local 'spare'; prefix it with '_' to silence this warning","path":"./Main.tz","span":{"start":40,"end":45,"line":2,"column":9,"end_line":2,"end_column":14}}`。

`E1012` の説明の Good の例（付録 D の `tz_explain`）は `check` が通り、`tsuzuri run .` が `6` を出す。

**G.2 解析のキャッシュの有無**（`/usr/bin/time -p tsuzuri check <dir>` の 7 回の中央値。1 回ずつ先に実行してキャッシュを温めた。`TSUZURI_CACHE_DIR=` はキャッシュなし。時間は 10 ms 刻み）:

| プロジェクト | キャッシュあり | キャッシュなし |
| --- | --- | --- |
| `examples/point`（2 モジュール） | 0.03 s | 0.03 s |
| `tests/fixtures/llvm_handoff`（12 モジュール） | 0.02 s | 0.01 s |

最初の 1 回（キャッシュを作る回）は 0.14〜0.38 s かかった。

**G.3 `tools/list` の大きさ**: 付録 B の 11 ツール、2025-06-18 の応答の 1 行は 14,411 bytes（改行を含む）。

**G.4 検証に使った道具**: Python 3.14.7、`jsonschema` 4.26.0（venv。`target/` の下）、Node.js v24（`npx --yes --package=node@24 node`）、`@modelcontextprotocol/sdk` 1.32.1。公式 SDK のクライアントを、付録 C の応答をそのまま返す偽のサーバー（付録 C の JSON を読んで返す 40 行の Node スクリプト）につなぎ、`initialize`・`ping`・`tools/list`（11 ツール）・`tools/call`（結果の `structuredContent` の検証を含む）・`isError`・`-32602`・`resources/list`・`resources/templates/list`・`resources/read`・`-32002` がすべて通った。**対照実験**: 偽のサーバーが `structuredContent.ok` を文字列 `"yes"` で返すと、クライアントが `Structured content does not match the tool's output schema: data/ok must be boolean` で拒否した（検証が実際に働いている）。

**G.5 HEAD の関連する数字**: `src/lsp.rs` 2,678 行、`tests/*.rs` 101 件・`tests/*.mjs` 80 件、`Cargo.lock` の `indexmap` は 0 件、`WORKER_STACK_BYTES` は 8 MiB、`collect_sources` の上限はディレクトリ 1,024・深さ 16・ファイル 4,096。LSP の出力（`printf '{"jsonrpc":"2.0","id":1,"method":"ping"}\n' \| tsuzuri lsp`）は `Content-Length: 95` の見出しの後に `{"error":{"code":-32700,"message":"invalid or oversized LSP header"},"id":null,"jsonrpc":"2.0"}`（キーは辞書順）。
## 付録 H: テストの一覧（Phase 1）

`tests/mcp.rs` は付録 E のハーネス（`Client`・`FakeBackend`）を使う。「次の行が `ping` の応答」は、先の要求が応答を持たないことの確かめ方（時間を待たない）。期待値は付録 C の JSON（公式 `schema.json` で検証済み）と、付録 D の例を使い、**サーバーの出力を写さない**。

**群 P（手順 2。プロトコルの基本。7 件）**

| テスト | 入力 → 期待 |
| --- | --- |
| `initialize_negotiates_the_protocol_version` | `2025-06-18`・`2025-03-26`・`2024-11-05` → 同じ値。`2025-11-25`・`2026-07-28`・`""` → `2025-06-18`。`protocolVersion` の欠落・数値 → `-32602`。同じセッションの 2 回目の `initialize` → `-32600` |
| `initialize_result_matches_the_appendix_examples` | 2025-06-18 と 2025-03-26 の応答が付録 C.1・C.3 と `Value` で等しい（`serverInfo.title` は 2025-03-26 で無い） |
| `ping_is_answered_in_every_state` | `initialize` の前後で `ping` → `{}` |
| `requests_before_initialize_are_rejected` | `tools/list`・`tools/call` → `-32600`、`ping` は通る。`initialize` の後は通る |
| `notifications_never_get_a_response` | `notifications/initialized`・未知の通知・`params` が配列の通知・未知の `requestId` の取り消し → 応答なし（後続の `ping` が次の行） |
| `framing_errors_are_reported_and_the_session_continues` | `}`・`[]`・`5`・`"x"`・`{"jsonrpc":"1.0","id":1,"method":"ping"}`・`method` 欠落・`id` が `1.5`・`true`・`null`・`{}`・4,194,305 bytes の行 → 仕様の表のコードとメッセージ（`-32700` は接頭辞だけ）。そのあと `ping` が通る |
| `unknown_methods_are_method_not_found` | `server/discover`（初期化の前も後も）・`prompts/list`・`logging/setLevel`・Phase 3 より前の `resources/list` → `-32601` |

**群 T（手順 6。ツールと結果。11 件）**

| テスト | 入力 → 期待 |
| --- | --- |
| `tools_list_matches_tools_json_for_every_version` | 3 つの版で、期待を**テスト内で** `tools.json` から作る（`title`・`outputSchema`・`annotations` を版ごとに除く）。`Capabilities::all()` で 11 件 |
| `tools_list_hides_unavailable_tools` | `Capabilities { check: true, ..none() }` → `tz_check` だけ。`tools/call tz_guide` → `-32602` `unknown tool: tz_guide` |
| `tools_list_rejects_a_cursor` | `{"cursor":"x"}`・`{"cursor":""}` → `-32602` |
| `tools_call_protocol_errors` | `params` なし・`name` が数値・`arguments` が配列・未知のツール → 「tools/call」の表の 1・2 のコードとメッセージ |
| `tools_call_validates_arguments` | `{"path":5}`・`{"unknown":1}`・`{"budget":10}`（`tz_brief`）・`{"budget":1000.0}`・`{}`（`tz_search`）・`{"query":"a","limit":0}`・`{"query":"a","kind":"x"}`・`{"id":"A","depth":9}`（`tz_callers`）・`{"kind":"calls","format":"svg"}`・`{"query":"E1012","extra":1}` → `isError: true` と本文 `invalid arguments: …`（上の表の文言） |
| `tool_results_conform_to_output_schemas` | `FakeBackend` で 11 ツール（`tz_check` は付録 C.2）を呼び、`structuredContent` を `schema::validate` で `outputSchema` に。`content[0].text` が空でない |
| `old_versions_get_text_only_results` | 2025-03-26・2024-11-05 → `structuredContent` がない |
| `text_over_the_limit_is_cut_and_marked` | 250,000 文字（`日` の列）→ 先頭 200,000 文字 + `\n… truncated: 50000 more characters`、`structuredContent.truncated == true` |
| `oversized_structured_content_becomes_an_error` | 400,001 bytes の `structured` → `isError`、本文 `result too large (` で始まる |
| `a_backend_error_becomes_an_error_result` | `Err(ToolError("x"))` → `isError: true`、本文 `x` |
| `a_panicking_tool_becomes_an_error_result_and_the_server_survives` | `panic!` → `isError`、本文 `internal error: tool 'tz_check' panicked; please report this`、次の `ping` が通る |

**群 C（手順 6。並行・取り消し・終了。10 件）**

| テスト | 入力 → 期待 |
| --- | --- |
| `ping_is_answered_while_a_tool_runs` | ゲートで止めた `tools/call` の最中の `ping` → `ping` の応答が先。ゲートを開くと `tools/call` の応答 |
| `tool_calls_run_in_order_one_at_a_time` | 3 件の `tools/call`（`id` 1・2・3）→ 応答は 1・2・3 の順。`calls` の記録が同じ順で、同時に 2 つ実行されない |
| `cancelling_the_running_request_drops_its_response` | 実行中の `id` を取り消し → ゲートを開く → 次の行が後続の `ping` の応答 |
| `cancelling_a_queued_request_never_runs_it` | 1 件目が止まっている間に 2 件目を取り消す → 2 件目は実行されず（`calls` に無い）、応答もない |
| `cancelling_unknown_requests_is_ignored` | 未知の `requestId`・終了済みの `id`・`initialize` の `id`・文字列 `"1"` と整数 `1` の取り違え → 応答なし・実行中のものに影響しない |
| `duplicate_in_flight_ids_are_rejected` | 実行中の `id` と同じ `id` の要求 → `-32600` `invalid request: request id is already in use` |
| `too_many_pending_requests_get_a_busy_error` | 止めた状態で 65 件 → 65 件目が `-32000` |
| `eof_drains_queued_requests_then_exits_ok` | 2 件の `tools/call` を送って入力を閉じる → 2 件の応答の後に `Ok(())` |
| `a_failed_write_is_fatal` | 出力が `BrokenPipe` を返す → `Err(Fatal(m))`、`m` に `cannot write to the client` |
| `blank_lines_and_crlf_are_accepted` | `\r\n` 終わりの行と空行を混ぜる → 普通に応答する |

**群 K（手順 7。`tz_check`・パス。8 件）**

| テスト | 入力 → 期待 |
| --- | --- |
| `tz_check_reports_the_moved_value_error` | 付録 G.1 の `Main.tz` → 付録 C.2 の `structuredContent` と `text` が等しい |
| `tz_check_reports_warnings_and_honors_the_warnings_flag` | 付録 G.1 の警告のソース → `ok: true`・警告 1 件。`warnings: false` → `warnings: 0`・`diagnostics: []` |
| `tz_check_matches_check_json` | 手順 7 の本文 |
| `tz_check_on_a_library_file_and_a_missing_main` | `Lib.tz` だけの root: `path: "Lib.tz"` → `ok: true`。`path` なし → `ok: false`、`E2001`、`path` は `Main.tz`、`message` に根の絶対パスがない |
| `tz_check_output_is_deterministic` | 2 回 → 応答の 1 行が同じ |
| `tz_check_never_shows_the_root_path` | 全応答の文字列に、根の絶対パス（`\` 区切りの形も）が含まれない |
| `resolve_path_table`（`path.rs`） | 「パスの閉じ込め」の表の全行（空・`.`・`a//b`・`../x`・`a/../b`・`/x`・`C:x`・`a\b`・NUL・4,097 文字・存在しない・ディレクトリでも `.tz` でもない・`#[cfg(unix)]` でシンボリックリンクを通る・根の外へのリンク） |
| `scrub_removes_the_root`（`path.rs`） | `cannot inspect source '/r/Main.tz': …` → `cannot inspect source 'Main.tz': …`、`/r` 自体 → `.` |

**群 L（手順 9。子プロセス・静的な検査。6 件）**

| テスト | 入力 → 期待 |
| --- | --- |
| `mcp_binary_speaks_only_json_lines_on_stdout` | `tsuzuri mcp --root <root>` に `initialize`→`initialized`→`tools/list`→`tools/call tz_check`→`ping` を流して stdin を閉じる → stdout の全行が JSON（`jsonrpc` が `"2.0"`）、標準エラーが空、終了コード 0 |
| `mcp_rejects_bad_arguments` | `mcp x`・`mcp --root`・`mcp --root a --root b`・`mcp --json`・`mcp --kb-dir` → `error[E2000]`、終了コード 2、標準出力が空 |
| `mcp_rejects_a_missing_root` | `mcp --root <存在しない>`・`--root <ファイル>` → `error[E2010]`、終了コード 1 |
| `mcp_invalid_thread_setting_is_e2000` | `TSUZURI_THREADS=0 tsuzuri mcp` → `E2000`、2 |
| `mcp_help_lists_the_command` | `tsuzuri --help` の標準出力に `tsuzuri mcp [--root DIR] [--kb-dir DIR]` |
| `mcp_sources_use_no_process_env_fs_writes_or_stdout` | `src/mcp/*.rs` に、禁止の語（下）を含む行がない。ただし `session.rs` の `std::thread::Builder` と、`#[cfg(test)]` の中と文字列リテラルは除く |

禁止の語: `std::process`・`Command::new`・`std::env`・`println!`・`print!(`・`eprint`・`dbg!`・`fs::write`・`File::create`・`OpenOptions`・`create_dir`・`remove_file`・`remove_dir`・`rename(`・`std::thread::spawn`（`Builder` を使う）。

**単体テスト（各ファイルの `#[cfg(test)]`）**

| ファイル | テスト |
| --- | --- |
| `src/mcp/rpc.rs`（4） | `parses_requests_notifications_and_invalid_messages`・`read_line_splits_strips_cr_and_skips_blank_lines`・`read_line_discards_an_oversized_line_and_resyncs`・`ids_round_trip_integers_and_strings` |
| `src/mcp/schema.rs`（4） | `validates_types_and_reports_the_first_error`・`integer_rejects_floats`・`required_is_checked_before_unknown_keys`・`unknown_keywords_are_reported` |
| `src/mcp/tools.rs`（5） | `tools_json_is_well_formed`・`tools_list_stays_small`（全ツール・2025-06-18 の応答を `serde_json::to_string` して 16,384 bytes 以下）・`list_downgrades_by_version`・`cap_text_cuts_at_a_character_boundary`・`shape_adds_truncated_and_enforces_the_structured_limit` |
| `src/main.rs`（1） | `parses_mcp_arguments`（`[]`・`--root a`・`--kb-dir b --root a`・重複・値なし・未知・位置引数） |

