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
    assert!(!worker.contains("call ptr @tz.alloc"));
    assert!(worker.rfind("alloca").unwrap() < worker.find("br label %loop").unwrap());
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
}

#[test]
fn standard_option_result_continuations_use_allocation_free_workers() {
    let module = analyze(
        "def option :: i64 -> i64
         fn option offset = Option.get (Option { let! x = Some 20; return x + offset })
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
    assert!(!body(&ir, "tz.fn.Main.once").contains("call ptr @tz.alloc"));
    assert!(body(&ir, "tz.fn.Main.observed").contains("call ptr @tz.alloc"));
}

#[test]
fn stores_single_scalar_captures_without_an_environment_allocation() {
    for scalar in ["bool", "i8", "i16u", "i32", "i64u", "i128u", "f32", "f64"] {
        let module = analyze(&format!(
            "def make :: bool -> {scalar} -> (unit -> {scalar})\nfn make flag value = if flag then fx unused -> value else fx unused -> value"
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
            assert_eq!(
                make.contains("call ptr @tz.alloc"),
                !fits,
                "{scalar} wasm={wasm}"
            );
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
