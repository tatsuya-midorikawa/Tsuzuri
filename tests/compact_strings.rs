use std::process::Command;
use tsuzuri::analyze;
use tsuzuri::llvm::{self, Allocator, EmitOptions, Entry, F16Path, StringForm};

#[test]
fn the_forms_have_names_and_the_default_is_utf16() {
    assert_eq!(StringForm::DEFAULT, StringForm::Utf16);
    assert_eq!(StringForm::default(), StringForm::DEFAULT);
    for form in [StringForm::Utf16, StringForm::Compact] {
        assert_eq!(StringForm::parse(form.name()), Some(form));
    }
    for name in ["", "UTF16", "Compact", "latin1", "utf16 ", "off", "1"] {
        assert_eq!(StringForm::parse(name), None, "{name:?}");
    }
}

#[test]
fn the_environment_chooses_the_form_and_a_wrong_name_is_an_error() {
    let root = std::env::temp_dir().join(format!("tsuzuri-strings-form-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), "let text = \"hello\"\ntext.length\n").unwrap();
    let build = |value: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .args(["build", "--emit", "llvm", "-O0", "-o"])
            .arg(root.join("out.ll"))
            .arg(root.join("Main.tz"))
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env_remove("TSUZURI_STRINGS");
        if let Some(value) = value {
            command.env("TSUZURI_STRINGS", value);
        }
        let output = command.output().unwrap();
        let ir = std::fs::read_to_string(root.join("out.ll")).ok();
        let _ = std::fs::remove_file(root.join("out.ll"));
        (output, ir)
    };
    // The form belongs to the cache key: one cache directory serves every build.
    let mut irs = Vec::new();
    for value in [
        None,
        Some("utf16"),
        Some(""),
        Some("compact"),
        Some("utf16"),
        None,
    ] {
        let (output, ir) = build(value);
        assert!(output.status.success(), "{value:?}");
        irs.push((value, ir.expect("the build writes its IR")));
    }
    for (value, ir) in &irs {
        let compact = *value == Some("compact");
        assert_eq!(
            ir.contains("@tz.string.allocate_latin1"),
            compact,
            "{value:?}"
        );
        assert_eq!(
            ir.contains("c\"\\68\\65\\6C\\6C\\6F\""),
            compact,
            "{value:?}"
        );
        assert_eq!(ir.contains("[5 x i16]"), !compact, "{value:?}");
    }
    let (output, ir) = build(Some("latin1"));
    assert_eq!(output.status.code(), Some(1));
    assert!(ir.is_none());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("E2000")
            && stderr.contains("TSUZURI_STRINGS must be utf16 or compact, not 'latin1'"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

fn options(wasm: bool, wasm_simd128: bool) -> EmitOptions {
    EmitOptions {
        entry: Entry::Library,
        wasm,
        wasm_simd128,
        debug_output: false,
        runtime_objects: false,
        allocator: Allocator::System,
        f16: F16Path::Soft,
    }
}

fn emit(source: &str, form: StringForm, wasm: bool, wasm_simd128: bool) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_with_strings(&module, options(wasm, wasm_simd128), form).unwrap();
    assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}");
    assert_eq!(
        ir,
        llvm::emit_with_strings(&module, options(wasm, wasm_simd128), form).unwrap(),
        "the IR is deterministic"
    );
    ir
}

/// Programs that read and write strings in every way that the emitter has a case for.
const PROGRAMS: &[&str] = &[
    r#"export def shape :: i64 -> i64
fn shape i =
    let text = "caf\u00e9 au lait"
    let mut sum = 0
    for unit in text do
        sum = sum + (unit as i64)
    sum + text.length + (text[i] as i64)
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let a = "abc" + "\u3042"
    let b = to_string i
    let same = if a == b then 1 else 0
    let before = if a < b then 2 else 0
    let mut c = a + b
    c = c + "\u00ff"
    c.length + same + before
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let name = "tsuzuri"
    let text = $"{name:>12}|{i:+}|{name:*^20}|{i:0>8}"
    text.length + (text[2] as i64)
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let mut map: HashMap<string, i64> = HashMap.empty()
    map = HashMap.insert map "caf\u00e9" i
    map = HashMap.insert map "plain" 2
    HashMap.length (ref map)
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let text = to_string 3.5
    let parsed: Maybe<f64> = Parse.parse (ref text)
    let number: Maybe<i128> = Parse.parse (ref "123")
    let wide: Maybe<i64> = Parse.parse (ref "9\u0663")
    (Maybe.default_value 0.0 parsed as i64) + (if Maybe.is_none (ref wide) then 1 else 0) + i
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let units = String.to_code_units "a\u00e9"
    let back = String.from_code_units units
    let maybe: Maybe<string> = if i > 0 then Maybe.Some "caf\u00e9" else Maybe.None
    back.length + (match maybe with | Some text -> text.length | None -> 0 - 1)
"#,
    r#"export def shape :: i64 -> i64
fn shape i =
    let shown = to_string ["a", "b\u00e9", "c\u3042"]
    let utf8 = Utf8String.from_string (ref shown)
    shown.length + Utf8String.length (ref utf8) + i
"#,
];

