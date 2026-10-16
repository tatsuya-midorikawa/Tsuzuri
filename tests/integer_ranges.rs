//! PR02: integer range proofs and overflow flags. Without a proof an integer `+ - *` wraps, so no
//! instruction may carry `nsw` or `nuw`; these tests pin that baseline and the loop shapes that the
//! structural proofs (R1-R5) depend on.

use tsuzuri::{analyze, llvm};

/// The IR of `source` for native or WASM; two emissions must be identical.
fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    let first = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        first,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    first
}

/// The body of `@tz.fn.Main.<name>`, from its `define` line to the closing brace.
fn function_ir(ir: &str, name: &str) -> String {
    let marker = format!("@tz.fn.Main.{name}(");
    let rest = ir
        .split_once(&marker)
        .unwrap_or_else(|| panic!("no function {name}"))
        .1;
    rest.split("\n}").next().unwrap().to_owned()
}

/// The IR of `source`, emitted once.
fn emit_once(source: &str, wasm: bool) -> String {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!(
            "{source}\n{}: {} at {:?}",
            error.code, error.message, error.span
        )
    });
    llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
}

fn has_flag(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| matches!(word, "nsw" | "nuw"))
}

/// One program per loop form and per kind of integer arithmetic, each with its function name.
const PROGRAMS: &[(&str, &str)] = &[
    (
        "ascending",
        "def ascending :: i64 -> i64\nfn ascending n =\n    let mut total = 0i64\n    for i in 0 .. n do total = total + i\n    total\n",
    ),
    (
        "descending",
        "def descending :: i64 -> i64\nfn descending n =\n    let mut total = 0i64\n    for i in n .. -1 .. 0 do total = total + i\n    total\n",
    ),
    (
        "unsigned",
        "def unsigned :: i64u -> i64u\nfn unsigned n =\n    let mut total = 0i64u\n    for i in 0i64u .. n do total = total + i\n    total\n",
    ),
    (
        "wide",
        "def wide :: i128 -> i128\nfn wide n =\n    let mut total = 0i128\n    for i in 0i128 .. n do total = total + i\n    total\n",
    ),
    (
        "narrow_up",
        "def narrow_up :: i32 -> i64\nfn narrow_up n =\n    let mut total = 0i64\n    for i = 0 to n - 1 do total = total + (i as i64)\n    total\n",
    ),
    (
        "narrow_down",
        "def narrow_down :: i32 -> i64\nfn narrow_down n =\n    let mut total = 0i64\n    for i = n downto 1 do total = total + (i as i64)\n    total\n",
    ),
    (
        "narrow_unsigned",
        "def narrow_unsigned :: i32u -> i64\nfn narrow_unsigned n =\n    let mut total = 0i64\n    for i in 0i32u .. n do total = total + (i as i64)\n    total\n",
    ),
    (
        "stepped",
        "def stepped :: i64 -> i64\nfn stepped n =\n    let mut total = 0i64\n    for i in 0 .. 2 .. n do total = total + i\n    total\n",
    ),
    (
        "each_array",
        "def each_array :: ref [i64] -> i64\nfn each_array values =\n    let mut total = 0i64\n    for value in values do total = total + value\n    total\n",
    ),
    (
        "each_list",
        "def each_list :: ref [|i64|] -> i64\nfn each_list values =\n    let mut total = 0i64\n    for value in values do total = total + value\n    total\n",
    ),
    (
        "each_text",
        "def each_text :: ref string -> i64\nfn each_text text =\n    let mut total = 0i64\n    for unit in text do total = total + (unit as i64)\n    total\n",
    ),
    (
        "arithmetic",
        "def arithmetic :: i64 -> i64 -> i64\nfn arithmetic a b = a + b * 3 - 1\n",
    ),
    (
        "counted",
        "def counted :: i64 -> i64\nfn counted count =\n    let values = new [i64](count, \\i -> i * 3)\n    let mut total = 0i64\n    for i in 0 .. values.length - 1 do total = total + values[i] + i * 4 + 3\n    total\n",
    ),
];

