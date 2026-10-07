//! D09 Phase 2: the std `Unicode` module (categories, normalization, segmentation, case conversion).

use std::collections::BTreeSet;

use tsuzuri::check::{CheckedModule, TypedExpr, TypedExprKind};
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
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
    let text = "Straße ǆ 👨‍👩‍👧 e\u{301}"
    let bytes = u8"Straße ǆ 👨‍👩‍👧 e\u{301}"
    let category: Unicode.Category = Unicode.category u8'A'
    let form: Unicode.NormalizationForm = if seed > 0 then Unicode.Nfkc else Unicode.Nfd
    let normalized: string = Unicode.normalize form (ref text)
    let normalized8: utf8string = Unicode.normalize_utf8 form (ref bytes)
    let checked: bool = Unicode.is_normalized form (ref text)
    let checked8: bool = Unicode.is_normalized_utf8 form (ref bytes)
    let clusters: [string] = Unicode.graphemes (ref text)
    let clusters8: [utf8string] = Unicode.graphemes_utf8 (ref bytes)
    let cluster_offsets: [i64] = Unicode.grapheme_boundaries (ref text)
    let cluster_offsets8: [i64] = Unicode.grapheme_boundaries_utf8 (ref bytes)
    let segments: [string] = Unicode.words (ref text)
    let segments8: [utf8string] = Unicode.words_utf8 (ref bytes)
    let word_offsets: [i64] = Unicode.word_boundaries (ref text)
    let word_offsets8: [i64] = Unicode.word_boundaries_utf8 (ref bytes)
    let lower: string = Unicode.to_lower (ref text)
    let upper: string = Unicode.to_upper (ref text)
    let title: string = Unicode.to_title (ref text)
    let folded: string = Unicode.case_fold (ref text)
    let lower8: utf8string = Unicode.to_lower_utf8 (ref bytes)
    let upper8: utf8string = Unicode.to_upper_utf8 (ref bytes)
    let title8: utf8string = Unicode.to_title_utf8 (ref bytes)
    let folded8: utf8string = Unicode.case_fold_utf8 (ref bytes)
    let flags = (if checked then 1 else 0) + (if checked8 then 2 else 0) + (if category == Unicode.Lu then 4 else 0)
    let counts = clusters.length + clusters8.length + cluster_offsets.length + cluster_offsets8.length + segments.length + segments8.length + word_offsets.length + word_offsets8.length
    let lengths = normalized.length + normalized8.length + lower.length + upper.length + title.length + folded.length + lower8.length + upper8.length + title8.length + folded8.length
    flags + counts + lengths
"#;

#[test]
fn public_api_signatures_type_check() {
    let module = accepts(ALL_APIS);
    for ir in library(&module) {
        assert_eq!(
            ir.matches("define internal i64 @tz.unicode.entry(").count(),
            1
        );
        assert!(ir.contains("@tz.unicode.table.17 = internal"), "{ir}");
        for line in ir.lines().filter(|line| line.starts_with("declare ")) {
            let name = line.split_once('@').unwrap().1;
            assert!(
                ["llvm.", "malloc(", "free(", "realloc("]
                    .iter()
                    .any(|known| name.starts_with(known)),
                "{line}"
            );
        }
    }
    // The encodings and the scalar type are checked.
    rejects(
        "let text = u8\"a\"\nUnicode.normalize Unicode.Nfc (ref text)",
        "E1003",
    );
    rejects("Unicode.category 'a'", "E1003");
}

