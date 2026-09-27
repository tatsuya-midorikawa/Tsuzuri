use tsuzuri::{analyze, llvm, parser};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
    module
}

#[test]
fn deriving_clauses_are_bounded_and_preserve_declaration_order() {
    let source = "record Point { x: i64, y: i64 } deriving (Eq, Ord, Display, Hash, Default,)\nunion Shape = Circle of f64 | Empty\n    deriving (Display, Eq)";
    let program = parser::parse(source).unwrap();
    assert_eq!(program.records[0].derives.len(), 5);
    assert_eq!(program.unions[0].derives[0].0.name(), "Display");
    for (source, code) in [
        ("record R { value: i64 } deriving (Eq, Eq)", "E1001"),
        ("record R { value: i64 } deriving (Copy)", "E1025"),
        ("record R { value: i64 } deriving ()", "E0002"),
        ("let deriving = 1", "E0002"),
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, code);
    }
    accepts("record Plain { value: i64 }");
}

#[test]
fn defaults_have_zero_argument_signatures() {
    for ty in [
        "i64",
        "f32",
        "f64",
        "d64",
        "bool",
        "unit",
        "string",
        "utf8string",
        "char",
        "utf8char",
        "[i64]",
        "[|Task<i64>|]",
        "i64 * string",
    ] {
        accepts(&format!("def make :: {ty}\nfn make = Default.default()"));
    }
    accepts("let create: fn() -> [i64] = Default.default\nlet values = create()\nvalues.length");
    for ty in ["ref i64", "Task<i64>", "i64 -> i64"] {
        let error = analyze(&format!("let value: {ty} = Default.default()\n()")).unwrap_err();
        assert_eq!(error.code, "E1005", "{ty}: {error:?}");
    }
}

#[test]
fn comparisons_and_defaults_use_normal_instance_synthesis() {
    for source in [
        "record Point { x: i64, y: i64 } deriving (Eq, Ord, Default)\nlet left = Point { x: 1, y: 2 }\nlet right = Default.default()\nleft > right",
        "record Box<'a> { value: 'a } deriving (Eq)\nBox { value: \"abc\" } == Box { value: \"abc\" }",
        "union Shape = Circle of f64 | Rect of f64 * f64 | Empty deriving (Eq, Ord, Default)\nCircle 1.0 < Rect (0.0, 0.0)",
        "union Maybe<'a> = Nothing | Just of 'a deriving (Eq, Default)\nlet value: Maybe<i64 -> i64> = Default.default()\n()",
        "union Tree = Leaf | Node of Tree * i64 * Tree deriving (Eq, Ord, Default)\nlet left = Node (Leaf, 1, Leaf)\nleft != Leaf",
    ] {
        accepts(source);
    }
    for (source, code) in [
        ("record R { callback: i64 -> i64 } deriving (Eq)", "E1025"),
        ("record R { work: Task<i64> } deriving (Default)", "E1025"),
        ("record R { value: i64 } deriving (Ord)", "E1025"),
        (
            "record R { value: i64 } deriving (Eq)\ninstance Eq<R> { fn eq _left _right = true; fn ne _left _right = false }",
            "E1016",
        ),
        (
            "union Tree = Node of Tree | Leaf deriving (Default)",
            "E1025",
        ),
    ] {
        let error = analyze(source).unwrap_err();
        assert_eq!(error.code, code, "{source}\n{error:?}");
    }
}

#[test]
fn primitive_hashes_use_canonical_bits_without_allocating() {
    for ty in [
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "i128u",
        "f16",
        "f32",
        "f64",
        "f128",
        "d32",
        "d64",
        "d128",
        "bool",
        "unit",
        "char",
        "utf8char",
        "string",
        "utf8string",
    ] {
        let module = accepts(&format!(
            "def hash :: ref {ty} -> i64u\nfn hash value = Hash.hash value"
        ));
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert!(ir.contains("1099511628211"), "{ty}");
        for function in ir.split("\ndefine ").filter(|function| {
            function
                .lines()
                .next()
                .is_some_and(|header| header.contains("hash"))
        }) {
            assert!(
                !function.split("\n}").next().unwrap().contains("@tz.alloc("),
                "{ty}"
            );
        }
    }
}

