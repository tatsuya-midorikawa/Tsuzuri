//! G29 Phase 2 (D-60): the tools that read the compiler's references and the project's knowledge
//! graph. Each is compared with the command line that answers the same question; none writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use tsuzuri::mcp::{Backend, Cancel, Options, ProjectBackend, ToolOutput, ToolRequest};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh root with the files of `fixture` (a directory of the repository) copied into it.
fn root_with(fixture: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-mcp-kg-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    // Named after the fixture, so that a project without a manifest keeps its namespace.
    let root = root.join(Path::new(fixture).file_name().unwrap());
    std::fs::create_dir_all(&root).unwrap();
    copy(&repository().join(fixture), &root);
    // The form that `tsuzuri mcp` gives the backend (not `\\?\` on Windows).
    tsuzuri::mcp::project_root(&root).unwrap()
}

/// Writes the project's graph to `target/tz/kg` with this process's compiler (the stamp that the
/// backend in this process expects).
fn build_in_process(root: &Path) {
    let project = tsuzuri::driver::Project::load_for_tests(root).unwrap();
    let dir = root.join("target/tz/kg");
    tsuzuri::kg::open::open_project(
        &project,
        &dir,
        tsuzuri::kg::open::Refresh::Auto,
        &mut |_, _| {},
    )
    .unwrap();
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

fn backend(root: &Path) -> ProjectBackend {
    ProjectBackend::new(&Options {
        root: root.to_path_buf(),
        kg_dir: None,
    })
}

fn call(backend: &ProjectBackend, name: &str, arguments: Value) -> Result<ToolOutput, String> {
    let cancel = Cancel::new();
    backend
        .call(&ToolRequest {
            name,
            arguments: &arguments,
            cancel: &cancel,
        })
        .map_err(|error| error.0)
}

fn tsuzuri(cwd: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(cwd)
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", cwd.join("cache"))
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn json_rows(output: &Output) -> Vec<Value> {
    stdout(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Every file under `root` with its size and modification time.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (u64, std::time::SystemTime)> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            if metadata.is_dir() {
                pending.push(path.clone());
            }
            files.insert(path, (metadata.len(), metadata.modified().unwrap()));
        }
    }
    files
}

#[test]
fn tz_guide_returns_the_card_and_lists_topics() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    let empty = std::env::temp_dir();
    let card = call(&backend, "tz_guide", json!({})).unwrap();
    assert_eq!(card.text, stdout(&tsuzuri(&empty, &["guide"])));
    assert_eq!(card.structured["topic"], "card");
    let list = call(&backend, "tz_guide", json!({"topic": "list"})).unwrap();
    assert_eq!(list.text, stdout(&tsuzuri(&empty, &["guide", "--list"])));
    assert_eq!(list.structured["topic"], "list");
    let error = call(&backend, "tz_guide", json!({"topic": "nothing"})).unwrap_err();
    assert_eq!(
        error,
        "unknown guide topic 'nothing'; call tz_guide with topic \"list\" to see the topics"
    );
}

#[test]
fn tz_explain_returns_a_page_and_rejects_unknown_queries() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    let cwd = std::env::temp_dir();
    for query in ["E1012", "use_after_move"] {
        let output = call(&backend, "tz_explain", json!({"query": query})).unwrap();
        assert_eq!(output.text, stdout(&tsuzuri(&cwd, &["explain", query])));
        let row: Value =
            serde_json::from_str(stdout(&tsuzuri(&cwd, &["explain", query, "--json"])).trim())
                .unwrap();
        assert_eq!(output.structured["code"], row["code"], "{query}");
        assert_eq!(output.structured["title"], row["title"], "{query}");
        if query == "use_after_move" {
            assert_eq!(output.structured["kind"], "use_after_move");
        } else {
            assert!(output.structured.get("kind").is_none());
        }
    }
    let error = call(&backend, "tz_explain", json!({"query": "E9999"})).unwrap_err();
    assert!(
        error.starts_with(
            "unknown diagnostic 'E9999'; use a code such as E1012 or a kind such as use_after_move"
        ),
        "{error}"
    );
}

