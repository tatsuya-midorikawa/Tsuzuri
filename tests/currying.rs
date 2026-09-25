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
        for signature in ["i32 -> i32 -> i32", "Add 'a -> 'a -> 'a"] {
            accepts(&format!(
                "def add :: {signature}\n{implementation}\ndef main :: i32\nfn main = {{ let plus20 = add 20; plus20 22 }}"
            ));
        }
    }
    accepts(
        "def add :: Add 'a -> 'a -> 'a\nlet add = x -> y -> x + y\ndef i :: i32\nfn i = add 20 22\ndef f :: f64\nfn f = add 1.5 2.5",
    );
    rejects("fn add :: i32 -> i32 -> i32\nfn add x y = x + y", "E0002");
}

#[test]
fn supports_nested_lambdas_and_equivalent_arrow_associations() {
    accepts(
        "def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         def main :: i32
         fn main = {
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
         def main :: i32
         fn main = {
           let offset: i32 = 20;
           let f = x -> x + offset;
           let callback = Callback { call: f };
           let array = [f, callback.call];
           let copy = array;
           copy[0] 1 + array[1] 1
         }",
    );
    accepts(
        "def main :: string\nfn main = { let prefix = \"hello\"; let f: string -> string = suffix -> prefix + suffix; f \"a\" + f \"b\" }",
    );
    accepts(
        "def add :: Add 'a -> 'a -> 'a\nfn add x y = x + y\ndef main :: string\nfn main = { let f = add \"prefix\"; f \"a\" + f \"b\" }",
    );
}

#[test]
fn checks_capture_lifetimes_moves_and_mutability() {
    accepts(
        "def main :: i64\nfn main = { let x = \"hello\"; let r = &x; let f: i64 -> i64 = n -> r.length + n; f 1 + f 2 }",
    );
    rejects(
        "def make :: fn() -> (i64 -> i64)\nfn make = { let x = \"hello\"; let r = &x; n -> r.length + n }",
        "E1013",
    );
    rejects(
        "def main :: string\nfn main = { let x = \"hello\"; let f: string -> string = y -> x + y; x }",
        "E1012",
    );
    rejects(
        "def main :: i64\nfn main = { let mut x = 1; let f: i64 -> i64 = y -> { x = x + y; x }; f 1 }",
        "E1014",
    );
    rejects(
        "def main :: i64\nfn main = { let mut x = 1; let r = &mut x; let f: i64 -> i64 = y -> *r + y; f 1 }",
        "E1005",
    );
    rejects(
        "def main :: i64\nfn main = { let mut x = 1; let r = &x; let f: i64 -> i64 = y -> *r + y; x = 2; f 0 }",
        "E1014",
    );
    rejects(
        "def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         def choose :: &mut i64 -> unit -> &mut i64
         fn choose r u = r
         def main :: i64
         fn main = { let mut x = 1; let f = apply choose (&mut x); let a = f (); *a }",
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
        "def main :: unit
         fn main = {
           let mut x = 1;
           let f: &i64 -> &i64 = r -> r;
           let r = f (&mut x);
           *r = 2;
         }",
        "E1003",
    );
    rejects(
        "def main :: unit
         fn main = { let x = 1; let f: &i64 -> unit = r -> { *r = 2; }; f (&x) }",
        "E1014",
    );
    rejects(
        "def make :: &'a -> (unit -> &'a)
         fn make r = { u -> r }
         def main :: unit -> &string
         fn main u = { let x = \"x\"; let f = make (&x); f () }",
        "E1013",
    );
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
        "def main :: i64
         fn main = {
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
    rejects("def f :: Missing 'a -> 'a\nfn f x = x", "E1004");
    rejects("def f :: Copy 'a -> 'a\nfn f x = x\nf \"owned\"", "E1005");
    rejects("def f :: i32 -> i32\nfn f x = x\nf 1 2", "E1006");
    rejects(
        &format!(
            "def main :: i32\nfn main = {{ let f = {}1; 1 }}",
            "x -> ".repeat(200)
        ),
        "E0002",
    );
}

#[test]
fn curries_legacy_functions_and_checks_inline_constraints() {
    accepts("fn add(x: i32, y: i32) -> i32 { x + y }\nlet f = add 20\nf 22");
    accepts("def add :: Add 'a -> 'a -> 'a\nfn add x y = x + y\nlet add = add 20i32\nadd 22");
    accepts(
        "def convert :: i32 -> bool -> i32 -> i32\nlet convert = x -> flag -> y -> if flag { x + y } else { x - y }\nlet next = convert 20 true\nnext 22",
    );
    rejects("def f :: Add 'a -> 'a\nfn f x = x\nf true", "E1005");
    rejects("class C 'a { fn f :: 'a -> i32 }", "E0002");
}

#[test]
fn curries_builtins_methods_module_functions_and_pipelines() {
    accepts("def main :: i32\nfn main = { let add = Add.add 20; 22 |> add }");
    accepts("def main :: f64\nfn main = { let f: f64 -> f64 = sqrt; 9.0 |> f }");
    accepts(
        "class Combine 'a { def combine :: 'a -> 'a -> 'a }\ninstance Combine i32 { fn combine x = y -> x + y }\n(Combine.combine 20i32) 22",
    );
    let module = analyze_modules(&[
        (
            "Arith",
            "def add :: Add 'a -> 'a -> 'a\nlet add = x -> y -> x + y",
        ),
        ("Main", "let f = Arith.add 20i32\nf 22"),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}
