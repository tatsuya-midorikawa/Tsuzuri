//! G30 (D-60): `tsuzuri kg diagram` and the views of `kg build`. The expected outputs in
//! `tests/golden/diagram/cli/` are those of the reference implementation (G30 appendix E) run on
//! the rows of the fixture projects' own graphs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A copy of a fixture project (`point` is `examples/point`) in a fresh root, in a folder of its
/// name, so that its namespace is that of the expected outputs.
fn project(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-diagram-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let source = if name == "point" {
        repository().join("examples/point")
    } else {
        repository().join("tests/fixtures/diagram/src").join(name)
    };
    let target = root.join(name);
    std::fs::create_dir_all(&target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, target.join(path.file_name().unwrap())).unwrap();
        }
    }
    target
}

fn tsuzuri(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
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

fn golden(name: &str) -> String {
    std::fs::read_to_string(repository().join("tests/golden/diagram/cli").join(name)).unwrap()
}

fn diagram(project: &Path, arguments: &[&str]) -> Output {
    let path = project.to_str().unwrap();
    let mut all = vec!["kg", "diagram"];
    all.extend_from_slice(arguments);
    all.extend_from_slice(&["--path", path]);
    tsuzuri(&all)
}

#[test]
fn kg_diagram_prints_the_diagram() {
    let cases: [(&str, &[&str], &str); 12] = [
        ("point", &["module"], "point.module.mmd"),
        ("point", &["module", "--format", "dot"], "point.module.dot"),
        (
            "point",
            &["calls", "Point::Point.hypotenuse"],
            "point.calls.mmd",
        ),
        ("cycle", &["module"], "cycle.module.mmd"),
        ("shapes", &["types"], "shapes.types.mmd"),
        ("shapes", &["types", "--format", "dot"], "shapes.types.dot"),
        (
            "shapes",
            &["layout", "Shapes::Shapes.Point"],
            "shapes.layout.point.md",
        ),
        (
            "shapes",
            &["layout", "Shapes.Shape"],
            "shapes.layout.shape.md",
        ),
        ("flow", &["calls", "Flow::Main.$entry"], "flow.calls.mmd"),
        ("flow", &["concurrency"], "flow.concurrency.mmd"),
        ("own", &["ownership", "Main.demo"], "own.ownership.md"),
        (
            "light",
            &["transitions", "Light.Light"],
            "light.transitions.mmd",
        ),
    ];
    for (name, arguments, expected) in cases {
        let root = project(name);
        let output = diagram(&root, arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{name} {arguments:?}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), golden(expected), "{name} {arguments:?}");
    }
}

#[test]
fn kg_diagram_json_line() {
    let root = project("point");
    let output = diagram(&root, &["module", "--json"]);
    let text = stdout(&output);
    assert_eq!(text.lines().count(), 1);
    let row: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        row,
        serde_json::json!({"kind":"module","format":"mermaid","text":golden("point.module.mmd"),"nodes":2,"folded":false})
    );
    assert!(text.starts_with("{\"kind\":\"module\",\"format\":\"mermaid\",\"text\":"));
    let output = diagram(&root, &["layout", "Point.Point", "--json"]);
    let row: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(row["format"], "markdown");
    assert_eq!(row["nodes"], 2);
}

