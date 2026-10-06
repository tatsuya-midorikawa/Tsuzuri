use tsuzuri::{analyze, diagnostic::Diagnostic, llvm};

/// Checks `source`, emits it twice to check determinism, and returns the native IR.
fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

fn diagnostic(source: &str) -> Diagnostic {
    analyze(source).expect_err(source)
}

fn rejects(source: &str, code: &str) -> Diagnostic {
    let error = diagnostic(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error
}

/// Rejects `source` with `code` and a message that contains `text`.
fn rejects_containing(source: &str, code: &str, text: &str) {
    let error = rejects(source, code);
    assert!(error.message.contains(text), "{source}\n{}", error.message);
}

const SHAPE: &str = "class Shape<'a> {\n    def area :: ref 'a -> i64\n}\nrecord Square { side: i64 }\ninstance Shape<Square> {\n    fn area s = s.side * s.side\n}\n";

fn with_shape(rest: &str) -> String {
    format!("{SHAPE}{rest}")
}

#[test]
fn dyn_is_reserved_keyword() {
    let keyword = diagnostic("let match = 1\n()");
    let error = diagnostic("let dyn = 1\n()");
    assert_eq!((error.code, keyword.code), ("E0002", "E0002"));
}

#[test]
fn dyn_types_parse_and_display() {
    let program = tsuzuri::parser::parse(
        "def f :: ref dyn Shape -> [dyn Shape] -> Maybe<dyn (Shape, Send)> -> i64 = \\a b c -> 0",
    )
    .unwrap();
    assert_eq!(program.dyn_types.len(), 3);
    rejects_containing(
        &with_shape("let x: dyn Shape = 42\n()"),
        "E1003",
        "expected dyn Main.Shape, found",
    );
    rejects_containing(
        &with_shape("def f :: dyn Shape<i64> -> i64 = \\s -> 0"),
        "E0002",
        "dyn takes a class name without type arguments; write dyn Shapes.Shape",
    );
    rejects_containing(
        "let x: dyn = 1\n()",
        "E0002",
        "expected a type class name after dyn",
    );
    // The classes are sorted, so their order does not matter.
    accepts(&with_shape(
        "class Tagged<'a> { def tag :: ref 'a -> i64 }\ninstance Tagged<Square> { fn tag s = s.side }\ndef f :: dyn (Tagged, Shape) -> i64\nfn f shape = Shape.area (ref shape)\ndef g :: dyn (Shape, Tagged) -> i64\nfn g shape = f shape",
    ));
}

#[test]
fn dyn_names_must_be_classes() {
    rejects_containing(
        &with_shape("def f :: dyn Nope -> i64 = \\s -> 0"),
        "E1004",
        "unknown type class 'Nope'; dyn needs a type class such as dyn Shapes.Shape",
    );
    rejects_containing(
        &with_shape("def f :: dyn Square -> i64 = \\s -> 0"),
        "E1004",
        "'Square' is not a type class; dyn needs a type class such as dyn Shapes.Shape",
    );
}

#[test]
fn programs_without_dyn_emit_no_dyn_ir() {
    let ir = accepts(&with_shape(
        "def area_of :: Shape<'a> => ref 'a -> i64 = \\shape -> Shape.area shape\nlet square = Square { side: 3 }\narea_of (ref square)",
    ));
    assert!(!ir.contains("tz.dyn") && !ir.contains("tz.vtable"), "{ir}");
}

#[test]
fn dyn_in_exports_and_externs_is_rejected() {
    rejects(
        &with_shape("export def f :: ref (dyn Shape) -> i64 = \\s -> 0"),
        "E1008",
    );
    assert!(analyze(&with_shape("extern def take :: dyn Shape -> i64\n()")).is_err());
}

#[test]
fn rejects_dyn_incompatible_classes() {
    for (declarations, ty, reason) in [
        (
            "class Same<'a> { def same :: ref 'a -> ref 'a -> bool }",
            "dyn Same",
            "method Main.Same.same uses the class type outside its first parameter",
        ),
        (
            "class Make<'a> { def make :: i64 -> 'a }",
            "dyn Make",
            "method Main.Make.make does not take the class type by value, ref, or ref mut as its first parameter",
        ),
        (
            "class Late<'a> { def late :: i64 -> ref 'a -> i64 }",
            "dyn Late",
            "does not take the class type",
        ),
        (
            "class Nested<'a> { def nested :: ref ['a] -> i64 }",
            "dyn Nested",
            "does not take the class type",
        ),
        ("", "dyn Copy", "Copy has no methods to dispatch"),
        ("", "dyn Integer", "Integer has no methods to dispatch"),
        (
            "",
            "dyn Eq",
            "method Eq.eq uses the class type outside its first parameter",
        ),
        (
            "",
            "dyn Ord",
            "uses the class type outside its first parameter",
        ),
        (
            "",
            "dyn Add",
            "uses the class type outside its first parameter",
        ),
        (
            "class Eq<'a> => Keyed<'a> { def key :: ref 'a -> i64 }",
            "dyn Keyed",
            "method Eq.eq uses the class type outside its first parameter",
        ),
        (
            "class Copy<'a> => Plain<'a> { def plain :: ref 'a -> i64 }",
            "dyn Plain",
            "superclass Copy needs the Copy marker",
        ),
        (
            "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }",
            "dyn Functor",
            "Main.Functor is higher-kinded",
        ),
        (
            "",
            "dyn Drop",
            "Drop runs by itself when a value is dropped",
        ),
    ] {
        let source = format!("{declarations}\ndef f :: {ty} -> i64 = \\s -> 0");
        let error = rejects(&source, "E1028");
        assert!(
            error
                .message
                .starts_with(&format!("{} is not allowed: ", ty.replace("Main.", "")))
                || error.message.contains(" is not allowed: "),
            "{source}\n{}",
            error.message
        );
        assert!(
            error.message.contains(reason),
            "{source}\n{}",
            error.message
        );
        assert!(
            error.message.contains("use a generic function with"),
            "{}",
            error.message
        );
    }
    // With the marker, a superclass `Copy` is satisfied by the vtable's clone slot (Phase 2).
    accepts(
        "class Copy<'a> => Plain<'a> { def plain :: ref 'a -> i64 }\nrecord Point { x: i64 }\ninstance Plain<Point> { fn plain p = p.x }\ndef f :: i64\nfn f =\n    let point: dyn (Plain, Copy) = Dyn.of (Point { x: 4 })\n    Plain.plain (ref point)",
    );
    rejects_containing(
        &with_shape(
            "def f {r} :: ref {r} i64 -> dyn (Shape, Send) {r}\nfn f side = Dyn.of (Square { side: deref side })",
        ),
        "E1028",
        "a dyn value with a region holds borrows, so it cannot be Send",
    );
}

const NAMED: &str = "class Named<'a> { def id :: ref 'a -> i64 }\nclass Named<'a> => Shape<'a> {\n    def area :: ref 'a -> i64\n    def describe :: ref 'a -> i64 = \\shape -> Shape.area shape * 1000 + Named.id shape\n}\nrecord Square { side: i64 }\ninstance Named<Square> { fn id _s = 1 }\ninstance Shape<Square> { fn area s = s.side * s.side }\n";

#[test]
fn superclass_and_default_methods_are_dispatched() {
    let ir = accepts(&format!(
        "{NAMED}def total :: ref (dyn Shape) -> i64 = \\s -> Shape.describe s + Named.id s\ndef make :: i64 -> dyn Shape = \\side -> Dyn.of (Square {{ side: side }})"
    ));
    // Slot 0 is Named.id, slot 1 Shape.area, and slot 2 the default method describe.
    for slot in [0, 2] {
        assert!(
            ir.contains(&format!(
                "getelementptr inbounds {{ ptr, ptr, i64, i64, [3 x ptr] }}, ptr %v2, i32 0, i32 4, i64 {slot}"
            )),
            "{ir}"
        );
    }
    assert!(
        ir.contains("@\"tz.vtable.Main.Shape[Main.Square]\""),
        "{ir}"
    );
}

#[test]
fn user_instances_for_dyn_heads() {
    accepts(&format!(
        "{NAMED}class Tagged<'a> {{ def tag :: ref 'a -> i64 }}\ninstance Tagged<dyn Shape> {{ fn tag shape = Shape.area shape + 7 }}\ndef f :: dyn Shape -> i64\nfn f shape =\n    let tagged: dyn Tagged = Dyn.of shape\n    Tagged.tag (ref tagged)"
    ));
    rejects(
        &format!("{NAMED}instance Shape<dyn Shape> {{ fn area _s = 0 }}"),
        "E1016",
    );
}

#[test]
fn generic_functions_instantiate_once_for_dyn() {
    let mut source = String::from(
        "class Shape<'a> { def area :: ref 'a -> i64 }\ndef area_of :: Shape<'a> => ref 'a -> i64 = \\s -> Shape.area s\n",
    );
    for index in 0..5 {
        source.push_str(&format!(
            "record R{index} {{ v: i64 }}\ninstance Shape<R{index}> {{ fn area r = r.v + {index} }}\n"
        ));
    }
    let mut dynamic = source.clone();
    let mut direct = source;
    dynamic.push_str("def f :: i64\nfn f =\n    let mut total = 0i64\n");
    direct.push_str("def f :: i64\nfn f =\n    let mut total = 0i64\n");
    for index in 0..5 {
        dynamic.push_str(&format!(
            "    let s{index}: dyn Shape = Dyn.of (R{index} {{ v: {index} }})\n    total = total + area_of (ref s{index})\n"
        ));
        direct.push_str(&format!(
            "    let s{index} = R{index} {{ v: {index} }}\n    total = total + area_of (ref s{index})\n"
        ));
    }
    dynamic.push_str("    total");
    direct.push_str("    total");
    let count = |ir: &str| {
        ir.matches("define internal i64 @tz.fn.Main.area_of")
            .count()
    };
    assert_eq!(count(&accepts(&dynamic)), 1);
    assert_eq!(count(&accepts(&direct)), 5);
}

#[test]
fn dyn_of_needs_expected_dyn_type() {
    for source in [
        "let s = Dyn.of (Square { side: 1 })\n()",
        "let f = Dyn.of\n()",
        "let s: dyn Shape = Square { side: 1 } |> Dyn.of\n()",
    ] {
        let error = rejects(&with_shape(source), "E1015");
        assert!(error.message.starts_with("Dyn.of "), "{}", error.message);
    }
    rejects_containing(
        &with_shape("let s = Dyn.of (Square { side: 1 })\n()"),
        "E1015",
        "Dyn.of needs an expected dyn type here; annotate the binding, parameter, or field, for example let shape: dyn Shapes.Shape = Dyn.of value",
    );
    rejects_containing(
        &with_shape("let f = Dyn.of\n()"),
        "E1015",
        "Dyn.of must be applied to exactly one argument where a dyn type is expected",
    );
}

#[test]
fn dyn_of_needs_an_instance() {
    let direct = diagnostic(&with_shape("let n = 42\nShape.area (ref n)"));
    let stored = diagnostic(&with_shape("let s: dyn Shape = Dyn.of 42\n()"));
    assert_eq!(stored.code, direct.code, "{}", stored.message);
    // A Copy dyn type needs a Copy value.
    rejects(
        &with_shape(
            "record Label { text: string }\ninstance Shape<Label> { fn area l = l.text.length }\nlet s: dyn (Shape, Copy) = Dyn.of (Label { text: \"a\" })\n()",
        ),
        "E1005",
    );
}

#[test]
fn dyn_rejects_borrowed_data() {
    let source = with_shape(
        "record View { value: ref i64 }\ninstance Shape<View> { fn area v = deref v.value }\ndef f :: i64\nfn f =\n    let number = 4\n    let shape: dyn Shape = Dyn.of (View { value: ref number })\n    Shape.area (ref shape)",
    );
    let error = diagnostic(&source);
    assert_eq!(error.code, "E1013", "{}", error.message);
    assert!(
        error.message.contains("cannot hold borrowed data"),
        "{}",
        error.message
    );
    // Generic code is checked where specialization makes the stored type concrete, upcasts too.
    let view =
        "record View { value: ref i64 }\ninstance Shape<View> { fn area v = deref v.value }\n";
    rejects_containing(
        &with_shape(&format!(
            "{view}def wrap :: Shape<'a> => 'a -> dyn Shape\nfn wrap x = Dyn.of x\ndef f :: i64\nfn f =\n    let number = 4\n    let shape = wrap (View {{ value: ref number }})\n    Shape.area (ref shape)"
        )),
        "E1013",
        "a dyn value without a region cannot hold borrowed data, and Main.View holds borrows",
    );
    rejects_containing(
        &with_shape(&format!(
            "{view}def lift {{r}} :: ref {{r}} i64 -> dyn Shape {{r}}\nfn lift value = Dyn.of (View {{ value: value }})\ndef wrap :: Shape<'a> => 'a -> dyn Shape\nfn wrap x = Dyn.of x\ndef f :: i64\nfn f =\n    let number = 4\n    let shape = wrap (lift (ref number))\n    Shape.area (ref shape)"
        )),
        "E1013",
        "cannot hold borrowed data",
    );
}

#[test]
fn borrowed_dyn_values_keep_their_loans() {
    let view = "record View {r} { value: ref {r} i64 }\ninstance Shape<View> { fn area v = deref v.value }\ndef view {r} :: ref {r} i64 -> dyn Shape {r}\nfn view value = Dyn.of (View { value: value })\n";
    accepts(&with_shape(&format!(
        "{view}def f :: i64\nfn f =\n    let number = 4\n    let shape = view (ref number)\n    Shape.area (ref shape)"
    )));
    // The dyn value borrows `number`, which cannot change while it lives.
    rejects(
        &with_shape(&format!(
            "{view}def f :: i64\nfn f =\n    let mut number = 4\n    let shape = view (ref number)\n    number = 5\n    Shape.area (ref shape)"
        )),
        "E1014",
    );
    // Named regions stay out of local annotations.
    rejects(
        &with_shape(&format!(
            "{view}def f :: i64\nfn f =\n    let number = 4\n    let shape: dyn Shape {{r}} = view (ref number)\n    0"
        )),
        "E1013",
    );
    // Exclusive borrows never go into a dyn value.
    rejects(
        &with_shape(
            "record Counter {r} { value: ref mut {r} i64 }\ninstance Shape<Counter> { fn area c = deref c.value }\ndef wrap {r} :: ref mut {r} i64 -> dyn Shape {r}\nfn wrap value = Dyn.of (Counter { value: value })",
        ),
        "E1013",
    );
}

#[test]
fn dyn_values_are_owned() {
    let label =
        "record Label { text: string }\ninstance Shape<Label> { fn area l = l.text.length }\n";
    rejects(
        &with_shape(&format!(
            "{label}def f :: i64\nfn f =\n    let label = Label {{ text: \"abc\" }}\n    let boxed: dyn Shape = Dyn.of label\n    label.text.length"
        )),
        "E1012",
    );
    rejects(
        &with_shape(
            "def take :: dyn Shape -> i64\nfn take s = Shape.area (ref s)\ndef f :: i64\nfn f =\n    let boxed: dyn Shape = Dyn.of (Square { side: 3 })\n    take boxed + take boxed",
        ),
        "E1012",
    );
    // Only a Copy dyn type is Copy.
    let string = diagnostic(
        "def twice :: Copy<'a> => 'a -> i64\nfn twice _x = 0\ndef f :: string -> i64\nfn f x = twice x",
    );
    let stored = diagnostic(&with_shape(
        "def twice :: Copy<'a> => 'a -> i64\nfn twice _x = 0\ndef f :: dyn Shape -> i64\nfn f x = twice x",
    ));
    assert_eq!(stored.code, string.code, "{}", stored.message);
    accepts(&with_shape(
        "def twice :: Copy<'a> => 'a -> i64\nfn twice _x = 0\ndef f :: dyn (Shape, Copy) -> i64\nfn f x = twice x",
    ));
    // A reusable function value cannot capture a dyn value without a clone slot.
    let task = diagnostic(
        "def f :: Task<i64> -> i64\nfn f t =\n    let g = \\k -> k\n    let h = \\k -> { let u = t; k }\n    h 1",
    );
    let stored = diagnostic(&with_shape(
        "def f :: dyn Shape -> i64\nfn f s =\n    let h = \\k -> Shape.area (ref s) * k\n    h 1 + h 2",
    ));
    assert_eq!(stored.code, task.code, "{}", stored.message);
    accepts(&with_shape(
        "def f :: dyn (Shape, Copy) -> i64\nfn f s =\n    let h = \\k -> Shape.area (ref s) * k\n    h 1 + h 2",
    ));
    // A task takes only a Send dyn value.
    assert!(
        analyze(&with_shape(
            "def f :: dyn Shape -> i64\nfn f s = Task.run (task { Shape.area (ref s) })"
        ))
        .is_err()
    );
    accepts(&with_shape(
        "def f :: dyn (Shape, Send) -> i64\nfn f s = Task.run (task { Shape.area (ref s) })",
    ));
}

#[test]
fn builtin_instances_fill_vtables() {
    let ir = accepts(
        "def f :: i64 -> string\nfn f x =\n    let shown: dyn Display = Dyn.of x\n    to_string shown",
    );
    assert!(ir.contains("@\"tz.vtable.Display[i64]\""), "{ir}");
    accepts(
        "record Point { x: i64 } deriving (Display)\ndef f :: string\nfn f =\n    let shown: dyn Display = Dyn.of (Point { x: 1 })\n    to_string shown",
    );
}

#[test]
fn vtables_are_unique_and_deterministic() {
    let ir = accepts(&with_shape(
        "record Rect { w: i64, h: i64 }\ninstance Shape<Rect> { fn area r = r.w * r.h }\ndef a :: dyn Shape = Dyn.of (Square { side: 2 })\ndef b :: dyn Shape = Dyn.of (Rect { w: 2, h: 3 })\ndef c :: dyn Shape = Dyn.of (Square { side: 4 })",
    ));
    assert_eq!(ir.matches("\n@\"tz.vtable.").count(), 2, "{ir}");
    assert_eq!(
        ir.matches("define internal void @\"tz.dyn.drop[").count(),
        2,
        "{ir}"
    );
}

#[test]
fn accepts_dyn_values_in_collections() {
    accepts(&with_shape(
        "record Bag { items: [dyn Shape] }\ninstance Shape<Bag> { fn area b = b.items.length }\ndef f :: i64\nfn f =\n    let items: [dyn Shape] = [Dyn.of (Square { side: 1 }), Dyn.of (Square { side: 2 })]\n    let maybe: Maybe<dyn Shape> = Some (Dyn.of (Square { side: 3 }))\n    let bag: dyn Shape = Dyn.of (Bag { items: items })\n    let empty: Vec<dyn Shape> = Vec.empty()\n    Shape.area (ref bag) + empty.length + (match maybe with\n        | Some s -> Shape.area (ref s)\n        | None -> 0)",
    ));
}

#[test]
fn display_and_hash_dispatch_user_instances() {
    let ir = accepts(
        "record Point { x: i64, y: i64 } deriving (Display, Hash)\ndef f :: string\nfn f =\n    let shown: dyn Display = Dyn.of (Point { x: 1, y: 2 })\n    let hashed: dyn Hash = Dyn.of (Point { x: 1, y: 2 })\n    to_string shown + to_string (Hash.hash (ref hashed))",
    );
    assert!(ir.contains("@\"tz.vtable.Display[Main.Point]\""), "{ir}");
    assert!(ir.contains("@\"tz.vtable.Hash[Main.Point]\""), "{ir}");
}

#[test]
fn deriving_rejects_dyn_fields() {
    let function = diagnostic("record Holder { f: i64 -> i64 } deriving (Eq)");
    let stored = diagnostic(&with_shape(
        "record Holder { shape: dyn Shape } deriving (Eq)",
    ));
    assert_eq!(stored.code, function.code, "{}", stored.message);
}

#[test]
fn several_classes_and_upcasts() {
    let source = |body: &str| {
        format!(
            "{NAMED}class Tagged<'a> {{ def tag :: ref 'a -> i64 }}\ninstance Tagged<Square> {{ fn tag s = s.side + 100 }}\n{body}"
        )
    };
    let ir = accepts(&source(
        "def f :: i64 -> i64\nfn f x =\n    let both: dyn (Shape, Tagged) = Dyn.of (Square { side: x })\n    let shape: dyn Shape = Dyn.of both\n    let named: dyn Named = Dyn.of shape\n    Named.id (ref named)",
    ));
    // The vtables of (Shape, Tagged) and Shape hold upcast tables; Named's has none.
    assert!(ir.contains("@\"tz.vtable.Main.Shape,Main.Tagged[Main.Square]\" = internal unnamed_addr constant { ptr, ptr, i64, i64, [4 x ptr], [1 x ptr] }"), "{ir}");
    assert!(ir.contains("@\"tz.vtable.Main.Shape[Main.Square]\" = internal unnamed_addr constant { ptr, ptr, i64, i64, [3 x ptr], [1 x ptr] }"), "{ir}");
    assert!(ir.contains("@\"tz.vtable.Main.Named[Main.Square]\" = internal unnamed_addr constant { ptr, ptr, i64, i64, [1 x ptr] }"), "{ir}");
    // An upcast does not store the value again.
    let upcast = &ir[ir.find("define internal i64 @tz.fn.Main.f(").unwrap()..];
    let upcast = &upcast[..upcast.find("\n}").unwrap()];
    assert_eq!(
        upcast.matches("@tz.fn.$builtin.Dyn.of").count(),
        3,
        "{upcast}"
    );
    // A class that the value does not dispatch needs an instance.
    rejects(
        &source(
            "def f :: dyn Named -> i64\nfn f named =\n    let shape: dyn Shape = Dyn.of named\n    0",
        ),
        "E1005",
    );
}

#[test]
fn dyn_breaks_type_growing_recursion() {
    let base = "class Shape<'a> { def area :: ref 'a -> i64 }\nrecord Wrap<'a> { inner: 'a }\ninstance Shape<i64> { fn area n = deref n }\ninstance Shape<'a> => Shape<Wrap<'a>> { fn area w = Shape.area (ref w.inner) + 1 }\n";
    accepts(&format!(
        "{base}def rec deep :: dyn Shape -> i64 -> i64\nfn rec deep x n = if n == 0 then Shape.area (ref x) else deep (Dyn.of (Wrap {{ inner: x }})) (n - 1)\ndef f :: i64\nfn f = deep (Dyn.of 1i64) 3"
    ));
    rejects(
        &format!(
            "{base}def rec deep :: Shape<'a> => 'a -> i64 -> i64\nfn rec deep x n = if n == 0 then Shape.area (ref x) else deep (Wrap {{ inner: x }}) (n - 1)\ndef f :: i64\nfn f = deep 1i64 3"
        ),
        "E1017",
    );
}

#[test]
fn runtime_fixture_lowers_for_both_targets() {
    let module = tsuzuri::analyze_modules(&[
        ("Shapes.tt", include_str!("fixtures/dyn_dispatch/Shapes.tt")),
        ("Main.tz", include_str!("fixtures/dyn_dispatch/Main.tz")),
    ])
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("%tz.dyn = type { ptr, ptr }"), "{ir}");
    }
}
