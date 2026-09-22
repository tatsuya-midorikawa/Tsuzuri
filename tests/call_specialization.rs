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
    for name in [
        "opt_consumed_string",
        "opt_mutable_bound_array",
        "opt_callee_replaced",
    ] {
        let worker = body(&ir, &format!("tz.fn.Optimization.{name}"));
        assert!(worker.contains("@tz.closure.clone"), "{name}");
        assert!(!worker.contains("@tz.specialized."), "{name}");
    }
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
