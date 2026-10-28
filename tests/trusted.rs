//! E15 Phase 1: `@trusted` on extern declarations, `E1037`, and `W1010` with
//! `--warn untrusted-extern`. Expected values follow appendix C of the ticket.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tsuzuri::analyze;
use tsuzuri::check::CheckedModule;

const MAIN: &str = "def main :: unit -> i32 = \\() -> c_abs (0 - 5)\n";

fn trusted_of(module: &CheckedModule, name: &str) -> Option<u8> {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("no function {name}"))
        .origin
        .trusted
}

fn rejects(source: &str, kind: &str) -> tsuzuri::diagnostic::Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1037", "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    error
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-trusted-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn tsuzuri(arguments: &[&str], directory: &Path, cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", cache)
        .output()
        .unwrap()
}

#[test]
fn trusted_marks_extern_functions_and_types() {
    let module = analyze(&format!(
        "@trusted\nextern \"abs\" def c_abs :: i32 -> i32\n\
         @trusted pure no_trap\nextern \"labs\" def c_labs :: i64 -> i64\n\
         extern \"llabs\" def c_llabs :: i64 -> i64\n\
         @trusted\nextern type Handle\n\
         {MAIN}"
    ))
    .unwrap();
    assert_eq!(trusted_of(&module, "c_abs"), Some(0));
    // `pure` is bit 0 and `no_trap` bit 3 (`TRUSTED_PROPERTIES`).
    assert_eq!(trusted_of(&module, "c_labs"), Some(0b01001));
    assert_eq!(trusted_of(&module, "c_llabs"), None);
    assert_eq!(trusted_of(&module, "main"), None);
    let program = tsuzuri::parser::parse(
        "@trusted terminates no_alloc\nprivate extern \"abs\" def c_abs :: i32 -> i32\n@trusted\nextern type Handle\n",
    )
    .unwrap();
    let marks: Vec<(&str, u8)> = program
        .trusted
        .iter()
        .map(|mark| (mark.target.text.as_str(), mark.properties))
        .collect();
    assert_eq!(marks, [("c_abs", 0b10010), ("Handle", 0)]);
}

#[test]
fn trusted_rejects_wrong_targets_unknown_and_duplicate_properties() {
    let span_text = |source: &str, error: &tsuzuri::diagnostic::Diagnostic| {
        source[error.span.start..error.span.end].to_owned()
    };
    let source = "@trusted\ndef f :: i32 -> i32 = \\x -> x\n";
    let error = rejects(source, "trusted_wrong_target");
    assert_eq!(
        error.message,
        "'@trusted' applies to an extern declaration; remove it or declare the function with 'extern'"
    );
    assert_eq!(span_text(source, &error), "@trusted");
    let source = "@trusted pure\nrecord R { x: i32 }\n";
    let error = rejects(source, "trusted_wrong_target");
    assert_eq!(span_text(source, &error), "@trusted pure");
    let source = "@trusted fast\nextern \"abs\" def c_abs :: i32 -> i32\n";
    let error = rejects(source, "trusted_unknown_property");
    assert_eq!(
        error.message,
        "unknown '@trusted' property 'fast'; use pure, no_alloc, no_copy, no_trap, or terminates"
    );
    assert_eq!(span_text(source, &error), "fast");
    let source = "@trusted pure no_trap pure\nextern \"abs\" def c_abs :: i32 -> i32\n";
    let error = rejects(source, "trusted_duplicate_property");
    assert_eq!(error.message, "'@trusted' lists 'pure' twice; remove one");
    assert_eq!(error.span.start, source.rfind("pure").unwrap());
    let source = "@trusted pure\nextern type Handle\n";
    let error = rejects(source, "trusted_type_property");
    assert_eq!(
        error.message,
        "'@trusted' on an extern type takes no properties; write '@trusted' alone"
    );
    assert_eq!(span_text(source, &error), "pure");
    let source = "@trusted\n@trusted\nextern \"abs\" def c_abs :: i32 -> i32\n";
    let error = rejects(source, "trusted_repeated");
    assert_eq!(
        error.message,
        "'@trusted' appears twice on one declaration; remove one"
    );
    assert_eq!(error.span.start, source.rfind('@').unwrap());
}

