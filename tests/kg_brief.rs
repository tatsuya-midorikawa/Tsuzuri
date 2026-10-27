//! G27 Phase 2 (D-60): the brief cards and `tsuzuri kg build|verify|clean|export`. The expected
//! cards come from the reference implementation of G27 appendix F (with the D-60 names), run on the
//! rows of the graph; they are not the compiler's output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::kg::brief::{self, ELLIPSIS};
use tsuzuri::kg::manifest::Manifest;
use tsuzuri::kg::store::KgData;
use tsuzuri::kg::write::Files;

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn root(label: &str, files: &[(String, String)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-kg-brief-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn shop(label: &str) -> PathBuf {
    let source = repository().join("tests/fixtures/kg_shop");
    let files: Vec<(String, String)> = std::fs::read_dir(&source)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap(),
            )
        })
        .collect();
    root(label, &files)
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Every file below `dir` whose name does not start with `.`.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().unwrap().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    found
}

/// A graph in memory from the given files, as `KgData::from_parts` reads one that was just built.
fn data(
    project: &str,
    edition: &str,
    version: Option<&str>,
    warnings: u64,
    parts: Files,
) -> KgData {
    let manifest = Manifest {
        schema: "tz-kg/1".into(),
        compiler: "tsuzuri 0.1.0".into(),
        edition: edition.into(),
        project: project.into(),
        version: version.map(str::to_owned),
        complete: true,
        source_hash: String::new(),
        counts: vec![("warnings".into(), warnings)],
        modules: Vec::new(),
        files: parts
            .0
            .keys()
            .map(|path| (path.clone(), String::new()))
            .collect(),
    };
    KgData::from_parts(&manifest, &parts).unwrap()
}

fn golden(name: &str, file: &str) -> Vec<u8> {
    std::fs::read(repository().join("tests/golden/kg").join(name).join(file)).unwrap()
}

