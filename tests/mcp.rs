//! G29 (D-60): `tsuzuri mcp`, the protocol and `tz_check`. The expected messages are those of the
//! MCP specification's examples (G29 appendix C) and the compiler's own `check --json`; the
//! server's output is never copied into an expectation.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tsuzuri::mcp::schema;
use tsuzuri::mcp::{
    Backend, Cancel, Capabilities, Fatal, ToolError, ToolOutput, ToolRequest, serve, serve_with,
};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The input of the server: the bytes a test sends; dropping the sender is the end of the input.
struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    position: usize,
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.position == self.buffer.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.buffer = chunk;
                    self.position = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let count = (self.buffer.len() - self.position).min(out.len());
        out[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

/// The output of the server, line by line.
#[derive(Clone, Default)]
struct Shared(Arc<(Mutex<Vec<u8>>, Condvar)>);

impl Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let (lock, changed) = &*self.0;
        lock.lock().unwrap().extend_from_slice(bytes);
        changed.notify_all();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Client {
    to_server: Option<mpsc::Sender<Vec<u8>>>,
    output: Shared,
    read: Mutex<usize>,
    server: Option<std::thread::JoinHandle<Result<(), Fatal>>>,
}

impl Client {
    fn start(backend: Arc<dyn Backend>) -> Client {
        Self::start_with(move |input, output| serve_with(backend, input, output))
    }

    fn project(root: &Path) -> Client {
        let root = root.to_path_buf();
        Self::start_with(move |input, output| {
            serve(tsuzuri::mcp::Options { root, kg_dir: None }, input, output)
        })
    }

    fn start_with(
        run: impl FnOnce(ChannelReader, Shared) -> Result<(), Fatal> + Send + 'static,
    ) -> Client {
        let (tx, rx) = mpsc::channel();
        let output = Shared::default();
        let writer = output.clone();
        let server = std::thread::spawn(move || {
            run(
                ChannelReader {
                    rx,
                    buffer: Vec::new(),
                    position: 0,
                },
                writer,
            )
        });
        Client {
            to_server: Some(tx),
            output,
            read: Mutex::new(0),
            server: Some(server),
        }
    }

    fn send(&self, message: Value) {
        self.send_raw(format!("{message}\n").as_bytes());
    }

    fn send_raw(&self, bytes: &[u8]) {
        self.to_server
            .as_ref()
            .unwrap()
            .send(bytes.to_vec())
            .unwrap();
    }

    /// The next line of the server; a test fails after 30 seconds without one.
    fn recv(&self) -> Value {
        let (lock, changed) = &*self.output.0;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut read = self.read.lock().unwrap();
        let mut bytes = lock.lock().unwrap();
        loop {
            if let Some(end) = bytes[*read..].iter().position(|byte| *byte == b'\n') {
                let line = bytes[*read..*read + end].to_vec();
                *read += end + 1;
                return serde_json::from_slice(&line).unwrap();
            }
            let now = Instant::now();
            assert!(now < deadline, "no answer from the server");
            bytes = changed.wait_timeout(bytes, deadline - now).unwrap().0;
        }
    }

    fn request(&self, id: i64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.recv()
    }

    fn initialize(&self, version: &str) -> Value {
        self.request(
            0,
            "initialize",
            json!({"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
        )
    }

    fn call(&self, id: i64, name: &str, arguments: Value) -> Value {
        self.request(
            id,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )
    }

    /// The next line is the answer to a `ping`: nothing came before it.
    fn assert_quiet(&self, id: i64) {
        assert_eq!(
            self.request(id, "ping", json!({})),
            json!({"jsonrpc": "2.0", "id": id, "result": {}})
        );
    }

    fn finish(mut self) -> Result<(), Fatal> {
        drop(self.to_server.take());
        self.server.take().unwrap().join().unwrap()
    }
}

fn error_of(message: &Value) -> (i64, String) {
    (
        message["error"]["code"].as_i64().unwrap(),
        message["error"]["message"].as_str().unwrap().to_owned(),
    )
}

fn tools_json() -> Vec<Value> {
    serde_json::from_str(&std::fs::read_to_string(repository().join("src/mcp/tools.json")).unwrap())
        .unwrap()
}

fn examples() -> Vec<Value> {
    serde_json::from_str(
        &std::fs::read_to_string(repository().join("tests/fixtures/mcp/tool-examples.json"))
            .unwrap(),
    )
    .unwrap()
}

/// The `structuredContent` of `tz_check` for the failing program of `bad_root`.
fn check_example() -> Value {
    json!({
        "ok": false, "files": 1, "errors": 1, "warnings": 0, "omitted": 0, "truncated": false,
        "diagnostics": [{
            "severity": "error", "code": "E1012",
            "message": "use of moved or partially moved value 'text'",
            "path": "Main.tz",
            "span": {"start": 151, "end": 155, "line": 5, "column": 26, "end_line": 5, "end_column": 30},
            "kind": "use_after_move",
            "related": [{"role": "moved_here", "message": "value moved here", "path": "Main.tz",
                         "span": {"start": 121, "end": 125, "line": 4, "column": 25, "end_line": 4, "end_column": 29}}],
            "help": "borrow the value with 'ref text' or move it last"
        }]
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Examples,
    Error,
    Panic,
    LongText,
    HugeStructured,
}

/// A backend that answers with the examples, can hold its calls at a gate, and records them.
struct FakeBackend {
    caps: Capabilities,
    mode: Mode,
    calls: Mutex<Vec<String>>,
    running: AtomicUsize,
    overlapped: AtomicUsize,
    gate: Option<Arc<(Mutex<bool>, Condvar)>>,
}

impl FakeBackend {
    fn new(caps: Capabilities, mode: Mode) -> Self {
        Self {
            caps,
            mode,
            calls: Mutex::new(Vec::new()),
            running: AtomicUsize::new(0),
            overlapped: AtomicUsize::new(0),
            gate: None,
        }
    }

    fn gated(caps: Capabilities) -> (Arc<Self>, Arc<(Mutex<bool>, Condvar)>) {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let backend = Arc::new(Self {
            gate: Some(Arc::clone(&gate)),
            ..Self::new(caps, Mode::Examples)
        });
        (backend, gate)
    }
}

fn open(gate: &(Mutex<bool>, Condvar)) {
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
}

impl Backend for FakeBackend {
    fn capabilities(&self) -> Capabilities {
        self.caps
    }

    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError> {
        if self.running.fetch_add(1, Ordering::SeqCst) > 0 {
            self.overlapped.fetch_add(1, Ordering::SeqCst);
        }
        self.calls
            .lock()
            .unwrap()
            .push(format!("{} {}", request.name, request.arguments));
        if let Some(gate) = &self.gate {
            let mut open = gate.0.lock().unwrap();
            while !*open {
                open = gate.1.wait(open).unwrap();
            }
        }
        self.running.fetch_sub(1, Ordering::SeqCst);
        match self.mode {
            Mode::Error => return Err(ToolError("x".into())),
            Mode::Panic => panic!("the fake backend panics"),
            Mode::LongText => {
                return Ok(ToolOutput {
                    text: "日".repeat(250_000),
                    structured: json!({}),
                });
            }
            Mode::HugeStructured => {
                return Ok(ToolOutput {
                    text: "t".into(),
                    structured: json!({"blob": "x".repeat(400_001)}),
                });
            }
            Mode::Examples => {}
        }
        if request.name == "tz_check" {
            let mut structured = check_example();
            structured.as_object_mut().unwrap().remove("truncated");
            return Ok(ToolOutput {
                text: "Main.tz:5:26: error[E1012]: use of moved or partially moved value 'text'"
                    .into(),
                structured,
            });
        }
        let example = examples()
            .into_iter()
            .find(|example| example["tool"] == request.name)
            .expect("an example of every tool");
        Ok(ToolOutput {
            text: example["text"].as_str().unwrap().to_owned(),
            structured: example["structured"].clone(),
        })
    }

    fn read_resource(&self, uri: &str, _cancel: &Cancel) -> Result<Option<String>, ToolError> {
        Ok(match uri {
            "tz://guide/L0" => Some("# Tsuzuri card\n".into()),
            "tz://kg/brief/Point%3A%3APoint.hypotenuse" => {
                Some("# Point::Point.hypotenuse\n".into())
            }
            _ => None,
        })
    }
}

fn fake(caps: Capabilities) -> Client {
    Client::start(Arc::new(FakeBackend::new(caps, Mode::Examples)))
}

fn initialized(caps: Capabilities) -> Client {
    let client = fake(caps);
    client.initialize("2025-06-18");
    client
}

// --- Group P: the protocol -------------------------------------------------------------------

#[test]
fn initialize_negotiates_the_protocol_version() {
    for (asked, answered) in [
        ("2025-06-18", "2025-06-18"),
        ("2025-03-26", "2025-03-26"),
        ("2024-11-05", "2024-11-05"),
        ("2025-11-25", "2025-06-18"),
        ("2026-07-28", "2025-06-18"),
        ("1.0.0", "2025-06-18"),
        ("", "2025-06-18"),
    ] {
        let client = fake(Capabilities::all());
        let answer = client.initialize(asked);
        assert_eq!(answer["result"]["protocolVersion"], answered, "{asked}");
        assert_eq!(
            error_of(&client.initialize(asked)),
            (-32600, "server is already initialized".into())
        );
    }
    for params in [json!({}), json!({"protocolVersion": 5})] {
        let client = fake(Capabilities::all());
        assert_eq!(
            error_of(&client.request(1, "initialize", params)),
            (
                -32602,
                "invalid params: protocolVersion must be a string".into()
            )
        );
    }
}

#[test]
fn initialize_result_matches_the_appendix_examples() {
    let instructions = "Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.";
    assert_eq!(instructions.chars().count(), 347);
    let version = env!("CARGO_PKG_VERSION");
    let client = fake(Capabilities::all());
    assert_eq!(
        client.initialize("2025-06-18"),
        json!({"jsonrpc": "2.0", "id": 0, "result": {
            "protocolVersion": "2025-06-18",
            "capabilities": {"resources": {}, "tools": {"listChanged": false}},
            "serverInfo": {"name": "tsuzuri", "title": "Tsuzuri compiler", "version": version},
            "instructions": instructions,
        }})
    );
    let client = fake(Capabilities::all());
    assert_eq!(
        client.initialize("2025-03-26"),
        json!({"jsonrpc": "2.0", "id": 0, "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": {"resources": {}, "tools": {"listChanged": false}},
            "serverInfo": {"name": "tsuzuri", "version": version},
            "instructions": instructions,
        }})
    );
}

#[test]
fn ping_is_answered_in_every_state() {
    let client = fake(Capabilities::all());
    client.assert_quiet(1);
    client.initialize("2025-06-18");
    client.assert_quiet(2);
    assert!(client.finish().is_ok());
}

#[test]
fn requests_before_initialize_are_rejected() {
    let client = fake(Capabilities::all());
    for (id, method) in [(1, "tools/list"), (2, "tools/call"), (3, "resources/list")] {
        assert_eq!(
            client.request(id, method, json!({"name": "tz_check"})),
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32600, "message": "server not initialized: send initialize first"}})
        );
    }
    client.assert_quiet(4);
    client.initialize("2025-06-18");
    assert!(client.request(5, "tools/list", json!({}))["result"]["tools"].is_array());
}

#[test]
fn notifications_never_get_a_response() {
    let client = initialized(Capabilities::all());
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/unknown", "params": {}}));
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": [1]}));
    client.send(
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 99}}),
    );
    client.send(json!({"jsonrpc": "2.0", "method": "tools/list"}));
    client.assert_quiet(1);
}

