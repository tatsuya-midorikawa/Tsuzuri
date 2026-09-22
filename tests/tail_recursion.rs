use tsuzuri::{analyze, llvm};

fn function_ir(source: &str, name: &str) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let first = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(first, llvm::emit(&module, llvm::Entry::Library).unwrap());
    first
        .split_once(&format!("@tz.fn.Main.{name}("))
        .unwrap()
        .1
        .split("\n}")
        .next()
        .unwrap()
        .to_owned()
}

#[test]
fn emits_integer_argument_arithmetic_after_operand_evaluation() {
    for update in ["n - 1", "Sub.sub n 1", "{ n - 1 }", "subtract n 1"] {
        let source = format!(
            "def step :: i64 -> i64\nfn step x = x ^ (x >>> 13)
             def subtract :: i64 -> i64 -> i64\nfn subtract x y = {{ x - y }}
             def rec f :: i64 -> i64 -> i64
             fn rec f n state = match n with | 0 -> state | n -> f ({update}) (step state)"
        );
        let body = function_ir(&source, "f");
        assert!(!body.contains("call i64 @tz.fn.Main.f("));
        assert!(
            body.find("call i64 @tz.fn.Main.step(").unwrap() < body.find("sub i64").unwrap(),
            "{update}\n{body}"
        );
        assert!(!body.contains("call i64 @tz.fn.Main.subtract("), "{update}");
        assert!(!body.contains("add nsw") && !body.contains("sub nsw"));
        assert!(!body.contains("nuw") && !body.contains("llvm.assume"));
        assert!(body.split("loop:").nth(1).unwrap().find("alloca").is_none());
    }
}

#[test]
fn snapshots_both_operands_before_later_argument_effects() {
    let source = "
        def mark :: &mut i64 -> i64 -> i64
        fn mark p digit = { *p = *p * 10 + digit; *p }
        def last :: &mut i64 -> i64
        fn last p = { *p = *p * 10 + 3; *p }
        def rec f :: i64 -> i64 -> i64 -> i64
        fn rec f n mut state observed =
            if n == 0 then state + observed else
            f (n - 1) (mark (&mut state) 1 + mark (&mut state) 2) (last (&mut state))
    ";
    let body = function_ir(source, "f");
    let marks: Vec<_> = body.match_indices("call i64 @tz.fn.Main.mark(").collect();
    let last = body.find("call i64 @tz.fn.Main.last(").unwrap();
    assert_eq!(marks.len(), 2);
    assert!(marks[0].0 < marks[1].0 && marks[1].0 < last);
    assert!(last < body.find("sub i64").unwrap());
    assert!(last < body.rfind("add i64").unwrap());
}

#[test]
fn preserves_trapping_operations_and_nontrivial_helpers() {
    for (helper, operator, call) in [
        ("", "n / 2", "sdiv i64"),
        ("", "n % 2", "srem i64"),
        (
            "def checked :: i64 -> i64 -> i64\nfn checked x y = { assert (x > 0); x - y }",
            "checked n 1",
            "call i64 @tz.fn.Main.checked(",
        ),
    ] {
        let source = format!(
            "{helper}
             def step :: i64 -> i64\nfn step x = x ^ (x >>> 13)
             def rec f :: i64 -> i64 -> i64
             fn rec f n state = if n == 0 then state else f ({operator}) (step state)"
        );
        let body = function_ir(&source, "f");
        assert!(
            body.find(call).unwrap() < body.find("call i64 @tz.fn.Main.step(").unwrap(),
            "{body}"
        );
    }
}

#[test]
fn preserves_floating_arithmetic_order_and_ordinary_call_fallbacks() {
    let body = function_ir(
        "def step :: i64 -> i64\nfn step n = n - 1
         def rec f :: f64 -> i64 -> f64
         fn rec f state n = if n == 0 then state else f (state + 0.5) (step n)",
        "f",
    );
    assert!(body.find("fadd double").unwrap() < body.find("call i64 @tz.fn.Main.step(").unwrap());
    let borrowed = function_ir(
        "def rec f :: &i64 -> i64 -> i64
         fn rec f value n = if n == 0 then *value else f value (n - 1)",
        "f",
    );
    assert!(borrowed.contains("call i64 @tz.fn.Main.f("));
    let ordinary = function_ir(
        "def step :: i64 -> i64\nfn step n = n ^ 13
         def rec f :: i64 -> i64 -> i64
         fn rec f n state = if n == 0 then state else f (n - 1) (step state) + 1",
        "f",
    );
    assert!(ordinary.contains("call i64 @tz.fn.Main.f("));
    assert!(
        ordinary.find("sub i64").unwrap() < ordinary.find("call i64 @tz.fn.Main.step(").unwrap()
    );
}

#[test]
fn keeps_owned_temporaries_in_entry_and_drops_on_back_edges() {
    let module = analyze(include_str!("fixtures/control/Recursion.tz")).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        for name in ["owned", "temporaries"] {
            let body = ir
                .split_once(&format!("@tz.fn.Main.{name}("))
                .unwrap()
                .1
                .split("\n}")
                .next()
                .unwrap();
            assert!(body.contains("call void @tz.free") || body.contains("@tz.fn.Main.length"));
            assert!(!body.split("loop:").nth(1).unwrap().contains("alloca"));
            assert!(!body.contains(&format!("call i64 @tz.fn.Main.{name}(")));
        }
    }
}

#[test]
fn wasm_keeps_stack_machine_argument_scheduling() {
    let module = analyze(
        "def step :: i64 -> i64\nfn step x = x ^ (x >>> 13)
             def rec f :: i64 -> i64 -> i64
             fn rec f n state = if n == 0 then state else f (n - 1) (step state)",
    )
    .unwrap();
    let ir = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    let body = ir
        .split_once("@tz.fn.Main.f(")
        .unwrap()
        .1
        .split("\n}")
        .next()
        .unwrap();
    assert!(!body.contains("call i64 @tz.fn.Main.f("));
    assert!(body.find("sub i64").unwrap() < body.find("call i64 @tz.fn.Main.step(").unwrap());
}
