//! What answers the tools (G29 appendix E): the `Backend` trait, the tools it offers
//! (`Capabilities`), and `ProjectBackend`, which answers them for one project root.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::Value;

/// Which tools a backend offers. A tool whose dependency is missing is left out of `tools/list`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub check: bool,
    pub guide: bool,
    pub explain: bool,
    /// The knowledge-graph tools: `tz_symbol`, `tz_brief`, `tz_search`, `tz_callers`, `tz_path`.
    pub kg: bool,
    pub impact: bool,
    pub find: bool,
    pub fix: bool,
    pub diagram: bool,
}

impl Capabilities {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            check: true,
            guide: true,
            explain: true,
            kg: true,
            impact: true,
            find: true,
            fix: true,
            diagram: true,
        }
    }

    /// Whether the tool `name` is offered. Unknown names never are.
    pub fn allows(&self, tool: &str) -> bool {
        match tool {
            "tz_check" => self.check,
            "tz_guide" => self.guide,
            "tz_explain" => self.explain,
            "tz_symbol" | "tz_brief" | "tz_search" | "tz_callers" | "tz_path" => self.kg,
            "tz_impact" => self.impact,
            "tz_find" => self.find,
            "tz_fix_plan" => self.fix,
            "tz_diagram" => self.diagram,
            _ => false,
        }
    }
}

/// The cancellation of one request: set by `notifications/cancelled`, read around the call.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A call with arguments that passed the tool's input schema; the defaults are not filled in.
pub struct ToolRequest<'a> {
    pub name: &'a str,
    pub arguments: &'a Value,
    pub cancel: &'a Cancel,
}

/// The result of a tool: the text for the model and the structured content (an object; its
/// `truncated` may be left out).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    pub structured: Value,
}

/// The text of a result with `isError: true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError(pub String);

pub trait Backend: Send + Sync {
    fn capabilities(&self) -> Capabilities;

    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError>;

    /// The text of a resource; `Ok(None)` when there is no such resource (`-32002`).
    fn read_resource(&self, _uri: &str, _cancel: &Cancel) -> Result<Option<String>, ToolError> {
        Ok(None)
    }
}

/// The backend of `tsuzuri mcp`: the compiler's own functions on the project at `root`. It
/// reads files and never writes them.
pub struct ProjectBackend {
    pub(crate) root: PathBuf,
    pub(crate) kg_dir: PathBuf,
    pub(crate) results: std::sync::Mutex<super::cache::ResultCache>,
    pub(crate) knowledge: std::sync::Mutex<Option<super::knowledge::Cached>>,
    /// How many analyses the backend ran (for the tests of the result cache).
    pub(crate) analyses: AtomicUsize,
}

impl ProjectBackend {
    pub fn new(options: &super::Options) -> Self {
        Self {
            root: options.root.clone(),
            kg_dir: options
                .kg_dir
                .clone()
                .unwrap_or_else(|| options.root.join("target").join("tz").join("kg")),
            results: std::sync::Mutex::new(super::cache::ResultCache::default()),
            knowledge: std::sync::Mutex::new(None),
            analyses: AtomicUsize::new(0),
        }
    }

    /// The number of analyses run so far.
    pub fn analysis_count(&self) -> usize {
        self.analyses.load(Ordering::Relaxed)
    }

    pub(crate) fn count_analysis(&self) {
        self.analyses.fetch_add(1, Ordering::Relaxed);
    }
}

impl Backend for ProjectBackend {
    fn capabilities(&self) -> Capabilities {
        Capabilities::all()
    }

    fn call(&self, request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError> {
        super::cache::cached(self, request, |request| match request.name {
            "tz_check" => super::check::run(self, request),
            "tz_guide" => super::knowledge::guide(self, request),
            "tz_explain" => super::knowledge::explain(request),
            "tz_symbol" => super::knowledge::symbol(self, request),
            "tz_brief" => super::knowledge::brief(self, request),
            "tz_search" => super::knowledge::search(self, request),
            "tz_callers" => super::knowledge::callers(self, request),
            "tz_path" => super::knowledge::path(self, request),
            "tz_find" => super::knowledge::find(self, request),
            "tz_impact" => super::knowledge::impact(self, request),
            "tz_fix_plan" => super::check::fix_plan(self, request),
            "tz_diagram" => super::knowledge::diagram(self, request),
            name => Err(ToolError(format!(
                "tool '{name}' is not available in this build"
            ))),
        })
    }

    fn read_resource(&self, uri: &str, cancel: &Cancel) -> Result<Option<String>, ToolError> {
        super::resources::read(self, uri, cancel)
    }
}
