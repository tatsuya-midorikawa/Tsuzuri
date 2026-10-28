//! G28 Phase 3 and 4 (D-60): `tsuzuri kg impact` and `tsuzuri test --affected`. The expected
//! output of the main scenario comes from the reference implementation of G28 appendix D.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fresh(label: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-kg-impact-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// Copies the fixture's sources into `directory`.
fn copy(fixture: &str, directory: &Path) {
    std::fs::create_dir_all(directory).unwrap();
    for entry in std::fs::read_dir(repository().join("tests/fixtures").join(fixture)).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, directory.join(path.file_name().unwrap())).unwrap();
        }
    }
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

/// A root with `base` (kg_shop) and `head` (kg_shop_head), both with their graphs.
fn scene(label: &str) -> PathBuf {
    let root = fresh(label);
    copy("kg_shop", &root.join("base"));
    copy("kg_shop_head", &root.join("head"));
    for project in ["base", "head"] {
        assert!(tsuzuri(&root, &["kg", "build", project]).status.success());
    }
    root
}

#[test]
fn impact_between_two_projects_matches_the_reference() {
    let root = scene("reference");
    let output = tsuzuri(&root, &["kg", "impact", "--from", "base", "--path", "head"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        std::fs::read_to_string(repository().join("tests/golden/kg/queries/impact.txt")).unwrap()
    );
    let output = tsuzuri(
        &root,
        &["kg", "impact", "--from", "base", "--to", "head", "--json"],
    );
    let rows: Vec<serde_json::Value> = stdout(&output)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows[0],
        serde_json::json!({"type":"impact","base":"base","head":"head"})
    );
    assert_eq!(
        rows[1],
        serde_json::json!({"type":"change","severity":"breaking","action":"added","kind":"field","name":"Acme::Shop::Cart.Line.tax","old":null,"new":"tax: i64","note":"constructions of the record without it stop compiling"})
    );
    let tests = &rows[rows.len() - 2];
    assert_eq!(tests["mode"], "some");
    assert_eq!(tests["total"], 3);
    assert_eq!(tests["paths"].as_array().unwrap().len(), 3);
    assert_eq!(
        rows.last().unwrap(),
        &serde_json::json!({"type":"summary","breaking":4,"non_breaking":1,"info":1,"omitted":0})
    );
}

#[test]
fn public_changes_equal_api_diff() {
    let root = scene("api-diff");
    let impact = stdout(&tsuzuri(
        &root,
        &["kg", "impact", "--from", "base", "--to", "head"],
    ));
    let diff = stdout(&tsuzuri(&root, &["api-diff", "base", "head"]));
    let api: Vec<&str> = diff
        .lines()
        .filter(|line| line.starts_with("breaking: ") || line.starts_with("non-breaking: "))
        .collect();
    assert_eq!(api.len(), 3, "{diff}");
    for line in &api {
        assert!(
            impact.lines().any(|other| other == *line),
            "{line}\n{impact}"
        );
    }
    // What only impact reports is a fact, a private change or a warning.
    for line in impact
        .lines()
        .filter(|line| line.starts_with("breaking: ") || line.starts_with("non-breaking: "))
    {
        assert!(api.contains(&line) || line.contains(" effect "), "{line}");
    }
}

