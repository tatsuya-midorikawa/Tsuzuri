//! B09: local `let rec` functions, with `and` groups.

use tsuzuri::diagnostic::{Diagnostic, Severity};
use tsuzuri::{analyze, llvm, parser};

/// How many bindings of `program` have each recursion, in the order `rec`, `and`.
fn recursive_bindings(source: &str) -> (usize, usize) {
    let program = parser::parse(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let debug = format!("{program:?}");
    (
        debug.matches("recursion: Rec,").count(),
        debug.matches("recursion: And,").count(),
    )
}

fn parse_error(source: &str) -> Diagnostic {
    parser::parse(source).expect_err(source)
}

/// The library IR of a program that must check, which two emissions must agree on.
fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    assert!(llvm::duplicate_declarations(&ir).is_empty(), "{source}");
    ir
}

fn rejects(source: &str, code: &str) -> Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error
}

/// The IR of every function that lifting made from a lambda, with its symbol.
fn lifted(ir: &str) -> Vec<(String, String)> {
    ir.split("\ndefine ")
        .skip(1)
        .filter_map(|function| {
            let header = function.lines().next()?;
            let start = header.find("@tz.fn.$lambda.")?;
            let symbol = header[start..].split('(').next()?.to_owned();
            Some((symbol, function.split("\n}").next()?.to_owned()))
        })
        .collect()
}

/// Whether a lifted function jumps back to its start: a parameter's `phi` has an incoming value
/// from a back edge besides the one from the entry.
fn loops(body: &str) -> bool {
    body.lines()
        .any(|line| line.contains("= phi") && line.matches('[').count() >= 2)
}

#[test]
fn parses_local_let_rec_bindings() {
    for (source, expected) in [
        // An indented function body.
        (
            "def total :: i64 -> i64 = \\limit ->\n    let rec go = \\n acc -> if n > limit then acc else go (n + 1) (acc + n)\n    go 1 0\n",
            (1, 0),
        ),
        // A braced block of the compatible syntax.
        (
            "def f :: i64 -> i64\nfn f n = { let rec go = \\m -> if m == 0 then 0 else go (m - 1); go n }\n",
            (1, 0),
        ),
        // The statements of `Main.tz`, when no signature has the name.
        (
            "let rec go = \\n -> if n == 0 then 0 else go (n - 1)\ngo 3\n",
            (1, 0),
        ),
        // A body that runs IO binds.
        (
            "def main :: unit -> i32 = \\() ->\n    let rec go = \\n -> if n == 0 then 0 else go (n - 1)\n    do! IO.write_line (to_string (go 3))\n    0\n",
            (1, 0),
        ),
        // An annotation, and a lambda in parentheses.
        (
            "def f :: i64 -> i64 = \\x ->\n    let rec go: i64 -> i64 = (\\n -> if n == 0 then x else go (n - 1))\n    go x\n",
            (1, 0),
        ),
        // A group of two and of three, indented and braced.
        (
            "def f :: i64 -> bool = \\n ->\n    let rec even = \\m -> if m == 0 then true else odd (m - 1)\n    and odd = \\m -> if m == 0 then false else even (m - 1)\n    even n\n",
            (1, 1),
        ),
        (
            "def f :: i64 -> bool\nfn f n = { let rec a = \\m -> b m; and b = \\m -> c m; and c = \\m -> m == 0; a n }\n",
            (1, 2),
        ),
        // Two groups in one block stay two groups: each `let rec` starts one.
        (
            "def f :: i64 -> i64 = \\n ->\n    let rec a = \\m -> if m == 0 then 0 else a (m - 1)\n    let rec b = \\m -> if m == 0 then 1 else b (m - 1)\n    a n + b n\n",
            (2, 0),
        ),
    ] {
        assert_eq!(recursive_bindings(source), expected, "{source}");
    }
}

#[test]
fn a_signed_top_level_let_rec_is_still_the_implementation_of_its_def() {
    // `let rec sum` has a signature: it is the definition, not a statement of the entry code.
    let program = parser::parse(
        "def rec sum :: i64 -> i64 -> i64\nlet rec sum = \\n total -> if n == 0 then total else sum (n - 1) (total + n)\n\nsum 3 0\n",
    )
    .unwrap();
    assert_eq!(program.functions.len(), 1);
    assert_eq!(
        recursive_bindings("def rec sum :: i64 -> i64 -> i64\nlet rec sum = \\n total -> total\n"),
        (0, 0)
    );
}

