//! G27 Phase 1 (D-60): `check`, `build`, `run` and `test` write the knowledge graph of a project to
//! `target/tz/kg`, leave a fresh one alone, never overwrite a directory they did not write, and
//! change neither the diagnostics nor the exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh root with the given files.
fn root(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-kg-build-{label}-{}-{}",
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

/// A copy of the sources of `tests/fixtures/kg_shop` in a fresh root (not the graph that a `check`
/// of the fixture leaves in its `target/`).
fn shop(label: &str) -> PathBuf {
    let source = repository().join("tests/fixtures/kg_shop");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        files.push((name, std::fs::read_to_string(&path).unwrap()));
    }
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    root(label, &files)
}

fn tsuzuri(cwd: &Path, arguments: &[&str], environment: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command
        .current_dir(cwd)
        .args(arguments)
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR");
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Every file below `dir` whose name does not start with `.`, by relative path.
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

/// The modification time and identity of every file below `dir`.
fn stamps(dir: &Path) -> BTreeMap<PathBuf, (std::time::SystemTime, u64)> {
    let mut found = BTreeMap::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::metadata(&path).unwrap();
            if metadata.is_dir() {
                pending.push(path);
            } else {
                #[cfg(unix)]
                let identity = std::os::unix::fs::MetadataExt::ino(&metadata);
                #[cfg(not(unix))]
                let identity = metadata.len();
                found.insert(path, (metadata.modified().unwrap(), identity));
            }
        }
    }
    found
}

fn manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

const BROKEN: &str = "def broken :: i64 -> i64 = \\x -> x\n\nlet value: i64 = \"a\"\nvalue\n";

#[test]
fn check_creates_the_kg_and_a_second_check_writes_nothing() {
    let root = shop("first");
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let kg = root.join("target/tz/kg");
    for file in [
        ".tsuzuri-kg",
        "manifest.json",
        "nodes.jsonl",
        "edges.jsonl",
        "diag.jsonl",
    ] {
        assert!(kg.join(file).is_file(), "{file}");
    }
    let manifest = manifest(&kg);
    assert_eq!(manifest["schema"], "tz-kg/1");
    assert_eq!(manifest["complete"], true);
    assert_eq!(manifest["counts"]["warnings"], 1);
    let diag = std::fs::read_to_string(kg.join("diag.jsonl")).unwrap();
    assert_eq!(diag.lines().count(), 1);
    assert!(diag.contains("\"path\":\"Cart.tz\""), "{diag}");
    let before = stamps(&kg);
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert!(output.status.success());
    assert_eq!(stamps(&kg), before);
}

#[test]
fn editing_a_source_rebuilds_the_kg() {
    let root = shop("edit");
    assert!(tsuzuri(&root, &["check", "."], &[]).status.success());
    let kg = root.join("target/tz/kg");
    let hash = manifest(&kg)["source_hash"].clone();
    let mut cart = std::fs::read_to_string(root.join("Cart.tz")).unwrap();
    cart.push_str(
        "\n/// The count of lines.\ndef count :: ref [Line] -> i64 = \\lines -> lines.length\n",
    );
    std::fs::write(root.join("Cart.tz"), cart).unwrap();
    assert!(tsuzuri(&root, &["check", "."], &[]).status.success());
    assert_ne!(manifest(&kg)["source_hash"], hash);
    let nodes = std::fs::read_to_string(kg.join("nodes.jsonl")).unwrap();
    assert!(nodes.contains("\"id\":\"Acme::Shop::Cart.count\""));
}

#[test]
fn no_kg_env_and_loose_files() {
    let root = shop("off");
    assert!(
        tsuzuri(&root, &["check", ".", "--no-kg"], &[])
            .status
            .success()
    );
    assert!(
        tsuzuri(&root, &["check", "."], &[("TSUZURI_KG", "0")])
            .status
            .success()
    );
    assert!(!root.join("target").exists());
    let loose = root_with_physics("loose");
    assert!(
        tsuzuri(&loose, &["check", "Physics.tz"], &[])
            .status
            .success()
    );
    assert!(!loose.join("target").exists());
    assert!(
        tsuzuri(&loose, &["check", "Physics.tz", "--kg"], &[])
            .status
            .success()
    );
    assert!(loose.join("target/tz/kg/nodes.jsonl").is_file());
    let loose = root_with_physics("loose-env");
    assert!(
        tsuzuri(&loose, &["check", "Physics.tz"], &[("TSUZURI_KG", "1")])
            .status
            .success()
    );
    assert!(loose.join("target/tz/kg/nodes.jsonl").is_file());
}

