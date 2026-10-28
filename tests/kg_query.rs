//! G28 Phase 1 and 2 (D-60): the queries of `tsuzuri kg`. The expected text comes from the
//! reference implementation of G28 appendix D (with the D-60 wording) run on the graph's rows.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A copy of a fixture in a fresh root, with its graph built.
fn project(fixture: &str, label: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-kg-query-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for entry in std::fs::read_dir(repository().join("tests/fixtures").join(fixture)).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, root.join(path.file_name().unwrap())).unwrap();
        }
    }
    let output = tsuzuri(&root, &["kg", "build"]);
    assert!(output.status.success(), "{}", stderr(&output));
    root
}

fn tsuzuri(cwd: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(cwd)
        .args(arguments)
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR")
        .output()
        .unwrap()
}

/// Removes the files of the graph at `dir` whose paths start with `prefix`, with their entries in
/// `manifest.json`: a graph without that table, not a broken one.
fn without(dir: &Path, prefix: &str) {
    let path = dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let files = manifest["files"].as_object_mut().unwrap();
    let removed: Vec<String> = files
        .keys()
        .filter(|file| file.starts_with(prefix))
        .cloned()
        .collect();
    assert!(!removed.is_empty(), "{prefix}");
    for file in &removed {
        files.remove(file);
        std::fs::remove_file(dir.join(file)).unwrap();
    }
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(repository().join("tests/golden/kg/queries").join(name)).unwrap()
}

fn query(root: &Path, arguments: &[&str]) -> Output {
    let mut all = vec!["kg"];
    all.extend_from_slice(arguments);
    all.push("--no-refresh");
    tsuzuri(root, &all)
}

#[test]
fn show_matches_the_appendix_output() {
    let root = project("kg_shop", "show");
    for (arguments, file) in [
        (vec!["show", "Acme::Shop::Cart.total"], "show_total.txt"),
        (vec!["show", "Acme::Shop::Main.main"], "show_main.txt"),
        (vec!["show", "Acme::Shop::Cart.Line"], "show_line.txt"),
        (vec!["show", "IO.write_line"], "show_io.txt"),
        (
            vec!["show", "Acme::Shop::Cart.total", "--budget", "200"],
            "show_total_budget_200.txt",
        ),
    ] {
        let output = query(&root, &arguments);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), golden(file), "{arguments:?}");
    }
}

#[test]
fn list_commands_match_the_reference() {
    let root = project("kg_shop", "lists");
    for (arguments, file) in [
        (
            vec!["callers", "Acme::Shop::Cart.line_total", "--depth", "2"],
            "callers_line_total_2.txt",
        ),
        (
            vec!["callees", "Acme::Shop::Main.main", "--depth", "3"],
            "callees_main_3.txt",
        ),
        (vec!["uses", "Acme::Shop::Cart.Line"], "uses_line.txt"),
        (
            vec!["tests-for", "Acme::Shop::Cart.line_total"],
            "tests_for_line_total.txt",
        ),
        (vec!["search", "total"], "search_total.txt"),
    ] {
        let output = query(&root, &arguments);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), golden(file), "{arguments:?}");
    }
}