#[test]
fn rejects_invalid_let_rec_syntax() {
    for (source, message) in [
        (
            "def f :: i64 -> i64 = \\n ->\n    let rec x = n + 1\n    x\n",
            "a 'let rec' binding needs a lambda such as '\\n -> ...'; bind other values with 'let'",
        ),
        (
            "def f :: i64 -> i64 = \\n ->\n    let rec go = Int.abs\n    go n\n",
            "a 'let rec' binding needs a lambda such as '\\n -> ...'; bind other values with 'let'",
        ),
        (
            "def f :: i64 -> i64 = \\n ->\n    let rec mut go = \\m -> m\n    go n\n",
            "a 'let rec' function cannot be mutable; rebind the function or use 'let mut' for data",
        ),
        (
            "def f :: i64 -> i64 = \\n ->\n    @checked let rec go = \\m -> m\n    go n\n",
            "@checked cannot apply to a 'let rec' binding; put it on the statements inside the function",
        ),
        (
            "def f :: i64 -> i64\nfn f n = { @checked let rec go = \\m -> m; go n }\n",
            "@checked cannot apply to a 'let rec' binding; put it on the statements inside the function",
        ),
        (
            "def f :: i64 -> i64\nfn f n = { let rec a = \\m -> m; @checked and b = \\m -> m; a n }\n",
            "@checked cannot apply to a 'let rec' binding; put it on the statements inside the function",
        ),
        // In a computation block, an `and` continues a group only after a `let rec` binding; the
        // diagnostic is the same with `@checked` before it as without.
        (
            "def f :: unit -> Maybe<i64> = \\() ->\n    let rec a = \\x -> x\n    @checked and g = \\x -> x\n    return 1\n",
            "@checked cannot apply to a 'let rec' binding; put it on the statements inside the function",
        ),
        (
            "def f :: unit -> Maybe<i64> = \\() ->\n    let a = 1\n    and g = \\x -> x\n    return a\n",
            "'and' continues a 'let rec' group; start it with 'let rec' before the 'and'",
        ),
        (
            "def f :: unit -> Maybe<i64> = \\() ->\n    let a = 1\n    @checked and g = \\x -> x\n    return a\n",
            "'and' continues a 'let rec' group; start it with 'let rec' before the 'and'",
        ),
        (
            "def choose :: i64 -> i64\nfn choose value\n    | go value > 10 -> 1\n    | otherwise -> 0\n    where\n        rec go = \\m -> m\n",
            "'where' bindings cannot be recursive; define the function with 'let rec' in the body",
        ),
        (
            "def f :: i64 -> i64 = \\n ->\n    let! rec go = \\m -> m\n    go n\n",
            "a 'let!' binding cannot be recursive; define the function with 'let rec' and bind its result with 'let!'",
        ),
        (
            "def f :: i64 -> i64 = \\n ->\n    let! a = Maybe.Some 1\n    and! rec b = \\m -> m\n    a\n",
            "a 'let!' binding cannot be recursive; define the function with 'let rec' and bind its result with 'let!'",
        ),
        // `use` takes no `rec`: the keyword is not an expression.
        (
            "def f :: i64 -> i64 = \\n ->\n    use rec go = \\m -> m\n    go n\n",
            "expected an expression",
        ),
    ] {
        let error = parse_error(source);
        assert_eq!(error.code, "E0002", "{source}\n{}", error.message);
        assert!(
            error.message.contains(message),
            "{source}\nexpected {message:?}, found {:?}",
            error.message
        );
    }
    // An `and` after a plain `let` stays what it was: a top-level `and` without a recursive
    // declaration before it.
    let error = parse_error(
        "def f :: i64 -> i64 = \\n ->\n    let go = \\m -> m\n    and other = \\m -> m\n    go n\n",
    );
    assert_eq!(error.code, "E1019");
    assert_eq!(
        error.message,
        "'and' requires a preceding recursive declaration or definition"
    );
}