#[test]
fn cards_match_the_golden_files() {
    // A whole graph: every file but the per-machine marker equals the golden one.
    let root = shop("cards");
    let output = tsuzuri(&root, &["check", "."]);
    assert!(output.status.success(), "{}", stderr(&output));
    let written = files(&root.join("target/tz/kg"));
    let expected = files(&repository().join("tests/golden/kg/shop"));
    assert_eq!(
        written.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &expected {
        assert_eq!(
            String::from_utf8_lossy(&written[path]),
            String::from_utf8_lossy(bytes),
            "{path}"
        );
    }
    // The cards of the G26 goldens: point has edges, polymorphism has nodes only.
    for (name, with_edges) in [("point", true), ("polymorphism", false)] {
        let mut parts = Files::default();
        parts.put("nodes.jsonl", golden(name, "nodes.jsonl"));
        if with_edges {
            parts.put("edges.jsonl", golden(name, "edges.jsonl"));
        }
        let graph = data(name, "2027", None, 0, parts);
        assert_eq!(
            brief::l0(&graph, brief::L0_LIMIT).into_bytes(),
            golden(name, "brief/L0.md"),
            "{name}"
        );
        for module in graph.nodes.iter().filter(|node| node.kind == "module") {
            assert_eq!(
                brief::l1(&graph, &module.id, brief::L1_LIMIT)
                    .unwrap()
                    .into_bytes(),
                golden(name, &format!("brief/L1/{}.md", module.id)),
                "{name} {}",
                module.id
            );
        }
        let (ids, omitted) = brief::l2_file_ids(&graph);
        assert_eq!(omitted, 0);
        let names = tsuzuri::kg::write::unique_names(&ids);
        for id in &ids {
            assert_eq!(
                brief::l2(&graph, id, brief::L2_LIMIT).unwrap().into_bytes(),
                golden(name, &format!("brief/L2/{}.md", names[id])),
                "{name} {id}"
            );
        }
    }
}

fn shop_graph() -> KgData {
    let golden_dir = repository().join("tests/golden/kg/shop");
    let mut parts = Files::default();
    for (path, bytes) in files(&golden_dir) {
        if path.ends_with(".jsonl") {
            parts.put(path, bytes);
        }
    }
    data("shop", "2026", Some("0.1.0"), 1, parts)
}

#[test]
fn card_fit_drops_the_lowest_priority_first() {
    // Appendix E-1: the L2 card of `Cart.total` at smaller and smaller limits.
    let graph = shop_graph();
    let card = |limit| brief::l2(&graph, "Acme::Shop::Cart.total", limit).unwrap();
    let length = |text: &str| text.chars().count();
    assert_eq!(length(&card(700)), 384);
    assert_eq!(length(&card(330)), 315);
    assert!(card(330).ends_with(&format!("{ELLIPSIS}1 more\n")));
    assert_eq!(length(&card(280)), 204);
    assert!(card(280).ends_with(&format!(
        "callees (1): Acme::Shop::Cart.line_total\n{ELLIPSIS}2 more\n"
    )));
    assert_eq!(length(&card(200)), 163);
    assert!(card(200).ends_with(&format!("sites: trap 1\n{ELLIPSIS}3 more\n")));
    assert_eq!(length(&card(100)), 93);
    assert!(card(100).ends_with(&format!("{ELLIPSIS}6 more\n")));
    assert_eq!(
        card(40),
        format!("Acme::Shop::Cart.total\ndef | public | C{ELLIPSIS}")
    );
}

fn node(id: &str, kind: &str, module: &str, name: &str, signature: &str) -> String {
    let path = format!("{}.tz", module.rsplit("::").next().unwrap());
    serde_json::json!({
        "id": id, "kind": kind, "module": module, "name": name, "signature": signature,
        "params": [], "result": null, "public": true, "export": false, "pure": false, "doc": null,
        "deprecated": null, "origin": "project",
        "span": {"path": path, "start": 0, "end": 0, "line": 1, "column": 1}
    })
    .to_string()
}

/// Appendix F-3: the synthetic project `syn` of `count` modules.
fn syn(count: usize) -> KgData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..count {
        let m = format!("M{i:04}");
        let path = format!("{m}.tz");
        let row = |id: String,
                   kind: &str,
                   name: &str,
                   signature: String,
                   line: usize,
                   start: usize,
                   public: bool,
                   export: bool| {
            serde_json::json!({
                "id": id, "kind": kind, "module": m, "name": name, "signature": signature, "params": [],
                "result": null, "public": public, "export": export, "pure": false, "doc": null,
                "deprecated": null, "origin": "project",
                "span": {"path": path, "start": start, "end": start + name.len(), "line": line, "column": 1}
            })
            .to_string()
        };
        nodes.push(node(&m, "module", &m, &m, &format!("module {m}")));
        let item = format!("Item{i:04}");
        nodes.push(row(
            format!("{m}.{item}"),
            "record",
            &item,
            format!("record {item}"),
            1,
            7,
            true,
            false,
        ));
        for (field, ty) in [("id", "i64"), ("weight", "i64")] {
            nodes.push(row(
                format!("{m}.{item}.{field}"),
                "field",
                field,
                format!("{field}: {ty}"),
                1,
                20 + field.len(),
                true,
                false,
            ));
        }
        for j in 0..4 {
            let name = format!("f{j}_{i:04}");
            let export = if j == 0 { "export " } else { "" };
            nodes.push(row(
                format!("{m}.{name}"),
                "def",
                &name,
                format!("{export}def {name} :: i64 -> i64"),
                3 + j,
                100 + j * 50,
                true,
                j == 0,
            ));
            if i > 0 {
                edges.push(
                    serde_json::json!({
                        "rel": "calls", "from": format!("{m}.{name}"),
                        "to": format!("M{:04}.f{j}_{:04}", i - 1, i - 1), "args": ["copy"],
                        "span": {"path": path, "start": 100 + j * 50, "end": 110 + j * 50, "line": 3 + j, "column": 40}
                    })
                    .to_string(),
                );
            }
        }
        let private = format!("private{i:04}");
        nodes.push(row(
            format!("{m}.{private}"),
            "def",
            &private,
            format!("def {private} :: ref {item} -> i64"),
            8,
            400,
            false,
            false,
        ));
        nodes.push(
            serde_json::json!({
                "id": format!("test:{m}.f0 of {m}"), "kind": "test", "module": m, "name": format!("f0 of {m}"),
                "signature": format!("test \"f0 of {m}\""), "params": [], "result": null, "public": false,
                "export": false, "pure": false, "doc": null, "deprecated": null, "origin": "project",
                "span": {"path": path, "start": 500, "end": 510, "line": 10, "column": 6}
            })
            .to_string(),
        );
    }
    let mut parts = Files::default();
    parts.put("nodes.jsonl", (nodes.join("\n") + "\n").into_bytes());
    parts.put("edges.jsonl", (edges.join("\n") + "\n").into_bytes());
    data("syn", "2026", None, 0, parts)
}

fn more_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with(ELLIPSIS))
        .collect()
}

