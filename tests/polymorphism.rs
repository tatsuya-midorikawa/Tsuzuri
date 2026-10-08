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
fn parses_typeclass_contexts_and_default_methods() {
    let source = "class (Eq<'a>, Ord<'a>) => Total<'a> { def same :: ref 'a -> ref 'a -> bool; fn same left right = Eq.eq left right }\nrecord Box<'a> { value: 'a }\ninstance Eq<'a> => Eq<Box<'a>> { fn eq left right = left.value == right.value; let ne = left -> right -> !(Eq.eq left right) }";
    let program = tsuzuri::parser::parse(source).unwrap();
    assert_eq!(program.classes[0].superclasses.len(), 2);
    assert_eq!(program.classes[0].methods.len(), 1);
    assert_eq!(program.classes[0].defaults.len(), 1);
    assert_eq!(program.classes[0].defaults[0].name.text, "same");
    assert_eq!(program.instances[0].constraints.len(), 1);
    assert_eq!(program.instances[0].methods.len(), 2);
}

#[test]
fn borrowed_comparison_method_signatures() {
    for (method, ty) in [
        ("Eq.eq", "i64"),
        ("Eq.ne", "string"),
        ("Ord.lt", "f64"),
        ("Ord.ge", "string"),
    ] {
        accepts(&format!(
            "def compare :: &{ty} -> &{ty} -> bool\nfn compare left right = {{ let method: &{ty} -> &{ty} -> bool = {method}; method left right }}"
        ));
    }
    accepts(
        "def compare :: Eq<'a> => &'a -> &'a -> bool\nfn compare left right = Eq.eq left right\nlet left = 42\nlet right = 42\ncompare (ref left) (ref right)",
    );
    accepts(
        "record Named { text: string }\ninstance Eq<Named> { fn eq left right = left.text == right.text; fn ne left right = !(Eq.eq left right) }\nlet left = Named { text: \"text\" }\nlet right = Named { text: \"text\" }\nEq.eq (ref left) (ref right)",
    );
    rejects("let method: string -> string -> bool = Eq.eq\n()", "E1003");
    rejects("let method: i64 -> i64 -> bool = Ord.lt\n()", "E1003");
    accepts("let method: i64 -> i64 -> i64 = Add.add\nmethod 20 22");
}

#[test]
fn superclasses_are_resolved_and_required() {
    accepts(
        "class Eq<'a> => Total<'a> { def same :: ref 'a -> ref 'a -> bool }\ninstance Total<i64> { fn same left right = Eq.eq left right }\nlet number = 42l\nTotal.same (ref number) (ref number)",
    );
    accepts(
        "class Later<'a> => Earlier<'a> { def first :: 'a -> i64 }\nclass Later<'a> { def last :: 'a -> i64 }\ninstance Earlier<bool> { fn first _value = 1 }\ninstance Later<bool> { fn last _value = 2 }\nEarlier.first true",
    );
    for source in [
        "class Missing<'a> => C<'a> { def value :: 'a -> i64 }",
        "class Eq<'b> => C<'a> { def value :: 'a -> i64 }",
        "class B<'a> => A<'a> { def first :: 'a -> i64 }\nclass A<'a> => B<'a> { def second :: 'a -> i64 }",
        "record Point { value: i64 }\ninstance Ord<Point> { fn lt left right = left.value < right.value; fn le left right = left.value <= right.value; fn gt left right = left.value > right.value; fn ge left right = left.value >= right.value }",
    ] {
        rejects(source, "E1027");
    }
}

#[test]
fn generic_class_dispatch_keeps_explicit_recursion() {
    for recursive in [false, true] {
        let marker = if recursive { "rec " } else { "" };
        let source = format!(
            "class G<'a> {{ def value :: 'a -> i64 }}\ndef {marker}forward :: G<'a> => 'a -> i64\nfn {marker}forward value = G.value value\ninstance G<i64> {{ fn {marker}value value = forward value }}"
        );
        let result = analyze(&source);
        if recursive {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert_eq!(result.as_ref().err().map(|error| error.code), Some("E1019"));
        }
    }
}

