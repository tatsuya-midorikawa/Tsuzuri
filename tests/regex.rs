//! D09: the std `Regex` and `Unicode` modules.

use tsuzuri::check::CheckedModule;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::{analyze, analyze_modules, analyze_modules_with_std, llvm};

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects(source: &str, code: &str) -> Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error
}

/// The library IR of both targets, which must be deterministic with deduplicated declarations.
fn library(module: &CheckedModule) -> [String; 2] {
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap()
        );
        assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}");
        ir
    })
}

const ALL_APIS: &str = r#"
export def probe :: i64 -> i64
fn probe seed =
    let pattern = "(\\w+)@(\\w+)"
    let text = "x@y and ab@cd"
    let bytes = u8"x@y and ab@cd"
    let replacement = "${2} at ${1}"
    let utf8_replacement = u8"${2}"
    match Regex.compile (ref pattern) with
    | Result.Error error -> error.offset + error.message.length
    | Result.Ok re ->
        let groups: i64 = Regex.group_count (ref re)
        let matched: bool = Regex.is_match (ref re) (ref text)
        let first: Maybe<(i64 * i64)> = Regex.find (ref re) (ref text)
        let later: Maybe<(i64 * i64)> = Regex.find_at (ref re) seed (ref text)
        let all: [(i64 * i64)] = Regex.find_all (ref re) (ref text)
        let spans: Maybe<[(i64 * i64)]> = Regex.captures (ref re) (ref text)
        let replaced: string = Regex.replace_all (ref re) (ref replacement) (ref text)
        let pieces: [string] = Regex.split (ref re) (ref text)
        let steps: i64 = Regex.search_steps (ref re) (ref text)
        let escaped: string = Regex.escape (ref text)
        let matched8: bool = Regex.is_match_utf8 (ref re) (ref bytes)
        let first8: Maybe<(i64 * i64)> = Regex.find_utf8 (ref re) (ref bytes)
        let later8: Maybe<(i64 * i64)> = Regex.find_at_utf8 (ref re) seed (ref bytes)
        let all8: [(i64 * i64)] = Regex.find_all_utf8 (ref re) (ref bytes)
        let spans8: Maybe<[(i64 * i64)]> = Regex.captures_utf8 (ref re) (ref bytes)
        let replaced8: utf8string = Regex.replace_all_utf8 (ref re) (ref utf8_replacement) (ref bytes)
        let pieces8: [utf8string] = Regex.split_utf8 (ref re) (ref bytes)
        let version: string = Unicode.version ()
        let name = "Lu"
        let ranges: Maybe<[(i64 * i64)]> = Unicode.property_ranges (ref name)
        let folding: [(i64 * i64)] = Unicode.simple_case_folding ()
        let flags = (if matched then 1 else 0) + (if matched8 then 1 else 0)
        let found = (if Maybe.is_some (ref first) then 1 else 0) + (if Maybe.is_some (ref later) then 1 else 0)
        let found8 = (if Maybe.is_some (ref first8) then 1 else 0) + (if Maybe.is_some (ref later8) then 1 else 0)
        let spanned = (if Maybe.is_some (ref spans) then 1 else 0) + (if Maybe.is_some (ref spans8) then 1 else 0)
        let named = if Maybe.is_some (ref ranges) then 1 else 0
        let lengths = all.length + all8.length + replaced.length + replaced8.length + pieces.length + pieces8.length
        groups + flags + found + found8 + spanned + named + lengths + steps + escaped.length + version.length + folding.length
"#;

#[test]
fn regex_and_unicode_are_reserved_std_modules() {
    for path in ["Regex.tz", "Unicode.tz"] {
        let sources = [
            (
                "Main.tz",
                "export def answer :: i64 -> i64\nfn answer x = x",
            ),
            (path, "export def answer :: i64 -> i64\nfn answer x = x"),
        ];
        let error = analyze_modules_with_std(&sources, &[]).expect_err(path);
        assert_eq!(error.code, "E1011", "{path}\n{}", error.message);
        assert!(
            error.message.contains("reserved for the standard library"),
            "{}",
            error.message
        );
        assert_eq!(error.span.source, Some(1), "{path}");
    }
}

