use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::driver::Project;
use tsuzuri::edition_migration::{apply, prepare, set_manifest_edition};
use tsuzuri::llvm::{self, Entry};
use tsuzuri::syntax::Edition;

struct Root(PathBuf);

impl Root {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/scratch/edition-migration")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        for (name, text) in files {
            let file = path.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        Self(path)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

const MANIFEST: &str = "[package]\nname = \"migration\"\nversion = \"0.1.0\"\nedition = \"2026\"\n";
const SOURCE: &str = "def take :: string -> string = \\text -> text\n\
    def apply :: ('a -> 'b) -> 'a -> 'b = \\f x -> f x\n\
    def identity :: 'a -> 'a = \\x -> x\n\
    export def answer :: i64 = (take (apply identity \"abc\")).length\n";

#[test]
fn preserves_value_signatures_callbacks_and_exact_ir() {
    let root = Root::new(&[("Tsuzuri.toml", MANIFEST), ("Main.tz", SOURCE)]);
    let emit = || {
        let module = Project::load(&root.0).unwrap().analyze().unwrap();
        llvm::emit(&module, Entry::Library).unwrap()
    };
    let before = emit();
    let migration = prepare(&root.0, Edition::E2027, None).unwrap();
    assert!(migration.manifest);
    assert_eq!(root.read("Main.tz"), SOURCE);
    assert_eq!(root.read("Tsuzuri.toml"), MANIFEST);
    apply(&migration).unwrap();
    assert!(root.read("Main.tz").contains("own string"));
    assert!(root.read("Main.tz").contains("own ('a -> 'b)"));
    assert!(root.read("Main.tz").contains("own 'a"));
    assert!(root.read("Tsuzuri.toml").contains("\"2027\""));
    assert_eq!(emit(), before);
    assert!(
        prepare(&root.0, Edition::E2027, None)
            .unwrap()
            .changes
            .is_empty()
    );
}

#[test]
fn file_scope_and_dependencies_are_never_upgraded_implicitly() {
    let manifest = format!("{MANIFEST}\n[dependencies]\ndep = {{ path = \"dependency\" }}\n");
    let dependency = MANIFEST.replace("migration", "dep");
    let root = Root::new(&[
        ("Tsuzuri.toml", &manifest),
        ("Main.tz", SOURCE),
        (
            "Other.tz",
            "def size :: string -> i64 = \\text -> text.length\n",
        ),
        ("dependency/Tsuzuri.toml", &dependency),
        (
            "dependency/Main.tz",
            "def take :: string -> string = \\text -> text\n",
        ),
    ]);
    let migration = prepare(&root.0.join("Main.tz"), Edition::E2027, None).unwrap();
    assert!(!migration.manifest);
    assert_eq!(migration.changes.len(), 1);
    apply(&migration).unwrap();
    assert_eq!(root.read("Tsuzuri.toml"), manifest);
    assert!(!root.read("Other.tz").contains("own"));
    let migration = prepare(&root.0, Edition::E2027, None).unwrap();
    apply(&migration).unwrap();
    assert!(root.read("Other.tz").contains("own string"));
    assert_eq!(root.read("dependency/Tsuzuri.toml"), dependency);
    assert!(!root.read("dependency/Main.tz").contains("own"));
}

#[test]
fn rejects_old_errors_downgrades_and_inconsistent_from_without_writes() {
    let root = Root::new(&[
        ("Tsuzuri.toml", MANIFEST),
        ("Main.tz", "def broken :: i64 = true\n"),
    ]);
    assert_eq!(
        prepare(&root.0, Edition::E2027, None)
            .unwrap_err()
            .diagnostic
            .code,
        "E1003"
    );
    assert!(prepare(&root.0, Edition::E2027, Some(Edition::E2027)).is_err());
    assert_eq!(root.read("Tsuzuri.toml"), MANIFEST);
    assert_eq!(root.read("Main.tz"), "def broken :: i64 = true\n");
    std::fs::write(
        root.0.join("Tsuzuri.toml"),
        MANIFEST.replace("2026", "2027"),
    )
    .unwrap();
    assert!(prepare(&root.0, Edition::E2026, None).is_err());
}

#[test]
fn failed_replacement_preserves_concurrent_edits_and_rolls_back_earlier_files() {
    let root = Root::new(&[
        ("Tsuzuri.toml", MANIFEST),
        ("Main.tz", SOURCE),
        (
            "Other.tz",
            "def size :: string -> i64 = \\text -> text.length\n",
        ),
    ]);
    let migration = prepare(&root.0, Edition::E2027, None).unwrap();
    std::fs::write(root.0.join("Other.tz"), "// concurrent edit\n").unwrap();
    assert_eq!(apply(&migration).unwrap_err().diagnostic.code, "E2008");
    assert_eq!(root.read("Main.tz"), SOURCE);
    assert_eq!(root.read("Other.tz"), "// concurrent edit\n");
    assert_eq!(root.read("Tsuzuri.toml"), MANIFEST);
}

#[test]
fn unversioned_sources_need_an_explicit_old_edition() {
    let root = Root::new(&[("Main.tz", SOURCE)]);
    let migration = prepare(&root.0, Edition::E2027, Some(Edition::E2026)).unwrap();
    assert!(!migration.manifest);
    apply(&migration).unwrap();
    assert!(root.read("Main.tz").contains("own string"));
}

#[test]
fn edits_only_the_package_edition_and_preserves_comments_and_line_endings() {
    let text = "[package] # root\r\nname=\"p\"\r\nversion=\"1.0.0\"\r\n  edition = \"2026\" # keep\r\n[other]\r\nedition=\"wrong\"\r\n";
    assert_eq!(
        set_manifest_edition(text, Edition::E2027).unwrap(),
        text.replacen("\"2026\"", "\"2027\"", 1)
    );
    for newline in ["\n", "\r\n"] {
        let text = format!("[package]{newline}version=\"1.0.0\"{newline}[dependencies]{newline}");
        let changed = set_manifest_edition(&text, Edition::E2027).unwrap();
        assert!(changed.contains(&format!(
            "version=\"1.0.0\"{newline}edition = \"2027\"{newline}[dependencies]"
        )));
        assert_eq!(
            set_manifest_edition(&changed, Edition::E2027).unwrap(),
            changed
        );
    }
    assert_eq!(
        set_manifest_edition("[package]\nversion=\"1.0.0\"", Edition::E2027).unwrap(),
        "[package]\nversion=\"1.0.0\"\nedition = \"2027\""
    );
    assert!(set_manifest_edition("[other]\nversion=\"1.0.0\"\n", Edition::E2027).is_none());
    assert!(set_manifest_edition("[package]\nedition=2026\n", Edition::E2027).is_none());
}

#[test]
fn cli_dry_run_json_is_nonmutating_and_bad_arguments_are_rejected() {
    let root = Root::new(&[("Tsuzuri.toml", MANIFEST), ("Main.tz", SOURCE)]);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["fix", "--edition", "2027", "--dry-run", "--json"])
        .arg(&root.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.last().unwrap()["type"], "edition_summary");
    assert_eq!(rows.last().unwrap()["dry_run"], true);
    assert_eq!(root.read("Main.tz"), SOURCE);
    assert_eq!(root.read("Tsuzuri.toml"), MANIFEST);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "fix",
            "--edition",
            "2027",
            "--from",
            "2026",
            "--from",
            "2026",
        ])
        .arg(&root.0)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