#[test]
fn conditional_instances_normalize_and_specialize_method_arguments() {
    let context = "record Box<'a> { value: 'a }\ninstance Eq<'a> => Eq<Box<'a>> { fn eq left right = left.value == right.value; fn ne left right = !(Eq.eq left right) }\n";
    for source in [
        "let left = Box { value: 42 }\nlet right = Box { value: 42 }\nleft == right",
        "def same :: ref Box<'a> -> ref Box<'a> -> bool\nfn same left right = Eq.eq left right\nlet left = Box { value: \"abc\" }\nlet right = Box { value: \"abc\" }\nsame (ref left) (ref right)",
        "let left = Box { value: 42 }\nlet same: ref Box<i64> -> ref Box<i64> -> bool = Eq.eq\nsame (ref left) (ref left)",
    ] {
        accepts(&format!("{context}{source}"));
    }
    rejects(
        &format!(
            "{context}instance Eq<Box<i64>> {{ fn eq _left _right = true; fn ne _left _right = false }}"
        ),
        "E1016",
    );
    rejects(
        "record Box<'a> { value: 'a }\ninstance Eq<'b> => Eq<Box<'a>> { fn eq _left _right = true; fn ne _left _right = false }",
        "E1027",
    );
    rejects(
        &format!(
            "{context}record NoEquality {{ number: i64 }}\nlet left = Box {{ value: NoEquality {{ number: 1 }} }}\nleft == left"
        ),
        "E1005",
    );
    rejects(
        "class C<'a> { def value :: 'a -> i64 }\ninstance C<'a> => C<'a> { fn value _value = 0 }\nC.value 1",
        "E1017",
    );
    accepts(
        "instance Eq<'a> => Eq<Maybe<'a>> { fn eq left right = match left with | None -> (match right with | None -> true | Some _ -> false) | Some value -> (match right with | Some other -> Eq.eq value other | None -> false); fn ne left right = !(Eq.eq left right) }\nlet left = Some \"abc\"\nlet right = Some \"abc\"\nleft == right",
    );
}

