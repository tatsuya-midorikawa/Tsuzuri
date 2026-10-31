//! A small reader of the textual LLVM IR that `tsuzuri build --emit llvm` writes, for the tests
//! that compare the effect summary (A20) with the code the compiler actually emits. No regex
//! crate: one function per `define`, its direct calls, and its trap sites.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug)]
pub struct IrFunction {
    /// The symbols that the body calls directly, `@` included.
    pub calls: BTreeSet<String>,
    /// The body calls through a pointer.
    pub indirect: bool,
    /// The body calls `@llvm.trap`.
    pub trap: bool,
    /// The ids of `call void @tz.trap.report(i32 N)` with a constant `N`.
    pub reports: BTreeSet<u64>,
    /// The `KIND` of every `add i32 %tz.context, KIND`: the trap kinds of an inherited context.
    pub context_kinds: BTreeSet<u64>,
}

/// The symbols that allocate on the heap.
pub const ALLOCATORS: &[&str] = &[
    "@tz.alloc",
    "@tz.realloc",
    "@tz.alloc.base",
    "@tz.realloc.base",
    "@tz.heap.alloc.unlocked",
    "@tz.heap.realloc.unlocked",
    "@malloc",
    "@realloc",
    "@calloc",
    "@tsuzuri_alloc",
];

/// The prefixes of the functions that stand for a function of the program.
pub const USER_PREFIXES: &[&str] = &["@tz.fn.", "@tz.specialized.", "@tz.apply."];

fn symbol(text: &str) -> Option<String> {
    let start = text.find('@')?;
    let rest = &text[start + 1..];
    if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some(format!("@\"{}\"", &quoted[..end]));
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-')))
        .unwrap_or(rest.len());
    Some(format!("@{}", &rest[..end]))
}

/// Every defined function of `ir`, by its symbol.
pub fn parse(ir: &str) -> BTreeMap<String, IrFunction> {
    let mut functions = BTreeMap::new();
    let mut current: Option<(String, IrFunction)> = None;
    for line in ir.lines() {
        if line.starts_with("define ") {
            let head = &line[..line.find('(').unwrap_or(line.len())];
            let name = head.rsplit(' ').next().and_then(symbol).unwrap_or_default();
            current = Some((name, IrFunction::default()));
            continue;
        }
        if line == "}" {
            if let Some((name, function)) = current.take() {
                functions.insert(name, function);
            }
            continue;
        }
        let Some((_, function)) = &mut current else {
            continue;
        };
        if let Some(at) = line.find("add i32 %tz.context, ") {
            let kind = &line[at + "add i32 %tz.context, ".len()..];
            if let Ok(kind) = kind.trim().parse() {
                function.context_kinds.insert(kind);
            }
        }
        let Some(at) = line.find("call ") else {
            continue;
        };
        let call = &line[at + 5..];
        let head = &call[..call.find('(').unwrap_or(call.len())];
        let Some(callee) = head.split_whitespace().last() else {
            continue;
        };
        if callee.starts_with('%') {
            function.indirect = true;
        } else if callee == "@llvm.trap" {
            function.trap = true;
        } else if callee.starts_with('@') {
            if callee == "@tz.trap.report" {
                let arguments = &call[head.len()..];
                if let Some(value) = arguments
                    .strip_prefix("(i32 ")
                    .and_then(|rest| rest.strip_suffix(')'))
                    && let Ok(id) = value.parse()
                {
                    function.reports.insert(id);
                }
            }
            let empty = (callee == "@tz.string.new" || callee == "@tz.utf8string.new")
                && (call.contains(", i64 0,") || call.contains(", i64 0)"));
            if !empty && let Some(callee) = symbol(callee) {
                function.calls.insert(callee);
            }
        }
    }
    functions
}

pub fn user_level(symbol: &str) -> bool {
    USER_PREFIXES
        .iter()
        .any(|prefix| symbol.starts_with(prefix))
}

/// Whether `symbol`'s own body allocates: it calls an allocator, or reaches one through helper
/// functions only (a function of the program has its own verdict).
pub fn own_alloc(functions: &BTreeMap<String, IrFunction>, symbol: &str) -> bool {
    let mut pending = vec![symbol.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(function) = functions.get(&name) else {
            continue;
        };
        for callee in &function.calls {
            if ALLOCATORS.contains(&callee.as_str()) {
                return true;
            }
            if !user_level(callee) {
                pending.push(callee.clone());
            }
        }
    }
    false
}

/// The helper chain from `symbol` to an allocator, for a failure message.
pub fn alloc_path(functions: &BTreeMap<String, IrFunction>, symbol: &str) -> Vec<String> {
    let mut pending = vec![vec![symbol.to_owned()]];
    let mut seen = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let name = path.last().unwrap().clone();
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(function) = functions.get(&name) else {
            continue;
        };
        for callee in &function.calls {
            let mut next = path.clone();
            next.push(callee.clone());
            if ALLOCATORS.contains(&callee.as_str()) {
                return next;
            }
            if !user_level(callee) {
                pending.push(next);
            }
        }
    }
    Vec::new()
}

/// The programs of the corpus: every directory of `tests/fixtures` and `examples` with a `Main.tz`.
pub fn corpus() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut projects = Vec::new();
    for parent in ["tests/fixtures", "examples"] {
        let Ok(entries) = std::fs::read_dir(root.join(parent)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("Main.tz").is_file() {
                projects.push(path);
            }
        }
    }
    projects.sort();
    projects
}

/// `tsuzuri build <dir> --emit llvm --trap-info`: the IR and the trap table's JSON. Every project
/// of the corpus emits; one that stops compiling fails the check instead of leaving it. A program
/// with failure domains (B10, `Domain.run`) is LLVM IR only with `--trap-mode return`, whose host
/// provides the trap runtime.
pub fn emit(directory: &std::path::Path, scratch: &std::path::Path) -> (String, String) {
    let name = directory
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let output = scratch.join(format!("{name}.ll"));
    let domain = std::fs::read_to_string(directory.join("Main.tz"))
        .is_ok_and(|text| text.contains("Domain.run"));
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(directory)
        .args(["--emit", "llvm", "--trap-info", "-o"])
        .arg(&output)
        .args(
            domain
                .then_some(["--trap-mode", "return"])
                .into_iter()
                .flatten(),
        )
        .output()
        .expect("run tsuzuri");
    assert!(
        status.status.success(),
        "{} does not emit LLVM IR:\n{}",
        directory.display(),
        String::from_utf8_lossy(&status.stderr)
    );
    let ir = std::fs::read_to_string(&output).expect("the emitted IR");
    let mut table = output.clone().into_os_string();
    table.push(".trap.json");
    let table = std::fs::read_to_string(table).unwrap_or_default();
    (ir, table)
}
