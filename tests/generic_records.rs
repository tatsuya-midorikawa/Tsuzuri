use tsuzuri::check::{CheckedModule, Type};
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::syntax::TypeExprKind;
use tsuzuri::{analyze, analyze_modules, llvm, parser};

const PAIR: &str = "record Pair<'a, 'b> { first: 'a, second: 'b }\n";

fn accepts(source: &str) -> CheckedModule {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Console).unwrap());
    module
}

fn rejects(source: &str, code: &str) -> Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error
}

fn ir(source: &str) -> String {
    llvm::emit(&accepts(source), llvm::Entry::Library).unwrap()
}

fn function<'a>(module: &'a CheckedModule, name: &str) -> &'a tsuzuri::check::CheckedFunction {
    module
        .functions
        .iter()
        .find(|function| function.name == name || function.name.starts_with(&format!("{name}.")))
        .unwrap_or_else(|| panic!("missing function {name}"))
}

#[test]
fn infers_substitutes_and_displays_type_arguments() {
    let module = accepts(&format!(
        "{PAIR}def first :: Pair<'a, 'b> -> 'a
fn first pair = pair.first
def swap :: Pair<'a, 'b> -> Pair<'b, 'a>
fn swap pair = Pair {{ first: pair.second, second: pair.first }}
def main :: i64
fn main =
    let p = Pair {{ first: 20, second: \"xx\" }}
    let n = p.second.length
    first (swap (swap p)) + n
"
    ));
    let types = module.types();
    let swaps: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("swap."))
        .map(|function| {
            format!(
                "{} -> {}",
                function.signature.parameters[0].display(&types),
                function.signature.result.display(&types)
            )
        })
        .collect();
    assert_eq!(swaps.len(), 2, "{swaps:?}");
    assert!(swaps.contains(&"Main.Pair<i64, string> -> Main.Pair<string, i64>".to_owned()));
    assert!(swaps.contains(&"Main.Pair<string, i64> -> Main.Pair<i64, string>".to_owned()));
    let first = function(&module, "first");
    assert!(matches!(
        &first.signature.parameters[0],
        Type::Record(_, arguments) if **arguments == [Type::I64, Type::String]
    ));
    assert_eq!(first.signature.result, Type::I64);
    let error = rejects(
        &format!(
            "{PAIR}def main :: i64
fn main =
    let p: Pair<i64, string> = Pair {{ first: 1, second: \"x\" }}
    let q: Pair<string, i64> = p
    0
"
        ),
        "E1003",
    );
    assert!(
        error
            .message
            .contains("expected Main.Pair<string, i64>, found Main.Pair<i64, string>"),
        "{}",
        error.message
    );
    // Partially inferred arguments report the first mismatching argument, not an
    // intermediate binding such as `Main.Pair<string, string>`.
    let error = rejects(
        &format!(
            "{PAIR}def main :: i64
fn main =
    let p = Pair {{ first: 1, second: \"x\" }}
    let q: Pair<string, i64> = p
    0
"
        ),
        "E1003",
    );
    assert!(!error.message.contains("Main.Pair"), "{}", error.message);
    let error = rejects(
        &format!(
            "{PAIR}def main :: i64
fn main =
    let p: Pair<i64, string> = Pair {{ first: 1, second: 2 }}
    0
"
        ),
        "E1003",
    );
    assert!(error.message.contains("string"), "{}", error.message);
}

#[test]
fn accepts_nested_instances_patterns_and_structural_ownership() {
    for source in [
        format!(
            "{PAIR}record Box<'a> {{ value: 'a }}
record Nested<'a> {{ item: Box<Pair<'a, i64>> }}
def get :: Nested<string> -> string
fn get n = n.item.value.first
def main :: i64
fn main = (get (Nested {{ item: Box {{ value: Pair {{ first: \"ok\", second: 1 }} }} }})).length
"
        ),
        format!(
            "{PAIR}instance Add<Pair<i64, i64>> {{
    fn add left right =
        Pair {{ first: left.first + right.first, second: left.second + right.second }}
}}
def main :: i64
fn main =
    let p = Pair {{ first: 20, second: 1 }} + Pair {{ first: 22, second: 2 }}
    p.first * 10 + p.second
"
        ),
        "record Holder<'a> { value: 'a }
def id_holder :: Holder<'a> -> Holder<'a>
fn id_holder h = h
def main :: i64
fn main = (id_holder (Holder { value: [1, 2, 3] })).value.length
"
        .to_owned(),
        format!(
            "{PAIR}def main :: i64
fn main =
    match Pair {{ first: \"a\", second: 42 }} with
    | Pair {{ first = text }} -> text.length
"
        ),
        format!(
            "{PAIR}def use_unqualified :: Pair<i64, i64> -> i64
fn use_unqualified pair =
    match pair with
    | {{ first = x; second = y }} -> x + y
def main :: i64
fn main = use_unqualified (Pair {{ first: 20, second: 22 }})
"
        ),
        // Copy records stay Copy; the generic accessor infers `Copy<'a>`.
        format!(
            "{PAIR}def first_twice :: Pair<'a, 'b> -> ('a * 'a)
fn first_twice p = (p.first, p.first)
def main :: i64
fn main =
    let p = Pair {{ first: 20, second: true }}
    let q = p
    match first_twice p with
    | (a, b) -> a + b + q.first
"
        ),
        // Function values, lists, and tuples are valid type arguments.
        format!(
            "{PAIR}record Box<'a> {{ value: 'a }}
def call :: Box<i64 -> i64> -> i64
fn call boxed = boxed.value 1
def main :: i64
fn main =
    let k = 40
    let items = Box {{ value: [|Pair {{ first: (1, \"a\"), second: 2 }}|] }}
    call (Box {{ value: x -> x + k }}) + items.value.length
"
        ),
    ] {
        accepts(&source);
    }
}

#[test]
fn ownership_and_copy_constraints_follow_substituted_fields() {
    rejects(
        &format!(
            "{PAIR}def main :: i64
fn main =
    let p = Pair {{ first: \"a\", second: 1 }}
    let q = p
    p.second
"
        ),
        "E1012",
    );
    rejects(
        &format!(
            "{PAIR}def first_twice :: Pair<'a, 'b> -> ('a * 'a)
fn first_twice p = (p.first, p.first)
def main :: i64
fn main =
    match first_twice (Pair {{ first: \"x\", second: 1 }}) with
    | (a, b) -> a.length
"
        ),
        "E1005",
    );
    // Moving one field leaves the others usable, as for non-generic records.
    accepts(&format!(
        "{PAIR}def main :: i64
fn main =
    let p = Pair {{ first: \"abc\", second: \"de\" }}
    let text = p.first
    text.length + p.second.length
"
    ));
}

#[test]
fn rejects_invalid_declarations_and_applications() {
    for (source, code, message) in [
        (
            "record Box<'a> { value: i64 }",
            "E1024",
            "not used by any field",
        ),
        (
            "record Box<'a, 'a> { value: 'a }",
            "E1024",
            "duplicate type parameter",
        ),
        (
            "record Box { value: 'a }",
            "E1024",
            "is not declared by record 'Box'",
        ),
        (
            "record Pair<'a, 'b> { first: 'a, second: 'b }\ndef f :: Pair<i64> -> i64\nfn f p = 0",
            "E1004",
            "type 'Pair' expects 2 type arguments, found 1",
        ),
        (
            "record Box<'a> { value: 'a }\ndef f :: Box -> i64\nfn f p = 0",
            "E1004",
            "expects 1 type argument, found 0",
        ),
        (
            "record Point { x: i64 }\ndef f :: Point<i64> -> i64\nfn f p = 0",
            "E1004",
            "takes no type arguments",
        ),
        (
            "def f :: i64<i64> -> i64\nfn f p = 0",
            "E1004",
            "takes no type arguments",
        ),
        ("record Add<'a> { value: 'a }", "E1001", ""),
        ("record Copy { value: i64 }", "E1001", ""),
        (
            "class C<'a> { def f :: 'a -> i64 }\nrecord C<'a> { value: 'a }",
            "E1001",
            "",
        ),
        (
            "record C<'a> { value: 'a }\nclass C<'a> { def f :: 'a -> i64 }",
            "E1001",
            "",
        ),
        (
            "record R<'a> { next: R<'a> }",
            "E1010",
            "recursive value layout",
        ),
        (
            "record R<'a> { next: [R<'a>] }",
            "E1010",
            "recursive value layout",
        ),
        (
            "record Holder<'a> { value: 'a }\nrecord Node { child: Holder<Node> }",
            "E1010",
            "recursive value layout",
        ),
        (
            "record Holder<'a> { value: 'a }\ndef f :: Holder<&i64> -> i64\nfn f h = 0",
            "E1013",
            "would store a reference in field 'value'",
        ),
        (
            "record Holder<'a> { value: ['a] }\ndef f :: Holder<&mut i64> -> i64\nfn f h = 0",
            "E1013",
            "would store a reference",
        ),
        (
            "record Holder<'a> { value: 'a }\ndef wrap :: 'a -> Holder<'a>\nfn wrap x = Holder { value: x }\ndef f :: i64 -> i64\nfn f x =\n    let h = wrap &x\n    0",
            "E1013",
            "would store a reference",
        ),
        (
            "record Holder<'a> { value: 'a }\ndef f :: i64 -> i64\nfn f x =\n    let h = Holder { value: &x }\n    0",
            "E1013",
            "would store a reference",
        ),
        (
            "record Box<'a> { value: 'a }\ndef rec grow :: 'a -> i64\nfn rec grow x = grow (Box { value: x })\ndef main :: i64\nfn main = grow 1",
            "E1017",
            "",
        ),
        (
            "record Pair<'a, 'b> { first: 'a, second: 'b }\nexport def f :: Pair<i64, i64> -> i64\nfn f p = p.first",
            "E1008",
            "",
        ),
        (
            "def f :: Missing<i64> -> i64\nfn f p = 0",
            "E1004",
            "unknown type class, record type, or union type 'Missing'",
        ),
    ] {
        let error = rejects(source, code);
        assert!(
            error.message.contains(message),
            "{source}\n{}",
            error.message
        );
    }
}

#[test]
fn rejects_anonymous_type_parameters_while_lexing() {
    let error = rejects("record Box<'_> { value: i64 }", "E0001");
    assert!(
        error.message.contains("apostrophe and an ASCII letter"),
        "{}",
        error.message
    );
}

#[test]
fn enforces_layout_limits_per_concrete_instance() {
    let fields = (0..8)
        .map(|index| format!("f{index}: 'a"))
        .collect::<Vec<_>>()
        .join(", ");
    let wide = format!("{PAIR}record Wide<'a> {{ {fields} }}\n");
    // Each array field takes 16 bytes, so four levels of eight fields are exactly 64 KiB.
    accepts(&format!(
        "{wide}def f :: Wide<Wide<Wide<Wide<[i64]>>>> -> i64\nfn f w = 0\ndef main :: i64\nfn main = 0"
    ));
    for ty in [
        "Pair<Wide<Wide<Wide<Wide<[i64]>>>>, i64>",
        "Wide<Wide<Wide<Wide<i64 * [i64]>>>>",
    ] {
        let error = rejects(&format!("{wide}def f :: {ty} -> i64\nfn f w = 0"), "E1010");
        assert!(error.message.contains("65536"), "{}", error.message);
    }
    // Generic records that are never instantiated too large are accepted.
    accepts(&format!(
        "{wide}def id :: 'a -> 'a\nfn id x = x\ndef main :: i64\nfn main = (id (Pair {{ first: 1, second: 2 }})).first"
    ));
}

#[test]
fn classifies_applied_names_as_constraints_or_records() {
    let module = analyze_modules(&[
        ("Main.tz", "def f :: Traits.Score<'a> => 'a -> i64\nfn f x = Traits.Score.score x\ndef main :: i64\nfn main = f (Shapes.Pair { first: 1, second: 2 })"),
        ("Traits.tt", "class Score<'a> { def score :: 'a -> i64 }"),
        (
            "Shapes.tz",
            "record Pair<'a, 'b> { first: 'a, second: 'b }\ninstance Traits.Score<Pair<i64, i64>> { fn score p = p.first + p.second }",
        ),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    llvm::emit(&module, llvm::Entry::Console).unwrap();
    let error = analyze_modules(&[
        (
            "Main.tz",
            "def f :: Shapes.Pair<i64, i64> -> i64\nfn f p = 0",
        ),
        (
            "Shapes.tz",
            "private record Pair<'a, 'b> { first: 'a, second: 'b }",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1022", "{}", error.message);
    let error = analyze_modules(&[
        (
            "Shapes.tz",
            "private record Hidden { value: i64 }\nrecord Box<'a> { value: 'a }\ndef leak :: Box<Hidden> -> i64\nfn leak b = 0",
        ),
        ("Main.tz", "42"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1022", "{}", error.message);
}

#[test]
fn parses_nested_angle_bracket_type_applications() {
    let program = parser::parse(
        "record Pair<'a, 'b> { first: 'a, second: 'b }
def f :: [Pair<i64, Pair<string, i64>>] -> Task<Pair<i64, i64>> -> &Pair<i64, i64> -> Pair<&i64, i64> -> i64
fn f a b c d = 0",
    )
    .unwrap();
    let record = &program.records[0];
    assert_eq!(
        record
            .parameters
            .iter()
            .map(|parameter| parameter.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let function = &program.functions[0];
    assert!(matches!(&function.result.kind, TypeExprKind::Named(name) if name == "i64"));
    let parameters: Vec<_> = function
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect();
    let TypeExprKind::Array(element) = &parameters[0].kind else {
        panic!("expected an array")
    };
    let TypeExprKind::Apply(head, arguments) = &element.kind else {
        panic!("expected an application")
    };
    assert_eq!(head.text, "Pair");
    assert_eq!(arguments.len(), 2);
    assert!(
        matches!(&arguments[1].kind, TypeExprKind::Apply(inner, nested)
        if inner.text == "Pair" && nested.len() == 2)
    );
    assert!(matches!(&parameters[1].kind, TypeExprKind::Task(inner)
        if matches!(inner.kind, TypeExprKind::Apply(..))));
    assert!(
        matches!(&parameters[2].kind, TypeExprKind::Reference(inner, false)
        if matches!(inner.kind, TypeExprKind::Apply(..)))
    );
    assert!(
        matches!(&parameters[3].kind, TypeExprKind::Apply(_, arguments)
        if matches!(arguments[0].kind, TypeExprKind::Reference(..)))
    );
}

#[test]
fn shares_angle_syntax_with_unions_classes_constraints_instances_and_tasks() {
    accepts(
        "record Pair<
    'a, /* the second type */ 'b,
> { first: 'a, second: 'b }
union Wrapped<'a,>=Wrap of 'a
class Measure<'a,> { def measure :: ref 'a -> i64 }
instance Measure<Pair<i64, string>> {
    fn measure pair = pair.first + pair.second.length
}
def measure :: Measure<'a>=>ref 'a -> i64
fn measure value = Measure.measure value
def deferred :: 'a -> Task<Wrapped<'a>>
fn deferred value = task { return Wrap value }
def constrained :: (Copy<Pair<i64, i64>>, Add<i64>) => i64 -> i64
fn constrained x = x + 1
def main :: i64
fn main =
    let wrapped: Wrapped<Pair<i64, string>>=Task.run (deferred (Pair { first: 39, second: \"abc\" }))
    match wrapped with
    | Wrap pair -> constrained (measure ref pair)",
    );
}

#[test]
fn displays_nested_named_types_tasks_and_borrows_with_angle_brackets() {
    let module = analyze(
        "record Box<'a> { value: 'a }
def inspect :: ref Box<Option<i64>> -> Task<Box<i64 -> i64>> -> unit
fn inspect value pending = ()",
    )
    .unwrap();
    let signature = &function(&module, "inspect").signature;
    assert_eq!(
        signature.parameters[0].display(&module.types()),
        "ref Main.Box<Option.Option<i64>>"
    );
    assert_eq!(
        signature.parameters[1].display(&module.types()),
        "Task<Main.Box<i64 -> i64>>"
    );
    let error = rejects(
        "let work: Task<unit> = task {}\nlet x: i64 = work\n0",
        "E1003",
    );
    assert!(error.message.contains("Task<unit>"), "{}", error.message);
    rejects("def f :: Add<i64, i64> -> i64\nfn f x = 0", "E1016");
}

#[test]
fn rejects_legacy_and_malformed_generic_syntax() {
    for source in [
        "record Pair 'a 'b { first: 'a, second: 'b }",
        "union Maybe 'a = None | Some of 'a",
        "class C 'a { def f :: 'a -> i64 }",
        "instance C i64 { fn f x = x }",
        "def f :: Pair i64 string -> i64\nfn f p = 0",
        "def f :: Task i64 -> i64\nfn f t = 0",
        "def f :: Add 'a -> 'a\nfn f x = x",
        "def f :: Add 'a => 'a -> 'a\nfn f x = x",
        "record R<> {}",
        "union U<> = Empty",
        "class C<> { def f :: i64 }",
        "class C<'a, 'b> { def f :: 'a -> 'b }",
        "record R<a> { value: a }",
        "record R<'a 'b> { value: 'a }",
        "record R<'a,, 'b> { value: 'a }",
        "record R<'a { value: 'a }",
        "record R<'a>> { value: 'a }",
        "record R <'a> { value: 'a }",
        "let x: Option<> = None\n0",
        "let x: Result<i64 string> = Ok 1\n0",
        "let x: Result<i64,, string> = Ok 1\n0",
        "let x: Option<i64 = Some 1\n0",
        "def f :: Task<i64, string> -> i64\nfn f t = 0",
        "def f :: 'a<i64> -> i64\nfn f x = 0",
        "def f :: Copy<> => i64\nfn f = 0",
        "def f :: Copy<i64, string> => i64\nfn f = 0",
        "instance C<> { fn f x = x }",
        "instance C<i64, string> { fn f x = x }",
    ] {
        let error = parser::parse(source).expect_err(source);
        assert_eq!(error.code, "E0002", "{source}\n{}", error.message);
    }
}

#[test]
fn retains_exact_source_spans_when_splitting_generic_closers() {
    let source = "def f :: &Box<Box<Box<i64>>> -> i64\nfn f x = 0";
    let program = parser::parse_with_source(source, 7).unwrap();
    let parameter = &program.functions[0].parameters[0].ty;
    assert_eq!(
        &source[parameter.span.start..parameter.span.end],
        "&Box<Box<Box<i64>>>"
    );
    let TypeExprKind::Reference(inner, _) = &parameter.kind else {
        panic!("expected a reference")
    };
    let mut inner = inner.as_ref();
    for expected in ["Box<Box<Box<i64>>>", "Box<Box<i64>>", "Box<i64>"] {
        assert_eq!(inner.span.source, Some(7));
        assert_eq!(&source[inner.span.start..inner.span.end], expected);
        let TypeExprKind::Apply(head, arguments) = &inner.kind else {
            panic!("expected an application")
        };
        assert_eq!(&source[head.span.start..head.span.end], "Box");
        assert_eq!(head.span.source, Some(7));
        inner = &arguments[0];
    }
    assert_eq!(&source[inner.span.start..inner.span.end], "i64");
}

#[test]
fn bounds_generic_lists_and_nested_types() {
    use tsuzuri::syntax::MAX_NESTING;

    for extra in [0, 1] {
        let count = MAX_NESTING + extra;
        let parameters = (0..count)
            .map(|index| format!("'p{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = vec!["i64"; count].join(", ");
        let depth = count - 1;
        for source in [
            format!("record R<{parameters}> {{ value: 'p0 }}"),
            format!("def f :: R<{arguments}> -> i64\nfn f x = 0"),
            format!(
                "def f :: {}i64{} -> i64\nfn f x = 0",
                "Box<".repeat(depth),
                ">".repeat(depth)
            ),
        ] {
            let result = parser::parse(&source);
            if extra == 0 {
                result.unwrap_or_else(|error| panic!("{source}\n{}", error.message));
            } else {
                let error = result.expect_err(&source);
                assert_eq!(error.code, "E0002", "{}", error.message);
            }
        }
    }
}

#[test]
fn keeps_comparisons_and_shifts_as_expression_operators() {
    let output = ir("def f :: i64 -> bool
fn f x =
    x<2 && x<=2 && x> -2 && x>= -2 &&
    (x>>1)>=0 && (x>>>1)>=0 && (x<<1)>=0 && x as i64 < 3
def main :: bool
fn main = f 1");
    for instruction in ["ashr i64", "lshr i64", "shl i64", "icmp slt i64"] {
        assert!(output.contains(instruction), "{instruction}\n{output}");
    }
}

#[test]
fn mangles_each_concrete_instance_once_in_canonical_order() {
    let source = format!(
        "{PAIR}record Wrap<'a> {{ value: 'a }}
record Point {{ x: i64 }}
def f :: Wrap<Pair<i64, string>> -> i64
fn f w = w.value.first + w.value.second.length
def g :: Pair<i64, i64> -> Wrap<Pair<i64 -> i64, [|Point|]>> -> Wrap<i64 * string> -> i64
fn g p w t = p.first + p.second + w.value.second.length
def main :: i64
fn main =
    let w = Wrap {{ value: Pair {{ first: 1, second: \"abc\" }} }}
    let k = 1
    f w + g (Pair {{ first: 1, second: 2 }}) (Wrap {{ value: Pair {{ first: x -> x + k, second: [|Point {{ x: 1 }}|] }} }}) (Wrap {{ value: (1, \"s\") }})
"
    );
    let ir = ir(&source);
    for definition in [
        "%tz.record.Main.Point = type { i64 }",
        "%\"tz.record.Main.Pair[i64,string]\" = type { i64, %tz.string }",
        "%\"tz.record.Main.Pair[i64,i64]\" = type { i64, i64 }",
        "%\"tz.record.Main.Pair[fn[i64->i64],list[Main.Point]]\" = type { %tz.closure, %tz.list }",
        "%\"tz.record.Main.Wrap[Main.Pair[i64,string]]\" = type { %\"tz.record.Main.Pair[i64,string]\" }",
        "%\"tz.record.Main.Wrap[Main.Pair[fn[i64->i64],list[Main.Point]]]\" = type { %\"tz.record.Main.Pair[fn[i64->i64],list[Main.Point]]\" }",
        "%\"tz.record.Main.Wrap[tuple[i64,string]]\" = type { { i64, %tz.string } }",
    ] {
        assert_eq!(
            ir.matches(&format!("{definition}\n")).count(),
            1,
            "{definition}\n{ir}"
        );
    }
    assert!(!ir.contains("%tz.record.Main.Pair ="), "{ir}");
    assert!(!ir.contains("%tz.record.Main.Wrap ="), "{ir}");
    let order: Vec<_> = ir
        .lines()
        .filter(|line| line.starts_with("%tz.record.") || line.starts_with("%\"tz.record."))
        .map(|line| line.split(" = ").next().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "%tz.record.Main.Point",
            "%\"tz.record.Main.Pair[i64,i64]\"",
            "%\"tz.record.Main.Pair[i64,string]\"",
            "%\"tz.record.Main.Pair[fn[i64->i64],list[Main.Point]]\"",
            "%\"tz.record.Main.Wrap[Main.Pair[i64,string]]\"",
            "%\"tz.record.Main.Wrap[Main.Pair[fn[i64->i64],list[Main.Point]]]\"",
            "%\"tz.record.Main.Wrap[tuple[i64,string]]\"",
        ]
    );
    for target in [false, true] {
        let module = analyze(&source).unwrap();
        let first = llvm::emit_target(&module, llvm::Entry::Console, target).unwrap();
        assert_eq!(
            first,
            llvm::emit_target(&analyze(&source).unwrap(), llvm::Entry::Console, target).unwrap()
        );
    }
}

#[test]
fn nongeneric_records_keep_their_llvm_names() {
    let ir = ir("record Point { x: f64, y: f64 }
def norm :: Point -> f64
fn norm p = p.x * p.x + p.y * p.y
def main :: i64
fn main = (norm (Point { x: 3.0, y: 4.0 })) as i64
");
    assert!(ir.contains("%tz.record.Main.Point = type { double, double }"));
    assert!(!ir.contains("%\"tz.record."));
}
