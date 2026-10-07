//! `tsuzuri bindgen` (E11): conversion of Clang's JSON AST, without running Clang.
//! Each AST is the smallest shape Clang writes; the expected lines follow from the C
//! declarations by hand.

use serde_json::{Value, json};
use tsuzuri::bindgen::{
    Arguments, BufferAnnotation, ConsumeAnnotation, Extras, Failure, Generated, HeaderInfo, MARKER,
    ParameterName, Skipped, generate, generate_with, lp64_target, parse_arguments,
};

const PATH: &str = "/h/main.h";

fn header() -> HeaderInfo<'static> {
    HeaderInfo {
        path: PATH,
        file_name: "main.h",
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        clang_version: "clang version 21.0.0",
        target: "arm64-apple-darwin27.0.0",
    }
}

fn unit(nodes: Vec<Value>) -> Value {
    json!({ "kind": "TranslationUnitDecl", "inner": nodes })
}

/// Generates from `nodes`; the first node is placed in the header.
fn run(mut nodes: Vec<Value>) -> Generated {
    if let Some(first) = nodes.first_mut() {
        first["loc"]["file"] = json!(PATH);
    }
    generate(&unit(nodes), &header()).expect("valid AST")
}

/// The declaration lines after the header comments and the blank line.
fn body(generated: &Generated) -> Vec<&str> {
    generated
        .text
        .lines()
        .skip_while(|line| !line.is_empty())
        .skip(1)
        .collect()
}

fn lines(nodes: Vec<Value>) -> Vec<String> {
    body(&run(nodes)).into_iter().map(str::to_owned).collect()
}

fn located(mut node: Value, offset: u64, length: u64) -> Value {
    node["loc"] = json!({ "offset": offset, "tokLen": length });
    node
}

fn function(name: &str, ty: &str, parameters: &[&str]) -> Value {
    json!({
        "kind": "FunctionDecl",
        "name": name,
        "loc": { "offset": 0, "tokLen": name.len() },
        "type": { "qualType": ty },
        "inner": parameters
            .iter()
            .map(|parameter| json!({ "kind": "ParmVarDecl", "type": { "qualType": parameter } }))
            .collect::<Vec<_>>(),
    })
}

fn record(name: &str, fields: &[(&str, &str)]) -> Value {
    json!({
        "kind": "RecordDecl",
        "name": name,
        "tagUsed": "struct",
        "completeDefinition": true,
        "inner": fields
            .iter()
            .map(|(field, ty)| json!({ "kind": "FieldDecl", "name": field, "type": { "qualType": ty } }))
            .collect::<Vec<_>>(),
    })
}

fn typedef(name: &str, ty: &str) -> Value {
    json!({ "kind": "TypedefDecl", "name": name, "type": { "qualType": ty } })
}

fn constant(name: &str, initializer: Option<Value>) -> Value {
    let mut node =
        json!({ "kind": "EnumConstantDecl", "name": name, "type": { "qualType": "int" } });
    if let Some(initializer) = initializer {
        node["inner"] = json!([initializer]);
    }
    node
}

fn value(text: &str, ty: &str) -> Value {
    json!({ "kind": "ConstantExpr", "value": text, "type": { "qualType": ty } })
}

fn enumeration(name: &str, constants: Vec<Value>) -> Value {
    json!({ "kind": "EnumDecl", "name": name, "inner": constants })
}

#[test]
fn accepts_only_lp64_targets() {
    for triple in [
        "arm64-apple-darwin27.0.0",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-macosx14.0.0",
    ] {
        assert!(lp64_target(triple), "{triple}");
    }
    for triple in [
        "x86_64-pc-windows-msvc",
        "x86_64-w64-mingw32",
        "aarch64-pc-windows-msvc",
        "i686-pc-linux-gnu",
        "wasm32-unknown-unknown",
        "armv7-unknown-linux-gnueabihf",
        "x86_64-pc-linux-gnux32",
        "aarch64-unknown-linux-gnu_ilp32",
        "arm64e-apple-darwin",
    ] {
        assert!(!lp64_target(triple), "{triple}");
    }
}

#[test]
fn tracks_the_file_of_each_declaration() {
    let at = |file: &str| json!({ "offset": 0, "file": file, "tokLen": 1 });
    let nodes = vec![
        json!({ "kind": "FunctionDecl", "name": "a", "loc": at(PATH), "type": { "qualType": "int (void)" } }),
        // Clang writes `file` only when it changes: `b` and `c` are in other.h.
        json!({ "kind": "FunctionDecl", "name": "b", "loc": at("/h/other.h"), "type": { "qualType": "int (void)" } }),
        json!({ "kind": "FunctionDecl", "name": "c", "loc": { "offset": 1 }, "range": { "begin": at(PATH), "end": {} }, "type": { "qualType": "int (void)" } }),
        // The range of `c` returned to the header.
        json!({ "kind": "FunctionDecl", "name": "d", "loc": { "offset": 2 }, "type": { "qualType": "int (void)" },
                "inner": [{ "kind": "ParmVarDecl", "loc": at("/h/other.h"), "type": { "qualType": "int" } }] }),
        // The parameter of `d` moved to other.h; `e` follows it there.
        json!({ "kind": "FunctionDecl", "name": "e", "loc": { "offset": 3 }, "type": { "qualType": "int (void)" } }),
        // Expanded from a macro of other.h at the header: the expansion decides.
        json!({ "kind": "FunctionDecl", "name": "f", "loc": { "spellingLoc": at("/h/other.h"), "expansionLoc": at(PATH) }, "type": { "qualType": "int (void)" } }),
        json!({ "kind": "FunctionDecl", "name": "g", "loc": { "offset": 4 }, "isImplicit": true, "type": { "qualType": "int (void)" } }),
        // An include location also spells `file`, which does not change the current file.
        json!({ "kind": "FunctionDecl", "name": "h", "loc": { "offset": 5, "includedFrom": { "file": "/h/other.h" } }, "type": { "qualType": "int (void)" } }),
    ];
    let generated = generate(&unit(nodes), &header()).unwrap();
    assert_eq!(
        body(&generated),
        [
            "extern \"a\" def a :: unit -> i32",
            "extern \"d\" def d :: i32 -> i32",
            "extern \"f\" def f :: unit -> i32",
            "extern \"h\" def h :: unit -> i32",
        ]
    );
}

#[test]
fn maps_scalar_parameters_and_results() {
    let rows = [
        ("signed char", "i8"),
        ("unsigned char", "i8u"),
        ("short", "i16"),
        ("unsigned short", "i16u"),
        ("int", "i32"),
        ("unsigned int", "i32u"),
        ("long", "i64"),
        ("long long", "i64"),
        ("unsigned long", "i64u"),
        ("unsigned long long", "i64u"),
        ("float", "f32"),
        ("double", "f64"),
    ];
    let mut nodes = Vec::new();
    let mut expected = Vec::new();
    for (index, (c, tsuzuri)) in rows.iter().enumerate() {
        let name = format!("f{index}");
        nodes.push(function(
            &name,
            &format!("{c} ({c}, const {c})"),
            &[c, &format!("const {c}")],
        ));
        expected.push(format!(
            "extern \"{name}\" def {name} :: {tsuzuri} -> {tsuzuri} -> {tsuzuri}"
        ));
    }
    nodes.push(function("none", "int (void)", &[]));
    expected.push("extern \"none\" def none :: unit -> i32".into());
    nodes.push(function(
        "effect",
        "void (volatile double)",
        &["volatile double"],
    ));
    expected.push("extern \"effect\" def effect :: f64 -> unit".into());
    // A typedef that Clang desugars at the top level resolves through the spelling.
    let mut desugared = function("sized", "size_t (size_t)", &[]);
    desugared["inner"] = json!([{ "kind": "ParmVarDecl", "type": { "qualType": "size_t", "desugaredQualType": "unsigned long" } }]);
    nodes.push(typedef("size_t", "unsigned long"));
    nodes.push(desugared);
    expected.push("type SizeT = i64u".into());
    expected.push("extern \"sized\" def sized :: i64u -> i64u".into());
    for (name, ty, parameters) in [
        ("plain_char", "int (char)", ["char"]),
        ("long_double", "int (long double)", ["long double"]),
        ("wide", "int (__int128)", ["__int128"]),
        ("half", "int (_Float16)", ["_Float16"]),
        ("complex", "int (_Complex double)", ["_Complex double"]),
        ("pointer", "int (int *)", ["int *"]),
    ] {
        nodes.push(function(name, ty, &parameters));
        expected.push(format!(
            "// skipped {name}: parameter 1 has unsupported type '{}'",
            parameters[0]
        ));
    }
    nodes.push(function("char_result", "char (void)", &[]));
    expected.push("// skipped char_result: result has unsupported type 'char'".into());
    assert_eq!(lines(nodes), expected);
}