#[test]
fn kg_acquisition_reads_a_fresh_disk_kg_and_rebuilds_in_memory_when_stale() {
    let root = root_with("tests/fixtures/kg_shop");
    build_in_process(&root);
    let backend = backend(&root);
    call(&backend, "tz_symbol", json!({"id": "total"})).unwrap();
    // A fresh graph on disk is read, not built.
    assert_eq!(backend.analysis_count(), 0);
    let kg = snapshot(&root.join("target/tz/kg"));
    let cart = root.join("Cart.tz");
    let text = std::fs::read_to_string(&cart).unwrap();
    std::fs::write(&cart, format!("{text}\n// edited\n")).unwrap();
    call(&backend, "tz_symbol", json!({"id": "total"})).unwrap();
    assert_eq!(backend.analysis_count(), 1);
    // The stale graph on disk is left as it was.
    assert_eq!(snapshot(&root.join("target/tz/kg")), kg);
}

#[test]
fn kg_acquisition_writes_nothing() {
    let root = root_with("tests/fixtures/kg_shop");
    // `old` of `tz_impact`: a graph directory, which the project's sources leave out.
    build_in_process(&root);
    let before = snapshot(&root);
    let backend = backend(&root);
    for (name, arguments) in [
        ("tz_check", json!({})),
        ("tz_symbol", json!({"id": "total"})),
        ("tz_brief", json!({})),
        ("tz_search", json!({"query": "total"})),
        ("tz_callers", json!({"id": "line_total"})),
        ("tz_path", json!({"from": "main", "to": "line_total"})),
        ("tz_find", json!({"query": "ref [Line] -> i64"})),
        ("tz_impact", json!({"old": "target/tz/kg"})),
        ("tz_fix_plan", json!({})),
        ("tz_diagram", json!({"kind": "module"})),
    ] {
        call(&backend, name, arguments).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert_eq!(snapshot(&root), before);
}

#[test]
fn kg_tools_report_an_incomplete_kg_for_a_failing_project() {
    let root = root_with("tests/fixtures/kg_shop");
    std::fs::write(root.join("Bad.tz"), "def bad :: i64 = \"text\"\n").unwrap();
    let backend = backend(&root);
    for (name, arguments) in [
        ("tz_symbol", json!({"id": "total"})),
        ("tz_brief", json!({})),
        ("tz_search", json!({"query": "x"})),
        ("tz_callers", json!({"id": "total"})),
        ("tz_diagram", json!({"kind": "module"})),
    ] {
        assert_eq!(
            call(&backend, name, arguments).unwrap_err(),
            "the project does not type-check, so there are no facts yet; call tz_check and fix the errors first",
            "{name}"
        );
    }
    assert_eq!(
        call(&backend, "tz_find", json!({"query": "i64"})).unwrap_err(),
        "the project does not type-check; call tz_check first"
    );
}

#[test]
fn tz_symbol_matches_the_kg_row() {
    let root = root_with("examples/point");
    assert!(tsuzuri(&root, &["kg", "build"]).status.success());
    let row = std::fs::read_to_string(root.join("target/tz/kg/nodes.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["id"] == "Point::Point.hypotenuse")
        .unwrap();
    let output = call(
        &backend(&root),
        "tz_symbol",
        json!({"id": "Point.hypotenuse"}),
    )
    .unwrap();
    assert_eq!(output.structured["symbol"], row);
    assert_eq!(
        output.structured["callees"],
        json!(["Point::Point.distance"])
    );
    let cli = tsuzuri(&root, &["kg", "show", "Point.hypotenuse"]);
    assert_eq!(output.text, stdout(&cli));
}

#[test]
fn kg_tools_agree_with_the_cli() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    let brief = call(&backend, "tz_brief", json!({})).unwrap();
    assert_eq!(brief.text, stdout(&tsuzuri(&root, &["kg", "brief"])));
    assert_eq!(brief.structured["level"], "L0");
    let card = call(&backend, "tz_brief", json!({"id": "total", "budget": 300})).unwrap();
    assert_eq!(
        card.text,
        stdout(&tsuzuri(
            &root,
            &["kg", "brief", "total", "--budget", "300"]
        ))
    );
    let search = call(&backend, "tz_search", json!({"query": "total"})).unwrap();
    assert_eq!(
        search.text,
        stdout(&tsuzuri(&root, &["kg", "search", "total"]))
    );
    let rows = json_rows(&tsuzuri(&root, &["kg", "search", "total", "--json"]));
    let expected: Vec<Value> = rows
        .iter()
        .filter(|row| row["type"] == "match")
        .map(|row| json!({"id": row["id"], "kind": row["kind"], "signature": row["signature"]}))
        .collect();
    assert_eq!(search.structured["matches"], Value::Array(expected));
    let defs = call(
        &backend,
        "tz_search",
        json!({"query": "total", "kind": "test"}),
    )
    .unwrap();
    assert_eq!(defs.structured["matches"].as_array().unwrap().len(), 1);
    let callers = call(
        &backend,
        "tz_callers",
        json!({"id": "line_total", "depth": 2}),
    )
    .unwrap();
    assert_eq!(
        callers.text,
        stdout(&tsuzuri(
            &root,
            &["kg", "callers", "line_total", "--depth", "2"]
        ))
    );
    let rows = json_rows(&tsuzuri(
        &root,
        &["kg", "callers", "line_total", "--depth", "2", "--json"],
    ));
    let edges: Vec<&Value> = rows.iter().filter(|row| row["type"] == "edge").collect();
    let callers_rows = callers.structured["callers"].as_array().unwrap();
    assert_eq!(callers_rows.len(), edges.len());
    for (row, edge) in callers_rows.iter().zip(&edges) {
        assert_eq!(row["from"], edge["id"]);
        assert_eq!(row["depth"], edge["depth"]);
        assert_eq!(row["span"], edge["span"]);
    }
    let path = call(
        &backend,
        "tz_path",
        json!({"from": "main", "to": "line_total"}),
    )
    .unwrap();
    assert_eq!(
        path.text,
        stdout(&tsuzuri(&root, &["kg", "path", "main", "line_total"]))
    );
    assert_eq!(path.structured["found"], true);
    assert_eq!(path.structured["steps"].as_array().unwrap().len(), 3);
    let uses = call(
        &backend,
        "tz_callers",
        json!({"id": "Line", "relation": "uses_type"}),
    )
    .unwrap();
    assert_eq!(uses.text, stdout(&tsuzuri(&root, &["kg", "uses", "Line"])));
    let diagram = call(
        &backend,
        "tz_diagram",
        json!({"kind": "types", "format": "dot"}),
    )
    .unwrap();
    assert_eq!(
        diagram.text,
        stdout(&tsuzuri(
            &root,
            &["kg", "diagram", "types", "--format", "dot"]
        ))
    );
    assert_eq!(diagram.structured["format"], "dot");
}

#[test]
fn tz_callers_follows_depth_and_stops_on_cycles() {
    let root = root_with("tests/fixtures/kg_shop");
    std::fs::write(
        root.join("Loop.tz"),
        "def rec count :: i64 -> i64 = \\n -> if n > 0 then 1 + count (n - 1) else 0\n\ndef twice :: i64 -> i64 = \\n -> count n + count n\n",
    )
    .unwrap();
    let backend = backend(&root);
    let output = call(&backend, "tz_callers", json!({"id": "count", "depth": 8})).unwrap();
    let rows = output.structured["callers"].as_array().unwrap();
    let froms: Vec<&str> = rows
        .iter()
        .map(|row| row["from"].as_str().unwrap())
        .collect();
    // `count` calls itself, a cycle back to the start, which `kg callers` does not list again;
    // `twice` calls it twice and is listed once.
    assert_eq!(froms, ["Acme::Shop::Loop.twice"]);
    assert_eq!(
        output.text,
        stdout(&tsuzuri(&root, &["kg", "callers", "count", "--depth", "8"]))
    );
    let limited = call(
        &backend,
        "tz_callers",
        json!({"id": "line_total", "depth": 2, "limit": 1}),
    )
    .unwrap();
    assert_eq!(limited.structured["callers"].as_array().unwrap().len(), 1);
    assert_eq!(limited.structured["truncated"], true);
    // The text is the same one caller, with the count of the rest.
    let first = &limited.structured["callers"][0];
    let lines: Vec<&str> = limited.text.lines().collect();
    assert_eq!(lines.len(), 4, "{}", limited.text);
    assert!(lines[0].starts_with("callers of Acme::Shop::Cart.line_total"));
    assert!(lines[1].starts_with(&format!("1 {}  ", first["from"].as_str().unwrap())));
    assert_eq!(lines[2], "\u{2026}4 more; raise limit");
    assert_eq!(lines[3], "summary: 5 callers, depth 2");
}

#[test]
fn kg_tools_reject_unknown_and_ambiguous_ids() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    assert_eq!(
        call(&backend, "tz_symbol", json!({"id": "totl"})).unwrap_err(),
        "unknown id 'totl'; use tz_search to find ids; did you mean: Acme::Shop::Cart.total"
    );
    assert_eq!(
        call(&backend, "tz_symbol", json!({"id": "zzz"})).unwrap_err(),
        "unknown id 'zzz'; use tz_search to find ids"
    );
    let error = call(&backend, "tz_callers", json!({"id": "t"})).unwrap_err();
    assert!(
        error.starts_with("ambiguous id 't'; candidates: "),
        "{error}"
    );
}

#[test]
fn tz_find_matches_the_cli() {
    let root = root_with("tests/fixtures/kg_shop");
    let output = call(
        &backend(&root),
        "tz_find",
        json!({"query": "ref [Line] -> i64", "limit": 3}),
    )
    .unwrap();
    let cli = stdout(&tsuzuri(
        &root,
        &["kg", "find", "ref [Line] -> i64", "--limit", "3"],
    ));
    assert_eq!(
        output.text,
        cli.replace("raise --limit (at most 100)", "raise limit (at most 10)")
    );
    let candidates = output.structured["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 3);
    assert_eq!(
        candidates[0],
        json!({"name": "Acme::Shop::Cart.total", "signature": "Acme::Shop::Cart.total :: ref [Line] -> i64", "how": "call"})
    );
    assert_eq!(output.structured["truncated"], true);
    let error = call(&backend(&root), "tz_find", json!({"query": "ref [Line"})).unwrap_err();
    assert_eq!(
        error,
        "cannot parse the type query: cannot parse the type 'ref [Line'"
    );
}

#[test]
fn tz_impact_reports_a_signature_change() {
    let root = root_with("tests/fixtures/kg_shop");
    copy(
        &repository().join("tests/fixtures/kg_shop"),
        &root.join("base"),
    );
    copy(
        &repository().join("tests/fixtures/kg_shop_head"),
        &root.join("head"),
    );
    let output = call(
        &backend(&root),
        "tz_impact",
        json!({"old": "base", "new": "head"}),
    )
    .unwrap();
    let cli = tsuzuri(&root, &["kg", "impact", "--from", "base", "--to", "head"]);
    assert_eq!(output.text, stdout(&cli));
    let rows = json_rows(&tsuzuri(
        &root,
        &["kg", "impact", "--from", "base", "--to", "head", "--json"],
    ));
    assert_eq!(output.structured["records"], Value::Array(rows));
    assert!(
        output
            .text
            .contains("breaking: changed def Acme::Shop::Report.print_total")
    );
}

/// A graph on disk whose manifest lists a path out of its directory, or whose files do not match
/// it, is not read: the project's graph is built in memory, and an imported graph is refused.
#[test]
fn unsafe_or_changed_graphs_on_disk_are_not_read() {
    let root = root_with("tests/fixtures/kg_shop");
    build_in_process(&root);
    let kg = root.join("target/tz/kg");
    let manifest_path = kg.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        root.join("secret.jsonl"),
        "{\"id\":\"Acme::Shop::Cart.total\",\"effects\":[\"leaked\"]}\n",
    )
    .unwrap();
    let facts = manifest
        .lines()
        .find(|line| line.contains("\"facts/Cart.jsonl\""))
        .unwrap()
        .to_owned();
    let secret = tsuzuri::kg::input::digest(&std::fs::read(root.join("secret.jsonl")).unwrap());
    let crafted = manifest.replace(
        &facts,
        &format!("    \"facts/../../../secret.jsonl\": \"{secret}\","),
    );
    std::fs::write(&manifest_path, &crafted).unwrap();
    let backend = backend(&root);
    let symbol = call(&backend, "tz_symbol", json!({"id": "total"})).unwrap();
    assert!(!symbol.text.contains("leaked"), "{}", symbol.text);
    assert_eq!(backend.analysis_count(), 1);
    assert!(
        call(&backend, "tz_impact", json!({"old": "target/tz/kg"}))
            .unwrap_err()
            .contains("is broken"),
    );
    // A changed file of a listed path is refused too.
    std::fs::write(&manifest_path, &manifest).unwrap();
    std::fs::write(kg.join("facts/Cart.jsonl"), "{}\n").unwrap();
    let backend = self::backend(&root);
    call(&backend, "tz_symbol", json!({"id": "total"})).unwrap();
    assert_eq!(backend.analysis_count(), 1);
    let error = call(&backend, "tz_impact", json!({"old": "target/tz/kg"})).unwrap_err();
    assert!(
        error.contains("facts/Cart.jsonl does not match manifest.json"),
        "{error}"
    );
}

#[test]
fn tz_impact_rejects_paths_outside_the_root() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    assert_eq!(
        call(&backend, "tz_impact", json!({"old": "../x"})).unwrap_err(),
        "path must not contain '..': '../x'"
    );
    assert_eq!(
        call(&backend, "tz_impact", json!({"old": "/tmp"})).unwrap_err(),
        "path must be relative to the project root: '/tmp'"
    );
    std::fs::create_dir_all(root.join("empty")).unwrap();
    assert_eq!(
        call(&backend, "tz_impact", json!({"old": "empty"})).unwrap_err(),
        "'empty' is neither a knowledge graph nor a project directory"
    );
}