#[test]
fn resolution_stages_and_errors() {
    let root = project("kg_shop", "resolve");
    let first_line = |text: &str| {
        let output = query(&root, &["show", text]);
        assert!(output.status.success(), "{text}: {}", stderr(&output));
        stdout(&output).lines().next().unwrap().to_owned()
    };
    assert_eq!(
        first_line("Acme::Shop::Cart.total"),
        "Acme::Shop::Cart.total"
    );
    assert_eq!(first_line("Line"), "Acme::Shop::Cart.Line");
    assert_eq!(first_line("main"), "Acme::Shop::Main.main");
    assert_eq!(first_line("Cart"), "Acme::Shop::Cart");
    assert_eq!(first_line("line total"), "test:Acme::Shop::Cart.line total");
    assert_eq!(
        first_line("Acme::Shop::Cart.l"),
        "Acme::Shop::Cart.line_total"
    );
    assert_eq!(first_line("IO.write_lin"), "IO.write_line");
    for (text, code, message) in [
        (
            "t",
            1,
            "ambiguous id 't'; candidates: test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Cart.line total, test:Acme::Shop::Main.main runs",
        ),
        (
            "totl",
            1,
            "unknown id 'totl'; did you mean: Acme::Shop::Cart.total",
        ),
        (
            "zzz",
            1,
            "unknown id 'zzz'; use tsuzuri kg search TEXT to find ids",
        ),
        (
            "IO.writ_line",
            1,
            "unknown id 'IO.writ_line'; did you mean: IO.write_line",
        ),
        ("", 2, "id must not be empty"),
    ] {
        let output = query(&root, &["show", text]);
        assert_eq!(output.status.code(), Some(code), "{text}");
        assert_eq!(
            stderr(&output),
            format!("<command line>:1:1: error[E2000]: {message}\n"),
            "{text}"
        );
    }
    let output = query(&root, &["show", "t", "--json"]);
    let row: serde_json::Value = serde_json::from_str(stderr(&output).trim()).unwrap();
    assert_eq!(row["code"], "E2000");
    assert_eq!(row["kind"], "kg_ambiguous_id");
}

