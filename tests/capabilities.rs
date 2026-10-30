//! E15: the capabilities that each package uses, the `[capabilities]` policy and `E1036`.
//! Expected values come from the std sources read by hand (appendix A of the ticket, with the
//! functions added to std since) and from positions counted in the fixture files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsuzuri::capabilities::{self, Analysis, Capability, CapabilitySet, Scope};
use tsuzuri::diagnostic::RelatedRole;
use tsuzuri::driver::{AnalysisKind, Project};

fn analysis_of(project: &Project, kind: AnalysisKind) -> Analysis {
    let module = project
        .analyze_cached(false, kind)
        .unwrap_or_else(|errors| {
            panic!(
                "{}",
                errors
                    .first()
                    .map_or(String::new(), |error| error.message.clone())
            )
        });
    capabilities::analyze(&module, &project.source_infos()).unwrap()
}

/// A fresh directory for one test's projects.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-capabilities-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn tsuzuri(arguments: &[&str], directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap()
}

/// A relative path as a diagnostic spells it on this platform.
fn native(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

/// Appendix D.4: `app` allows `fs` and `stdio`; its dependency `fscache` is granted `fs` and
/// also reads the clock.
fn d4(root: &Path, allow: &str, grant: &str) -> PathBuf {
    write(
        root,
        "app/Tsuzuri.toml",
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfscache = {{ path = \"../fscache\"{grant} }}\n\n[capabilities]\nallow = {allow}\n"
        ),
    );
    write(
        root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() ->\n    let! _text = Fscache::Cache.load \"notes.txt\"\n    do! IO.write_line \"loaded\"\n    0\n",
    );
    write(
        root,
        "fscache/Tsuzuri.toml",
        "[package]\nname = \"fscache\"\nversion = \"0.2.0\"\n",
    );
    write(
        root,
        "fscache/Cache.tz",
        "def load :: string -> IO<Result<string, Os.Error>> = \\path -> File.read_text path\n\ndef stamp :: unit -> IO<Result<i64, Os.Error>> = \\() -> Time.monotonic_ns ()\n",
    );
    root.join("app")
}

fn check_errors(project: &Path) -> Vec<tsuzuri::diagnostic::Diagnostic> {
    let project = Project::load(project).unwrap();
    let module = project
        .analyze_cached(false, AnalysisKind::Program)
        .unwrap();
    project.check_capabilities(&module).unwrap()
}

/// `(line, column, callee, capability)` of each site of the root package.
fn root_sites(project: &Project, analysis: &Analysis) -> Vec<(usize, usize, String, Capability)> {
    analysis
        .sites
        .iter()
        .filter(|(package, _)| package.is_none())
        .map(|(_, site)| {
            let source = &project.sources[site.span.source.unwrap()];
            let (line, column) = tsuzuri::diagnostic::location(&source.text, site.span.start);
            (line, column, site.callee.clone(), site.capability)
        })
        .collect()
}

#[test]
fn std_probe_matches_the_expected_sites() {
    use Capability::*;
    let project = Project::load(Path::new("tests/fixtures/capabilities/probe")).unwrap();
    let analysis = analysis_of(&project, AnalysisKind::Program);
    let mut sites = root_sites(&project, &analysis);
    sites.sort_by_key(|(line, column, _, _)| (*line, *column));
    let expected = [
        (3, 65, "File.read_text", Fs),
        (4, 66, "Dir.walk", Fs),
        (5, 71, "Env.var", Env),
        (6, 62, "Time.monotonic_ns", Time),
        (7, 65, "Random.next_u64", Random),
        (8, 46, "IO.write_line", Stdio),
        (9, 42, "Debug.print", Stdio),
        (10, 83, "Net.connect", Net),
        (11, 87, "Net.read_async", Net),
        (12, 61, "HashMap.randomized", Random),
        (13, 56, "Async.block_on", Time),
        (14, 65, "Gpu.request", Gpu),
        (15, 76, "Process.run", Process),
        (18, 41, "c_abs", Ffi),
    ];
    let expected: Vec<_> = expected
        .into_iter()
        .map(|(line, column, callee, capability)| (line, column, callee.to_owned(), capability))
        .collect();
    assert_eq!(sites, expected);
    // `Net.parse_address` (16:63) and `Path.join` (17:52) are pure.
    assert!(sites.iter().all(|(line, ..)| *line != 16 && *line != 17));
    let [c_abs] = analysis.externs.as_slice() else {
        panic!("{:?}", analysis.externs);
    };
    assert_eq!(
        (c_abs.name.as_str(), c_abs.symbol.as_str()),
        ("c_abs", "abs")
    );
    assert_eq!((c_abs.trusted, c_abs.uses.len()), (None, 1));
    assert_eq!(analysis.multiplexed_outside, 0);
}

#[test]
fn std_world_primitives_are_referenced_only_by_the_known_files() {
    let world: Vec<&str> = [
        "Debug.__print_string",
        "IO.__read_line",
        "IO.__write",
        "Os.__read",
        "Os.__write",
        "Os.__open",
        "Os.__handle",
        "Os.__close",
        "Os.__args",
        "Os.__random",
        "Os.__clock",
        "Os.__sleep",
        "Os.__spawn",
        "Async.__clock",
        "Async.__wait",
        "Async.__posted",
        "Bench.now",
    ]
    .into_iter()
    .chain(
        [
            "resolve", "open", "accept", "read", "write", "close", "watch", "unwatch", "connect",
            "names", "send", "classify",
        ]
        .iter()
        .map(|name| &*Box::leak(format!("Net.__{name}").into_boxed_str())),
    )
    .chain(
        ["open", "features", "run", "select", "last"]
            .iter()
            .map(|name| &*Box::leak(format!("Gpu.__{name}").into_boxed_str())),
    )
    .collect();
    let mut found = BTreeMap::new();
    for entry in std::fs::read_dir("std").unwrap().flatten() {
        let path = entry.path();
        let text: String = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| line.split("//").next().unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = text.as_bytes();
        let mut count = 0;
        for name in &world {
            let mut from = 0;
            while let Some(at) = text[from..].find(name) {
                let start = from + at;
                let end = start + name.len();
                let before = start.checked_sub(1).map(|index| bytes[index]);
                let after = bytes.get(end);
                let boundary =
                    |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.';
                if !before.is_some_and(boundary)
                    && !after.is_some_and(|byte| boundary(*byte) && *byte != b'.')
                {
                    count += 1;
                }
                from = end;
            }
        }
        if count > 0 {
            found.insert(
                path.file_name().unwrap().to_string_lossy().into_owned(),
                count,
            );
        }
    }
    let expected = BTreeMap::from(
        [
            ("Async.tc", 3),
            ("Bench.tz", 4),
            ("Debug.tz", 1),
            ("Dir.tz", 3),
            ("Env.tz", 3),
            ("File.tz", 6),
            ("Gpu.tz", 9),
            ("IO.tc", 2),
            ("Net.tz", 26),
            ("Process.tz", 1),
            ("Random.tz", 1),
            ("Time.tz", 2),
        ]
        .map(|(file, count)| (file.to_owned(), count)),
    );
    assert_eq!(
        found, expected,
        "a std file reaches the host in a new place; update appendix A of E15, the table of \
         src/capabilities.rs and tests/fixtures/capabilities/std-table.txt"
    );
    assert_eq!(found.values().sum::<usize>(), 61);
}

/// The projects of the corpus: every fixture and example with a `Main.tz`.
fn corpus() -> Vec<PathBuf> {
    let mut projects = Vec::new();
    for parent in ["tests/fixtures", "examples"] {
        for entry in std::fs::read_dir(parent).unwrap().flatten() {
            let path = entry.path();
            if path.join("Main.tz").is_file() {
                projects.push(path);
            }
        }
    }
    projects.sort();
    projects
}

/// The capabilities of every public std function that the corpus instantiates. Every public
/// std function has an instance there (`tests/effects_alloc_conservative.rs` checks it).
fn corpus_table() -> BTreeMap<String, CapabilitySet> {
    let projects = corpus();
    let tables = std::sync::Mutex::new(Vec::new());
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(path) = projects.get(index) else {
                        break;
                    };
                    let Ok(project) = Project::load_for_tests(path) else {
                        continue;
                    };
                    let Ok(module) = project.analyze_cached(false, AnalysisKind::Tests) else {
                        continue;
                    };
                    let analysis = capabilities::analyze(&module, &project.source_infos()).unwrap();
                    assert_eq!(analysis.multiplexed_outside, 0, "{}", path.display());
                    tables.lock().unwrap().push(analysis.std_functions);
                }
            });
        }
    });
    let mut table = BTreeMap::new();
    for part in tables.into_inner().unwrap() {
        for (name, set) in part {
            let entry: &mut CapabilitySet = table.entry(name).or_default();
            *entry = entry.union(set);
        }
    }
    table
}

