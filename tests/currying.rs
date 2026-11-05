use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let first = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(first, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn accepts_all_requested_declarations_and_implementations() {
    for implementation in ["fn add x y = x + y", "let add = x -> y -> x + y"] {
        for signature in ["i32 -> i32 -> i32", "Add<'a> -> 'a -> 'a"] {
            accepts(&format!(
                "def add :: {signature}\n{implementation}\ndef answer :: i32\nfn answer = {{ let plus20 = add 20; plus20 22 }}"
            ));
        }
    }
    accepts(
        "def add :: Add<'a> -> 'a -> 'a\nlet add = x -> y -> x + y\ndef i :: i32\nfn i = add 20 22\ndef f :: f64\nfn f = add 1.5 2.5",
    );
    rejects("fn add :: i32 -> i32 -> i32\nfn add x y = x + y", "E0002");
}

#[test]
fn supports_nested_lambdas_and_equivalent_arrow_associations() {
    accepts(
        "def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         def answer :: i32
         fn answer = {
           let add: i32 -> (i32 -> i32) = x -> y -> x + y;
           let increment = add 1;
           let identity = x -> x;
           apply identity (apply increment 41)
         }",
    );
    accepts(
        "def make :: i32 -> i32 -> i32\nfn make x = { let offset: i32 = 1; y -> x + y + offset }\n(make 20) 21",
    );
    accepts("def f :: i32 -> i32 -> i32\nfn f x = y -> x + y\nf 20 22");
    accepts(
        "export def choose :: bool -> i32 -> i32\nfn choose flag = if flag { x -> x + 1 } else { x -> x - 1 }",
    );
}

#[test]
fn supports_owned_snapshots_and_function_aggregates() {
    accepts(
        "record Callback { call: i32 -> i32 }
         def answer :: i32
         fn answer = {
           let offset: i32 = 20;
           let f = x -> x + offset;
           let callback = Callback { call: f };
           let array = [f, callback.call];
           let copy = array;
           copy[0] 1 + array[1] 1
         }",
    );
    accepts(
        "def answer :: string\nfn answer = { let prefix = \"hello\"; let f: string -> string = suffix -> prefix + suffix; f \"a\" + f \"b\" }",
    );
    accepts(
        "def add :: Add<'a> -> 'a -> 'a\nfn add x y = x + y\ndef answer :: string\nfn answer = { let f = add \"prefix\"; f \"a\" + f \"b\" }",
    );
}

#[test]
fn checks_capture_lifetimes_moves_and_mutability() {
    accepts(
        "def answer :: i64\nfn answer = { let x = \"hello\"; let r = &x; let f: i64 -> i64 = n -> r.length + n; f 1 + f 2 }",
    );
    rejects(
        "def make :: fn() -> (i64 -> i64)\nfn make = { let x = \"hello\"; let r = &x; n -> r.length + n }",
        "E1013",
    );
    rejects(
        "def answer :: string\nfn answer = { let x = \"hello\"; let f: string -> string = y -> x + y; x }",
        "E1012",
    );
    rejects(
        "def answer :: i64\nfn answer = { let mut x = 1; let f: i64 -> i64 = y -> { x = x + y; x }; f 1 }",
        "E1014",
    );
    rejects(
        "def answer :: i64\nfn answer = { let mut x = 1; let r = &mut x; let f: i64 -> i64 = y -> *r + y; f 1 }",
        "E1005",
    );
    rejects(
        "def answer :: i64\nfn answer = { let mut x = 1; let r = &x; let f: i64 -> i64 = y -> *r + y; x = 2; f 0 }",
        "E1014",
    );
    rejects(
        "def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         def choose :: &mut i64 -> unit -> &mut i64
         fn choose r u = r
         def answer :: i64
         fn answer = { let mut x = 1; let f = apply choose (&mut x); let a = f (); *a }",
        "E1005",
    );
    rejects(
        "def keep :: 'a -> i64 -> 'a
         fn keep x = { n -> x }
         def make :: fn() -> (i64 -> &string)
         fn make = { let x = \"hello\"; keep (&x) }",
        "E1013",
    );
    rejects(
        "def answer :: unit
         fn answer = {
           let mut x = 1;
           let f: &i64 -> &i64 = r -> r;
           let r = f (&mut x);
           *r = 2;
         }",
        "E1003",
    );
    rejects(
        "def answer :: unit
         fn answer = { let x = 1; let f: &i64 -> unit = r -> { *r = 2; }; f (&x) }",
        "E1014",
    );
    rejects(
        "def make :: &'a -> (unit -> &'a)
         fn make r = { u -> r }
         def answer :: unit -> &string
         fn answer u = { let x = \"x\"; let f = make (&x); f () }",
        "E1013",
    );
}

fn rejects_with_kind(source: &str, code: &str, kind: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
}

/// A28: a call's result holds the borrows of the arguments that the callee's body can put in its
/// result. `pass` and `keep` have one signature; only `keep` puts its reference in its result.
#[test]
fn function_results_retain_only_the_arguments_that_reach_them() {
    const PASS: &str =
        "def pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn pass _key f = f\n";
    const PASS_GENERIC: &str = "def pass :: ref 'k -> own 'v -> 'v\nfn pass _key f = f\n";
    const WRAP: &str = "def wrap :: i64 -> (unit -> i64) -> (unit -> i64)\nfn wrap key f = pass (ref key) f\nlet g = wrap 1 (\\() -> 42)\ng ()";
    for source in [
        format!("{PASS}{WRAP}"),
        format!("{PASS_GENERIC}{WRAP}"),
        format!(
            "{PASS_GENERIC}def wrap :: i64 -> i64 -> i64\nfn wrap key value = pass (ref key) value\nwrap 1 42"
        ),
        // The fixed point of a recursive function and of two mutually recursive ones.
        "def rec pass_n :: i64 -> ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn rec pass_n n key f = if n == 0 then f else pass_n (n - 1) key f\ndef wrap :: i64 -> (unit -> i64) -> (unit -> i64)\nfn wrap key f = pass_n 3 (ref key) f\nlet g = wrap 1 (\\() -> 42)\ng ()".to_owned(),
        "def rec ping :: i64 -> ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn rec ping n key f = if n == 0 then f else pong (n - 1) key f\ndef rec pong :: i64 -> ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn rec pong n key f = ping n key f\ndef wrap :: i64 -> (unit -> i64) -> (unit -> i64)\nfn wrap key f = ping 3 (ref key) f\nlet g = wrap 1 (\\() -> 42)\ng ()".to_owned(),
        // The text decides which function comes back; neither holds it.
        "def pick :: ref string -> own (unit -> i64) -> own (unit -> i64) -> (unit -> i64)\nfn pick text f g = if String.length text > 2 then f else g\ndef choose :: i64 -> (unit -> i64)\nfn choose n =\n    let text = to_string n\n    pick (ref text) (\\() -> 1) (\\() -> 2)\nlet g = choose 100\ng ()".to_owned(),
    ] {
        accepts(&source);
    }
    const KEEP: &str =
        "def keep :: ref string -> (unit -> i64)\nfn keep text = \\() -> String.length text\n";
    for (source, code, kind) in [
        (
            format!("{KEEP}def leak :: own string -> (unit -> i64)\nfn leak text = keep (ref text)\n0"),
            "E1013",
            "task_result_retains_borrow",
        ),
        (
            format!("{KEEP}let text = \"abc\"\nlet f = keep (ref text)\ntzroutine {{\n    let _n = f ()\n}}\n0"),
            "E1013",
            "task_capture_retains_borrow",
        ),
        (
            "def make :: unit -> [unit -> i64]\nfn make _unit =\n    let texts = [\"a\", \"bc\"]\n    Array.mapr (\\text -> \\() -> String.length text) (ref texts)\nlet fs = make ()\nArray.length (ref fs)".to_owned(),
            "E1013",
            "borrow_outlives_block",
        ),
        // The fixed point raises a recursive function to retaining when one path captures.
        (
            "def rec keep_n :: i64 -> ref string -> (unit -> i64)\nfn rec keep_n n text = if n == 0 then \\() -> String.length text else keep_n (n - 1) text\ndef leak :: own string -> (unit -> i64)\nfn leak text = keep_n 3 (ref text)\n0".to_owned(),
            "E1013",
            "task_result_retains_borrow",
        ),
        (
            "def rec ping :: i64 -> ref string -> (unit -> i64)\nfn rec ping n text = if n == 0 then \\() -> 0 else pong (n - 1) text\ndef rec pong :: i64 -> ref string -> (unit -> i64)\nfn rec pong n text = if n == 0 then \\() -> String.length text else ping n text\ndef leak :: own string -> (unit -> i64)\nfn leak text = ping 3 (ref text)\n0".to_owned(),
            "E1013",
            "task_result_retains_borrow",
        ),
        // A callback that receives the reference may put it in what it returns.
        (
            "def apply_key :: ref string -> own (ref string -> unit -> i64) -> (unit -> i64)\nfn apply_key text f = f text\ndef leak :: own string -> (unit -> i64)\nfn leak text = apply_key (ref text) (\\t -> \\() -> String.length t)\n0".to_owned(),
            "E1013",
            "task_result_retains_borrow",
        ),
    ] {
        rejects_with_kind(&source, code, kind);
    }
}

/// A28: the facts are computed for each specialization; one instance of a generic function may
/// put its reference in its result through a class method while another does not.
#[test]
fn function_result_facts_follow_each_specialization() {
    const TRAITS: &str = "class Make<'value> {\n    def make :: ref string -> 'value\n}\n";
    const MAIN: &str = "record Holder { run: unit -> i64 }\ninstance Traits.Make<Holder> {\n    fn make text = Holder { run: \\() -> String.length text }\n}\nrecord Counter { run: unit -> i64 }\ninstance Traits.Make<Counter> {\n    fn make text =\n        let length = String.length text\n        Counter { run: \\() -> length }\n}\ndef pick :: ref string -> own HashMap<i64, 'v> -> HashMap<i64, 'v>\n  @'v : Traits.Make\nfn pick text map = HashMap.insert 1 (Traits.Make.make text) map\n";
    let accepted = format!(
        "{MAIN}def count :: own string -> HashMap<i64, Counter>\nfn count text = pick (ref text) (HashMap.empty())\nlet map = count \"abc\"\nHashMap.length (ref map)"
    );
    analyze_modules(&[("Main.tz", &accepted), ("Traits.tt", TRAITS)])
        .unwrap_or_else(|error| panic!("{accepted}\n{}: {}", error.code, error.message));
    for (tail, kind) in [
        (
            "def leak :: own string -> HashMap<i64, Holder>\nfn leak text = pick (ref text) (HashMap.empty())\nlet map = leak \"abc\"\nHashMap.length (ref map)",
            "task_result_retains_borrow",
        ),
        (
            "def leak :: i64 -> HashMap<i64, Holder>\nfn leak n =\n    let text = to_string n\n    pick (ref text) (HashMap.empty())\nlet map = leak 1\nHashMap.length (ref map)",
            "borrow_outlives_block",
        ),
    ] {
        let source = format!("{MAIN}{tail}");
        let error =
            analyze_modules(&[("Main.tz", &source), ("Traits.tt", TRAITS)]).expect_err(&source);
        assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
        assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    }
}

#[test]
fn borrows_dynamic_calls_without_changing_owned_capture_fallbacks() {
    for (case, captures) in [
        "let offset = seed; let transform: i64 -> i64 = if flag then \\value -> value + offset else \\value -> value - offset; transform seed",
        "let text = to_string seed; let transform: i64 -> i64 = if flag then \\value -> text.length + value else \\value -> text.length - value; transform seed",
        "let text = to_string seed; let transform: string -> string = if flag then \\value -> text + value else \\value -> value + text; (transform \"a\" + transform \"b\").length",
    ].into_iter().enumerate() {
        let module = analyze(&format!(
            "export def invoke :: bool -> i64 -> i64\nfn invoke flag seed = {{ {captures} }}"
        ))
        .unwrap();
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert!(ir.contains("i1 true)"));
            assert_eq!(ir.contains("br i1 %borrow"), case != 0 || wasm || usize::BITS < 64);
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
}

#[test]
fn rejects_borrowed_results_from_captured_owners_and_tracks_aggregate_loans() {
    rejects(
        "def make :: fn() -> (unit -> &string)\nfn make = { let x = \"x\"; u -> &x }",
        "E1013",
    );
    rejects(
        "record Callback { run: unit -> i64 }
         def make :: fn() -> Callback
         fn make = {
           let owner = \"x\";
           let r = &owner;
           Callback { run: u -> r.length }
         }",
        "E1013",
    );
    rejects(
        "def answer :: i64
         fn answer = {
           let mut owner = \"x\";
           let r = &owner;
           let fs: [unit -> i64] = [u -> r.length];
           let copy = fs;
           owner = \"new\";
           copy[0] ()
         }",
        "E1014",
    );
}

#[test]
fn rejects_invalid_definitions_and_bounds_lambda_nesting() {
    rejects("def f :: i32 -> i32\nlet f = 1", "E0002");
    rejects("def f :: i32 -> i32\nlet f = x -> x\nfn f x = x", "E1001");
    rejects("def f :: i32 -> i32 -> i32\nlet f = x -> x -> x", "E1001");
    rejects("def f :: Missing<'a> -> 'a\nfn f x = x", "E1004");
    rejects("def f :: Copy<'a> -> 'a\nfn f x = x\nf \"owned\"", "E1005");
    rejects("def f :: i32 -> i32\nfn f x = x\nf 1 2", "E1006");
    rejects(
        &format!(
            "def answer :: i32\nfn answer = {{ let f = {}1; 1 }}",
            "x -> ".repeat(200)
        ),
        "E0002",
    );
}

#[test]
fn curries_legacy_functions_and_checks_inline_constraints() {
    accepts("fn add(x: i32, y: i32) -> i32 { x + y }\nlet f = add 20\nf 22");
    accepts("def add :: Add<'a> -> 'a -> 'a\nfn add x y = x + y\nlet add = add 20i32\nadd 22");
    accepts(
        "def convert :: i32 -> bool -> i32 -> i32\nlet convert = x -> flag -> y -> if flag { x + y } else { x - y }\nlet next = convert 20 true\nnext 22",
    );
    rejects("def f :: Add<'a> -> 'a\nfn f x = x\nf true", "E1005");
    rejects("class C<'a> { fn f :: 'a -> i32 }", "E0002");
}

#[test]
fn curries_builtins_methods_module_functions_and_pipelines() {
    accepts("def answer :: i32\nfn answer = { let add = Add.add 20; 22 |> add }");
    accepts("def answer :: f64\nfn answer = { let f: f64 -> f64 = sqrt; 9.0 |> f }");
    accepts(
        "class Combine<'a> { def combine :: 'a -> 'a -> 'a }\ninstance Combine<i32> { fn combine x = y -> x + y }\n(Combine.combine 20i32) 22",
    );
    let module = analyze_modules(&[
        (
            "Arith",
            "def add :: Add<'a> -> 'a -> 'a\nlet add = x -> y -> x + y",
        ),
        ("Main", "let f = Arith.add 20i32\nf 22"),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}
