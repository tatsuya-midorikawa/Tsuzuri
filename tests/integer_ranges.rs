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