#[test]
fn bool_results_become_i8u() {
    assert_eq!(
        lines(vec![
            function("flip", "_Bool (_Bool)", &["_Bool"]),
            typedef("flag_t", "_Bool"),
        ]),
        [
            "extern \"flip\" def flip :: bool -> i8u",
            "type FlagT = bool",
        ]
    );
}

#[test]
fn rejects_unprototyped_and_function_pointer_results() {
    let generated = run(vec![
        function("old", "int ()", &[]),
        function("getter", "int (*(int))(void)", &["int"]),
        function("block", "int (^(void))(int)", &[]),
        function("quit", "void (int) __attribute__((noreturn))", &["int"]),
        function("windows", "int (int) __attribute__((ms_abi))", &["int"]),
    ]);
    assert_eq!(
        body(&generated),
        [
            "// skipped old: function has no prototype",
            "// skipped getter: result has unsupported type 'int (*)(void)'",
            "// skipped block: result has unsupported type 'int (^)(int)'",
            "extern \"quit\" def quit :: i32 -> unit",
            "// skipped windows: function type has the attribute 'ms_abi', which may change its calling convention",
        ]
    );
}

#[test]
fn numbers_enum_constants_like_c() {
    let cast = |inner: Value, ty: &str| json!({ "kind": "ImplicitCastExpr", "castKind": "IntegralCast", "type": { "qualType": ty }, "inner": [inner] });
    assert_eq!(
        lines(vec![
            enumeration(
                "mode",
                vec![
                    constant("A", None),
                    constant("B", Some(value("5", "int"))),
                    constant("C", None),
                ]
            ),
            enumeration(
                "sign",
                vec![
                    constant("NEG", Some(value("-3", "int"))),
                    constant("NEXT", None),
                ]
            ),
            // `enum { BIG = 4294967295, AFTER }`: Clang widens these to `unsigned long`.
            enumeration(
                "big",
                vec![
                    constant(
                        "BIG",
                        Some(cast(value("4294967295", "unsigned int"), "unsigned long"))
                    ),
                    constant("AFTER", None),
                    constant("LAST", Some(value("1", "int"))),
                ]
            ),
            // `X = 1u` and `Y = -1L` convert the initializer to `int`.
            enumeration(
                "cast",
                vec![
                    constant("X", Some(cast(value("1", "unsigned int"), "int"))),
                    constant("Y", Some(cast(value("-1", "long"), "int"))),
                    constant("Z", None),
                    constant(
                        "W",
                        Some(json!({ "kind": "BinaryOperator", "type": { "qualType": "int" } }))
                    ),
                    constant("V", None),
                ]
            ),
        ]),
        [
            "const A: i32 = 0",
            "const B: i32 = 5",
            "const C: i32 = 6",
            "const NEG: i32 = -3",
            "const NEXT: i32 = -2",
            "// skipped BIG: enumerator value 4294967295 does not fit in i32",
            "// skipped AFTER: enumerator value 4294967296 does not fit in i32",
            "const LAST: i32 = 1",
            "const X: i32 = 1",
            "const Y: i32 = -1",
            "const Z: i32 = 0",
            "// skipped W: enumerator value is not an integer constant in the clang AST",
            "// skipped V: enumerator value is not an integer constant in the clang AST",
        ]
    );
    // Only an enum whose every constant fits in i32 is an `int` parameter.
    assert_eq!(
        lines(vec![
            enumeration(
                "big",
                vec![constant("HUGE", Some(value("4294967296", "long")))]
            ),
            function(
                "take",
                "void (enum big, enum mode)",
                &["enum big", "enum mode"]
            ),
        ])[1],
        "// skipped take: parameter 1 has unsupported type 'enum big'"
    );
}

#[test]
fn resolves_typedef_chains_and_stops_after_32() {
    let mut nodes = vec![
        typedef("base_t", "long long"),
        typedef("middle_t", "base_t"),
        typedef("top_t", "const middle_t"),
        function("chain", "top_t (top_t)", &["top_t"]),
    ];
    // t0 = int, t1 = t0, ..., t32 = t31: naming t31 expands 32 typedefs, t32 expands 33.
    nodes.push(typedef("t0", "int"));
    for level in 1..=32 {
        nodes.push(typedef(&format!("t{level}"), &format!("t{}", level - 1)));
    }
    nodes.push(function("deep", "void (t31)", &["t31"]));
    nodes.push(function("deeper", "void (t32)", &["t32"]));
    let generated = lines(nodes);
    assert_eq!(
        &generated[..4],
        [
            "type BaseT = i64",
            "type MiddleT = i64",
            "type TopT = i64",
            "extern \"chain\" def chain :: i64 -> i64",
        ]
    );
    assert!(generated.contains(&"type T31 = i32".to_owned()));
    assert!(!generated.iter().any(|line| line.starts_with("type T32")));
    assert_eq!(
        generated[generated.len() - 2..],
        [
            "extern \"deep\" def deep :: i32 -> unit",
            "// skipped deeper: parameter 1 has unsupported type 't32'",
        ]
    );
}

#[test]
fn generates_scalar_records_and_ref_parameters() {
    let mut union = record("u", &[("a", "int")]);
    union["tagUsed"] = json!("union");
    let mut anonymous = record("", &[("x", "int")]);
    anonymous["id"] = json!("0x2");
    let mut bits = record("bits", &[("a", "int"), ("b", "int")]);
    bits["inner"][0]["isBitfield"] = json!(true);
    let mut packed = record("packed", &[("a", "int")]);
    packed["inner"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({ "kind": "PackedAttr" }));
    assert_eq!(
        lines(vec![
            // Declared before its definition: functions may borrow it before it is complete.
            json!({ "kind": "RecordDecl", "name": "point", "tagUsed": "struct" }),
            function(
                "before",
                "double (const struct point *)",
                &["const struct point *"]
            ),
            record(
                "point",
                &[
                    ("x", "double"),
                    ("y", "double"),
                    ("tag", "int"),
                    ("kind", "enum kind"),
                    ("count", "unsigned long")
                ]
            ),
            enumeration("kind", vec![constant("ONE", Some(value("1", "int")))]),
            typedef("point_t", "struct point"),
            function("norm", "double (const point_t *)", &["const point_t *"]),
            function(
                "constant",
                "double (const struct point *const)",
                &["const struct point *const"]
            ),
            function("mutable", "void (struct point *)", &["struct point *"]),
            function("by_value", "double (struct point)", &["struct point"]),
            function("make", "struct point (void)", &[]),
            record("narrow", &[("small", "signed char"), ("x", "int")]),
            record("flags", &[("on", "_Bool")]),
            union,
            anonymous,
            json!({ "kind": "TypedefDecl", "name": "anon_t", "type": { "qualType": "struct anon_t" },
                    "inner": [{ "kind": "ElaboratedType", "ownedTagDecl": { "id": "0x2", "kind": "RecordDecl", "name": "" } }] }),
            bits,
            packed,
            record("pointer", &[("next", "struct pointer *")]),
            record("array", &[("values", "int[4]")]),
            json!({ "kind": "RecordDecl", "name": "empty", "tagUsed": "struct", "completeDefinition": true }),
        ]),
        [
            "extern \"before\" def before :: ref Point -> f64",
            "record Point { x: f64, y: f64, tag: i32, kind: i32, count: i64u }",
            "const ONE: i32 = 1",
            "extern \"norm\" def norm :: ref Point -> f64",
            "extern \"constant\" def constant :: ref Point -> f64",
            "// skipped mutable: parameter 1 has unsupported type 'struct point *'",
            "// skipped by_value: parameter 1 has unsupported type 'struct point'",
            "// skipped make: result has unsupported type 'struct point'",
            "// skipped narrow: field 'small' has unsupported type 'signed char'",
            "// skipped flags: field 'on' has unsupported type '_Bool'",
            "// skipped u: C union",
            "// skipped anon_t: anonymous struct; give the struct a tag",
            "// skipped bits: bit-field 'a'",
            "// skipped packed: struct has __attribute__((packed)), which may change its layout",
            "// skipped pointer: field 'next' has unsupported type 'struct pointer *'",
            "// skipped array: field 'values' has unsupported type 'int[4]'",
            "// skipped empty: struct has no fields",
        ]
    );
}