#[test]
fn trusted_survives_the_frontend_cache_round_trip() {
    let root = scratch("cache");
    let cache = root.join("cache");
    std::fs::write(
        root.join("Main.tz"),
        format!(
            "@trusted pure\nextern \"abs\" def c_abs :: i32 -> i32\nextern \"labs\" def c_labs :: i64 -> i64\n\
             def twice :: i64 -> i64 = \\x -> c_labs x\n{MAIN}"
        ),
    )
    .unwrap();
    let mut outputs = Vec::new();
    for _ in 0..2 {
        let output = tsuzuri(&["check", ".", "--warn", "untrusted-extern"], &root, &cache);
        assert!(output.status.success());
        outputs.push(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    assert!(
        cache.read_dir().unwrap().next().is_some(),
        "the frontend cache was not written"
    );
    assert_eq!(outputs[0], outputs[1]);
    assert!(
        outputs[1].contains("warning[W1010]: extern function 'c_labs'"),
        "{}",
        outputs[1]
    );
    assert!(!outputs[1].contains("'c_abs'"), "{}", outputs[1]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn formatter_keeps_trusted_attributes() {
    let root = scratch("fmt");
    let source = format!("@trusted pure no_trap\nextern \"abs\" def c_abs :: i32 -> i32\n\n{MAIN}");
    std::fs::write(root.join("Main.tz"), &source).unwrap();
    let check = tsuzuri(&["fmt", "--check", "Main.tz"], &root, &root.join("cache"));
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let format = tsuzuri(&["fmt", "Main.tz"], &root, &root.join("cache"));
    assert!(
        format.status.success(),
        "{}",
        String::from_utf8_lossy(&format.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("Main.tz")).unwrap(),
        source
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn untrusted_extern_warning_is_off_by_default_and_on_with_the_flag() {
    let root = scratch("warn");
    let cache = root.join("cache");
    let project = root.join("ffi");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("Main.tz"),
        format!("extern \"abs\" def c_abs :: i32 -> i32\n\n{MAIN}"),
    )
    .unwrap();
    let off = tsuzuri(&["check", "ffi"], &root, &cache);
    assert!(off.status.success());
    assert!(
        off.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&off.stderr)
    );
    let on = tsuzuri(
        &["check", "ffi", "--warn", "untrusted-extern"],
        &root,
        &cache,
    );
    assert!(on.status.success());
    let stderr = String::from_utf8_lossy(&on.stderr);
    assert!(
        stderr.starts_with(
            "ffi/Main.tz:1:18: warning[W1010]: extern function 'c_abs' is used but not marked '@trusted'; review its contract and add '@trusted' above its declaration\n"
        ),
        "{stderr}"
    );
    assert_eq!(stderr.matches("warning[W1010]").count(), 1, "{stderr}");
    let denied = tsuzuri(
        &[
            "check",
            "ffi",
            "--warn",
            "untrusted-extern",
            "--deny-warnings",
        ],
        &root,
        &cache,
    );
    assert_eq!(denied.status.code(), Some(1));
    let json = tsuzuri(
        &["check", "ffi", "--warn", "untrusted-extern", "--json"],
        &root,
        &cache,
    );
    let json = String::from_utf8_lossy(&json.stderr);
    assert!(
        json.contains("\"code\":\"W1010\"") && json.contains("\"kind\":\"untrusted_extern\""),
        "{json}"
    );
    // An extern that nothing uses is not reported.
    std::fs::write(
        project.join("Main.tz"),
        "extern \"abs\" def c_abs :: i32 -> i32\n\ndef main :: unit -> i32 = \\() -> 0\n",
    )
    .unwrap();
    let unused = tsuzuri(
        &["check", "ffi", "--warn", "untrusted-extern"],
        &root,
        &cache,
    );
    assert!(
        unused.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&unused.stderr)
    );
    std::fs::write(
        project.join("Main.tz"),
        format!("@trusted\nextern \"abs\" def c_abs :: i32 -> i32\n\n{MAIN}"),
    )
    .unwrap();
    let trusted = tsuzuri(
        &["check", "ffi", "--warn", "untrusted-extern"],
        &root,
        &cache,
    );
    assert!(trusted.status.success());
    assert!(
        trusted.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&trusted.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn trusted_pure_extern_may_be_called_from_a_pure_def() {
    let source = "@trusted pure\nextern \"abs\" def c_abs :: i32 -> i32\n\
                  pure def distance :: i32 -> i32 -> i32 = \\a b -> c_abs (a - b)\n\
                  def far :: i32 -> bool = \\x ->\n    @requires c_abs x < 100\n    x > 10\n\
                  def main :: unit -> i32 = \\() -> if far 50 then distance 3 7 else 0\n";
    analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    // Without the mark the same program breaks `pure` (A19) and the contract's purity (A21).
    let unmarked = source.replace("@trusted pure\n", "");
    let error = analyze(&unmarked).unwrap_err();
    assert_eq!(error.code, "E1029", "{}", error.message);
    let contract_only = unmarked.replace("pure def distance", "def distance");
    let error = analyze(&contract_only).unwrap_err();
    assert_eq!(
        (error.code, error.kind),
        ("E1033", Some("contract_impure")),
        "{}",
        error.message
    );
}

#[test]
fn trusted_no_alloc_does_not_allow_pure() {
    let source = "@trusted no_alloc no_trap terminates\nextern \"abs\" def c_abs :: i32 -> i32\n\
                  pure def distance :: i32 -> i32 -> i32 = \\a b -> c_abs (a - b)\n\
                  def main :: unit -> i32 = \\() -> distance 3 7\n";
    let error = analyze(source).unwrap_err();
    assert_eq!(error.code, "E1029", "{}", error.message);
    // The same properties satisfy the matching effect attributes.
    let attributed = "@trusted no_alloc no_trap terminates\nextern \"abs\" def c_abs :: i32 -> i32\n\
                      @no_alloc\n@no_trap\n@terminates\ndef distance :: i32 -> i32 -> i32 = \\a b -> c_abs (a - b)\n\
                      def main :: unit -> i32 = \\() -> distance 3 7\n";
    analyze(attributed).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
}

#[test]
fn untrusted_extern_still_breaks_no_trap() {
    for mark in ["", "@trusted\n", "@trusted pure no_alloc\n"] {
        let source = format!(
            "{mark}extern \"abs\" def c_abs :: i32 -> i32\n\
             @no_trap\ndef distance :: i32 -> i32 -> i32 = \\a b -> c_abs (a - b)\n\
             def main :: unit -> i32 = \\() -> distance 3 7\n"
        );
        let error = analyze(&source).unwrap_err();
        assert_eq!(error.code, "E1031", "{mark}: {}", error.message);
        assert!(
            error.message.contains("extern function 'Main.c_abs'"),
            "{}",
            error.message
        );
    }
}