#[test]
fn tz_fix_plan_matches_fix_dry_run_and_changes_no_file() {
    let root = root_with("tests/fixtures/kg_shop");
    let before = snapshot(&root);
    let output = call(&backend(&root), "tz_fix_plan", json!({})).unwrap();
    assert_eq!(
        output.text,
        stdout(&tsuzuri(&root, &["fix", "--dry-run", "."]))
    );
    assert_eq!(snapshot(&root), before);
    let fixes = output.structured["fixes"].as_array().unwrap();
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0]["code"], "W1001");
    assert!(
        output.structured["diff"]
            .as_str()
            .unwrap()
            .contains("+++ b/Cart.tz")
    );
    assert_eq!(output.structured["remaining_warnings"], 0);
}

/// The plan of a project below the root names its files from the root, as `tz_check` does.
#[test]
fn tz_fix_plan_paths_are_relative_to_the_root() {
    let root = root_with("tests/fixtures/kg_shop");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(
        root.join("sub/Main.tz"),
        "def main :: unit -> i32 = \\() ->\n    let unused = 1\n    0\n",
    )
    .unwrap();
    let output = call(
        &backend(&root),
        "tz_fix_plan",
        json!({"path": "sub/Main.tz"}),
    )
    .unwrap();
    let fixes = output.structured["fixes"].as_array().unwrap();
    assert_eq!(fixes.len(), 1, "{}", output.text);
    assert_eq!(fixes[0]["edits"][0]["path"], "sub/Main.tz");
    let diff = output.structured["diff"].as_str().unwrap();
    assert!(
        diff.starts_with("--- a/sub/Main.tz\n+++ b/sub/Main.tz\n"),
        "{diff}"
    );
    assert!(
        output.text.starts_with("--- a/sub/Main.tz\n"),
        "{}",
        output.text
    );
}