fn root_with_physics(label: &str) -> PathBuf {
    root(
        label,
        &[(
            "Physics.tz",
            "def speed :: f64 -> f64 -> f64 = \\d t -> d / t\n",
        )],
    )
}

#[test]
fn kg_flag_conflicts_and_bad_env_are_e2000() {
    let root = shop("conflicts");
    for (arguments, environment, message) in [
        (
            vec!["check", ".", "--kg", "--no-kg"],
            vec![],
            "--kg and --no-kg cannot be used together",
        ),
        (
            vec!["check", "."],
            vec![("TSUZURI_KG", "2")],
            "TSUZURI_KG must be 0 or 1",
        ),
        (
            vec!["check", ".", "--kg-dir", ""],
            vec![],
            "--kg-dir must not be empty",
        ),
        (
            vec!["doc", ".", "-o", "docs", "--kg"],
            vec![],
            "--kg, --no-kg, and --kg-dir are only valid with check, build, run, or test",
        ),
    ] {
        let output = tsuzuri(&root, &arguments, &environment);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        let text = stderr(&output);
        assert!(text.contains("error[E2000]"), "{text}");
        assert!(text.contains(message), "{text}");
    }
    assert!(!root.join("target").exists());
}

#[test]
fn kg_dir_flag_and_env_choose_the_directory() {
    let root = shop("dirs");
    assert!(
        tsuzuri(
            &root,
            &["check", ".", "--kg-dir", "flag"],
            &[("TSUZURI_KG_DIR", "env")]
        )
        .status
        .success()
    );
    assert!(root.join("flag/manifest.json").is_file());
    assert!(!root.join("env").exists());
    assert!(
        tsuzuri(&root, &["check", "."], &[("TSUZURI_KG_DIR", "env")])
            .status
            .success()
    );
    assert!(root.join("env/manifest.json").is_file());
    assert!(!root.join("target").exists());
}

#[test]
fn a_foreign_directory_is_never_overwritten() {
    let root = shop("foreign");
    std::fs::create_dir_all(root.join("target/tz/kg/x")).unwrap();
    std::fs::write(root.join("target/tz/kg/x/keep.txt"), "mine").unwrap();
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert_eq!(output.status.code(), Some(0));
    let text = stderr(&output);
    assert_eq!(text.matches("kg not updated: ").count(), 1, "{text}");
    assert!(
        text.contains("exists and is not a KG (no .tsuzuri-kg marker)"),
        "{text}"
    );
    let output = tsuzuri(&root, &["check", ".", "--json"], &[]);
    let notices: Vec<&str> = std::str::from_utf8(&output.stderr)
        .unwrap()
        .lines()
        .filter(|line| line.contains("kg not updated"))
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].starts_with("{\"severity\":\"warning\",\"code\":\"W2001\""),
        "{}",
        notices[0]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("target/tz/kg/x/keep.txt")).unwrap(),
        "mine"
    );
    assert!(!root.join("target/tz/kg/manifest.json").exists());
}

/// `kg clean` removes only a graph: never a directory that holds the project's sources, even with
/// a marker copied into it.
#[test]
fn clean_never_removes_the_sources() {
    let root = shop("clean-root");
    assert!(tsuzuri(&root, &["kg", "build"], &[]).status.success());
    std::fs::copy(
        root.join("target/tz/kg/.tsuzuri-kg"),
        root.join(".tsuzuri-kg"),
    )
    .unwrap();
    let output = tsuzuri(&root, &["kg", "clean", "--kg-dir", "."], &[]);
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("E2009"), "{text}");
    assert!(text.contains("holds the project's sources"), "{text}");
    assert!(root.join("Main.tz").is_file() && root.join("Tsuzuri.toml").is_file());
    assert!(root.join("target/tz/kg/manifest.json").is_file());
    assert!(tsuzuri(&root, &["kg", "clean"], &[]).status.success());
    assert!(!root.join("target/tz/kg").exists());
    assert!(root.join("Main.tz").is_file());
}

