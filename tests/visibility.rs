use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::syntax::Visibility;
use tsuzuri::{analyze_modules, llvm};

const SECRET: &str = "private record Token { value: i64 }

private def hidden :: i64 -> i64
fn hidden x = x + 1

private def token :: i64 -> Token
fn token value = Token { value: value }

private def (|Even|_|) :: i64 -> bool
fn (|Even|_|) n = n % 2 == 0

private def unwrap :: Token -> i64
fn unwrap token = match token with | Token { value = v } -> v

def reveal :: i64 -> i64
fn reveal x = match unwrap (token (hidden x)) with | Even -> 42 | _ -> 0
";

fn accepts(sources: &[(&str, &str)]) -> tsuzuri::check::CheckedModule {
    analyze_modules(sources)
        .unwrap_or_else(|error| panic!("{sources:?}\n{}: {}", error.code, error.message))
}

fn rejects(sources: &[(&str, &str)], code: &str) -> Diagnostic {
    let error = analyze_modules(sources).expect_err("source should be rejected");
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    error
}

fn span_of(source: &str, text: &str) -> (usize, usize) {
    let start = source
        .find(text)
        .unwrap_or_else(|| panic!("missing {text}"));
    (start, start + text.len())
}

fn assert_span(error: &Diagnostic, source: &str, text: &str, index: usize) {
    assert_eq!((error.span.start, error.span.end), span_of(source, text));
    assert_eq!(error.span.source, Some(index), "{}", error.message);
}

#[test]
fn private_declarations_are_usable_inside_their_module() {
    let module = accepts(&[
        ("Secret", SECRET),
        ("Main", "fn main() -> i64 { Secret.reveal 40 }"),
    ]);
    let visibility = |name: &str| {
        module
            .functions
            .iter()
            .find(|function| function.module == "Secret" && function.name == name)
            .unwrap()
            .visibility
    };
    assert_eq!(visibility("hidden"), Visibility::Private);
    assert_eq!(visibility("token"), Visibility::Private);
    assert_eq!(visibility("reveal"), Visibility::Public);
    let record = module
        .records
        .iter()
        .find(|record| record.name == "Secret.Token")
        .unwrap();
    assert_eq!(record.visibility, Visibility::Private);
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("@tz.fn.Secret.hidden"));
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Console).unwrap());
}

#[test]
fn let_implementations_and_recursive_groups_inherit_def_visibility() {
    let module = accepts(&[(
        "Main",
        "private def add :: i64 -> i64 -> i64
         let add = x -> y -> x + y
         def rec even :: i64 -> bool
         private def and odd :: i64 -> bool
         fn rec even n = if n == 0 then true else odd (n - 1)
         and odd n = if n == 0 then false else even (n - 1)
         def main :: i64
         fn main = if even 10 then add 40 2 else 0",
    )]);
    for (name, visibility) in [
        ("add", Visibility::Private),
        ("even", Visibility::Public),
        ("odd", Visibility::Private),
        ("main", Visibility::Public),
    ] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        assert_eq!(function.visibility, visibility, "{name}");
    }
}

#[test]
fn exports_only_public_abi_even_when_they_use_private_helpers() {
    let module = accepts(&[(
        "Main",
        "private record Pair { left: i64, right: i64 }
         private def helper :: i64 -> i64
         fn helper x = { let pair = Pair { left: x, right: 2 }; pair.left + pair.right }
         export def api :: i64
         fn api = helper 40",
    )]);
    let header = llvm::header(&module);
    assert!(header.contains("tz_api"));
    assert!(!header.contains("helper"));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("define i64 @tz_api"));
    assert!(!ir.contains("@tz_helper"));
    assert!(ir.contains("@tz.fn.Main.helper"));
}

#[test]
fn builder_helpers_may_be_private_but_operations_may_not() {
    let helper = "private def wrap :: 'a -> 'a
fn wrap value = value

def Return :: 'a -> 'a
fn Return value = wrap value
";
    accepts(&[("Wrapped.tc", helper), ("Main", "Wrapped { return 42 }")]);
    let error = rejects(
        &[("Wrapped.tc", helper), ("Main", "Wrapped.wrap 42")],
        "E1022",
    );
    assert_span(&error, "Wrapped.wrap 42", "wrap", 1);
    let private_bind = "private def Bind :: 'a -> ('a -> 'b) -> 'b
fn Bind value next = next value

def Return :: 'a -> 'a
fn Return value = value
";
    let error = rejects(
        &[
            ("Wrapped.tc", private_bind),
            ("Main", "Wrapped { return 1 }"),
        ],
        "E1022",
    );
    assert!(
        error
            .message
            .contains("builder operation 'Bind' cannot be private")
    );
    let start = private_bind.find("Bind value").unwrap();
    assert_eq!(
        (error.span.start, error.span.end),
        (start, start + "Bind".len())
    );
}

