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
            "def step :: i64 -> i64\nfn step x = x ^ (Bits.ushr x 13)
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
             def step :: i64 -> i64\nfn step x = x ^ (Bits.ushr x 13)
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
        "def step :: i64 -> i64\nfn step x = x ^ (Bits.ushr x 13)
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

// PM09: calls that the language guarantees to be tail calls are `musttail` calls.

const EVEN_ODD: &str = "def rec even :: i64 -> bool
    def and odd :: i64 -> bool
    fn rec even n = if n == 0 then true else odd (n - 1)
    and odd n = if n == 0 then false else even (n - 1)";

const CYCLE: &str = "def rec a :: i64 -> i64 -> i64
    def and b :: i64 -> i64 -> i64
    def and c :: i64 -> i64 -> i64
    fn rec a n acc = if n == 0 then acc else b (n - 1) (acc + 1)
    and b n acc = if n == 0 then acc else c (n - 1) (acc + 2)
    and c n acc = if n == 0 then acc else a (n - 1) (acc + 3)";

/// Each `musttail call` of `body` with the line after it.
fn musttail_lines(body: &str) -> Vec<(String, String)> {
    let lines: Vec<_> = body.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("musttail call"))
        .map(|(index, line)| ((*line).to_owned(), lines[index + 1].to_owned()))
        .collect()
}

/// The value that a line defines: `  %v5 = ...` is `%v5`.
fn defined(line: &str) -> &str {
    line.trim().split_once(" = ").unwrap().0
}

fn native(source: &str) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let first = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(first, llvm::emit(&module, llvm::Entry::Library).unwrap());
    first
}

/// What follows the header of the definition of `@tz.fn.Main.<name>` in `ir`, up to its closing brace:
/// the first mention of the name may be a call in a function of the same group.
fn definition(ir: &str, name: &str) -> String {
    let header = format!("@tz.fn.Main.{name}(");
    let start = ir
        .match_indices("define ")
        .map(|(at, _)| at)
        .find(|at| {
            let line = ir[*at..].lines().next().unwrap();
            line.contains(&header)
        })
        .unwrap_or_else(|| panic!("no definition of {name}\n{ir}"));
    let rest = &ir[start..];
    let body = rest.split_once(&header).unwrap().1;
    body.split("\n}").next().unwrap().to_owned()
}

#[test]
fn marks_mutual_tail_calls_musttail() {
    for (source, calls) in [
        (
            EVEN_ODD,
            vec![
                ("even", "i1 @tz.fn.Main.odd(i64"),
                ("odd", "i1 @tz.fn.Main.even(i64"),
            ],
        ),
        (
            CYCLE,
            vec![
                ("a", "i64 @tz.fn.Main.b(i64"),
                ("b", "i64 @tz.fn.Main.c(i64"),
                ("c", "i64 @tz.fn.Main.a(i64"),
            ],
        ),
    ] {
        for (name, call) in calls {
            let body = definition(&native(source), name);
            let found = musttail_lines(&body);
            assert_eq!(found.len(), 1, "{body}");
            let (line, next) = &found[0];
            assert!(line.contains(&format!("musttail call {call}")), "{body}");
            // The next line returns the value of the call, and nothing else comes between.
            assert_eq!(
                next.trim(),
                format!(
                    "ret {} {}",
                    if name == "even" || name == "odd" {
                        "i1"
                    } else {
                        "i64"
                    },
                    defined(line)
                ),
                "{body}"
            );
        }
    }
    // WebAssembly has no tail calls unless the build asks for them: the same program has none.
    for source in [EVEN_ODD, CYCLE] {
        let module = analyze(source).unwrap();
        let ir = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
        assert!(!ir.contains("musttail"), "{ir}");
        assert!(ir.contains("call i"), "{ir}");
    }
}

