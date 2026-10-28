//! G30 `transitions` reads the syntax tree: the shapes it relies on (a `match` over the first
//! parameter, case patterns, case constructors in the arms) are those that `light/Light.tz` parses
//! into.

use tsuzuri::parser::parse_with_source_all;
use tsuzuri::syntax::{Edition, ExprKind, PatternKind};

#[test]
fn light_function_bodies_have_the_expected_shape() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/diagram/src/light/Light.tz"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let program = parse_with_source_all(&text, 0, Edition::E2026).unwrap();
    let names: Vec<&str> = program
        .functions
        .iter()
        .map(|function| function.name.text.as_str())
        .collect();
    assert_eq!(names, ["next", "reset"]);
    for function in &program.functions {
        assert_eq!(function.parameters[0].name.text, "light");
        // A body on the lines after `=` is a block without bindings around the `match`.
        let ExprKind::Block { bindings, result } = &function.body.kind else {
            panic!("{} is not a block", function.name.text);
        };
        assert!(bindings.is_empty());
        let ExprKind::Match { value, arms, .. } = &result.kind else {
            panic!("{} is not a match", function.name.text);
        };
        assert!(matches!(&value.kind, ExprKind::Name(id) if id.text == "light"));
        let patterns: Vec<String> = arms
            .iter()
            .map(|arm| match &arm.pattern.kind {
                PatternKind::Binding(id) => id.text.clone(),
                PatternKind::Apply(id, _) => format!("{}(..)", id.text),
                PatternKind::Wildcard => "_".to_owned(),
                other => format!("{other:?}"),
            })
            .collect();
        match function.name.text.as_str() {
            "next" => {
                assert_eq!(patterns, ["Red", "Green", "Yellow(..)"]);
                assert!(matches!(&arms[1].body.kind, ExprKind::Call(callee, _)
                    if matches!(&callee.kind, ExprKind::Name(id) if id.text == "Yellow")));
                assert!(matches!(&arms[2].body.kind, ExprKind::If { .. }));
            }
            _ => assert_eq!(patterns, ["Red", "_"]),
        }
    }
}
