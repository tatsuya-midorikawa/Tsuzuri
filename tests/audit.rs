//! E15 Phase 3: `tsuzuri audit`. The expected reports are appendix D of the ticket, whose
//! positions were counted in the files; Phase 4 added the `effects` key to the package rows.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-audit-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn audit(arguments: &[&str], directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("audit")
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// Appendix D.4: a root package with a policy and a dependency that reads the clock.
fn d4(root: &Path) -> PathBuf {
    write(
        root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nfscache = { path = \"../fscache\", capabilities = [\"fs\"] }\n\n[capabilities]\nallow = [\"fs\", \"stdio\"]\n",
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

/// Appendix D.5: an extern function without a manifest.
fn d5(root: &Path, mark: &str) -> PathBuf {
    write(
        root,
        "ffi/Main.tz",
        &format!(
            "{mark}extern \"abs\" def c_abs :: i32 -> i32\n\ndef main :: unit -> i32 = \\() -> c_abs (0 - 5)\n"
        ),
    );
    root.join("ffi")
}

const NO_EFFECTS: &str = "\"effects\":{\"declared\":1,\"asserted\":{\"pure\":0,\"no_alloc\":0,\"no_copy\":0,\"no_trap\":0,\"terminates\":0}}";

#[test]
fn audit_text_matches_the_examples() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = audit(&["examples/io"], repository);
    assert_eq!(
        stdout(&io),
        "audit: io\npolicy: none\npackages: 1\n\npackage io (root)\n  allowed: all\n  used: stdio\n    stdio: 3 sites\n      Main.tz:2:9  IO.write\n      Main.tz:3:17  IO.read_line\n      Main.tz:5:9  IO.write_line\n  denied: none\n  externs: none\n\nsummary: used=stdio denied=0 untrusted_externs=0\n"
    );
    let hello = audit(&["examples/hello"], repository);
    assert_eq!(
        stdout(&hello),
        "audit: hello\npolicy: none\npackages: 1\n\npackage hello (root)\n  allowed: all\n  used: stdio\n    stdio: 1 site\n      Main.tz:11:9  IO.write_line\n  denied: none\n  externs: none\n\nsummary: used=stdio denied=0 untrusted_externs=0\n"
    );
    let tasks = audit(&["examples/tasks"], repository);
    assert_eq!(
        stdout(&tasks),
        "audit: tasks\npolicy: none\npackages: 1\n\npackage tasks (root)\n  allowed: all\n  used: none\n  denied: none\n  externs: none\n\nsummary: used=none denied=0 untrusted_externs=0\n"
    );
    let root = scratch("text");
    d4(&root);
    let app = audit(&["app"], &root);
    assert_eq!(
        stdout(&app),
        "audit: app 0.1.0\npolicy: enforced\npackages: 2\n\n\
         package app 0.1.0 (root)\n  allowed: fs stdio\n  used: stdio\n    stdio: 1 site\n      Main.tz:3:9  IO.write_line\n  denied: none\n  unused: fs\n  externs: none\n\n\
         package fscache 0.2.0 (dependency)\n  allowed: fs\n  granted: app: fs\n  used: fs time\n    fs: 1 site\n      Cache.tz:1:63  File.read_text\n    time: 1 site\n      Cache.tz:3:57  Time.monotonic_ns\n  denied: time\n  unused: none\n  externs: none\n\n\
         summary: used=fs,stdio,time denied=1 untrusted_externs=0\n"
    );
    let stderr = String::from_utf8_lossy(&app.stderr);
    assert!(
        stderr.contains("fscache/Cache.tz:3:57: error[E1036]: package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); the root [capabilities] allow does not list \"time\""),
        "{stderr}"
    );
    d5(&root, "");
    let ffi = audit(&["ffi"], &root);
    assert_eq!(
        stdout(&ffi),
        "audit: ffi\npolicy: none\npackages: 1\n\npackage ffi (root)\n  allowed: all\n  used: ffi\n    ffi: 1 site\n      Main.tz:3:34  c_abs\n  denied: none\n  externs: 1 (0 trusted)\n    c_abs -> \"abs\"  Main.tz:1:18  untrusted\n\nsummary: used=ffi denied=0 untrusted_externs=1\n"
    );
    let stderr = String::from_utf8_lossy(&ffi.stderr);
    assert!(
        stderr.starts_with("ffi/Main.tz:1:18: warning[W1010]: extern function 'c_abs' is used but not marked '@trusted'"),
        "{stderr}"
    );
    d5(&root, "@trusted pure\n");
    let trusted = audit(&["ffi"], &root);
    assert!(
        stdout(&trusted).contains("    c_abs -> \"abs\"  Main.tz:2:18  trusted pure\n"),
        "{}",
        stdout(&trusted)
    );
    assert!(stdout(&trusted).ends_with("untrusted_externs=0\n"));
    assert!(
        trusted.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&trusted.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn audit_json_matches_the_examples() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let io = audit(&["examples/io", "--json"], repository);
    assert_eq!(
        stdout(&io),
        format!(
            "{{\"type\":\"audit\",\"schema\":\"tz-audit/1\",\"root\":\"io\",\"policy\":\"none\",\"packages\":1}}\n\
             {{\"type\":\"package\",\"name\":\"io\",\"version\":null,\"root\":true,\"allowed\":[\"env\",\"ffi\",\"fs\",\"gpu\",\"net\",\"process\",\"random\",\"stdio\",\"time\"],\"granted\":[],\"used\":[{{\"capability\":\"stdio\",\"count\":3,\"sites\":[{{\"path\":\"Main.tz\",\"line\":2,\"column\":9,\"callee\":\"IO.write\",\"function\":\"Main.main\",\"scope\":\"program\"}},{{\"path\":\"Main.tz\",\"line\":3,\"column\":17,\"callee\":\"IO.read_line\",\"function\":\"Main.main\",\"scope\":\"program\"}},{{\"path\":\"Main.tz\",\"line\":5,\"column\":9,\"callee\":\"IO.write_line\",\"function\":\"Main.main\",\"scope\":\"program\"}}]}}],\"denied\":[],\"unused\":[],{NO_EFFECTS}}}\n\
             {{\"type\":\"summary\",\"packages\":1,\"used\":[\"stdio\"],\"denied\":0,\"untrusted_externs\":0,\"exit\":0}}\n"
        )
    );
    let tasks = audit(&["examples/tasks", "--json"], repository);
    let lines: Vec<&str> = std::str::from_utf8(&tasks.stdout)
        .unwrap()
        .lines()
        .collect();
    assert_eq!(lines.len(), 3);
    assert!(
        lines[1].contains("\"used\":[],\"denied\":[],\"unused\":[]"),
        "{}",
        lines[1]
    );
    let root = scratch("json");
    d4(&root);
    let app = audit(&["app", "--json"], &root);
    let text = stdout(&app);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "{\"type\":\"audit\",\"schema\":\"tz-audit/1\",\"root\":\"app\",\"policy\":\"enforced\",\"packages\":2}"
    );
    assert_eq!(
        lines[1],
        format!(
            "{{\"type\":\"package\",\"name\":\"app\",\"version\":\"0.1.0\",\"root\":true,\"allowed\":[\"fs\",\"stdio\"],\"granted\":[],\"used\":[{{\"capability\":\"stdio\",\"count\":1,\"sites\":[{{\"path\":\"Main.tz\",\"line\":3,\"column\":9,\"callee\":\"IO.write_line\",\"function\":\"Main.main\",\"scope\":\"program\"}}]}}],\"denied\":[],\"unused\":[\"fs\"],{NO_EFFECTS}}}"
        )
    );
    assert_eq!(
        lines[2],
        "{\"type\":\"package\",\"name\":\"fscache\",\"version\":\"0.2.0\",\"root\":false,\"allowed\":[\"fs\"],\"granted\":[{\"by\":\"app\",\"capabilities\":[\"fs\"]}],\"used\":[{\"capability\":\"fs\",\"count\":1,\"sites\":[{\"path\":\"Cache.tz\",\"line\":1,\"column\":63,\"callee\":\"File.read_text\",\"function\":\"Fscache.Cache.load\",\"scope\":\"program\"}]},{\"capability\":\"time\",\"count\":1,\"sites\":[{\"path\":\"Cache.tz\",\"line\":3,\"column\":57,\"callee\":\"Time.monotonic_ns\",\"function\":\"Fscache.Cache.stamp\",\"scope\":\"program\"}]}],\"denied\":[\"time\"],\"unused\":[],\"effects\":{\"declared\":2,\"asserted\":{\"pure\":0,\"no_alloc\":0,\"no_copy\":0,\"no_trap\":0,\"terminates\":0}}}"
    );
    assert_eq!(
        lines[3],
        "{\"type\":\"summary\",\"packages\":2,\"used\":[\"fs\",\"stdio\",\"time\"],\"denied\":1,\"untrusted_externs\":0,\"exit\":1}"
    );
    assert_eq!(lines.len(), 4);
    // The diagnostics are JSON on standard error.
    let stderr = String::from_utf8_lossy(&app.stderr);
    assert!(
        stderr.contains("\"code\":\"E1036\"") && stderr.contains("\"kind\":\"capability_denied\""),
        "{stderr}"
    );
    d5(&root, "");
    let ffi = audit(&["ffi", "--json"], &root);
    let text = stdout(&ffi);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[2],
        "{\"type\":\"extern\",\"package\":\"ffi\",\"name\":\"c_abs\",\"symbol\":\"abs\",\"path\":\"Main.tz\",\"line\":1,\"column\":18,\"trusted\":false,\"properties\":[],\"used\":1}"
    );
    assert_eq!(
        lines[3],
        "{\"type\":\"summary\",\"packages\":1,\"used\":[\"ffi\"],\"denied\":0,\"untrusted_externs\":1,\"exit\":0}"
    );
    d5(&root, "@trusted pure\n");
    let trusted = stdout(&audit(&["ffi", "--json"], &root));
    assert!(
        trusted.contains(
            "\"line\":2,\"column\":18,\"trusted\":true,\"properties\":[\"pure\"],\"used\":1}"
        ),
        "{trusted}"
    );
    // Phase 4: the package rows count the effect promises.
    write(
        &root,
        "promises/Main.tz",
        "pure def twice :: i64 -> i64 = \\x -> x * 2\n@no_alloc\n@terminates\ndef inc :: i64 -> i64 = \\x -> x + 1\ndef main :: unit -> i32 = \\() -> (twice (inc 1)) as i32\n",
    );
    let promises = stdout(&audit(&["promises", "--json"], &root));
    assert!(
        promises.contains("\"effects\":{\"declared\":3,\"asserted\":{\"pure\":1,\"no_alloc\":1,\"no_copy\":0,\"no_trap\":0,\"terminates\":1}}"),
        "{promises}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn audit_exit_codes() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = scratch("exit");
    d4(&root);
    d5(&root, "");
    write(
        &root,
        "broken/Main.tz",
        "def main :: unit -> i32 = \\() -> true\n",
    );
    let code = |arguments: &[&str], directory: &Path| audit(arguments, directory).status.code();
    assert_eq!(code(&["app"], &root), Some(1));
    assert_eq!(code(&["examples/io"], repository), Some(0));
    assert_eq!(code(&["ffi"], &root), Some(0));
    assert_eq!(code(&["ffi", "--deny-warnings"], &root), Some(1));
    let json = stdout(&audit(&["ffi", "--deny-warnings", "--json"], &root));
    assert!(
        json.ends_with("\"untrusted_externs\":1,\"exit\":1}\n"),
        "{json}"
    );
    let broken = audit(&["broken"], &root);
    assert_eq!(broken.status.code(), Some(1));
    assert!(broken.stdout.is_empty());
    for arguments in [
        vec!["--frobnicate"],
        vec!["a", "b"],
        vec!["--json", "--json"],
        vec!["--deny-warnings", "--deny-warnings"],
    ] {
        let output = audit(&arguments, &root);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("usage: tsuzuri audit [PATH] [--json] [--deny-warnings]"),
            "{arguments:?}"
        );
    }
    // Without PATH, the current directory.
    assert_eq!(code(&[], &root.join("ffi")), Some(0));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn audit_is_deterministic_and_path_free() {
    let first = scratch("place-a");
    let second = scratch("place-b");
    d4(&first);
    d4(&second);
    let reports: Vec<String> = [
        audit(&["app"], &first),
        audit(&["app"], &first),
        audit(&["app"], &second),
    ]
    .iter()
    .map(stdout)
    .collect();
    assert_eq!(reports[0], reports[1]);
    assert_eq!(reports[0], reports[2]);
    for root in [&first, &second] {
        assert!(!reports[0].contains(&*root.to_string_lossy()));
    }
    let json = stdout(&audit(&["app", "--json"], &first));
    assert!(!json.contains(&*first.to_string_lossy()));
    assert_eq!(json, stdout(&audit(&["app", "--json"], &second)));
    let _ = std::fs::remove_dir_all(first);
    let _ = std::fs::remove_dir_all(second);
}

#[test]
fn audit_reports_untrusted_externs_of_dependencies() {
    let root = scratch("dependency");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nabsval = { path = \"../absval\" }\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> Absval::Abs.of (0 - 5)\n",
    );
    write(
        &root,
        "absval/Tsuzuri.toml",
        "[package]\nname = \"absval\"\nversion = \"1.0.0\"\n",
    );
    write(
        &root,
        "absval/Abs.tz",
        "extern \"abs\" def c_abs :: i32 -> i32\n\ndef of :: i32 -> i32 = \\x -> c_abs x\n",
    );
    let output = audit(&["app"], &root);
    let text = stdout(&output);
    assert!(
        text.contains("package absval 1.0.0 (dependency)\n  allowed: all\n  used: ffi\n    ffi: 1 site\n      Abs.tz:3:30  c_abs\n"),
        "{text}"
    );
    assert!(
        text.contains("  externs: 1 (0 trusted)\n    c_abs -> \"abs\"  Abs.tz:1:18  untrusted\n"),
        "{text}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("absval/Abs.tz:1:18: warning[W1010]"),
        "{stderr}"
    );
    assert_eq!(output.status.code(), Some(0));
    let json = stdout(&audit(&["app", "--json"], &root));
    assert!(
        json.contains("{\"type\":\"extern\",\"package\":\"absval\",\"name\":\"c_abs\""),
        "{json}"
    );
    // `--warn untrusted-extern` reports only the root package's own declarations.
    let check = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["check", "app", "--warn", "untrusted-extern"])
        .current_dir(&root)
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    assert!(
        check.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}