#[test]
fn std_table_file_lists_the_96_functions() {
    let expected = std::fs::read_to_string("tests/fixtures/capabilities/std-table.txt").unwrap();
    let lines: Vec<&str> = expected.lines().collect();
    assert_eq!(lines.len(), 96);
    let derived: Vec<String> = corpus_table()
        .into_iter()
        .map(|(name, set)| {
            let names: Vec<&str> = set.iter().map(Capability::name).collect();
            format!("{name}: {}", names.join(" "))
        })
        .collect();
    assert_eq!(derived, lines);
    // Every line is `Module.function: capability` with a known capability.
    for line in lines {
        let (name, capability) = line.split_once(": ").unwrap();
        assert!(name.contains('.'), "{line}");
        assert!(Capability::from_name(capability).is_some(), "{line}");
    }
}

#[test]
fn policy_off_costs_nothing_and_changes_nothing() {
    let root = scratch("off");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfscache = { path = \"../fscache\" }\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() ->\n    let! _text = Fscache::Cache.load \"notes.txt\"\n    do! IO.write_line \"loaded\"\n    0\n",
    );
    write(
        &root,
        "fscache/Tsuzuri.toml",
        "[package]\nname = \"fscache\"\nversion = \"0.2.0\"\n",
    );
    write(
        &root,
        "fscache/Cache.tz",
        "def load :: string -> IO<Result<string, Os.Error>> = \\path -> File.read_text path\n",
    );
    let project = Project::load(&root.join("app")).unwrap();
    let policy = project.capabilities.as_ref().unwrap();
    assert!(!policy.enforced);
    assert!(
        policy
            .effective
            .values()
            .all(|set| *set == CapabilitySet::ALL)
    );
    assert!(check_errors(&root.join("app")).is_empty());
    let check = tsuzuri(&["check", "app"], &root);
    assert!(check.status.success());
    assert!(
        check.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    // A project without a manifest has no policy at all.
    let plain = Project::load(Path::new("examples/io")).unwrap();
    assert!(plain.capabilities.is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn root_allow_denies_the_root_itself() {
    let root = scratch("root");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = [\"stdio\"]\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() ->\n    let! _t = File.read_text \"notes.txt\"\n    0\n",
    );
    let errors = check_errors(&root.join("app"));
    let [error] = errors.as_slice() else {
        panic!("{errors:?}");
    };
    assert_eq!(error.code, "E1036");
    assert_eq!(error.kind, Some("capability_denied"));
    assert_eq!(
        error.message,
        "the root package may not use capability 'fs' ('File.read_text'); add \"fs\" to allow in [capabilities]"
    );
    let output = tsuzuri(&["check", "app"], &root);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("{}:2:15: error[E1036]", native("app/Main.tz"))),
        "{stderr}"
    );
    // `build` stops before code generation, and `run` too.
    for command in ["build", "run"] {
        let output = tsuzuri(&[command, "app"], &root);
        assert_eq!(output.status.code(), Some(1), "{command}");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dependency_without_a_grant_gets_the_empty_set() {
    let root = scratch("empty");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlogger = { path = \"../logger\" }\n\n[capabilities]\nallow = [\"stdio\"]\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() ->\n    do! Logger::Log.say \"hi\"\n    0\n",
    );
    write(
        &root,
        "logger/Tsuzuri.toml",
        "[package]\nname = \"logger\"\nversion = \"1.0.0\"\n",
    );
    write(
        &root,
        "logger/Log.tz",
        "def say :: string -> IO<unit> = \\text -> IO.write_line text\n",
    );
    let errors = check_errors(&root.join("app"));
    let messages: Vec<&str> = errors.iter().map(|error| error.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "package 'logger' may not use capability 'stdio' ('IO.write_line'); grant it with capabilities = [\"stdio\"] in the entry of 'logger' in 'app'"
        ]
    );
    assert_eq!(
        errors[0].help.as_deref(),
        Some("grant \"stdio\" to package 'logger' in Tsuzuri.toml")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn dependency_grant_is_cut_by_the_root_allow() {
    let root = scratch("cut");
    let app = d4(&root, "[\"stdio\"]", ", capabilities = [\"fs\"]");
    let errors = check_errors(&app);
    let messages: Vec<&str> = errors.iter().map(|error| error.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "package 'fscache' may not use capability 'fs' ('File.read_text'); the root [capabilities] allow does not list \"fs\"",
            "package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); the root [capabilities] allow does not list \"time\"",
        ]
    );
    // Appendix C.1, example 1, and its variant: the root allows `time`, the entry does not.
    let root = scratch("d4");
    let app = d4(&root, "[\"fs\", \"stdio\"]", ", capabilities = [\"fs\"]");
    let messages: Vec<String> = check_errors(&app)
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages,
        [
            "package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); the root [capabilities] allow does not list \"time\""
        ]
    );
    let app = d4(
        &root,
        "[\"fs\", \"stdio\", \"time\"]",
        ", capabilities = [\"fs\"]",
    );
    let messages: Vec<String> = check_errors(&app)
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages,
        [
            "package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); grant it with capabilities = [\"time\"] in the entry of 'fscache' in 'app'"
        ]
    );
    let app = d4(
        &root,
        "[\"fs\", \"stdio\", \"time\"]",
        ", capabilities = [\"fs\", \"time\"]",
    );
    assert!(check_errors(&app).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn denial_names_the_std_chain() {
    let root = scratch("chain");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nprops = { path = \"../props\" }\n\n[capabilities]\nallow = [\"stdio\"]\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() ->\n    Props::Check.holds ()\n    0\n",
    );
    write(
        &root,
        "props/Tsuzuri.toml",
        "[package]\nname = \"props\"\nversion = \"1.0.0\"\n",
    );
    write(
        &root,
        "props/Check.tz",
        "def holds :: unit -> unit = \\() -> Gen.for_all (Gen.range 0 9) (\\n -> n >= 0)\n",
    );
    let errors = check_errors(&root.join("app"));
    let error = errors
        .iter()
        .find(|error| error.message.contains("('Gen.for_all')"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert!(
        error
            .message
            .starts_with("package 'props' may not use capability 'stdio'")
    );
    let via: Vec<&str> = error
        .related
        .iter()
        .filter(|related| related.role == RelatedRole::ViaCall)
        .map(|related| related.message.as_str())
        .collect();
    assert!(
        via.first()
            .is_some_and(|first| first.starts_with("'Gen.for_all' calls ")),
        "{via:?}"
    );
    let last = error.related.last().unwrap();
    assert_eq!(last.role, RelatedRole::Note);
    assert!(
        last.message
            .ends_with("reaches the host through 'Debug.__print_string'"),
        "{}",
        last.message
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn generic_std_instances_keep_their_capability() {
    let root = scratch("generic");
    write(
        &root,
        "Main.tz",
        "def ints :: unit -> IO<HashMap<i64, i64>> = \\() -> HashMap.randomized ()\n\
         def texts :: unit -> IO<HashMap<string, i64>> = \\() -> HashMap.randomized ()\n\
         def pick :: 'a -> 'a = \\value -> value\n\
         def main :: unit -> i32 = \\() -> pick 0\n",
    );
    let project = Project::load(&root).unwrap();
    let analysis = analysis_of(&project, AnalysisKind::Program);
    let sites = root_sites(&project, &analysis);
    assert_eq!(
        sites,
        [
            (1, 52, "HashMap.randomized".to_owned(), Capability::Random),
            (2, 56, "HashMap.randomized".to_owned(), Capability::Random),
        ]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tests_and_benches_are_exempt_from_the_check() {
    let root = scratch("tests");
    write(
        &root,
        "Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = []\n",
    );
    write(
        &root,
        "Main.tz",
        &format!("{MAIN}test \"prints\" = Debug.print (ref \"x\")\n"),
    );
    let project = Project::load_for_tests(&root).unwrap();
    let module = project.analyze_cached(false, AnalysisKind::Tests).unwrap();
    assert!(project.check_capabilities(&module).unwrap().is_empty());
    let analysis = capabilities::analyze(&module, &project.source_infos()).unwrap();
    let scopes: Vec<(Capability, Scope)> = analysis
        .sites
        .iter()
        .map(|(_, site)| (site.capability, site.scope))
        .collect();
    assert_eq!(scopes, [(Capability::Stdio, Scope::Test)]);
    let output = tsuzuri(&["test", "."], &root);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn extern_needs_ffi_and_trusted_does_not_change_it() {
    let root = scratch("ffi");
    write(
        &root,
        "Tsuzuri.toml",
        "[package]\nname = \"ffi\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = [\"stdio\"]\n",
    );
    write(
        &root,
        "Main.tz",
        "@trusted pure\nextern \"abs\" def c_abs :: i32 -> i32\n\ndef main :: unit -> i32 = \\() -> c_abs (0 - 5)\n",
    );
    let errors = check_errors(&root);
    let [error] = errors.as_slice() else {
        panic!("{errors:?}");
    };
    assert_eq!(
        error.message,
        "the root package may not use capability 'ffi' ('c_abs'); add \"ffi\" to allow in [capabilities]"
    );
    assert_eq!(error.related[0].role, RelatedRole::DeclaredHere);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn limits_report_e1017() {
    let project = Project::load(Path::new("examples/io")).unwrap();
    let module = project
        .analyze_cached(false, AnalysisKind::Program)
        .unwrap();
    let sources = project.source_infos();
    let error = capabilities::analyze_with_limits(&module, &sources, 2, 1_000_000).unwrap_err();
    assert_eq!(
        (error.code, error.kind),
        ("E1017", Some("capability_analysis_limit"))
    );
    assert!(
        error.message.contains("at most 2 functions"),
        "{}",
        error.message
    );
    let error = capabilities::analyze_with_limits(&module, &sources, 65_536, 10).unwrap_err();
    assert!(
        error.message.contains("at most 10 expression nodes"),
        "{}",
        error.message
    );
    assert!(capabilities::analyze(&module, &sources).is_ok());
}

/// Review of #65: top-level code whose result is printed writes to the standard output through
/// the compiler's entry, so a policy without `stdio` refuses it.
#[test]
fn a_printed_top_level_result_uses_stdio() {
    let root = scratch("printed");
    write(
        &root,
        "Tsuzuri.toml",
        "[package]\nname = \"printer\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = []\n",
    );
    write(&root, "Main.tz", "42\n");
    let errors = check_errors(&root);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, "E1036");
    assert!(
        errors[0].message.contains("'stdio'"),
        "{}",
        errors[0].message
    );
    // A unit result prints nothing, and `def main` returns its exit code.
    write(&root, "Main.tz", "let x = 1\n");
    assert!(check_errors(&root).is_empty());
    write(&root, "Main.tz", MAIN);
    assert!(check_errors(&root).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

/// Review 3 of #65: a `def main` that takes the command-line arguments reads them from the
/// host through the compiler's entry, so a policy without `env` refuses it and `audit` lists it.
#[test]
fn main_with_arguments_uses_env() {
    let root = scratch("arguments");
    write(
        &root,
        "Tsuzuri.toml",
        "[package]\nname = \"arguments\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = []\n",
    );
    write(
        &root,
        "Main.tz",
        "def main :: Array<string> -> i32 = \\args -> args.length as i32\n",
    );
    let errors = check_errors(&root);
    let [error] = errors.as_slice() else {
        panic!("{errors:?}");
    };
    assert_eq!(
        error.message,
        "the root package may not use capability 'env' ('the command-line arguments of main'); add \"env\" to allow in [capabilities]"
    );
    let output = tsuzuri(&["audit", "."], &root);
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains(
            "  used: env\n    env: 1 site\n      Main.tz:1:37  the command-line arguments of main\n"
        ),
        "{report}"
    );
    // `main` without the arguments reads nothing from the host.
    write(&root, "Main.tz", MAIN);
    assert!(check_errors(&root).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

/// Review of #65: an extern `main` is called by the entry that the compiler adds.
#[test]
fn an_extern_main_uses_ffi() {
    let root = scratch("extern-main");
    write(
        &root,
        "Tsuzuri.toml",
        "[package]\nname = \"externmain\"\nversion = \"0.1.0\"\n\n[capabilities]\nallow = []\n",
    );
    write(
        &root,
        "Main.tz",
        "extern \"host_main\" def main :: unit -> i32\n",
    );
    let errors = check_errors(&root);
    let [error] = errors.as_slice() else {
        panic!("{errors:?}");
    };
    assert_eq!(
        error.message,
        "the root package may not use capability 'ffi' ('main'); add \"ffi\" to allow in [capabilities]"
    );
    let output = tsuzuri(&["audit", "."], &root);
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("  used: ffi\n    ffi: 1 site\n      Main.tz:1:24  main\n"),
        "{report}"
    );
    assert!(
        report.contains("summary: used=ffi denied=1 untrusted_externs=1"),
        "{report}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning[W1010]: extern function 'main' is used but not marked '@trusted'"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Review of #65: only `capabilities` may follow `native` in a path dependency.
#[test]
fn a_dependency_key_after_native_must_be_capabilities() {
    let root = scratch("extra-key");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nhost = { path = \"../host\", native = true, capabilities_extra = [\"ffi\"] }\n",
    );
    write(&root, "app/Main.tz", MAIN);
    write(
        &root,
        "host/Tsuzuri.toml",
        "[package]\nname = \"host\"\nversion = \"0.1.0\"\n",
    );
    write(&root, "host/Lib.tz", "def one :: unit -> i64 = \\() -> 1\n");
    let output = tsuzuri(&["check", "."], &root.join("app"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr
            .contains("only the path, native, and capabilities keys are supported in a dependency"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(root);
}
