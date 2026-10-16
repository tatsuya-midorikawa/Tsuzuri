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
        .match_indices(&format!("@{symbol}("))
        .map(|(at, _)| at)
        .find(|&at| ir[ir[..at].rfind('\n').map_or(0, |line| line + 1)..].starts_with("define"))
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

#[test]
fn option_like_unions_are_their_payload() {
    // One nullary case and one string, utf8string, or array payload, in either order.
    let source = "union Alias = NoAlias | Spelled of string
union Label = Named of string | Anonymous
union Raw = NoRaw | Bytes of utf8string
union Numbers = NoNumbers | Filled of [i64]
def alias_of :: i64 -> Alias
fn alias_of n = if n == 0 then NoAlias else Spelled \"a\"
def label_of :: i64 -> Label
fn label_of n = if n == 0 then Anonymous else Named \"a\"
def alias_length :: Alias -> i64
fn alias_length alias =
    match alias with
    | Spelled text -> text.length
    | NoAlias -> 0
def label_length :: Label -> i64
fn label_length label =
    match label with
    | Named text -> text.length
    | Anonymous -> 0
def raw_of :: Raw
fn raw_of = Bytes u8\"a\"
def numbers_of :: Numbers
fn numbers_of = NoNumbers
export def total :: i64
fn total = alias_length (alias_of 1) + label_length (label_of 1) + (match raw_of() with | Bytes _ -> 1 | NoRaw -> 0) + (match numbers_of() with | Filled _ -> 1 | NoNumbers -> 0)
";
    for wasm in [false, true] {
        let ir = ir_for(source, wasm);
        for name in ["Alias", "Label", "Raw", "Numbers"] {
            assert!(
                !ir.contains(&format!("tz.union.Main.{name}")),
                "{name}\n{ir}"
            );
        }
        // The union is its payload: no type of its own, and the nullary case is a constant.
        assert!(
            body(&ir, "tz.fn.Main.alias_of").contains("ret %tz.string { ptr null, i64 -1 }"),
            "{ir}"
        );
        assert!(
            body(&ir, "tz.fn.Main.label_of").contains("ret %tz.string { ptr null, i64 -1 }"),
            "{ir}"
        );
        assert!(
            body(&ir, "tz.fn.Main.raw_of").contains("ret %tz.utf8string"),
            "{ir}"
        );
        assert!(
            body(&ir, "tz.fn.Main.numbers_of").contains("ret %tz.array { ptr null, i64 -1 }"),
            "{ir}"
        );
        // A match tests the sign of the length and selects the case number: `NoAlias` is case 0 and
        // `Anonymous` is case 1, so the selects differ and nothing reads a tag byte.
        let alias = body(&ir, "tz.fn.Main.alias_length");
        assert!(alias.contains("icmp slt i64"), "{alias}");
        assert!(alias.contains(", i32 0, i32 1"), "{alias}");
        let label = body(&ir, "tz.fn.Main.label_length");
        assert!(label.contains("icmp slt i64"), "{label}");
        assert!(label.contains(", i32 1, i32 0"), "{label}");
        assert!(!alias.contains("load i8"), "{alias}");
    }
}

#[test]
fn only_aggregate_unions_return_through_a_slot() {
    // PR03's return slot merges the exits of a function that returns a tag and a payload; a niche
    // union is its payload alone, so its exits keep their plain `ret`s.
    let source = "def text :: bool -> string -> Maybe<string>
fn text flag value = if flag then Some value else None
def number :: bool -> i64 -> Maybe<i64>
fn number flag value = if flag then Some value else None
";
    for wasm in [false, true] {
        let ir = ir_for(source, wasm);
        let text = body(&ir, "tz.fn.Main.text");
        assert!(!text.contains("tz.return"), "{text}");
        assert_eq!(text.matches("  ret %tz.string ").count(), 2, "{text}");
        let number = body(&ir, "tz.fn.Main.number");
        assert_eq!(number.matches("%tz.return.slot = alloca").count(), 1);
        assert_eq!(number.matches("  ret ").count(), 1, "{number}");
    }
}

#[test]
fn other_unions_keep_their_tag() {
    // Two payload cases, three cases, a Drop union, a nested option, and a recursive union.
    let source = "extern def drop_log :: i64 -> unit
union Twin = Left of string | Right of string
union Trio = Uno | Dos of string | Tres
union Guard = Dormant | Gripped of string
instance Drop<Guard> { fn drop _value = drop_log 4 }
union Sized = Missing | Present of i64
union Chain = End | Link of Chain
export def total :: i64
fn total =
    let twin = Left \"a\"
    let trio = Uno
    let guard = Dormant
    let sized = Missing
    let chain = End
    let nested: Maybe<Maybe<string>> = Some None
    let result: Result<string, string> = Ok \"a\"
    0
";
    for wasm in [false, true] {
        let ir = ir_for(source, wasm);
        for (name, body) in [
            ("Twin", "{ i8, %tz.string }"),
            ("Trio", "{ i8, %tz.string }"),
            ("Guard", "{ i8, %tz.string, i8 }"),
            ("Sized", "{ i8, i64 }"),
        ] {
            assert_eq!(
                definition(&ir, &format!("tz.union.Main.{name}")),
                format!("%\"tz.union.Main.{name}\" = type {body}"),
            );
        }
        assert_eq!(
            definition(&ir, "tz.union.Maybe.Maybe[Maybe.Maybe[string]]"),
            "%\"tz.union.Maybe.Maybe[Maybe.Maybe[string]]\" = type { i8, %tz.string }",
        );
        assert!(!ir.contains("tz.union.Maybe.Maybe[string]\""), "{ir}");
        assert!(ir.contains("tz.rec.node.Main.Chain"), "{ir}");
    }
}

#[test]
fn an_option_of_a_reference_is_its_pointer_on_native_targets_only() {
    let source = "export def find :: i64 -> i64
fn find n =
    let values = [1, 2, 3]
    match Array.find (\\x -> deref x == n) (ref values) with
    | Some value -> deref value
    | None -> -1
";
    let native = ir_for(source, false);
    // No type of its own: the option is the `ptr`, and the null pointer is `None`.
    assert!(
        !native.contains("tz.union.Maybe.Maybe[ref[i64]]"),
        "{native}"
    );
    let search = "tz.fn.Array.find.$mono.1";
    assert!(body(&native, search).contains("ret ptr null"), "{native}");
    let find = body(&native, "tz.fn.Main.find");
    assert!(find.contains("icmp eq ptr"), "{find}");
    assert!(find.contains("select i1"), "{find}");
    // WebAssembly keeps the tag: `--stack-first` can place a valid object at address 0.
    let wasm = ir_for(source, true);
    assert_eq!(
        definition(&wasm, "tz.union.Maybe.Maybe[ref[i64]]"),
        "%\"tz.union.Maybe.Maybe[ref[i64]]\" = type { i8, ptr }",
    );
    assert!(!body(&wasm, search).contains("ret ptr null"), "{wasm}");
}