#[test]
fn private_is_a_keyword_and_only_precedes_def_or_record() {
    rejects(&[("Main", "let private = 1\nprivate")], "E0002");
    rejects(&[("private", "fn value() -> i64 { 1 }")], "E1011");
    for (name, source) in [
        ("Main", "private fn f() -> i64 { 1 }"),
        ("Main", "def f :: i64\nprivate fn f = 1"),
        ("Main", "def f :: i64 -> i64\nprivate let f = x -> x"),
        ("Main", "private 42"),
        (
            "Classes.tt",
            "private class Show<'a> { def show :: 'a -> i64 }",
        ),
        (
            "Main",
            "record T { x: i64 }\nprivate instance Add<T> { fn add a b = a }",
        ),
    ] {
        let error = rejects(&[(name, source)], "E1022");
        assert_span(&error, source, "private", 0);
    }
    for (source, text) in [
        ("private export def f :: i64\nfn f = 1", "private export"),
        ("export private def f :: i64\nfn f = 1", "export private"),
    ] {
        let error = rejects(&[("Main", source)], "E1022");
        assert!(error.message.contains("'private export' is not allowed"));
        assert_span(&error, source, text, 0);
    }
}

#[test]
fn rejects_private_names_from_other_modules() {
    for (main, name) in [
        ("fn main() -> i64 { Secret.hidden 1 }", "hidden"),
        ("fn main() -> i64 { let f = Secret.hidden; f 1 }", "hidden"),
        (
            "fn main() -> i64 { let token = Secret.Token { value: 1 }; token.value }",
            "Secret.Token",
        ),
        (
            "fn main() -> i64 { let token: Secret.Token = Secret.reveal 1; 0 }",
            "Secret.Token",
        ),
        (
            "def f :: [Secret.Token] -> i64\nfn f tokens = tokens.length",
            "Secret.Token",
        ),
        ("fn f(token: Token) -> i64 { 0 }", "Token"),
        (
            "fn f(n: i64) -> i64 { match n with | Secret.Even -> 1 | _ -> 0 }",
            "Secret.Even",
        ),
        (
            "fn f(n: i64) -> i64 { match n with | Secret.Token { value = v } -> v }",
            "Secret.Token",
        ),
    ] {
        let error = rejects(&[("Secret", SECRET), ("Main", main)], "E1022");
        assert!(
            error
                .message
                .contains("only visible inside module 'Secret'"),
            "{}",
            error.message
        );
        assert_span(&error, main, name, 1);
    }
}

#[test]
fn local_values_still_shadow_modules_with_private_functions() {
    accepts(&[
        ("Secret", "private def value :: i64\nfn value = 1"),
        (
            "Main",
            "record Box { value: i64 }
             fn main() -> i64 { let Secret = Box { value: 42 }; Secret.value }",
        ),
    ]);
}

#[test]
fn rejects_private_types_leaking_from_public_declarations() {
    for (source, owner, text) in [
        (
            "private record Token { value: i64 }\ndef make :: Token\nfn make = Token { value: 1 }",
            "public function 'Secret.make'",
            "Token\nfn",
        ),
        (
            "private record Token { value: i64 }\ndef count :: [Token] -> i64\nfn count tokens = tokens.length",
            "public function 'Secret.count'",
            "Token]",
        ),
        (
            "private record Token { value: i64 }\nrecord Box { token: Token }",
            "public record 'Secret.Box'",
            "Token }",
        ),
        (
            "private record Token { value: i64 }\ndef (|Valid|_|) :: Token -> bool\nfn (|Valid|_|) token = token.value > 0",
            "public function 'Secret.(|Valid|_|)'",
            "Token ->",
        ),
    ] {
        let error = rejects(&[("Secret", source)], "E1022");
        assert!(
            error
                .message
                .contains("private type 'Secret.Token' leaks from")
        );
        assert!(error.message.contains(owner), "{}", error.message);
        let start = source.find(text).unwrap();
        assert_eq!(
            (error.span.start, error.span.end),
            (start, start + "Token".len())
        );
    }
    accepts(&[(
        "Secret",
        "private record Token { value: i64 }
         private record Box { token: Token }
         private def make :: Token
         fn make = Token { value: 1 }
         instance Add<Token> { fn add left right = Token { value: left.value + right.value } }",
    )]);
}

#[test]
fn private_records_do_not_make_unqualified_names_ambiguous() {
    let module = accepts(&[
        ("Left", "private record Value { x: i64 }"),
        ("Right", "record Value { x: i64 }"),
        ("Main", "fn f(value: Value) -> i64 { value.x }"),
    ]);
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    let tsuzuri::check::Type::Record(id, _) = function.signature.parameters[0] else {
        panic!("expected a record parameter")
    };
    assert_eq!(module.records[id].name, "Right.Value");
    let error = rejects(
        &[
            ("Left", "record Value { x: i64 }"),
            ("Right", "record Value { x: i64 }"),
            ("Hidden", "private record Value { x: i64 }"),
            ("Main", "fn f(value: Value) -> i64 { value.x }"),
        ],
        "E1004",
    );
    assert!(error.message.contains("Left.Value or Right.Value"));
    assert!(!error.message.contains("Hidden.Value"));
}