#[test]
fn kg_diagram_argument_errors() {
    let root = project("point");
    for (arguments, message) in [
        (vec![], "usage: tsuzuri kg diagram KIND [ID] [options]"),
        (
            vec!["calls", "a", "b"],
            "usage: tsuzuri kg diagram KIND [ID] [options]",
        ),
        (
            vec!["flows"],
            "unknown diagram kind 'flows'; use module, types, calls, ownership, concurrency, layout, or transitions",
        ),
        (vec!["calls"], "diagram kind 'calls' needs an id"),
        (vec!["ownership"], "diagram kind 'ownership' needs an id"),
        (
            vec!["layout", "x", "--format", "dot"],
            "format 'dot' is not available for kind 'layout'",
        ),
        (
            vec!["module", "--format", "svg"],
            "--format must be mermaid or dot",
        ),
        (
            vec!["module", "--max-nodes", "4"],
            "--max-nodes must be an integer from 5 to 200",
        ),
        (
            vec!["module", "--max-nodes", "x"],
            "--max-nodes must be an integer from 5 to 200",
        ),
        (
            vec!["calls", "hypotenuse", "--depth", "7"],
            "--depth must be an integer from 1 to 6",
        ),
        (
            vec!["module", "--depth", "2"],
            "--depth is only valid with the calls diagram",
        ),
        (
            vec!["module", "--json", "--json"],
            "--json specified more than once",
        ),
        (
            vec!["module", "--bogus"],
            "unknown option '--bogus'; use --help",
        ),
        (
            vec!["module", "--budget", "200"],
            "--budget is not valid with kg diagram",
        ),
        (
            vec!["calls", "Point::Point.Point"],
            "'Point::Point.Point' is a record; diagram kind 'calls' needs a function id",
        ),
        (
            vec!["types", "Point::Point.distance"],
            "'Point::Point.distance' is a def; diagram kind 'types' needs a module id",
        ),
        (
            vec!["transitions", "Point::Point.Point"],
            "'Point::Point.Point' is a record; diagram kind 'transitions' needs a union id",
        ),
        (
            vec!["ownership", "zzz"],
            "unknown id 'zzz' for diagram kind 'ownership'",
        ),
    ] {
        let output = diagram(&root, &arguments);
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
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains(message),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
    // A missing value (the helper puts `--path` last, so the command is run as it is).
    let output = tsuzuri(&["kg", "diagram", "module", "--max-nodes"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--max-nodes needs a value"),
        "{}",
        stderr(&output)
    );
    // The kinds of the errors.
    for (arguments, kind) in [
        (vec!["module", "--bogus"], "diagram_bad_option"),
        (vec!["flows"], "diagram_unknown_kind"),
        (vec!["calls", "zzz"], "diagram_unknown_id"),
    ] {
        let mut arguments = arguments;
        arguments.push("--json");
        let output = diagram(&root, &arguments);
        let row: serde_json::Value =
            serde_json::from_str(stderr(&output).lines().last().unwrap()).unwrap();
        assert_eq!(row["kind"], kind, "{arguments:?}");
    }
    // Other `kg` commands do not take the diagram's options.
    let output = tsuzuri(&[
        "kg",
        "show",
        "x",
        "--format",
        "dot",
        "--path",
        root.to_str().unwrap(),
    ]);
    assert!(stderr(&output).contains("--format is only valid with kg diagram"));
}

#[test]
fn kg_diagram_unknown_id_suggests() {
    let root = project("point");
    let output = diagram(&root, &["calls", "Other.distance"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains(
            "unknown id 'Other.distance' for diagram kind 'calls'; did you mean: Point::Point.distance"
        ),
        "{}",
        stderr(&output)
    );
    // A unique suffix names the function, as the queries of `kg` do.
    let output = diagram(&root, &["calls", "hypotenuse"]);
    assert_eq!(stdout(&output), golden("point.calls.mmd"));
}

#[test]
fn kg_diagram_no_refresh_without_a_kg_is_e2009() {
    let root = project("point");
    let output = diagram(&root, &["module", "--no-refresh"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2009]: no KG at"),
        "{}",
        stderr(&output)
    );
    // Without --no-refresh the graph is built first.
    assert!(diagram(&root, &["module"]).status.success());
    assert!(diagram(&root, &["module", "--no-refresh"]).status.success());
}

#[test]
fn kg_diagram_dot_for_ownership_is_e2000() {
    let root = project("own");
    let output = diagram(&root, &["ownership", "Main.demo", "--format", "dot"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("format 'dot' is not available for kind 'ownership'"));
}

#[test]
fn kg_diagram_transitions_reject_a_changed_source() {
    let root = project("light");
    assert!(
        tsuzuri(&["kg", "build", root.to_str().unwrap()])
            .status
            .success()
    );
    let path = root.join("Light.tz");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{text}\n")).unwrap();
    let output = diagram(&root, &["transitions", "Light.Light", "--no-refresh"]);
    // The graph is stale before the source is read.
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2009]"),
        "{}",
        stderr(&output)
    );
}

fn views(root: &Path) -> Vec<(String, Vec<u8>)> {
    let dir = root.join("target/tz/kg/views");
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .map(|entry| {
                    let path = entry.unwrap().path();
                    (
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        std::fs::read(&path).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

#[test]
fn kg_build_writes_views_and_verify_passes() {
    for (name, expected) in [
        (
            "shapes",
            vec![
                ("modules.mmd", "shapes.views.modules.mmd"),
                ("types.mmd", "shapes.views.types.mmd"),
            ],
        ),
        (
            "flow",
            vec![
                ("concurrency.mmd", "flow.views.concurrency.mmd"),
                ("modules.mmd", "flow.views.modules.mmd"),
            ],
        ),
    ] {
        let root = project(name);
        let path = root.to_str().unwrap();
        assert!(tsuzuri(&["kg", "build", path]).status.success());
        let files = views(&root);
        let names: Vec<&str> = files.iter().map(|(file, _)| file.as_str()).collect();
        let wanted: Vec<&str> = expected.iter().map(|(file, _)| *file).collect();
        assert_eq!(names, wanted, "{name}");
        for ((_, bytes), (_, golden_name)) in files.iter().zip(&expected) {
            assert_eq!(
                String::from_utf8_lossy(bytes),
                golden(golden_name),
                "{name}"
            );
        }
        let output = tsuzuri(&["kg", "verify", path]);
        assert!(output.status.success(), "{}", stderr(&output));
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("target/tz/kg/manifest.json")).unwrap(),
        )
        .unwrap();
        for (file, _) in &expected {
            assert!(
                manifest["files"][format!("views/{file}")].is_string(),
                "{name} {file}"
            );
        }
    }
}

#[test]
fn kg_build_is_deterministic_with_views() {
    let (first, second) = (project("flow"), project("flow"));
    for root in [&first, &second] {
        assert!(
            tsuzuri(&["kg", "build", root.to_str().unwrap()])
                .status
                .success()
        );
    }
    assert_eq!(views(&first), views(&second));
    let manifest = |root: &Path| std::fs::read(root.join("target/tz/kg/manifest.json")).unwrap();
    assert_eq!(manifest(&first), manifest(&second));
}

#[test]
fn golden_diagrams_parse_with_mmdc_when_available() {
    let available = Command::new("mmdc")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        return;
    }
    let out = std::env::temp_dir().join(format!("tsuzuri-mmdc-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    for directory in ["tests/golden/diagram", "tests/golden/diagram/cli"] {
        for entry in std::fs::read_dir(repository().join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "mmd") {
                let target = out.join(format!(
                    "{}.svg",
                    path.file_stem().unwrap().to_string_lossy()
                ));
                let output = Command::new("mmdc")
                    .arg("-i")
                    .arg(&path)
                    .arg("-o")
                    .arg(&target)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}: {}",
                    path.display(),
                    stderr(&output)
                );
            }
        }
    }
}