#[test]
fn an_unwritable_target_warns_once() {
    let root = shop("unwritable");
    std::fs::write(root.join("target"), "a file").unwrap();
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stderr(&output).matches("kg not updated: ").count(),
        1,
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_failed_analysis_writes_an_incomplete_kg() {
    let root = root("failed", &[("Main.tz", BROKEN)]);
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert_eq!(output.status.code(), Some(1));
    let kg = root.join("target/tz/kg");
    let manifest = manifest(&kg);
    assert_eq!(manifest["complete"], false);
    assert_eq!(manifest["counts"]["errors"], 1);
    assert_eq!(manifest["modules"], serde_json::json!([]));
    let diag = std::fs::read_to_string(kg.join("diag.jsonl")).unwrap();
    assert!(diag.contains("\"code\":\"E1003\""), "{diag}");
    assert!(!kg.join("nodes.jsonl").exists());
    let before = stamps(&kg);
    assert_eq!(tsuzuri(&root, &["check", "."], &[]).status.code(), Some(1));
    assert_eq!(stamps(&kg), before);
}

/// `kg build` of a graph that is incomplete and unchanged reports it so, and fails again.
#[test]
fn an_unchanged_incomplete_kg_stays_a_failure() {
    let root = root("incomplete-again", &[("Main.tz", BROKEN)]);
    let build = || tsuzuri(&root, &["kg", "build", "--json"], &[]);
    let first = build();
    assert_eq!(first.status.code(), Some(1));
    let second = build();
    assert_eq!(second.status.code(), Some(1), "{}", stderr(&second));
    assert_eq!(
        String::from_utf8_lossy(&second.stdout),
        "{\"type\":\"kg_build\",\"status\":\"incomplete\",\"dir\":\"target/tz/kg\",\"files\":0,\"complete\":false}\n"
    );
    let text = tsuzuri(&root, &["kg", "build"], &[]);
    assert_eq!(text.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&text.stdout),
        "kg: incomplete, 1 errors (target/tz/kg)\n"
    );
}

