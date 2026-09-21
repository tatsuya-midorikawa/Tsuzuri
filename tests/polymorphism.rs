use tsuzuri::check::Type;
use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    module
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn accepts_the_requested_signatures_and_space_separated_arguments() {
    accepts("def add :: i32 -> i32 -> i32\nfn add x y =\n  x + y\nadd 20 22");
    let module = accepts(
        "def add :: 'a -> 'a -> 'a
         fn add x y =
           x + y
         export def integer :: i32 -> i32 -> i32
         fn integer x y = add x y
         export def floating :: f64 -> f64 -> f64
         fn floating x y = add x y
         def text :: string
         fn text = add \"hello\" \" world\"
         def main :: i32
         fn main = add 20 (22)",
    );
    let specializations: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("add.$mono."))
        .collect();
    assert_eq!(specializations.len(), 3);
    assert!(
        specializations
            .iter()
            .any(|f| f.signature.result == Type::Integer(32, true))
    );
    assert!(
        specializations
            .iter()
            .any(|f| f.signature.result == Type::F64)
    );
    assert!(
        specializations
            .iter()
            .any(|f| f.signature.result == Type::String)
    );
}

#[test]
fn supports_parametric_values_aggregates_borrows_and_higher_order_functions() {
    accepts(
        "record Box { value: i32 }
         def id :: 'a -> 'a
         fn id x = x
         def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         def first :: ['a] -> 'a
         fn first xs = xs[0]
         def borrow :: &'a -> &'a
         fn borrow x = x
         def choose :: bool -> ('a -> 'a)
         fn choose flag = if flag { id } else { id }
         def main :: i32
         fn main = {
           let f: i32 -> i32 = id;
           let g = choose true;
           let boxed = id (Box { value: 40i32 });
           let text = id \"owned\";
           let size = (borrow (&text)).length;
           apply id (f (g (first [boxed.value, 0i32]))) + (size as i32) - 3i32
         }",
    );
    accepts(
        "def id :: 'a -> 'a\nfn id x = x\ndef main :: i64\nfn main = { let text = \"x\"; (id (&text)).length }",
    );
    accepts(
        "def replace :: &mut 'a -> 'a -> unit\nfn replace p x = { *p = x; }\ndef main :: string\nfn main = { let mut x = \"old\"; replace (&mut x) \"new\"; x }",
    );
}

#[test]
fn infers_constraints_through_forward_calls_and_recursion() {
    accepts(
        "def twice :: 'a -> 'a
         fn twice x = plus x x
         def plus :: 'a -> 'a -> 'a
         fn plus x y = x + y
         def total :: i64 -> 'a -> 'a -> 'a
         fn total n x acc = if n == 0 { acc } else { total (n - 1) x (plus acc x) }
         def main :: i32
         fn main = total 10 (twice 2i32) 2",
    );
    rejects(
        "def twice :: 'a -> 'a\nfn twice x = plus x x\ndef plus :: 'a -> 'a -> 'a\nfn plus x y = x + y\ntwice true",
        "E1005",
    );
    rejects("def id :: 'a -> 'a\nfn id x = true", "E1003");
    rejects("def bad :: 'a -> 'b\nfn bad x = x", "E1003");
    rejects("def bad :: 'a -> 'a\nfn bad x = x.missing", "E1007");
    rejects("def bad :: 'a -> 'a\nfn bad x = missing", "E1002");
}

#[test]
fn supports_explicit_constraints_and_literal_specialization() {
    accepts(
        "def increment :: (Add 'a, Integer 'a) => 'a -> 'a
         fn increment x = x + 1
         def fraction :: 'a -> 'a
         fn fraction x = x + 0.1
         def negate :: 'a -> 'a
         fn negate x = -x
         def narrow :: i8
         fn narrow = increment 126
         def wide :: i128
         fn wide = increment 170141183460469231731687303715884105726
         def decimal :: d128
         fn decimal = fraction 0.2d128
         def main :: i32
         fn main = negate (increment (-43i32))",
    );
    rejects(
        "def huge :: 'a -> 'a\nfn huge x = x + 128\ndef main :: i8\nfn main = huge 0",
        "E1009",
    );
    rejects(
        "def number :: 'a -> 'a\nfn number x = x + 1\nnumber 1.0",
        "E1005",
    );
    rejects("def f :: Unknown 'a => 'a -> 'a\nfn f x = x", "E1016");
    rejects("def f :: Add 'b => 'a -> 'a\nfn f x = x", "E1015");
    accepts("def f :: Copy (fn() -> i64) => i32\nfn f = 42");
}

#[test]
fn supports_user_classes_instances_and_operator_instances() {
    accepts(
        "record Point { x: i32, y: i32 }
         class Measure 'a {
           def measure :: &'a -> i32
         }
         instance Measure Point {
           fn measure p = p.x + p.y
         }
         instance Measure i32 {
           fn measure x = *x
         }
         instance Add Point {
           fn add p q = Point { x: p.x + q.x, y: p.y + q.y }
         }
         def sum :: 'a -> 'a -> 'a
         fn sum x y = x + y
         def measure :: Measure 'a => &'a -> i32
         fn measure x = Measure.measure x
         def main :: i32
         fn main = {
           let p = sum (Point { x: 10, y: 20 }) (Point { x: 5, y: 7 });
           let f: &Point -> i32 = Measure.measure;
           f (&p)
         }",
    );
    accepts(
        "def f :: i32\nfn f = Add.add 20 22\ndef g :: f64\nfn g = { let op: f64 -> f64 -> f64 = Mul.mul; op 2.0 3.0 }",
    );
    rejects(
        "class C 'a { def f :: 'a -> i32 }\ninstance C bool {}\n",
        "E1016",
    );
    rejects(
        "class C 'a { def f :: 'a -> i32 }\ninstance C bool { fn f x = true }",
        "E1003",
    );
    rejects(
        "class C 'a { def f :: 'a -> i32 }\ninstance C bool { fn other x = 1 }",
        "E1016",
    );
    rejects("instance Add i32 { fn add x y = x - y }", "E1016");
    rejects("instance Copy string {}", "E1016");
    rejects(
        "class C 'a { def f :: 'a -> i32 }\ninstance C bool { fn f x = 1 }\ninstance C bool { fn f x = 2 }",
        "E1016",
    );
}