#[test]
fn programs_have_no_overflow_flags_without_a_proof() {
    for (name, source) in PROGRAMS {
        for wasm in [false, true] {
            let ir = emit(source, wasm);
            let body = function_ir(&ir, name);
            assert!(!has_flag(&body), "{name} wasm={wasm}\n{body}");
        }
    }
}

/// `x op y` and the builtin call `Class.method x y` for each operator, and the types that carry it.
const OPERATORS: &[(&str, &str, &str)] = &[
    ("+", "Add.add", "add"),
    ("-", "Sub.sub", "sub"),
    ("*", "Mul.mul", "mul"),
];
const NUMBER_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "i8u", "i16u", "i32u", "i64u", "i128u", "f16", "f32", "f64",
    "f128", "d32", "d64", "d128",
];

/// Two functions that apply `operator` to their parameters, once as the operator and once as the builtin.
fn operator_program(ty: &str, operator: &str, builtin: &str) -> String {
    format!(
        "def written :: {ty} -> {ty} -> {ty}\nfn written a b = a {operator} b\n\
         def named :: {ty} -> {ty} -> {ty}\nfn named a b = {builtin} a b\n"
    )
}

#[test]
fn builtin_and_operator_forms_emit_the_same_instructions() {
    for (operator, builtin, instruction) in OPERATORS {
        for ty in NUMBER_TYPES {
            for wasm in [false, true] {
                let ir = emit_once(&operator_program(ty, operator, builtin), wasm);
                let operator_body = function_ir(&ir, "written");
                let builtin_body = function_ir(&ir, "named");
                assert_eq!(operator_body, builtin_body, "{ty} {builtin} wasm={wasm}");
                assert!(
                    !builtin_body.contains("$intrinsic"),
                    "{ty} {builtin}\n{builtin_body}"
                );
                // The types with a direct LLVM instruction show it; the others call the numeric runtime.
                let direct = match *ty {
                    "f32" | "f64" => Some(format!("f{instruction} ")),
                    "f16" | "f128" | "d32" | "d64" | "d128" => None,
                    _ => Some(format!("{instruction} ")),
                };
                if let Some(direct) = direct {
                    assert!(
                        builtin_body.contains(&direct),
                        "{ty} {builtin}\n{builtin_body}"
                    );
                    assert!(
                        !builtin_body.contains("call "),
                        "{ty} {builtin}\n{builtin_body}"
                    );
                }
            }
        }
    }
}

#[test]
fn builtin_forms_evaluate_their_operands_left_to_right_like_the_operator() {
    let source = "def f :: i64 -> i64\nfn f x = x + 1\ndef g :: i64 -> i64\nfn g x = x * 2\n\
        def operator :: i64 -> i64\nfn operator n = f n + g n\n\
        def builtin :: i64 -> i64\nfn builtin n = Add.add (f n) (g n)\n";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in ["operator", "builtin"] {
            let body = function_ir(&ir, name);
            let first = body.find("call i64 @tz.fn.Main.f(").unwrap();
            let second = body.find("call i64 @tz.fn.Main.g(").unwrap();
            assert!(first < second, "{name}\n{body}");
            assert_eq!(body.matches("call ").count(), 2, "{name}\n{body}");
        }
    }
}

#[test]
fn builtin_calls_that_are_not_two_number_operands_stay_calls() {
    // Partial application and a function value keep the generated method; a string `Add.add` takes
    // its operands, and a function that merely has the name `Add.add` in a user module is a call.
    let source = "def partial :: i64 -> i64\nfn partial n = { let add = Add.add 20; add n }\n\
        def value :: unit -> (i64 -> i64 -> i64)\nfn value _unit = Add.add\n\
        def join :: string -> string -> string\nfn join a b = Add.add a b\n";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(function_ir(&ir, "partial").contains("@tz.apply.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "value").contains("@tz.apply.$intrinsic.Add.add"));
        assert!(function_ir(&ir, "join").contains("call "));
    }
}
