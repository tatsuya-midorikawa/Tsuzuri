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
        def optional :: Maybe<i64>\nfn optional = Maybe { let! unused = Some 1; return 2 }\n\
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

/// A fresh root with the given files, each at a path below it.
fn package_root(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-deprecation-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

const LEGACY_MANIFEST: &str = "[package]\nname = \"legacy-lib\"\nversion = \"0.1.0\"\n";
const APP_MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nlegacy-lib = { path = \"../legacy-lib\" }\n";
const LEGACY_OLD: &str = "\
/// @deprecated use LegacyLib::Old.next instead
def old :: i64 -> i64 = \\x -> x + 1

def next :: i64 -> i64 = \\x -> x + 1

/// @deprecated use a tuple
record Pair { left: i64, right: i64 }

/// @deprecated use bool
union Light = Red | Green

def inside :: i64 -> i64 = \\x -> old x
";
const APP_MAIN: &str = "\
let pair = LegacyLib::Old.Pair { left: 1, right: 2 }
let light = LegacyLib::Old.Red
let bonus = match light with
    | LegacyLib::Old.Red -> 0
    | LegacyLib::Old.Green -> 1
LegacyLib::Old.old 39 + pair.right + bonus
";

/// `(line, column, message)` of each warning with `code`, in reported order, with the file name
/// each is in.
fn located(
    project: &tsuzuri::driver::Project,
    warnings: &[Diagnostic],
) -> Vec<(String, usize, usize, String)> {
    warnings
        .iter()
        .map(|warning| {
            let source = project.source_for(warning);
            let (line, column) = tsuzuri::diagnostic::location(&source.text, warning.span.start);
            let file = source
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (
                file,
                line,
                column,
                format!("{}: {}", warning.code, warning.message),
            )
        })
        .collect()
}

#[test]
fn warns_on_deprecated_uses_from_other_packages_only() {
    let root = package_root(
        "graph",
        &[
            ("legacy-lib/Tsuzuri.toml", LEGACY_MANIFEST),
            ("legacy-lib/Old.tz", LEGACY_OLD),
            ("app/Tsuzuri.toml", APP_MANIFEST),
            ("app/Main.tz", APP_MAIN),
        ],
    );
    let project = tsuzuri::driver::Project::load(&root.join("app")).unwrap();
    let module = project.analyze().unwrap();
    let main = |line, column, message: &str| {
        (
            "Main.tz".to_owned(),
            line,
            column,
            format!("W1005: {message}"),
        )
    };
    // The record's construction, the union's case, its two patterns, and the function. The
    // use of `old` inside legacy-lib's own `inside` is in the same package and does not warn.
    assert_eq!(
        located(&project, &module.warnings),
        [
            main(1, 27, "'LegacyLib::Old.Pair' is deprecated: use a tuple"),
            main(2, 28, "'LegacyLib::Old.Light' is deprecated: use bool"),
            main(4, 22, "'LegacyLib::Old.Light' is deprecated: use bool"),
            main(5, 22, "'LegacyLib::Old.Light' is deprecated: use bool"),
            main(
                6,
                16,
                "'LegacyLib::Old.old' is deprecated: use LegacyLib::Old.next instead"
            ),
        ]
    );
    assert!(
        module
            .warnings
            .iter()
            .all(|warning| warning.severity == Severity::Warning)
    );
    // The library itself is one package: nothing in it is a use from another package.
    let library = tsuzuri::driver::Project::load(&root.join("legacy-lib/Old.tz")).unwrap();
    assert!(library.analyze().unwrap().warnings.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn deprecation_covers_types_signatures_payload_cases_and_other_dependencies() {
    let library = "\
/// @deprecated
union Reading = Missing | Value of i64

/// @deprecated The type moved.
record Box<'a> { item: 'a }

/// @deprecatedX is not the tag
def fine :: i64 -> i64 = \\x -> x

/// @deprecated never called from outside
private def hidden :: i64 -> i64 = \\x -> x

/// @deprecated a constant
const LIMIT: i64 = 3

/// @deprecated an alias
type Count = i64

/// @deprecated an extern
extern def host :: i64 -> i64

/// @deprecated first line
/// @deprecated second line is ordinary text
def twice :: i64 -> i64 = \\x -> x + x
";
    let middle = "\
def lift :: i64 -> i64 = \\x -> Library::Old.twice x
";
    let app = "\
def read :: Library::Old.Reading -> i64 = \\reading ->
    match reading with
    | Library::Old.Missing -> 0
    | Library::Old.Value value -> value

def wrap :: i64 -> Library::Old.Box<i64> = \\value -> Library::Old.Box { item: value }

let made = Library::Old.Value 4
let typed: Library::Old.Reading = made
let unused = Library::Old.fine 1 + Library::Old.LIMIT
let count: Library::Old.Count = 2
let step = Middle::Bridge.lift
read typed + count + (wrap 1).item + Library::Old.twice 1
";
    let root = package_root(
        "kinds",
        &[
            (
                "library/Tsuzuri.toml",
                "[package]\nname = \"library\"\nversion = \"0.1.0\"\n",
            ),
            ("library/Old.tz", library),
            (
                "middle/Tsuzuri.toml",
                "[package]\nname = \"middle\"\nversion = \"0.1.0\"\n[dependencies]\nlibrary = { path = \"../library\" }\n",
            ),
            ("middle/Bridge.tz", middle),
            (
                "app/Tsuzuri.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nlibrary = { path = \"../library\" }\nmiddle = { path = \"../middle\" }\n",
            ),
            ("app/Main.tz", app),
        ],
    );
    let project = tsuzuri::driver::Project::load(&root.join("app")).unwrap();
    let module = project.analyze().unwrap();
    let reported: Vec<_> = located(&project, &module.warnings)
        .into_iter()
        .filter_map(|(file, line, column, message)| {
            Some((
                file,
                line,
                column,
                message.strip_prefix("W1005: ")?.to_owned(),
            ))
        })
        .collect();
    let see = "see its documentation for a replacement";
    let reading = format!("'Library::Old.Reading' is deprecated; {see}");
    let twice = "'Library::Old.twice' is deprecated: first line";
    let boxed = "'Library::Old.Box' is deprecated: The type moved.";
    let at = |file: &str, line, column, message: &str| {
        (file.to_owned(), line, column, message.to_owned())
    };
    // `Main.tz` sorts before `Middle/Bridge.tz`. A case, a payload case, a pattern and a type
    // annotation each count as a use of the union. Only W1005 is listed: the const, the alias,
    // `@deprecatedX` and the private declaration have no W1005, and a second tag line is text.
    let expected = [
        at("Main.tz", 1, 26, &reading),
        at("Main.tz", 3, 20, &reading),
        at("Main.tz", 4, 20, &reading),
        at("Main.tz", 6, 33, boxed),
        at("Main.tz", 6, 67, boxed),
        at("Main.tz", 8, 25, &reading),
        at("Main.tz", 9, 25, &reading),
        at("Main.tz", 13, 51, twice),
        // The dependency `middle` uses `twice` of `library`, another package.
        at("Bridge.tz", 1, 45, twice),
    ];
    assert_eq!(reported, expected, "{reported:#?}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn warns_on_deprecated_std_declarations() {
    let std_legacy = "\
/// @deprecated use Legacy.next
def old :: i64 -> i64 = \\x -> x + 1

def next :: i64 -> i64 = \\x -> x + 1

/// @deprecated
record Pair { left: i64, right: i64 }

/// @deprecated a constant
const LIMIT: i64 = 3

/// Std itself may still use what it deprecates.
def inside :: i64 -> i64 = \\x -> old x
";
    let main = "\
let pair = Legacy.Pair { left: 1, right: 2 }
Legacy.old 1 + Legacy.LIMIT + pair.left
";
    let module = tsuzuri::analyze_modules_with_std_all(
        &[("Main.tz", main)],
        &[("std/Legacy.tz", std_legacy)],
    )
    .unwrap();
    let messages: Vec<_> = module
        .warnings
        .iter()
        .map(|warning| {
            let (line, column) = tsuzuri::diagnostic::location(main, warning.span.start);
            (line, column, warning.code, warning.message.as_str())
        })
        .collect();
    assert_eq!(
        messages,
        [
            (
                1,
                19,
                "W1005",
                "'std::Legacy.Pair' is deprecated; see its documentation for a replacement"
            ),
            (
                2,
                8,
                "W1005",
                "'std::Legacy.old' is deprecated: use Legacy.next"
            ),
        ]
    );
    // A deprecated module of the same package is not a use from another package.
    let module = tsuzuri::analyze_modules(&[
        ("Main.tz", "Old.old 1\n"),
        (
            "Old.tz",
            "/// @deprecated use new\ndef old :: i64 -> i64 = \\x -> x\n",
        ),
    ])
    .unwrap();
    assert!(module.warnings.is_empty(), "{:?}", module.warnings);
}

#[test]
fn a_long_deprecation_note_is_cut() {
    let note = "n".repeat(250);
    let std_legacy = format!("/// @deprecated {note}\ndef old :: i64 = 1\n");
    let module = tsuzuri::analyze_modules_with_std_all(
        &[("Main.tz", "Legacy.old\n")],
        &[("std/Legacy.tz", &std_legacy)],
    )
    .unwrap();
    assert_eq!(module.warnings.len(), 1, "{:?}", module.warnings);
    assert_eq!(
        module.warnings[0].message,
        format!("'std::Legacy.old' is deprecated: {}...", "n".repeat(200))
    );
}

#[test]
fn deprecation_warnings_reach_the_cli_and_match_with_and_without_the_frontend_cache() {
    use std::process::Command;
    let root = package_root(
        "cli",
        &[
            ("legacy-lib/Tsuzuri.toml", LEGACY_MANIFEST),
            ("legacy-lib/Old.tz", LEGACY_OLD),
            ("app/Tsuzuri.toml", APP_MANIFEST),
            ("app/Main.tz", APP_MAIN),
        ],
    );
    let app = root.join("app");
    let cache = root.join("cache");
    let no_cache = std::path::Path::new("");
    // The exit status, standard output and standard error of `tsuzuri <command> app <flags>`.
    let tsuzuri = |command: &str, flags: &[&str], cache: &std::path::Path| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg(command)
            .arg(&app)
            .args(flags)
            .env("TSUZURI_CACHE_DIR", cache)
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    };
    // An empty TSUZURI_CACHE_DIR disables every cache.
    let uncached = tsuzuri("check", &["--json"], no_cache);
    assert_eq!(uncached.0, Some(0), "{uncached:?}");
    let lines: Vec<_> = uncached.2.lines().collect();
    assert_eq!(lines.len(), 5, "{uncached:?}");
    for line in lines {
        assert!(line.contains("\"severity\":\"warning\""), "{line}");
        assert!(line.contains("\"code\":\"W1005\""), "{line}");
    }
    // The first run with a cache parses and stores; the second reads the stored parse.
    let cold = tsuzuri("check", &["--json"], &cache);
    let stored = std::fs::read_dir(cache.join("frontend"))
        .unwrap()
        .any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("p-")
        });
    assert!(stored, "the frontend cache stored no pack");
    let warm = tsuzuri("check", &["--json"], &cache);
    assert_eq!(cold, uncached);
    assert_eq!(warm, uncached);
    // The rendered form, too, is the same with and without the cache.
    assert_eq!(
        tsuzuri("check", &[], &cache),
        tsuzuri("check", &[], no_cache)
    );
    // Without --deny-warnings the program runs; with it, nothing does.
    let ran = tsuzuri("run", &[], &cache);
    assert_eq!((ran.0, ran.1.trim()), (Some(0), "42"), "{ran:?}");
    let denied = tsuzuri("run", &["--deny-warnings"], &cache);
    assert_eq!(denied.0, Some(1), "{denied:?}");
    assert!(denied.1.is_empty(), "{denied:?}");
    assert!(denied.2.contains("warning[W1005]"), "{denied:?}");
    std::fs::remove_dir_all(root).unwrap();
}
