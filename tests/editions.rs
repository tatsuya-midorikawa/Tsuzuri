use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::check::ModuleOrigin;
use tsuzuri::driver::Project;
use tsuzuri::lexer::{self, EDITION_KEYWORDS};
use tsuzuri::syntax::{Edition, TokenKind};

/// A fresh root with the given files, each at a path below it.
fn root(label: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-editions-{label}-{}-{}",
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

fn tsuzuri(arguments: &[&std::ffi::OsStr]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

const APP: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n[dependencies]\nhelper = { path = \"../helper\" }\n";
const HELPER: &str = "[package]\nname = \"helper\"\nversion = \"0.1.0\"\n";

#[test]
fn edition_names_parse_exactly() {
    assert_eq!(Edition::parse("2026"), Some(Edition::E2026));
    for text in [
        "",
        " 2026",
        "2026 ",
        "2030",
        "02026",
        "2026.0",
        "edition 2026",
    ] {
        assert_eq!(Edition::parse(text), None, "{text:?}");
    }
    assert_eq!(Edition::E2026.to_string(), "2026");
    assert_eq!(Edition::SUPPORTED, [Edition::E2026]);
    assert!(Edition::SUPPORTED.contains(&Edition::LATEST));
    // A manifest without `edition` keeps meaning what it meant, whatever the latest edition is.
    assert_eq!(Edition::MANIFEST_DEFAULT, Edition::E2026);
}

#[test]
fn manifest_edition_selects_the_source_edition() {
    let root = root(
        "select",
        &[
            ("app/Tsuzuri.toml", APP),
            (
                "app/Main.tz",
                "def main :: unit -> i32 = \\() -> (Helper::Util.answer ()) as i32\n",
            ),
            ("helper/Tsuzuri.toml", HELPER),
            ("helper/Util.tz", "def answer :: unit -> i64 = \\() -> 42\n"),
        ],
    );
    let project = Project::load(&root.join("app")).unwrap();
    assert!(project.sources.iter().any(|source| source.name == "Main"));
    for source in &project.sources {
        // The helper has no `edition`, so it takes the default; std takes the latest.
        let expected = match source.origin {
            ModuleOrigin::User => Edition::MANIFEST_DEFAULT,
            ModuleOrigin::Std => Edition::LATEST,
        };
        assert_eq!(source.edition, expected, "{}", source.path.display());
    }
    assert!(
        project
            .sources
            .iter()
            .any(|source| source.origin == ModuleOrigin::Std)
    );
    assert!(
        project
            .manifests
            .iter()
            .all(|manifest| manifest.edition == Edition::E2026)
    );
    project.analyze_all().unwrap();
    // The same sources are read the same way for tests and documentation.
    for project in [
        Project::load_for_tests(&root.join("app")).unwrap(),
        Project::load_for_docs(&root.join("app")).unwrap(),
    ] {
        assert!(
            project
                .sources
                .iter()
                .all(|source| source.edition == Edition::E2026)
        );
    }
    // A project without a manifest, and a single file, are read with the latest edition.
    let loose = self::root("loose", &[("Main.tz", "42\n")]);
    for project in [
        Project::load(&loose).unwrap(),
        Project::load(&loose.join("Main.tz")).unwrap(),
    ] {
        assert!(project.manifests.is_empty());
        assert!(
            project
                .sources
                .iter()
                .all(|source| source.edition == Edition::LATEST)
        );
        project.analyze_all().unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(loose).unwrap();
}

#[test]
fn rejects_unknown_and_unsupported_editions() {
    let main = "42\n";
    for (keys, message) in [
        (
            "edition = \"2030\"\n",
            "unsupported edition '2030'; supported editions: 2026",
        ),
        (
            "edition = \"latest\"\n",
            "unsupported edition 'latest'; supported editions: 2026",
        ),
        ("edition = 2026\n", "expected '\"'"),
        (
            "edition = \"\"\n",
            "package fields must be nonempty and occur exactly once",
        ),
        (
            "edition = \"2026\"\nedition = \"2026\"\n",
            "package fields must be nonempty and occur exactly once",
        ),
        (
            "license = \"MIT\"\n",
            "unknown package key; expected name, version, namespace, or edition",
        ),
    ] {
        let manifest = format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{keys}");
        let root = root("reject", &[("Tsuzuri.toml", &manifest), ("Main.tz", main)]);
        let error = Project::load(&root).unwrap_err();
        assert_eq!(error.diagnostic.code, "E0002", "{keys}");
        assert_eq!(error.diagnostic.message, message, "{keys}");
        assert!(error.path.ends_with("Tsuzuri.toml"), "{keys}");
        // The CLI reports it too, and `check` does not touch anything.
        let (status, _, stderr) = tsuzuri(&["check".as_ref(), root.as_os_str()]);
        assert_eq!(status, Some(1), "{keys}");
        assert!(stderr.contains("error[E0002]"), "{stderr}");
        assert!(stderr.contains(message), "{stderr}");
        std::fs::remove_dir_all(root).unwrap();
    }
    // A dependency's manifest is read with the same rule, and the error names its file.
    let root = root(
        "dependency",
        &[
            ("app/Tsuzuri.toml", APP),
            ("app/Main.tz", main),
            (
                "helper/Tsuzuri.toml",
                "[package]\nname = \"helper\"\nversion = \"0.1.0\"\nedition = \"2030\"\n",
            ),
            ("helper/Util.tz", "def answer :: i64 = 1\n"),
        ],
    );
    let error = Project::load(&root.join("app")).unwrap_err();
    assert_eq!(error.diagnostic.code, "E0002");
    assert!(
        error
            .diagnostic
            .message
            .starts_with("unsupported edition '2030'")
    );
    assert!(
        error.path.ends_with("helper/Tsuzuri.toml"),
        "{}",
        error.path.display()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn edition_keywords_are_consistent_with_the_supported_editions() {
    for &(word, since) in EDITION_KEYWORDS {
        assert!(Edition::SUPPORTED.contains(&since), "{word}");
        // From its edition on the word is a keyword; before it, an identifier.
        let kind = |edition| lexer::lex(word, edition).unwrap().remove(0).kind;
        assert!(!matches!(kind(since), TokenKind::Ident(_)), "{word}");
        for &older in Edition::SUPPORTED.iter().filter(|older| **older < since) {
            assert!(
                matches!(kind(older), TokenKind::Ident(_)),
                "{word} in {older}"
            );
        }
    }
    // Words that exist today are reserved by every edition: `dyn` and `bench` belong to 2026.
    for word in ["dyn", "bench", "def", "union", "test"] {
        for &edition in Edition::SUPPORTED {
            let tokens = lexer::lex(word, edition).unwrap();
            assert!(!matches!(tokens[0].kind, TokenKind::Ident(_)), "{word}");
        }
    }
}

#[test]
fn fmt_uses_the_manifest_edition() {
    let messy = "def  answer::i64=\\()->42\n";
    let tidy =
        tsuzuri::formatter::format_source("Main.tz", messy, tsuzuri::syntax::SourceKind::Code)
            .unwrap()
            .formatted;
    assert_ne!(tidy, messy);
    let manifest =
        |edition: &str| format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{edition}");
    // The manifest of an unsupported edition stops fmt, and the file stays as it is.
    let root = root(
        "fmt",
        &[
            ("Tsuzuri.toml", &manifest("edition = \"2030\"\n")),
            ("Main.tz", messy),
        ],
    );
    for input in [root.clone(), root.join("Main.tz")] {
        let (status, _, stderr) = tsuzuri(&["fmt".as_ref(), "--check".as_ref(), input.as_os_str()]);
        assert_eq!(status, Some(1), "{stderr}");
        assert!(stderr.contains("Tsuzuri.toml"), "{stderr}");
        assert!(stderr.contains("error[E0002]"), "{stderr}");
        assert!(stderr.contains("unsupported edition '2030'"), "{stderr}");
        assert_eq!(
            std::fs::read_to_string(root.join("Main.tz")).unwrap(),
            messy
        );
    }
    // Supported editions and an absent key format as usual, for a directory and for a file.
    for keys in ["edition = \"2026\"\n", ""] {
        std::fs::write(root.join("Tsuzuri.toml"), manifest(keys)).unwrap();
        for input in [root.clone(), root.join("Main.tz")] {
            std::fs::write(root.join("Main.tz"), messy).unwrap();
            let (status, _, stderr) =
                tsuzuri(&["fmt".as_ref(), "--check".as_ref(), input.as_os_str()]);
            assert_eq!(status, Some(1), "{stderr}");
            assert!(stderr.contains("not formatted"), "{stderr}");
            let (status, _, stderr) = tsuzuri(&["fmt".as_ref(), input.as_os_str()]);
            assert_eq!(status, Some(0), "{stderr}");
            assert_eq!(std::fs::read_to_string(root.join("Main.tz")).unwrap(), tidy);
        }
    }
    // Nothing above the input is searched: a file in a subdirectory has no manifest of its own.
    std::fs::write(root.join("Tsuzuri.toml"), manifest("edition = \"2030\"\n")).unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/Main.tz"), messy).unwrap();
    let (status, _, stderr) = tsuzuri(&["fmt".as_ref(), root.join("sub/Main.tz").as_os_str()]);
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(root.join("sub/Main.tz")).unwrap(),
        tidy
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_projects_declare_the_latest_edition() {
    let root = root("new", &[]);
    let project = root.join("hello-world");
    let (status, _, stderr) = tsuzuri(&["new".as_ref(), project.as_os_str()]);
    assert_eq!(status, Some(0), "{stderr}");
    let manifest = std::fs::read_to_string(project.join("Tsuzuri.toml")).unwrap();
    assert!(
        manifest.contains(&format!("edition = \"{}\"\n", Edition::LATEST)),
        "{manifest}"
    );
    let (status, _, stderr) = tsuzuri(&["check".as_ref(), project.as_os_str()]);
    assert_eq!(status, Some(0), "{stderr}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_edition_key_changes_no_generated_code() {
    let main = "def main :: unit -> i32 = \\() -> (Helper::Util.answer ()) as i32\n";
    let util = "def answer :: unit -> i64 = \\() -> 42\n";
    let manifest = |edition: &str| {
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n{edition}[dependencies]\nhelper = {{ path = \"../helper\" }}\n"
        )
    };
    let build = |label: &str, edition: &str, target: &str| {
        let root = root(
            label,
            &[
                ("app/Tsuzuri.toml", &manifest(edition)),
                ("app/Main.tz", main),
                ("helper/Tsuzuri.toml", HELPER),
                ("helper/Util.tz", util),
            ],
        );
        let output: &Path = &root.join("out.ll");
        let (status, _, stderr) = tsuzuri(&[
            "build".as_ref(),
            root.join("app").as_os_str(),
            "--emit".as_ref(),
            "llvm".as_ref(),
            "--target".as_ref(),
            target.as_ref(),
            "-o".as_ref(),
            output.as_os_str(),
        ]);
        assert_eq!(status, Some(0), "{stderr}");
        let ir = std::fs::read_to_string(output).unwrap();
        std::fs::remove_dir_all(root).unwrap();
        ir
    };
    for target in ["native", "wasm32"] {
        let explicit = build("explicit", "edition = \"2026\"\n", target);
        let implicit = build("implicit", "", target);
        assert!(explicit.contains("define"), "{target}");
        assert_eq!(explicit, implicit, "{target}");
    }
}