#[test]
fn checks_local_let_rec_types_and_captures() {
    // A capture that copies without allocating is passed by value; any other capture is
    // borrowed, so a `let rec` function can read a string, an array, or a function value.
    for source in [
        "def f :: i64 -> bool -> i64 = \\n flag ->\n    let rec go = \\k -> if k == 0 then (if flag then n else 0) else go (k - 1)\n    go 3\n",
        "def f :: i64 -> i64 = \\n ->\n    let c = 'a'\n    let rec go = \\k -> if k == 0 then (if c == 'a' then 1 else 0) else go (k - 1)\n    go n\n",
        "def f :: [i64] -> i64 = \\values ->\n    let data = ref values\n    let rec go = \\i -> if i == data.length then 0 else data[i] + go (i + 1)\n    go 0\n",
        "def f :: string -> i64 = \\text ->\n    let view = ref text\n    let rec go = \\i -> if i == 0 then String.length view else go (i - 1)\n    go 2\n",
        "record P { x: i64, y: i64 }\ndef f :: i64 -> i64 = \\n ->\n    let p = P { x: 1, y: 2 }\n    let pair = (3, 4)\n    let rec go = \\k -> if k == 0 then p.x + p.y else go (k - 1)\n    go n\n",
        "def f :: [i64; 3] -> i64 = \\fixed ->\n    let rec go = \\i -> if i == 3 then 0 else fixed[i] + go (i + 1)\n    go 0\n",
        "def f :: i64 -> i64 = \\n ->\n    let scale = 2\n    let rec go = \\k -> if k == 0 then scale else go (k - 1)\n    go n\n",
        "def f :: string -> i64 -> i64 = \\text n ->\n    let rec go = \\k -> if k == 0 then String.length (ref text) else go (k - 1)\n    go n\n",
        "def f :: [i64] -> i64 = \\values ->\n    let rec go = \\i -> if i == values.length then 0 else values[i] + go (i + 1)\n    go 0\n",
        "def f :: [|i64|] -> i64 = \\values ->\n    let rec go = \\k -> if k == 0 then List.length (ref values) else go (k - 1)\n    go 2\n",
        "def f :: i64 -> i64 = \\n ->\n    let values = Vec.push (Vec.empty()) 7\n    let rec go = \\k -> if k == 0 then Vec.length (ref values) else go (k - 1)\n    go n\n",
        "record Named { name: string, size: i64 }\ndef f :: Named -> i64 = \\item ->\n    let rec go = \\k -> if k == 0 then item.size else go (k - 1)\n    go 2\n",
        "def f :: i64 -> i64 = \\n ->\n    let double = \\x -> x * 2\n    let rec go = \\k -> if k == 0 then 0 else double k + go (k - 1)\n    go n\n",
    ] {
        accepts(source);
    }
    // What a closure cannot capture, a `let rec` function cannot either.
    let error = rejects(
        "record Resource { id: i64 }\ninstance Drop<Resource> { fn drop value = () }\ndef f :: Resource -> i64 = \\resource ->\n    let rec go = \\k -> if k == 0 then resource.id else go (k - 1)\n    go 2\n",
        "E1005",
    );
    assert!(
        error
            .message
            .starts_with("cannot capture Main.Resource in a reusable function"),
        "{}",
        error.message
    );
    // A `const def` takes no function values, so it takes no `let rec` either.
    rejects(
        "const def triangle :: i64 -> i64 = \\n ->\n    let rec go = \\k acc -> if k > n then acc else go (k + 1) (acc + k)\n    go 1 0\nconst T: i64 = triangle 10\n",
        "E1026",
    );
    // A type variable decides nothing: the captures' types are known after specialization, but the
    // function is checked against the signature first.
    let error = rejects(
        "def keep :: Copy<'a> => 'a -> i64 -> 'a = \\x n ->\n    let rec go = \\k -> if k == 0 then x else go (k - 1)\n    go n\n",
        "E1005",
    );
    assert!(
        error.message.contains("a 'let rec' function cannot capture 'x', whose type 'a has a type variable; pass it as an argument"),
        "{}",
        error.message
    );
    // A generic function whose `let rec` captures nothing generic is fine at every instance.
    accepts(
        "def f :: [Add<'a>] -> i64 = \\values ->\n    let rec go = \\k -> if k == 0 then 0 else 1 + go (k - 1)\n    go 3\nlet a = f [1, 2]\nf [\"x\"]\n",
    );
    // The name has one monomorphic type, fixed by its uses.
    let error = rejects(
        "def f :: i64 -> i64 = \\n ->\n    let rec go = \\k -> if k == 0 then 0 else go \"x\"\n    go n\n",
        "E1003",
    );
    assert_eq!(error.message, "expected string, found i64");
    // Without `rec` the name is not in scope in its own value.
    let error = rejects(
        "def f :: i64 -> i64 = \\n ->\n    let go = \\k -> if k == 0 then 0 else go (k - 1)\n    go n\n",
        "E1002",
    );
    assert_eq!(error.message, "unknown value 'go'");
    // A group names each function once.
    rejects(
        "def f :: i64 -> i64 = \\n ->\n    let rec go = \\k -> k\n    and go = \\k -> k\n    go n\n",
        "E1001",
    );
    // The body borrows what it captures: it cannot move it out, also into a nested closure.
    for source in [
        "def f :: string -> i64 -> string = \\text n ->\n    let rec go = \\k -> if k == 0 then text else go (k - 1)\n    go n\n",
        "def f :: string -> i64 -> i64 = \\text n ->\n    let rec go = \\k ->\n        let g = \\j -> String.length (ref text) + j\n        if k == 0 then g 1 else go (k - 1)\n    go n\n",
    ] {
        let error = rejects(source, "E1012");
        assert_eq!(
            error.message,
            "cannot move 'text' out of a 'let rec' function, which borrows it; borrow it with 'ref' instead"
        );
    }
    // The closure holds the borrow, so it cannot leave the function that owns the string.
    rejects(
        "def make :: string -> (i64 -> i64) = \\text ->\n    let rec go = \\k -> if k == 0 then String.length (ref text) else go (k - 1)\n    go\n",
        "E1013",
    );
}