fn rows(output: &Output) -> Vec<serde_json::Value> {
    stdout(output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn json_rows_of_the_queries() {
    let root = project("kg_shop", "json");
    let callers = rows(&query(
        &root,
        &["callers", "Acme::Shop::Cart.line_total", "--json"],
    ));
    assert_eq!(callers.len(), 3);
    assert_eq!(callers[0]["type"], "edge");
    assert_eq!(callers[0]["id"], "Acme::Shop::Cart.total");
    assert_eq!(callers[0]["args"], serde_json::json!(["ref"]));
    assert_eq!(callers[0]["span"]["line"], 14);
    assert_eq!(
        callers[2],
        serde_json::json!({"type":"summary","count":2,"depth":1})
    );
    let show = rows(&query(&root, &["show", "Acme::Shop::Main.main", "--json"]));
    assert_eq!(show.len(), 1);
    let keys: Vec<&String> = show[0].as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 7);
    assert_eq!(show[0]["symbol"]["id"], "Acme::Shop::Main.main");
    assert_eq!(show[0]["facts"]["effects"], serde_json::json!(["io"]));
    assert_eq!(
        show[0]["callees"][0]["to"],
        "Acme::Shop::Report.print_total"
    );
    let io = rows(&query(&root, &["show", "IO.write_line", "--json"]));
    assert_eq!(io[0]["symbol"], serde_json::Value::Null);
    assert_eq!(
        io[0]["callers"][0]["from"],
        "Acme::Shop::Report.print_total"
    );
    let search = rows(&query(&root, &["search", "total", "--json"]));
    assert_eq!(search[0]["rank"], 1);
    assert_eq!(search.last().unwrap()["count"], 4);
    let brief = rows(&query(
        &root,
        &["brief", "Acme::Shop::Cart.total", "--json"],
    ));
    assert_eq!(brief[0]["level"], "L2");
    assert_eq!(
        brief[0]["text"].as_str().unwrap(),
        std::fs::read_to_string(root.join("target/tz/kg/brief/L2/Acme.Shop.Cart.total.md"))
            .unwrap()
    );
}

#[test]
fn brief_levels_and_budget() {
    let root = project("kg_shop", "brief");
    let card =
        |path: &str| std::fs::read_to_string(root.join("target/tz/kg/brief").join(path)).unwrap();
    assert_eq!(stdout(&query(&root, &["brief"])), card("L0.md"));
    assert_eq!(
        stdout(&query(&root, &["brief", "--project"])),
        card("L0.md")
    );
    assert_eq!(
        stdout(&query(&root, &["brief", "--module", "Cart"])),
        card("L1/Cart.md")
    );
    assert_eq!(
        stdout(&query(&root, &["brief", "Cart"])),
        card("L1/Cart.md")
    );
    assert_eq!(
        stdout(&query(&root, &["brief", "Acme::Shop::Cart.total"])),
        card("L2/Acme.Shop.Cart.total.md")
    );
    let small = stdout(&query(&root, &["brief", "--budget", "450"]));
    assert!(small.chars().count() <= 450, "{small}");
    assert!(small.contains("\u{2026}"), "{small}");
    let output = query(
        &root,
        &["brief", "Acme::Shop::Cart.total", "--module", "Cart"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("kg brief takes one of ID, --project, or --module"));
}

#[test]
fn argument_errors_are_e2000() {
    let root = project("kg_shop", "arguments");
    for (arguments, message) in [
        (
            vec!["show", "x", "--budget", "1"],
            "--budget must be an integer from 100 to 100000",
        ),
        (
            vec!["callers", "x", "--depth", "9"],
            "--depth must be an integer from 1 to 8",
        ),
        (
            vec!["show", "x", "--depth", "2"],
            "--depth is only valid with kg callers, kg callees, kg path, or kg diagram",
        ),
        (
            vec!["show", "x", "--std"],
            "--std is only valid with kg search, kg build, or kg clean",
        ),
        (
            vec!["show", "x", "--module", "M"],
            "--module is only valid with kg brief or kg find",
        ),
        (
            vec!["show", "x", "--git", "HEAD"],
            "--git is only valid with kg impact",
        ),
        (
            vec!["impact", "--from", ".", "--budget", "500"],
            "--budget is not valid with kg impact",
        ),
        (
            vec!["show", "x", "--budget", "500", "--budget", "600"],
            "--budget specified more than once",
        ),
        (vec!["show", "--budget"], "--budget needs a value"),
        (vec!["show"], "usage: tsuzuri kg show ID"),
        (vec!["search", ""], "search text must not be empty"),
        (vec!["impact"], "usage: tsuzuri kg impact"),
        (vec!["path", "a"], "usage: tsuzuri kg path FROM TO"),
        (
            vec!["path", "a", "b", "--rel", "friends"],
            "unknown relation 'friends' for --rel",
        ),
        (vec!["show", "a", "b"], "usage: tsuzuri kg show ID"),
        (
            vec!["show", "x", "--budget", "x"],
            "--budget must be an integer from 100 to 100000",
        ),
        (
            vec!["callers", "x", "--depth", "0"],
            "--depth must be an integer from 1 to 8",
        ),
        (
            vec!["callees", "x", "--depth", "x"],
            "--depth must be an integer from 1 to 8",
        ),
        (
            vec!["path", "a", "b", "--depth", "17"],
            "--depth must be an integer from 1 to 16",
        ),
        (
            vec!["brief", "x", "--project"],
            "kg brief takes one of ID, --project, or --module",
        ),
        (
            vec!["show", "x", "--project"],
            "--project is only valid with kg brief",
        ),
        (
            vec!["show", "x", "--from", "a"],
            "--from is only valid with kg impact",
        ),
        (
            vec!["show", "x", "--to", "a"],
            "--to is only valid with kg impact",
        ),
        (
            vec!["show", "x", "--rel", "calls"],
            "--rel is only valid with kg path",
        ),
        (
            vec!["show", "x", "--limit", "3"],
            "--limit is only valid with kg find",
        ),
        (
            vec!["show", "x", "--json", "--json"],
            "--json specified more than once",
        ),
        (
            vec!["show", "x", "--bogus"],
            "unknown option '--bogus'; use --help",
        ),
        (
            vec!["nope"],
            "unknown kg command 'nope'; use build, verify, clean, export, brief, show, callers, callees, uses, tests-for, search, find, impact, path, or diagram",
        ),
        (vec!["show", ""], "id must not be empty"),
        (
            vec!["impact", "--git", "HEAD", "--from", "x"],
            "usage: tsuzuri kg impact",
        ),
        (vec!["impact", "--to", "x"], "usage: tsuzuri kg impact"),
        (vec!["impact", "--git", "-x"], "invalid git revision '-x'"),
        (vec!["impact", "--git", ""], "invalid git revision ''"),
        (
            vec!["find", "ref [Line"],
            "cannot parse the type query: cannot parse the type 'ref [Line'",
        ),
        (
            vec!["find", "i64", "--limit", "0"],
            "--limit must be an integer from 1 to 100",
        ),
    ] {
        let output = tsuzuri(&root, &[&["kg"], arguments.as_slice()].concat());
        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?}: {}",
            stderr(&output)
        );
        // With --json the error is a JSON row.
        assert!(
            stderr(&output).contains("error[E2000]")
                || stderr(&output).contains("\"code\":\"E2000\""),
            "{arguments:?}"
        );
        assert!(
            stderr(&output).contains(message),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_kg_without_edges_notes_it() {
    let root = project("kg_shop", "no-edges");
    without(&root.join("target/tz/kg"), "edges.jsonl");
    let output = query(&root, &["callers", "Acme::Shop::Cart.total"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "callers of Acme::Shop::Cart.total (depth 1)\nnote: this KG has no edges.jsonl\nsummary: 0 callers, depth 1\n"
    );
}

#[test]
fn path_finds_a_shortest_chain() {
    let root = project("kg_shop", "path");
    let output = query(
        &root,
        &[
            "path",
            "test:Acme::Shop::Main.main runs",
            "Acme::Shop::Cart.line_total",
        ],
    );
    assert_eq!(
        stdout(&output),
        "path from test:Acme::Shop::Main.main runs to Acme::Shop::Cart.line_total (by calls, depth 8)\n\
         test:Acme::Shop::Main.main runs --calls--> Acme::Shop::Cart.total  Main.tz:8:28\n\
         Acme::Shop::Cart.total --calls--> Acme::Shop::Cart.line_total  Cart.tz:14:21\n\
         summary: 2 steps\n"
    );
    // Along `contains` and `calls`, from the module to a callee of one of its functions.
    let output = query(
        &root,
        &[
            "path",
            "Acme::Shop::Main",
            "Acme::Shop::Cart.total",
            "--rel",
            "contains,calls",
        ],
    );
    // The module contains the test `main runs`, which calls `Cart.total`: 2 steps.
    assert_eq!(
        stdout(&output),
        "path from Acme::Shop::Main to Acme::Shop::Cart.total (by contains or calls, depth 8)\n\
         Acme::Shop::Main --contains--> test:Acme::Shop::Main.main runs  Main.tz:8:6\n\
         test:Acme::Shop::Main.main runs --calls--> Acme::Shop::Cart.total  Main.tz:8:28\n\
         summary: 2 steps\n"
    );
    let output = query(
        &root,
        &[
            "path",
            "Acme::Shop::Cart.line_total",
            "Acme::Shop::Main.main",
        ],
    );
    assert!(stdout(&output).ends_with("summary: no path within depth 8\n"));
    let json = rows(&query(
        &root,
        &[
            "path",
            "Acme::Shop::Main.main",
            "Acme::Shop::Cart.total",
            "--json",
            "--depth",
            "1",
        ],
    ));
    assert_eq!(
        json,
        [serde_json::json!({"type":"summary","found":false,"steps":0,"depth":1})]
    );
}

#[test]
fn find_searches_the_graph_by_type() {
    let root = project("kg_shop", "find");
    // Without `--no-refresh` the search reads std's graph too, built in a cache of its own.
    let cache = root.join("cache");
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(&root)
        .args(["kg", "find", "ref [Line] -> i64"])
        .env("TSUZURI_CACHE_DIR", &cache)
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR")
        .output()
        .unwrap();
    let text = stdout(&output);
    assert!(
        text.starts_with("exact   Acme::Shop::Cart.total :: ref [Line] -> i64\n"),
        "{text}"
    );
    assert!(
        text.contains("exact   Array.length :: ref ['a] -> i64\n"),
        "{text}"
    );
    // A constraint on a primitive type follows the builtin class's rule: `Float<i64>` fails.
    let search = |query: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&root)
            .args(["kg", "find", query, "--limit", "100"])
            .env("TSUZURI_CACHE_DIR", &cache)
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap();
        stdout(&output)
    };
    let average = "exact   Array.average :: Float<'a> => ref ['a] -> 'a\n";
    assert!(!search("ref [i64] -> i64").contains(average));
    assert!(search("ref [f64] -> f64").contains(average));
    assert!(
        search("ref [i64] -> i64").contains("exact   Array.sum :: Numeric<'a> => ref ['a] -> 'a\n")
    );
    let output = query(
        &root,
        &["find", "ref Line -> i64", "--json", "--module", "Cart"],
    );
    let found = rows(&output);
    // The field accessors `ref Line -> i64` match too, and come first in byte order.
    let names: Vec<(&str, &str, &str)> = found
        .iter()
        .filter(|row| row["event"] == "result")
        .map(|row| {
            (
                row["tier"].as_str().unwrap(),
                row["name"].as_str().unwrap(),
                row["origin"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("exact", "Acme::Shop::Cart.Line.quantity", "user"),
            ("exact", "Acme::Shop::Cart.Line.unit_price", "user"),
            ("exact", "Acme::Shop::Cart.line_total", "user"),
        ]
    );
    assert_eq!(found.last().unwrap()["event"], "summary");
    // A field reads as its accessor, a case as its constructor.
    let output = query(
        &root,
        &[
            "find", "--result", "string", "--takes", "ref Line", "--module", "Cart",
        ],
    );
    assert!(
        stdout(&output).contains("Acme::Shop::Cart.Line.sku"),
        "{}",
        stdout(&output)
    );
    let output = query(&root, &["find", "Line -> Line -> Line -> Line"]);
    assert_eq!(stdout(&output), "no matches\n");
    let output = query(&root, &["find", "ref [i64 ->"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("cannot parse the type query"));
    let output = query(&root, &["find", "ref Line -> i64", "--limit", "1"]);
    assert!(
        stdout(&output).ends_with("... 2 more; raise --limit (at most 100)\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn no_refresh_reports_e2009_and_a_stale_kg_is_rebuilt() {
    let root = project("kg_shop", "stale");
    let mut cart = std::fs::read_to_string(root.join("Cart.tz")).unwrap();
    cart.push_str("\n// edited\n");
    std::fs::write(root.join("Cart.tz"), cart).unwrap();
    let output = query(&root, &["show", "total"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2009]"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("is stale (sources changed); run tsuzuri kg build"),
        "{}",
        stderr(&output)
    );
    // Without --no-refresh the graph is built again first.
    let output = tsuzuri(&root, &["kg", "show", "total"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).starts_with("Acme::Shop::Cart.total\n"));
    assert!(query(&root, &["show", "total"]).status.success());
    std::fs::remove_dir_all(root.join("target/tz/kg")).unwrap();
    let output = query(&root, &["show", "total"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2009]: no KG at"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn output_is_deterministic() {
    let (first, second) = (project("kg_shop", "same-a"), project("kg_shop", "same-b"));
    for arguments in [
        vec!["brief"],
        vec!["show", "total"],
        vec!["show", "total", "--json"],
        vec!["callers", "line_total", "--depth", "3"],
        vec!["callees", "main", "--depth", "3", "--json"],
        vec!["uses", "Line"],
        vec!["search", "t"],
        vec!["path", "main", "line_total"],
    ] {
        let once = stdout(&query(&first, &arguments));
        assert!(!once.is_empty(), "{arguments:?}");
        assert_eq!(once, stdout(&query(&first, &arguments)), "{arguments:?}");
        assert_eq!(once, stdout(&query(&second, &arguments)), "{arguments:?}");
    }
}

/// A generic record's accessor and a generic union's constructors keep the owner's type arguments,
/// a view's `..` ends a name, and an unterminated region is a parse error, not a panic.
#[test]
fn find_keeps_type_arguments_and_rejects_bad_queries() {
    let outer =
        std::env::temp_dir().join(format!("tsuzuri-kg-query-generic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    // Without a manifest the folder's name is the namespace.
    let root = outer.join("generic");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        concat!(
            "record Box<'a> { value: 'a }\n",
            "union Slot<'a> = Empty | Full of 'a\n",
            "def head :: ref mut [i64..] -> i64 = \\values -> values[0]\n",
            "def first :: Copy<'a> => ref mut ['a..] -> 'a = \\values -> values[0]\n",
        ),
    )
    .unwrap();
    let cache = outer.join("cache");
    let find = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&root)
            .args(["kg", "find"])
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", &cache)
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap()
    };
    let main = |query: &str| stdout(&find(&[query, "--module", "Main"]));
    assert_eq!(
        main("ref Box<i64> -> i64"),
        "exact   Generic::Main.Box.value :: ref Box<'a> -> 'a\n"
    );
    assert_eq!(main("ref Box<i64> -> string"), "no matches\n");
    assert_eq!(
        main("i64 -> Slot<i64>"),
        "exact   Generic::Main.Slot.Full :: 'a -> Slot<'a>\n"
    );
    assert_eq!(main("string -> Slot<i64>"), "no matches\n");
    assert!(main("Slot<i64>").starts_with("exact   Generic::Main.Slot.Empty :: Slot<'a>\n"));
    // The standard constructors too.
    let some = stdout(&find(&["i64 -> Maybe<i64>", "--limit", "100"]));
    assert!(
        some.contains("exact   Maybe.Maybe.Some :: 'a -> Maybe<'a>\n"),
        "{some}"
    );
    let none = stdout(&find(&["Maybe<i64>", "--limit", "100"]));
    assert!(
        none.contains("exact   Maybe.Maybe.None :: Maybe<'a>\n"),
        "{none}"
    );
    // `[i64..]` is a view, and `'a..` is the variable `'a` before `..`.
    assert_eq!(
        main("ref mut [string..] -> string"),
        "exact   Generic::Main.first :: Copy<'a> => ref mut ['a..] -> 'a\n"
    );
    assert_eq!(main("ref mut [string..] -> i64"), "no matches\n");
    assert_eq!(main("ref [i64] -> i64"), "no matches\n");
    assert!(
        main("ref mut [i64..] -> i64")
            .contains("exact   Generic::Main.head :: ref mut [i64..] -> i64\n")
    );
    for broken in ["ref {r", "ref {r [i64]", "{"] {
        let output = find(&[broken]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{broken}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("cannot parse the type query"),
            "{broken}: {}",
            stderr(&output)
        );
    }
    // `--budget` bounds the text and not the JSON.
    let wide = find(&["--takes", "_", "--limit", "100", "--budget", "100"]);
    let shown = stdout(&wide);
    assert!(shown.chars().count() <= 100, "{shown}");
    assert!(shown.contains(" more\n"), "{shown}");
    let json = find(&[
        "--takes", "_", "--limit", "100", "--budget", "100", "--json",
    ]);
    let results = rows(&json)
        .iter()
        .filter(|row| row["event"] == "result")
        .count();
    assert_eq!(results, 100);
    let _ = std::fs::remove_dir_all(&outer);
}

/// `--no-refresh` builds no std graph, and reads the one that the cache has.
#[test]
fn no_refresh_reads_an_existing_std_kg() {
    let root = project("kg_shop", "std-cached");
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&root)
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap()
    };
    let output = run(&["kg", "show", "Array.length", "--no-refresh"]);
    assert!(
        stderr(&output).contains("unknown id 'Array.length'"),
        "{}",
        stderr(&output)
    );
    assert!(!root.join("cache/kg-std").exists());
    assert!(run(&["kg", "build", "--std"]).status.success());
    let output = run(&["kg", "show", "Array.length", "--no-refresh"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).starts_with("Array.length\n"));
    let output = run(&["kg", "find", "ref ['a] -> i64", "--no-refresh"]);
    assert!(
        stdout(&output).contains("exact   Array.length :: ref ['a] -> i64\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn std_ids_resolve_when_the_std_kg_exists() {
    let root = project("kg_shop", "std-ids");
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&root)
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_KG")
            .env_remove("TSUZURI_KG_DIR")
            .output()
            .unwrap()
    };
    let output = run(&["kg", "show", "Array.length"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("Array.length\n"), "{text}");
    assert!(text.contains("\nsignature: "), "{text}");
    // A call into std from the project is an edge of the project's graph.
    let output = run(&["kg", "callers", "IO.write_line"]);
    assert!(
        stdout(&output).contains("1 Acme::Shop::Report.print_total  Report.tz:6:9"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn queries_read_a_kg_without_facts() {
    let root = project("kg_shop", "no-facts");
    without(&root.join("target/tz/kg"), "facts/");
    let output = query(&root, &["show", "total"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        !text.contains("effects:") && !text.contains("sites:"),
        "{text}"
    );
    assert!(text.contains("callers (3):\n"), "{text}");
}