#[test]
fn framing_errors_are_reported_and_the_session_continues() {
    let client = initialized(Capabilities::all());
    let parse = |line: &[u8]| {
        client.send_raw(line);
        client.send_raw(b"\n");
        client.recv()
    };
    let answer = parse(b"}");
    assert_eq!(answer["id"], Value::Null);
    assert_eq!(answer["error"]["code"], -32700);
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("parse error: ")
    );
    for (line, id, code, message) in [
        (
            "[]",
            Value::Null,
            -32600,
            "invalid request: batch requests are not supported",
        ),
        (
            "5",
            Value::Null,
            -32600,
            "invalid request: expected a JSON object",
        ),
        (
            "\"x\"",
            Value::Null,
            -32600,
            "invalid request: expected a JSON object",
        ),
        (
            r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
            json!(1),
            -32600,
            "invalid request: jsonrpc must be \"2.0\"",
        ),
        (
            r#"{"jsonrpc":"2.0","id":2}"#,
            json!(2),
            -32600,
            "invalid request: method must be a string",
        ),
        (
            r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#,
            Value::Null,
            -32600,
            "invalid request: id must be a string or an integer",
        ),
        (
            r#"{"jsonrpc":"2.0","id":true,"method":"ping"}"#,
            Value::Null,
            -32600,
            "invalid request: id must be a string or an integer",
        ),
        (
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            Value::Null,
            -32600,
            "invalid request: id must be a string or an integer",
        ),
        (
            r#"{"jsonrpc":"2.0","id":{},"method":"ping"}"#,
            Value::Null,
            -32600,
            "invalid request: id must be a string or an integer",
        ),
    ] {
        assert_eq!(
            parse(line.as_bytes()),
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
            "{line}"
        );
    }
    let long = vec![b'x'; 4_194_305];
    assert_eq!(
        parse(&long),
        json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "invalid request: message exceeds 4194304 bytes"}})
    );
    client.assert_quiet(3);
}

