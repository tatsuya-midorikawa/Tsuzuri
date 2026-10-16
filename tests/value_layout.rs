//! Value layout (PM01): how many bytes a union tag takes, how a union stores its payload, which
//! unions use a niche, and in what order a record keeps its fields. The IR of the programs below
//! is checked against the rules of `docs/architecture.md`; `tests/features.mjs` (`value_layout`)
//! runs the same shapes natively and as WebAssembly.
use tsuzuri::{analyze, llvm};

/// The IR of `source` for native or WebAssembly code, emitted twice to check determinism.
fn ir_for(source: &str, wasm: bool) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    ir
}

/// The body of the LLVM function whose symbol starts with `symbol`.
fn body<'a>(ir: &'a str, symbol: &str) -> &'a str {
    let start = ir
        .find(&format!("@{symbol}("))
        .unwrap_or_else(|| panic!("missing {symbol}\n{ir}"));
    let end = ir[start..].find("\n}\n").unwrap() + start;
    &ir[start..end]
}

/// The definition line of the LLVM type `name`.
fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
    let prefix = format!("%\"{name}\" = type ");
    ir.lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing {name}\n{ir}"))
}

/// `union Many` with `cases` cases; the first carries `payload` when it is given, and so does
/// every second case after it.
fn many(cases: usize, payload: Option<&str>) -> String {
    let names: Vec<_> = (0..cases)
        .map(|index| match payload {
            Some(payload) if index % 2 == 0 => format!("C{index} of {payload}"),
            _ => format!("C{index}"),
        })
        .collect();
    format!("union Many = {}\n", names.join(" | "))
}

#[test]
fn tags_take_the_fewest_bytes_that_hold_every_case() {
    for (cases, bits) in [(1, 8), (2, 8), (255, 8), (256, 8), (257, 16), (300, 16)] {
        let tag = format!("i{bits}");
        // Nullary cases are the tag alone; the same count with one payload type puts the tag in
        // front of the payload.
        let enumeration = format!(
            "{}export def first :: i64\nfn first = match C0 with | C0 -> 1 | _ -> 0\n",
            many(cases, None)
        );
        let common = format!(
            "{}export def first :: i64\nfn first = match C0 7 with | C0 n -> n | _ -> 0\n",
            many(cases, Some("i64"))
        );
        let general = format!(
            "{}export def first :: i64\nfn first = match C0 \"a\" with | C0 text -> text.length | _ -> 0\n",
            many(cases, Some("string")).replacen("C2 of string", "C2 of i64", 1)
        );
        for wasm in [false, true] {
            let ir = ir_for(&enumeration, wasm);
            assert_eq!(
                definition(&ir, "tz.union.Main.Many"),
                format!("%\"tz.union.Main.Many\" = type {tag}"),
                "{cases} nullary cases, wasm {wasm}"
            );
            let ir = ir_for(&common, wasm);
            assert_eq!(
                definition(&ir, "tz.union.Main.Many"),
                format!("%\"tz.union.Main.Many\" = type {{ {tag}, i64 }}"),
                "{cases} cases with an i64 payload, wasm {wasm}"
            );
            if cases > 2 {
                let ir = ir_for(&general, wasm);
                let line = definition(&ir, "tz.union.Main.Many");
                assert!(
                    line.starts_with(&format!("%\"tz.union.Main.Many\" = type {{ {tag}, [")),
                    "{cases} cases with payloads of two types, wasm {wasm}: {line}"
                );
            }
        }
    }
}

#[test]
fn a_switch_reads_the_narrow_tag_and_widens_it() {
    for (cases, bits) in [(3, 8), (256, 8), (257, 16)] {
        let source = format!(
            "{}export def pick :: i64 -> i64\nfn pick n =\n    let value = if n == 0 then C0 7 else C2 5\n    match value with\n    | C0 x -> x\n    | C2 y -> y + 1\n    | _ -> 0\n",
            many(cases, Some("i64"))
        );
        let ir = ir_for(&source, false);
        let pick = body(&ir, "tz.fn.Main.pick");
        // The tag is loaded once in its own width, widened to `i32` for the switch, and never
        // read as a signed value.
        assert_eq!(
            pick.matches(&format!("load i{bits}, ptr")).count(),
            1,
            "{pick}"
        );
        assert!(pick.contains(&format!("zext i{bits} ")), "{pick}");
        assert!(pick.contains("switch i32 "), "{pick}");
        assert!(!pick.contains("sext"), "{pick}");
        assert!(!pick.contains("load i32, ptr"), "{pick}");
    }
}

#[test]
fn tags_are_written_in_the_stored_width() {
    let source = format!(
        "{}union Wider = W0 | W1 of i64 | W256 of i64\ndef code :: Many -> i64\nfn code value = match value with | C256 -> 1 | _ -> 0\nexport def make :: i64\nfn make = code C256\nexport def payload :: i64\nfn payload = match W256 9 with | W256 n -> n | _ -> 0\n",
        many(257, None)
    );
    for wasm in [false, true] {
        let ir = ir_for(&source, wasm);
        // An enumeration is its tag, so a constant case is the case number.
        assert!(
            body(&ir, "tz.fn.Main.make").contains("(%\"tz.union.Main.Many\" 256)"),
            "{ir}"
        );
        assert!(
            body(&ir, "tz.fn.Main.payload")
                .contains("insertvalue %\"tz.union.Main.Wider\" zeroinitializer, i8 2, 0"),
            "{ir}"
        );
    }
}

#[test]
fn recursive_unions_keep_an_i32_tag_in_their_node() {
    let source = "union Chain = End | Link of Chain\nexport def depth :: i64\nfn depth =\n    match Link (Link End) with\n    | End -> 0\n    | Link _ -> 1\n";
    for wasm in [false, true] {
        let ir = ir_for(source, wasm);
        assert_eq!(
            definition(&ir, "tz.rec.node.Main.Chain"),
            "%\"tz.rec.node.Main.Chain\" = type { ptr, ptr, ptr, i32, ptr }",
        );
        assert!(
            body(&ir, "tz.fn.Main.depth").contains("store i32 1, ptr"),
            "{ir}"
        );
    }
}

#[test]
fn dyn_values_allocate_the_llvm_size_of_their_value() {
    let source = "class Shape<'a> {\n    def area :: ref 'a -> i64\n}\nrecord Square { side: i64, tag: i8 }\ninstance Shape<Square> {\n    fn area s = s.side * s.side\n}\nexport def total :: i64\nfn total =\n    let shape: dyn Shape = Dyn.of (Square { side: 3, tag: 1 })\n    Shape.area (ref shape)\n";
    let size = "ptrtoint (ptr getelementptr (%tz.record.Main.Square, ptr null, i32 1) to i64)";
    for wasm in [false, true] {
        let ir = ir_for(source, wasm);
        // `Dyn.of` and the clone thunk of the vtable size their cell with the type's own size.
        assert!(ir.matches(size).count() >= 2, "{ir}");
        assert!(ir.contains(&format!("icmp eq i64 {size}, 0")), "{ir}");
    }
}
