//! D09: the std `Regex` and `Unicode` modules.

use std::collections::BTreeSet;

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
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: BTreeSet<_> = declarations.iter().collect();
        assert_eq!(unique.len(), declarations.len(), "{ir}");
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
        let later: Maybe<(i64 * i64)> = Regex.find_at (ref re) (ref text) seed
        let all: [(i64 * i64)] = Regex.find_all (ref re) (ref text)
        let spans: Maybe<[(i64 * i64)]> = Regex.captures (ref re) (ref text)
        let replaced: string = Regex.replace_all (ref re) (ref text) (ref replacement)
        let pieces: [string] = Regex.split (ref re) (ref text)
        let steps: i64 = Regex.search_steps (ref re) (ref text)
        let escaped: string = Regex.escape (ref text)
        let matched8: bool = Regex.is_match_utf8 (ref re) (ref bytes)
        let first8: Maybe<(i64 * i64)> = Regex.find_utf8 (ref re) (ref bytes)
        let later8: Maybe<(i64 * i64)> = Regex.find_at_utf8 (ref re) (ref bytes) seed
        let all8: [(i64 * i64)] = Regex.find_all_utf8 (ref re) (ref bytes)
        let spans8: Maybe<[(i64 * i64)]> = Regex.captures_utf8 (ref re) (ref bytes)
        let replaced8: utf8string = Regex.replace_all_utf8 (ref re) (ref bytes) (ref utf8_replacement)
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
    // Matching only borrows the compiled program, so it can be shared and moved into a task.
    let module = accepts(&format!(
        "{compiled}let text = \"aaa\"\nlet both = Regex.is_match (ref re) (ref text) && Regex.is_match (ref re) (ref text)\nlet job = task {{ let owned = re; return Regex.group_count (ref owned) }}\nif both then Task.run job else 0"
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
    assert!(tables.starts_with(
        "; Generated by scripts/generate-unicode.mjs from Unicode 17.0.0 UCD. Do not edit.\n"
    ));
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