#[test]
fn unknown_methods_are_method_not_found() {
    let client = fake(Capabilities::all());
    assert_eq!(
        client.request(1, "server/discover", json!({})),
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "method not found: server/discover"}})
    );
    client.initialize("2025-06-18");
    for (id, method) in [
        (2, "server/discover"),
        (3, "prompts/list"),
        (4, "logging/setLevel"),
        (5, "completion/complete"),
        (6, "resources/subscribe"),
    ] {
        assert_eq!(
            error_of(&client.request(id, method, json!({}))),
            (-32601, format!("method not found: {method}"))
        );
    }
}

// --- Group T: the tools and their results ----------------------------------------------------

#[test]
fn tools_list_matches_tools_json_for_every_version() {
    for (version, title, annotations) in [
        ("2025-06-18", true, true),
        ("2025-03-26", false, true),
        ("2024-11-05", false, false),
    ] {
        let client = fake(Capabilities::all());
        client.initialize(version);
        let expected: Vec<Value> = tools_json()
            .into_iter()
            .map(|mut tool| {
                let object = tool.as_object_mut().unwrap();
                if !title {
                    object.remove("title");
                    object.remove("outputSchema");
                }
                if !annotations {
                    object.remove("annotations");
                }
                tool
            })
            .collect();
        assert_eq!(expected.len(), 12);
        let answer = client.request(1, "tools/list", json!({}));
        assert_eq!(
            answer["result"]["tools"],
            Value::Array(expected),
            "{version}"
        );
    }
}

