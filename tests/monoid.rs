//! `Monoid`, `Monoids` and the float-instance rejection E1039 (F15 Phase 1).

use std::process::{Command, Output};

use tsuzuri::{analyze, analyze_modules, llvm};

const MEAN: &str = r#"record Mean { total: f64, count: i64 }
instance Monoid<Mean> {
    let empty = \() -> Mean { total: 0.0, count: 0 }
    fn combine left right = Mean { total: left.total + right.total, count: left.count + right.count }
}
"#;

const PAIR: &str = r#"record Pair { left: i64, right: f32 }
instance Monoid<Pair> {
    let empty = \() -> Pair { left: 0, right: 0.0 }
    fn combine left right = Pair { left: left.left + right.left, right: left.right + right.right }
}
"#;

const OPTIONAL: &str = r#"record Optional { value: Maybe<f64> }
instance Monoid<Optional> {
    let empty = \() -> Optional { value: Maybe.None }
    fn combine _left right = right
}
"#;

const MONEY: &str = r#"record Money { amount: d64 }
instance Monoid<Money> {
    let empty = \() -> Money { amount: 0.0m }
    fn combine left right = Money { amount: left.amount + right.amount }
}
"#;

const LANES: &str = r#"record Lanes { value: f32x4 }
instance Monoid<Lanes> {
    let empty = \() -> Lanes { value: Simd.splat 0.0f32 }
    fn combine left right = Lanes { value: left.value + right.value }
}
"#;

const FIXED: &str = r#"record Fixed { micros: i64 }
instance Monoid<Fixed> {
    let empty = \() -> Fixed { micros: 0 }
    fn combine left right = Fixed { micros: left.micros + right.micros }
}
"#;

fn e1039_message(ty: &str) -> String {
    format!(
        "no Monoid instance for {ty}: it holds a floating-point number, whose operations are not associative, so Parallel.fold could differ from Array.fold_monoid; accumulate in integers (fixed point) or use Parallel.sum for a documented order"
    )
}

fn emits(source: &str) {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
    }
}

#[test]
fn float_monoids_are_rejected_where_they_are_used() {
    let cases = [
        (format!("{MEAN}let means = [Mean {{ total: 1.5, count: 1 }}]\nlet total = Array.fold_monoid (ref means)\ntotal.count"), "Main.Mean", "Array.fold_monoid"),
        (format!("{PAIR}let values = [Pair {{ left: 1, right: 1.0 }}]\nlet total = Parallel.fold (ref values)\ntotal.left"), "Main.Pair", "Parallel.fold"),
        (format!("{OPTIONAL}let values = [Optional {{ value: Maybe.Some 1.0 }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Optional", "Array.fold_monoid"),
        (format!("{MONEY}let values = [Money {{ amount: 1.5m }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Money", "Array.fold_monoid"),
        (format!("{LANES}let values = [Lanes {{ value: Simd.splat 1.0f32 }}]\nlet total = Array.fold_monoid (ref values)\n0"), "Main.Lanes", "Array.fold_monoid"),
        ("let values = [Monoids.Sum { value: 1.5 }, Monoids.Sum { value: 2.5 }]\nlet total = Parallel.fold (ref values)\ntotal.value".to_string(), "Monoids.Sum<f64>", "Parallel.fold"),
        ("let left = Monoids.Sum { value: 1.5 }\nlet right = Monoids.Sum { value: 2.5 }\nlet both = Monoid.combine left right\nboth.value".to_string(), "Monoids.Sum<f64>", "Monoid.combine"),
        ("let values = [Monoids.MaxOf { value: Maybe.Some 1.5 }]\nlet total = Array.fold_monoid (ref values)\n0".to_string(), "Monoids.MaxOf<f64>", "Array.fold_monoid"),
    ];
    for (source, ty, call) in cases {
        let error = analyze(&source).expect_err(&source);
        assert_eq!(error.code, "E1039", "{source}\n{}", error.message);
        assert_eq!(error.message, e1039_message(ty), "{source}");
        assert_eq!(error.span.start, source.find(call).unwrap(), "{source}");
    }
}

#[test]
fn integer_monoids_and_unused_float_instances_are_accepted() {
    for source in [
        format!("{FIXED}let values = [Fixed {{ micros: 1 }}, Fixed {{ micros: 2 }}]\nlet total = Array.fold_monoid (ref values)\nlet parallel = Parallel.fold (ref values)\ntotal.micros + parallel.micros"),
        format!("{MEAN}0"),
        "let values = [Monoids.Sum { value: 1 }, Monoids.Sum { value: 2 }]\nlet total = Parallel.fold (ref values)\ntotal.value".to_string(),
        "let values = [Monoids.MaxOf { value: Maybe.Some 1 }, Monoids.MaxOf { value: Maybe.None }]\nlet total = Array.fold_monoid (ref values)\nmatch total.value with\n| Maybe.Some value -> value\n| Maybe.None -> 0".to_string(),
        "let values = [Monoids.All { value: true }, Monoids.All { value: false }]\nlet total = Parallel.fold (ref values)\nif total.value then 1 else 0".to_string(),
    ] {
        emits(&source);
    }
}

#[test]
fn monoids_are_qualified_and_the_module_names_are_reserved() {
    let error = analyze("let total = Sum { value: 1 }\ntotal.value").unwrap_err();
    assert_eq!(error.code, "E1004", "{}", error.message);
    for name in ["Monoids.tz", "Monoid.tt"] {
        let error = analyze_modules(&[("Main.tz", "0"), (name, "")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
        assert_eq!(
            error.message,
            format!(
                "module name '{}' is reserved for the standard library; rename the file",
                name.split('.').next().unwrap()
            ),
            "{name}"
        );
    }
}

fn run(level: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", "tests/fixtures/monoid", level])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn law_tests_pass_at_o0_and_o3() {
    for level in ["-O0", "-O3"] {
        let output = run(level);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{level}\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("11 passed; 0 failed; 0 ignored"),
            "{level}\n{stdout}"
        );
    }
}