#[test]
fn the_utf16_form_never_makes_latin1_and_the_explicit_form_is_the_default() {
    // `emit_with_options` takes the form of the environment, which a run of the whole suite may set.
    let environment = std::env::var("TSUZURI_STRINGS").unwrap_or_default();
    for source in PROGRAMS {
        for (wasm, simd) in [(false, false), (true, false), (true, true)] {
            let module = analyze(source).unwrap();
            let utf16 = emit(source, StringForm::Utf16, wasm, simd);
            let default = llvm::emit_with_options(&module, options(wasm, simd)).unwrap();
            if matches!(environment.as_str(), "" | "utf16") {
                assert_eq!(utf16, default, "{source}");
            }
            for name in [
                "allocate_latin1",
                "tz.format.allocate",
                "borrow_utf16",
                "@tz.string.new.utf16(",
                ".latin1(",
            ] {
                assert!(
                    !utf16.contains(name),
                    "{name} in the UTF-16 form of\n{source}"
                );
            }
        }
    }
}

#[test]
fn literals_are_bytes_when_every_unit_fits_and_units_otherwise() {
    let source = "export def shape :: i64 -> i64\nfn shape i = \"abc\".length + \"caf\\u00e9\".length + \"\\u3042\".length + \"\".length + i\n";
    let compact = emit(source, StringForm::Compact, false, false);
    assert!(compact.contains("[3 x i8] c\"\\61\\62\\63\""), "{compact}");
    assert!(
        compact.contains("[4 x i8] c\"\\63\\61\\66\\E9\""),
        "{compact}"
    );
    assert!(compact.contains("[1 x i16] [i16 12354]"), "{compact}");
    // The words of the Latin-1 literals carry bit 62 and the other has the plain length.
    assert!(compact.contains("i64 4611686018427387907, 1"), "{compact}");
    assert!(compact.contains("i64 4611686018427387908, 1"), "{compact}");
    assert!(
        !compact.contains("[3 x i16]") && !compact.contains("[4 x i16]"),
        "{compact}"
    );
}

/// The uses of the length word of a string in the code that the emitter writes (not the runtime).
/// A word is a count only after `and %word, 2^62 - 1`; everything else it may do is to be passed
/// on whole, to a function that takes the word apart, or to be compared with 0 (the empty string),
/// with another word, or with the sign of the niche of `Maybe<string>`.
fn raw_word_uses(ir: &str) -> Vec<String> {
    let runtime = [
        "tz.string.",
        "tz.utf8string.",
        "tz.display.",
        "tz.integer.",
        "tz.character.",
        "tz.format.",
        "tz.heap.",
        "tz.unicode.",
        "tz.alloc",
        "tz.free",
        "tz.realloc",
        "tz.console.",
        "tz.debug.",
        "tz_soft_",
        "tz_math_",
        "tz_small_",
    ];
    let mut found = Vec::new();
    for function in ir.split("\ndefine ").skip(1) {
        let name = function
            .split_once('@')
            .map(|(_, rest)| rest.trim_start_matches('"'))
            .and_then(|rest| rest.split_once('('))
            .map_or("", |(name, _)| name);
        if runtime.iter().any(|prefix| name.starts_with(prefix)) || name == "main" {
            continue;
        }
        let mut words = std::collections::BTreeSet::new();
        // The result of a conversion to UTF-16 has a plain count for a word.
        let mut plain = std::collections::BTreeSet::new();
        for line in function.lines().map(str::trim) {
            if let Some((value, rest)) = line.split_once(" = ") {
                if rest.starts_with("call %tz.string @tz.string.to_utf16(")
                    || rest.starts_with("call %tz.string @tz.string.borrow_utf16(")
                {
                    plain.insert(value.to_owned());
                    continue;
                }
                let source_word =
                    rest.strip_prefix("extractvalue %tz.string ")
                        .is_some_and(|operands| {
                            operands.ends_with(", 1")
                                && !plain.iter().any(|value: &String| {
                                    operands.starts_with(&format!("{value}, "))
                                })
                        });
                let derived = (rest.starts_with("phi i64") || rest.starts_with("select i1"))
                    && words.iter().any(|word: &String| mentions(rest, word));
                if source_word || derived {
                    words.insert(value.to_owned());
                    continue;
                }
            }
            let Some(word) = words.iter().find(|word| mentions(line, word)) else {
                continue;
            };
            let operation = line.split_once(" = ").map_or(line, |(_, rest)| rest);
            let allowed = operation.starts_with(&format!("and i64 {word}, 4611686018427387903"))
                || operation.starts_with(&format!("and i64 {word}, 4611686018427387904"))
                || operation.starts_with("insertvalue %tz.string")
                || operation.starts_with(&format!("icmp ne i64 {word}, 0"))
                || operation.starts_with(&format!("icmp slt i64 {word}, 0"))
                || operation
                    .strip_prefix(&format!("icmp eq i64 {word}, "))
                    .is_some_and(|constant| constant.parse::<u64>().is_ok())
                || operation.starts_with("store i64")
                || operation.contains("@tz.string.")
                || operation.contains("@tz.utf8string.")
                || operation.contains("@tz.integer.parse")
                || operation.contains("@tz.display.quote")
                || operation.contains("@tz.format.put")
                || operation.contains("@tz.format.scalars(");
            if !allowed {
                found.push(format!("{name}: {line}"));
            }
        }
    }
    found
}