#[test]
fn tools_list_hides_unavailable_tools() {
    let client = initialized(Capabilities {
        check: true,
        ..Capabilities::none()
    });
    let tools = client.request(1, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["tz_check"]);
    assert_eq!(
        error_of(&client.call(2, "tz_guide", json!({}))),
        (-32602, "unknown tool: tz_guide".into())
    );
}

#[test]
fn tools_list_rejects_a_cursor() {
    let client = initialized(Capabilities::all());
    for (id, cursor) in [(1, json!("x")), (2, json!(""))] {
        assert_eq!(
            error_of(&client.request(id, "tools/list", json!({"cursor": cursor}))),
            (-32602, "invalid params: invalid cursor".into())
        );
    }
}

#[test]
fn tools_call_protocol_errors() {
    let client = initialized(Capabilities::all());
    client.send(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call"}));
    assert_eq!(
        error_of(&client.recv()),
        (-32602, "invalid params: expected an object".into())
    );
    assert_eq!(
        error_of(&client.request(2, "tools/call", json!({"name": 5}))),
        (-32602, "invalid params: name must be a string".into())
    );
    assert_eq!(
        error_of(&client.request(
            3,
            "tools/call",
            json!({"name": "tz_check", "arguments": []})
        )),
        (-32602, "invalid params: arguments must be an object".into())
    );
    assert_eq!(
        client.call(7, "tz_nothing", json!({})),
        json!({"jsonrpc": "2.0", "id": 7, "error": {"code": -32602, "message": "unknown tool: tz_nothing"}})
    );
}

#[test]
fn tools_call_validates_arguments() {
    let client = initialized(Capabilities::all());
    for (index, (tool, arguments, message)) in [
        ("tz_check", json!({"path": 5}), "/path: must be a string"),
        ("tz_check", json!({"unknown": 1}), "/unknown: unknown property"),
        ("tz_brief", json!({"budget": 10}), "/budget: must be at least 200"),
        ("tz_brief", json!({"budget": 1000.0}), "/budget: must be an integer"),
        ("tz_search", json!({}), "/query: is required"),
        ("tz_search", json!({"query": "a", "limit": 0}), "/limit: must be at least 1"),
        ("tz_search", json!({"query": "a", "kind": "x"}), "/kind: must be one of \"module\", \"def\", \"const\", \"record\", \"union\", \"case\", \"field\", \"class\", \"method\", \"instance\", \"alias\", \"test\", \"bench\", \"extern\", \"handle\""),
        ("tz_callers", json!({"id": "A", "depth": 9}), "/depth: must be at most 8"),
        ("tz_path", json!({"from": "a"}), "/to: is required"),
        ("tz_diagram", json!({"kind": "calls", "format": "svg"}), "/format: must be one of \"mermaid\", \"dot\""),
        ("tz_explain", json!({"query": "E1012", "extra": 1}), "/extra: unknown property"),
    ]
    .into_iter()
    .enumerate()
    {
        let answer = client.call(index as i64 + 1, tool, arguments);
        assert_eq!(
            answer["result"],
            json!({"content": [{"type": "text", "text": format!("invalid arguments: {message}")}], "isError": true}),
            "{tool}"
        );
    }
}

#[test]
fn tool_results_conform_to_output_schemas() {
    let client = initialized(Capabilities::all());
    for (index, tool) in tools_json().iter().enumerate() {
        let name = tool["name"].as_str().unwrap();
        let arguments = examples()
            .into_iter()
            .find(|example| example["tool"] == name)
            .map_or(json!({}), |example| example["arguments"].clone());
        let answer = client.call(index as i64 + 1, name, arguments);
        let result = &answer["result"];
        assert!(result.get("isError").is_none(), "{name}: {result}");
        assert!(
            !result["content"][0]["text"].as_str().unwrap().is_empty(),
            "{name}"
        );
        if let Err(error) = schema::validate(&tool["outputSchema"], &result["structuredContent"]) {
            panic!("{name}: {error}");
        }
        if name == "tz_check" {
            assert_eq!(result["structuredContent"], check_example());
        }
    }
}

#[test]
fn old_versions_get_text_only_results() {
    for version in ["2025-03-26", "2024-11-05"] {
        let client = fake(Capabilities::all());
        client.initialize(version);
        let answer = client.call(1, "tz_check", json!({}));
        assert!(
            answer["result"].get("structuredContent").is_none(),
            "{version}"
        );
        assert!(answer["result"]["content"][0]["text"].is_string());
    }
}

#[test]
fn text_over_the_limit_is_cut_and_marked() {
    let client = Client::start(Arc::new(FakeBackend::new(
        Capabilities::all(),
        Mode::LongText,
    )));
    client.initialize("2025-06-18");
    let answer = client.call(1, "tz_guide", json!({}));
    let text = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        text,
        format!(
            "{}\n\u{2026} truncated: 50000 more characters",
            "日".repeat(200_000)
        )
    );
    assert_eq!(answer["result"]["structuredContent"]["truncated"], true);
}