/// `tz_check` of a project below the root is cached by that project's inputs.
#[test]
fn result_cache_follows_the_project_at_path() {
    let root = root_with("tests/fixtures/kg_shop");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(
        root.join("sub/Tsuzuri.toml"),
        "[package]\nname = \"sub\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("sub/Main.tz"),
        "def main :: unit -> i32 = \\() -> 0\n",
    )
    .unwrap();
    let backend = backend(&root);
    let first = call(&backend, "tz_check", json!({"path": "sub"})).unwrap();
    assert_eq!(first.structured["ok"], true, "{}", first.text);
    std::fs::write(root.join("sub/Tsuzuri.toml"), "[package]\nname = 1\n").unwrap();
    let second = call(&backend, "tz_check", json!({"path": "sub"})).unwrap();
    assert_eq!(second.structured["ok"], false, "{}", second.text);
}

#[test]
fn tz_diagram_matches_kg_diagram() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    for arguments in [
        json!({"kind": "module"}),
        json!({"kind": "calls", "id": "main", "depth": 3}),
        json!({"kind": "ownership", "id": "total"}),
        json!({"kind": "layout", "id": "Line"}),
    ] {
        let output = call(&backend, "tz_diagram", arguments.clone()).unwrap();
        let mut cli = vec!["kg", "diagram", arguments["kind"].as_str().unwrap()];
        if let Some(id) = arguments["id"].as_str() {
            cli.push(id);
        }
        let depth = arguments["depth"].to_string();
        if arguments.get("depth").is_some() {
            cli.extend_from_slice(&["--depth", &depth]);
        }
        assert_eq!(output.text, stdout(&tsuzuri(&root, &cli)), "{arguments}");
    }
    assert_eq!(
        call(&backend, "tz_diagram", json!({"kind": "calls"})).unwrap_err(),
        "diagram kind 'calls' needs an id"
    );
    assert_eq!(
        call(
            &backend,
            "tz_diagram",
            json!({"kind": "layout", "id": "x", "format": "dot"})
        )
        .unwrap_err(),
        "format 'dot' is not available for kind 'layout'"
    );
}

