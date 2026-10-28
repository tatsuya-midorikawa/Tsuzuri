//! The session (G29 "主ループ"): a reader thread turns the input into lines, the calling thread
//! answers what needs no tool and is the only writer, and one worker runs `tools/call` and
//! `resources/read` one at a time, first come first served. Cancelling a request drops its answer.

use std::collections::VecDeque;
use std::io::{BufReader, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc;

use serde_json::{Value, json};

use super::backend::{Backend, Cancel, ToolRequest};
use super::rpc::{self, Incoming, Line};
use super::tools::{self, Registry, Version};
use super::{Fatal, MAX_LINE_BYTES, MAX_PENDING, READER_QUEUE, resources};

/// The text that a client can give its model when the server is initialized (D18).
pub const INSTRUCTIONS: &str = "Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.";

enum Event {
    Line(Line),
    ReadFailed(String),
    Done { id: Value, outcome: Outcome },
}

enum Outcome {
    Result(Value),
    ProtocolError {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    Cancelled,
}

enum Work {
    Tool(Value),
    Resource(Value),
}

struct Job {
    id: Value,
    cancel: Cancel,
    version: Version,
    work: Work,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Fresh,
    Negotiated,
}

struct Session<W: Write> {
    backend: Arc<dyn Backend>,
    registry: Arc<Registry>,
    output: W,
    phase: Phase,
    version: Version,
    queue: VecDeque<Job>,
    running: Option<(Value, Cancel)>,
    eof: bool,
    jobs: mpsc::Sender<Job>,
}

fn spawn_failed(error: std::io::Error) -> Fatal {
    Fatal(format!("cannot start the worker thread: {error}"))
}

pub(crate) fn run(
    backend: Arc<dyn Backend>,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> Result<(), Fatal> {
    let (events, received) = mpsc::sync_channel::<Event>(READER_QUEUE);
    let reader_events = events.clone();
    std::thread::Builder::new()
        .name("tsuzuri-mcp-reader".into())
        .spawn(move || {
            let mut reader = BufReader::new(input);
            loop {
                let event = match rpc::read_line(&mut reader) {
                    Ok(Line::Eof) => {
                        let _ = reader_events.send(Event::Line(Line::Eof));
                        return;
                    }
                    Ok(line) => Event::Line(line),
                    Err(error) => {
                        let _ = reader_events.send(Event::ReadFailed(error.to_string()));
                        return;
                    }
                };
                if reader_events.send(event).is_err() {
                    return;
                }
            }
        })
        .map_err(spawn_failed)?;
    let registry = Arc::new(Registry::load());
    let (jobs, queued) = mpsc::channel::<Job>();
    let worker_backend = Arc::clone(&backend);
    let worker_registry = Arc::clone(&registry);
    let worker_events = events.clone();
    std::thread::Builder::new()
        .name("tsuzuri-mcp-worker".into())
        .stack_size(crate::parallel::WORKER_STACK_BYTES)
        .spawn(move || {
            for job in queued {
                let outcome = run_job(worker_backend.as_ref(), &worker_registry, &job);
                if worker_events
                    .send(Event::Done {
                        id: job.id,
                        outcome,
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .map_err(spawn_failed)?;
    drop(events);
    let mut session = Session {
        backend,
        registry,
        output,
        phase: Phase::Fresh,
        version: Version::V2025_06_18,
        queue: VecDeque::new(),
        running: None,
        eof: false,
        jobs,
    };
    loop {
        if session.eof && session.running.is_none() && session.queue.is_empty() {
            return Ok(());
        }
        let Ok(event) = received.recv() else {
            return Ok(());
        };
        session.event(event)?;
        session.start_next()?;
    }
}

impl<W: Write> Session<W> {
    fn write(&mut self, message: &Value) -> Result<(), Fatal> {
        rpc::write_line(&mut self.output, message)
            .map_err(|error| Fatal(format!("cannot write to the client: {error}")))
    }

    fn reply(&mut self, id: &Value, result: Value) -> Result<(), Fatal> {
        self.write(&rpc::result_message(id, result))
    }

    fn fail(&mut self, id: &Value, code: i64, message: &str) -> Result<(), Fatal> {
        self.write(&rpc::error_message(id, code, message, None))
    }

    fn event(&mut self, event: Event) -> Result<(), Fatal> {
        match event {
            Event::Line(Line::Bytes(bytes)) => self.line(&bytes),
            Event::Line(Line::TooLong) => self.fail(
                &Value::Null,
                rpc::INVALID_REQUEST,
                &format!("invalid request: message exceeds {MAX_LINE_BYTES} bytes"),
            ),
            Event::Line(Line::Eof) => {
                self.eof = true;
                Ok(())
            }
            Event::ReadFailed(message) => {
                Err(Fatal(format!("cannot read the client's input: {message}")))
            }
            Event::Done { id, outcome } => {
                let cancelled = self
                    .running
                    .take()
                    .is_some_and(|(_, cancel)| cancel.is_cancelled());
                match outcome {
                    _ if cancelled => Ok(()),
                    Outcome::Cancelled => Ok(()),
                    Outcome::Result(result) => self.reply(&id, result),
                    Outcome::ProtocolError {
                        code,
                        message,
                        data,
                    } => self.write(&rpc::error_message(&id, code, &message, data)),
                }
            }
        }
    }

    fn start_next(&mut self) -> Result<(), Fatal> {
        if self.running.is_some() {
            return Ok(());
        }
        let Some(job) = self.queue.pop_front() else {
            return Ok(());
        };
        self.running = Some((job.id.clone(), job.cancel.clone()));
        self.jobs
            .send(job)
            .map_err(|_| Fatal("cannot start the worker thread: the worker stopped".into()))
    }

    fn line(&mut self, bytes: &[u8]) -> Result<(), Fatal> {
        match rpc::parse(bytes) {
            Incoming::Invalid { id, code, message } => self.fail(&id, code, &message),
            Incoming::Ignore => Ok(()),
            Incoming::Notification { method, params } => {
                if method == "notifications/cancelled" && self.phase == Phase::Negotiated {
                    self.cancel(params.get("requestId"));
                }
                Ok(())
            }
            Incoming::Request { id, method, params } => self.request(id, &method, params),
        }
    }

    fn cancel(&mut self, request: Option<&Value>) {
        let Some(request) = request else {
            return;
        };
        match &self.running {
            Some((id, cancel)) if id == request => cancel.cancel(),
            _ => self.queue.retain(|job| job.id != *request),
        }
    }

    fn request(&mut self, id: Value, method: &str, params: Value) -> Result<(), Fatal> {
        // The ID of a request that runs or waits belongs to it, whatever the method of another
        // request (a `ping` answered at once would give the ID two responses).
        if self.in_use(&id) {
            return self.fail(
                &id,
                rpc::INVALID_REQUEST,
                "invalid request: request id is already in use",
            );
        }
        let known = matches!(
            method,
            "ping"
                | "initialize"
                | "tools/list"
                | "tools/call"
                | "resources/list"
                | "resources/templates/list"
                | "resources/read"
        );
        if !known {
            return self.fail(
                &id,
                rpc::METHOD_NOT_FOUND,
                &format!("method not found: {method}"),
            );
        }
        match method {
            "ping" => return self.reply(&id, json!({})),
            "initialize" => return self.initialize(&id, &params),
            _ => {}
        }
        if self.phase == Phase::Fresh {
            return self.fail(
                &id,
                rpc::INVALID_REQUEST,
                "server not initialized: send initialize first",
            );
        }
        let caps = self.backend.capabilities();
        match method {
            "tools/list" | "resources/list" | "resources/templates/list"
                if params.get("cursor").is_some() =>
            {
                self.fail(&id, rpc::INVALID_PARAMS, "invalid params: invalid cursor")
            }
            "tools/list" => {
                let tools = self.registry.list(self.version, caps);
                self.reply(&id, json!({"tools": tools}))
            }
            "resources/list" => {
                let list = resources::list(self.version, caps);
                self.reply(&id, json!({"resources": list}))
            }
            "resources/templates/list" => {
                let list = resources::templates(self.version, caps);
                self.reply(&id, json!({"resourceTemplates": list}))
            }
            "tools/call" => self.enqueue(id, Work::Tool(params)),
            _ => self.enqueue(id, Work::Resource(params)),
        }
    }

    fn initialize(&mut self, id: &Value, params: &Value) -> Result<(), Fatal> {
        if self.phase == Phase::Negotiated {
            return self.fail(id, rpc::INVALID_REQUEST, "server is already initialized");
        }
        let Some(requested) = params.get("protocolVersion").and_then(Value::as_str) else {
            return self.fail(
                id,
                rpc::INVALID_PARAMS,
                "invalid params: protocolVersion must be a string",
            );
        };
        self.version = Version::parse(requested).unwrap_or(Version::V2025_06_18);
        self.phase = Phase::Negotiated;
        let mut server = json!({"name": "tsuzuri", "version": env!("CARGO_PKG_VERSION")});
        if self.version.titles() {
            server["title"] = json!("Tsuzuri compiler");
        }
        let result = json!({
            "protocolVersion": self.version.as_str(),
            "capabilities": {"tools": {"listChanged": false}, "resources": {}},
            "serverInfo": server,
            "instructions": INSTRUCTIONS,
        });
        self.reply(id, result)
    }

    fn in_use(&self, id: &Value) -> bool {
        self.running
            .as_ref()
            .is_some_and(|(running, _)| running == id)
            || self.queue.iter().any(|job| job.id == *id)
    }

    fn enqueue(&mut self, id: Value, work: Work) -> Result<(), Fatal> {
        if self.queue.len() + usize::from(self.running.is_some()) >= MAX_PENDING {
            return self.fail(
                &id,
                rpc::TOO_MANY_PENDING,
                &format!("too many pending requests (limit {MAX_PENDING})"),
            );
        }
        self.queue.push_back(Job {
            id,
            cancel: Cancel::new(),
            version: self.version,
            work,
        });
        Ok(())
    }
}

fn protocol_error(code: i64, message: impl Into<String>) -> Outcome {
    Outcome::ProtocolError {
        code,
        message: message.into(),
        data: None,
    }
}

/// One `tools/call` or `resources/read` (G29 "tools/call" rows 1 to 6, "リソース").
fn run_job(backend: &dyn Backend, registry: &Registry, job: &Job) -> Outcome {
    match &job.work {
        Work::Tool(params) => run_tool(backend, registry, job, params),
        Work::Resource(params) => run_resource(backend, registry, job, params),
    }
}

fn run_tool(backend: &dyn Backend, registry: &Registry, job: &Job, params: &Value) -> Outcome {
    if !params.is_object() {
        return protocol_error(rpc::INVALID_PARAMS, "invalid params: expected an object");
    }
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return protocol_error(rpc::INVALID_PARAMS, "invalid params: name must be a string");
    };
    let empty = json!({});
    let arguments = match params.get("arguments") {
        None => &empty,
        Some(arguments) if arguments.is_object() => arguments,
        Some(_) => {
            return protocol_error(
                rpc::INVALID_PARAMS,
                "invalid params: arguments must be an object",
            );
        }
    };
    let Some(tool) = registry.get(name, backend.capabilities()) else {
        return protocol_error(rpc::INVALID_PARAMS, format!("unknown tool: {name}"));
    };
    if let Err(reason) = super::schema::validate(&tool["inputSchema"], arguments) {
        return Outcome::Result(tools::error_result(&format!("invalid arguments: {reason}")));
    }
    if job.cancel.is_cancelled() {
        return Outcome::Cancelled;
    }
    let request = ToolRequest {
        name,
        arguments,
        cancel: &job.cancel,
    };
    let result = catch_unwind(AssertUnwindSafe(|| backend.call(&request)));
    if job.cancel.is_cancelled() {
        return Outcome::Cancelled;
    }
    Outcome::Result(match result {
        Err(_) => tools::error_result(&format!(
            "internal error: tool '{name}' panicked; please report this"
        )),
        Ok(Err(error)) => tools::error_result(&error.0),
        Ok(Ok(output)) => tools::shape(job.version, output),
    })
}

fn run_resource(backend: &dyn Backend, registry: &Registry, job: &Job, params: &Value) -> Outcome {
    let Some(uri) = params.get("uri").and_then(Value::as_str) else {
        return protocol_error(rpc::INVALID_PARAMS, "invalid params: uri must be a string");
    };
    if let Err(reason) = resources::parse(uri) {
        return protocol_error(rpc::INVALID_PARAMS, format!("invalid params: {reason}"));
    }
    // The arguments that the URI gives its tool meet that tool's schema (an ID of at most 512
    // characters), as those of `tools/call` do.
    let caps = backend.capabilities();
    if let Some((name, arguments)) = resources::tool_request(uri, caps)
        && let Some(tool) = registry.get(name, caps)
        && let Err(reason) = super::schema::validate(&tool["inputSchema"], &arguments)
    {
        return protocol_error(rpc::INVALID_PARAMS, format!("invalid params: {reason}"));
    }
    let not_found = || Outcome::ProtocolError {
        code: rpc::RESOURCE_NOT_FOUND,
        message: format!("resource not found: {uri}"),
        data: Some(json!({"uri": uri})),
    };
    let result = catch_unwind(AssertUnwindSafe(|| backend.read_resource(uri, &job.cancel)));
    if job.cancel.is_cancelled() {
        return Outcome::Cancelled;
    }
    match result {
        Err(_) => protocol_error(
            rpc::INTERNAL_ERROR,
            format!("internal error: resource '{uri}' panicked; please report this"),
        ),
        Ok(Err(_)) | Ok(Ok(None)) => not_found(),
        Ok(Ok(Some(text))) => {
            let (text, _) = tools::cap_text(text);
            Outcome::Result(json!({
                "contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]
            }))
        }
    }
}