#[test]
fn local_let_rec_calls_itself_directly_and_loops_in_tail_position() {
    // The captures are numbers, so the lifted function takes them first and the tail call is a loop.
    let ir = accepts(
        "def triangle :: i64 -> i64 = \\limit ->\n    let rec go = \\n acc -> if n > limit then acc else go (n + 1) (acc + n)\n    go 1 0\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 1, "{ir}");
    let (symbol, body) = &functions[0];
    assert!(loops(body), "{body}");
    assert!(!body.contains(&format!("call i64 {symbol}(")), "{body}");
    assert!(!body.contains("%tz.closure"), "{body}");
    // `x |> go` is a direct call as well, and a loop when it ends the body.
    let ir = accepts(
        "def countdown :: i64 -> i64 = \\n ->\n    let rec go = \\k -> if k <= 0 then 10 else (k - 1) |> go\n    n |> go\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 1, "{ir}");
    assert!(loops(&functions[0].1), "{}", functions[0].1);
    // A borrowed capture is a reference parameter that the function passes on unchanged, which
    // points outside of its frame: the tail call is still a loop, and nothing allocates.
    let ir = accepts(
        "def total :: [i64] -> i64 = \\values ->\n    let rec go = \\i acc -> if i == values.length then acc else go (i + 1) (acc + values[i])\n    go 0 0\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 1, "{ir}");
    let (symbol, body) = &functions[0];
    assert!(
        loops(body) && body.contains("phi %tz.array [ %arg0, %entry ], [ "),
        "{body}"
    );
    assert!(!body.contains(&format!("call i64 {symbol}(")), "{body}");
    assert!(!body.contains("%tz.closure"), "{body}");
    // A string is passed on as a pointer, the same way.
    let ir = accepts(
        "def total :: string -> i64 = \\text ->\n    let rec go = \\i acc -> if i == String.length (ref text) then acc else go (i + 1) (acc + 1)\n    go 0 0\n",
    );
    let (symbol, body) = &lifted(&ir)[0];
    assert!(
        loops(body) && body.contains("phi ptr [ %arg0, %entry ], [ "),
        "{body}"
    );
    assert!(!body.contains(&format!("call i64 {symbol}(")), "{body}");
    // A reference that is a parameter of the function itself may point into the frame that the
    // jump reuses, as in a `def rec`, so the function calls itself.
    let ir = accepts(
        "def length :: string -> i64 = \\text ->\n    let rec go = \\view i -> if i == 0 then String.length view else go view (i - 1)\n    go (ref text) 3\n",
    );
    let (symbol, body) = &lifted(&ir)[0];
    assert!(body.contains(&format!("call i64 {symbol}(")), "{body}");
    assert!(!loops(body), "{body}");
    // Non-tail recursion is a direct call too.
    let ir = accepts(
        "def fib :: i64 -> i64 = \\n ->\n    let rec go = \\k -> if k < 2 then k else go (k - 1) + go (k - 2)\n    go n\n",
    );
    let functions = lifted(&ir);
    let (symbol, body) = &functions[0];
    assert_eq!(
        body.matches(&format!("call i64 {symbol}(")).count(),
        2,
        "{body}"
    );
}

#[test]
fn local_let_rec_values_escape_as_closures() {
    for source in [
        // Returned from the block, and from the function that made it.
        "def make :: i64 -> (i64 -> i64) = \\step ->\n    let rec go = \\n -> if n <= 0 then 0 else step + go (n - 1)\n    go\n",
        // Passed to a function.
        "def f :: [i64] -> [i64] = \\values ->\n    let rec sum = \\n -> if n <= 0 then 0 else n + sum (n - 1)\n    Array.map sum values\n",
        // Applied to one argument.
        "def partial :: i64 -> (i64 -> i64) = \\step ->\n    let rec add = \\x y -> if x == 0 then y else add (x - 1) (y + step)\n    add 3\n",
        // Passed to a function from its own body, which builds a new closure.
        "def f :: i64 -> [i64] = \\n ->\n    let rec spread = \\k -> if k <= 0 then [0] else Array.map spread [k - 1] |> Array.length |> (\\count -> [count as i64])\n    spread n\n",
        // The functions of a group, returned together.
        "def pair :: i64 -> (i64 -> bool) * (i64 -> bool) = \\base ->\n    let rec even = \\m -> if m == base then true else odd (m - 1)\n    and odd = \\m -> if m == base then false else even (m - 1)\n    (even, odd)\n",
        // A closure inside the body that mentions the function.
        "def f :: i64 -> i64 = \\n ->\n    let rec count = \\k -> if k <= 0 then 0 else (\\x -> x + 1) (count (k - 1))\n    count n\n",
    ] {
        accepts(source);
    }
}

#[test]
fn local_let_rec_groups_share_captures_and_call_each_other_directly() {
    let ir = accepts(
        "def parity :: i64 -> i64 -> bool = \\base n ->\n    let rec even = \\m -> if m == base then true else odd (m - 1)\n    and odd = \\m -> if m == base then false else even (m - 1)\n    even n\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 2, "{ir}");
    // Each member takes the captures of the group and calls the other one directly.
    for (index, (symbol, body)) in functions.iter().enumerate() {
        let other = &functions[1 - index].0;
        assert!(
            body.contains(&format!("call i1 {other}(")),
            "{symbol}\n{body}"
        );
        assert!(!body.contains("%tz.closure"), "{symbol}\n{body}");
    }
    // The members capture different values: the group takes the union, borrowed or not.
    accepts(
        "def f :: string -> [i64] -> i64 = \\text values ->\n    let rec a = \\k -> if k == 0 then String.length (ref text) else b (k - 1)\n    and b = \\k -> if k == 0 then values.length else a (k - 1)\n    a 3\n",
    );
    // Three functions, the last one used only through the others.
    accepts(
        "def f :: i64 -> i64 = \\n ->\n    let rec a = \\k -> if k == 0 then 0 else b (k - 1)\n    and b = \\k -> c k\n    and c = \\k -> a (k - 1) + 1\n    a n\n",
    );
}

#[test]
fn local_let_rec_functions_call_earlier_and_enclosing_ones_directly() {
    // A later `let rec` calls an earlier one without capturing it as a function value.
    let ir = accepts(
        "def f :: i64 -> i64 = \\n ->\n    let rec a = \\m -> if m == 0 then 0 else 1 + a (m - 1)\n    let rec b = \\m -> a m + 2\n    b n\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 2, "{ir}");
    assert!(
        functions[1]
            .1
            .contains(&format!("call i64 {}(", functions[0].0)),
        "{}",
        functions[1].1
    );
    // A `let rec` in the body of another one calls it, passing its captures on.
    let ir = accepts(
        "def g :: i64 -> i64 -> i64 = \\base n ->\n    let rec outer = \\k ->\n        let rec inner = \\j -> if j == 0 then outer (k - 1) else inner (j - 1)\n        if k == 0 then base else 1 + inner 1\n    outer n\n",
    );
    assert_eq!(lifted(&ir).len(), 2, "{ir}");
    // The earlier function's borrowed captures are borrowed again by the later one.
    accepts(
        "def h :: string -> i64 -> i64 = \\text n ->\n    let rec a = \\m -> if m == 0 then String.length (ref text) else a (m - 1)\n    let rec b = \\m -> a m + String.length (ref text)\n    b n\n",
    );
}

/// The captures of a `let rec` function are locals where its group is bound and parameters in the
/// bodies of the group, so a call that passes them on is right only there. The programs below
/// were found by a review: each one printed a wrong value, crashed, or made the compiler panic.
#[test]
fn local_let_rec_functions_are_called_directly_only_where_their_captures_exist() {
    // A group nested in the body of another one: the body calls it through its closure. Its
    // capture `s` is a parameter of the lifted outer function, not a borrowed capture of it, so
    // passing `s` on as a reference parameter passed the string where a reference was expected.
    // The same holds for a record, and for a function that is a local of the outer body.
    for source in [
        "def f :: i64 -> i64 = \\n ->\n    let rec outer = \\s ->\n        let rec inner = \\j -> if j == 0 then String.length (ref s) else inner (j - 1)\n        inner 3\n    outer (to_string n)\nlet five = f 12345\n",
        "record Labeled { value: i64, label: string }\ndef f :: i64 -> i64 = \\n ->\n    let rec outer = \\(item: Labeled) ->\n        let rec inner = \\j -> if j == 0 then item.value + String.length (ref item.label) else inner (j - 1)\n        inner 3\n    outer (Labeled { value: n, label: to_string n })\nlet shown = f 5\n",
        "def f :: i64 -> i64 = \\n ->\n    let rec outer = \\k ->\n        let held = \\x -> x + n\n        let rec inner = \\j -> held j\n        inner k\n    outer 1\nlet shown = f 5\n",
    ] {
        let ir = accepts(source);
        let functions = lifted(&ir);
        let (outer, inner) = (&functions[0], functions.last().unwrap());
        assert!(
            !outer.1.contains(&format!("call i64 {}(", inner.0)),
            "the outer function calls the inner one through its closure:\n{}",
            outer.1
        );
    }
    let ir = accepts(
        "def f :: i64 -> i64 = \\n ->\n    let rec outer = \\s ->\n        let rec inner = \\j -> if j == 0 then String.length (ref s) else inner (j - 1)\n        inner 3\n    outer (to_string n)\nlet five = f 12345\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 2, "{ir}");
    assert!(loops(&functions[1].1), "{}", functions[1].1);
    // A group in a closure or a continuation that mentions an earlier `let rec` function with
    // captures holds it as a function value: the lifted function has no such captures. The emitter
    // used to panic at a capture that its function does not have.
    for source in [
        "def f :: i64 -> i64 = \\n ->\n    let base = n * 2\n    let rec early = \\k -> k + base\n    let g = \\x ->\n        let rec inner = \\j -> if j == 0 then early x else inner (j - 1)\n        inner 2\n    g 3\nlet shown = f 12345\n",
        "def compute :: i64 -> Maybe<i64> = \\n ->\n    let scale = n * 2\n    let rec f1 = \\k -> if k == 0 then scale else f1 (k - 1)\n    let! start = Maybe.Some n\n    let rec f2 = \\k -> f1 k + 1\n    return f2 start\nlet shown = compute 45\nshown\n",
    ] {
        let functions = lifted(&accepts(source));
        assert!(
            !functions[1]
                .1
                .contains(&format!("call i64 {}(", functions[0].0)),
            "the later function calls the earlier one through its closure:\n{}",
            functions[1].1
        );
    }
    // The body of a loop is part of the function body, so a group there calls the earlier one directly.
    let ir = accepts(
        "def f :: i64 -> i64 = \\n ->\n    let text = to_string n\n    let rec early = \\k -> k + String.length (ref text)\n    let mut total = 0i64\n    for i in 0i64 .. 2i64 do\n        let rec later = \\k -> early k + i\n        total = total + later 1\n    total\nlet shown = f 12345\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 2, "{ir}");
    assert!(
        functions[1]
            .1
            .contains(&format!("call i64 {}(", functions[0].0)),
        "{}",
        functions[1].1
    );
    // A closure holds the value that a local had when the function was made. A later function
    // that took over the captures of an earlier one would pass the value that the local has now.
    let ir = accepts(
        "def f :: i64 -> i64 = \\n ->\n    let mut x = 1\n    let rec early = \\k -> k + x\n    x = 100\n    let rec later = \\k -> early k\n    later n\nlet shown = f 0\n",
    );
    let functions = lifted(&ir);
    assert_eq!(functions.len(), 2, "{ir}");
    assert!(
        !functions[1]
            .1
            .contains(&format!("call i64 {}(", functions[0].0)),
        "{}",
        functions[1].1
    );
    // Without the assignment it is a direct call.
    let ir = accepts(
        "def f :: i64 -> i64 = \\n ->\n    let x = 1\n    let rec early = \\k -> k + x\n    let rec later = \\k -> early k\n    later n\nlet shown = f 0\n",
    );
    let functions = lifted(&ir);
    assert!(
        functions[1]
            .1
            .contains(&format!("call i64 {}(", functions[0].0)),
        "{}",
        functions[1].1
    );
    // The name of an earlier `let rec` function is a function value, which says nothing about a
    // type variable: only a capture whose type has one decides by-value or by-reference.
    accepts(
        "def pick_after :: 'a -> 'a -> i64 -> 'a = \\x y n ->\n    let rec pick = \\(a: 'a) (b: 'a) (k: i64) -> if k == 0 then a else pick b a (k - 1)\n    let rec twice = \\(a: 'a) (b: 'a) -> pick a b 2\n    twice x y\nlet shown = pick_after 1 2 3\n",
    );
}

/// A `match` reads the place that it matches; only a binding that takes a non-Copy value out of it
/// moves it. A move out of a captured value is `E1012`, and the error is at the closure that moves.
#[test]
fn local_let_rec_borrows_what_a_match_only_reads() {
    for source in [
        "def f :: i64 -> i64 = \\n ->\n    let text = to_string n\n    let rec go = \\k ->\n        if k == 0 then\n            match text with\n            | \"12345\" -> 1\n            | _ -> 2\n        else go (k - 1)\n    go 3\nlet shown = f 12345\n",
        "def f :: i64 -> i64 = \\n ->\n    let found: Maybe<string> = Maybe.Some (to_string n)\n    let rec go = \\k ->\n        if k == 0 then\n            match found with\n            | Maybe.Some _ -> 1\n            | Maybe.None -> 2\n        else go (k - 1)\n    go 2\nlet shown = f 5\n",
        "def f :: i64 -> i64 = \\n ->\n    let found: Maybe<string> = Maybe.Some (to_string n)\n    let rec go = \\k ->\n        if k == 0 then\n            match ref found with\n            | Maybe.Some s -> String.length s\n            | Maybe.None -> 0\n        else go (k - 1)\n    go 2\nlet shown = f 5\n",
    ] {
        accepts(source);
    }
    let error = rejects(
        "def f :: i64 -> i64 = \\n ->\n    let found: Maybe<string> = Maybe.Some (to_string n)\n    let rec go = \\k ->\n        if k == 0 then\n            match found with\n            | Maybe.Some s -> String.length (ref s)\n            | Maybe.None -> 0\n        else go (k - 1)\n    go 2\nlet shown = f 5\n",
        "E1012",
    );
    assert_eq!(
        error.message,
        "cannot move 'found' out of a 'let rec' function, which borrows it; borrow it with 'ref' instead"
    );
    // A nested closure that moves a borrowed value is reported where the closure is written, not
    // at the declaration of the value.
    let source = "def f :: i64 -> i64 = \\n ->\n    let text = to_string n\n    let rec go = \\k ->\n        let g = \\x -> x + String.length (ref text)\n        if k == 0 then g 1 else go (k - 1)\n    go 3\nlet shown = f 12345\n";
    let error = rejects(source, "E1012");
    assert_eq!(error.span.start, source.find("\\x ->").unwrap());
}

#[test]
fn local_let_rec_in_computation_expressions() {
    for source in [
        // An explicit block, before and after a `let!`.
        "let sum = Maybe {\n    let rec total = \\k -> if k == 0 then 0 else k + total (k - 1)\n    let! base = Maybe.Some 6\n    return total base\n}\nsum\n",
        "let both = Maybe {\n    let! seed = Maybe.Some 9\n    let rec even = \\k -> if k == 0 then 0 else k + odd (k - 1)\n    and odd = \\k -> if k == 0 then 0 else k + even (k - 1)\n    return even seed\n}\nboth\n",
        // The builder is the expected result type.
        "def compute :: i64 -> Maybe<i64> = \\n ->\n    let rec fib = \\k -> if k < 2 then k else fib (k - 1) + fib (k - 2)\n    let! start = Maybe.Some n\n    return fib start\nlet shown = compute 6\nshown\n",
        // The builder is the first `let!` source, after the `let rec` that precedes it.
        "def main :: unit -> i32 = \\() ->\n    let rec sum = \\k -> if k == 0 then 0 else k + sum (k - 1)\n    let! _line = IO.read_line ()\n    do! IO.write_line (to_string (sum 10))\n    0\n",
        "let compute = \\n ->\n    let rec fib = \\k -> if k < 2 then k else fib (k - 1) + fib (k - 2)\n    let! start = Maybe.Some n\n    return fib start\ncompute 6\n",
    ] {
        let module = analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    }
    // `let! rec` stays a syntax error in a computation, as it is in a task block.
    let error = parse_error("let r = Maybe {\n    let! rec go = \\m -> m\n    return 1\n}\nr\n");
    assert_eq!(error.code, "E0002");
}

#[test]
fn unused_local_let_rec_warns() {
    fn unused(source: &str) -> Vec<String> {
        analyze(source)
            .unwrap()
            .warnings
            .iter()
            .filter(|warning| warning.code == "W1001")
            .map(|warning| source[warning.span.start..warning.span.end].to_owned())
            .collect()
    }
    // A function that only its own body mentions is unused, however often it calls itself.
    let warnings = analyze("def f :: i64 -> i64 = \\n ->\n    let rec helper = \\k -> if k == 0 then 0 else helper (k - 1)\n    n\n")
        .unwrap()
        .warnings;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].severity, Severity::Warning);
    assert_eq!(
        warnings[0].message,
        "unused local 'helper'; prefix it with '_' to silence this warning"
    );
    // Using it once, anywhere after the binding, is enough; so is the `_` prefix.
    assert!(unused("def f :: i64 -> i64 = \\n ->\n    let rec helper = \\k -> if k == 0 then 0 else helper (k - 1)\n    helper n\n").is_empty());
    assert!(unused("def f :: i64 -> i64 = \\n ->\n    let rec _helper = \\k -> if k == 0 then 0 else _helper (k - 1)\n    n\n").is_empty());
    // The functions of a group are used when a used one calls them; a group that nothing outside
    // mentions is unused as a whole.
    assert!(unused("def f :: i64 -> bool = \\n ->\n    let rec even = \\m -> if m == 0 then true else odd (m - 1)\n    and odd = \\m -> if m == 0 then false else even (m - 1)\n    even n\n").is_empty());
    assert_eq!(
        unused(
            "def f :: i64 -> i64 = \\n ->\n    let rec even = \\m -> if m == 0 then true else odd (m - 1)\n    and odd = \\m -> if m == 0 then false else even (m - 1)\n    n\n"
        ),
        ["even", "odd"]
    );
    assert_eq!(
        unused(
            "def f :: i64 -> bool = \\n ->\n    let rec live = \\m -> if m == 0 then true else live (m - 1)\n    and dead = \\m -> if m == 0 then false else dead (m - 1)\n    live n\n"
        ),
        ["dead"]
    );
}

/// A depth of 1,000,000 would overflow the stack of a native program at `-O0` if the tail call of
/// the lifted function were a call, whether the function captures numbers or a borrowed array.
#[test]
fn deep_local_tail_recursion_does_not_grow_the_stack() {
    let source = "def triangle :: i64 -> i64 = \\limit ->\n    let rec go = \\n acc -> if n > limit then acc else go (n + 1) (acc + n)\n    go 1 0\n\ndef walk :: i64 -> i64 = \\count ->\n    let values = Array.init count (i -> i)\n    let rec go = \\i acc -> if i == values.length then acc else go (i + 1) (acc + values[i])\n    go 0 0\n\ndef main :: unit -> i32 = \\() ->\n    do! IO.write_line (to_string (triangle 1000000))\n    do! IO.write_line (to_string (walk 1000000))\n    0\n";
    let ir = accepts(source);
    let functions = lifted(&ir);
    assert!(
        functions.len() >= 2 && functions.iter().filter(|(_, body)| loops(body)).count() >= 2,
        "{ir}"
    );
    let root = std::env::temp_dir().join(format!("tsuzuri-deep-let-rec-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(&root)
        .arg("-O0")
        .arg("--no-cache")
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8(run.stdout).unwrap(),
        "500000500000\n499999500000\n"
    );
}
