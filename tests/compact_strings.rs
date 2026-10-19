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

#[test]
fn a_latin1_string_has_the_length_limit_of_the_heap_of_its_wasm_module_and_none_elsewhere() {
    let root = std::env::temp_dir().join(format!("tsuzuri-strings-limit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), "let text = \"hello\"\ntext.length\n").unwrap();
    let build = |form: &str, arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["build", "--emit", "llvm", "-O0", "-o"])
            .arg(root.join("out.ll"))
            .arg(root.join("Main.tz"))
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", root.join("cache"))
            .env("TSUZURI_STRINGS", form)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(root.join("out.ll")).unwrap()
    };
    // Native: the allocator gives what it can, so there is nothing to check.
    let native = build("compact", &[]);
    assert!(native.contains(
        "define internal void @tz.string.latin1_fits(i64 %length) nounwind alwaysinline"
    ));
    assert!(!native.contains("%latin1_fits = icmp"));
    // WebAssembly: the heap's limit, on twice the length, as the UTF-16 string would be asked.
    let wasm = build("compact", &["--target", "wasm32"]);
    assert!(
        wasm.contains(
            "  %size = shl i64 %length, 1\n  %latin1_fits = icmp ule i64 %size, 16777184\n"
        )
    );
    assert!(wasm.contains("  %fits = icmp ule i64 %size, 16777184\n"));
    // --wasm-max-memory moves the limit of the heap and the one of the Latin-1 strings together.
    let large = build(
        "compact",
        &["--target", "wasm32", "--wasm-max-memory", "64MiB"],
    );
    assert!(large.contains("  %latin1_fits = icmp ule i64 %size, 67108832\n"));
    assert!(large.contains("  %fits = icmp ule i64 %size, 67108832\n"));
    assert!(!large.contains("16777184"));
    // The host's allocator has no limit that the module knows.
    let host = build("compact", &["--target", "wasm32", "--allocator", "host"]);
    assert!(!host.contains("%latin1_fits = icmp"));
    assert!(host.contains(
        "define internal void @tz.string.latin1_fits(i64 %length) nounwind alwaysinline"
    ));
    // The UTF-16 form has no such function.
    for arguments in [&[][..], &["--target", "wasm32"][..]] {
        assert!(!build("utf16", arguments).contains("latin1_fits"));
    }
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
    r#"extern def host_note :: ref string -> i64

export def shape :: i64 -> i64
fn shape i =
    let text = "caf\u00e9 " + to_string i
    host_note (&text) + i
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
                || takes_the_word(operation, word);
            if !allowed {
                found.push(format!("{name}: {line}"));
            }
        }
    }
    found
}

/// The runtime functions that take the length word of a string whole, and take it apart themselves, with
/// the positions of their parameters that are such a word. The other parameters are counts: the word of a
/// string must never be passed as `allocate`, `copy`, `widen` and the like take it.
const WORD_PARAMETERS: &[(&str, &[usize])] = &[
    ("tz.string.new", &[1]),
    ("tz.string.to_ascii", &[2]),
    ("tz.string.is_well_formed", &[1]),
    ("tz.string.to_well_formed", &[1]),
    ("tz.string.slice", &[1]),
    ("tz.string.join", &[1]),
    ("tz.utf8string.from_string", &[1]),
    ("tz.integer.parse", &[1]),
    ("tz.integer.parse128", &[1]),
    ("tz.display.quote", &[1]),
    ("tz.format.put", &[3]),
    ("tz.format.scalars", &[1]),
];

