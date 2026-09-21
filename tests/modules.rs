use tsuzuri::check::{Type, TypedExprKind};
use tsuzuri::{analyze_modules, llvm};

const POINT: &str = "
record Point { x: f64, y: f64 }

fn distance(point: Point) -> f64 {
    sqrt(point.x * point.x + point.y * point.y)
}

export fn hypotenuse(x: f64, y: f64) -> f64 {
    distance(Point { x: x, y: y })
}
";

#[test]
fn accepts_the_point_example_with_top_level_bindings() {
    let module = analyze_modules(&[
        ("Point", POINT),
        (
            "Main",
            "let p = Point { x: 10.0, y: 20.5}\nlet d = Point.distance(p)",
        ),
    ])
    .unwrap();
    let entry = &module.functions[module.entry.unwrap()];
    assert_eq!(entry.signature.result, Type::Unit);
    assert_eq!(module.records[0].name, "Point.Point");
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call double @tz.fn.Point.distance"));
    assert!(ir.contains("call i8 @tz.fn.Main.$entry()"));
    assert!(ir.contains("define double @tz_hypotenuse"));
    assert!(!ir.contains("@tz_distance("));
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Console).unwrap());
}

#[test]
fn resolves_qualified_function_values_pipelines_and_lexical_shadowing() {
    let module = analyze_modules(&[
        (
            "Main",
            "record Callback { distance: fn(Point.Point) -> f64 }
             fn apply(f: fn(Point) -> f64, p: Point) -> f64 { p |> f }
             fn main() -> f64 {
                 let p: Point.Point = Point.Point { x: 3.0, y: 4.0 };
                 let f = Point.distance;
                 let Point = Callback { distance: f };
                 apply(Point.distance, p)
             }",
        ),
        ("Point", POINT),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("@tz.fn.Point.distance"));
    assert!(ir.contains("getelementptr inbounds %tz.record.Main.Callback"));
}

#[test]
fn keeps_same_named_functions_and_records_in_separate_modules() {
    let module = analyze_modules(&[
        (
            "Left",
            "record Value { x: i64 }
             fn value(v: Value) -> i64 { v.x }
             fn make() -> Value { Value { x: 20 } }",
        ),
        (
            "Right",
            "record Value { x: i64 }
             fn value(v: Value) -> i64 { v.x + 1 }
             fn make() -> Value { Value { x: 21 } }",
        ),
        (
            "Main",
            "fn main() -> i64 {
                 let left: Left.Value = Left.make();
                 let right = Right.Value { x: 21 };
                 Left.value(left) + Right.value(right)
             }",
        ),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for symbol in [
        "%tz.record.Left.Value = type",
        "%tz.record.Right.Value = type",
        "@tz.fn.Left.value",
        "@tz.fn.Right.value",
    ] {
        assert!(ir.contains(symbol), "{symbol}");
    }
    let error = analyze_modules(&[
        ("Left", "record Value { x: i64 }"),
        ("Right", "record Value { x: i64 }"),
        ("Main", "fn f(value: Value) -> i64 { value.x }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004");
    assert!(error.message.contains("ambiguous"));
    assert!(error.message.contains("Left.Value"));
    assert!(error.message.contains("Right.Value"));
    assert_eq!(error.span.source, Some(2));

    let error = analyze_modules(&[
        ("Left", "record Value { x: i64 }"),
        ("Right", "record Value { x: i64 }"),
        ("Main", "fn f(value: Left.Value) -> Right.Value { value }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1003");
    assert!(
        error
            .message
            .contains("expected Right.Value, found Left.Value")
    );
}

#[test]
fn resolves_cross_module_recursion_and_qualified_self_tail_calls() {
    let module = analyze_modules(&[
        (
            "Even",
            "fn test(n: i64) -> bool { if n == 0 { true } else { Odd.test(n - 1) } }",
        ),
        (
            "Odd",
            "fn test(n: i64) -> bool { if n == 0 { false } else { Even.test(n - 1) } }",
        ),
        (
            "Loop",
            "fn sum(n: i64, a: [i64]) -> i64 {
                 let value = a[n & 1];
                 if n == 0 { value } else { Loop.sum(n - 1, [value + 1, value]) }
             }
             fn down(n: i64) -> i64 { if n == 0 { 0 } else { n - 1 |> Loop.down } }",
        ),
        ("Main", "Even.test(100)"),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i1 @tz.fn.Odd.test"));
    assert!(ir.contains("call i1 @tz.fn.Even.test"));
    for name in ["sum", "down"] {
        let body = ir
            .split(&format!("define internal i64 @tz.fn.Loop.{name}("))
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains(&format!("call i64 @tz.fn.Loop.{name}")));
    }
    let sum = ir.split("@tz.fn.Loop.sum(").nth(1).unwrap();
    assert!(sum.find("alloca").unwrap() < sum.find("br label %loop").unwrap());
}

#[test]
fn rejects_unqualified_foreign_functions_and_missing_members() {
    for source in [
        "fn main() -> f64 { distance(Point { x: 3.0, y: 4.0 }) }",
        "fn main() -> f64 { Point.missing() }",
        "fn main() -> f64 { Missing.distance() }",
        "fn main() -> f64 { Point.sqrt(4.0) }",
    ] {
        let error = analyze_modules(&[("Point", POINT), ("Main", source)]).unwrap_err();
        assert_eq!(error.code, "E1002", "{source}: {}", error.message);
        assert_eq!(error.span.source, Some(1));
    }
}

#[test]
fn validates_module_names_and_preserves_unique_export_abi() {
    for name in ["", "_", "fn", "Bad-Name", "Nested.Module", "日本語"] {
        let error = analyze_modules(&[(name, "fn value() -> i64 { 1 }")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}");
    }
    assert_eq!(
        analyze_modules(&[("Same", ""), ("Same", "")])
            .unwrap_err()
            .code,
        "E1011"
    );
    for source in ["module Nested {}", "namespace Nested", "module Main"] {
        assert_eq!(
            analyze_modules(&[("Main", source)]).unwrap_err().code,
            "E0002"
        );
    }
    let error = analyze_modules(&[
        ("A", "export fn value() -> i64 { 1 }"),
        ("B", "export fn value() -> i64 { 2 }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1001");
    assert_eq!(error.span.source, Some(1));
    assert!(error.message.contains("tz_value"));
    assert!(
        analyze_modules(&[
            ("module", "fn value() -> i64 { 42 }"),
            ("Main", "module.value()"),
        ])
        .is_ok()
    );
}

#[test]
fn uses_only_main_as_the_application_entry_point() {
    let module = analyze_modules(&[
        ("Other", "fn main() -> bool { true }"),
        ("Main", "fn main() -> i64 { 42 }"),
    ])
    .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].qualified_name(),
        "Main.main"
    );
    assert!(
        llvm::emit(&module, llvm::Entry::Console)
            .unwrap()
            .contains("call i64 @tz.fn.Main.main()")
    );

    let library = analyze_modules(&[("Other", "fn main() -> i64 { 42 }")]).unwrap();
    assert_eq!(
        llvm::emit(&library, llvm::Entry::Console).unwrap_err().code,
        "E2004"
    );
    for sources in [
        vec![("Other", "let x = 42")],
        vec![("main", "42")],
        vec![("Main", "fn main() -> i64 { 1 }\nlet value = 42")],
    ] {
        assert_eq!(analyze_modules(&sources).unwrap_err().code, "E2004");
    }
    for source in [
        "fn main(x: i64) -> i64 { x }",
        "record R {} fn main() -> R { R {} }",
        "[1, 2]",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        assert_eq!(
            llvm::emit(&module, llvm::Entry::Console).unwrap_err().code,
            "E2004"
        );
    }
}

#[test]
fn supports_entry_bindings_with_newlines_or_semicolons_and_a_final_result() {
    for source in [
        "let x = 40\nlet x = x + 2\nx",
        "let x = 40; let x = x + 2; x",
        "let x: i64 = 40\r\nlet y = x + 2\r\ny",
        "let x = 40 // comment\nlet y = x + 2 // comment\n y",
        "let x = 40\n(x + 2)",
        "let x = 40\n-x",
        "let x = 40 +\n2\nx",
        "let x = (40\n+ 2)\nx",
        "let x = if true {\n40\n} else {\n0\n}\nx + 2",
        "fn f() -> i64 { 42 }\nlet g = f\n{ g() }",
        "fn f() -> i64 { 42 }\nlet g = Main.f\n{ g() }",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        let entry = &module.functions[module.entry.unwrap()];
        assert_eq!(entry.signature.result, Type::I64);
        assert!(matches!(entry.body.kind, TypedExprKind::Block { .. }));
    }
    for source in [
        "let x = 1 let y = 2",
        "fn f() -> i64 { let x = 1\nx }",
        "let x = 1\nfn f() -> i64 { x }",
    ] {
        assert_eq!(
            analyze_modules(&[("Main", source)]).unwrap_err().code,
            "E0002",
            "{source}"
        );
    }
}

#[test]
fn keeps_source_identity_for_lexing_parsing_types_and_recursive_layouts() {
    for (source, code) in [
        ("fn value() -> i64 { @ }", "E0001"),
        ("fn value() -> i64 {", "E0002"),
        ("fn value() -> i64 { true }", "E1003"),
        ("record R { r: R }", "E1010"),
    ] {
        let error = analyze_modules(&[("Main", "42"), ("Other", source)]).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.span.source, Some(1));
        assert!(error.span.end <= source.len());
    }
    let error =
        analyze_modules(&[("A", "record R { r: B.R }"), ("B", "record R { r: A.R }")]).unwrap_err();
    assert_eq!(error.code, "E1010");
}