#[test]
fn structural_hashes_use_component_instances() {
    for source in [
        "record Point { x: i64, y: i64 } deriving (Hash)\nlet point = Point { x: 1, y: 2 }\nHash.hash (ref point)",
        "record Box<'a> { value: 'a } deriving (Hash)\nlet box = Box { value: [\"abc\"] }\nHash.hash (ref box)",
        "union Tree = Leaf | Node of Tree * i64 * Tree deriving (Hash)\nlet tree = Node (Leaf, 42, Leaf)\nHash.hash (ref tree)",
        "let values = [|(1, \"a\"), (2, \"b\")|]\nHash.hash (ref values)",
    ] {
        accepts(source);
    }
    assert_eq!(
        analyze("record R { callback: i64 -> i64 } deriving (Hash)")
            .unwrap_err()
            .code,
        "E1025"
    );
}

#[test]
fn structural_display_quotes_generic_text_and_characters() {
    for source in [
        "record Point { x: i64, text: string } deriving (Display)\nlet value = Point { x: 42, text: \"a\\n\" }\nDisplay.display (ref value)",
        "record Box<'a> { value: 'a } deriving (Display)\nlet value = Box { value: '\\uD800' }\nDisplay.display (ref value)",
        "union Value<'a> = Empty | Item of 'a deriving (Display)\nlet value = Item u8'\\u{1F600}'\nDisplay.display (ref value)",
        "record Box<'a> { value: 'a } deriving (Display)\nlet value = Box { value: u8\"text\" }\nDisplay.display (ref value)",
        "union Shape = Rect of i64 * string | Empty deriving (Display)\nlet value = Rect (1, \"abc\")\nDisplay.display (ref value)",
        "record Box<'a> { value: 'a } deriving (Display)\nlet value = Box { value: [|(1, \"abc\")|] }\nDisplay.display (ref value)",
        "union Tree = Leaf | Node of Tree * i64 * Tree deriving (Display)\nlet value = Node (Leaf, 42, Leaf)\nDisplay.display (ref value)",
    ] {
        accepts(source);
    }
    assert_eq!(
        analyze("record R { callback: i64 -> i64 } deriving (Display)")
            .unwrap_err()
            .code,
        "E1025"
    );
}

#[test]
fn derives_preserve_scopes_limits_and_standard_independence() {
    let module = tsuzuri::analyze_modules(&[
        ("Main.tz", "record Value { number: i64 } deriving (Eq, Ord, Display, Hash, Default)\nlet value = Value { number: 42 }\nvalue == value"),
        ("Eq.tz", "def eq :: i64 -> i64 -> bool\nfn eq _left _right = false"),
        ("Display.tz", "def display :: i64 -> string\nfn display _value = \"wrong\""),
    ]).unwrap();
    llvm::emit(&module, llvm::Entry::Library).unwrap();
    let module = tsuzuri::analyze_modules_with_std(&[("Main.tz", "record Value { text: string } deriving (Display)\nlet value = Value { text: \"a\\n\" }\nDisplay.display (ref value)")], &[]).unwrap();
    assert!(
        llvm::emit(&module, llvm::Entry::Library)
            .unwrap()
            .contains("@tz.display.quote")
    );
    let fields = (0..130)
        .map(|index| format!("field{index}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(
        analyze(&format!("record Large {{ {fields} }} deriving (Ord, Eq)"))
            .unwrap_err()
            .code,
        "E1017"
    );
    for class in ["Eq", "Ord", "Hash", "Display", "Default"] {
        let source = format!("record R {{ value: i64 }} deriving ({class})");
        let formatted = tsuzuri::formatter::format_source(
            "Main.tz",
            &source,
            tsuzuri::syntax::SourceKind::Code,
        )
        .unwrap();
        assert_eq!(
            tsuzuri::formatter::ast_fingerprint(parser::parse(&source).unwrap()),
            tsuzuri::formatter::ast_fingerprint(parser::parse(&formatted.formatted).unwrap())
        );
    }
    for class in ["Eq", "Ord", "Hash", "Display", "Default"] {
        let source = format!("union Tree = Node of Tree | Leaf deriving ({class})");
        let result = analyze(&source);
        if class == "Ord" || class == "Default" {
            assert_eq!(result.unwrap_err().code, "E1025");
        } else {
            result.unwrap();
        }
    }
}