fn touched(label: &str, file: &str) -> (PathBuf, String) {
    let root = scene(label);
    let path = root.join("work");
    copy("kg_shop", &path);
    let mut text = std::fs::read_to_string(path.join(file)).unwrap();
    text.push_str("\n// touched\n");
    std::fs::write(path.join(file), text).unwrap();
    let output = tsuzuri(&root, &["kg", "impact", "--from", "base", "--path", "work"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    (root, stdout(&output))
}

#[test]
fn affected_tests_follow_the_changed_module() {
    let (_, text) = touched("main", "Main.tz");
    assert_eq!(
        text,
        "impact: base -> working tree\ntests: 1 of 3 affected\n  test:Acme::Shop::Main.main runs\nsummary: 0 breaking, 0 non-breaking, 0 info\n"
    );
    let (_, text) = touched("cart", "Cart.tz");
    assert!(text.contains("tests: 3 of 3 affected\n"), "{text}");
    let (_, text) = touched("report", "Report.tz");
    assert_eq!(
        text,
        "impact: base -> working tree\ntests: 0 of 3 affected\nsummary: 0 breaking, 0 non-breaking, 0 info\n"
    );
}

#[test]
fn another_compiler_or_no_edges_runs_every_test() {
    let root = scene("compiler");
    let other = root.join("other");
    copy_tree(&root.join("head/target/tz/kg"), &other);
    let manifest = std::fs::read_to_string(other.join("manifest.json")).unwrap();
    std::fs::write(
        other.join("manifest.json"),
        manifest.replace("\"tsuzuri 0.1.0\"", "\"tsuzuri 0.2.0\""),
    )
    .unwrap();
    let output = tsuzuri(
        &root,
        &[
            "kg",
            "impact",
            "--from",
            "base/target/tz/kg",
            "--to",
            "other",
        ],
    );
    let text = stdout(&output);
    assert!(
        text.contains("info: skipped facts (the two KGs were built by different compilers or one has no facts)\n"),
        "{text}"
    );
    assert!(
        text.contains("tests: all 3 (the compiler changed)\n"),
        "{text}"
    );
    let bare = root.join("bare");
    copy_tree(&root.join("head/target/tz/kg"), &bare);
    without(&bare, "edges.jsonl");
    let output = tsuzuri(
        &root,
        &[
            "kg",
            "impact",
            "--from",
            "base/target/tz/kg",
            "--to",
            "bare",
        ],
    );
    assert!(
        stdout(&output).contains("tests: all 3 (the KG has no graph)\n"),
        "{}",
        stdout(&output)
    );
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

#[test]
fn many_changes_are_cut_at_the_listing_limit() {
    let root = scene("many");
    let mut cart = std::fs::read_to_string(root.join("head/Cart.tz")).unwrap();
    for index in 0..1003 {
        cart.push_str(&format!(
            "\ndef extra{index:04} :: i64 -> i64 = \\n -> n + {index}\n"
        ));
    }
    std::fs::write(root.join("head/Cart.tz"), cart).unwrap();
    let output = tsuzuri(&root, &["kg", "impact", "--from", "base", "--path", "head"]);
    let text = stdout(&output);
    assert!(
        text.contains("\nnote: 9 more changes not shown\n"),
        "{}",
        &text[text.len().saturating_sub(600)..]
    );
    let output = tsuzuri(
        &root,
        &["kg", "impact", "--from", "base", "--path", "head", "--json"],
    );
    let last: serde_json::Value =
        serde_json::from_str(stdout(&output).lines().last().unwrap()).unwrap();
    assert_eq!(last["omitted"], 9);
}

fn git(cwd: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(arguments)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        stderr(&output)
    );
}

#[test]
fn impact_against_a_git_revision() {
    let root = fresh("git");
    let project = root.join("repo/shop");
    copy("kg_shop", &project);
    git(&root.join("repo"), &["init", "-q"]);
    git(&root.join("repo"), &["add", "."]);
    git(&root.join("repo"), &["commit", "-q", "-m", "base"]);
    for entry in std::fs::read_dir(repository().join("tests/fixtures/kg_shop_head")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, project.join(path.file_name().unwrap())).unwrap();
    }
    let output = tsuzuri(&project, &["kg", "impact", "--git", "HEAD"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    let first = text.lines().next().unwrap();
    assert!(
        first.starts_with("impact: git ") && first.ends_with(" -> working tree"),
        "{first}"
    );
    assert!(
        text.contains("breaking: added field Acme::Shop::Cart.Line.tax"),
        "{text}"
    );
    let worktrees = Command::new("git")
        .current_dir(&project)
        .args(["worktree", "list"])
        .output()
        .unwrap();
    assert_eq!(
        stdout(&worktrees).lines().count(),
        1,
        "{}",
        stdout(&worktrees)
    );
    for (revision, code, message) in [
        ("-x", 2, "invalid git revision '-x'"),
        ("no-such-branch", 1, "unknown git revision 'no-such-branch'"),
    ] {
        let output = tsuzuri(&project, &["kg", "impact", "--git", revision]);
        assert_eq!(output.status.code(), Some(code), "{revision}");
        assert!(stderr(&output).contains(message), "{}", stderr(&output));
    }
}

/// The head's graph may be where the base's is: the base is read in full before the head's graph
/// replaces it (`--from` the project's own graph, and `--git` or `test --affected` with
/// `TSUZURI_KG_DIR`, which names only the working tree's graph).
#[test]
fn impact_against_the_graph_that_the_head_replaces() {
    let root = fresh("same-dir");
    let project = root.join("repo/shop");
    copy("kg_shop", &project);
    assert!(tsuzuri(&project, &["kg", "build"]).status.success());
    git(&root.join("repo"), &["init", "-q"]);
    std::fs::write(root.join("repo/.gitignore"), "target/\n").unwrap();
    git(&root.join("repo"), &["add", "."]);
    git(&root.join("repo"), &["commit", "-q", "-m", "base"]);
    for entry in std::fs::read_dir(repository().join("tests/fixtures/kg_shop_head")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, project.join(path.file_name().unwrap())).unwrap();
    }
    let effect = "breaking: added effect io on Acme::Shop::Cart.line_total (reached by 4 functions and 3 tests)\n";
    let shared = root.join("shared");
    let with_shared = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .current_dir(&project)
            .args(arguments)
            .env_remove("TSUZURI_KG")
            .env("TSUZURI_KG_DIR", &shared)
            .output()
            .unwrap()
    };
    let output = with_shared(&["kg", "impact", "--git", "HEAD"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains(effect), "{}", stdout(&output));
    let output = with_shared(&["test", ".", "--affected", "HEAD"]);
    assert!(
        stdout(&output).contains("affected: 3 of 3 tests (HEAD)"),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    // The graph that the head refreshes is the base.
    let output = tsuzuri(&project, &["kg", "impact", "--from", "target/tz/kg"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("impact: target/tz/kg -> working tree\n"),
        "{text}"
    );
    assert!(text.contains(effect), "{text}");
}

#[test]
fn test_affected_runs_only_the_reached_tests() {
    let root = scene("affected");
    let edited = root.join("edited");
    copy("kg_shop", &edited);
    let main = std::fs::read_to_string(edited.join("Main.tz")).unwrap();
    std::fs::write(edited.join("Main.tz"), main.replace("== 250)", "== 251)")).unwrap();
    let output = tsuzuri(&root, &["test", "edited", "--affected", "base"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.starts_with("affected: 1 of 3 tests (base)\n"),
        "{text}"
    );
    assert!(text.contains("not ok 3 - Main main runs"), "{text}");
    let output = tsuzuri(&root, &["test", "edited", "--affected", "base", "--json"]);
    assert_eq!(
        stdout(&output).lines().next().unwrap(),
        "{\"type\":\"affected\",\"mode\":\"some\",\"affected\":1,\"total\":3,\"base\":\"base\",\"reason\":null}"
    );
    let report = root.join("report");
    copy("kg_shop", &report);
    let text = std::fs::read_to_string(report.join("Report.tz")).unwrap();
    std::fs::write(
        report.join("Report.tz"),
        text.replace("Cart.total lines)", "Cart.total lines + 0)"),
    )
    .unwrap();
    let output = tsuzuri(&root, &["test", "report", "--affected", "base"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "affected: 0 of 3 tests (base)\n");
    for (arguments, message) in [
        (
            vec!["check", "edited", "--affected", "base"],
            "--affected is only valid with test",
        ),
        (
            vec!["test", "edited", "--affected", "base", "--index", "0"],
            "--affected cannot be combined with --filter or --index",
        ),
        (
            vec!["test", "edited", "--affected", "base", "--no-kg"],
            "--affected needs the KG; remove --no-kg or TSUZURI_KG=0",
        ),
    ] {
        let output = tsuzuri(&root, &arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(stderr(&output).contains(message), "{}", stderr(&output));
    }
}

/// A repository at `root/repo` whose first commit holds `kg_shop` in `shop/`.
fn repository_with_shop(label: &str) -> (PathBuf, PathBuf) {
    let root = fresh(label);
    let repo = root.join("repo");
    copy("kg_shop", &repo.join("shop"));
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    (root, repo)
}

fn git_output(cwd: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(arguments)
        .output()
        .unwrap();
    stdout(&output)
}

/// `kg impact --git` with TMPDIR (TEMP on Windows) in `temporary`.
fn impact_git(cwd: &Path, temporary: &Path, arguments: &[&str]) -> Output {
    std::fs::create_dir_all(temporary).unwrap();
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .current_dir(cwd)
        .args(arguments)
        .env_remove("TSUZURI_KG")
        .env_remove("TSUZURI_KG_DIR")
        .env("TMPDIR", temporary)
        .env("TEMP", temporary)
        .env("TMP", temporary)
        .output()
        .unwrap()
}

fn leftovers(temporary: &Path) -> Vec<String> {
    std::fs::read_dir(temporary)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".tsuzuri-"))
        .collect()
}

#[test]
fn worktree_is_removed_on_success_and_on_failure() {
    let (root, repo) = repository_with_shop("worktree");
    let project = repo.join("shop");
    let temporary = root.join("tmp");
    for entry in std::fs::read_dir(repository().join("tests/fixtures/kg_shop_head")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, project.join(path.file_name().unwrap())).unwrap();
    }
    let output = impact_git(&project, &temporary, &["kg", "impact", "--git", "HEAD"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(git_output(&repo, &["worktree", "list"]).lines().count(), 1);
    assert!(
        leftovers(&temporary).is_empty(),
        "{:?}",
        leftovers(&temporary)
    );
    // A head that does not type-check fails after the checkout, which is removed all the same.
    let mut cart = std::fs::read_to_string(project.join("Cart.tz")).unwrap();
    cart.push_str("\ndef broken :: i64 = \"text\"\n");
    std::fs::write(project.join("Cart.tz"), cart).unwrap();
    let output = impact_git(&project, &temporary, &["kg", "impact", "--git", "HEAD"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(git_output(&repo, &["worktree", "list"]).lines().count(), 1);
    assert!(
        leftovers(&temporary).is_empty(),
        "{:?}",
        leftovers(&temporary)
    );
}

#[test]
fn git_status_is_unchanged() {
    let (root, repo) = repository_with_shop("status");
    let project = repo.join("shop");
    std::fs::write(project.join("notes.txt"), "untracked\n").unwrap();
    let main = std::fs::read_to_string(project.join("Main.tz")).unwrap();
    std::fs::write(project.join("Main.tz"), main.replace("== 250)", "== 251)")).unwrap();
    git(&repo, &["branch", "side"]);
    let state = || {
        [
            vec!["status", "--porcelain=v1", "-uall"],
            vec!["stash", "list"],
            vec!["branch", "--list"],
            vec!["rev-parse", "HEAD"],
            vec!["for-each-ref"],
            vec!["worktree", "list", "--porcelain"],
        ]
        .iter()
        .map(|arguments| git_output(&repo, arguments))
        .collect::<Vec<_>>()
    };
    // impact writes the head's graph into the project (untracked files): build it before the
    // snapshot, so that the comparison sees only what impact itself does.
    assert!(tsuzuri(&project, &["kg", "build"]).status.success());
    let before = state();
    let output = impact_git(
        &project,
        &root.join("tmp"),
        &["kg", "impact", "--git", "HEAD"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(state(), before);
}

#[cfg(unix)]
#[test]
fn hooks_do_not_run() {
    use std::os::unix::fs::PermissionsExt;
    let (root, repo) = repository_with_shop("hooks");
    let marker = root.join("marker");
    let hook = repo.join(".git/hooks/post-checkout");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        format!("#!/bin/sh\necho ran > '{}'\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = impact_git(
        &repo.join("shop"),
        &root.join("tmp"),
        &["kg", "impact", "--git", "HEAD"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!marker.exists());
}

#[test]
fn unknown_revision_and_not_a_repository() {
    let (root, repo) = repository_with_shop("unknown");
    let output = impact_git(
        &repo.join("shop"),
        &root.join("tmp"),
        &["kg", "impact", "--git", "nope"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2000]: unknown git revision 'nope'"),
        "{}",
        stderr(&output)
    );
    let outside = fresh("outside");
    copy("kg_shop", &outside.join("shop"));
    let output = impact_git(
        &outside.join("shop"),
        &outside.join("tmp"),
        &["kg", "impact", "--git", "HEAD"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E2001]"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("is not inside a git repository"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn project_directory_missing_at_revision() {
    let root = fresh("missing");
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("README"), "x\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    copy("kg_shop", &repo.join("app"));
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "app"]);
    let sha = git_output(&repo, &["rev-parse", "HEAD~1"]);
    let output = impact_git(
        &repo,
        &root.join("tmp"),
        &["kg", "impact", "--git", "HEAD~1", "--path", "app"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains(&format!(
            "the project directory 'app' does not exist at git revision 'HEAD~1' ({})",
            &sha[..12]
        )),
        "{}",
        stderr(&output)
    );
    assert_eq!(git_output(&repo, &["worktree", "list"]).lines().count(), 1);
}

/// The test numbers in the output of `tsuzuri test` (failed) or `--list` (listed).
fn numbers(text: &str, failed: bool) -> std::collections::BTreeSet<usize> {
    text.lines()
        .filter_map(|line| {
            if failed {
                let rest = line.strip_prefix("not ok ")?;
                rest.split(' ').next()?.parse::<usize>().ok().map(|n| n - 1)
            } else {
                line.split(' ').next()?.parse::<usize>().ok()
            }
        })
        .collect()
}

#[test]
fn affected_tests_cover_every_failing_test() {
    let root = fresh("differential");
    copy("kg_shop", &root.join("shop"));
    // G28 appendix C-3 and F-6: one edit each, and the number of tests it selects.
    let edits = [
        (
            "E1",
            "Cart.tz",
            "line.quantity * line.unit_price",
            "line.quantity + line.unit_price",
            3,
        ),
        ("E2", "Main.tz", "== 250", "== 251", 1),
        ("E3", "Cart.tz", "let mut sum = 0", "let mut sum = 1", 3),
        (
            "E4",
            "Report.tz",
            "(Cart.total lines)",
            "(Cart.total lines + 0)",
            0,
        ),
        ("E5", "Main.tz", "    0", "    1", 1),
    ];
    for (name, file, from, to, size) in edits {
        let copy_dir = root.join(name);
        copy("kg_shop", &copy_dir);
        let text = std::fs::read_to_string(copy_dir.join(file)).unwrap();
        assert!(text.contains(from), "{name}");
        std::fs::write(copy_dir.join(file), text.replacen(from, to, 1)).unwrap();
        let failed = numbers(&stdout(&tsuzuri(&root, &["test", name])), true);
        let listed = tsuzuri(&root, &["test", name, "--affected", "shop", "--list"]);
        assert_eq!(listed.status.code(), Some(0), "{name}: {}", stderr(&listed));
        let selected = numbers(&stdout(&listed), false);
        assert!(
            failed.is_subset(&selected),
            "{name}: {failed:?} {selected:?}"
        );
        assert_eq!(selected.len(), size, "{name}: {}", stdout(&listed));
        assert!(
            stdout(&listed).starts_with(&format!("affected: {size} of 3 tests (shop)\n")),
            "{name}"
        );
    }
}

#[test]
fn affected_with_no_changes_runs_nothing() {
    let root = fresh("unchanged");
    copy("kg_shop", &root.join("shop"));
    let output = tsuzuri(&root, &["test", "shop", "--affected", "shop"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "affected: 0 of 3 tests (shop)\n");
}

#[test]
fn affected_all_when_the_manifest_changes() {
    let root = fresh("manifest");
    copy("kg_shop", &root.join("shop"));
    copy("kg_shop", &root.join("bumped"));
    let manifest = std::fs::read_to_string(root.join("bumped/Tsuzuri.toml")).unwrap();
    std::fs::write(
        root.join("bumped/Tsuzuri.toml"),
        manifest.replace("version = \"0.1.0\"", "version = \"0.2.0\""),
    )
    .unwrap();
    let output = tsuzuri(&root, &["test", "bumped", "--affected", "shop", "--list"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "affected: all 3 tests (the manifest or a dependency changed)\n0 Cart.line total\n1 Cart.empty cart\n2 Main.main runs\n"
    );
}

#[test]
fn affected_value_is_a_directory_a_kg_or_a_revision() {
    let (_, repo) = repository_with_shop("values");
    // The base commit, then a commit that leaves the project as it is.
    std::fs::write(repo.join("README"), "x\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "readme"]);
    git(&repo, &["branch", "base", "HEAD~1"]);
    copy("kg_shop", &repo.join("base"));
    assert!(tsuzuri(&repo, &["kg", "build", "base"]).status.success());
    let main = std::fs::read_to_string(repo.join("shop/Main.tz")).unwrap();
    std::fs::write(
        repo.join("shop/Main.tz"),
        main.replace("== 250)", "== 251)"),
    )
    .unwrap();
    for value in [
        "base/target/tz/kg",
        "base",
        "./base",
        "HEAD~1",
        "refs/heads/base",
    ] {
        let output = tsuzuri(&repo, &["test", "shop", "--affected", value, "--list"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{value}: {}",
            stderr(&output)
        );
        assert_eq!(
            stdout(&output),
            format!("affected: 1 of 3 tests ({value})\n2 Main.main runs\n"),
            "{value}"
        );
    }
}