#[test]
fn worked_truncation_example() {
    // Appendix E-2 with the D-60 header line: 30 modules fit in 2,385 characters.
    let text = brief::l0(&syn(30), brief::L0_LIMIT);
    assert_eq!(text.chars().count(), 2385);
    assert_eq!(
        more_lines(&text),
        [
            "\u{2026}10 more",
            "\u{2026}15 more",
            "\u{2026}23 more",
            "\u{2026}30 more"
        ]
    );
    assert!(
        text.contains("tz-kg/1 | edition 2026 | modules 30 | nodes 300 | edges 116 | tests 30\n")
    );
}

#[test]
fn l0_fits_for_1000_modules() {
    let text = brief::l0(&syn(1000), brief::L0_LIMIT);
    assert_eq!(text.chars().count(), 2397);
    assert_eq!(
        more_lines(&text),
        [
            "\u{2026}980 more",
            "\u{2026}985 more",
            "\u{2026}993 more",
            "\u{2026}1000 more"
        ]
    );
}

#[test]
fn mandatory_lines_fit_every_limit() {
    // Appendix E-4: a 64-character name and a 300-character ID do not reach the clipping rule.
    let long = "x".repeat(250);
    let module = format!("A::{long}");
    let name = "n".repeat(64);
    let id = format!("{module}.{name}");
    let mut row: serde_json::Value = serde_json::from_str(&node(
        &id,
        "def",
        &module,
        &name,
        &format!("export pure def {name} :: {}", "t".repeat(400)),
    ))
    .unwrap();
    row["export"] = true.into();
    row["pure"] = true.into();
    row["span"]["path"] = "p".repeat(200).into();
    row["span"]["line"] = 1_000_000_000u64.into();
    let mut parts = Files::default();
    parts.put(
        "nodes.jsonl",
        format!(
            "{}\n{row}\n",
            node(
                &module,
                "module",
                &module,
                &long,
                &format!("module {module}")
            )
        )
        .into_bytes(),
    );
    parts.put("edges.jsonl", Vec::new());
    let graph = data(&"p".repeat(100), "2026", Some(&"v".repeat(50)), 0, parts);
    let l2 = brief::l2(&graph, &id, brief::L2_LIMIT).unwrap();
    assert!(
        l2.chars().count() <= brief::L2_LIMIT && l2.ends_with('\n'),
        "{l2}"
    );
    let l1 = brief::l1(&graph, &module, brief::L1_LIMIT).unwrap();
    assert!(l1.chars().count() <= brief::L1_LIMIT && l1.ends_with('\n'));
    let l0 = brief::l0(&graph, brief::L0_LIMIT);
    assert!(l0.chars().count() <= brief::L0_LIMIT && l0.ends_with('\n'));
}

#[test]
fn l2_files_are_capped_at_256() {
    let mut main = String::new();
    for index in 0..300 {
        main.push_str(&format!(
            "export def f{index:03} :: i64 -> i64 = \\n -> n + {index}\n"
        ));
    }
    let root = root("capped", &[("Main.tz".into(), main)]);
    let output = tsuzuri(&root, &["check", "."]);
    assert!(output.status.success(), "{}", stderr(&output));
    let kg = root.join("target/tz/kg");
    assert_eq!(std::fs::read_dir(kg.join("brief/L2")).unwrap().count(), 256);
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(kg.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["counts"]["l2_cards"], 256);
    assert_eq!(manifest["counts"]["l2_omitted"], 44);
}

#[test]
fn kg_build_verify_clean_export_round_trip() {
    let root = shop("round-trip");
    let output = tsuzuri(&root, &["kg", "build"]);
    assert_eq!(
        stdout(&output),
        "kg: wrote 19 files to target/tz/kg\n",
        "{}",
        stderr(&output)
    );
    assert_eq!(
        stdout(&tsuzuri(&root, &["kg", "build"])),
        "kg: up to date (target/tz/kg)\n"
    );
    let output = tsuzuri(&root, &["kg", "build", "--json"]);
    assert_eq!(
        stdout(&output),
        "{\"type\":\"kg_build\",\"status\":\"unchanged\",\"dir\":\"target/tz/kg\",\"files\":0,\"complete\":true}\n"
    );
    let output = tsuzuri(&root, &["kg", "verify"]);
    assert_eq!(
        stdout(&output),
        "kg: ok (14 nodes, 17 files, source a52eb1a4dfdb)\n",
        "{}",
        stderr(&output)
    );
    let output = tsuzuri(&root, &["kg", "export", "--nodes"]);
    assert_eq!(output.stdout, golden("shop", "nodes.jsonl"));
    let output = tsuzuri(&root, &["kg", "export", "--edges", "--no-refresh"]);
    assert_eq!(output.stdout, golden("shop", "edges.jsonl"));
    let output = tsuzuri(&root, &["kg", "export", "--facts"]);
    let mut facts = golden("shop", "facts/Cart.jsonl");
    facts.extend(golden("shop", "facts/Main.jsonl"));
    facts.extend(golden("shop", "facts/Report.jsonl"));
    assert_eq!(output.stdout, facts);
    assert!(tsuzuri(&root, &["kg", "clean"]).status.success());
    assert!(!root.join("target/tz/kg").exists());
    // `--force` writes a fresh graph again, byte for byte the same.
    assert!(tsuzuri(&root, &["kg", "build"]).status.success());
    let first = files(&root.join("target/tz/kg"));
    assert!(tsuzuri(&root, &["kg", "build", "--force"]).status.success());
    assert_eq!(files(&root.join("target/tz/kg")), first);
}

