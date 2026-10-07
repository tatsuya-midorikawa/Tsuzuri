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
        "f16",
        "d64",
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