#[test]
fn default_methods_are_checked_once_even_without_instances() {
    accepts(
        "class C<'a> { def value :: 'a -> i64; fn value _value = 42 }\ninstance C<bool> {}\nC.value true",
    );
    accepts(
        "class Eq<'a> => Total<'a> { def same :: ref 'a -> ref 'a -> bool; fn same left right = Eq.eq left right }\ninstance Total<i64> {}\nlet number = 42l\nTotal.same (ref number) (ref number)",
    );
    accepts(
        "record Box<'a> { value: 'a }\nclass Size<'a> { def size :: ref 'a -> i64; def empty :: ref 'a -> bool; fn empty value = Size.size value == 0 }\ninstance Size<'a> => Size<Box<'a>> { fn size value = Size.size (ref value.value) }\ninstance Size<string> { fn size value = value.length }\nlet value = Box { value: \"abc\" }\nSize.empty (ref value)",
    );
    for (source, code) in [
        (
            "class C<'a> { def value :: 'a -> i64; fn value _value = true }",
            "E1003",
        ),
        (
            "class C<'a> { def value :: 'a -> i64; fn missing _value = 42 }",
            "E1016",
        ),
        (
            "class C<'a> { def value :: 'a -> i64; fn value value = C.value value }",
            "E1019",
        ),
        (
            "class C<'a> { def twice :: 'a -> 'a; fn twice value = value + value }",
            "E1027",
        ),
        (
            "class C<'a> { def twice :: 'a -> 'a }\ninstance C<'a> { fn twice value = value + value }",
            "E1027",
        ),
    ] {
        rejects(source, code);
    }
    let error = analyze_modules(&[
        (
            "Traits.tt",
            "class C<'a> { def value :: 'a -> i64; fn value _value = true }",
        ),
        ("Main.tz", "42"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1003");
    assert_eq!(error.span.source, Some(0));
}

#[test]
fn structural_comparisons_propagate_element_constraints() {
    for source in [
        "[1, 2] == [1, 2]",
        "[|1, 2|] < [|1, 2, 0|]",
        "(1, \"abc\") != (1, \"abd\")",
        "def equal :: ref ['a] -> ref ['a] -> bool\nfn equal left right = Eq.eq left right\nlet left = [\"abc\"]\nequal (ref left) (ref left)",
        "record Box<'a> { value: 'a }\ninstance Eq<'a> => Eq<Box<'a>> { fn eq left right = left.value == right.value; fn ne left right = !(Eq.eq left right) }\n[Box { value: \"abc\" }] == [Box { value: \"abc\" }]",
    ] {
        accepts(source);
    }
    for source in [
        "instance Eq<'a> => Eq<['a]> { fn eq _left _right = true; fn ne _left _right = false }",
        "instance Ord<'a> => Ord<[|'a|]> { fn lt _left _right = false; fn le _left _right = true; fn gt _left _right = false; fn ge _left _right = true }",
        "instance Eq<'a * 'b> { fn eq _left _right = true; fn ne _left _right = false }",
    ] {
        rejects(source, "E1016");
    }
    rejects(
        "record NoEquality { value: i64 }\n[NoEquality { value: 1 }] == [NoEquality { value: 1 }]",
        "E1005",
    );
    let module =
        accepts("export def compare :: i64 -> bool\nfn compare number = [|1, number|] < [|1, 3|]");
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.matches("phi ptr").count() >= 2);
    let elements = vec!["i64"; 100].join(" * ");
    accepts(&format!(
        "def compare :: ref ({elements}) -> ref ({elements}) -> bool\nfn compare left right = Ord.lt left right"
    ));
}

#[test]
fn instance_resolution_limits_and_alpha_renaming_are_enforced() {
    rejects(
        "record Box<'a> { value: 'a }\nclass C<'a> { def value :: 'a -> i64 }\ninstance C<Box<'a>> => C<'a> { fn value _value = 0 }\nC.value 1",
        "E1017",
    );
    rejects(
        "record Pair<'a, 'b> { first: 'a, second: 'b }\nclass C<'a> { def value :: 'a -> i64 }\ninstance C<Pair<'a, i64>> { fn value _value = 0 }\ninstance C<Pair<bool, 'b>> { fn value _value = 1 }",
        "E1016",
    );
    // Only heads with the same outermost constructor are unified and counted (D08).
    let declarations = (0..47)
        .map(|index| {
            format!(
                "record R{index} {{ value: i64 }}\ninstance C<Box<R{index}>> {{ fn value _value = 0 }}\n"
            )
        })
        .collect::<String>();
    rejects(
        &format!(
            "record Box<'a> {{ value: 'a }}\nclass C<'a> {{ def value :: 'a -> i64 }}\n{declarations}"
        ),
        "E1017",
    );
    let error = analyze_modules(&[("Main.tz", "record Box<'a> { value: 'a }\ninstance Eq<'missing> => Eq<Box<i64>> { fn eq _left _right = true; fn ne _left _right = false }")]).unwrap_err();
    assert_eq!(error.code, "E1027");
    assert_eq!(error.span.source, Some(0));
}

const BORROWED_EQUALITY: &str = "record Named { text: string }\n\
    instance Eq<Named> { fn eq left right = left.text == right.text; fn ne left right = !(Eq.eq left right) }\n";

#[test]
fn borrowed_comparison_operators_preserve_noncopy_values() {
    for source in [
        "let value = Named { text: \"text\" }\nlet same = value == value\nsame && value.text.length == 4",
        "Named { text: \"text\" } == Named { text: \"text\" }",
        "let values = new [Named { text: \"text\" }, Named { text: \"text\" }]\nvalues[0] == values[1] && values[0].text.length == 4",
        "let values = new [|Named { text: \"text\" }, Named { text: \"text\" }|]\nvalues[0] == values[1]",
        "def same :: Eq<'a> => 'a -> 'a -> bool\nfn same left right = left == right\nsame (Named { text: \"text\" }) (Named { text: \"text\" })",
    ] {
        accepts(&format!("{BORROWED_EQUALITY}{source}"));
    }
    accepts(
        "record Key { name: string, rank: i64 }\ninstance Eq<Key> { fn eq left right = left.name == right.name && left.rank == right.rank; fn ne left right = !(Eq.eq left right) }\ninstance Ord<Key> { fn lt left right = left.name < right.name || (left.name == right.name && left.rank < right.rank); fn le left right = !(Ord.lt right left); fn gt left right = Ord.lt right left; fn ge left right = !(Ord.lt left right) }\nlet key = Key { name: \"same\", rank: 42 }\nkey <= key && key.rank == 42",
    );
}

#[test]
fn borrowed_comparisons_preserve_conflicts_and_source_borrow_rules() {
    for source in [
        "let value = Named { text: \"text\" }\nvalue == { let moved = value; moved }",
        "let mut value = Named { text: \"text\" }\nvalue == { value = Named { text: \"next\" }; value }",
    ] {
        rejects(&format!("{BORROWED_EQUALITY}{source}"), "E1014");
    }
    rejects(
        &format!("{BORROWED_EQUALITY}let value = ref (Named {{ text: \"text\" }})\n()"),
        "E1013",
    );
    rejects(
        "record Named { text: string }\ninstance Eq<Named> { fn eq left right = left == right; fn ne left right = false }",
        "E1005",
    );
    rejects(
        "record Named { text: string }\ndef consume :: Named -> i64\nfn consume value = value.text.length\ninstance Eq<Named> { fn eq left right = consume (*left) == consume (*right); fn ne left right = false }",
        "E1012",
    );
    rejects(
        "record Named { text: string }\ninstance Eq<Named> { fn eq left right = Eq.eq left right; fn ne left right = false }",
        "E1019",
    );
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
         def answer :: i32
         fn answer = add 20 (22)",
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
         def answer :: i32
         fn answer = {
           let f: i32 -> i32 = id;
           let g = choose true;
           let boxed = id (Box { value: 40i32 });
           let text = id \"owned\";
           let size = (borrow (&text)).length;
           apply id (f (g (first [boxed.value, 0i32]))) + (size as i32) - 3i32
         }",
    );
    accepts(
        "def id :: 'a -> 'a\nfn id x = x\ndef answer :: i64\nfn answer = { let text = \"x\"; (id (&text)).length }",
    );
    accepts(
        "def replace :: &mut 'a -> 'a -> unit\nfn replace p x = { *p = x; }\ndef answer :: string\nfn answer = { let mut x = \"old\"; replace (&mut x) \"new\"; x }",
    );
}

#[test]
fn infers_constraints_through_forward_calls_and_recursion() {
    accepts(
        "def twice :: 'a -> 'a
         fn twice x = plus x x
         def plus :: 'a -> 'a -> 'a
         fn plus x y = x + y
         def rec total :: i64 -> 'a -> 'a -> 'a
         fn rec total n x acc = if n == 0 { acc } else { total (n - 1) x (plus acc x) }
         def answer :: i32
         fn answer = total 10 (twice 2i32) 2",
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
        "def increment :: (Add<'a>, Integer<'a>) => 'a -> 'a
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
         def answer :: i32
         fn answer = negate (increment (-43i32))",
    );
    rejects(
        "def huge :: 'a -> 'a\nfn huge x = x + 128\ndef answer :: i8\nfn answer = huge 0",
        "E1009",
    );
    rejects(
        "def number :: 'a -> 'a\nfn number x = x + 1\nnumber 1.0",
        "E1005",
    );
    rejects("def f :: Unknown<'a> => 'a -> 'a\nfn f x = x", "E1016");
    rejects("def f :: Add<'b> => 'a -> 'a\nfn f x = x", "E1015");
    accepts("def f :: Copy<fn() -> i64> => i32\nfn f = 42");
}

#[test]
fn supports_indented_type_class_constraints() {
    accepts(
        "def increment :: 'a -> 'a\n  @'a : Add, Integer\nfn increment value = value + 1\nincrement 41i32",
    );
    accepts(
        "def keep :: 'a -> 'b -> 'a\n  @'a : Add, Integer\n  @'b : Copy\nfn keep value other = value\nkeep 42i32 true",
    );
    accepts(
        "def add :: Add<'a> -> 'a -> 'a\n  @'a : Integer\nfn add left right = left + right\nadd 20i32 22",
    );
    accepts("def id :: 'a -> 'a\r\n  @'a : Copy\r\nfn id value = value\r\nid 42i32");
    rejects(
        "def id :: 'a -> 'a\n  @'a : Add, Integer\nfn id value = value\nid 1.0",
        "E1005",
    );
    rejects(
        "def id :: 'a -> 'a\n  @'b : Copy\nfn id value = value",
        "E1015",
    );
    rejects(
        "def id :: 'a -> 'a\n  @'a : Unknown\nfn id value = value",
        "E1016",
    );
    for declaration in [
        "def id :: 'a -> 'a @'a : Copy",
        "def id :: 'a -> 'a\n@'a : Copy",
        "def id :: 'a -> 'a\n  @'a :",
        "def id :: 'a -> 'a\n  @'a : Copy,",
        "def id :: 'a -> 'a\n  @'a Copy",
        "def id :: 'a -> 'a\n  @i32 : Copy",
        "def id :: 'a -> 'a\n  @\n  'a : Copy",
        "def id :: 'a -> 'a\n  @'a\n  : Copy",
        "def id :: 'a -> 'a\n  @'a : #\n  distance",
        "def id :: 'a -> 'a\n  @'a : #Point.distance",
    ] {
        rejects(&format!("{declaration}\nfn id value = value"), "E0002");
    }
}

#[test]
fn resolves_module_function_constraints_and_infers_results() {
    let module = analyze_modules(&[
        (
            "Main.tz",
            "def func :: 'T -> 'U\n  @'T : #distance\nfn func value = 'T.distance value\nlet result = func (Point { x: 3.0, y: 4.0 })\nresult",
        ),
        (
            "Point.tz",
            "record Point { x: f64, y: f64 }\ndef distance :: Point -> f64\nfn distance point = sqrt (point.x * point.x + point.y * point.y)",
        ),
    ])
    .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].signature.result,
        Type::F64
    );
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("call double @tz.fn.Point.distance"));
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

