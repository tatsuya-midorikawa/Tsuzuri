//! A20 Phase 6: an attributed function that calls a function-typed parameter is checked where it
//! is called, against the effects of the function that the call passes.

use tsuzuri::analyze;

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";
const APPLY: &str = "@no_alloc\ndef apply :: (i64 -> i64) -> i64 -> i64 = \\f x -> f x\n";

fn check(source: &str) -> Result<(), (String, Option<&'static str>, String)> {
    let source = format!("{source}{MAIN}");
    match analyze(&source) {
        Ok(_) => Ok(()),
        Err(error) => {
            assert_eq!(error.code, "E1031", "{source}\n{}", error.message);
            let at = source[error.span.start..error.span.end].to_owned();
            Err((error.message.clone(), error.kind, at))
        }
    }
}

#[test]
fn higher_order_no_alloc_accepts_a_pure_lambda() {
    check(&format!(
        "{APPLY}def user :: i64 -> i64 = \\n -> apply (\\y -> y + 1) n\n"
    ))
    .unwrap();
    check(&format!("{APPLY}def double :: i64 -> i64 = \\y -> y * 2\ndef user :: i64 -> i64 = \\n -> apply double n\n")).unwrap();
}

#[test]
fn higher_order_no_alloc_rejects_an_allocating_lambda() {
    let (message, kind, at) = check(&format!(
        "{APPLY}def user :: i64 -> i64 = \\n -> apply (\\y -> (Array.init y (\\i -> i)).length) n\n"
    ))
    .unwrap_err();
    assert_eq!(kind, Some("effect_alloc_in_no_alloc"), "{message}");
    assert!(
        message.starts_with("'Main.apply' is marked '@no_alloc' but this call reaches"),
        "{message}"
    );
    assert!(at.starts_with("apply"), "{at}");
}

#[test]
fn forwarded_parameter_moves_the_obligation() {
    check(&format!(
        "{APPLY}@no_alloc\ndef forward :: (i64 -> i64) -> i64 -> i64 = \\f x -> apply f x + 1\n\
         def user :: i64 -> i64 = \\n -> forward (\\y -> y + 1) n\n"
    ))
    .unwrap();
    let (message, kind, _) = check(&format!(
        "{APPLY}def forward :: (i64 -> i64) -> i64 -> i64 = \\f x -> apply f x + 1\n"
    ))
    .unwrap_err();
    assert_eq!(kind, Some("effect_unknown_callee"));
    assert!(
        message.contains("add '@no_alloc' to 'Main.forward' so that its callers are checked"),
        "{message}"
    );
}

#[test]
fn dyn_call_is_unknown() {
    let (message, kind, _) = check(
        "class Shape<'a> {\n    def area :: ref 'a -> i64\n}\nrecord Square { side: i64 }\n\
         instance Shape<Square> {\n    fn area square = square.side * square.side\n}\n\
         @no_alloc\ndef measure :: ref dyn Shape -> i64 = \\shape -> Shape.area shape\n",
    )
    .unwrap_err();
    assert_eq!(kind, Some("effect_unknown_callee"));
    assert!(
        message.contains("uses dynamic dispatch ('dyn')"),
        "{message}"
    );
}

#[test]
fn closed_callers_need_no_obligation() {
    check(&format!(
        "{APPLY}@no_alloc\ndef user :: i64 -> i64 = \\n -> apply (\\y -> y * n) n\n"
    ))
    .unwrap_err();
    check(&format!(
        "{APPLY}@no_alloc\ndef user :: i64 -> i64 = \\n -> apply (\\y -> y * 3) n\n"
    ))
    .unwrap();
}

/// Review of #63: a call through an immutable alias or a partial application is a call of the
/// attributed function, so the callback it passes is checked as at a direct call.
#[test]
fn alias_and_partial_calls_check_their_callbacks() {
    const ALLOCATING: &str = "(\\y -> (Array.init y (\\i -> i)).length)";
    for body in [
        format!("\\n ->\n    let invoke = apply\n    invoke {ALLOCATING} n"),
        format!("\\n ->\n    let bound = apply {ALLOCATING}\n    bound n"),
        format!("\\n ->\n    let invoke = apply\n    let again = invoke\n    again {ALLOCATING} n"),
    ] {
        let (message, kind, _) =
            check(&format!("{APPLY}def user :: i64 -> i64 = {body}\n")).unwrap_err();
        assert_eq!(kind, Some("effect_alloc_in_no_alloc"), "{body}\n{message}");
    }
    check(&format!(
        "{APPLY}def user :: i64 -> i64 = \\n ->\n    let invoke = apply\n    invoke (\\y -> y + 1) n\n"
    ))
    .unwrap();
}

/// Review of #63: an attributed function that takes a function inside an aggregate (here an
/// `Owned.Function`) still has its callers checked.
#[test]
fn owned_function_parameters_are_checked_at_their_callers() {
    let wrapper = "@no_trap\ndef run :: ref Owned.Function<i64, i64> -> i64 -> i64 = \\f x -> Owned.call f x\n";
    let (message, kind, _) = check(&format!(
        "{wrapper}def user :: i64 -> i64 = \\n ->\n    let f = Owned.function (\\y -> 10 / y)\n    run (ref f) n\n"
    ))
    .unwrap_err();
    assert!(
        matches!(
            kind,
            Some("effect_trap_in_no_trap" | "effect_unknown_callee")
        ),
        "{kind:?}: {message}"
    );
}

/// Review of #63: a function that calls its 65th parameter cannot record it in the bit set, so the
/// call is one that the analysis cannot follow, and an attribute that needs it is refused.
#[test]
fn a_call_of_a_parameter_beyond_64_is_not_followed() {
    let parameters: Vec<String> = (0..65).map(|index| format!("p{index}")).collect();
    let types = vec!["i64"; 64].join(" -> ");
    let source = format!(
        "@no_alloc\ndef wide :: {types} -> (i64 -> i64) -> i64 = \\{} -> {} 1\n",
        parameters.join(" "),
        parameters[64]
    );
    let (message, kind, _) = check(&source).unwrap_err();
    assert_eq!(kind, Some("effect_unknown_callee"), "{message}");
}