#[test]
fn converts_names() {
    let mut nodes = vec![
        function("glClearColor", "void (void)", &[]),
        function("SDL_Init", "int (unsigned int)", &["unsigned int"]),
        function("XMLParse", "int (void)", &[]),
        function("add", "int (int, int)", &["int", "int"]),
        function("type", "int (void)", &[]),
        function("sqrt", "double (double)", &["double"]),
        function("to_string", "int (void)", &[]),
        function("a$b", "int (void)", &[]),
        function("tz_x", "int (void)", &[]),
        function("fooBar", "int (void)", &[]),
        function("foo_bar", "int (void)", &[]),
        record("point", &[("type", "int"), ("camelCase", "int")]),
        record("point_t", &[("x", "int")]),
        record("SDL_Rect", &[("x", "int")]),
        record("Vec", &[("x", "int")]),
        record("Display", &[("x", "int")]),
        typedef("_1x", "int"),
        // Tags and typedef names are separate C namespaces but one Tsuzuri namespace.
        typedef("point", "int"),
        enumeration(
            "words",
            vec![
                constant("match", None),
                constant("_", None),
                constant("abs", None),
                constant("red", None),
            ],
        ),
        function("RED", "int (void)", &[]),
    ];
    nodes.push(record("twice", &[("fooBar", "int"), ("foo_bar", "int")]));
    assert_eq!(
        lines(nodes),
        [
            "extern \"glClearColor\" def gl_clear_color :: unit -> unit",
            "extern \"SDL_Init\" def sdl_init :: i32u -> i32",
            "extern \"XMLParse\" def xmlparse :: unit -> i32",
            "extern \"add\" def add :: i32 -> i32 -> i32",
            "extern \"type\" def type_ :: unit -> i32",
            "extern \"sqrt\" def sqrt_ :: f64 -> f64",
            "extern \"to_string\" def to_string_ :: unit -> i32",
            "// skipped a$b: name is not an ASCII C identifier",
            "// skipped tz_x: symbol uses the reserved tz_, tsuzuri, or __ prefix",
            "extern \"fooBar\" def foo_bar :: unit -> i32",
            "// skipped foo_bar: name 'foo_bar' collides with an earlier declaration",
            "record Point { type_: i32, camel_case: i32 }",
            "record PointT { x: i32 }",
            "record SDLRect { x: i32 }",
            "record Vec_ { x: i32 }",
            "record Display_ { x: i32 }",
            "type C1x = i32",
            "// skipped point: name 'Point' collides with an earlier declaration",
            "const match_: i32 = 0",
            "const __: i32 = 1",
            "const abs_: i32 = 2",
            "const red: i32 = 3",
            "// skipped RED: name 'red' collides with an earlier declaration",
            "// skipped twice: field 'foo_bar' collides with an earlier field as 'foo_bar'",
        ]
    );
}

#[test]
fn reports_skip_reasons_in_order() {
    let mut nodes = vec![
        function("internal", "int (void)", &[]),
        function("inlined", "int (void)", &[]),
        function("variadic", "int (int, ...)", &["int"]),
        function("unprototyped", "int ()", &[]),
        function("dollar$", "int (void)", &[]),
        function("__reserved", "int (void)", &[]),
        function("malloc", "void *(unsigned long)", &["unsigned long"]),
        function("renamed", "int (void)", &[]),
        function("parameter", "int (int, char)", &["int", "char"]),
        function("result", "char (int)", &["int"]),
        function("first", "int (void)", &[]),
        function("First", "int (void)", &[]),
        json!({ "kind": "VarDecl", "name": "counter", "type": { "qualType": "int" } }),
    ];
    // `static inline` reports linkage before `inline`; `variadic` is checked before
    // the prototype, and the name before the types.
    nodes[0]["storageClass"] = json!("static");
    nodes[0]["inline"] = json!(true);
    nodes[1]["inline"] = json!(true);
    nodes[2]["variadic"] = json!(true);
    nodes[7]["mangledName"] = json!("_renamed$UNIX2003");
    for (index, node) in nodes.iter_mut().enumerate() {
        node["loc"] = json!({ "offset": 100 + index * 10, "tokLen": index + 1 });
    }
    let generated = run(nodes);
    let reasons = [
        ("internal", "function has internal linkage (static)"),
        (
            "inlined",
            "inline function has no guaranteed external definition",
        ),
        ("variadic", "variadic function"),
        ("unprototyped", "function has no prototype"),
        ("dollar$", "name is not an ASCII C identifier"),
        (
            "__reserved",
            "symbol uses the reserved tz_, tsuzuri, or __ prefix",
        ),
        ("malloc", "symbol is reserved by the Tsuzuri runtime"),
        (
            "renamed",
            "symbol is renamed to '_renamed$UNIX2003' by an asm label or overloading",
        ),
        ("parameter", "parameter 2 has unsupported type 'char'"),
        ("result", "result has unsupported type 'char'"),
        ("First", "name 'first' collides with an earlier declaration"),
        ("counter", "global variable"),
    ];
    let expected: Vec<_> = reasons
        .iter()
        .map(|(name, reason)| {
            let index = [
                "internal",
                "inlined",
                "variadic",
                "unprototyped",
                "dollar$",
                "__reserved",
                "malloc",
                "renamed",
                "parameter",
                "result",
                "first",
                "First",
                "counter",
            ]
            .iter()
            .position(|candidate| candidate == name)
            .unwrap();
            Skipped {
                name: (*name).to_owned(),
                reason: (*reason).to_owned(),
                offset: 100 + index * 10,
                length: index + 1,
            }
        })
        .collect();
    assert_eq!(generated.skipped, expected);
    let skipped_lines: Vec<_> = body(&generated)
        .into_iter()
        .filter(|line| line.starts_with("// skipped"))
        .collect();
    assert_eq!(
        skipped_lines,
        reasons
            .iter()
            .map(|(name, reason)| format!("// skipped {name}: {reason}"))
            .collect::<Vec<_>>()
    );
    assert!(body(&generated).contains(&"extern \"first\" def first :: unit -> i32"));
}

#[test]
fn sanitizes_comment_text() {
    let generated = generate(
        &unit(vec![located(
            json!({ "kind": "VarDecl", "name": "caf\u{e9}\nx", "type": { "qualType": "int" } }),
            0,
            1,
        )]),
        &HeaderInfo {
            path: "/h/caf\u{e9}\n.h",
            file_name: "caf\u{e9}\nextern \"x\" def x :: i64.h",
            sha256: "00",
            clang_version: "clang\r\nversion",
            target: "arm64-apple-darwin\u{7f}",
        },
    )
    .unwrap();
    assert_eq!(
        generated.text,
        format!(
            "{MARKER}\n// header: caf??extern \"x\" def x :: i64.h sha256=00\n// clang: clang??version\n// target: arm64-apple-darwin?\n"
        )
    );
    let generated = run(vec![
        json!({ "kind": "VarDecl", "name": "caf\u{e9}\nx", "type": { "qualType": "int" } }),
        function(
            "anonymous",
            "int (struct (unnamed struct at /home/me/h.h:1:2))",
            &["struct (unnamed struct at /home/me/h.h:1:2)"],
        ),
    ]);
    assert_eq!(
        body(&generated),
        [
            "// skipped caf??x: global variable",
            "// skipped anonymous: parameter 1 has unsupported type 'struct (unnamed struct)'",
        ]
    );
    assert!(!generated.text.contains("/home/me"));
}

#[test]
fn output_is_deterministic() {
    let nodes = || {
        vec![
            typedef("count_t", "int"),
            record("pair", &[("a", "int"), ("b", "double")]),
            function("sum", "int (const struct pair *)", &["const struct pair *"]),
            json!({ "kind": "VarDecl", "name": "global", "type": { "qualType": "int" } }),
        ]
    };
    let first = run(nodes());
    let second = run(nodes());
    assert_eq!(first, second);
    assert!(!first.text.contains("/h/"));
    assert!(first.text.starts_with(MARKER));
    assert!(first.text.ends_with("// skipped global: global variable\n"));
    assert!(!first.text.ends_with("\n\n"));
    // A header without declarations still ends with one newline.
    let empty = generate(&unit(Vec::new()), &header()).unwrap();
    assert!(empty.text.ends_with("darwin27.0.0\n") && empty.skipped.is_empty());
}

#[test]
fn builtin_redeclarations_still_count() {
    // Clang declares `sqrt` implicitly before a header's own declaration of it.
    let mut implicit = function("sqrt", "double (double)", &["double"]);
    implicit["isImplicit"] = json!(true);
    implicit["id"] = json!("0x10");
    let mut explicit = function("sqrt", "double (double)", &["double"]);
    explicit["previousDecl"] = json!("0x10");
    let mut redeclared = function("sqrt", "double (double)", &["double"]);
    redeclared["previousDecl"] = json!("0x11");
    explicit["id"] = json!("0x11");
    assert_eq!(
        lines(vec![
            function("first", "int (void)", &[]),
            implicit,
            explicit,
            redeclared,
        ]),
        [
            "extern \"first\" def first :: unit -> i32",
            "extern \"sqrt\" def sqrt_ :: f64 -> f64",
        ]
    );
}