#[test]
fn oversized_structured_content_becomes_an_error() {
    let client = Client::start(Arc::new(FakeBackend::new(
        Capabilities::all(),
        Mode::HugeStructured,
    )));
    client.initialize("2025-06-18");
    let answer = client.call(1, "tz_guide", json!({}));
    assert_eq!(answer["result"]["isError"], true);
    assert!(
        answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("result too large (")
    );
}

#[test]
fn a_backend_error_becomes_an_error_result() {
    let client = Client::start(Arc::new(FakeBackend::new(Capabilities::all(), Mode::Error)));
    client.initialize("2025-06-18");
    assert_eq!(
        client.call(1, "tz_check", json!({}))["result"],
        json!({"content": [{"type": "text", "text": "x"}], "isError": true})
    );
}

#[test]
fn a_panicking_tool_becomes_an_error_result_and_the_server_survives() {
    let client = Client::start(Arc::new(FakeBackend::new(Capabilities::all(), Mode::Panic)));
    client.initialize("2025-06-18");
    assert_eq!(
        client.call(1, "tz_check", json!({}))["result"],
        json!({"content": [{"type": "text", "text": "internal error: tool 'tz_check' panicked; please report this"}], "isError": true})
    );
    client.assert_quiet(2);
}

// --- Group C: concurrency, cancellation and the end ------------------------------------------

#[test]
fn ping_is_answered_while_a_tool_runs() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend);
    client.initialize("2025-06-18");
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    client.assert_quiet(2);
    open(&gate);
    assert_eq!(client.recv()["id"], 1);
}

#[test]
fn tool_calls_run_in_order_one_at_a_time() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend.clone());
    client.initialize("2025-06-18");
    for id in 1..=3 {
        client.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "tz_check", "arguments": {"path": format!("{id}.tz")}}}));
    }
    open(&gate);
    for id in 1..=3 {
        assert_eq!(client.recv()["id"], id);
    }
    assert_eq!(
        *backend.calls.lock().unwrap(),
        [
            "tz_check {\"path\":\"1.tz\"}",
            "tz_check {\"path\":\"2.tz\"}",
            "tz_check {\"path\":\"3.tz\"}"
        ]
    );
    assert_eq!(backend.overlapped.load(Ordering::SeqCst), 0);
}

fn cancel(client: &Client, id: Value) {
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": id, "reason": "test"}}));
}

#[test]
fn cancelling_the_running_request_drops_its_response() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend.clone());
    client.initialize("2025-06-18");
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    // The call is running when its record exists.
    while backend.calls.lock().unwrap().is_empty() {
        std::thread::yield_now();
    }
    cancel(&client, json!(1));
    client.assert_quiet(2);
    open(&gate);
    client.assert_quiet(3);
}

#[test]
fn cancelling_a_queued_request_never_runs_it() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend.clone());
    client.initialize("2025-06-18");
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    client.send(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "tz_guide"}}),
    );
    cancel(&client, json!(2));
    client.assert_quiet(3);
    open(&gate);
    assert_eq!(client.recv()["id"], 1);
    client.assert_quiet(4);
    assert_eq!(backend.calls.lock().unwrap().len(), 1);
}

#[test]
fn cancelling_unknown_requests_is_ignored() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend);
    client.initialize("2025-06-18");
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    for id in [json!(42), json!(0), json!("1"), json!(null)] {
        cancel(&client, id);
    }
    client.assert_quiet(2);
    open(&gate);
    assert_eq!(client.recv()["id"], 1);
}

#[test]
fn duplicate_in_flight_ids_are_rejected() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend);
    client.initialize("2025-06-18");
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    assert_eq!(
        error_of(&client.call(1, "tz_check", json!({}))),
        (
            -32600,
            "invalid request: request id is already in use".into()
        )
    );
    open(&gate);
    assert_eq!(client.recv()["id"], 1);
}

#[test]
fn too_many_pending_requests_get_a_busy_error() {
    let (backend, gate) = FakeBackend::gated(Capabilities::all());
    let client = Client::start(backend);
    client.initialize("2025-06-18");
    for id in 1..=65 {
        client.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "tz_check"}}));
    }
    assert_eq!(
        client.recv(),
        json!({"jsonrpc": "2.0", "id": 65, "error": {"code": -32000, "message": "too many pending requests (limit 64)"}})
    );
    open(&gate);
    for id in 1..=64 {
        assert_eq!(client.recv()["id"], id);
    }
}

#[test]
fn eof_drains_queued_requests_then_exits_ok() {
    let client = initialized(Capabilities::all());
    client.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "tz_check"}}),
    );
    client.send(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "tz_guide"}}),
    );
    let mut client = client;
    drop(client.to_server.take());
    assert_eq!(client.recv()["id"], 1);
    assert_eq!(client.recv()["id"], 2);
    assert!(client.server.take().unwrap().join().unwrap().is_ok());
}

struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failed_write_is_fatal() {
    let input = io::Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec());
    let backend = Arc::new(FakeBackend::new(Capabilities::all(), Mode::Examples));
    let Err(Fatal(message)) = serve_with(backend, input, Broken) else {
        panic!("a write to a broken pipe must end the server");
    };
    assert!(message.contains("cannot write to the client"), "{message}");
}

#[test]
fn blank_lines_and_crlf_are_accepted() {
    let client = fake(Capabilities::all());
    client.send_raw(b"\r\n\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n\r\n");
    assert_eq!(
        client.recv(),
        json!({"jsonrpc": "2.0", "id": 1, "result": {}})
    );
}

// --- Group K: tz_check on a project -----------------------------------------------------------

fn temporary_root(files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-mcp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        std::fs::write(root.join(path), text).unwrap();
    }
    // The form that `tsuzuri mcp` gives the backend (not `\\?\` on Windows).
    tsuzuri::mcp::project_root(&root).unwrap()
}

const BAD: &str = "def consume :: own string -> i64 = \\text -> text.length\n\ndef run :: own string -> i64 = \\text ->\n    let first = consume text\n    let second = consume text\n    first + second\n\nrun \"abc\"\n";
const WARN: &str = "def scale :: i64 -> i64 = \\n ->\n    let spare = 1\n    n * 2\n\nscale 21\n";

/// `text` with the CLI's spelling of `Main.tz` given from the root (`./Main.tz`, `.\Main.tz` on
/// Windows, escaped in JSON) as the root-relative name that the server reports.
fn from_root(text: &str) -> String {
    text.replace(r".\\Main.tz", "Main.tz")
        .replace(r".\Main.tz", "Main.tz")
        .replace("./Main.tz", "Main.tz")
}

fn tsuzuri(cwd: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(cwd)
        .args(arguments)
        .env_remove("TSUZURI_THREADS")
        .output()
        .unwrap()
}

#[test]
fn tz_check_reports_the_moved_value_error() {
    let root = temporary_root(&[("Main.tz", BAD)]);
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    let answer = client.call(1, "tz_check", json!({"path": "Main.tz"}));
    assert_eq!(answer["result"]["structuredContent"], check_example());
    // The text is that of `tsuzuri check Main.tz`, from the root, with the summary.
    let output = tsuzuri(&root, &["check", "Main.tz"]);
    let expected = format!(
        "{}\n\nFAILED: 1 error, 0 warnings in 1 file",
        from_root(String::from_utf8_lossy(&output.stderr).trim_end())
    );
    assert_eq!(answer["result"]["content"][0]["text"], expected);
}

#[test]
fn tz_check_reports_warnings_and_honors_the_warnings_flag() {
    let root = temporary_root(&[("Main.tz", WARN)]);
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    let answer = client.call(1, "tz_check", json!({}));
    let structured = &answer["result"]["structuredContent"];
    assert_eq!(structured["ok"], true);
    assert_eq!(structured["warnings"], 1);
    assert_eq!(structured["diagnostics"][0]["code"], "W1001");
    let answer = client.call(2, "tz_check", json!({"warnings": false}));
    let structured = &answer["result"]["structuredContent"];
    assert_eq!(structured["warnings"], 0);
    assert_eq!(structured["diagnostics"], json!([]));
    assert_eq!(
        answer["result"]["content"][0]["text"],
        "OK: 0 errors, 0 warnings in 1 file"
    );
}

#[test]
fn tz_check_matches_check_json() {
    for source in [BAD, WARN] {
        let root = temporary_root(&[("Main.tz", source)]);
        let output = tsuzuri(&root, &["check", "Main.tz", "--json"]);
        let mut expected: Vec<Value> = String::from_utf8_lossy(&output.stderr)
            .lines()
            .map(|line| serde_json::from_str(&from_root(line)).unwrap())
            .collect();
        expected.retain(|row: &Value| row["severity"] != "note");
        let client = Client::project(&root);
        client.initialize("2025-06-18");
        let answer = client.call(1, "tz_check", json!({"path": "Main.tz"}));
        assert_eq!(
            answer["result"]["structuredContent"]["diagnostics"],
            Value::Array(expected)
        );
    }
}

#[test]
fn tz_check_on_a_library_file_and_a_missing_main() {
    let root = temporary_root(&[("Lib.tz", "def twice :: i64 -> i64 = \\n -> n * 2\n")]);
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    let answer = client.call(1, "tz_check", json!({"path": "Lib.tz"}));
    assert_eq!(answer["result"]["structuredContent"]["ok"], true);
    let answer = client.call(2, "tz_check", json!({}));
    let structured = &answer["result"]["structuredContent"];
    assert_eq!(structured["ok"], false);
    assert_eq!(structured["files"], 0);
    assert_eq!(structured["diagnostics"][0]["code"], "E2001");
    assert_eq!(structured["diagnostics"][0]["path"], "Main.tz");
    let message = structured["diagnostics"][0]["message"].as_str().unwrap();
    assert!(!message.contains(root.to_str().unwrap()), "{message}");
}

