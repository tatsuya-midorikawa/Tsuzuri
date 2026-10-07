//! `tsuzuri bindgen` (E11): conversion of Clang's JSON AST, without running Clang.
//! Each AST is the smallest shape Clang writes; the expected lines follow from the C
//! declarations by hand.

use serde_json::{Value, json};
use tsuzuri::bindgen::{
    Arguments, Generated, HeaderInfo, MARKER, Skipped, generate, lp64_target, parse_arguments,
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

/// The declaration lines after the four comment lines and the blank line.
fn body(generated: &Generated) -> Vec<&str> {
    generated.text.lines().skip(5).collect()
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
            "// skipped packed: struct has a packing or alignment attribute",
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
            json: true,
        })
    );
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
            "bindgen accepts only -o, --include-dir, and --json",
        ),
        (
            &["a.h", "-o", "A.tz", "-O3"][..],
            "bindgen accepts only -o, --include-dir, and --json",
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
