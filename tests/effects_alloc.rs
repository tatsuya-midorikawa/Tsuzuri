//! A20 Phase 3: the `alloc` rule (appendix C of A20) through `@no_alloc`. Each case is one row of
//! the rule; the expected verdicts follow the emitter's own behavior, which
//! `tests/effects_alloc_conservative.rs` checks against the IR of the whole corpus.

use tsuzuri::analyze;

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";
const PRELUDE: &str = "record P { x: i64, y: i64 }\nunion Tree = Leaf | Node of (Tree * i64 * Tree)\n\
                       def takes :: string -> i64 = \\s -> s.length\n\
                       def curry_add :: i64 -> i64 -> i64 = \\a b -> a + b\n";

fn verdict(function: &str) -> Result<(), String> {
    let source = format!("{PRELUDE}@no_alloc\n{function}\n{MAIN}");
    match analyze(&source) {
        Ok(_) => Ok(()),
        Err(error) => {
            assert_eq!(error.code, "E1031", "{source}\n{}", error.message);
            assert_eq!(
                error.kind,
                Some("effect_alloc_in_no_alloc"),
                "{}",
                error.message
            );
            Err(error.message.clone())
        }
    }
}

fn no_alloc(function: &str) {
    if let Err(message) = verdict(function) {
        panic!("expected no alloc: {function}\n{message}");
    }
}

fn alloc(function: &str, what: &str) {
    match verdict(function) {
        Ok(()) => panic!("expected alloc: {function}"),
        Err(message) => assert!(message.contains(what), "{function}\n{message}"),
    }
}

#[test]
fn scalars_records_and_tuples_are_not_alloc() {
    no_alloc("def f :: i64 -> i64 = \\n -> n * 2 + 1");
    no_alloc("def f :: i64 -> P = \\n -> P { x: n, y: n + 1 }");
    no_alloc("def f :: i64 -> (i64 * bool) = \\n -> (n, n > 0)");
}

#[test]
fn array_literal_read_in_place_is_not_alloc() {
    no_alloc("def f :: i64 -> i64 = \\n ->\n    let xs = [1, 2, 3]\n    xs[0] + n");
    no_alloc(
        "def f :: i64 -> i64 = \\n ->\n    let mut total = 0\n    for i in 0 .. n do\n        let xs = [i, i + 1]\n        total = total + xs[1]\n    total",
    );
}

#[test]
fn returned_array_literal_is_alloc() {
    alloc(
        "def f :: i64 -> [i64] = \\n -> [n, n + 1, n + 2]",
        "leaves its slot, so it is copied to the heap",
    );
}

#[test]
fn new_array_is_alloc() {
    alloc(
        "def f :: i64 -> i64 = \\n ->\n    let xs = new [1, 2, 3]\n    xs[0] + n",
        "allocates an array or list",
    );
}

#[test]
fn string_literal_read_in_place_is_not_alloc() {
    no_alloc("def f :: i64 -> i64 = \\n ->\n    let s = \"hello\"\n    s.length + n");
    no_alloc("def f :: i64 -> bool = \\n -> String.starts_with \"he\" \"hello\"");
}

#[test]
fn moved_string_literal_is_alloc() {
    alloc(
        "def f :: i64 -> i64 = \\n -> takes \"hello\" + n",
        "leaves its slot, so it is copied to the heap",
    );
    alloc(
        "def f :: i64 -> string = \\_n -> \"hello\"",
        "leaves its slot",
    );
}

#[test]
fn empty_string_literal_is_not_alloc() {
    no_alloc("def f :: i64 -> string = \\_n -> \"\"");
}

#[test]
fn interpolation_and_concatenation_are_alloc() {
    alloc(
        "def f :: ref string -> ref string -> string = \\a b -> $\"{a}{b}\"",
        "builds a new string",
    );
}

#[test]
fn large_literal_is_alloc() {
    let elements = vec!["1"; 9000].join(", ");
    alloc(
        &format!("def f :: i64 -> i64 = \\n ->\n    let xs = [{elements}]\n    xs[0] + n"),
        "is larger than 65536 bytes",
    );
    let elements = vec!["1"; 2000].join(", ");
    no_alloc(&format!(
        "def f :: i64 -> i64 = \\n ->\n    let xs = [{elements}]\n    xs[0] + n"
    ));
}

#[test]
fn closure_without_captures_is_not_alloc() {
    no_alloc("def f :: i64 -> i64 = \\n ->\n    let g = \\x -> x * 2\n    g (g n)");
}

#[test]
fn capturing_closure_is_alloc() {
    alloc(
        "def f :: ref [i64] -> i64 -> [i64] = \\xs n -> Array.map (\\x -> x + n) xs",
        "needs a closure environment",
    );
}

#[test]
fn recursive_union_node_is_alloc() {
    alloc(
        "def f :: i64 -> Tree = \\n -> Node (Leaf, n, Leaf)",
        "creates a heap-allocated node of a recursive union",
    );
}

#[test]
fn partial_application_is_alloc() {
    alloc(
        "def f :: i64 -> i64 = \\n ->\n    let add = curry_add n\n    add 1",
        "needs a closure environment",
    );
}

#[test]
fn matched_tuple_of_arrays_is_alloc() {
    alloc(
        "def f :: i64 -> i64 = \\n ->\n    match ([1, 2], [3, 4]) with\n    | (a, b) -> a[0] + b[1] + n",
        "leaves its slot",
    );
}

#[test]
fn allocating_std_calls_are_alloc() {
    alloc(
        "def f :: i64 -> i64 = \\n ->\n    let out = Array.init n (\\i -> i * 2)\n    out.length",
        "this call reaches 'Array.init', which allocates an array or list",
    );
}

#[test]
fn reading_std_calls_are_not_alloc() {
    no_alloc("def f :: ref [i64] -> i64 = \\xs -> Array.length xs + Array.sum xs");
    no_alloc("def f :: ref string -> i64 = \\s -> s.length");
    no_alloc("def f :: f64 -> f64 = \\x -> Math.sqrt x + Math.abs x");
    no_alloc("def f :: i64 -> i64 = \\x -> Int.abs x + Int.max x 3");
}