/// Whether `operation` calls a function of `WORD_PARAMETERS` and passes `word` only as an argument that is a word.
fn takes_the_word(operation: &str, word: &str) -> bool {
    let Some(call) = operation
        .strip_prefix("call ")
        .or_else(|| operation.strip_prefix("tail call "))
    else {
        return false;
    };
    let Some((name, arguments)) = call
        .split_once('@')
        .and_then(|(_, rest)| rest.split_once('('))
    else {
        return false;
    };
    let Some((_, positions)) = WORD_PARAMETERS
        .iter()
        .find(|(function, _)| *function == name)
    else {
        return false;
    };
    let arguments = arguments
        .rsplit_once(')')
        .map_or(arguments, |(list, _)| list);
    let mut passed = false;
    for (index, argument) in arguments.split(", ").enumerate() {
        if argument.ends_with(&format!(" {word}")) {
            if !positions.contains(&index) {
                return false;
            }
            passed = true;
        }
    }
    passed
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
fn the_lint_lets_a_word_go_only_to_the_parameter_that_takes_it() {
    let ir = "\ndefine internal i64 @tz.fn.Main.g(%tz.string %s, ptr %p) {\n  %v1 = extractvalue %tz.string %s, 1\n  %v2 = call %tz.string @tz.string.allocate(i64 %v1)\n  %v3 = call %tz.string @tz.string.slice(ptr %p, i64 %v1, i64 %v1, i64 0)\n  %v4 = call %tz.string @tz.string.slice(ptr %p, i64 %v1, i64 0, i64 1)\n  %v5 = call i1 @tz.string.to_ascii(ptr %p, ptr %p, i64 %v1)\n  %v6 = call i64 @tz.string.copy(ptr %p, ptr %p, i64 %v1)\n  ret i64 0\n}\n";
    assert_eq!(
        raw_word_uses(ir),
        [
            "tz.fn.Main.g: %v2 = call %tz.string @tz.string.allocate(i64 %v1)".to_owned(),
            "tz.fn.Main.g: %v3 = call %tz.string @tz.string.slice(ptr %p, i64 %v1, i64 %v1, i64 0)"
                .to_owned(),
            "tz.fn.Main.g: %v6 = call i64 @tz.string.copy(ptr %p, ptr %p, i64 %v1)".to_owned(),
        ]
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

/// The text of the function `name` (without the `@`), from its `define` line to its closing brace.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    let head = format!(" @{name}(");
    let start = ir
        .match_indices("\ndefine ")
        .map(|(at, _)| at + 1)
        .find(|at| {
            ir[*at..]
                .lines()
                .next()
                .is_some_and(|line| line.contains(&head))
        })
        .unwrap_or_else(|| panic!("{name} is defined"));
    let rest = &ir[start..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |end| end + 3)]
}

#[test]
fn a_host_import_gets_a_utf16_copy_that_is_freed_after_the_call() {
    let source = PROGRAMS[PROGRAMS.len() - 1];
    assert!(source.starts_with("extern def host_note :: ref string -> i64"));
    for (wasm, simd) in [(false, false), (true, false), (true, true)] {
        let compact = emit(source, StringForm::Compact, wasm, simd);
        let wrapper = function_body(&compact, "tz.fn.Main.host_note");
        let borrow = wrapper
            .find("call %tz.string @tz.string.borrow_utf16(")
            .unwrap_or_else(|| panic!("the import borrows UTF-16 text\n{wrapper}"));
        let call = wrapper
            .find("call i64 @tsuzuri_host_Main_host_note(")
            .unwrap_or_else(|| panic!("the import is called\n{wrapper}"));
        let free = wrapper
            .rfind("call void @tz.free(ptr ")
            .unwrap_or_else(|| panic!("the copy is freed\n{wrapper}"));
        assert!(borrow < call && call < free, "{wrapper}");
        // Only a copy is freed: the pointer is the copy for a string with the mark, null for the original.
        let freed = wrapper[free..]
            .split_once("ptr ")
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(value, _)| value.to_owned())
            .unwrap();
        assert!(
            wrapper.contains(&format!("{freed} = select i1 ")) && wrapper.contains(", ptr null"),
            "{wrapper}"
        );
        // The UTF-16 form passes the string as it is and frees nothing.
        let utf16 = emit(source, StringForm::Utf16, wasm, simd);
        let wrapper = function_body(&utf16, "tz.fn.Main.host_note");
        assert!(
            !wrapper.contains("borrow_utf16") && !wrapper.contains("@tz.free"),
            "{wrapper}"
        );
    }
}

/// A host import that takes a callback is lowered at its call site, so that an argument that raises leaves the call
/// there. The UTF-16 copies of its strings are made after the last argument: before it, the raise would skip the
/// `free` that follows the call and the copies would be left behind (tests/compact_strings.mjs counts them).
#[test]
fn a_callback_host_call_makes_its_utf16_copies_after_every_argument_is_evaluated() {
    let source = include_str!("fixtures/compact_strings_import/Main.tz");
    for (wasm, simd) in [(false, false), (true, false), (true, true)] {
        let compact = emit(source, StringForm::Compact, wasm, simd);
        let caught = function_body(&compact, "tz.fn.Main.caught");
        let borrow = "call %tz.string @tz.string.borrow_utf16(";
        assert_eq!(caught.matches(borrow).count(), 2, "{caught}");
        let overflow = caught
            .find("@llvm.sadd.with.overflow.i64(")
            .unwrap_or_else(|| panic!("n + 1 is checked\n{caught}"));
        let first = caught.find(borrow).unwrap();
        let call = caught
            .find("call i64 @pm03_apply(")
            .unwrap_or_else(|| panic!("the import is called\n{caught}"));
        assert!(
            overflow < first && first < call,
            "wasm {wasm}, simd {simd}\n{caught}"
        );
        // Nothing is made between the copies and the call: only their pointers and lengths are read.
        let between = &caught[first..call];
        assert!(
            !between.contains("@llvm.sadd") && !between.contains("store i32"),
            "{between}"
        );
        // Each copy is freed after the call, and a string that is the original is not (null).
        let selects = |text: &str| {
            text.lines()
                .filter(|line| line.contains("= select i1 ") && line.ends_with(", ptr null"))
                .count()
        };
        assert_eq!((selects(&caught[..call]), selects(&caught[call..])), (0, 2));
        let utf16 = emit(source, StringForm::Utf16, wasm, simd);
        let caught = function_body(&utf16, "tz.fn.Main.caught");
        assert!(!caught.contains("borrow_utf16"), "{caught}");
    }
}