#[test]
fn drops_owned_locals_before_guaranteed_tail_calls() {
    // The strings are heap strings: a function of a recursion keeps no buffer in its frame.
    let source = "def rec ping :: i64 -> i64
        def and pong :: i64 -> i64
        fn rec ping n = {
            let s = (if n > 0 then \"ab\" else \"abc\") + \"x\";
            if n == 0 then s.length else pong (n - 1)
        }
        and pong n = {
            let s = (if n > 0 then \"cd\" else \"cde\") + \"y\";
            if n == 0 then s.length else ping (n - 1)
        }";
    for name in ["ping", "pong"] {
        let body = definition(&native(source), name);
        let lines: Vec<_> = body.lines().collect();
        let call = lines
            .iter()
            .position(|line| line.contains("musttail call"))
            .unwrap();
        // The string that the function owns is freed before the call, and the `ret` follows it.
        let freed = lines[..call]
            .iter()
            .rposition(|line| line.contains("call void @tz.free("))
            .unwrap();
        let argument = lines[..freed]
            .iter()
            .rposition(|line| line.contains("sub i64"))
            .unwrap();
        assert!(argument < freed && freed < call, "{body}");
        assert!(lines[call + 1].trim().starts_with("ret i64 "), "{body}");
        // The call is the only call after the drops: the order is arguments, drops, call, ret.
        assert!(
            lines[freed + 1..call]
                .iter()
                .all(|line| !line.contains("call ")),
            "{body}"
        );
        // The other way out frees the string before it returns, as an ordinary function does.
        assert!(
            lines
                .iter()
                .any(|line| line.contains("ret i64 ") && !line.contains("musttail")),
            "{body}"
        );
    }
}

#[test]
fn keeps_ordinary_calls_outside_the_guarantee() {
    for (what, source, names) in [
        (
            "a function of another group",
            format!("{EVEN_ODD}\ndef asks :: i64 -> bool\nfn asks n = even n"),
            vec!["asks"],
        ),
        (
            "a result that is not a scalar",
            "def rec f :: i64 -> string
             def and g :: i64 -> string
             fn rec f n = if n == 0 then \"x\" else g (n - 1)
             and g n = if n == 0 then \"y\" else f (n - 1)"
                .to_owned(),
            vec!["f", "g"],
        ),
        (
            "a parameter that holds a borrow",
            "def rec f :: &i64 -> i64 -> i64
             def and g :: &i64 -> i64 -> i64
             fn rec f value n = if n == 0 then *value else g value (n - 1)
             and g value n = if n == 0 then *value else f value (n - 1)"
                .to_owned(),
            vec!["f", "g"],
        ),
        (
            "different parameter types",
            "def rec f :: i64 -> bool
             def and g :: i32 -> bool
             fn rec f n = if n == 0 then true else g (n as i32)
             and g m = if m == 0 then false else f (m as i64)"
                .to_owned(),
            vec!["f", "g"],
        ),
        (
            "a call through a function value",
            "def rec f :: i64 -> i64
             fn rec f n = { let next = \\i -> f (i - 1); if n == 0 then 0 else next n }"
                .to_owned(),
            vec!["f"],
        ),
        (
            "a call that is not the last thing",
            "def rec f :: i64 -> i64
             def and g :: i64 -> i64
             fn rec f n = if n == 0 then 0 else g (n - 1) + 1
             and g n = if n == 0 then 0 else f (n - 1) + 1"
                .to_owned(),
            vec!["f", "g"],
        ),
        (
            "a different number of parameters",
            "def rec f :: i64 -> bool
             def and g :: i64 -> i64 -> bool
             fn rec f n = if n == 0 then true else g (n - 1) 1
             and g n m = if n == 0 then false else f (n - m)"
                .to_owned(),
            vec!["f", "g"],
        ),
    ] {
        let source = analyze(&source).unwrap_or_else(|error| panic!("{what}\n{error:?}"));
        let ir = llvm::emit(&source, llvm::Entry::Library).unwrap();
        for name in names {
            let body = definition(&ir, name);
            assert!(!body.contains("musttail"), "{what}\n{body}");
        }
    }
    // A tail call of the function itself is a jump back to its loop, and a call through a name of a
    // function of the group is a `musttail` call.
    let body = function_ir(
        "def rec f :: i64 -> i64 -> i64
         fn rec f n acc = if n == 0 then acc else f (n - 1) (acc + 1)",
        "f",
    );
    assert!(
        !body.contains("musttail") && body.contains("br label %loop"),
        "{body}"
    );
}

#[test]
fn self_tail_calls_stay_loops() {
    // `f` calls itself and `g`: the call of itself is a jump to its loop, as it was, and the call of `g` is
    // a `musttail` call.
    let ir = native(
        "def rec f :: i64 -> i64 -> i64
         def and g :: i64 -> i64 -> i64
         fn rec f n acc = if n == 0 then acc else if n % 2 == 0 then f (n - 1) (acc + 1) else g (n - 1) (acc + 2)
         and g n acc = if n == 0 then acc else f (n - 1) (acc + 3)",
    );
    let body = definition(&ir, "f");
    let found = musttail_lines(&body);
    assert_eq!(found.len(), 1, "{body}");
    assert!(found[0].0.contains("@tz.fn.Main.g("), "{body}");
    assert!(!body.contains("@tz.fn.Main.f("), "{body}");
    assert!(body.contains("br label %loop"), "{body}");
    let body = definition(&ir, "g");
    assert!(
        musttail_lines(&body)[0].0.contains("@tz.fn.Main.f("),
        "{body}"
    );
}

#[test]
fn pipes_into_a_function_of_the_group_are_tail_calls() {
    let ir = native(
        "def rec even :: i64 -> bool
         def and odd :: i64 -> bool
         fn rec even n = if n == 0 then true else (n - 1) |> odd
         and odd n = if n == 0 then false else (n - 1) |> even",
    );
    for name in ["even", "odd"] {
        let body = definition(&ir, name);
        assert_eq!(musttail_lines(&body).len(), 1, "{body}");
    }
}

#[test]
fn native_trap_context_and_debug_builds_keep_the_prototypes_equal() {
    let module = analyze(EVEN_ODD).unwrap();
    let sources = [tsuzuri::trap::TrapSource {
        path: "Main.tz",
        text: EVEN_ODD,
    }];
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    // Trap information gives each function an `i32` context as its last parameter and each call an
    // `i32` argument: the prototypes still match, and the line after the call is still the `ret`.
    let traced = llvm::emit_with_trap_info(&module, options, &sources).unwrap();
    for (name, other) in [("even", "odd"), ("odd", "even")] {
        let body = definition(&traced.ir, name);
        assert!(
            body.starts_with("i64 noundef %arg0, i32 %tz.context)"),
            "{body}"
        );
        let found = musttail_lines(&body);
        assert_eq!(found.len(), 1, "{body}");
        assert!(
            found[0]
                .0
                .contains(&format!("musttail call i1 @tz.fn.Main.{other}(i64 %v")),
            "{body}"
        );
        assert!(
            found[0].0.contains(", i32 ") && found[0].0.trim_end().ends_with(')'),
            "{body}"
        );
        assert!(found[0].1.trim().starts_with("ret i1 "), "{body}");
        assert!(!found[0].0.contains("!tz.site"), "{body}");
    }
    // Line tables put a location on the call and on the `ret`; nothing else comes between them.
    let debugged = llvm::emit_with_debug_info(&module, options, &sources, false, false).unwrap();
    let body = definition(&debugged.ir, "even");
    let found = musttail_lines(&body);
    assert_eq!(found.len(), 1, "{body}");
    assert!(
        found[0].0.contains("!dbg") && found[0].1.contains("ret i1") && found[0].1.contains("!dbg")
    );
}