#[test]
fn propagates_function_constraints_and_supports_function_values() {
    let sources = [
        (
            "Main.tz",
            "def forward :: 'T -> 'U
fn forward value = measure value
def measure :: 'T -> 'U
    @'T : Copy, #distance, #constant
fn measure value = { let offset: 'U = 'T.constant(); (apply 'T.distance value) + offset }
def apply :: ('a -> 'b) -> 'a -> 'b
fn apply action value = action value
def unbox :: 'T -> 'U
  @'T : #unwrap
fn unbox value = (\\item -> 'T.unwrap item) value
def scaled :: 'T -> (f64 -> 'T)
  @'T : #scale
fn scaled value = 'T.scale value
export def left :: f64
fn left = forward (Left.Point { value: 5.0 })
export def right :: i64
fn right = forward (Right.Point { value: 42 })
export def boxed :: i64
fn boxed = unbox (Boxes.Box { value: 42i64 })
export def owned :: i64
fn owned = { let text: string = unbox (Boxes.Box { value: \"owned\" }); text.length }
export def partial :: f64
fn partial = Left.distance ((scaled (Left.Point { value: 5.0 })) 2.0)",
        ),
        (
            "Left.tz",
            "record Point { value: f64 }
def distance :: Point -> f64
fn distance point = point.value
def constant :: f64
fn constant = 0.0
def scale :: Point -> f64 -> Point
fn scale point factor = Point { value: point.value * factor }",
        ),
        (
            "Right.tz",
            "record Point { value: i64 }
def distance :: Point -> i64
fn distance point = point.value
def constant :: i64
fn constant = 0",
        ),
        (
            "Boxes.tz",
            "record Box<'a> { value: 'a }
def unwrap :: Box<'a> -> 'a
fn unwrap box = box.value",
        ),
    ];
    for sources in [sources.to_vec(), sources.into_iter().rev().collect()] {
        let module = analyze_modules(&sources).unwrap();
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert!(ir.contains("call double @tz.fn.Left.distance"));
        assert!(ir.contains("call i64 @tz.fn.Right.distance"));
        assert!(ir.contains("@tz.fn.Boxes.unwrap.$mono."));
        assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    }
}

#[test]
fn rejects_missing_inaccessible_and_mismatched_function_constraints() {
    let declaration = "def func :: 'T -> 'U\n  @'T : #distance\nfn func value = 'T.distance value";
    let record = "record Point { value: i64 }";
    for (implementation, call, code) in [
        ("", "func (Point { value: 42 })", "E1005"),
        ("", "func 42i64", "E1005"),
        (
            "private def distance :: Point -> i64\nfn distance point = point.value",
            "func (Point { value: 42 })",
            "E1022",
        ),
        (
            "def distance :: i64 -> i64\nfn distance value = value",
            "func (Point { value: 42 })",
            "E1003",
        ),
        (
            "def distance :: Point -> i64\nfn distance point = point.value",
            "let result: bool = func (Point { value: 42 })",
            "E1003",
        ),
        (
            "def distance :: Point -> i64 -> i64\nfn distance point extra = point.value + extra",
            "let result: i64 = func (Point { value: 42 })",
            "E1003",
        ),
    ] {
        let main = format!("{declaration}\n{call}");
        let point = format!("{record}\n{implementation}");
        let error = analyze_modules(&[("Main.tz", &main), ("Point.tz", &point)]).unwrap_err();
        assert_eq!(error.code, code, "{main}\n{point}\n{}", error.message);
        assert_eq!(error.span.source, Some(0));
    }
    rejects("def f :: 'T -> 'U\nfn f value = 'T.distance value", "E1016");
    rejects(
        "def f :: i64 -> i64\nfn f value = 'T.distance value",
        "E1015",
    );
    rejects(
        "def f :: 'T -> 'T\n  @'U : #distance\nfn f value = value",
        "E1015",
    );
    let error = analyze_modules(&[
        (
            "Main.tz",
            "def id :: 'T -> 'T\n  @'T : #distance\nfn id value = value\nid (Point { value: 42 })",
        ),
        ("Point.tz", record),
        (
            "Unrelated.tz",
            "def distance :: Point -> i64\nfn distance point = point.value",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1005");
}

#[test]
fn function_constraints_preserve_module_visibility_ownership_and_recursion() {
    let module = analyze_modules(&[
        (
            "Main.tz",
            "let value = Point { value: 42 }\nPoint.reveal value",
        ),
        (
            "Point.tz",
            "record Point { value: i64 }
private def distance :: Point -> i64
fn distance point = point.value
def reveal :: 'T -> 'U
  @'T : #distance
fn reveal value = 'T.distance value",
        ),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Library).unwrap();
    let point = "record Point { value: string }
def distance :: Point -> i64
fn distance point = point.value.length";
    let error = analyze_modules(&[
        (
            "Main.tz",
            "def twice :: 'T -> i64
  @'T : #distance
fn twice value = { let first = 'T.distance value; first + 'T.distance value }
twice (Point { value: \"owned\" })",
        ),
        ("Point.tz", point),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1005");
    let module = analyze_modules(&[
        ("Main.tz", "def borrow :: ref 'T -> ref 'T
  @'T : #borrow
fn borrow value = 'T.borrow value
def size :: i64
fn size = { let point = Point { value: \"owned\" }; (borrow (ref point)).value.length }"),
        ("Point.tz", "record Point { value: string }\ndef borrow :: ref Point -> ref Point\nfn borrow value = value"),
    ]).unwrap();
    llvm::emit(&module, llvm::Entry::Library).unwrap();
    for recursive in [false, true] {
        let recursion = if recursive { "rec " } else { "" };
        let main = format!(
            "def {recursion}func :: 'T -> 'U\n  @'T : #distance\nfn {recursion}func value = 'T.distance value\nfunc (Point {{ value: 42 }})"
        );
        let point = format!(
            "record Point {{ value: i64 }}\ndef {recursion}distance :: Point -> i64\nfn {recursion}distance value = Main.func value"
        );
        let result = analyze_modules(&[("Main.tz", &main), ("Point.tz", &point)]);
        if recursive {
            llvm::emit(&result.unwrap(), llvm::Entry::Library).unwrap();
        } else {
            assert_eq!(result.unwrap_err().code, "E1019");
        }
    }
}

#[test]
fn function_constraint_recursion_ignores_instance_method_names() {
    let module = analyze_modules(&[
        (
            "Main.tz",
            "def func :: 'T -> 'U\n  @'T : #distance\nfn func value = 'T.distance value",
        ),
        (
            "Point.tz",
            "record Point { value: i64 }
def distance :: Point -> i64
fn distance point = point.value
instance Traits.Distance<Point> { fn distance point = Main.func point }",
        ),
        (
            "Traits.tt",
            "class Distance<'a> { def distance :: 'a -> i64 }",
        ),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("call i64 @tz.fn.Point.distance"));
}

#[test]
fn bounds_and_deduplicates_function_constraints() {
    for (count, accepted) in [(128, true), (129, false)] {
        let constraints = (0..count)
            .map(|index| format!("#f{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!("def id :: 'T -> 'T\n  @'T : {constraints}\nfn id value = value");
        if accepted {
            accepts(&source);
        } else {
            rejects(&source, "E1017");
        }
    }
    let constraints = vec!["#distance"; 200].join(", ");
    accepts(&format!(
        "def id :: 'T -> 'T\n  @'T : {constraints}\nfn id value = value"
    ));
}

#[test]
fn supports_user_classes_instances_and_operator_instances() {
    accepts(
        "record Point { x: i32, y: i32 }
         class Measure<'a> {
           def measure :: &'a -> i32
         }
         instance Measure<Point> {
           fn measure p = p.x + p.y
         }
         instance Measure<i32> {
           fn measure x = *x
         }
         instance Add<Point> {
           fn add p q = Point { x: p.x + q.x, y: p.y + q.y }
         }
         def sum :: 'a -> 'a -> 'a
         fn sum x y = x + y
         def measure :: Measure<'a> => &'a -> i32
         fn measure x = Measure.measure x
         def answer :: i32
         fn answer = {
           let p = sum (Point { x: 10, y: 20 }) (Point { x: 5, y: 7 });
           let f: &Point -> i32 = Measure.measure;
           f (&p)
         }",
    );
    accepts(
        "def f :: i32\nfn f = Add.add 20 22\ndef g :: f64\nfn g = { let op: f64 -> f64 -> f64 = Mul.mul; op 2.0 3.0 }",
    );
    rejects(
        "class C<'a> { def f :: 'a -> i32 }\ninstance C<bool> {}\n",
        "E1016",
    );
    rejects(
        "class C<'a> { def f :: 'a -> i32 }\ninstance C<bool> { fn f x = true }",
        "E1003",
    );
    rejects(
        "class C<'a> { def f :: 'a -> i32 }\ninstance C<bool> { fn other x = 1 }",
        "E1016",
    );
    rejects("instance Add<i32> { fn add x y = x - y }", "E1016");
    rejects("instance Copy<string> {}", "E1016");
    rejects(
        "class C<'a> { def f :: 'a -> i32 }\ninstance C<bool> { fn f x = 1 }\ninstance C<bool> { fn f x = 2 }",
        "E1016",
    );
}

#[test]
fn resolves_polymorphism_across_modules_and_preserves_diagnostics() {
    let module = analyze_modules(&[
        ("Library", "class Size<'a> { def size :: &'a -> i64 }\ndef id :: 'a -> 'a\nfn id x = x\ndef size :: &'a -> i64\nfn size x = Size.size x"),
        ("Main", "instance Library.Size<string> { fn size text = text.length }\nlet text = Library.id \"hello\"\nLibrary.size (&text)"),
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
        (
            "Main",
            "def answer :: i32\nfn answer = Add.add 20 22\nanswer()",
        ),
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
    rejects(
        "def rec answer :: 'a\nfn rec answer = answer()\nanswer()",
        "E1015",
    );
    rejects(
        "def duplicate :: 'a -> ['a]\nfn duplicate x = [x, x]\nduplicate \"owned\"",
        "E1005",
    );
    rejects(
        "def first :: ['a] -> 'a\nfn first x = x[0]\nfirst [\"a\", \"b\"]",
        "E1005",
    );
    rejects(
        "def id :: 'a -> 'a\nfn id x = x\ndef answer :: &string\nfn answer = { let x = \"x\"; id (&x) }",
        "E1013",
    );
}

#[test]
fn bounds_type_growing_polymorphic_recursion() {
    rejects("def rec f :: 'a -> unit\nfn rec f x = f [x]\nf 1", "E1017");
    rejects(
        "def rec f :: 'a -> unit\nfn rec f x = { let y = x + x; f [y] }",
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
fn accepts_more_specializations_than_the_old_limit() {
    // G17 Phase 3 raised the limit of 1,024 to 65,536 and stops type-growing recursion on its
    // own. `check::polymorph::tests::honors_the_exact_specialization_limit` tests the boundary
    // with small limits, and `tests/e2e.mjs` the real one.
    use std::fmt::Write;
    let mut source = String::from("def id :: 'a -> 'a\nfn id x = x\n");
    for index in 0..1024 {
        writeln!(
            source,
            "record R{index} {{ value: i8 }}\nfn f{index}(x: R{index}) -> R{index} {{ id x }}"
        )
        .unwrap();
    }
    source.push_str("fn extra(x: [i8]) -> [i8] { id x }");
    assert!(analyze(&source).is_ok());
}

#[test]
fn reports_type_growing_recursion_by_name() {
    for recursion in ["f [x]", "{ f (ref x, 1); f (ref x, true) }"] {
        let source = format!("def rec f :: 'a -> unit\nfn rec f x = {recursion}\nf 1");
        let error = analyze(&source).expect_err(&source);
        assert_eq!(error.code, "E1017", "{}", error.message);
        assert!(
            error
                .message
                .starts_with("polymorphic recursion grows the types of 'Main.f' without bound;"),
            "{}",
            error.message
        );
    }
}

#[test]
fn honors_the_constraint_limit_and_deduplicates_repeated_requirements() {
    use std::fmt::Write;
    let mut declarations = String::new();
    for index in 0..129 {
        writeln!(declarations, "class C{index}<'a> {{ def f :: 'a -> i64 }}").unwrap();
    }
    for (count, accepted) in [(128, true), (129, false)] {
        let constraints = (0..count)
            .map(|index| format!("C{index}<'a>"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!("{declarations}def f :: ({constraints}) => 'a -> 'a\nfn f x = x");
        if accepted {
            accepts(&source);
        } else {
            rejects(&source, "E1017");
        }
    }
    let constraints = vec!["Copy<'a>"; 200].join(", ");
    accepts(&format!("def f :: ({constraints}) => 'a -> 'a\nfn f x = x"));
}

#[test]
fn checks_declarations_and_specialized_layouts_even_when_unused() {
    rejects("def f :: i32 -> i32\nfn f x = x\nfn f x = x", "E1001");
    rejects("def f :: i32\ndef f :: i32\nfn f = 1", "E1001");
    rejects("class C<'a> { def f :: 'b -> 'b }", "E1016");
    accepts("class C<'a> { def f :: 'a -> [[i64]] }");
    accepts("class C<'a> { def f :: 'a -> i64 }\ninstance C<'a> { fn f _value = 1 }");
    rejects(
        "class C<'a> { def f :: 'a -> i64 }\ninstance C<sbyte> { fn f x = 1 }\ninstance C<i8> { fn f x = 2 }",
        "E1016",
    );
    rejects(
        "class C<'a> { def f :: 'a -> i64 }\ninstance C<byte> { fn f x = 1 }\ninstance C<i8u> { fn f x = 2 }",
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
         def answer :: i32
         fn answer = add ((choose true) (R { x: 20 }).x) 22",
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
        "def rec loop :: i64 -> 'a -> 'a\nfn rec loop n x = if n == 0 { x } else { loop (n - 1) x }\nloop 10 \"owned\"",
        "def replace :: 'a -> 'a -> 'a\nfn replace mut x y = { let old = x; x = y; x }\nreplace \"old\" \"new\"",
        "def dup :: 'a -> ['a]\nfn dup x = [x, x]\ndup 2i32",
        "def read :: &'a -> 'a\nfn read x = *x\ndef answer :: i64\nfn answer = { let x = 42; read (&x) }",
    ] {
        accepts(source);
    }
    rejects(
        "def read :: &'a -> 'a\nfn read x = *x\ndef answer :: string\nfn answer = { let x = \"x\"; read (&x) }",
        "E1005",
    );
    rejects(
        "def f :: Copy<'a> => 'a -> 'a\nfn f x = x\nf [\"owned\"]",
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
        "def bits :: 'a -> 'a -> 'a\nfn bits x y = Bits.ushr ((~~~x &&& y ||| x ^^^ y) <<< y >>> y) (y % x)\ndef answer :: i32\nfn answer = bits 42 2",
    );
    accepts("instance Add<[i32]> { fn add x y = x }\ndef answer :: [i32]\nfn answer = [] + []");
}