#[test]
fn regex_values_are_opaque_noncopy_owned() {
    let compiled = "let pattern = \"a+\"\nlet re = Result.get (Regex.compile (ref pattern))\n";
    for source in [
        format!("{compiled}re.code.length"),
        format!("{compiled}re.groups"),
        "let re = Regex { code: [], classes: [], ranges: [], groups: 0, anchored: false }\n0"
            .to_owned(),
        format!("{compiled}let other = {{ re with groups = 2 }}\nRegex.group_count (ref other)"),
        format!("{compiled}match re with | Regex {{ groups = count }} -> count"),
        "Unicode.__table_length 0".to_owned(),
        "let entry = Unicode.__table_entry\n0".to_owned(),
    ] {
        rejects(&source, "E1022");
    }
    rejects(
        &format!(
            "{compiled}let other = re\nRegex.group_count (ref re) + Regex.group_count (ref other)"
        ),
        "E1012",
    );
    rejects(
        "export def bad :: Regex\nfn bad =\n    let pattern = \"a\"\n    Result.get (Regex.compile (ref pattern))",
        "E1008",
    );
    // Matching only borrows the compiled program, so it can be shared and moved into a tzroutine.
    let module = accepts(&format!(
        "{compiled}let text = \"aaa\"\nlet both = Regex.is_match (ref re) (ref text) && Regex.is_match (ref re) (ref text)\ntzroutine {{ let owned = re; let _groups = Regex.group_count (ref owned) }}\nif both then 1 else 0"
    ));
    library(&module);
    // The error record is public: its fields can be read and matched.
    accepts(
        "let pattern = \"(\"\nmatch Regex.compile (ref pattern) with\n| Result.Error error -> (if error.kind == Regex.Syntax then error.offset else 1) + error.message.length\n| Result.Ok _ -> 0",
    );
}

#[test]
fn public_api_signatures_type_check() {
    let module = accepts(ALL_APIS);
    let [native, wasm] = library(&module);
    for ir in [&native, &wasm] {
        assert!(
            ir.contains("define internal i64 @tz.unicode.entry("),
            "{ir}"
        );
        assert_eq!(ir.matches("@tz.unicode.table.0 = internal").count(), 1);
    }
    // Patterns are always `string`; a `utf8string` pattern is the same type error as for String.find.
    let regex = rejects(
        "let pattern = u8\"a\"\nlet result = Regex.compile (ref pattern)\n0",
        "E1003",
    );
    let string = rejects(
        "let needle = u8\"a\"\nlet text = \"a\"\nString.find (ref needle) (ref text)",
        "E1003",
    );
    assert_eq!(regex.code, string.code);
    rejects(
        "let pattern = \"a\"\nlet text = u8\"a\"\nlet re = Result.get (Regex.compile (ref pattern))\nRegex.find (ref re) (ref text)",
        "E1003",
    );
}

#[test]
fn unused_regex_emits_nothing() {
    for source in [
        "export def answer :: i64 -> i64\nfn answer x = x + 1",
        "let text = \"a,b\"\nlet parts = String.split (ref \",\") (ref text)\nparts.length",
        "Unicode.version ()",
    ] {
        let module = accepts(source);
        for ir in library(&module) {
            assert!(!ir.contains("@tz.fn.Regex."), "{source}\n{ir}");
            assert!(!ir.contains("@tz.unicode."), "{source}\n{ir}");
            assert!(!ir.contains("Unicode.__table"), "{source}\n{ir}");
        }
    }
}

#[test]
fn regex_ir_is_deterministic() {
    let module = accepts(ALL_APIS);
    let again = analyze_modules(&[("Main.tz", ALL_APIS)]).unwrap();
    for (left, right) in library(&module).iter().zip(library(&again).iter()) {
        assert_eq!(left, right);
    }
    for (index, ir) in library(&module).iter().enumerate() {
        for line in ir.lines().filter(|line| line.starts_with("declare ")) {
            let name = line.split_once('@').unwrap().1;
            assert!(
                name.starts_with("llvm.")
                    || (index == 0
                        && ["malloc(", "free(", "realloc("]
                            .iter()
                            .any(|known| name.starts_with(known))),
                "{line}"
            );
        }
    }
}

#[test]
fn generated_unicode_tables_are_private_and_linked_on_use() {
    let tables = include_str!("../src/runtime/unicode.ll");
    // The tables are generated with std's Unicode version.
    let header = format!(
        "; Generated by scripts/generate-unicode.mjs from Unicode {} UCD. Do not edit.\n",
        unicode_version()
    );
    assert!(
        tables.starts_with(&header),
        "{}",
        tables.lines().next().unwrap()
    );
    assert!(tables.len() < 512 * 1024, "{}", tables.len());
    // Only the std Unicode and Regex modules may read the tables.
    let error = rejects("Unicode.__table_entry 0 0", "E1022");
    assert!(
        error.message.contains("Unicode and Regex"),
        "{}",
        error.message
    );
    let module = accepts(
        "let name = \"Nd\"\nmatch Unicode.property_ranges (ref name) with\n| Maybe.Some ranges -> ranges.length\n| Maybe.None -> 0",
    );
    for ir in library(&module) {
        assert!(
            ir.contains("define internal i64 @tz.unicode.length("),
            "{ir}"
        );
        assert!(!ir.contains("@tz.fn.Regex."), "{ir}");
    }
}

