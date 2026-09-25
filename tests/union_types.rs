use tsuzuri::check::{CheckedModule, Type};
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::{analyze, analyze_modules, llvm, parser};

const SHAPE: &str = "union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty
";
const MAYBE: &str = "union Maybe 'a = None | Some of 'a\n";

fn accepts(source: &str) -> CheckedModule {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
    if module.entry.is_some() || module.functions.iter().any(|f| f.name == "main") {
        llvm::emit(&module, llvm::Entry::Console).unwrap();
    }
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

/// The body of the LLVM function whose symbol starts with `symbol`.
fn body<'a>(ir: &'a str, symbol: &str) -> &'a str {
    let start = ir
        .find(&format!("@{symbol}("))
        .unwrap_or_else(|| panic!("missing {symbol}\n{ir}"));
    let end = ir[start..].find("\n}\n").unwrap() + start;
    &ir[start..end]
}

#[test]
fn accepts_sum_types_enums_generic_and_first_class_constructors() {
    for source in [
        format!(
            "{SHAPE}def area :: Shape -> f64
fn area shape =
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0
area (Rect (3.0, 4.0))
"
        ),
        format!(
            "{MAYBE}def default_value :: 'a -> Maybe 'a -> 'a
fn default_value fallback value =
    match value with
    | Some x -> x
    | None -> fallback
default_value 10 (Some 42) + (default_value \"fallback\" None).length
"
        ),
        "union Color = Red | Green | Blue
def code :: Color -> i64
fn code color =
    match color with
    | Red -> 1
    | Green -> 2
    | Blue -> 3
code Green
"
        .to_owned(),
        format!(
            "{MAYBE}def apply :: ('a -> 'b) -> 'a -> 'b
fn apply f x = f x
let wrap = Some
let piped = 1 |> Some
match apply Some 42 with
| Some n -> n + (match wrap \"text\" with | Some t -> t.length | None -> 0)
| None -> 0
"
        ),
        // A leading bar, one-line declarations, and cases of records, tuples,
        // collections, and functions.
        "record Point { x: i64, y: i64 }
union Item = | Dot of Point | Pair of i64 * string | Many of [i64] | Chain of [|string|] | Call of i64 -> i64 | Nothing
def score :: Item -> i64
fn score item =
    match item with
    | Dot Point { x = x; y = y } -> x + y
    | Pair (n, text) -> n + text.length
    | Many values -> values.length
    | Chain values -> values.length
    | Call f -> f 1
    | Nothing -> 0
let k = 10
score (Dot (Point { x: 1, y: 2 })) + score (Pair (3, \"four\")) + score (Many [1, 2]) + score (Chain [|\"a\"|]) + score (Call (x -> x + k)) + score Nothing
"
        .to_owned(),
        // `of` stays an ordinary identifier outside union declarations.
        "def of :: i64 -> i64\nfn of x = x + 1\nlet of_value = of 41\nof_value".to_owned(),
    ] {
        accepts(&source);
    }
}

#[test]
fn infers_and_monomorphizes_generic_unions() {
    let module = accepts(&format!(
        "{MAYBE}def unwrap_or :: 'a -> Maybe 'a -> 'a
fn unwrap_or fallback value =
    match value with
    | Some x -> x
    | None -> fallback
def main :: i64
fn main =
    let text = unwrap_or \"\" (Some \"abc\")
    unwrap_or 0 (Some 1) + text.length
"
    ));
    let types = module.types();
    let mut instances: Vec<String> = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("unwrap_or."))
        .map(|function| function.signature.parameters[1].display(&types))
        .collect();
    instances.sort();
    assert_eq!(instances, ["Main.Maybe i64", "Main.Maybe string"]);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap();
    assert_eq!(main.signature.result, Type::I64);
    let error = rejects(
        &format!(
            "{MAYBE}def main :: i64
fn main =
    let value: Maybe string = Some 1
    0
"
        ),
        "E1003",
    );
    assert!(error.message.contains("string"), "{}", error.message);
}

#[test]
fn resolves_qualified_cases_across_modules() {
    let module = analyze_modules(&[
        (
            "Main.tz",
            "union Shape = Empty | Circle of f64
def remote :: Shapes.Shape -> i64
fn remote shape =
    match shape with
    | Shapes.Circle r -> to_int r
    | Shapes.Empty -> 0
def local :: Shape -> i64
fn local shape =
    match shape with
    | Shape.Circle r -> to_int r
    | Empty -> 1
remote (Shapes.Shape.Circle 2.0) + remote Shapes.Empty + local (Circle 3.0) + local Shape.Empty + Shapes.size (Shapes.Circle 4.0)",
        ),
        (
            "Shapes.tz",
            "union Shape = Empty | Circle of f64
def size :: Shape -> i64
fn size shape =
    match shape with
    | Circle r -> to_int r
    | Empty -> 0",
        ),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    llvm::emit(&module, llvm::Entry::Console).unwrap();

    // Unique cases from another module resolve without qualification.
    analyze_modules(&[
        ("Main.tz", "match Some 1 with\n| Some n -> n\n| None -> 0"),
        ("Choice.tz", "union Choice 'a = None | Some of 'a"),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    analyze_modules(&[
        (
            "Main.tz",
            "let x = Choice.Some 1\nmatch x with\n| Choice.Some n -> n\n| Choice.None -> 0",
        ),
        ("Choice.tz", "union Choice 'a = None | Some of 'a"),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));

    for (main, other, code, message) in [
        // `Choice.Some` names both a module case and a local union case.
        (
            "union Choice = Some\nlet x = Choice.Some\n0",
            "union Choice 'a = None | Some of 'a",
            "E1004",
            "Module.Union.Case",
        ),
        (
            "let x = Choice.Choice.Missing\n0",
            "union Choice 'a = None | Some of 'a",
            "E1002",
            "has no case 'Missing'",
        ),
        (
            "let x = Choice.Some 1\n0",
            "private union Choice 'a = None | Some of 'a",
            "E1022",
            "",
        ),
        (
            "def f :: Choice.Choice i64 -> i64\nfn f x = 0",
            "private union Choice 'a = None | Some of 'a",
            "E1022",
            "",
        ),
        (
            "0",
            "private union Hidden = Secret of i64\ndef leak :: Hidden -> i64\nfn leak h = 0",
            "E1022",
            "",
        ),
        (
            "0",
            "private record Hidden { value: i64 }\nunion Leak = Wrapped of Hidden",
            "E1022",
            "",
        ),
    ] {
        let error = analyze_modules(&[("Main.tz", main), ("Choice.tz", other)]).expect_err(main);
        assert_eq!(error.code, code, "{main}\n{}", error.message);
        assert!(error.message.contains(message), "{main}\n{}", error.message);
    }
    // Cases with one name in two other modules need qualification.
    let error = analyze_modules(&[
        ("Main.tz", "match Some 1 with\n| Some n -> n\n| None -> 0"),
        ("Choice.tz", "union Choice 'a = None | Some of 'a"),
        ("Other.tz", "union Other 'a = Some of 'a | Nothing"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004", "{}", error.message);
    assert!(
        error
            .message
            .contains("ambiguous union case 'Some'; qualify it as Choice.Some or Other.Some"),
        "{}",
        error.message
    );
}

#[test]
fn allows_unions_in_builders_but_not_type_class_files() {
    let module = analyze_modules(&[
        (
            "Maybe.tc",
            "union Maybe 'a = Nothing | Just of 'a
def Return :: 'a -> Maybe 'a
fn Return value = Just value
def Bind :: Maybe 'a -> ('a -> Maybe 'b) -> Maybe 'b
fn Bind value next =
    match value with
    | Just x -> next x
    | Nothing -> Nothing",
        ),
        (
            "Main.tz",
            "let result = Maybe {
    let! a = Maybe.Just 20
    let! b = Just 22
    return a + b
}
match result with
| Maybe.Just n -> n
| Nothing -> 0",
        ),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    llvm::emit(&module, llvm::Entry::Console).unwrap();
    let error = analyze_modules(&[("Classes.tt", "union U = A"), ("Main.tz", "0")]).unwrap_err();
    assert_eq!(error.code, "E1018", "{}", error.message);
    assert!(error.message.contains("unions"), "{}", error.message);
}

#[test]
fn rejects_invalid_declarations_patterns_and_uses() {
    for (source, code, message) in [
        ("union option = None", "E1024", "uppercase"),
        (
            "union U = none",
            "E1024",
            "lowercase names in patterns bind",
        ),
        (
            "union Option 'a = None",
            "E1024",
            "not used by any case payload",
        ),
        (
            "union Option = Some of 'a",
            "E1024",
            "is not declared by union 'Option'",
        ),
        (
            "union U 'a 'a = A of 'a",
            "E1024",
            "duplicate type parameter",
        ),
        (
            "union U = A | A",
            "E1024",
            "duplicate case 'A' in union 'U'",
        ),
        ("record None {}\nunion U = None", "E1001", ""),
        ("union U = A\nunion V = A", "E1001", ""),
        // A case and its own union share the type-and-case namespace.
        ("union Pair = Pair of i64 * i64", "E1001", "'Pair'"),
        ("union U = A\nunion U = B", "E1001", ""),
        ("record U { x: i64 }\nunion U = A", "E1001", ""),
        ("union U = Task", "E1001", ""),
        ("union Add 'a = Value of 'a", "E1001", ""),
        (
            "class C 'a { def f :: 'a -> i64 }\nunion C 'a = Value of 'a",
            "E1001",
            "",
        ),
        (
            "union C 'a = Value of 'a\nclass C 'a { def f :: 'a -> i64 }",
            "E1001",
            "",
        ),
        (
            "union U = Build\ndef Build :: i64\nfn Build = 1",
            "E1001",
            "",
        ),
        (
            "union U = Even of i64\ndef (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0",
            "E1001",
            "",
        ),
        (
            "union Maybe 'a = None | Some of 'a\nlet x = Some\n0",
            "E1015",
            "",
        ),
        (
            "union Maybe 'a = None | Some of 'a\nmatch Some 1 with\n| Some -> 0\n| None -> 1",
            "E1020",
            "carries a payload",
        ),
        (
            "union Maybe 'a = None | Some of 'a\nmatch None with\n| None x -> x\n| Some x -> x",
            "E1020",
            "has no payload",
        ),
        (
            "union Couple = Pair of i64 * i64\nmatch Pair (1, 2) with\n| Pair a b -> a + b",
            "E1020",
            "one tuple pattern",
        ),
        (
            "union Tree 'a = Leaf | Node of Tree 'a",
            "E1010",
            "recursive union layout for 'Main.Tree'",
        ),
        (
            "union Tree = Leaf | Node of [Tree]",
            "E1010",
            "recursive union layout",
        ),
        (
            "union U = A of i64\nexport def f :: U -> i64\nfn f u = 0",
            "E1008",
            "",
        ),
        (
            "union U = A of i64\nexport def f :: i64 -> U\nfn f x = A x",
            "E1008",
            "",
        ),
        (
            "union Maybe 'a = None | Some of 'a\nlet x = Some 1 == Some 1\n0",
            "E1005",
            "",
        ),
        (
            "union Color = Red | Green\nlet x = Red < Green\n0",
            "E1005",
            "",
        ),
        ("union Holder = Hold of &i64", "E1013", "owned values"),
        (
            "union Holder 'a = Hold of 'a\ndef f :: i64 -> i64\nfn f x =\n    let h = Hold &x\n    0",
            "E1013",
            "would store a reference",
        ),
        (
            "union Holder 'a = Hold of 'a\ndef f :: Holder (&mut i64) -> i64\nfn f h = 0",
            "E1013",
            "would store a reference",
        ),
        (
            "union Shape = Circle of f64\nlet s = Circle 1\n0",
            "E1003",
            "",
        ),
        (
            "union Shape = Circle of f64\nlet s = Circle 1.0 2.0\n0",
            "E1006",
            "",
        ),
        (
            "union U = A\ndef f :: U i64 -> i64\nfn f u = 0",
            "E1004",
            "takes no type arguments",
        ),
        (
            "union Maybe 'a = None | Some of 'a\ndef f :: Maybe -> i64\nfn f u = 0",
            "E1004",
            "expects 1 type argument, found 0",
        ),
        ("let x = Missing 1\n0", "E1002", "unknown value 'Missing'"),
        (
            "union Maybe 'a = None | Some of 'a\nmatch Some 1 with\n| Missing n -> n\n| _ -> 0",
            "E1020",
            "unknown union case or active pattern 'Missing'",
        ),
        (
            "union Maybe 'a = None | Some of 'a\ndef rec grow :: 'a -> i64\nfn rec grow x = grow (Some x)\ndef main :: i64\nfn main = grow 1",
            "E1017",
            "",
        ),
    ] {
        let error = rejects(source, code);
        assert!(
            error.message.contains(message),
            "{source}\n{}",
            error.message
        );
    }
    let error = parser::parse("let union = 1").unwrap_err();
    assert!(error.code.starts_with("E0"), "{}", error.message);
}

#[test]
fn tracks_payload_moves_copies_and_guards() {
    // Moving a payload out of a union consumes the union.
    rejects(
        &format!(
            "{MAYBE}def f :: Maybe string -> i64
fn f value =
    match value with
    | Some text ->
        let moved = text
        match value with
        | Some again -> again.length + moved.length
        | None -> 0
    | None -> 0
"
        ),
        "E1012",
    );
    rejects(
        &format!(
            "{MAYBE}def main :: i64
fn main =
    let value = Some \"a\"
    let moved = value
    match value with
    | Some text -> text.length
    | None -> 0
"
        ),
        "E1012",
    );
    for source in [
        // Copy payloads leave the union usable, and Copy unions are copied.
        format!(
            "{MAYBE}def main :: i64
fn main =
    let value = Some [1, 2, 3]
    let copied = value
    let first = match value with
        | Some items -> items.length
        | None -> 0
    let second = match copied with
        | Some items -> items.length
        | None -> 0
    first + second
"
        ),
        // A failed guard keeps the payload in the union for later arms.
        format!(
            "{MAYBE}def f :: i64 -> i64
fn f n =
    let value = Some \"guarded\"
    match value with
    | Some text when text.length > n -> text.length
    | Some text -> text.length + 100
    | None -> 0
"
        ),
        // Payloads can be matched as a whole and bound with `as`.
        format!(
            "{MAYBE}def f :: Maybe (i64 * string) -> i64
fn f value =
    match value with
    | Some (n, text) -> n + text.length
    | None -> 0
f (Some (1, \"ab\"))
"
        ),
        // Tasks may capture and return owned unions.
        format!(
            "{MAYBE}def main :: i64
fn main =
    let value = Some \"sent\"
    let job = task {{
        match value with
        | Some text -> Some text.length
        | None -> None
    }}
    match Task.run job with
    | Some n -> n
    | None -> 0
"
        ),
    ] {
        accepts(&source);
    }
}

#[test]
fn lowers_layouts_constructors_and_tag_switches() {
    let shapes = ir(&format!(
        "{SHAPE}def area :: Shape -> f64
fn area shape =
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0
export def scaled :: i64
fn scaled = to_int (area (Rect (3.0, 4.0)) * 10.0)
"
    ));
    assert!(shapes.contains("%\"tz.union.Main.Shape\" = type { i32, [1 x i128] }"));
    assert!(shapes.contains("switch i32"));
    // The general layout stores a payload through its field pointer, and a
    // match reads the tag from the subject slot.
    assert!(shapes.contains("insertvalue %\"tz.union.Main.Shape\" zeroinitializer, i32 1, 0"));
    let area = body(&shapes, "tz.fn.Main.area");
    assert_eq!(area.matches("load i32, ptr").count(), 1, "{area}");
    assert!(
        area.contains("getelementptr inbounds %\"tz.union.Main.Shape\", ptr"),
        "{area}"
    );
    assert!(area.contains("load double, ptr"), "{area}");
    assert!(
        area.contains("getelementptr inbounds { double, double }, ptr"),
        "{area}"
    );

    let colors = ir("union Color = Red | Green | Blue
def code :: Color -> i64
fn code color =
    match color with
    | Red -> 1
    | Green -> 2
    | Blue -> 3
export def green :: i64
fn green = code Green
");
    assert!(colors.contains("%\"tz.union.Main.Color\" = type i32"));
    assert!(colors.contains("switch i32"));
    // Enum values are their tags.
    assert!(
        body(&colors, "tz.fn.Main.green").contains("(%\"tz.union.Main.Color\" 1)"),
        "{colors}"
    );

    let maybe = ir(&format!(
        "{MAYBE}def unwrap_or :: 'a -> Maybe 'a -> 'a
fn unwrap_or fallback value =
    match value with
    | Some x -> x
    | None -> fallback
export def main :: i64
fn main =
    let text = unwrap_or \"\" (Some \"abc\")
    unwrap_or 0 (Some 1) + text.length
"
    ));
    for definition in [
        "%\"tz.union.Main.Maybe[i64]\" = type { i32, i64 }",
        "%\"tz.union.Main.Maybe[string]\" = type { i32, %tz.string }",
    ] {
        assert_eq!(
            maybe.matches(&format!("{definition}\n")).count(),
            1,
            "{definition}"
        );
    }
    assert!(!maybe.contains("%\"tz.union.Main.Maybe\" ="));
    assert!(!maybe.contains("%tz.union.Main.Maybe ="));
    // Direct constructor applications build the value without a call.
    assert!(!maybe.contains("$case"), "{maybe}");
}

#[test]
fn switch_plans_read_payload_projections() {
    let ir = ir("union U = A of string | B of i64 | C
def f :: U -> i64
fn f value =
    match value with
    | A text -> text.length
    | B n -> n
    | C -> 0
export def main :: i64
fn main = f (A \"abc\") + f (B 4) + f C
");
    assert!(ir.contains("%\"tz.union.Main.U\" = type { i32, [1 x i128] }"));
    let f = body(&ir, "tz.fn.Main.f");
    // The tag is loaded once for one switch over every case.
    assert_eq!(f.matches("load i32, ptr").count(), 1, "{f}");
    let dispatch = f.find("switch i32").unwrap();
    assert!(f[dispatch..].contains("i32 0, label"), "{f}");
    assert!(f[dispatch..].contains("i32 1, label"), "{f}");
    assert!(f[dispatch..].contains("i32 2, label"), "{f}");
    // Payload bindings read the payload field, not the whole union slot.
    assert!(
        f.contains("getelementptr inbounds %\"tz.union.Main.U\", ptr"),
        "{f}"
    );
    assert!(f.contains("load %tz.string, ptr"), "{f}");
    assert!(f.contains("load i64, ptr"), "{f}");

    // Guards fall back to ordered tests.
    let guarded = self::ir(&format!(
        "{MAYBE}def f :: Maybe i64 -> i64
fn f value =
    match value with
    | Some n when n > 3 -> n
    | Some n -> n + 100
    | None -> 0
export def main :: i64
fn main = f (Some 4) + f (Some 1) + f None
"
    ));
    assert!(!body(&guarded, "tz.fn.Main.f").contains("switch i32"));
}

#[test]
fn first_class_constructors_share_one_function_per_instance() {
    let ir = ir(&format!(
        "{MAYBE}def apply :: ('a -> 'b) -> 'a -> 'b
fn apply f x = f x
def value :: Maybe 'a -> 'a -> 'a
fn value m fallback =
    match m with
    | Some x -> x
    | None -> fallback
export def main :: i64
fn main =
    let wrap = Some
    let text = value (apply Some \"abc\") \"\"
    value (apply Some 1) 0 + value (wrap 2) 0 + value (apply wrap 3) 0 + text.length
"
    ));
    let constructors: Vec<&str> = ir
        .lines()
        .filter(|line| {
            line.starts_with("define internal ") && line.contains("@tz.fn.$case.Main.Maybe.Some")
        })
        .collect();
    assert_eq!(constructors.len(), 2, "{constructors:#?}");
    assert!(
        constructors
            .iter()
            .any(|line| line.starts_with("define internal %\"tz.union.Main.Maybe[i64]\""))
    );
    assert!(
        constructors
            .iter()
            .any(|line| line.starts_with("define internal %\"tz.union.Main.Maybe[string]\""))
    );
    // Recursion diagnostics run before hidden constructor functions exist.
    let error = rejects(
        &format!(
            "{MAYBE}def f :: i64 -> i64
fn f n =
    let wrap = Some
    match wrap n with
    | Some x -> f x
    | None -> 0
"
        ),
        "E1019",
    );
    assert!(error.message.contains("'Main.f'"), "{}", error.message);
    assert!(!error.message.contains("$case"), "{}", error.message);
}

#[test]
fn enforces_union_layout_limits() {
    let wide = |count: usize| {
        let fields = (0..count)
            .map(|index| format!("f{index}: i64"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("record Wide {{ {fields} }}\n")
    };
    // 16 tag bytes plus 4095 16-byte fields are exactly 64 KiB.
    let near = format!(
        "{}union NearLimit = Big of Wide | Small\ndef f :: NearLimit -> i64\nfn f value =\n    match value with\n    | Big w -> w.f0\n    | Small -> 0\nexport def main :: i64\nfn main = f Small",
        wide(4095)
    );
    for wasm in [false, true] {
        let ir = llvm::emit_target(&accepts(&near), llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("%\"tz.union.Main.NearLimit\" = type { i32, %tz.record.Main.Wide }"));
    }
    let error = rejects(
        &format!("{}union TooLarge = Big of Wide | Small", wide(4096)),
        "E1010",
    );
    assert!(error.message.contains("65536"), "{}", error.message);
    // Generic unions are measured per concrete instance.
    let error = rejects(
        &format!(
            "{}{MAYBE}def f :: Maybe Wide -> i64\nfn f value = 0",
            wide(4096)
        ),
        "E1010",
    );
    assert!(error.message.contains("65536"), "{}", error.message);
    accepts(&format!(
        "{}{MAYBE}def f :: Maybe Wide -> i64\nfn f value = 0\nexport def main :: i64\nfn main = 0",
        wide(4095)
    ));
}