fn mentions(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        !line[at + word.len()..]
            .starts_with(|next: char| next.is_ascii_alphanumeric() || next == '.' || next == '_')
    })
}

#[test]
fn the_length_word_of_a_string_is_only_a_count_after_it_is_masked() {
    for source in PROGRAMS {
        for (wasm, simd) in [(false, false), (true, true)] {
            let compact = emit(source, StringForm::Compact, wasm, simd);
            let uses = raw_word_uses(&compact);
            assert!(uses.is_empty(), "{source}\n{}", uses.join("\n"));
        }
    }
}

#[test]
fn the_lint_finds_a_word_that_is_used_as_a_count() {
    let ir = "\ndefine internal i64 @tz.fn.Main.f(%tz.string %s) {\n  %v1 = extractvalue %tz.string %s, 1\n  %v2 = add i64 %v1, 1\n  %v3 = and i64 %v1, 4611686018427387903\n  %v4 = icmp eq i64 %v1, 0\n  ret i64 %v2\n}\n";
    assert_eq!(
        raw_word_uses(ir),
        ["tz.fn.Main.f: %v2 = add i64 %v1, 1".to_owned()]
    );
}

#[test]
fn the_niche_of_a_maybe_string_is_the_sign_of_the_word_and_a_latin1_word_is_positive() {
    let source = "export def shape :: i64 -> i64\nfn shape i =\n    let maybe: Maybe<string> = if i > 0 then Maybe.Some \"hello\" else Maybe.None\n    match maybe with\n    | Some text -> text.length\n    | None -> 0 - 1\n";
    for form in [StringForm::Utf16, StringForm::Compact] {
        let ir = emit(source, form, false, false);
        // None is `{ ptr null, i64 -1 }`, and the case is told by the sign of the word.
        assert!(ir.contains("{ ptr null, i64 -1 }"), "{form:?}\n{ir}");
        assert!(ir.contains("icmp slt i64"), "{form:?}\n{ir}");
    }
    let compact = emit(source, StringForm::Compact, false, false);
    assert!(
        compact.contains("@tz.string.new(ptr @tz.literal.0, i64 4611686018427387909)"),
        "{compact}"
    );
    assert!(
        !compact.contains("i64 -9223372036854775"),
        "no word has bit 63 set"
    );
}

#[test]
fn the_host_boundary_stays_utf16() {
    let source =
        "export def echo :: ref string -> string\nfn echo text = clone_string text + \"!\"\n";
    let compact = emit(source, StringForm::Compact, false, false);
    assert!(
        compact.contains("call %tz.string @tz.string.to_utf16("),
        "an exported string is UTF-16 for the host\n{compact}"
    );
    let utf16 = emit(source, StringForm::Utf16, false, false);
    assert!(!utf16.contains("@tz.string.to_utf16"));
}

/// The text of the functions that the program defines, without the runtime that is linked in.
fn program_functions(ir: &str) -> String {
    ir.split("\ndefine ")
        .skip(1)
        .filter(|function| function.contains("@tz.fn.") && !function.contains("@tz.fn.tz."))
        .collect::<Vec<_>>()
        .join("\ndefine ")
}

#[test]
fn interpolation_allocates_once_through_the_format_helpers() {
    let source = "export def shape :: i64 -> i64\nfn shape i =\n    let name = \"tsuzuri\"\n    let text = $\"a {name} b {i}\"\n    text.length\n";
    let compact = program_functions(&emit(source, StringForm::Compact, false, false));
    assert_eq!(
        compact
            .matches("call %tz.string @tz.format.allocate(")
            .count(),
        1,
        "{compact}"
    );
    assert!(compact.contains("call void @tz.format.put("), "{compact}");
    assert!(!compact.contains("@tz.string.allocate("), "{compact}");
    assert!(!compact.contains("@tz.string.concat"), "{compact}");
}