/// std's Unicode version: `UNICODE_VERSION` of scripts/generate-unicode.mjs, the one place that names it (G38).
fn unicode_version() -> &'static str {
    const GENERATOR: &str = include_str!("../scripts/generate-unicode.mjs");
    let (_, rest) = GENERATOR
        .split_once("export const UNICODE_VERSION = \"")
        .expect("scripts/generate-unicode.mjs exports UNICODE_VERSION");
    let version = rest.split_once('"').unwrap().0;
    assert!(
        version.split('.').count() == 3
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "{version}"
    );
    version
}

/// The generator and the two cases modules run as scripts when they are reached through a symlinked directory (G38):
/// Node's loader gives `import.meta.url` as the real path, so the check that a module is the script compares real
/// paths. Before, `--check` passed without checking and the write commands wrote nothing, all with exit status 0.
#[cfg(unix)]
#[test]
fn unicode_scripts_run_through_a_symlinked_directory() {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            // remove_dir_all removes the symlinks, not the directories that they name.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = Scratch(root.join("target/scratch/unicode-scripts-symlinked"));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::create_dir_all(&scratch.0).unwrap();
    for directory in ["scripts", "tests"] {
        std::os::unix::fs::symlink(root.join(directory), scratch.0.join(directory)).unwrap();
    }
    let node = |args: &[&str]| {
        std::process::Command::new("node")
            .args(args)
            .current_dir(&scratch.0)
            .output()
    };
    let Ok(unicode) = node(&["-p", "process.versions.unicode"]) else {
        eprintln!("skipped: node is not available");
        return;
    };
    let script = |path: &str| scratch.0.join(path).to_str().unwrap().to_owned();
    let missing = scratch.0.join("missing");
    let missing = missing.to_str().unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    // Without a UCD directory, the generator prints its usage.
    let usage = node(&[&script("scripts/generate-unicode.mjs"), "--check"]).unwrap();
    assert_eq!(usage.status.code(), Some(2), "{usage:?}");
    assert!(
        text(&usage.stderr).contains("usage: node scripts/generate-unicode.mjs [--check] <UCD"),
        "{usage:?}"
    );
    // `--check` with a directory that does not exist fails instead of passing.
    let check = node(&[&script("scripts/generate-unicode.mjs"), "--check", missing]).unwrap();
    assert_eq!(check.status.code(), Some(1), "{check:?}");
    assert!(text(&check.stderr).contains("ENOENT"), "{check:?}");
    // `--write` without a UCD directory prints its usage.
    let write = node(&[&script("tests/unicode-cases.mjs"), "--write"]).unwrap();
    assert_eq!(write.status.code(), Some(1), "{write:?}");
    assert!(
        text(&write.stderr).contains("usage: node tests/unicode-cases.mjs --write <UCD"),
        "{write:?}"
    );
    // Without a command, the regex cases module checks Cases.tz and counts the expectations.
    let cases = node(&[&script("tests/regex-cases.mjs")]).unwrap();
    assert!(cases.status.success(), "{cases:?}");
    assert!(text(&cases.stdout).contains("regex cases: "), "{cases:?}");
    // On a Node whose Unicode is not std's, the write commands refuse and write nothing.
    let pinned = unicode_version().rsplit_once('.').unwrap().0;
    if text(&unicode.stdout).trim() == pinned {
        eprintln!("the refusal is not checked: this Node has std's Unicode {pinned}");
        return;
    }
    let written = [
        "tests/unicode-v8.mjs",
        "tests/regex-v8.mjs",
        "tests/fixtures/regex/Cases.tz",
        "tests/unicode-ucd.mjs",
    ];
    let before = written.map(|path| std::fs::read(root.join(path)).unwrap());
    for command in [
        &["tests/unicode-cases.mjs", "--write-v8"][..],
        &["tests/unicode-cases.mjs", "--write", missing],
        &["tests/regex-cases.mjs", "--write"],
    ] {
        let script = script(command[0]);
        let mut args = vec![script.as_str()];
        args.extend_from_slice(&command[1..]);
        let refused = node(&args).unwrap();
        assert_eq!(refused.status.code(), Some(1), "{command:?}: {refused:?}");
        assert!(
            text(&refused.stderr).contains(&format!(
                "is not std's ({pinned}); use a Node with Unicode {pinned}"
            )),
            "{command:?}: {refused:?}"
        );
    }
    assert_eq!(
        written.map(|path| std::fs::read(root.join(path)).unwrap()),
        before
    );
}