#[test]
fn resolves_polymorphism_across_modules_and_preserves_diagnostics() {
    let module = analyze_modules(&[
        ("Library", "class Size 'a { def size :: &'a -> i64 }\ndef id :: 'a -> 'a\nfn id x = x\ndef size :: &'a -> i64\nfn size x = Size.size x"),
        ("Main", "instance Library.Size string { fn size text = text.length }\nlet text = Library.id \"hello\"\nLibrary.size (&text)"),
    ]).unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
    let error = analyze_modules(&[
        ("Library", "def add :: 'a -> 'a -> 'a\nfn add x y = x + y"),
        ("Main", "Library.add true false"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1005");
    assert_eq!(error.span.source, Some(1));
    let module = analyze_modules(&[
        ("Add", "def add :: i32 -> i32 -> i32\nfn add x y = x - y"),
        ("Main", "def main :: i32\nfn main = Add.add 20 22"),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i32 @tz.fn.Add.add"));
}

#[test]
fn rejects_ambiguous_mismatched_and_unsafe_instantiations() {
    rejects("def id :: 'a -> 'a\nfn id x = x\nlet f = id", "E1015");
    rejects(
        "def add :: 'a -> 'a -> 'a\nfn add x y = x + y\nadd 1i32 2i64",
        "E1003",
    );
    rejects("def add :: i32 -> i32 -> i32\nfn add x = x", "E1003");
    rejects("fn add x y = x + y", "E0002");
    rejects("def add :: i32 -> i32 -> i32", "E0002");
    rejects("export def id :: 'a -> 'a\nfn id x = x", "E1008");
    rejects("def main :: 'a\nfn main = main()", "E1015");
    rejects(
        "def duplicate :: 'a -> ['a]\nfn duplicate x = [x, x]\nduplicate \"owned\"",
        "E1005",
    );
    rejects(
        "def first :: ['a] -> 'a\nfn first x = x[0]\nfirst [\"a\", \"b\"]",
        "E1005",
    );
    rejects(
        "def id :: 'a -> 'a\nfn id x = x\ndef main :: &string\nfn main = { let x = \"x\"; id (&x) }",
        "E1013",
    );
}

#[test]
fn bounds_type_growing_polymorphic_recursion() {
    rejects("def f :: 'a -> unit\nfn f x = f [x]\nf 1", "E1017");
    rejects(
        "def f :: 'a -> unit\nfn f x = { let y = x + x; f [y] }",
        "E1017",
    );
    rejects(
        &format!(
            "def f :: i32\nfn f = {}0{}",
            "f (".repeat(200),
            ")".repeat(200)
        ),
        "E0002",
    );
    rejects(
        &format!(
            "def f :: {}i32{}\nfn f = 0",
            "(".repeat(200),
            ")".repeat(200)
        ),
        "E0002",
    );
}

#[test]
fn honors_the_exact_specialization_limit() {
    use std::fmt::Write;
    let mut source = String::from("def id :: 'a -> 'a\nfn id x = x\n");
    for index in 0..1024 {
        writeln!(
            source,
            "record R{index} {{ value: i8 }}\nfn f{index}(x: R{index}) -> R{index} {{ id x }}"
        )
        .unwrap();
    }
    assert!(analyze(&source).is_ok());
    source.push_str("fn extra(x: [i8]) -> [i8] { id x }");
    rejects(&source, "E1017");
}

#[test]
fn honors_the_constraint_limit_and_deduplicates_repeated_requirements() {
    use std::fmt::Write;
    let mut declarations = String::new();
    for index in 0..129 {
        writeln!(declarations, "class C{index} 'a {{ def f :: 'a -> i64 }}").unwrap();
    }
    for (count, accepted) in [(128, true), (129, false)] {
        let constraints = (0..count)
            .map(|index| format!("C{index} 'a"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!("{declarations}def f :: ({constraints}) => 'a -> 'a\nfn f x = x");
        if accepted {
            accepts(&source);
        } else {
            rejects(&source, "E1017");
        }
    }
    let constraints = vec!["Copy 'a"; 200].join(", ");
    accepts(&format!("def f :: ({constraints}) => 'a -> 'a\nfn f x = x"));
}

#[test]
fn checks_declarations_and_specialized_layouts_even_when_unused() {
    rejects("def f :: i32 -> i32\nfn f x = x\nfn f x = x", "E1001");
    rejects("def f :: i32\ndef f :: i32\nfn f = 1", "E1001");
    rejects("class C 'a { def f :: 'b -> 'b }", "E1016");
    accepts("class C 'a { def f :: 'a -> [[i64]] }");
    rejects(
        "class C 'a { def f :: 'a -> i64 }\ninstance C 'a { fn f x = 1 }",
        "E1015",
    );
    rejects(
        "class C 'a { def f :: 'a -> i64 }\ninstance C byte { fn f x = 1 }\ninstance C i8 { fn f x = 2 }",
        "E1016",
    );
    accepts(
        "record Big { values: [[i64]] }
         def pair :: 'a -> ['a]
         fn pair x = [x, x]
         def f :: Big -> unit
         fn f x = { let _ = pair x; }",
    );
}

#[test]
fn distinguishes_multi_argument_application_from_returned_function_application() {
    accepts(
        "record R { x: i32 }
         def id :: 'a -> 'a
         fn id x = x
         def add :: i32 -> i32 -> i32
         fn add x y = x + y
         def choose :: bool -> (i32 -> i32)
         fn choose flag = id
         def main :: i32
         fn main = add ((choose true) (R { x: 20 }).x) 22",
    );
    accepts(
        "def id :: i32 -> i32\nfn id x = x\ndef choose :: bool -> (i32 -> i32)\nfn choose flag = id\n(choose true) 42",
    );
    accepts("fn f(x: i64, y: i64) -> i64 { x + y }\nf (20, 22)");
    accepts("def unit :: unit -> i32\nfn unit x = 42\nunit ()");
    accepts("def add :: i32 -> i32 -> i32\nfn add x y = x + y\nadd 1");
}

#[test]
fn infers_copy_only_when_required_by_ownership() {
    for source in [
        "def choose :: bool -> 'a -> 'a\nfn choose flag x = if flag { x } else { x }\nchoose true \"owned\"",
        "def first :: 'a -> 'b -> 'a\nfn first x y = x\nfirst \"owned\" \"dropped\"",
        "def loop :: i64 -> 'a -> 'a\nfn loop n x = if n == 0 { x } else { loop (n - 1) x }\nloop 10 \"owned\"",
        "def replace :: 'a -> 'a -> 'a\nfn replace mut x y = { let old = x; x = y; x }\nreplace \"old\" \"new\"",
        "def dup :: 'a -> ['a]\nfn dup x = [x, x]\ndup 2i32",
        "def read :: &'a -> 'a\nfn read x = *x\ndef main :: i64\nfn main = { let x = 42; read (&x) }",
    ] {
        accepts(source);
    }
    rejects(
        "def read :: &'a -> 'a\nfn read x = *x\ndef main :: string\nfn main = { let x = \"x\"; read (&x) }",
        "E1005",
    );
    rejects(
        "def f :: Copy 'a => 'a -> 'a\nfn f x = x\nf [\"owned\"]",
        "E1005",
    );
    rejects(
        "def f :: 'a -> 'a\nfn f x = { let s = \"x\"; let t = s; let u = s; x }",
        "E1012",
    );
}

#[test]
fn specializes_operators_for_every_numeric_representation() {
    for ty in [
        "i8", "i16", "i32", "i64", "i128", "i8u", "i16u", "i32u", "i64u", "i128u", "f16", "f32",
        "f64", "f128", "d32", "d64", "d128",
    ] {
        accepts(&format!(
            "def arithmetic :: 'a -> 'a -> 'a
             fn arithmetic x y = (x + y) * x - x / y
             def compare :: 'a -> 'a -> bool
             fn compare x y = x == y || x != y && x < y || x <= y || x > y || x >= y
             def f :: {ty} -> {ty} -> {ty}
             fn f x y = arithmetic x y
             def g :: {ty} -> {ty} -> bool
             fn g x y = compare x y"
        ));
    }
    accepts(
        "def bits :: 'a -> 'a -> 'a\nfn bits x y = (~x & y | x ^ y) << y >> y >>> y % x\ndef main :: i32\nfn main = bits 42 2",
    );
    accepts("instance Add [i32] { fn add x y = x }\ndef main :: [i32]\nfn main = [] + []");
}