fn kind_of(output: &Output) -> String {
    let text = stderr(output);
    let line = text
        .lines()
        .find(|line| line.contains("E2009"))
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
    value["kind"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn verify_reports_each_e2009_kind() {
    let root = shop("kinds");
    let verify = |root: &Path| tsuzuri(root, &["kg", "verify", "--json"]);
    let output = verify(&root);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(kind_of(&output), "kg_missing");
    assert!(tsuzuri(&root, &["kg", "build"]).status.success());
    let kg = root.join("target/tz/kg");
    let manifest = std::fs::read_to_string(kg.join("manifest.json")).unwrap();
    std::fs::write(kg.join("manifest.json"), "{not json").unwrap();
    assert_eq!(kind_of(&verify(&root)), "kg_broken");
    let hash = serde_json::from_str::<serde_json::Value>(&manifest).unwrap()["source_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(
        kg.join("manifest.json"),
        manifest.replace(&hash, &"0".repeat(64)),
    )
    .unwrap();
    let output = verify(&root);
    assert_eq!(kind_of(&output), "kg_stale");
    assert!(stderr(&output).contains("is stale (sources changed); run tsuzuri kg build"));
    std::fs::write(kg.join("manifest.json"), &manifest).unwrap();
    assert!(verify(&root).status.success());
    std::fs::remove_file(kg.join(".tsuzuri-kg")).unwrap();
    let output = verify(&root);
    assert_eq!(kind_of(&output), "kg_missing");
    assert!(stderr(&output).contains("no .tsuzuri-kg marker"));
    let broken = self::root(
        "kinds-broken",
        &[("Main.tz".into(), "let value: i64 = \"a\"\nvalue\n".into())],
    );
    assert_eq!(tsuzuri(&broken, &["check", "."]).status.code(), Some(1));
    let output = verify(&broken);
    assert_eq!(kind_of(&output), "kg_incomplete");
    assert!(stderr(&output).contains("is incomplete: the project has 1 errors"));
}

#[test]
fn export_needs_exactly_one_flag() {
    let root = shop("export-flags");
    for arguments in [
        vec!["kg", "export"],
        vec!["kg", "export", "--nodes", "--edges"],
    ] {
        let output = tsuzuri(&root, &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            stderr(&output).contains("kg export needs exactly one of --nodes, --edges, --facts")
        );
    }
    let output = tsuzuri(&root, &["kg"]);
    assert_eq!(output.status.code(), Some(2));
    let output = tsuzuri(&root, &["kg", "frobnicate"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown kg command 'frobnicate'"));
    let output = tsuzuri(&root, &["kg", "build", "--bogus"]);
    assert!(stderr(&output).contains("unknown option '--bogus'; use --help"));
}

#[test]
fn check_and_kg_build_write_the_same_kg() {
    let checked = shop("same-check");
    let built = shop("same-build");
    assert!(tsuzuri(&checked, &["check", "."]).status.success());
    assert!(tsuzuri(&built, &["kg", "build", "."]).status.success());
    assert_eq!(
        files(&checked.join("target/tz/kg")),
        files(&built.join("target/tz/kg"))
    );
}

#[test]
fn kg_build_from_a_loose_file() {
    let root = root(
        "loose",
        &[(
            "Physics.tz".into(),
            "def speed :: f64 -> f64 -> f64 = \\d t -> d / t\n".into(),
        )],
    );
    let output = tsuzuri(&root, &["kg", "build", "Physics.tz"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let nodes = std::fs::read_to_string(root.join("target/tz/kg/nodes.jsonl")).unwrap();
    assert!(nodes.contains(".speed\""), "{nodes}");
}
