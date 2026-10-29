//! A20 Phase 2: the effect attributes `@no_alloc`, `@no_copy`, `@no_trap` and `@terminates`:
//! where they may be written (`E1032`), the `W1008` warnings, `@no_copy` (`E1031`), generic
//! instances, and that an attribute changes no emitted IR.

use tsuzuri::check::effects::Effect;
use tsuzuri::{analyze, llvm, parser};

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects(source: &str, code: &str, kind: &str) -> tsuzuri::diagnostic::Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    error
}

#[test]
fn parses_effect_attributes_in_any_order_with_cpu() {
    let source = format!(
        "@no_copy\n@cpu [\"avx2\"]\n@no_trap\ndef f :: i64 -> i64 = \\n -> n + 1\n\
         @terminates @no_alloc\nexport pure def g :: i64 -> i64 = \\n -> n * 2\n{MAIN}"
    );
    let program = parser::parse(&source).unwrap();
    let attributes: Vec<_> = program
        .effect_attributes
        .iter()
        .map(|attribute| (attribute.function.text.as_str(), attribute.kind.name()))
        .collect();
    assert_eq!(
        attributes,
        [
            ("f", "no_copy"),
            ("f", "no_trap"),
            ("g", "terminates"),
            ("g", "no_alloc")
        ]
    );
    assert_eq!(program.cpu_attributes.len(), 1);
    let first = &program.effect_attributes[0];
    assert_eq!(&source[first.span.start..first.span.end], "@no_copy");
    accepts(&source);
}

#[test]
fn keeps_effect_attribute_names_as_identifiers() {
    let module = accepts(&format!(
        "record R {{ no_copy: i64 }}\n\
         def f :: i64 -> i64 = \\n ->\n    let no_alloc = 1\n    let terminates = 2\n    n + no_alloc + terminates\n\
         def no_trap :: R -> i64 = \\r -> r.no_copy\n{MAIN}"
    ));
    assert!(module.effects.is_none());
}

#[test]
fn rejects_misplaced_duplicate_and_extern_attributes() {
    let misplaced = "effect_attribute_misplaced";
    rejects(
        &format!("@no_alloc\nrecord R {{ x: i64 }}\n{MAIN}"),
        "E1032",
        misplaced,
    );
    rejects(
        &format!("@no_trap\nunion U = A | B\n{MAIN}"),
        "E1032",
        misplaced,
    );
    rejects(
        &format!("@no_copy\ntype T = i64\n{MAIN}"),
        "E1032",
        misplaced,
    );
    rejects(
        &format!("@no_copy\nconst limit = 4\n{MAIN}"),
        "E1032",
        misplaced,
    );
    rejects(
        &format!(
            "def f :: i64 -> i64\ndef g :: i64 -> i64 = \\n -> n\n@terminates\nfn f n = n\n{MAIN}"
        ),
        "E1032",
        misplaced,
    );
    // After a signature without a body, an attribute at the declaration's column starts the next
    // declaration instead of a constraint line.
    accepts(&format!(
        "extern \"abs\" def c_abs :: i32 -> i32\n@no_copy\ndef f :: i64 -> i64 = \\n -> n\n{MAIN}"
    ));
    rejects(
        &format!("def f :: i64 -> i64 = \\n ->\n    @no_alloc n + 1\n{MAIN}"),
        "E1032",
        misplaced,
    );
    let error = rejects(
        &format!("@no_alloc\n@no_trap\n@no_alloc\ndef f :: i64 -> i64 = \\n -> n\n{MAIN}"),
        "E1032",
        "effect_attribute_duplicate",
    );
    assert_eq!(error.span.start, "@no_alloc\n@no_trap\n".len());
    rejects(
        &format!("@no_alloc\nextern \"abs\" def c_abs :: i32 -> i32\n{MAIN}"),
        "E1032",
        "effect_attribute_on_extern",
    );
}

#[test]
fn generic_instances_keep_the_attribute() {
    let generic = "@no_copy\ndef pair :: ref 'a -> ('a * 'a) = \\x -> (deref x, deref x)\n";
    let module = accepts(&format!(
        "{generic}{MAIN}\ndef use :: i64 -> i64 = \\n ->\n    match pair (ref n) with\n    | (a, b) -> a + b\n"
    ));
    let instances: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("pair"))
        .collect();
    assert!(!instances.is_empty());
    assert!(
        instances
            .iter()
            .all(|function| !function.origin.asserts.is_empty())
    );
    rejects(
        &format!(
            "{generic}def use :: ref [i64] -> i64 = \\xs ->\n    match pair xs with\n    | (a, _) -> a.length\n{MAIN}"
        ),
        "E1031",
        "effect_copy_in_no_copy",
    );
}

#[test]
fn lambdas_do_not_inherit_the_attribute() {
    let module = accepts(&format!(
        "@no_alloc\ndef twice :: i64 -> i64 = \\n ->\n    let f = \\x -> x * 2\n    f (f n)\n{MAIN}"
    ));
    for function in &module.functions {
        let attributed = !function.origin.asserts.is_empty();
        assert_eq!(
            attributed,
            function.qualified_name() == "Main.twice",
            "{}",
            function.qualified_name()
        );
    }
}

#[test]
fn no_copy_accepts_borrowing_code() {
    accepts(&format!(
        "@no_copy\ndef total :: ref [i64] -> i64 = \\xs ->\n    let mut sum = 0\n    for x in xs do\n        sum = sum + x\n    sum\n{MAIN}"
    ));
}