#[test]
fn tz_check_output_is_deterministic() {
    let root = temporary_root(&[("Main.tz", BAD)]);
    let first = {
        let client = Client::project(&root);
        client.initialize("2025-06-18");
        client.call(1, "tz_check", json!({}))
    };
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    assert_eq!(
        client.call(1, "tz_check", json!({})).to_string(),
        first.to_string()
    );
}

#[test]
fn tz_check_never_shows_the_root_path() {
    let root = temporary_root(&[("Main.tz", BAD)]);
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    let mut all = String::new();
    for (id, arguments) in [
        (1, json!({})),
        (2, json!({"path": "Main.tz"})),
        (3, json!({"path": "missing.tz"})),
        (4, json!({"path": "../x"})),
    ] {
        all.push_str(&client.call(id, "tz_check", arguments).to_string());
    }
    let root_text = root.to_str().unwrap();
    assert!(!all.contains(root_text), "{all}");
    assert!(!all.contains(&root_text.replace('/', "\\")), "{all}");
    assert!(
        all.contains("no such file or directory: 'missing.tz'"),
        "{all}"
    );
}

// --- Group L: the binary and the sources -----------------------------------------------------

#[test]
fn mcp_binary_speaks_only_json_lines_on_stdout() {
    let root = temporary_root(&[("Main.tz", WARN)]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["mcp", "--root", root.to_str().unwrap()])
        .env_remove("TSUZURI_THREADS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "tz_check"}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "ping"}),
    ]
    .iter()
    .map(|message| format!("{message}\n"))
    .collect::<String>();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().all(|line| line["jsonrpc"] == "2.0"));
    // `ping` may be answered before the tool's result: find the answers by id.
    let check = lines.iter().find(|line| line["id"] == 3).unwrap();
    assert_eq!(check["result"]["structuredContent"]["warnings"], 1);
    assert!(!root.join("target").exists());
}

#[test]
fn mcp_rejects_bad_arguments() {
    let cwd = std::env::temp_dir();
    for arguments in [
        vec!["mcp", "x"],
        vec!["mcp", "--root"],
        vec!["mcp", "--root", "a", "--root", "b"],
        vec!["mcp", "--json"],
        vec!["mcp", "--kg-dir"],
    ] {
        let output = tsuzuri(&cwd, &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("error[E2000]"),
            "{arguments:?}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn mcp_rejects_a_missing_root() {
    let root = temporary_root(&[("Main.tz", WARN)]);
    for given in [root.join("missing"), root.join("Main.tz")] {
        let output = tsuzuri(&root, &["mcp", "--root", given.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error[E2010]: project root '"), "{stderr}");
    }
}

#[test]
fn mcp_invalid_thread_setting_is_e2000() {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("mcp")
        .env("TSUZURI_THREADS", "0")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error[E2000]"));
}

#[test]
fn mcp_help_lists_the_command() {
    let output = tsuzuri(&std::env::temp_dir(), &["--help"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("tsuzuri mcp [--root DIR] [--kg-dir DIR]"),
        "{text}"
    );
}

#[test]
fn mcp_sources_use_no_process_env_fs_writes_or_stdout() {
    let forbidden = [
        "std::process",
        "Command::new",
        "std::env",
        "println!",
        "print!(",
        "eprint",
        "dbg!",
        "fs::write",
        "File::create",
        "OpenOptions",
        "create_dir",
        "remove_file",
        "remove_dir",
        "rename(",
        "std::thread::spawn",
    ];
    let directory = repository().join("src/mcp");
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        for (number, line) in code.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for word in forbidden {
                assert!(
                    !line.contains(word),
                    "{}:{}: {word}: {line}",
                    path.display(),
                    number + 1
                );
            }
        }
    }
}

// --- Phase 3: resources -----------------------------------------------------------------------

#[test]
fn resources_are_declared_and_listed_as_in_the_appendix() {
    let client = initialized(Capabilities::all());
    assert_eq!(
        client.request(1, "resources/list", json!({}))["result"],
        json!({"resources": [
            {"description": "Short guide to the language that matches this compiler.", "mimeType": "text/markdown", "name": "guide-l0", "title": "Tsuzuri language card", "uri": "tz://guide/L0"},
            {"description": "Full guide to the language that matches this compiler.", "mimeType": "text/markdown", "name": "guide-l1", "title": "Tsuzuri language guide", "uri": "tz://guide/L1"},
            {"description": "Summary card of the whole project.", "mimeType": "text/markdown", "name": "brief-l0", "title": "Project brief", "uri": "tz://kg/brief/L0"}
        ]})
    );
    assert_eq!(
        client.request(2, "resources/templates/list", json!({}))["result"],
        json!({"resourceTemplates": [
            {"description": "{id} is a knowledge-graph id, percent-encoded.", "mimeType": "text/markdown", "name": "brief", "title": "Brief card of a module, function or type", "uriTemplate": "tz://kg/brief/{id}"},
            {"description": "{kind} is a diagnostic code such as E1012 or a kind such as use_after_move.", "mimeType": "text/markdown", "name": "explain", "title": "Explanation of a diagnostic", "uriTemplate": "tz://explain/{kind}"}
        ]})
    );
    assert_eq!(
        error_of(&client.request(3, "resources/list", json!({"cursor": "x"}))),
        (-32602, "invalid params: invalid cursor".into())
    );
}

#[test]
fn resources_read_returns_markdown() {
    let client = initialized(Capabilities::all());
    assert_eq!(
        client.request(
            1,
            "resources/read",
            json!({"uri": "tz://kg/brief/Point%3A%3APoint.hypotenuse"})
        )["result"],
        json!({"contents": [{"uri": "tz://kg/brief/Point%3A%3APoint.hypotenuse", "mimeType": "text/markdown", "text": "# Point::Point.hypotenuse\n"}]})
    );
}

#[test]
fn resources_read_not_found_is_32002() {
    let client = initialized(Capabilities::all());
    assert_eq!(
        client.request(1, "resources/read", json!({"uri": "tz://explain/E9999"})),
        json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32002, "message": "resource not found: tz://explain/E9999", "data": {"uri": "tz://explain/E9999"}}})
    );
    assert_eq!(
        error_of(&client.request(2, "resources/read", json!({"uri": 5}))),
        (-32602, "invalid params: uri must be a string".into())
    );
}