#[test]
fn result_cache_returns_identical_bytes_and_skips_the_analysis() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    let first = call(&backend, "tz_check", json!({})).unwrap();
    assert_eq!(backend.analysis_count(), 1);
    let second = call(&backend, "tz_check", json!({})).unwrap();
    assert_eq!(first, second);
    assert_eq!(backend.analysis_count(), 1);
}

#[test]
fn result_cache_invalidates_when_a_source_changes() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    call(&backend, "tz_check", json!({})).unwrap();
    let main = root.join("Main.tz");
    let text = std::fs::read_to_string(&main).unwrap();
    std::fs::write(&main, format!("{text}\n// edited\n")).unwrap();
    call(&backend, "tz_check", json!({})).unwrap();
    assert_eq!(backend.analysis_count(), 2);
}

#[test]
fn result_cache_keeps_eight_entries() {
    let root = root_with("tests/fixtures/kg_shop");
    let backend = backend(&root);
    // Nine calls that differ in their arguments only.
    let path = |index: usize| json!({"path": "./".repeat(index) + "Main.tz"});
    for index in 0..9 {
        call(&backend, "tz_check", path(index)).unwrap();
    }
    assert_eq!(backend.analysis_count(), 9);
    // The last eight are kept; the first is gone.
    call(&backend, "tz_check", path(8)).unwrap();
    call(&backend, "tz_check", path(1)).unwrap();
    assert_eq!(backend.analysis_count(), 9);
    call(&backend, "tz_check", path(0)).unwrap();
    assert_eq!(backend.analysis_count(), 10);
}