#[test]
fn no_copy_rejects_an_implicit_array_copy() {
    let source = format!("@no_copy\ndef copies :: ref [i64] -> [i64] = \\xs -> deref xs\n{MAIN}");
    let error = rejects(&source, "E1031", "effect_copy_in_no_copy");
    assert_eq!(&source[error.span.start..error.span.end], "deref xs");
    assert!(
        error.message.starts_with(
            "'Main.copies' is marked '@no_copy' but this expression implicitly copies every element"
        ),
        "{}",
        error.message
    );
    assert!(
        error
            .related
            .iter()
            .any(|related| related.message == "required by this attribute")
    );
}

#[test]
fn no_copy_follows_calls() {
    let source = format!(
        "def inner :: ref [i64] -> [i64] = \\xs -> deref xs\n\
         def middle :: ref [i64] -> i64 = \\xs -> (inner xs).length\n\
         @no_copy\ndef outer :: ref [i64] -> i64 = \\xs -> middle xs + 1\n{MAIN}"
    );
    let error = rejects(&source, "E1031", "effect_copy_in_no_copy");
    assert_eq!(&source[error.span.start..error.span.end], "middle");
    assert!(
        error.message.contains("this call reaches 'Main.middle', which implicitly copies every element (through 'Main.middle' -> 'Main.inner')"),
        "{}",
        error.message
    );
}

#[test]
fn explicit_array_copy_is_not_a_copy_effect() {
    let module = accepts(&format!(
        "@no_copy\ndef copied :: ref [i64] -> [i64] = \\xs -> Array.copy xs\n{MAIN}"
    ));
    let summaries = module.effects.as_ref().unwrap();
    let id = module
        .functions
        .iter()
        .position(|function| function.qualified_name() == "Main.copied")
        .unwrap();
    assert!(!summaries.summary(id).effects.contains(Effect::Copy));
}

#[test]
fn unchecked_generic_warns() {
    let module = accepts(&format!(
        "@no_alloc\ndef count :: ref ['a] -> i64 = \\xs -> xs.length\n{MAIN}"
    ));
    let warnings: Vec<_> = module
        .warnings
        .iter()
        .filter(|warning| warning.code == "W1008")
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, Some("effect_attribute_unchecked_generic"));
    assert_eq!(
        warnings[0].message,
        "'@no_alloc' on 'Main.count' is checked for each instantiation, and nothing instantiates it"
    );
    assert_eq!(warnings[0].span.start, 0);
}

#[test]
fn const_def_warns() {
    let module = accepts(&format!(
        "@no_trap\nconst def square :: i64 -> i64 = \\n -> n * n\nconst area: i64 = square 4\n{MAIN}"
    ));
    let warnings: Vec<_> = module
        .warnings
        .iter()
        .filter(|warning| warning.code == "W1008")
        .map(|warning| warning.kind)
        .collect();
    assert!(
        warnings.contains(&Some("effect_attribute_on_const_def")),
        "{warnings:?}"
    );
}

#[test]
fn all_four_attributes_are_available() {
    accepts(&format!(
        "@no_alloc\n@no_copy\n@no_trap\n@terminates\ndef f :: i64 -> i64 = \\n -> n * 3 + 1\n{MAIN}"
    ));
}

#[test]
fn attribute_does_not_change_ir() {
    let attributed = format!(
        "@no_alloc\n@no_trap\n@terminates\n@no_copy\ndef checksum :: ref [ubyte] -> i32u = \\bytes ->\n    let mut sum = 0i32u\n    for i in 0 .. bytes.length - 1 do\n        sum = sum * 31i32u + (bytes[i] as i32u)\n    sum\n\
         @no_trap @no_alloc\ndef safe :: ref [i64] -> i64 -> i64 = \\xs i -> if i < 0 || i >= xs.length then 0 else xs[i]\n{MAIN}"
    );
    // The same text with each attribute replaced by a comment of its length, so that every other
    // span stays where it was.
    let mut plain = attributed.clone();
    for name in ["@no_alloc", "@no_trap", "@terminates", "@no_copy"] {
        plain = plain.replace(name, &format!("//{}", " ".repeat(name.len() - 2)));
    }
    let emit = |source: &str| {
        let module = accepts(source);
        llvm::emit(&module, llvm::Entry::Library).unwrap()
    };
    let with = emit(&attributed);
    assert!(accepts(&attributed).effects.is_some());
    assert_eq!(emit(&plain), with);
    assert_eq!(with, emit(&attributed));
}

/// Review of #63: the attribute of the next declaration may follow the constraint lines of a
/// signature without a body; the constraint lines' indentation does not apply to it.
#[test]
fn an_attribute_after_constraint_lines_starts_the_next_declaration() {
    let source = "def show_it :: 'a -> string\n    @'a: Display\n@no_copy\ndef twice :: i64 -> i64 = \\x -> x * 2\nfn show_it value = to_string value\n";
    tsuzuri::analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    // A misindented constraint line is still refused.
    let error = tsuzuri::analyze(
        "def show_it :: 'a -> string\n    @'a: Display\n  @'a: Eq\nfn show_it value = to_string value\n",
    )
    .unwrap_err();
    assert_eq!(error.code, "E0002", "{}", error.message);
}
