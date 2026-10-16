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

fn returns(function: &str) -> usize {
    function
        .lines()
        .filter(|line| line.starts_with("  ret "))
        .count()
}

#[test]
fn union_results_return_through_one_slot() {
    let module = analyze(
        "union Color = Red | Green | Blue
         union Chain = End | Link of (i64 * Chain)
         def make :: bool -> i64 -> Maybe<i64>
         fn make flag value = if flag then Some value else None
         def pick :: i64 -> Result<i64, string>
         fn pick n = match n with | 0 -> Error \"zero\" | _ -> Ok (n * 2)
         def helper :: i64 -> Maybe<i64>
         fn helper x = Some (x * 3)
         def wrap :: bool -> i64 -> Maybe<i64>
         fn wrap flag x = if flag then helper x else None
         def single :: i64 -> Maybe<i64>
         fn single x = Some (x + 1)
         def scalar :: bool -> i64
         fn scalar flag = if flag then 1 else 2
         def color :: i64 -> Color
         fn color n = if n == 0 then Red elif n == 1 then Green else Blue
         def chain :: i64 -> Chain
         fn chain n = if n == 0 then End else Link (n, End)
         def big :: bool -> Maybe<[i64; 200]>
         fn big flag = if flag then Some (FixedArray.init (\\i -> i)) else None",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        // Two exits that construct or call an acyclic function share one slot and one `ret`.
        for name in ["make", "pick", "wrap"] {
            let function = body(&ir, &format!("tz.fn.Main.{name}"));
            assert_eq!(
                function.matches("%tz.return.slot = alloca").count(),
                1,
                "{name} wasm={wasm}: {function}"
            );
            assert_eq!(function.matches("tz.return:\n").count(), 1, "{name}");
            assert_eq!(returns(function), 1, "{name} wasm={wasm}: {function}");
            // The slot is an entry-block allocation, like the other static frames.
            assert!(
                function.find("%tz.return.slot = alloca").unwrap()
                    < function.find("br label %loop").unwrap(),
                "{name}"
            );
        }
        // One exit, a scalar or bare-tag result, a recursive union, and a result bigger than the
        // slot limit keep their plain `ret`s.
        for (name, exits) in [
            ("single", 1),
            ("helper", 1),
            ("scalar", 2),
            ("color", 3),
            ("chain", 2),
            ("big", 2),
        ] {
            let function = body(&ir, &format!("tz.fn.Main.{name}"));
            assert!(!function.contains("tz.return"), "{name} wasm={wasm}");
            assert_eq!(returns(function), exits, "{name} wasm={wasm}: {function}");
        }
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn tail_forwarding_to_a_cycle_keeps_every_ret() {
    // LLVM makes a sibling call or a loop out of `ret (call f)` only; a slot would hide the call.
    let module = analyze(
        "def rec even_m :: i64 -> Maybe<i64> = \\n -> if n == 0 then Some 1 else odd_m (n - 1)
         and odd_m :: i64 -> Maybe<i64> = \\n -> if n == 0 then None else even_m (n - 1)
         def rec count_down :: i64 -> Maybe<i64>
         fn rec count_down n = if n == 0 then Some 7 else Maybe.bind (Some (n - 1)) (\\m -> count_down m)
         def apply_to :: (i64 -> Maybe<i64>) -> i64 -> Maybe<i64>
         fn apply_to f x = if x < 0 then None else f x
         def wrap :: bool -> i64 -> Maybe<i64>
         fn wrap flag x = if flag then even_m x else None",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        // A function in a cycle, a call of one, and a call of an unknown function.
        for name in ["even_m", "odd_m", "count_down", "apply_to", "wrap"] {
            let function = body(&ir, &format!("tz.fn.Main.{name}"));
            assert!(!function.contains("tz.return"), "{name} wasm={wasm}");
            assert_eq!(returns(function), 2, "{name} wasm={wasm}: {function}");
        }
        let function = body(&ir, "tz.fn.Main.even_m");
        let lines: Vec<_> = function.lines().collect();
        let call = lines
            .iter()
            .position(|line| line.contains("call ") && line.contains("@tz.fn.Main.odd_m("))
            .unwrap();
        let value = lines[call].split(" = ").next().unwrap().trim();
        assert!(lines[call + 1].starts_with("  ret "), "{function}");
        assert!(lines[call + 1].ends_with(value), "{function}");
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

/// `chains` independent functions, each with its own two-`let!` Maybe chain (about 8 workers each).
fn chain_source(chains: usize) -> String {
    let mut source = String::from(
        "def pick :: i64 -> i64 -> Maybe<i64>
         fn pick state salt = if (state & 7) == 0 then None else Some (state ^ salt)\n",
    );
    for index in 0..chains {
        writeln!(
            source,
            "def step{index} :: i64 -> i64
             fn step{index} state = Maybe.default_value (-1) (Maybe {{ let! a = pick (state + {index}) 1; let! b = pick a 2; return a + b }})"
        )
        .unwrap();
    }
    source
}

/// The ids of the worker definitions, in the order the IR defines them.
fn worker_ids(ir: &str) -> Vec<usize> {
    ir.lines()
        .filter(|line| line.starts_with("define "))
        .filter_map(|line| line.split_once("@tz.specialized."))
        .map(|(_, rest)| rest.split('(').next().unwrap().parse().unwrap())
        .collect()
}

#[test]
fn exhausted_worker_budget_completes_chains_instead_of_cutting_all_of_them() {
    // 600 chains need far more than the 1,024 workers of the budget. Emitting the requests in order
    // cut every chain part-way, which left an indirect call in each of them; the chains of the first
    // functions are now complete and the later ones keep the generic path.
    let module = analyze(&chain_source(600)).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(worker_ids(&ir).len(), 1024, "wasm={wasm}");
        for index in [0, 50] {
            let root = format!("tz.fn.Main.step{index}");
            assert!(body(&ir, &root).contains("@tz.specialized."), "{index}");
            reachable_workers(&ir, &[&root]);
        }
        assert!(
            !body(&ir, "tz.fn.Main.step599").contains("@tz.specialized."),
            "wasm={wasm}"
        );
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

#[test]
fn workers_within_the_budget_are_emitted_in_request_order() {
    let module = analyze(&chain_source(40)).unwrap();
    let ir = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    let ids = worker_ids(&ir);
    assert!(ids.len() > 200 && ids.len() < 1024, "{}", ids.len());
    assert!(ids.windows(2).all(|pair| pair[0] + 1 == pair[1]), "{ids:?}");
    reachable_workers(&ir, &["tz.fn.Main.step0", "tz.fn.Main.step39"]);
}
