use std::fmt::Write;
use tsuzuri::{analyze, analyze_modules, llvm};

fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split("\n\n")
        .find(|block| {
            block.starts_with("define ")
                && block.lines().next().unwrap().contains(&format!("@{name}("))
        })
        .unwrap_or_else(|| panic!("missing {name}"))
}

#[test]
fn known_continuations_use_direct_workers_and_entry_block_storage() {
    let module = analyze_modules(&[
        ("Main.tz", "def run :: i64 -> i64\nfn run offset = Identity { let! x = 20; return x + offset }\nrun 22"),
        ("Identity.tc", include_str!("fixtures/computations/Identity.tc")),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let worker = body(&ir, "tz.fn.Main.run");
    assert!(worker.contains("@tz.specialized."));
    assert!(!worker.contains("@tz.alloc("));
    assert!(worker.rfind("alloca").unwrap() < worker.find("br label %loop").unwrap());
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
}

#[test]
fn standard_maybe_result_continuations_use_allocation_free_workers() {
    let module = analyze(
        "def option :: i64 -> i64
         fn option offset = Maybe.get (Maybe { let! x = Some 20; return x + offset })
         def result :: i64 -> i64
         fn result offset =
             let value: Result<i64, i64> = Result { let! x = Ok 20; return x + offset }
             Result.get value",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let mut visited = std::collections::BTreeSet::new();
        let mut pending = Vec::new();
        for name in ["option", "result"] {
            assert!(body(&ir, &format!("tz.fn.Main.{name}")).contains("@tz.specialized."));
            pending.push(format!("tz.fn.Main.{name}"));
        }
        // Generic adapters may allocate, but no such path may be reachable
        // from these fully applied, scalar-only builder calls.
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            for line in body(&ir, &name)
                .lines()
                .filter(|line| line.contains("call "))
            {
                let (_, call) = line.split_once("call ").unwrap();
                let (_, target) = call.split_once('@').expect("no indirect callback");
                let target = target.split('(').next().unwrap();
                assert_ne!(target, "tz.alloc", "{name}: {line}");
                assert_ne!(target, "tz.closure.clone", "{name}: {line}");
                if target.starts_with("tz.") {
                    pending.push(target.to_owned());
                }
            }
        }
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn owned_captures_and_dynamic_effectful_cases_use_their_correct_paths() {
    let module = analyze_modules(&[
        (
            "Optimization.tz",
            include_str!("fixtures/computations/Optimization.tz"),
        ),
        ("Skew.tc", include_str!("fixtures/computations/Skew.tc")),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    for name in [
        "opt_read_array",
        "opt_read_string",
        "opt_pipeline",
        "opt_initializer",
        "opt_mutual",
    ] {
        assert!(
            body(&ir, &format!("tz.fn.Optimization.{name}")).contains("@tz.specialized."),
            "{name}",
        );
    }
    for name in ["opt_consumed_string", "opt_mutable_bound_array"] {
        let worker = body(&ir, &format!("tz.fn.Optimization.{name}"));
        assert!(worker.contains("i1 true)"), "{name}");
        assert!(!worker.contains("@tz.specialized."), "{name}");
        let wrapper = worker
            .lines()
            .find_map(|line| {
                let (_, target) = line.split_once("ptr @")?;
                let target = target.split(',').next()?;
                target.starts_with("tz.apply.").then_some(target)
            })
            .expect("a captured closure has an apply wrapper");
        assert!(
            body(&ir, wrapper).contains("call ptr @tz.env.clone."),
            "{name}"
        );
    }
    let replaced = body(&ir, "tz.fn.Optimization.opt_callee_replaced");
    assert!(replaced.contains("@tz.closure.clone"));
    assert!(!replaced.contains("@tz.specialized."));
    let count = ir.matches("define internal i64 @tz.specialized.").count();
    assert!(count > 0 && count < 100);
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

#[test]
fn only_unobserved_single_use_copies_are_transferred() {
    let module = analyze(
        "def once :: [i64] -> [i64]\nfn once values = values
         def observed :: [i64] -> [i64]\nfn observed values = {
             let copy = values;
             assert (values.length == copy.length);
             copy
         }",
    )
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!body(&ir, "tz.fn.Main.once").contains("@tz.alloc("));
    assert!(body(&ir, "tz.fn.Main.observed").contains("@tz.alloc("));
}

#[test]
fn stores_single_scalar_captures_without_an_environment_allocation() {
    for scalar in ["bool", "i8", "i16u", "i32", "i64u", "i128u", "f32", "f64"] {
        let module = analyze(&format!(
            "def make :: bool -> {scalar} -> (unit -> {scalar})\nfn make flag value = if flag then \\unused -> value else \\unused -> value"
        )).unwrap();
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            let make = body(&ir, "tz.fn.Main.make");
            let width = match scalar {
                "i128u" => 128,
                "i64u" | "f64" => 64,
                _ => 32,
            };
            let fits = width <= if wasm { 32 } else { usize::BITS };
            assert_eq!(make.contains("inttoptr"), fits, "{scalar} wasm={wasm}");
            assert_eq!(make.contains("@tz.alloc("), !fits, "{scalar} wasm={wasm}");
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
}

#[test]
fn specialization_budget_falls_back_instead_of_growing_unbounded() {
    let mut source = String::from(
        "def apply :: (i64 -> i64) -> i64 -> i64\nfn apply body value = body value
         def probe :: i64 -> i64\nfn probe offset = {\nlet values = [offset];\nlet mut total = 0;\n",
    );
    for value in 0..520 {
        writeln!(
            source,
            "total = total + apply (x -> x + values[0]) {value};"
        )
        .unwrap();
    }
    source.push_str("total\n}");
    let module = analyze(&source).unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let specializations = ir
        .lines()
        .filter(|line| line.starts_with("define ") && line.contains("@tz.specialized."))
        .count();
    assert_eq!(specializations, 1024);
    assert!(body(&ir, "tz.fn.Main.probe").contains("@tz.specialized."));
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

/// The callee text of a `call` line: everything between `call ` and the argument list.
fn callee(line: &str) -> &str {
    let (_, call) = line.split_once("call ").unwrap();
    call.split_once('(').map_or(call, |(callee, _)| callee)
}

/// Every function that `roots` reach through calls. Each call on the way names its callee and the
/// callees never allocate, clone a closure, or go through a closure wrapper.
fn reachable_workers(ir: &str, roots: &[&str]) -> std::collections::BTreeSet<String> {
    let mut visited = std::collections::BTreeSet::new();
    let mut pending: Vec<String> = roots.iter().map(|name| (*name).to_owned()).collect();
    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        for line in body(ir, &name)
            .lines()
            .filter(|line| line.contains("call "))
        {
            let callee = callee(line);
            let (_, target) = callee
                .split_once('@')
                .unwrap_or_else(|| panic!("{name}: indirect call: {line}"));
            assert_ne!(target, "tz.alloc", "{name}: {line}");
            assert_ne!(target, "tz.closure.clone", "{name}: {line}");
            assert!(!target.starts_with("tz.apply."), "{name}: {line}");
            if target.starts_with("tz.") {
                pending.push(target.to_owned());
            }
        }
    }
    visited
}

#[test]
fn standard_let_chain_workers_are_direct_and_allocation_free() {
    // Two `let!` steps use both `Bind` (a continuation that binds again) and `BindReturn`.
    let module = analyze(
        "def run :: i64 -> i64
         fn run offset = Maybe.get (Maybe { let! x = Some 20; let! y = Some (x + offset); return x + y })
         def checked :: i64 -> i64
         fn checked offset =
             let value: Result<i64, i64> = Result { let! x = Ok 20; let! y = Ok (x + offset); return x + y }
             Result.get value",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        for name in ["run", "checked"] {
            let root = format!("tz.fn.Main.{name}");
            assert!(body(&ir, &root).contains("@tz.specialized."), "{name}");
            // Every worker reachable from the function is direct and allocation free.
            reachable_workers(&ir, &[&root]);
        }
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn known_local_function_values_call_directly() {
    let module = analyze(
        "def add3 :: i64 -> i64 -> i64 -> i64
         fn add3 a b c = a + b + c
         def let_twice :: i64 -> i64 -> i64
         fn let_twice a b = { let add = \\x -> x + a; add (add b) }
         def let_plain :: i64 -> i64
         fn let_plain b = { let inc = \\x -> x + 1; inc (inc b) }
         def partial :: i64 -> i64
         fn partial x = { let f = add3 1 2; f (f x) }",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        for name in ["let_twice", "let_plain", "partial"] {
            let function = body(&ir, &format!("tz.fn.Main.{name}"));
            let direct = function
                .lines()
                .filter(|line| line.contains("call "))
                .filter(|line| {
                    let callee = callee(line);
                    callee.contains("@tz.specialized.") || callee.contains("@tz.fn.")
                })
                .count();
            // Both calls name the lambda or its worker; neither goes through the closure's code pointer.
            assert!(direct >= 2, "{name} wasm={wasm}: {function}");
            assert!(!function.contains("call i64 %"), "{name} wasm={wasm}");
        }
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}
