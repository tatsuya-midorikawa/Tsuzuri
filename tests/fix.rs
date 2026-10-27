use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::check::ModuleOrigin;
use tsuzuri::diagnostic::{
    Applicability, Diagnostic, DiagnosticKind, DiagnosticSet, Edit, Fix, Span,
};
use tsuzuri::driver::{FileUpdate, Project, ReplaceFailure, replace_files};
use tsuzuri::fix::{self, Analysis};

/// A fresh root with the given files, each at a path below it.
fn root(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-fix-{label}-{}-{}",
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

/// `(status, stdout, stderr)` of the compiler with `arguments`.
fn tsuzuri(arguments: &[&str], cwd: Option<&Path>) -> (i32, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command.args(arguments);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn replace_files_replaces_every_file() {
    let root = root("replace", &[("A.tz", "a"), ("B.tz", "b")]);
    let update = |name: &str, original: &str, replacement: &str| FileUpdate {
        path: root.join(name),
        original: original.to_owned(),
        replacement: replacement.to_owned(),
    };
    replace_files(&[update("A.tz", "a", "A"), update("B.tz", "b", "B")]).unwrap();
    assert_eq!(read(&root.join("A.tz")), "A");
    assert_eq!(read(&root.join("B.tz")), "B");
}

#[test]
fn replace_files_restores_earlier_files_when_a_later_one_changed() {
    let root = root("restore", &[("A.tz", "a"), ("B.tz", "changed")]);
    let updates = [
        FileUpdate {
            path: root.join("A.tz"),
            original: "a".to_owned(),
            replacement: "A".to_owned(),
        },
        FileUpdate {
            path: root.join("B.tz"),
            original: "b".to_owned(),
            replacement: "B".to_owned(),
        },
    ];
    let error = replace_files(&updates).unwrap_err();
    assert_eq!(
        error.failure,
        ReplaceFailure::Changed {
            path: root.join("B.tz")
        }
    );
    assert_eq!(error.restored, 1);
    assert!(error.unrestored.is_empty());
    assert_eq!(read(&root.join("A.tz")), "a");
    assert_eq!(read(&root.join("B.tz")), "changed");
}

#[cfg(unix)]
#[test]
fn replace_files_refuses_symlinks() {
    let root = root("symlink", &[("A.tz", "a"), ("Real.tz", "b")]);
    std::os::unix::fs::symlink(root.join("Real.tz"), root.join("Link.tz")).unwrap();
    let updates = [
        FileUpdate {
            path: root.join("A.tz"),
            original: "a".to_owned(),
            replacement: "A".to_owned(),
        },
        FileUpdate {
            path: root.join("Link.tz"),
            original: "b".to_owned(),
            replacement: "B".to_owned(),
        },
    ];
    let error = replace_files(&updates).unwrap_err();
    assert!(matches!(error.failure, ReplaceFailure::Failed { .. }));
    assert_eq!(read(&root.join("A.tz")), "a");
    assert_eq!(read(&root.join("Real.tz")), "b");
}

#[cfg(unix)]
#[test]
fn replace_files_keeps_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let root = root("mode", &[("A.tz", "a")]);
    let path = root.join("A.tz");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    replace_files(&[FileUpdate {
        path: path.clone(),
        original: "a".to_owned(),
        replacement: "A".to_owned(),
    }])
    .unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn usage_errors_exit_2() {
    for arguments in [
        &["fix"][..],
        &["fix", "a", "b"],
        &["fix", "a", "--x"],
        &["fix", "a", "--applicability", "maybe"],
    ] {
        let (status, stdout, stderr) = tsuzuri(arguments, None);
        assert_eq!(status, 2, "{arguments:?}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("error[E2000]"), "{arguments:?}: {stderr}");
        let mut json = arguments.to_vec();
        json.push("--json");
        let (status, _, stderr) = tsuzuri(&json, None);
        assert_eq!(status, 2, "{json:?}");
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        let line: serde_json::Value = serde_json::from_str(&stderr).unwrap();
        assert_eq!(line["code"], "E2000");
    }
}

/// A program whose only diagnostic, an unreachable arm (`W1003`), has no fix.
const UNREACHABLE: &str =
    "def f :: i64 -> i64 = \\n ->\n    match n with\n    | _ -> 1\n    | 0 -> 2\n\nf 1\n";

#[test]
fn no_fixes_prints_a_note_and_changes_nothing() {
    let root = root("none", &[("Main.tz", UNREACHABLE)]);
    let (status, stdout, stderr) = tsuzuri(&["fix", "."], Some(&root));
    assert_eq!((status, stdout.as_str()), (0, "no fixes to apply\n"));
    assert!(stderr.contains("warning[W1003]"), "{stderr}");
    assert_eq!(read(&root.join("Main.tz")), UNREACHABLE);
}

#[test]
fn a_directory_without_main_fails_like_check() {
    let root = root("lib", &[("Util.tz", "def one :: i64 = 1\n")]);
    let (status, stdout, stderr) = tsuzuri(&["fix", "."], Some(&root));
    assert_eq!(status, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("error[E2001]"), "{stderr}");
}

const HELPER_MANIFEST: &str = "[package]\nname = \"helper\"\nversion = \"0.1.0\"\n";
const APP_MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nhelper = { path = \"../helper\" }\n";
const LEGACY: &str = "/// @deprecated use Helper::Legacy.next instead\ndef old :: i64 -> i64 = \\n -> n + 1\n\ndef next :: i64 -> i64 = \\n -> n + 1\n";
const APP: &str = "using Helper\n\nlet a = Legacy.old 1\na\n";

fn deprecation_root(legacy: &str) -> PathBuf {
    root(
        "deprecated",
        &[
            ("helper/Tsuzuri.toml", HELPER_MANIFEST),
            ("helper/Legacy.tz", legacy),
            ("app/Tsuzuri.toml", APP_MANIFEST),
            ("app/Main.tz", APP),
        ],
    )
}

fn arguments(input: &Path, extra: &[&str]) -> fix::Arguments {
    let mut words = vec![OsString::from(input)];
    words.extend(extra.iter().map(OsString::from));
    fix::parse_arguments(&words).unwrap()
}

/// A fake analysis: a `W1001` with a fix at the start of each source of `project` that `pick`
/// selects, which replaces the first byte with `#`.
fn fake(project: &Project, pick: &dyn Fn(usize, &tsuzuri::driver::SourceFile) -> bool) -> Analysis {
    let mut warnings = Vec::new();
    for (index, source) in project.sources.iter().enumerate() {
        if !pick(index, source) || source.text.starts_with('#') || source.text.is_empty() {
            continue;
        }
        let span = Span::new(0, 1).in_source(index);
        let mut diagnostic = Diagnostic::of(DiagnosticKind::UnusedLocal, "m", span);
        diagnostic.fixes.push(Fix {
            title: "Replace with '#'".to_owned(),
            applicability: Applicability::Certain,
            edits: vec![Edit {
                span,
                text: "#".to_owned(),
            }],
        });
        warnings.push(diagnostic);
    }
    Analysis {
        errors: DiagnosticSet::from_diagnostics([], 0),
        warnings,
    }
}

#[test]
fn dependencies_and_std_are_never_candidates() {
    let root = deprecation_root(LEGACY);
    let mut project = Project::load(&root.join("app")).unwrap();
    let main = project.root;
    let execution = fix::execute_with(
        &mut project,
        &arguments(&root.join("app"), &["--json"]),
        &mut |project, _| {
            fake(project, &|index, source| {
                index != main && (source.origin == ModuleOrigin::Std || source.package.is_some())
            })
        },
    )
    .unwrap();
    let rows: Vec<serde_json::Value> = execution
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let skipped: Vec<_> = rows
        .iter()
        .filter(|row| row["event"] == "fix")
        .map(|row| (row["status"].clone(), row["reason"].clone()))
        .collect();
    assert!(!skipped.is_empty());
    assert!(
        skipped
            .iter()
            .all(|row| row.0 == "skipped" && row.1 == "outside_project"),
        "{skipped:?}"
    );
    assert_eq!(read(&root.join("helper/Legacy.tz")), LEGACY);
    assert_eq!(read(&root.join("app/Main.tz")), APP);
}

#[test]
fn run_with_a_fake_analyzer_writes_and_reports_on_a_real_project() {
    let root = root("fake", &[("Main.tz", "let x = 40\nx + 2\n")]);
    let mut project = Project::load(&root).unwrap();
    let main = project.root;
    let execution = fix::execute_with(&mut project, &arguments(&root, &[]), &mut |project, _| {
        fake(project, &|index, _| index == main)
    })
    .unwrap();
    assert_eq!(
        execution.stdout,
        "fixed Main.tz: 1 fix\napplied 1 fix in 1 file\n"
    );
    assert_eq!(execution.exit, 0);
    assert_eq!(read(&root.join("Main.tz")), "#et x = 40\nx + 2\n");
}

/// The program of appendix C1 of G24: an unused parameter `b` and an unused local `unused`.
const F1: &str =
    "def pair :: i64 -> i64 -> i64 = \\a b -> a\n\nlet unused = 5\nlet x = 40\npair x 2\n";
const F1_FIXED: &str =
    "def pair :: i64 -> i64 -> i64 = \\a _b -> a\n\nlet _unused = 5\nlet x = 40\npair x 2\n";

#[test]
fn w1001_carries_a_certain_prefix_fix() {
    let module = tsuzuri::analyze_modules_all(&[("Main", F1)]).unwrap();
    let fixes: Vec<_> = module
        .warnings
        .iter()
        .map(|warning| {
            assert_eq!(warning.code, "W1001");
            assert_eq!(warning.fixes.len(), 1);
            let fix = &warning.fixes[0];
            assert_eq!(fix.applicability, Applicability::Certain);
            assert_eq!(fix.edits.len(), 1);
            assert_eq!(fix.edits[0].span, warning.span);
            (fix.title.clone(), fix.edits[0].text.clone())
        })
        .collect();
    assert_eq!(
        fixes,
        [
            ("Prefix 'b' with '_'".to_owned(), "_b".to_owned()),
            ("Prefix 'unused' with '_'".to_owned(), "_unused".to_owned()),
        ]
    );
}

#[test]
fn w1001_has_no_fix_when_the_prefixed_name_is_taken() {
    for source in [
        "def f :: i64 -> i64 = \\n ->\n    let _unused = 1\n    let unused = 5\n    n + _unused\n\nf 1\n",
        "def _unused :: i64 = 1\n\nlet unused = 5\n_unused\n",
    ] {
        let module = tsuzuri::analyze_modules_all(&[("Main", source)]).unwrap();
        let warnings: Vec<_> = module
            .warnings
            .iter()
            .filter(|warning| warning.code == "W1001")
            .collect();
        assert_eq!(warnings.len(), 1, "{source}\n{:#?}", module.warnings);
        assert!(warnings[0].fixes.is_empty(), "{source}");
    }
}

#[test]
fn w1001_has_no_fix_when_one_rename_cannot_reach_every_occurrence() {
    // The binders of an or-pattern, and a `let rec` function that only its own body mentions.
    let source = "union U = A of i64 | B of i64\n\ndef f :: U -> i64 = \\u ->\n    match u with\n    | A n | B n -> 1\n\ndef g :: i64 -> i64 = \\x ->\n    let rec loop = \\n -> if n == 0 then 0 else loop (n - 1)\n    x\n\nf (A 1) + g 2\n";
    let module = tsuzuri::analyze_modules_all(&[("Main", source)]).unwrap();
    let names: Vec<_> = module
        .warnings
        .iter()
        .map(|warning| (warning.message.clone(), warning.fixes.len()))
        .collect();
    assert_eq!(
        names,
        [
            (
                "unused local 'n'; prefix it with '_' to silence this warning".to_owned(),
                0
            ),
            (
                "unused local 'loop'; prefix it with '_' to silence this warning".to_owned(),
                0
            ),
        ]
    );
}

/// The `W1005` warnings of the app of `deprecation_root(legacy)`.
fn deprecations(legacy: &str) -> Vec<Diagnostic> {
    let root = deprecation_root(legacy);
    let project = Project::load(&root.join("app")).unwrap();
    project
        .analyze()
        .unwrap()
        .warnings
        .into_iter()
        .filter(|warning| warning.code == "W1005")
        .collect()
}

#[test]
fn fix_replaces_deprecated_functions_in_parentheses() {
    let app = "using Helper\n\ndef apply :: (i64 -> i64) -> i64 = \\f -> f 1\n\nlet a = apply (Legacy.old)\nlet b = ( (Legacy.old) ) 2\na + b + Legacy.old 3\n";
    let root = root(
        "deprecated-parentheses",
        &[
            ("helper/Tsuzuri.toml", HELPER_MANIFEST),
            ("helper/Legacy.tz", LEGACY),
            ("app/Tsuzuri.toml", APP_MANIFEST),
            ("app/Main.tz", app),
        ],
    );
    let (status, _, stderr) = tsuzuri(
        &[
            "fix",
            "--applicability",
            "likely",
            root.join("app").to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(
        read(&root.join("app/Main.tz")),
        app.replace("Legacy.old", "Legacy.next")
    );
}

#[test]
fn w1005_carries_a_likely_replacement_for_a_same_signature_function() {
    let warnings = deprecations(LEGACY);
    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    let fix = &warnings[0].fixes[0];
    assert_eq!(fix.applicability, Applicability::Likely);
    assert_eq!(fix.title, "Replace 'old' with 'next'");
    assert_eq!((fix.edits[0].span.start, fix.edits[0].span.end), (29, 32));
    assert_eq!(&APP[29..32], "old");
    assert_eq!(fix.edits[0].text, "next");
}

/// `LEGACY` with generic functions.
const GENERIC: &str = "/// @deprecated use Helper::Legacy.next instead\ndef old :: 'a -> 'a = \\n -> n\n\ndef next :: 'a -> 'a = \\n -> n\n";

#[test]
fn w1005_replaces_a_generic_function_with_the_same_constraints() {
    for legacy in [
        GENERIC.to_owned(),
        GENERIC.replace(":: 'a", ":: Numeric<'a> => 'a"),
    ] {
        let warnings = deprecations(&legacy);
        assert_eq!(warnings.len(), 1, "{warnings:#?}");
        assert_eq!(warnings[0].fixes[0].edits[0].text, "next", "{legacy}");
    }
}

#[test]
fn w1005_has_no_fix_when_the_signature_differs_or_the_note_is_free_text() {
    for legacy in [
        LEGACY.replace(
            "def next :: i64 -> i64 = \\n -> n + 1",
            "def next :: i64 -> i32 = \\n -> 1",
        ),
        LEGACY.replace("def old ::", "pure def old ::"),
        GENERIC.replace("def next ::", "def next :: Numeric<'a> =>"),
        LEGACY.replace("use Helper::Legacy.next instead", "use next"),
        LEGACY.replace(
            "use Helper::Legacy.next instead",
            "use Helper::Legacy.next instead (faster)",
        ),
    ] {
        let warnings = deprecations(&legacy);
        assert_eq!(warnings.len(), 1, "{legacy}");
        assert!(warnings[0].fixes.is_empty(), "{legacy}");
    }
}

#[test]
fn fix_prefixes_unused_locals_and_a_second_run_changes_nothing() {
    let root = root("f1", &[("Main.tz", F1)]);
    let (_, before, _) = tsuzuri(&["run", "."], Some(&root));
    let (status, stdout, stderr) = tsuzuri(&["fix", "."], Some(&root));
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(
        stdout,
        "fixed Main.tz: 2 fixes\napplied 2 fixes in 1 file\n"
    );
    assert_eq!(stderr, "");
    assert_eq!(read(&root.join("Main.tz")), F1_FIXED);
    let (status, stdout, _) = tsuzuri(&["fix", "."], Some(&root));
    assert_eq!((status, stdout.as_str()), (0, "no fixes to apply\n"));
    assert_eq!(read(&root.join("Main.tz")), F1_FIXED);
    let (_, after, _) = tsuzuri(&["run", "."], Some(&root));
    assert_eq!(before, after);
}

#[test]
fn fix_dry_run_prints_the_diff_and_writes_nothing() {
    let root = root("dry", &[("Main.tz", F1)]);
    let (status, stdout, stderr) = tsuzuri(&["fix", ".", "--dry-run"], Some(&root));
    assert_eq!(status, 0);
    assert_eq!(
        stdout,
        "--- a/Main.tz\n+++ b/Main.tz\n@@ -1,5 +1,5 @@\n-def pair :: i64 -> i64 -> i64 = \\a b -> a\n+def pair :: i64 -> i64 -> i64 = \\a _b -> a\n \n-let unused = 5\n+let _unused = 5\n let x = 40\n pair x 2\nwould apply 2 fixes in 1 file\n"
    );
    assert_eq!(stderr, "");
    assert_eq!(read(&root.join("Main.tz")), F1);
}

#[test]
fn fix_json_events_are_one_object_per_line() {
    let root = root("json", &[("Main.tz", F1)]);
    let (status, stdout, _) = tsuzuri(&["fix", ".", "--json"], Some(&root));
    assert_eq!(status, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert!(lines[0].starts_with(
        "{\"event\":\"fix\",\"status\":\"applied\",\"round\":1,\"code\":\"W1001\",\"kind\":\"unused_local\",\"applicability\":\"certain\",\"title\":"
    ));
    assert_eq!(
        lines[2],
        "{\"event\":\"file\",\"path\":\"Main.tz\",\"fixes\":2}"
    );
    let summary: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(summary["event"], "summary");
    assert_eq!(summary["applied"], 2);
    assert_eq!(summary["files"], 1);
    assert_eq!(summary["remaining_warnings"], 0);
    assert!(lines[3].starts_with(
        "{\"event\":\"summary\",\"dry_run\":false,\"applicability\":\"certain\",\"rounds\":1,\"stop\":\"fixed_point\","
    ));
}

#[test]
fn fix_applies_deprecation_replacements_only_with_likely() {
    let root = deprecation_root(LEGACY);
    let app = root.join("app");
    let (status, stdout, _) = tsuzuri(&["fix", "."], Some(&app));
    assert_eq!(status, 0);
    assert_eq!(
        stdout,
        "no fixes to apply\nskipped 1 fix (use --json for the list)\nnote: rerun with --applicability likely to apply 1 more fix\n"
    );
    assert_eq!(read(&app.join("Main.tz")), APP);
    let (status, stdout, stderr) = tsuzuri(&["fix", ".", "--applicability", "likely"], Some(&app));
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(stdout, "fixed Main.tz: 1 fix\napplied 1 fix in 1 file\n");
    assert_eq!(
        read(&app.join("Main.tz")),
        APP.replace("Legacy.old 1", "Legacy.next 1")
    );
}

#[test]
fn fix_never_writes_dependencies() {
    let legacy =
        format!("{LEGACY}\ndef helper_value :: i64 -> i64 = \\n ->\n    let spare = 1\n    n\n");
    let root = deprecation_root(&legacy);
    let app = root.join("app");
    let (status, stdout, _) = tsuzuri(&["fix", ".", "--json"], Some(&app));
    assert_eq!(status, 0);
    assert_eq!(read(&root.join("helper/Legacy.tz")), legacy);
    assert!(!stdout.contains("\"status\":\"applied\""), "{stdout}");
}

#[test]
fn fix_with_a_file_input_writes_only_that_file() {
    let other = "def other :: i64 -> i64 = \\n ->\n    let spare = 1\n    n\n";
    let root = root(
        "file",
        &[
            ("Main.tz", "let unused = 5\nOther.other 1\n"),
            ("Other.tz", other),
        ],
    );
    let (status, stdout, stderr) = tsuzuri(&["fix", "Main.tz", "--json"], Some(&root));
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(
        read(&root.join("Main.tz")),
        "let _unused = 5\nOther.other 1\n"
    );
    assert_eq!(read(&root.join("Other.tz")), other);
    assert!(
        stdout.contains("\"status\":\"skipped\",\"reason\":\"outside_input\""),
        "{stdout}"
    );
}

#[test]
fn fix_then_check_with_a_warm_cache_sees_the_new_text() {
    let root = root("cache", &[("Main.tz", F1)]);
    let cache = root.join("cache");
    let run = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(arguments)
            .arg(root.join("Main.tz"))
            .env("TSUZURI_CACHE_DIR", &cache)
            .output()
            .unwrap();
        String::from_utf8(output.stderr).unwrap()
    };
    assert!(run(&["check"]).contains("W1001"));
    run(&["fix"]);
    assert_eq!(run(&["check"]), "");
}

#[test]
fn edition_option_rejects_unsupported_years() {
    let root = root("edition-2030", &[("Main.tz", "42\n")]);
    let (status, _, stderr) = tsuzuri(&["fix", ".", "--edition", "2030"], Some(&root));
    assert_eq!(status, 2);
    assert!(stderr.contains("error[E2000]"), "{stderr}");
    assert!(stderr.contains("unsupported edition '2030'"), "{stderr}");
}

#[test]
fn edition_option_to_the_current_edition_is_a_no_op() {
    let manifest = "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2026\"\n";
    let root = root(
        "edition-same",
        &[
            ("Tsuzuri.toml", manifest),
            ("Main.tz", "let unused = 5\n42\n"),
        ],
    );
    let (status, stdout, stderr) = tsuzuri(&["fix", ".", "--edition", "2026"], Some(&root));
    assert_eq!(status, 0, "{stderr}");
    assert_eq!(stdout, "edition is already 2026; nothing to migrate\n");
    assert_eq!(read(&root.join("Tsuzuri.toml")), manifest);
    assert_eq!(read(&root.join("Main.tz")), "let unused = 5\n42\n");
}

#[test]
fn edition_option_conflicts_with_applicability() {
    let (status, _, stderr) = tsuzuri(
        &[
            "fix",
            ".",
            "--edition",
            "2026",
            "--applicability",
            "likely",
            "--json",
        ],
        None,
    );
    assert_eq!(status, 2);
    let line: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(line["kind"], "fix_edition_with_applicability");
}