/// The code unit that `for unit in text` binds is a value of its own, not a place in the string: a Latin-1 string has no
/// 16-bit unit in memory to refer to, and the loop widens the unit into a slot that the next iteration overwrites. A
/// borrow of it ends with its iteration in both forms alike (the checker does not know the form), while the elements of
/// an array and the bytes of a `utf8string` stay places in memory.
#[test]
fn a_borrow_of_the_code_unit_of_a_string_loop_ends_with_its_iteration() {
    const KEPT: &str = r#"export def shape :: i64 -> i64
fn shape i =
    let text = "ab"
    let zero = 0i16u
    let mut saved = ref zero
    for unit in text do
        if unit == 97i16u then saved = ref unit
    ((deref saved) as i64) + i
"#;
    const RETURNED: &str = r#"def pick :: ref string -> ref i16u -> ref i16u
fn pick text fallback =
    let mut result = fallback
    for unit in text do
        if unit == 97i16u then result = ref unit
    result

export def shape :: i64 -> i64
fn shape i =
    let zero = 0i16u
    let found = pick (ref "ab") (ref zero)
    ((deref found) as i64) + i
"#;
    const THROUGH_A_FUNCTION: &str = r#"def same :: ref i16u -> ref i16u
fn same unit = unit

export def shape :: i64 -> i64
fn shape i =
    let text = "ab"
    let zero = 0i16u
    let mut saved = ref zero
    for unit in text do
        if unit == 97i16u then saved = same (ref unit)
    ((deref saved) as i64) + i
"#;
    const SCOPED: &str = r#"export def shape :: i64 -> i64
fn shape i =
    let text = "caf\u00e9"
    let mut sum = 0
    for unit in text do
        let seen = ref unit
        let again = ref unit
        sum = sum + ((deref seen) as i64) + ((deref again) as i64)
    sum + i
"#;
    const BYTES: &str = r#"export def shape :: i64 -> i64
fn shape i =
    let bytes = u8"ab"
    let first = bytes[0]
    let mut saved = ref first
    for byte in bytes do
        if byte == bytes[0] then saved = ref byte
    ((deref saved) as i64) + i
"#;
    for source in [KEPT, RETURNED, THROUGH_A_FUNCTION] {
        let error = analyze(source).err().unwrap_or_else(|| {
            panic!("a borrow that outlives its iteration is accepted:\n{source}")
        });
        assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
        assert!(error.message.contains("iteration binding"), "{error:?}");
    }
    // The command line refuses it the same way in both forms.
    let root = std::env::temp_dir().join(format!("tsuzuri-strings-unit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), KEPT).unwrap();
    let diagnostics: Vec<String> = ["utf16", "compact"]
        .iter()
        .map(|form| {
            let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
                .args(["build", "--emit", "llvm", "-O0", "-o"])
                .arg(root.join("out.ll"))
                .arg(root.join("Main.tz"))
                .env("TSUZURI_CACHE_DIR", root.join("cache"))
                .env("TSUZURI_STRINGS", form)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{form}");
            String::from_utf8(output.stderr).unwrap()
        })
        .collect();
    assert!(
        diagnostics[0].contains("error[E1013]"),
        "{}",
        diagnostics[0]
    );
    assert_eq!(diagnostics[0], diagnostics[1]);
    std::fs::remove_dir_all(&root).unwrap();
    // The elements of an array are places in the array, and a borrow of one may outlive the iteration.
    analyze(&KEPT.replace("\"ab\"", "[97i16u, 98i16u]"))
        .expect("an element of an array can be kept");
    analyze(BYTES).expect("a byte of a utf8string can be kept");
    // A borrow that ends in its iteration is fine, in both forms.
    for form in [StringForm::Utf16, StringForm::Compact] {
        emit(SCOPED, form, false, false);
    }
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