/// A library without `Main.tz` keeps its graph in its own directory, under its own name, although
/// a dependency's source and manifest sort first.
#[test]
fn a_library_keeps_its_own_directory_and_name() {
    let root = root(
        "library",
        &[
            (
                "zoo/Tsuzuri.toml",
                "[package]\nname = \"zoo\"\nversion = \"1.2.0\"\nedition = \"2027\"\n[dependencies]\nalpha = { path = \"../alpha\" }\n",
            ),
            (
                "zoo/Lib.tz",
                "def twice :: i64 -> i64 = \\n -> Alpha::Util.inc (Alpha::Util.inc n)\n",
            ),
            (
                "alpha/Tsuzuri.toml",
                "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
            ),
            ("alpha/Util.tz", "def inc :: i64 -> i64 = \\n -> n + 1\n"),
        ],
    );
    let output = tsuzuri(&root.join("zoo"), &["kg", "build"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let kg = root.join("zoo/target/tz/kg");
    assert_eq!(manifest(&kg)["project"], "zoo");
    assert_eq!(manifest(&kg)["version"], "1.2.0");
    assert_eq!(manifest(&kg)["edition"], "2027");
    assert!(!root.join("alpha/target").exists());
}

/// Without a manifest the directory's name is the namespace, so renaming the directory makes the
/// graph stale.
#[test]
fn renaming_a_project_without_a_manifest_makes_the_kg_stale() {
    let root = root(
        "rename",
        &[("foo/Main.tz", "def answer :: unit -> i64 = \\() -> 42\n")],
    );
    let output = tsuzuri(&root.join("foo"), &["kg", "build"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let nodes = std::fs::read_to_string(root.join("foo/target/tz/kg/nodes.jsonl")).unwrap();
    assert!(nodes.contains("\"Foo::Main.answer\""), "{nodes}");
    std::fs::rename(root.join("foo"), root.join("bar")).unwrap();
    let output = tsuzuri(&root.join("bar"), &["kg", "verify"], &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("is stale (sources changed)"),
        "{}",
        stderr(&output)
    );
}

/// A reader takes only files that the manifest lists, with their digests: a missing or changed
/// file breaks the graph instead of shrinking it.
#[test]
fn listed_files_must_be_there_and_match() {
    let root = shop("listed");
    assert!(tsuzuri(&root, &["kg", "build"], &[]).status.success());
    let kg = root.join("target/tz/kg");
    let export = |flag: &str| tsuzuri(&root, &["kg", "export", flag, "--no-refresh"], &[]);
    let facts: Vec<String> = manifest(&kg)["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|path| path.starts_with("facts/"))
        .cloned()
        .collect();
    assert!(export("--facts").status.success());
    std::fs::remove_file(kg.join(&facts[0])).unwrap();
    let output = export("--facts");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains(&format!("is broken ({} cannot be read)", facts[0])),
        "{}",
        stderr(&output)
    );
    assert!(
        tsuzuri(&root, &["kg", "build", "--force"], &[])
            .status
            .success()
    );
    let edges = std::fs::read(kg.join("edges.jsonl")).unwrap();
    std::fs::write(kg.join("edges.jsonl"), &edges[1..]).unwrap();
    let output = export("--edges");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("is broken (edges.jsonl does not match manifest.json)"),
        "{}",
        stderr(&output)
    );
    // `--edges` reads `nodes.jsonl` too, through the graph it opens.
    std::fs::write(kg.join("edges.jsonl"), &edges).unwrap();
    std::fs::remove_file(kg.join("nodes.jsonl")).unwrap();
    let output = export("--edges");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("is broken (nodes.jsonl cannot be read)"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn the_kg_is_identical_in_two_directories_and_thread_counts() {
    let first = shop("same-1");
    let second = shop("same-2");
    assert!(
        tsuzuri(&first, &["check", "."], &[("TSUZURI_THREADS", "1")])
            .status
            .success()
    );
    assert!(
        tsuzuri(&second, &["check", "."], &[("TSUZURI_THREADS", "8")])
            .status
            .success()
    );
    let a = files(&first.join("target/tz/kg"));
    let b = files(&second.join("target/tz/kg"));
    assert_eq!(a, b);
    let stamp = std::fs::read_to_string(first.join("target/tz/kg/.tsuzuri-kg")).unwrap();
    let stamp = stamp.lines().nth(1).unwrap().trim_start_matches("stamp ");
    for (path, bytes) in &a {
        let text = String::from_utf8_lossy(bytes);
        for leak in [first.to_string_lossy().as_ref(), stamp] {
            assert!(!text.contains(leak), "{path} holds '{leak}'");
        }
    }
}

#[test]
fn concurrent_checks_leave_a_valid_kg() {
    let root = shop("concurrent");
    let mut children: Vec<_> = (0..8)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
                .current_dir(&root)
                .args(["check", "."])
                .env_remove("TSUZURI_KG")
                .env_remove("TSUZURI_KG_DIR")
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let kg = root.join("target/tz/kg");
    assert_eq!(manifest(&kg)["complete"], true);
    let output = tsuzuri(&root, &["kg", "verify", "."], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let leftovers: Vec<_> = std::fs::read_dir(root.join("target/tz"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".tsuzuri-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_output_of_commands_does_not_depend_on_the_kg() {
    let with = shop("output-with");
    let without = shop("output-without");
    for arguments in [vec!["check", "."], vec!["test", "."]] {
        let mut no_kg = arguments.clone();
        no_kg.push("--no-kg");
        let a = tsuzuri(&with, &arguments, &[]);
        let b = tsuzuri(&without, &no_kg, &[]);
        assert_eq!(a.status.code(), b.status.code(), "{arguments:?}");
        // The compiler names the sources of a project by their canonical path, which differs
        // from the temporary root (`/var` and `/private/var` on macOS; a short name and `\\?\` on
        // Windows).
        let normalize = |output: &Output, root: &Path| {
            let canonical = std::fs::canonicalize(root).unwrap();
            let canonical = canonical.to_string_lossy();
            let plain = canonical.strip_prefix(r"\\?\").unwrap_or(&canonical);
            (String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr))
                .replace(canonical.as_ref(), "<root>")
                .replace(plain, "<root>")
                .replace(root.to_string_lossy().as_ref(), "<root>")
        };
        assert_eq!(
            normalize(&a, &with),
            normalize(&b, &without),
            "{arguments:?}"
        );
    }
    let failing = root("output-fail", &[("Main.tz", BROKEN)]);
    let a = tsuzuri(&failing, &["check", "."], &[]);
    let b = tsuzuri(&failing, &["check", ".", "--no-kg"], &[]);
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stderr, b.stderr);
}

#[test]
fn kg_dir_is_skipped_by_source_discovery() {
    let root = shop("discovery");
    assert!(tsuzuri(&root, &["check", "."], &[]).status.success());
    std::fs::write(
        root.join("target/tz/kg/Broken.tz"),
        "def broken :: i64 = (\n",
    )
    .unwrap();
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    std::fs::remove_file(root.join("target/tz/kg/.tsuzuri-kg")).unwrap();
    let output = tsuzuri(&root, &["check", "."], &[]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("error[E0002]"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn test_run_and_build_create_the_kg() {
    let root = root(
        "commands",
        &[(
            "Main.tz",
            "def double :: i64 -> i64 = \\n -> n * 2\n\ntest \"doubles\" = assert (double 2 == 4)\n\ndouble 21\n",
        )],
    );
    let kg = root.join("target/tz/kg");
    let output = tsuzuri(&root, &["test", "."], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(kg.join("tests.jsonl").is_file());
    std::fs::remove_dir_all(&kg).unwrap();
    let output = tsuzuri(
        &root,
        &["build", ".", "--emit", "llvm", "-o", "out.ll"],
        &[],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(kg.join("nodes.jsonl").is_file());
    std::fs::remove_dir_all(&kg).unwrap();
    let output = tsuzuri(&root, &["run", "."], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(kg.join("nodes.jsonl").is_file());
}