#[test]
fn rejects_malformed_asts() {
    assert!(generate(&json!({ "kind": "TranslationUnitDecl" }), &header()).is_err());
    assert!(generate(&unit(vec![json!({ "name": "x" })]), &header()).is_err());
}

#[test]
fn parses_bindgen_arguments() {
    let parse = |values: &[&str]| {
        parse_arguments(
            &values
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        parse(&[
            "sample.h",
            "-o",
            "Sample.tz",
            "--include-dir",
            "a",
            "--json",
            "--include-dir",
            "b"
        ]),
        Ok(Arguments {
            header: "sample.h".into(),
            output: "Sample.tz".into(),
            include_dirs: vec!["a".into(), "b".into()],
            buffers: Vec::new(),
            consumes: Vec::new(),
            json: true,
        })
    );
    let annotated = parse(&[
        "a.h",
        "-o",
        "A.tz",
        "--buffer",
        "sum:values:count",
        "--buffer",
        "mean:1:2",
        "--consume",
        "close:handle",
        "--consume",
        "close:2",
    ])
    .unwrap();
    assert_eq!(
        annotated.buffers,
        [
            BufferAnnotation {
                function: "sum".into(),
                pointer: ParameterName::Name("values".into()),
                length: ParameterName::Name("count".into()),
            },
            BufferAnnotation {
                function: "mean".into(),
                pointer: ParameterName::Position(1),
                length: ParameterName::Position(2),
            },
        ]
    );
    assert_eq!(
        annotated.consumes,
        [
            ConsumeAnnotation {
                function: "close".into(),
                parameter: ParameterName::Name("handle".into()),
            },
            ConsumeAnnotation {
                function: "close".into(),
                parameter: ParameterName::Position(2),
            },
        ]
    );
    let buffer_usage = "--buffer takes FUNCTION:POINTER:LENGTH with C names or 1-based parameter positions, such as --buffer sum:values:count";
    let consume_usage = "--consume takes FUNCTION:PARAMETER with a C name or a 1-based parameter position, such as --consume close:handle";
    for (values, message) in [
        (&["a.h", "-o", "A.tz", "--buffer"][..], buffer_usage),
        (
            &["a.h", "-o", "A.tz", "--buffer", "sum:values"][..],
            buffer_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--buffer", "sum:values:count:x"][..],
            buffer_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--buffer", "s$um:1:2"][..],
            buffer_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--buffer", "sum:0:1"][..],
            buffer_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--buffer", "sum::1"][..],
            buffer_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--consume", "close"][..],
            consume_usage,
        ),
        (
            &["a.h", "-o", "A.tz", "--consume", "close:-1"][..],
            consume_usage,
        ),
        (
            &[
                "a.h", "-o", "A.tz", "--buffer", "f:p:a", "--buffer", "f:p:b",
            ][..],
            "--buffer f:p is specified more than once",
        ),
        (
            &["a.h", "-o", "A.tz", "--consume", "f:1", "--consume", "f:1"][..],
            "--consume f:1 is specified more than once",
        ),
    ] {
        assert_eq!(parse(values), Err(message.to_owned()), "{values:?}");
    }
    assert_eq!(
        parse(&["--output", "out/Lib.tz", "--", "-odd.h"])
            .unwrap()
            .header,
        std::path::PathBuf::from("-odd.h")
    );
    for (values, message) in [
        (&[][..], "bindgen requires exactly one header path"),
        (
            &["a.h", "b.h", "-o", "A.tz"][..],
            "bindgen requires exactly one header path",
        ),
        (&["a.h"][..], "bindgen requires -o OUTPUT.tz"),
        (
            &["a.h", "-o", "a.txt"][..],
            "bindgen output must have the .tz extension",
        ),
        (
            &["a.h", "-o", "A.tz", "--target", "wasm32"][..],
            "bindgen accepts only -o, --include-dir, --buffer, --consume, and --json",
        ),
        (
            &["a.h", "-o", "A.tz", "-O3"][..],
            "bindgen accepts only -o, --include-dir, --buffer, --consume, and --json",
        ),
        (
            &["a.h", "-o", "A.tz", "-o", "B.tz"][..],
            "output specified more than once",
        ),
        (&["a.h", "-o"][..], "--output needs a value"),
        (
            &["a.h", "-o", "A.tz", "--include-dir"][..],
            "--include-dir needs a value",
        ),
    ] {
        assert_eq!(parse(values), Err(message.to_owned()), "{values:?}");
    }
    assert!(
        parse(&["a.h", "-o", "lower.tz"])
            .unwrap_err()
            .contains("module file name")
    );
    assert!(parse(&["a.h", "-o", "Two-Words.tz"]).is_err());
}

// ---------------------------------------------------------------------------
// Phase 2

/// The header text and the `clang -E -dD` output for a header made of `lines`: the
/// predefined macros come first, as in Clang's output.
fn preprocessed(lines: &[&str]) -> (String, String) {
    let header: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let output = format!(
        "# 1 \"{PATH}\"\n# 1 \"<built-in>\" 1\n# 1 \"<built-in>\" 3\n#define __STDC__ 1\n#define ZERO 0\n# 1 \"<command line>\" 1\n# 1 \"<built-in>\" 2\n# 1 \"{PATH}\" 2\n{header}"
    );
    (header, output)
}

fn with_macros(nodes: Vec<Value>, lines: &[&str]) -> Generated {
    let (text, output) = preprocessed(lines);
    let mut nodes = nodes;
    if let Some(first) = nodes.first_mut() {
        first["loc"]["file"] = json!(PATH);
    }
    generate_with(
        &unit(nodes),
        &header(),
        &Extras {
            preprocessed: Some(&output),
            header_text: text.as_bytes(),
            ..Extras::default()
        },
    )
    .expect("valid input")
}

fn annotated(
    nodes: Vec<Value>,
    buffers: &[BufferAnnotation],
    consumes: &[ConsumeAnnotation],
) -> Result<Generated, Failure> {
    let mut nodes = nodes;
    if let Some(first) = nodes.first_mut() {
        first["loc"]["file"] = json!(PATH);
    }
    generate_with(
        &unit(nodes),
        &header(),
        &Extras {
            buffers,
            consumes,
            ..Extras::default()
        },
    )
}

fn buffer(function: &str, pointer: &str, length: &str) -> BufferAnnotation {
    let name = |text: &str| match text.parse() {
        Ok(position) => ParameterName::Position(position),
        Err(_) => ParameterName::Name(text.into()),
    };
    BufferAnnotation {
        function: function.into(),
        pointer: name(pointer),
        length: name(length),
    }
}

fn consume(function: &str, parameter: &str) -> ConsumeAnnotation {
    ConsumeAnnotation {
        function: function.into(),
        parameter: match parameter.parse() {
            Ok(position) => ParameterName::Position(position),
            Err(_) => ParameterName::Name(parameter.into()),
        },
    }
}

fn named_function(name: &str, ty: &str, parameters: &[(&str, &str)]) -> Value {
    let mut node = function(name, ty, &[]);
    node["inner"] = json!(
        parameters
            .iter()
            .map(|(parameter, ty)| json!({ "kind": "ParmVarDecl", "name": parameter, "type": { "qualType": ty } }))
            .collect::<Vec<_>>()
    );
    node
}

#[test]
fn types_integer_macros_like_c_on_lp64() {
    // Each type follows C17 6.4.4.1 with 32-bit int and 64-bit long, worked out by hand.
    let cases = [
        ("D1 42", "const D1: i32 = 42"),
        ("D2 2147483647", "const D2: i32 = 2147483647"),
        ("D3 2147483648", "const D3: i64 = 2147483648"),
        ("D4 0x7FFFFFFF", "const D4: i32 = 2147483647"),
        ("D5 0x80000000", "const D5: i32u = 2147483648"),
        ("D6 0xffffffff", "const D6: i32u = 4294967295"),
        ("D7 0x100000000", "const D7: i64 = 4294967296"),
        (
            "D8 0x8000000000000000",
            "const D8: i64u = 9223372036854775808",
        ),
        (
            "D9 9223372036854775807",
            "const D9: i64 = 9223372036854775807",
        ),
        (
            "D10 9223372036854775808u",
            "const D10: i64u = 9223372036854775808",
        ),
        (
            "D11 18446744073709551615U",
            "const D11: i64u = 18446744073709551615",
        ),
        ("D12 017", "const D12: i32 = 15"),
        ("D13 0b101", "const D13: i32 = 5"),
        ("D14 0", "const D14: i32 = 0"),
        ("D15 1U", "const D15: i32u = 1"),
        ("D16 1L", "const D16: i64 = 1"),
        ("D17 1UL", "const D17: i64u = 1"),
        ("D18 1LL", "const D18: i64 = 1"),
        ("D19 1ull", "const D19: i64u = 1"),
        ("D20 1lu", "const D20: i64u = 1"),
        ("D21 1LLU", "const D21: i64u = 1"),
        ("D22 0x1l", "const D22: i64 = 1"),
        (
            "D23 0xFFFFFFFFFFFFFFFFl",
            "const D23: i64u = 18446744073709551615",
        ),
        ("D24 (-1)", "const D24: i32 = -1"),
        ("D25 -(1)", "const D25: i32 = -1"),
        ("D26 - -1", "const D26: i32 = 1"),
        ("D27 (-(-(2)))", "const D27: i32 = 2"),
        ("D28 (-1u)", "const D28: i32u = 4294967295"),
        ("D29 -0x80000000", "const D29: i32u = 2147483648"),
        ("D30 -2147483648", "const D30: i64 = -2147483648"),
        (
            "D31 (-9223372036854775807L)",
            "const D31: i64 = -9223372036854775807",
        ),
        ("D32 -1ull", "const D32: i64u = 18446744073709551615"),
        ("D33 ((7))", "const D33: i32 = 7"),
        // Reported: integer literals without an exact Tsuzuri value.
        (
            "E1 9223372036854775808",
            "// skipped E1: integer literal '9223372036854775808' does not fit in a signed 64-bit type; C gives it no type without a 'u' suffix",
        ),
        (
            "E2 18446744073709551616",
            "// skipped E2: integer literal '18446744073709551616' does not fit in 64 bits",
        ),
        (
            "E3 09",
            "// skipped E3: '09' is not a valid C integer literal",
        ),
        (
            "E4 1lL",
            "// skipped E4: '1lL' is not a valid C integer literal",
        ),
        (
            "E5 1f",
            "// skipped E5: integer literal '1f' has a suffix that bindgen does not convert",
        ),
        (
            "E6 0x",
            "// skipped E6: '0x' is not a valid C integer literal",
        ),
        (
            "E7 1wb",
            "// skipped E7: integer literal '1wb' has a suffix that bindgen does not convert",
        ),
        (
            "E8 0b2",
            "// skipped E8: '0b2' is not a valid C integer literal",
        ),
    ];
    let mut lines: Vec<String> = cases
        .iter()
        .map(|(body, _)| format!("#define {body}"))
        .collect();
    // Ignored: not one integer literal, function-like, or undefined again.
    lines.extend(
        [
            "#define F1 1.5",
            "#define F2 1e3",
            "#define F3 0x1p3",
            "#define F4 (1 << 2)",
            "#define F5 \"s\"",
            "#define F6 'c'",
            "#define F7 --1",
            "#define F8 +1",
            "#define F9 D1",
            "#define F10",
            "#define F11(x) (x)",
            "#define F12 ()",
            "#define F13 (1",
            "#define U1 1",
            "#undef U1",
        ]
        .map(String::from),
    );
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let generated = with_macros(Vec::new(), &refs);
    let expected: Vec<&str> = cases.iter().map(|(_, line)| *line).collect();
    assert_eq!(body(&generated), expected);
    assert!(!generated.text.contains("__STDC__") && !generated.text.contains("ZERO"));
}

#[test]
fn places_macros_in_header_order_and_ownership() {
    let header_lines = [
        "#define FIRST 1",
        "int f(void);",
        "#define type 2",
        "#define sqrt 3",
        "#define fooBar 4",
        "int foo_bar(void);",
        "#undef REDEFINED",
        "#define REDEFINED 5",
        "#define DROPPED 6",
        "#define caf\u{e9} 7",
    ];
    let (text, mut output) = preprocessed(&header_lines);
    // An included file: its macros are not the header's, and it undefines DROPPED.
    output.push_str(&format!(
        "# 1 \"/h/other.h\" 1\n#define OTHER 8\n#define REDEFINED 9\n#undef DROPPED\n# 11 \"{PATH}\" 2\n"
    ));
    let f = text.find("f(void)").unwrap() as u64;
    let foo_bar = text.find("foo_bar(void)").unwrap() as u64;
    let nodes = vec![
        located(function("f", "int (void)", &[]), f, 1),
        located(function("foo_bar", "int (void)", &[]), foo_bar, 7),
    ];
    let mut nodes = nodes;
    nodes[0]["loc"]["file"] = json!(PATH);
    let generated = generate_with(
        &unit(nodes),
        &header(),
        &Extras {
            preprocessed: Some(&output),
            header_text: text.as_bytes(),
            ..Extras::default()
        },
    )
    .unwrap();
    assert_eq!(
        body(&generated),
        [
            "const FIRST: i32 = 1",
            "extern \"f\" def f :: unit -> i32",
            "const type_: i32 = 2",
            "const sqrt_: i32 = 3",
            "const fooBar: i32 = 4",
            "extern \"foo_bar\" def foo_bar :: unit -> i32",
            "// skipped caf\u{e9}: name is not an ASCII C identifier"
                .replace('\u{e9}', "?")
                .as_str(),
        ]
    );
    // REDEFINED was last defined in other.h, so it is not the header's.
    assert!(!generated.text.contains("REDEFINED") && !generated.text.contains("OTHER"));
    let skipped = &generated.skipped[0];
    assert_eq!(skipped.name, "caf\u{e9}");
    assert_eq!(
        &text[skipped.offset..skipped.offset + skipped.length],
        "caf\u{e9}"
    );
    // A macro defined after a function named the same in Tsuzuri collides.
    let generated = with_macros(
        vec![located(function("fooBar", "int (void)", &[]), 0, 6)],
        &["int fooBar(void);", "#define foo_bar 1"],
    );
    assert_eq!(
        body(&generated),
        [
            "extern \"fooBar\" def foo_bar :: unit -> i32",
            "// skipped foo_bar: name 'foo_bar' collides with an earlier declaration",
        ]
    );
}

#[test]
fn maps_opaque_structs_to_handles() {
    let forward = |tag: &str| json!({ "kind": "RecordDecl", "name": tag, "tagUsed": "struct" });
    let mut redeclared = forward("handle");
    redeclared["previousDecl"] = json!("0x1");
    let mut elsewhere = forward("elsewhere");
    elsewhere["loc"] = json!({ "offset": 0, "file": "/h/other.h" });
    let mut back = forward("elsewhere");
    back["loc"] = json!({ "offset": 0, "file": PATH });
    back["previousDecl"] = json!("0x2");
    let mut outer = record("outer", &[("x", "int")]);
    outer["inner"]
        .as_array_mut()
        .unwrap()
        .push(record("nested", &[("y", "int")]));
    let generated = annotated(
        vec![
            forward("handle"),
            typedef("handle_t", "struct handle"),
            typedef("handle_ref", "struct handle *"),
            redeclared,
            function("handle_new", "struct handle *(long)", &["long"]),
            function(
                "handle_use",
                "int (handle_t *, const struct handle *, handle_ref, const handle_ref)",
                &[
                    "handle_t *",
                    "const struct handle *",
                    "handle_ref",
                    "const handle_ref",
                ],
            ),
            function("handle_shared", "const struct handle *(void)", &[]),
            function(
                "handle_open",
                "int (struct handle **)",
                &["struct handle **"],
            ),
            function(
                "handle_volatile",
                "int (volatile struct handle *)",
                &["volatile struct handle *"],
            ),
            named_function(
                "handle_close",
                "long (struct handle *)",
                &[("handle", "struct handle *")],
            ),
            forward("later"),
            function(
                "later_use",
                "int (struct later *, const struct later *)",
                &["struct later *", "const struct later *"],
            ),
            record("later", &[("x", "int")]),
            elsewhere,
            back,
            function(
                "elsewhere_use",
                "int (struct elsewhere *)",
                &["struct elsewhere *"],
            ),
            outer,
            forward("nested"),
            function("nested_use", "int (struct nested *)", &["struct nested *"]),
            record("point", &[("x", "int")]),
            forward("Point"),
            forward("Vec"),
        ],
        &[],
        &[consume("handle_close", "handle")],
    )
    .unwrap();
    assert_eq!(
        body(&generated),
        [
            "extern type Handle",
            "extern \"handle_new\" def handle_new :: i64 -> Handle",
            "extern \"handle_use\" def handle_use :: ref Handle -> ref Handle -> ref Handle -> ref Handle -> i32",
            "// skipped handle_shared: result has unsupported type 'const struct handle *'",
            "// skipped handle_open: parameter 1 has unsupported type 'struct handle **'",
            "// skipped handle_volatile: parameter 1 has unsupported type 'volatile struct handle *'",
            "extern \"handle_close\" def handle_close :: Handle -> i64",
            "// skipped later_use: parameter 1 has unsupported type 'struct later *'",
            "record Later { x: i32 }",
            "// skipped elsewhere_use: parameter 1 has unsupported type 'struct elsewhere *'",
            "record Outer { x: i32 }",
            "// skipped nested_use: parameter 1 has unsupported type 'struct nested *'",
            "record Point { x: i32 }",
            "// skipped Point: name 'Point' collides with an earlier declaration",
            "extern type Vec_",
        ]
    );
    // --consume on a parameter that is not an opaque handle skips the function.
    let generated = annotated(
        vec![named_function(
            "close_int",
            "int (int)",
            &[("value", "int")],
        )],
        &[],
        &[consume("close_int", "value")],
    )
    .unwrap();
    assert_eq!(
        body(&generated),
        [
            "// skipped close_int: --consume names parameter 1, which is not a pointer to an opaque struct of the header"
        ]
    );
}

#[test]
fn maps_function_pointer_parameters_to_callbacks() {
    let forward = json!({ "kind": "RecordDecl", "name": "handle", "tagUsed": "struct" });
    assert_eq!(
        lines(vec![
            forward,
            typedef("cmp_fn", "int (*)(int, int)"),
            function(
                "apply",
                "int (int (*)(int, int), int)",
                &["int (*)(int, int)", "int"]
            ),
            function("apply_typedef", "int (cmp_fn)", &["cmp_fn"]),
            function("run", "void (void (*)(void))", &["void (*)(void)"]),
            function(
                "test",
                "_Bool (_Bool (*)(_Bool, short), unsigned char)",
                &["_Bool (*)(_Bool, short)", "unsigned char"]
            ),
            function(
                "visit",
                "long (long (*)(const struct handle *), struct handle *)",
                &["long (*)(const struct handle *)", "struct handle *"]
            ),
            function(
                "nullable",
                "int (int (* _Nonnull)(int))",
                &["int (* _Nonnull)(int)"]
            ),
            function(
                "context",
                "void (void (*)(void *), void *)",
                &["void (*)(void *)", "void *"]
            ),
            function(
                "variadic",
                "void (int (*)(int, ...))",
                &["int (*)(int, ...)"]
            ),
            function("unprototyped", "void (int (*)())", &["int (*)()"]),
            function(
                "pointer_result",
                "void (int *(*)(void))",
                &["int *(*)(void)"]
            ),
            function(
                "nested",
                "void (int (*)(int (*)(int)))",
                &["int (*)(int (*)(int))"]
            ),
            function("block", "void (int (^)(int))", &["int (^)(int)"]),
            function("plain_char", "void (char (*)(char))", &["char (*)(char)"]),
            function(
                "convention",
                "void (void (*)(int) __attribute__((ms_abi)))",
                &["void (*)(int) __attribute__((ms_abi))"]
            ),
            function(
                "pointer_to_pointer",
                "void (int (**)(int))",
                &["int (**)(int)"]
            ),
        ]),
        [
            "extern type Handle",
            "extern \"apply\" def apply :: (i32 -> i32 -> i32) -> i32 -> i32",
            "extern \"apply_typedef\" def apply_typedef :: (i32 -> i32 -> i32) -> i32",
            "extern \"run\" def run :: (unit -> unit) -> unit",
            "extern \"test\" def test_ :: (i8u -> i16 -> bool) -> i8u -> i8u",
            "extern \"visit\" def visit :: (ref Handle -> i64) -> ref Handle -> i64",
            "extern \"nullable\" def nullable :: (i32 -> i32) -> i32",
            "// skipped context: parameter 1 has unsupported type 'void (*)(void *)'",
            "// skipped variadic: parameter 1 has unsupported type 'int (*)(int, ...)'",
            "// skipped unprototyped: parameter 1 has unsupported type 'int (*)()'",
            "// skipped pointer_result: parameter 1 has unsupported type 'int *(*)(void)'",
            "// skipped nested: parameter 1 has unsupported type 'int (*)(int (*)(int))'",
            "// skipped block: parameter 1 has unsupported type 'int (^)(int)'",
            "// skipped plain_char: parameter 1 has unsupported type 'char (*)(char)'",
            "// skipped convention: parameter 1 has unsupported type 'void (*)(int) __attribute__((ms_abi))'",
            "// skipped pointer_to_pointer: parameter 1 has unsupported type 'int (**)(int)'",
        ]
    );
}

#[test]
fn pairs_annotated_buffers() {
    let generated = annotated(
        vec![
            typedef("int64_t", "long long"),
            typedef("size_t", "unsigned long"),
            named_function(
                "sum",
                "long (const long *, unsigned long)",
                &[("values", "const long *"), ("count", "unsigned long")],
            ),
            named_function(
                "mean",
                "double (const double *, long)",
                &[("values", "const double *"), ("count", "long")],
            ),
            named_function(
                "bytes",
                "unsigned long (const unsigned char *, unsigned long)",
                &[
                    ("data", "const unsigned char *"),
                    ("length", "unsigned long"),
                ],
            ),
            named_function(
                "chars",
                "int (const char *, unsigned long)",
                &[("text", "const char *"), ("length", "unsigned long")],
            ),
            named_function(
                "voids",
                "int (const void *, unsigned long)",
                &[("data", "const void *"), ("size", "unsigned long")],
            ),
            named_function(
                "typed",
                "int (const int64_t *, size_t)",
                &[("values", "const int64_t *"), ("count", "size_t")],
            ),
            named_function(
                "two",
                "int (int, const double *, long, const double *, long)",
                &[
                    ("flags", "int"),
                    ("a", "const double *"),
                    ("an", "long"),
                    ("b", "const double *"),
                    ("bn", "long"),
                ],
            ),
            named_function(
                "writable",
                "int (long *, unsigned long)",
                &[("values", "long *"), ("count", "unsigned long")],
            ),
            named_function(
                "apart",
                "int (const long *, int, unsigned long)",
                &[
                    ("values", "const long *"),
                    ("flags", "int"),
                    ("count", "unsigned long"),
                ],
            ),
            named_function(
                "narrow",
                "int (const long *, unsigned int)",
                &[("values", "const long *"), ("count", "unsigned int")],
            ),
            named_function(
                "ints",
                "int (const int *, unsigned long)",
                &[("values", "const int *"), ("count", "unsigned long")],
            ),
            named_function(
                "signed_bytes",
                "int (const signed char *, unsigned long)",
                &[
                    ("values", "const signed char *"),
                    ("count", "unsigned long"),
                ],
            ),
            named_function(
                "scalar",
                "int (long, unsigned long)",
                &[("value", "long"), ("count", "unsigned long")],
            ),
            named_function(
                "reversed",
                "int (unsigned long, const long *)",
                &[("count", "unsigned long"), ("values", "const long *")],
            ),
        ],
        &[
            buffer("sum", "1", "2"),
            buffer("mean", "values", "count"),
            buffer("bytes", "data", "length"),
            buffer("chars", "text", "length"),
            buffer("voids", "data", "size"),
            buffer("typed", "values", "count"),
            buffer("two", "a", "an"),
            buffer("two", "b", "bn"),
            buffer("writable", "values", "count"),
            buffer("apart", "values", "count"),
            buffer("narrow", "values", "count"),
            buffer("ints", "values", "count"),
            buffer("signed_bytes", "values", "count"),
            buffer("scalar", "value", "count"),
            buffer("reversed", "values", "count"),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        body(&generated),
        [
            "type Int64T = i64",
            "type SizeT = i64u",
            "extern \"sum\" def sum :: ref [i64] -> i64",
            "extern \"mean\" def mean :: ref [f64] -> f64",
            "extern \"bytes\" def bytes :: ref [ubyte] -> i64u",
            "extern \"chars\" def chars :: ref [ubyte] -> i32",
            "extern \"voids\" def voids :: ref [ubyte] -> i32",
            "extern \"typed\" def typed :: ref [i64] -> i32",
            "extern \"two\" def two :: i32 -> ref [f64] -> ref [f64] -> i32",
            "// skipped writable: buffer parameter 1 has type 'long *', not a pointer to const; the host could write into a shared Tsuzuri array",
            "// skipped apart: --buffer pairs parameter 1 with parameter 3, but the buffer ABI passes the length right after the pointer",
            "// skipped narrow: buffer length parameter 2 has type 'unsigned int', not a 64-bit integer",
            "// skipped ints: buffer parameter 1 has type 'const int *'; Tsuzuri buffers hold 64-bit integers, doubles, or bytes",
            "// skipped signed_bytes: buffer parameter 1 has type 'const signed char *'; Tsuzuri buffers hold 64-bit integers, doubles, or bytes",
            "// skipped scalar: --buffer names parameter 1, which has type 'long', not a pointer",
            "// skipped reversed: --buffer pairs parameter 2 with parameter 1, but the buffer ABI passes the length right after the pointer",
        ]
    );
    assert!(
        generated
            .text
            .contains("\n// options: --buffer sum:1:2 --buffer mean:values:count ")
    );
}

#[test]
fn rejects_annotations_the_header_cannot_satisfy() {
    let nodes = || {
        vec![named_function(
            "sum",
            "long (const long *, unsigned long)",
            &[("values", "const long *"), ("count", "unsigned long")],
        )]
    };
    let failure = |buffers: &[BufferAnnotation], consumes: &[ConsumeAnnotation]| match annotated(
        nodes(),
        buffers,
        consumes,
    ) {
        Err(Failure::Annotation(message)) => message,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        failure(&[buffer("total", "1", "2")], &[]),
        "--buffer names 'total', which the header does not declare as a function"
    );
    assert_eq!(
        failure(&[], &[consume("close", "1")]),
        "--consume names 'close', which the header does not declare as a function"
    );
    assert_eq!(
        failure(&[buffer("sum", "values", "size")], &[]),
        "--buffer names parameter 'size', which 'sum' does not have"
    );
    assert_eq!(
        failure(&[buffer("sum", "1", "3")], &[]),
        "--buffer names parameter 3 of 'sum', which has 2 parameters"
    );
    assert_eq!(
        failure(&[buffer("sum", "1", "1")], &[]),
        "--buffer names a parameter of 'sum' that another annotation already names"
    );
    assert_eq!(
        failure(&[buffer("sum", "1", "2")], &[consume("sum", "values")]),
        "--buffer names a parameter of 'sum' that another annotation already names"
    );
    assert_eq!(
        failure(
            &[buffer("sum", "1", "2"), buffer("sum", "count", "values")],
            &[]
        ),
        "--buffer names a parameter of 'sum' that another annotation already names"
    );
}

#[test]
fn generated_modules_check_and_consume_moves_the_handle() {
    let generated = annotated(
        vec![
            json!({ "kind": "RecordDecl", "name": "counter", "tagUsed": "struct" }),
            named_function(
                "counter_new",
                "struct counter *(long)",
                &[("start", "long")],
            ),
            named_function(
                "counter_add",
                "long (struct counter *, long)",
                &[("counter", "struct counter *"), ("amount", "long")],
            ),
            named_function(
                "counter_free",
                "long (struct counter *)",
                &[("counter", "struct counter *")],
            ),
            typedef("map_fn", "long (*)(long)"),
            named_function(
                "apply",
                "long (map_fn, long)",
                &[("map", "map_fn"), ("value", "long")],
            ),
            named_function(
                "sum",
                "long (const long *, unsigned long)",
                &[("values", "const long *"), ("count", "unsigned long")],
            ),
            enumeration("mode", vec![constant("MODE_ONE", Some(value("1", "int")))]),
            record("pair", &[("left", "int"), ("right", "double")]),
            function(
                "pair_sum",
                "double (const struct pair *)",
                &["const struct pair *"],
            ),
        ],
        &[buffer("sum", "values", "count")],
        &[consume("counter_free", "counter")],
    )
    .unwrap();
    let main = |tail: &str| {
        format!(
            "private def twice :: i64 -> i64\nfn twice value = value * 2\n\nexport def run :: i64\nfn run =\n    let counter = Lib.counter_new 1\n    let added = Lib.counter_add (&counter) 2\n    let values = [1, 2]\n    let pair = Pair {{ left: 1, right: 2.5 }}\n    let total = added + Lib.apply twice 3 + Lib.sum (&values) + (Lib.pair_sum (&pair) as i64) + (Lib.MODE_ONE as i64)\n{tail}"
        )
    };
    let valid = main("    total + Lib.counter_free counter\n");
    tsuzuri::analyze_modules(&[("Main.tz", &valid), ("Lib.tz", &generated.text)])
        .unwrap_or_else(|error| panic!("{}: {}\n{}", error.code, error.message, generated.text));
    // Freeing moves the handle, so using it afterwards is rejected.
    let after_free = main(
        "    let freed = Lib.counter_free counter\n    total + freed + Lib.counter_add (&counter) 1\n",
    );
    let error = tsuzuri::analyze_modules(&[("Main.tz", &after_free), ("Lib.tz", &generated.text)])
        .expect_err("use after free");
    assert_eq!(error.code, "E1012", "{}", error.message);
}

// ---------------------------------------------------------------------------
// Review fixes

fn attributed(mut node: Value, attributes: &[&str]) -> Value {
    let inner = node["inner"].as_array().cloned().unwrap_or_default();
    node["inner"] = json!(
        attributes
            .iter()
            .map(|attribute| json!({ "kind": attribute }))
            .chain(inner)
            .collect::<Vec<_>>()
    );
    node
}

#[test]
fn converts_only_types_whose_attributes_keep_the_layout() {
    let enumerated = |tag: &str, attributes: &[&str]| {
        attributed(
            enumeration(
                tag,
                vec![constant(&format!("{}_ZERO", tag.to_uppercase()), None)],
            ),
            attributes,
        )
    };
    assert_eq!(
        lines(vec![
            // `aligned` and `mode` change an enum's size or alignment: its constants stay,
            // but parameters, results, and fields of its type are skipped.
            enumerated("aligned", &["AlignedAttr"]),
            enumerated("byte", &["ModeAttr"]),
            enumerated("tight", &["PackedAttr"]),
            enumerated("unknown", &["SomeFutureAttr"]),
            enumerated(
                "open",
                &["EnumExtensibilityAttr", "FlagEnumAttr", "AvailabilityAttr"]
            ),
            function("take_aligned", "int (enum aligned)", &["enum aligned"]),
            function("give_byte", "enum byte (void)", &[]),
            function("take_tight", "int (enum tight)", &["enum tight"]),
            function("take_unknown", "int (enum unknown)", &["enum unknown"]),
            function("take_open", "enum open (enum open)", &["enum open"]),
            record("holder", &[("value", "enum aligned")]),
            // Typedefs, structs, and fields follow the same allow-list.
            attributed(typedef("u64m", "unsigned long"), &["ModeAttr"]),
            attributed(
                typedef("float_t", "float"),
                &["AvailableOnlyInDefaultEvalMethodAttr"]
            ),
            function("take_mode", "void (u64m)", &["u64m"]),
            attributed(
                record("shuffled", &[("a", "int")]),
                &["RandomizeLayoutAttr"]
            ),
            attributed(
                record("bridged", &[("a", "int")]),
                &["ObjCBridgeAttr", "SwiftPrivateAttr"]
            ),
            json!({ "kind": "RecordDecl", "name": "laid", "tagUsed": "struct", "completeDefinition": true,
                    "inner": [{ "kind": "FieldDecl", "name": "a", "type": { "qualType": "int" }, "inner": [{ "kind": "ModeAttr" }] }] }),
            attributed(function("jump", "int (void)", &[]), &["ReturnsTwiceAttr"]),
        ]),
        [
            "const ALIGNED_ZERO: i32 = 0",
            "const BYTE_ZERO: i32 = 0",
            "const TIGHT_ZERO: i32 = 0",
            "const UNKNOWN_ZERO: i32 = 0",
            "const OPEN_ZERO: i32 = 0",
            "// skipped take_aligned: parameter 1 has unsupported type 'enum aligned'",
            "// skipped give_byte: result has unsupported type 'enum byte'",
            "// skipped take_tight: parameter 1 has unsupported type 'enum tight'",
            "// skipped take_unknown: parameter 1 has unsupported type 'enum unknown'",
            "extern \"take_open\" def take_open :: i32 -> i32",
            "// skipped holder: field 'value' has unsupported type 'enum aligned'",
            "type FloatT = f32",
            "// skipped take_mode: parameter 1 has unsupported type 'u64m'",
            "// skipped shuffled: struct has the RandomizeLayout attribute, which may change its layout",
            "record Bridged { a: i32 }",
            "// skipped laid: field 'a' has __attribute__((mode)), which may change its layout",
            "// skipped jump: function returns twice (returns_twice), which a Tsuzuri call cannot follow",
        ]
    );
}

#[test]
fn does_not_read_unnamed_struct_typedefs_as_tags() {
    // `typedef struct { int small; } S;` is spelled `struct S`, but the tag `struct S` is
    // another type: C keeps tags and typedef names in separate namespaces.
    let unnamed = |id: &str, kind: &str, tag_used: &str| {
        json!({ "kind": kind, "id": id, "tagUsed": tag_used, "completeDefinition": true,
                "inner": [{ "kind": "FieldDecl", "name": "small", "type": { "qualType": "int" } }] })
    };
    let named_by = |name: &str, spelled: &str, id: &str, kind: &str| {
        json!({ "kind": "TypedefDecl", "name": name, "type": { "qualType": spelled },
                "inner": [{ "kind": "ElaboratedType", "ownedTagDecl": { "id": id, "kind": kind, "name": "" } }] })
    };
    assert_eq!(
        lines(vec![
            record("S", &[("big", "double"), ("other", "double")]),
            unnamed("0x1", "RecordDecl", "struct"),
            named_by("S", "struct S", "0x1", "RecordDecl"),
            function("use_typedef", "double (const S *)", &["const S *"]),
            function(
                "use_tag",
                "double (const struct S *)",
                &["const struct S *"]
            ),
            typedef("S_alias", "S"),
            function(
                "use_alias",
                "double (const S_alias *)",
                &["const S_alias *"]
            ),
            json!({ "kind": "RecordDecl", "name": "H", "tagUsed": "struct" }),
            unnamed("0x2", "RecordDecl", "struct"),
            named_by("H", "struct H", "0x2", "RecordDecl"),
            function("use_handle_typedef", "void (H *)", &["H *"]),
            function("use_handle", "void (struct H *)", &["struct H *"]),
            unnamed("0x3", "RecordDecl", "union"),
            named_by("U", "union U", "0x3", "RecordDecl"),
            // An anonymous enum named like an enum tag (here only declared) is ambiguous.
            json!({ "kind": "EnumDecl", "name": "E" }),
            json!({ "kind": "EnumDecl", "id": "0x4", "inner": [{ "kind": "EnumConstantDecl", "name": "E_ZERO" }] }),
            named_by("E", "enum E", "0x4", "EnumDecl"),
            function("use_enum", "int (E)", &["E"]),
            json!({ "kind": "EnumDecl", "id": "0x5", "inner": [{ "kind": "EnumConstantDecl", "name": "F_ZERO" }] }),
            named_by("F", "enum F", "0x5", "EnumDecl"),
            function("use_anonymous_enum", "int (F)", &["F"]),
            // A tag declared inside a struct has file scope, so it is ambiguous too.
            json!({ "kind": "RecordDecl", "name": "outer", "tagUsed": "struct", "completeDefinition": true,
                    "inner": [
                        { "kind": "EnumDecl", "name": "G", "inner": [{ "kind": "EnumConstantDecl", "name": "G_WIDE",
                            "inner": [{ "kind": "ConstantExpr", "value": "140737488355327" }] }] },
                        { "kind": "FieldDecl", "name": "g", "type": { "qualType": "enum G" } },
                    ] }),
            json!({ "kind": "EnumDecl", "id": "0x6", "inner": [{ "kind": "EnumConstantDecl", "name": "G_ZERO" }] }),
            named_by("G", "enum G", "0x6", "EnumDecl"),
            function("use_nested_tag", "int (enum G)", &["enum G"]),
            function("use_nested_typedef", "int (G)", &["G"]),
        ]),
        [
            "record S { big: f64, other: f64 }",
            "// skipped S: anonymous struct; give the struct a tag",
            "// skipped use_typedef: parameter 1 has unsupported type 'const S *'",
            "extern \"use_tag\" def use_tag :: ref S -> f64",
            "// skipped use_alias: parameter 1 has unsupported type 'const S_alias *'",
            "extern type H",
            "// skipped H: anonymous struct; give the struct a tag",
            "// skipped use_handle_typedef: parameter 1 has unsupported type 'H *'",
            "extern \"use_handle\" def use_handle :: ref H -> unit",
            "// skipped U: C union",
            "const E_ZERO: i32 = 0",
            "// skipped use_enum: parameter 1 has unsupported type 'E'",
            "const F_ZERO: i32 = 0",
            "extern \"use_anonymous_enum\" def use_anonymous_enum :: i32 -> i32",
            "// skipped outer: field 'g' has unsupported type 'enum G'",
            "const G_ZERO: i32 = 0",
            "// skipped use_nested_tag: parameter 1 has unsupported type 'enum G'",
            "// skipped use_nested_typedef: parameter 1 has unsupported type 'G'",
        ]
    );
}

#[test]
fn never_reads_desugared_types() {
    // Desugaring `typeof (wide_value)` drops the `aligned(16)` of the typedef behind it,
    // so the plain `long long` would put the field at the wrong offset.
    let typed = |mut node: Value, spelled: &str, desugared: &str| {
        node["type"] = json!({ "qualType": spelled, "desugaredQualType": desugared });
        node
    };
    let mut with_typeof = record("with_typeof", &[("a", "int"), ("b", "")]);
    with_typeof["inner"][1]["type"] =
        json!({ "qualType": "typeof (wide_value)", "desugaredQualType": "long long" });
    assert_eq!(
        lines(vec![
            attributed(typedef("wide_aligned", "long long"), &["AlignedAttr"]),
            with_typeof,
            typed(
                typedef("via_typeof", ""),
                "typeof (wide_value)",
                "long long"
            ),
            record("with_typedef", &[("a", "int"), ("b", "via_typeof")]),
            json!({ "kind": "FunctionDecl", "name": "take", "type": { "qualType": "int (typeof (x))" },
                    "inner": [{ "kind": "ParmVarDecl", "type": { "qualType": "typeof (x)", "desugaredQualType": "int" } }] }),
            typed(
                typedef("my_size_t", ""),
                "typeof (sizeof (0))",
                "unsigned long"
            ),
            function("size_of", "my_size_t (my_size_t)", &["my_size_t"]),
        ]),
        [
            "// skipped with_typeof: field 'b' has unsupported type 'typeof (wide_value)'",
            "// skipped with_typedef: field 'b' has unsupported type 'via_typeof'",
            "// skipped take: parameter 1 has unsupported type 'typeof (x)'",
            "// skipped size_of: parameter 1 has unsupported type 'my_size_t'",
        ]
    );
}

#[test]
fn generates_a_macro_only_where_its_final_definition_agrees() {
    // `#pragma push_macro`/`pop_macro` leave no trace in `clang -E -dD`: LEVEL reads as 2
    // there, but C ends with 1, which `clang -E -dM` shows.
    let header_lines = [
        "#define LEVEL 1",
        "#pragma push_macro(\"LEVEL\")",
        "#undef LEVEL",
        "#define LEVEL 2",
        "#pragma pop_macro(\"LEVEL\")",
        "#define KEPT 3",
        "#define GONE 4",
        "#define CALLED 5",
        "#define TEXT \"a\"",
        "#define NOW_NUMBER x",
    ];
    let (text, output) = preprocessed(&header_lines);
    let output = output
        .replace("#pragma push_macro(\"LEVEL\")", "")
        .replace("#pragma pop_macro(\"LEVEL\")", "");
    let finals = "#define __STDC__ 1\n#define LEVEL 1\n#define KEPT 3\n#define CALLED(x) (x)\n#define TEXT \"b\"\n#define NOW_NUMBER 6\n";
    let generated = generate_with(
        &unit(Vec::new()),
        &header(),
        &Extras {
            preprocessed: Some(&output),
            final_macros: Some(finals),
            header_text: text.as_bytes(),
            ..Extras::default()
        },
    )
    .unwrap();
    let reason = "the macro does not end with this #define (#pragma push_macro and pop_macro can restore another value)";
    assert_eq!(
        body(&generated),
        [
            format!("// skipped LEVEL: {reason}").as_str(),
            "const KEPT: i32 = 3",
            format!("// skipped GONE: {reason}").as_str(),
            format!("// skipped CALLED: {reason}").as_str(),
            format!("// skipped NOW_NUMBER: {reason}").as_str(),
        ]
    );
    // The warning points at the last `#define` of LEVEL that the `-dD` output shows.
    let level = &generated.skipped[0];
    assert_eq!(level.offset, text.find("LEVEL 2").unwrap());
}
