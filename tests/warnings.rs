use tsuzuri::diagnostic::{Diagnostic, DiagnosticSet, Severity, Span};

fn warnings(source: &str) -> Vec<Diagnostic> {
    tsuzuri::analyze(source).unwrap().warnings
}

#[test]
fn warns_on_user_parameters_bindings_patterns_and_loop_variables() {
    let source = "def unused :: i64 -> i64\nfn unused arg0 =\n    let discarded = 1\n    42\n\
        def loop :: unit\nfn loop = for ignored = 0 to 2 do ()\n\
        def pattern :: (i64 * i64) -> i64\nfn pattern pair = match pair with | (first, second) -> first";
    let diagnostics = warnings(source);
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    for (diagnostic, name) in diagnostics
        .iter()
        .zip(["arg0", "discarded", "ignored", "second"])
    {
        assert_eq!(diagnostic.code, "W1001");
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(&source[diagnostic.span.start..diagnostic.span.end], name);
    }
}

#[test]
fn counts_moves_borrows_guards_and_captures_as_uses() {
    let diagnostics = warnings(
        "def consume :: string -> unit\nfn consume value = { let _ = value; () }\n\
        def moved :: string -> unit\nfn moved value = consume value\n\
        def borrowed :: string -> i64\nfn borrowed value = String.length (ref value)\n\
        def guarded :: i64 -> i64\nfn guarded value = match 0 with | found when value > 0 -> found | _ -> 0\n\
        def captured :: i64 -> i64\nfn captured value = { let next = argument -> value + argument; next 1 }\n\
        def ignored :: i64 -> i64\nfn ignored _argument = { let _local = 1; 0 }",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn suppresses_generated_and_standard_library_bindings() {
    let diagnostics = warnings(
        "export def curried :: i64 -> i64 -> i64\nfn curried value = next -> value + next\n\
        def optional :: Option<i64>\nfn optional = Option { let! unused = Some 1; return 2 }\n\
        def destructured :: (i64 * i64) -> i64\nfn destructured pair = (\\(first, _) -> first) pair",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("'unused'"));
}

#[test]
fn private_reachability_includes_types_aliases_and_public_roots() {
    let diagnostics = warnings(
        "private type KeptAlias = i64\nprivate type DeadAlias = i64\n\
        private record Kept { value: KeptAlias }\nprivate record Dead { value: DeadAlias }\n\
        private def helper :: i64\nfn helper = { let value: KeptAlias = 3; let item = Kept { value: value }; item.value }\n\
        def api :: i64\nfn api = helper()\n\
        private def dead :: i64\nfn dead = { let item = Dead { value: 4 }; item.value }\n\
        private def rec cycle :: i64 -> i64\nfn rec cycle value = cycle value\n\
        record Public { value: i64 }\ntype PublicAlias = i64\ndef public_unused :: i64\nfn public_unused = 1",
    );
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    for (diagnostic, name) in
        diagnostics
            .iter()
            .zip(["Main.DeadAlias", "Main.Dead", "Main.dead", "Main.cycle"])
    {
        assert_eq!(diagnostic.code, "W1002");
        assert!(diagnostic.message.contains(name), "{diagnostic:?}");
    }
}

#[test]
fn active_patterns_instances_and_entry_annotations_keep_private_dependencies() {
    let diagnostics = warnings(
        "private type UnitCount = i64\n\
        private def helper :: i64 -> i64\nfn helper value = value + 1\n\
        def (|Whole|) :: i64 -> i64\nfn (|Whole|) value = helper value\n\
        class Score<'a> { def score :: 'a -> i64 }\nrecord Point { value: i64 }\n\
        private def score_helper :: Point -> i64\nfn score_helper value = value.value\n\
        instance Score<Point> { fn score value = score_helper value }\n\
        let count: UnitCount = 2\ncount",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn cli_caps_warnings_and_denies_before_touching_artifacts() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-warnings-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let input = root.join("Main.tz");
    let artifact = root.join("kept.ll");
    fs::write(&input, "let unused = 1\n42").unwrap();
    fs::write(&artifact, "unchanged").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("check")
        .arg(&input)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("\"severity\":\"warning\""));
    for action in ["check", "build", "run"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .arg(action)
            .arg(&input)
            .args(["--json", "--deny-warnings"]);
        if action == "build" {
            command.args(["--emit", "llvm", "-o"]).arg(&artifact);
        }
        let output = command
            .env("TSUZURI_CLANG", root.join("missing-clang"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{action}: {output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("\"severity\":\"warning\""), "{stderr}");
        assert!(!stderr.contains("\"severity\":\"error\""), "{stderr}");
        assert_eq!(fs::read_to_string(&artifact).unwrap(), "unchanged");
    }
    let source = (0..60)
        .map(|index| format!("let unused{index} = 1\n"))
        .collect::<String>()
        + "42";
    fs::write(&input, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("check")
        .arg(&input)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("\"severity\":\"warning\""))
            .count(),
        50
    );
    assert!(stderr.contains("10 more warnings not shown"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn warning_severity_survives_rendering_sorting_and_capping() {
    let warning = Diagnostic::warning("W1001", "unused local 'value'", Span::new(4, 9));
    assert_eq!(warning.severity, Severity::Warning);
    assert!(
        warning
            .render("Main.tz", "let value = 1")
            .contains("warning[W1001]")
    );
    assert!(
        warning
            .json("Main.tz", "let value = 1")
            .contains("\"severity\":\"warning\"")
    );
    assert_eq!(
        Diagnostic::new("E1003", "bad type", Span::default()).severity,
        Severity::Error
    );
    let diagnostics = DiagnosticSet::from_diagnostics(
        (0..60).map(|index| Diagnostic::warning("W1001", "unused", Span::new(index, index + 1))),
        0,
    );
    assert_eq!(diagnostics.diagnostics.len(), 50);
    assert_eq!(diagnostics.omitted, 10);
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == Severity::Warning)
    );
}