#[test]
fn resources_uri_decoding_table() {
    let client = initialized(Capabilities::all());
    for (id, uri) in [(1, "tz://kg/brief/%"), (2, "tz://kg/brief/%G0")] {
        assert_eq!(
            error_of(&client.request(id, "resources/read", json!({"uri": uri}))),
            (
                -32602,
                "invalid params: invalid percent-encoding in uri".into()
            ),
            "{uri}"
        );
    }
}

#[test]
fn resources_follow_capabilities() {
    let client = initialized(Capabilities {
        check: true,
        ..Capabilities::none()
    });
    assert_eq!(
        client.request(1, "resources/list", json!({}))["result"],
        json!({"resources": []})
    );
    assert_eq!(
        client.request(2, "resources/templates/list", json!({}))["result"],
        json!({"resourceTemplates": []})
    );
}

// --- Phase 4: hostile inputs ------------------------------------------------------------------

#[test]
fn hostile_inputs_never_break_the_session() {
    let root = temporary_root(&[("Main.tz", WARN)]);
    let client = Client::project(&root);
    client.initialize("2025-06-18");
    let mut next = 100;
    let mut expect = |client: &Client, line: Vec<u8>, check: &dyn Fn(&Value)| {
        client.send_raw(&line);
        client.send_raw(b"\n");
        check(&client.recv());
        next += 1;
        client.assert_quiet(next);
    };
    let code =
        |code: i64| move |answer: &Value| assert_eq!(answer["error"]["code"], code, "{answer}");
    let is_error = |answer: &Value| assert_eq!(answer["result"]["isError"], true, "{answer}");
    expect(&client, vec![0xFF, 0xFE], &code(-32700));
    let nested = format!("{}{}", "[".repeat(129), "]".repeat(129));
    expect(&client, nested.into_bytes(), &code(-32700));
    expect(
        &client,
        br#"{"jsonrpc":"2.0","id":1e400,"method":"ping"}"#.to_vec(),
        &code(-32700),
    );
    expect(
        &client,
        br#"{"jsonrpc":"2.0","id":18446744073709551616,"method":"ping"}"#.to_vec(),
        &code(-32600),
    );
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": i64::MIN, "method": "ping"})
            .to_string()
            .into_bytes(),
        &|answer: &Value| assert_eq!(answer["id"], json!(i64::MIN)),
    );
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": 1, "method": "x".repeat(1 << 20)})
            .to_string()
            .into_bytes(),
        &code(-32601),
    );
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "x".repeat(1 << 20)}}).to_string().into_bytes(),
        &code(-32602),
    );
    let many: serde_json::Map<String, Value> = (0..10_000)
        .map(|index| (format!("k{index}"), json!(index)))
        .collect();
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "tz_check", "arguments": many}}).to_string().into_bytes(),
        &is_error,
    );
    for path in ["../", "..\\", "//etc", "C:\\", "a\u{0}b", &"a".repeat(4097)] {
        expect(
            &client,
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "tz_check", "arguments": {"path": path}}}).to_string().into_bytes(),
            &is_error,
        );
    }
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "tz_check", "arguments": {"path": "\u{0}\"\\ud800"}}}).to_string().into_bytes(),
        &is_error,
    );
    expect(
        &client,
        br#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"tz_check","arguments":{"path":"\ud800"}}}"#.to_vec(),
        &|answer: &Value| assert!(answer.get("error").is_some() || answer["result"]["isError"] == true, "{answer}"),
    );
    expect(
        &client,
        json!({"jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": {"uri": "tz://kg/brief/%"}}).to_string().into_bytes(),
        &code(-32602),
    );
    client.send(json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": {"x": [1]}}}));
    client.assert_quiet(1000);
    assert!(!root.join("target").exists());
}
