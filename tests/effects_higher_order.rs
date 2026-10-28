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
