//! Std `Json`, the built-in classes `Encode`/`Decode`, and `deriving (Encode, Decode)` (D08).

use std::fmt::Write;
use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
    module
}

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message
}

fn ir(source: &str) -> String {
    llvm::emit(&accepts(source), llvm::Entry::Library).unwrap()
}

/// A function that encodes a borrowed `ty` and decodes the JSON value back.
fn round_trip(ty: &str) -> String {
    format!(
        "def round :: ref {ty} -> Result<{ty}, Json.Error>\nfn round value =\n    match Json.encode value with\n    | Ok json -> Json.decode (ref json)\n    | Error error -> Error error\n"
    )
}

#[test]
fn json_module_is_reserved_and_loaded() {
    let error = analyze_modules(&[
        ("Json.tz", "def answer :: i64\nfn answer = 1"),
        ("Main.tz", "()"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1011");
    accepts("let value = Json.Null\nlet items = Json.Items [Json.Bool true, Json.Text \"a\"]\n()");
    accepts("let value = std::Json.Object [(\"a\", std::Json.Null)]\n()");
    // The cases of `Json.Value` are visible unqualified, after the user's own names.
    accepts(
        "union Token = Text of string | Number of i64\nlet token = Text \"a\"\nmatch token with\n| Text _ -> 1\n| Number n -> n",
    );
    accepts("const Text: string = \"hello\"\nString.length (ref Text)");
}

#[test]
fn numeral_functions_type_check() {
    accepts(
        "def check :: utf8string -> i64\nfn check text =\n    match Json.numeral (ref text) with\n    | Some numeral ->\n        let whole: Result<i64, Json.Error> = Json.to_i64 (ref numeral)\n        let real: Result<f64, Json.Error> = Json.to_f64 (ref numeral)\n        if Result.is_ok (ref whole) && Result.is_ok (ref real) then 1 else 0\n    | None -> -1\n",
    );
    rejects("let numeral = Json.numeral \"12\"\n()", "E1003");
    rejects(
        "let value = Json.Number (Json.Numeral { text: \"1\" })\n()",
        "E1003",
    );
}

#[test]
fn parse_signature_and_ir_are_deterministic() {
    let used = ir(
        "def round :: utf8string -> utf8string\nfn round text =\n    match Json.parse (ref text) with\n    | Ok value -> Json.to_utf8string (ref value)\n    | Error error -> Utf8String.from_string (ref (Display.display (ref error)))\n",
    );
    assert!(used.contains("@tz.fn.Json.parse("), "{used}");
    assert!(used.contains("@tz.fn.Json.to_utf8string("));
    let unused = ir("def answer :: i64 -> i64\nfn answer x = x + 1\n");
    assert!(!unused.contains("Json."), "{unused}");
    rejects("let value = Json.parse \"[1]\"\n()", "E1003");
    rejects(
        "let value: Result<Json.Value, string> = Json.parse (ref u8\"1\")\n()",
        "E1003",
    );
}

#[test]
fn builtin_classes_have_json_signatures() {
    accepts(
        "def encode :: ref i64 -> Result<Json.Value, Json.Error>\nfn encode value = Encode.encode value\ndef decode :: ref Json.Value -> Result<i64, Json.Error>\nfn decode json = Decode.decode json\n",
    );
    rejects(
        "def encode :: ref i64 -> Json.Value\nfn encode value = Encode.encode value\n",
        "E1003",
    );
    // A custom std without `Json` type-checks programs that do not use the classes.
    tsuzuri::analyze_modules_with_std(&[("Main.tz", "let x = 1\nx")], &[]).unwrap();
    for (source, class) in [
        (
            "def encode :: ref i64 -> i64\nfn encode value = Encode.encode value\n",
            "Encode",
        ),
        ("record Point { x: i64 } deriving (Decode)\n()", "Decode"),
    ] {
        let error = tsuzuri::analyze_modules_with_std(&[("Main.tz", source)], &[]).unwrap_err();
        assert_eq!(error.code, "E1004", "{source}: {}", error.message);
        assert_eq!(
            error.message,
            format!("{class} needs the standard module Json")
        );
    }
    // `Encode` and `Decode` are reserved class names.
    rejects("record Encode { x: i64 }\n()", "E1001");
    rejects("union Decode = A | B\n()", "E1001");
}

#[test]
fn scalar_instances_type_check() {
    for ty in [
        "bool",
        "i8",
        "i16",
        "i32",
        "i64",
        "i128",
        "i8u",
        "i16u",
        "i32u",
        "i64u",
        "i128u",
        "f32",
        "f64",
        "string",
        "utf8string",
    ] {
        accepts(&round_trip(ty));
    }
}

#[test]
fn user_instances_cannot_overlap_std() {
    let message = rejects(
        "instance Encode<i64> { fn encode _value = Ok Json.Null }\n()",
        "E1016",
    );
    assert_eq!(message, "overlapping instance for Encode<i64>");
    rejects(
        "instance Decode<'a> => Decode<['a]> { fn decode _json = Ok [] }\n()",
        "E1016",
    );
    let meters = "record Meters { value: f64 }\ninstance Encode<Meters> { fn encode meters = Encode.encode (ref meters.value) }\ninstance Decode<Meters> { fn decode json = Result.map (\\value -> Meters { value: value }) (Decode.decode json) }\n";
    accepts(meters);
    accepts(&format!("{meters}{}", round_trip("[Maybe<Meters>]")));
    // The polymorphic `Json` functions may reach every instance, so a method that calls them is
    // part of a recursive group and needs `rec`.
    let polymorphic = "record Meters { value: f64 }\ninstance Decode<Meters> { fn decode json = Result.map (\\value -> Meters { value: value }) (Json.decode json) }\n";
    rejects(polymorphic, "E1019");
    accepts(&polymorphic.replace("fn decode", "fn rec decode"));
}

#[test]
fn container_instances_type_check() {
    for ty in [
        "Maybe<i64>",
        "[string]",
        "[|f64|]",
        "Vec<bool>",
        "(i64 * string)",
        "(i64 * string * bool)",
        "(i8 * i16 * i32 * i64)",
        "Json.Value",
        "Maybe<[(string * Vec<[|Maybe<i64>|]>)]>",
        "[[(Json.Value * utf8string)]]",
    ] {
        accepts(&round_trip(ty));
    }
    accepts(
        "def text :: ref [Maybe<f32>] -> Result<utf8string, Json.Error>\nfn text values = Json.serialize values\ndef back :: ref utf8string -> Result<[Maybe<f32>], Json.Error>\nfn back text = Json.deserialize text\n",
    );
}

#[test]
fn unsupported_types_report_e1005() {
    for ty in [
        "char",
        "utf8char",
        "unit",
        "[i64; 2]",
        "i64 -> i64",
        "(i64 * i64 * i64 * i64 * i64)",
        "Task<i64>",
    ] {
        let message = rejects(
            &format!(
                "def encode :: ref ({ty}) -> Result<Json.Value, Json.Error>\nfn encode value = Json.encode value\n"
            ),
            "E1005",
        );
        assert!(
            message.starts_with("no instance for Encode<"),
            "{ty}: {message}"
        );
        rejects(
            &format!(
                "def decode :: ref Json.Value -> Result<({ty}), Json.Error>\nfn decode json = Json.decode json\n"
            ),
            "E1005",
        );
    }
}

#[test]
fn derives_records_with_constant_depth() {
    let fields = (0..128)
        .map(|index| format!("field{index}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let module = accepts(&format!(
        "record Large {{ {fields} }} deriving (Encode, Decode)\n{}",
        round_trip("Large")
    ));
    assert!(!module.functions.is_empty());
    accepts(&format!(
        "record Empty {{}} deriving (Encode, Decode)\n{}",
        round_trip("Empty")
    ));
    let record = accepts(&format!(
        "record Point {{ x: i64, y: f64 }} deriving (Encode, Decode)\n{}",
        round_trip("Point")
    ));
    let text = llvm::emit(&record, llvm::Entry::Library).unwrap();
    assert!(text.contains("@tz.fn.Json.encode_field"), "{text}");
    assert!(text.contains("@tz.fn.Json.decode_field"));
}

#[test]
fn derives_generic_and_recursive_records() {
    accepts(&format!(
        "record Box<'a> {{ value: 'a }} deriving (Encode, Decode)\n{}{}",
        round_trip("Box<i64>"),
        round_trip("Box<Maybe<[string]>>").replace("round", "round_text")
    ));
    accepts(&format!(
        "record Node {{ value: i64, next: Maybe<Node> }} deriving (Encode, Decode)\n{}",
        round_trip("Node")
    ));
    accepts(&format!(
        "union Tree = Leaf of string | Branch of [Tree] | Pair of Tree * Tree deriving (Encode, Decode)\n{}",
        round_trip("Tree")
    ));
}

#[test]
fn derives_unions_with_constant_depth() {
    let cases = (0..128)
        .map(|index| {
            if index % 2 == 0 {
                format!("Case{index}")
            } else {
                format!("Case{index} of i64 * string")
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");
    accepts(&format!(
        "union Large = {cases} deriving (Encode, Decode)\n{}",
        round_trip("Large")
    ));
    accepts(&format!(
        "union Shape = Circle of f64 | Rect of f64 * f64 | Empty deriving (Encode, Decode)\n{}",
        round_trip("Shape")
    ));
    accepts(&format!(
        "union Single = Only of i64 deriving (Encode, Decode)\nunion Mark = Marked deriving (Encode, Decode)\n{}{}",
        round_trip("Single"),
        round_trip("Mark").replace("round", "round_mark")
    ));
}

#[test]
fn derive_components_without_instances_report_e1025() {
    for source in [
        "record R { f: i64 -> i64 } deriving (Encode)\n()",
        "record R { c: char } deriving (Decode)\n()",
        "union U = A of utf8char | B deriving (Encode)\n()",
        "union U = A of i64 | B of Task<i64> deriving (Decode)\n()",
    ] {
        let message = rejects(source, "E1025");
        assert!(
            message.starts_with("no instance for "),
            "{source}: {message}"
        );
    }
    let message = rejects("record R { value: i64 } deriving (Copy)\n()", "E1025");
    assert_eq!(
        message,
        "only Eq, Ord, Display, Hash, Default, Encode and Decode can be derived"
    );
    rejects(
        "record R { value: i64 } deriving (Encode, Encode)\n()",
        "E1001",
    );
    // A generic record's missing instance is reported where it is used.
    rejects(
        "record Box<'a> { value: 'a } deriving (Decode)\ndef decode :: ref Json.Value -> Result<Box<char>, Json.Error>\nfn decode json = Json.decode json\n",
        "E1005",
    );
}

/// std holds many `Encode` and `Decode` instances. Heads with different outermost constructors
/// cannot overlap, so only pairs with the same constructor count toward the 1024 unified pairs.
#[test]
fn overlap_budget_counts_only_unifiable_heads() {
    let mut records = String::new();
    for index in 0..100 {
        writeln!(
            records,
            "record R{index} {{ value: i64 }} deriving (Eq, Hash, Encode, Decode)"
        )
        .unwrap();
    }
    accepts(&format!("{records}()"));
    let classes = "class Show<'a> { def show :: ref 'a -> i64 }";
    let mut boxes = String::from("record Box<'a> { value: 'a }\n");
    for index in 0..47 {
        writeln!(
            boxes,
            "record S{index} {{ value: i64 }}\ninstance Shows.Show<Box<S{index}>> {{ fn show _value = {index} }}"
        )
        .unwrap();
    }
    let error = analyze_modules(&[("Main.tz", &boxes), ("Shows.tt", classes)]).unwrap_err();
    assert_eq!(error.code, "E1017", "{}", error.message);
    assert_eq!(
        error.message,
        "instance overlap checking exceeds 1024 pairs"
    );
    // A variable head may unify with every other head of its class.
    let mut open = String::from("instance Shows.Show<'a> { fn show _value = 0 }\n");
    for index in 0..3 {
        writeln!(
            open,
            "record S{index} {{ value: i64 }}\ninstance Shows.Show<S{index}> {{ fn show _value = {index} }}"
        )
        .unwrap();
    }
    let error = analyze_modules(&[("Main.tz", &open), ("Shows.tt", classes)]).unwrap_err();
    assert_eq!(error.code, "E1016", "{}", error.message);
    let mut plain = String::new();
    for index in 0..200 {
        writeln!(
            plain,
            "record S{index} {{ value: i64 }}\ninstance Shows.Show<S{index}> {{ fn show _value = {index} }}"
        )
        .unwrap();
    }
    analyze_modules(&[("Main.tz", &plain), ("Shows.tt", classes)]).unwrap();
}

#[test]
fn phase_two_instances_type_check() {
    for ty in [
        "f16",
        "f128",
        "d32",
        "d64",
        "d128",
        "Map<string, i64>",
        "Map<i64, [string]>",
        "Map<(i64 * string), Maybe<f64>>",
        "Set<string>",
        "HashMap<string, Vec<i64>>",
        "HashMap<utf8string, Set<i8>>",
        "HashSet<i64>",
        "Maybe<HashSet<string>>",
    ] {
        accepts(&round_trip(ty));
    }
    accepts(
        "union Color = Red | Green deriving (Encode, Decode, Eq, Ord, Hash)\nrecord Palette { by_name: Map<Color, f16>, used: HashSet<Color> } deriving (Encode, Decode)\ndef text :: ref Palette -> Result<utf8string, Json.Error>\nfn text palette = Json.serialize_pretty palette 2\n",
    );
    // Decoding a map or set needs the key operations of its container.
    let message = rejects(&round_trip("Map<Json.Value, i64>"), "E1005");
    assert!(message.contains("Ord<Json.Value>"), "{message}");
    let message = rejects(&round_trip("HashSet<Json.Value>"), "E1005");
    assert!(message.contains("Hash<Json.Value>"), "{message}");
    accepts(
        "def pretty :: ref Json.Value -> utf8string\nfn pretty value = Json.to_utf8string_pretty value 4\n",
    );
}

#[test]
fn json_attributes_parse_format_and_check() {
    let source = "record User { @json \"user_id\" id: i64, name: string } deriving (Encode, Decode)\nunion Event = | @json \"created\" Created of User | Reset deriving (Encode, Decode)\n";
    let program = tsuzuri::parser::parse(source).unwrap();
    let units = |text: &str| text.encode_utf16().collect::<Vec<_>>();
    assert_eq!(
        program.records[0].fields[0].json.as_ref().unwrap().units,
        units("user_id")
    );
    assert!(program.records[0].fields[1].json.is_none());
    assert_eq!(
        program.unions[0].cases[0].json.as_ref().unwrap().units,
        units("created")
    );
    accepts(&format!("{source}{}", round_trip("[Event]")));
    let formatted = tsuzuri::formatter::format_source(
        "Main.tz",
        "record User {  @json    \"user_id\"   id: i64,@json\"n\" name: string } deriving (Encode)\n",
        tsuzuri::syntax::SourceKind::Code,
    )
    .unwrap();
    assert_eq!(
        formatted.formatted,
        "record User { @json \"user_id\" id: i64, @json \"n\" name: string } deriving (Encode)\n"
    );
    let rendered = tsuzuri::docgen::render_module("Main", &program);
    assert!(rendered.contains("@json \"user_id\" id: i64"), "{rendered}");
    assert!(
        rendered.contains("| @json \"created\" Created of User"),
        "{rendered}"
    );
    for (source, code, message) in [
        (
            "record R { @json \"x\" x: i64 }\n()",
            "E1025",
            "'@json' names a field only for deriving (Encode, Decode); derive one of them or remove the attribute",
        ),
        (
            "record R { @json \"y\" x: i64, y: i64 } deriving (Encode)\n()",
            "E1025",
            "the JSON name \"y\" of field 'y' repeats another; give each field a distinct '@json' name",
        ),
        (
            "union U = @json \"B\" A | B deriving (Decode)\n()",
            "E1025",
            "the JSON name \"B\" of case 'B' repeats another; give each case a distinct '@json' name",
        ),
        (
            "record R { @foo \"x\" x: i64 } deriving (Encode)\n()",
            "E0002",
            "only '@json \"name\"' can come before a record field",
        ),
        (
            "record R { @json u8\"x\" x: i64 } deriving (Encode)\n()",
            "E0002",
            "expected the JSON name as a string literal after '@json', as in '@json \"name\"'",
        ),
        (
            "union U = A | @json \"a\" @json \"b\" B deriving (Encode)\n()",
            "E0002",
            "a union case takes one '@json' attribute",
        ),
    ] {
        assert_eq!(rejects(source, code), message, "{source}");
    }
}

#[test]
fn streaming_reader_and_writer_type_check() {
    accepts(
        "def count :: utf8string -> i64\nfn count text =\n    let mut total = 0\n    let mut state = Json.reader (ref text)\n    let mut going = true\n    while going do\n        match state with\n        | Ok reader ->\n            match Json.next (ref text) reader with\n            | Ok (event, after) ->\n                match event with\n                | Json.KeyToken token -> if Json.token_matches (ref text) (ref token) (ref \"id\") then total = total + 1\n                | Json.EndOfInput -> going = false\n                | _ -> ()\n                state = Ok after\n            | Error error ->\n                going = false\n                state = Error error\n        | Error error ->\n            going = false\n            state = Error error\n    total\n",
    );
    accepts(
        "def write :: unit -> Result<utf8string, Json.Error>\nfn write _unit = Result {\n    let! w0 = Json.open_array (Json.writer ())\n    let! w1 = Json.write_text w0 (ref \"a\")\n    let! w2 = Json.close_array w1\n    return! Json.finish w2\n}\n",
    );
    for source in [
        "let reader = Json.Reader { position: 0, containers: Vec.empty(), keys: Vec.empty(), marks: Vec.empty(), state: 0 }\n0",
        "let writer = Json.writer ()\nwriter.state",
        "let text = u8\"1\"\nlet reader = Result.get (Json.reader (ref text))\nreader.position",
    ] {
        rejects(source, "E1022");
    }
}

#[test]
fn cbor_module_is_reserved_and_typed() {
    let error = analyze_modules(&[
        ("Cbor.tz", "def answer :: i64\nfn answer = 1"),
        ("Main.tz", "()"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1011");
    accepts(
        "record Point { x: i64, y: f64 } deriving (Encode, Decode)\ndef bytes :: ref Point -> Result<[ubyte], Json.Error>\nfn bytes point = Cbor.serialize point\ndef back :: ref [ubyte] -> Result<Point, Json.Error>\nfn back bytes = Cbor.deserialize bytes\ndef raw :: ref [ubyte] -> Result<Json.Value, Json.Error>\nfn raw bytes = Cbor.decode bytes\ndef value :: ref Json.Value -> Result<[ubyte], Json.Error>\nfn value json = Cbor.encode json\n",
    );
    rejects("let bytes = Cbor.serialize (ref 'a')\n()", "E1005");
    let unused = ir("def answer :: i64 -> i64\nfn answer x = x + 1\n");
    assert!(!unused.contains("Cbor."), "{unused}");
}