#[test]
fn categories_and_forms_are_exhaustive_unions() {
    let cases = [
        "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe",
        "Pi", "Pf", "Po", "Sm", "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co", "Cn",
    ];
    let arms: String = cases
        .iter()
        .enumerate()
        .map(|(index, case)| format!("    | Unicode.{case} -> {index}\n"))
        .collect();
    let source = format!(
        "export def number :: i64 -> i64\nfn number code =\n    match Unicode.category (Maybe.get (Utf8Char.of_u32 (code as i32u))) with\n{arms}"
    );
    library(&accepts(&source));
    // Leaving out a case is the non-exhaustive match error.
    let partial: String = arms
        .lines()
        .skip(1)
        .map(|line| format!("{line}\n"))
        .collect();
    rejects(
        &format!(
            "export def number :: i64 -> i64\nfn number code =\n    match Unicode.category (Maybe.get (Utf8Char.of_u32 (code as i32u))) with\n{partial}"
        ),
        "E1021",
    );
    accepts(
        "let form = Unicode.Nfkd\nlet shown = Display.display form\nmatch form with\n| Unicode.Nfc -> 0\n| Unicode.Nfd -> 1\n| Unicode.Nfkc -> 2\n| Unicode.Nfkd -> shown.length",
    );
}

#[test]
fn unicode_tables_are_linked_only_on_use() {
    for source in [
        "export def answer :: i64 -> i64\nfn answer x = x + 1",
        "Unicode.version ()",
        "let text = \"Straße\"\nString.to_ascii_upper (ref text)",
    ] {
        for ir in library(&accepts(source)) {
            assert!(!ir.contains("@tz.unicode."), "{source}\n{ir}");
            assert!(!ir.contains("@tz.fn.Unicode.normalize"), "{source}\n{ir}");
        }
    }
    let module = accepts("let text = \"e\\u{301}\"\nUnicode.normalize Unicode.Nfc (ref text)");
    for ir in library(&module) {
        assert_eq!(
            ir.matches("define internal i64 @tz.unicode.length(")
                .count(),
            1
        );
        assert!(!ir.contains("@tz.fn.Regex."), "{ir}");
        assert!(!ir.contains("@tz.fn.Unicode.word_breaks"), "{ir}");
        // Table reads inline into their callers, so that a constant table number selects one table and
        // LLVM can drop the tables that the program never reads.
        for function in [
            "@tz.fn.$builtin.Unicode.__table_length(",
            "@tz.fn.$builtin.Unicode.__table_entry(",
            "@tz.builtin.Unicode.__table_length(",
            "@tz.builtin.Unicode.__table_entry(",
            "@tz.unicode.length(",
            "@tz.unicode.entry(",
        ] {
            let definition = ir
                .lines()
                .find(|line| line.starts_with("define ") && line.contains(function))
                .unwrap_or_else(|| panic!("{function}\n{ir}"));
            assert!(definition.contains(" alwaysinline"), "{definition}");
        }
    }
}

/// The deepest nesting of loops in an expression: `while`, `for` and `new [T](n, f)`. Lambdas are lifted
/// into functions of their own.
fn loop_depth(expression: &TypedExpr) -> usize {
    let inner = expression
        .children()
        .into_iter()
        .map(loop_depth)
        .max()
        .unwrap_or(0);
    let looping = matches!(
        expression.kind,
        TypedExprKind::While { .. }
            | TypedExprKind::ForRange { .. }
            | TypedExprKind::ForEach { .. }
            | TypedExprKind::NewArray(..)
            | TypedExprKind::NewList(..)
    );
    inner + usize::from(looping)
}

#[test]
fn segmentation_and_case_conversion_do_not_rescan_the_input() {
    // A review found the WB6/WB7b/WB12 lookahead rescanning every following Extend, Format and ZWJ at each
    // scalar, which made `word_boundaries` and `to_title` of "a" + "\u{301}" × n quadratic. Each scalar's
    // work is now a constant number of table lookups, so the loops over the scalars do not nest.
    let module = accepts(ALL_APIS);
    for name in ["word_breaks", "grapheme_breaks", "converted"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.module == "Unicode" && function.name == name)
            .unwrap_or_else(|| panic!("Unicode.{name}"));
        assert_eq!(loop_depth(&function.body), 1, "Unicode.{name}");
    }
}
