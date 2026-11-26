//! NC17 Phase 2: edition 2028 canonical rules and `tsuzuri fix --edition 2028` migrations.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::parser::parse_with_source_all;
use tsuzuri::syntax::Edition;

const REJECTED_IN_2028: [&str; 7] = [
    "let a = 86uy",
    "let a = 1.5f",
    "let r = &x",
    "let r = &mut y",
    "let r = ref x\nlet a = *r + 1",
    "let g = \\(r: &i64) -> deref r",
    "let g = a -> b -> a + b",
];

fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/edition-2028")
        .join(format!(
            "{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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

fn tsuzuri(arguments: &[&str], directory: &Path, cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(directory)
        .env("TSUZURI_CACHE_DIR", cache)
        .output()
        .unwrap()
}

#[test]
fn edition_2028_rejects_five_legacy_forms_and_accepts_bare_for_range() {
    for body in REJECTED_IN_2028 {
        let text = format!("let x = 1\nlet mut y = 2\n{body}\n0\n");
        let findings = parse_with_source_all(&text, 0, Edition::E2026)
            .unwrap()
            .noncanonical;
        assert_eq!(findings.len(), 1, "{body}");
        let errors = parse_with_source_all(&text, 0, Edition::E2028).unwrap_err();
        assert_eq!(errors.len(), 1, "{body}");
        assert_eq!(errors[0].code, "E0004", "{body}");
        assert_eq!(
            errors[0].message,
            format!(
                "'{}' is not allowed in edition 2028; write '{}'",
                findings[0].spelling, findings[0].canonical
            )
        );
    }

    // Bare `..` in `for` is canonical again in edition 2028.
    let ok_text = "let mut s = 0\nfor i in 1 .. 3 do\n    s = s + i\nfor j in 1 .. 2 .. 5 do\n    s = s + j\ns\n";
    let parsed = parse_with_source_all(ok_text, 0, Edition::E2028).unwrap();
    assert!(parsed.noncanonical.is_empty());
    let checked = tsuzuri::analyze_with_edition(ok_text, Edition::E2028).unwrap();
    assert!(checked.warnings.is_empty());
}

#[test]
fn fix_2027_to_2028_adds_no_own() {
    let root = scratch("from-2027");
    let cache = root.join("cache");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def len_and_sub :: string -> [i64] -> i64 = \\s xs ->\n\
         \x20   s.length + (ref xs[0..2]).length + (ref xs[..1]).length\n\
         export def entry :: i64 = len_and_sub \"hello\" [10, 20, 30]\n",
    );
    let before = tsuzuri(
        &["build", "app", "--emit", "llvm", "-O0", "-o", "before.ll"],
        &root,
        &cache,
    );
    assert!(
        before.status.success(),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let before_ir = std::fs::read_to_string(root.join("before.ll")).unwrap();

    let migrate = tsuzuri(
        &["fix", "--edition", "2028", "--from", "2027", "app"],
        &root,
        &cache,
    );
    assert!(
        migrate.status.success(),
        "{}",
        String::from_utf8_lossy(&migrate.stderr)
    );

    let migrated = std::fs::read_to_string(root.join("app/Main.tz")).unwrap();
    assert!(
        !migrated.contains("own "),
        "2027 -> 2028 must not add 'own': {migrated}"
    );
    assert!(migrated.contains("ref xs[0..<2]"), "{migrated}");
    assert!(migrated.contains("ref xs[..<1]"), "{migrated}");

    let after = tsuzuri(
        &["build", "app", "--emit", "llvm", "-O0", "-o", "after.ll"],
        &root,
        &cache,
    );
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("after.ll")).unwrap(),
        before_ir
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fix_2026_to_2028_keeps_the_bare_for_range() {
    let root = scratch("from-2026");
    let cache = root.join("cache");
    write(
        &root,
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    write(
        &root,
        "app/Main.tz",
        "def consume :: string -> i64 = \\s -> s.length\n\
         export def total :: ref [i64] -> i64 = \\xs ->\n\
         \x20   let mut sum = 0i64\n\
         \x20   for i in 1 .. 3 do\n\
         \x20       sum = sum + (i as i64)\n\
         \x20   sum + consume \"hi\" + (ref xs[0..2]).length\n",
    );
    let before = tsuzuri(
        &["build", "app", "--emit", "llvm", "-O0", "-o", "before.ll"],
        &root,
        &cache,
    );
    assert!(
        before.status.success(),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let before_ir = std::fs::read_to_string(root.join("before.ll")).unwrap();

    let migrate = tsuzuri(
        &["fix", "--edition", "2028", "--from", "2026", "app"],
        &root,
        &cache,
    );
    assert!(
        migrate.status.success(),
        "{}",
        String::from_utf8_lossy(&migrate.stderr)
    );

    let migrated = std::fs::read_to_string(root.join("app/Main.tz")).unwrap();
    assert!(
        migrated.contains("for i in 1 .. 3 do"),
        "bare for range kept: {migrated}"
    );
    assert!(
        migrated.contains("ref xs[0..<2]"),
        "bare slice migrated to ..<: {migrated}"
    );

    let after = tsuzuri(
        &["build", "app", "--emit", "llvm", "-O0", "-o", "after.ll"],
        &root,
        &cache,
    );
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("after.ll")).unwrap(),
        before_ir
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn corpus_2027_migrates_to_2028_without_changing_the_ir() {
    let root = scratch("corpus");
    let cache = root.join("cache");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/canonical_2026");
    std::fs::create_dir_all(root.join("app")).unwrap();
    for file in ["Tsuzuri.toml", "Main.tz"] {
        std::fs::copy(fixture.join(file), root.join("app").join(file)).unwrap();
    }
    let build_ir = |name: &str| {
        let out = tsuzuri(
            &["build", "app", "--emit", "llvm", "-O0", "-o", name],
            &root,
            &cache,
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::read_to_string(root.join(name)).unwrap()
    };
    let ir_2026 = build_ir("2026.ll");
    let to_2027 = tsuzuri(
        &["fix", "--edition", "2027", "--from", "2026", "app"],
        &root,
        &cache,
    );
    assert!(
        to_2027.status.success(),
        "{}",
        String::from_utf8_lossy(&to_2027.stderr)
    );
    assert_eq!(build_ir("2027.ll"), ir_2026);

    let to_2028 = tsuzuri(
        &["fix", "--edition", "2028", "--from", "2027", "app"],
        &root,
        &cache,
    );
    assert!(
        to_2028.status.success(),
        "{}",
        String::from_utf8_lossy(&to_2028.stderr)
    );
    assert_eq!(build_ir("2028.ll"), ir_2026);
    let manifest = std::fs::read_to_string(root.join("app/Tsuzuri.toml")).unwrap();
    assert!(manifest.contains("edition = \"2028\""), "{manifest}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn manifest_less_sources_note_the_meaning_change() {
    use std::io::Cursor;

    let root = scratch("meaning-changed");
    let cache = root.join("cache");
    write(
        &root,
        "loose/Main.tz",
        "let xs = [10, 20, 30, 40]\nlet sub = ref xs[1..2]\nsub.length\n",
    );

    let chk = tsuzuri(&["check", "loose", "--json"], &root, &cache);
    assert!(
        chk.status.success(),
        "{}",
        String::from_utf8_lossy(&chk.stderr)
    );
    let stderr = String::from_utf8_lossy(&chk.stderr);
    assert!(stderr.contains("\"code\":\"W1013\""), "{stderr}");
    assert!(
        stderr.contains("\"kind\":\"bare_slice_range_meaning_changed\""),
        "{stderr}"
    );
    assert!(!stderr.contains("\"fixes\""), "{stderr}");

    let run = tsuzuri(&["run", "loose"], &root, &cache);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "2");
    assert!(String::from_utf8_lossy(&run.stderr).contains("warning[W1013]"));

    let script = tsuzuri(&["script", "loose/Main.tz"], &root, &cache);
    assert!(
        script.status.success(),
        "{}",
        String::from_utf8_lossy(&script.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&script.stdout).trim(), "2");
    assert!(String::from_utf8_lossy(&script.stderr).contains("warning[W1013]"));

    let explicit_flag = tsuzuri(&["check", "loose", "--edition", "2028"], &root, &cache);
    assert!(explicit_flag.status.success());
    assert!(
        explicit_flag.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&explicit_flag.stderr)
    );

    write(
        &root,
        "explicit/Main.tz",
        "let xs = [10, 20, 30, 40]\n(ref xs[1..=2]).length + (ref xs[1..<2]).length\n",
    );
    let explicit_ops = tsuzuri(&["check", "explicit"], &root, &cache);
    assert!(explicit_ops.status.success());
    assert!(
        explicit_ops.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&explicit_ops.stderr)
    );

    write(
        &root,
        "loose/Tsuzuri.toml",
        "[package]\nname = \"loose\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    );
    let with_manifest = tsuzuri(&["check", "loose"], &root, &cache);
    assert!(with_manifest.status.success());
    assert!(
        with_manifest.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&with_manifest.stderr)
    );

    let (mut repl_out, mut repl_err) = (Vec::new(), Vec::new());
    tsuzuri::repl::run(
        tsuzuri::repl::ReplOptions::default(),
        Cursor::new(
            "let xs = [10, 20, 30, 40]\n(ref xs[1..2]).length\n(ref xs[0..xs.length]).length\n(ref xs[1..<2]).length\n:quit\n",
        ),
        &mut repl_out,
        &mut repl_err,
        false,
    )
    .unwrap();
    let repl_stdout = String::from_utf8(repl_out).unwrap();
    let repl_stderr = String::from_utf8(repl_err).unwrap();
    assert!(repl_stdout.contains("it: i64 = 2"), "{repl_stdout}");
    assert!(repl_stdout.contains("it: i64 = 1"), "{repl_stdout}");
    // Both `(ref xs[1..2]).length` and the trapping `(ref xs[0..xs.length]).length` emit W1013
    assert_eq!(
        repl_stderr.matches("warning[W1013]").count(),
        2,
        "{repl_stderr}"
    );

    // Saturating MAX_FINDINGS (65,536) with warning-only W1013 entries must never suppress E0004
    let mut compact = String::with_capacity(800_000);
    compact.push_str("let x = [0, 1]\n");
    for _ in 0..65_536 {
        compact.push_str("ref x[0..1];");
    }
    compact.push_str("()\n86uy\n");
    let err = tsuzuri::parser::parse_with_source(&compact, 0, tsuzuri::syntax::Edition::E2028)
        .expect_err("saturated W1013 must not drop E0004");
    assert_eq!(err.code, "E0004", "{err:?}");

    // Saturating MAX_FINDINGS with 131,072 rejected findings stays linear-time (no O(N^2) rescan)
    let mut all_rejected = String::with_capacity(660_000);
    for _ in 0..131_072 {
        all_rejected.push_str("86uy;");
    }
    all_rejected.push_str("0\n");
    let err2 =
        tsuzuri::parser::parse_with_source(&all_rejected, 0, tsuzuri::syntax::Edition::E2028)
            .expect_err("saturated rejected findings must report E0004 quickly");
    assert_eq!(err2.code, "E0004", "{err2:?}");

    let _ = std::fs::remove_dir_all(&root);
}
