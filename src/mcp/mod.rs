//! `tsuzuri mcp` (G29, D-60): a Model Context Protocol server over stdio. It answers typed,
//! read-only tools (the diagnostics of `check`, the guide, `explain`, the knowledge graph's
//! queries, type search, `impact`, the fix plan, the diagrams) and a few resources. It never
//! writes a file, starts a process, opens the network or reads the environment itself; it writes
//! nothing but the protocol's lines to its output.

pub mod backend;
mod cache;
mod check;
mod knowledge;
mod path;
pub mod resources;
mod rpc;
pub mod schema;
mod session;
pub mod tools;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub use backend::{
    Backend, Cancel, Capabilities, ProjectBackend, ToolError, ToolOutput, ToolRequest,
};
pub use session::INSTRUCTIONS;

/// The protocol versions, newest first; the first answers a version it does not know.
pub const SUPPORTED_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PENDING: usize = 64;
pub const READER_QUEUE: usize = 8;
pub const MAX_TEXT_CHARS: usize = 200_000;
pub const MAX_STRUCTURED_BYTES: usize = 400_000;

/// A failure that ends the server (`E2010`).
#[derive(Debug)]
pub struct Fatal(pub String);

pub struct Options {
    /// The project root, canonical.
    pub root: PathBuf,
    /// Where the knowledge graph is read from; `<root>/target/tz/kg` by default.
    pub kg_dir: Option<PathBuf>,
}

/// The canonical project root that `path` names, or `None` when it is no directory.
pub fn project_root(path: &std::path::Path) -> Option<PathBuf> {
    crate::cache::real_path(path)
        .ok()
        .filter(|root| root.is_dir())
}

/// Serves the project of `options` on `input` and `output` until the input ends.
pub fn serve(
    options: Options,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> Result<(), Fatal> {
    serve_with(Arc::new(ProjectBackend::new(&options)), input, output)
}

/// Serves with any backend (the tests use their own).
pub fn serve_with(
    backend: Arc<dyn Backend>,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> Result<(), Fatal> {
    session::run(backend, input, output)
}
