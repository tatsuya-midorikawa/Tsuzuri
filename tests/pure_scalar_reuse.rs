use tsuzuri::{analyze, llvm};

fn ir(source: &str) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let emit = || llvm::emit(&module, llvm::Entry::Library).unwrap();
    let ir = emit();
    assert_eq!(ir, emit());
    ir
}

fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    let marker = format!("@tz.fn.Main.{name}(");
    ir.split("\n}")
        .find(|body| {
            body.lines()
                .any(|line| line.starts_with("define internal ") && line.contains(&marker))
        })
        .unwrap_or_else(|| panic!("missing {name}\n{ir}"))
}

#[test]
fn scalar_reuse_requires_both_explicit_purity_and_independent_memory_proof() {
    for (qualifier, count) in [("", 2), ("pure ", 1)] {
        let ir = ir(&format!(
            "{qualifier}def f :: own i64 -> i64 = \\value -> value * 2 + 1\n\
             export def sum :: own i64 -> i64 = \\value -> f value + f value\n"
        ));
        assert!(function(&ir, "f").contains("memory(none)"));
        assert_eq!(
            function(&ir, "sum")
                .matches("call i64 @tz.fn.Main.f(")
                .count(),
            count,
            "{ir}"
        );
    }
    let ir = ir(
        "pure def f :: own i64 -> i64 = \\value -> { assert (value != 0); value * 2 }\n\
         export def sum :: own i64 -> i64 = \\value -> f value + f value\n",
    );
    assert!(!function(&ir, "f").contains("memory(none)"));
    assert_eq!(
        function(&ir, "sum")
            .matches("call i64 @tz.fn.Main.f(")
            .count(),
        2
    );
}

#[test]
fn scalar_reuse_never_crosses_blocks_or_changes_arguments_or_discards_the_first_call() {
    let prefix = "pure def f :: own i64 -> i64 = \\value -> value * 2 + 1\n";
    for (body, count) in [
        ("{ f value; f value; 0 }", 1),
        (
            "{ let first = f value; if value > 0 then first + f value else first + f value }",
            3,
        ),
        (
            "{ let mut current = value; let first = f current; current = current + 1; first + f current }",
            2,
        ),
    ] {
        let ir = ir(&format!(
            "{prefix}export def sum :: own i64 -> i64 = \\value -> {body}\n"
        ));
        assert_eq!(
            function(&ir, "sum")
                .matches("call i64 @tz.fn.Main.f(")
                .count(),
            count,
            "{ir}"
        );
    }
}

#[test]
fn weakening_pure_contracts_preserves_same_stage_direct_specialization() {
    for qualifier in ["", "pure "] {
        let ir = ir(&format!(
            "{qualifier}def increment :: own i64 -> i64 = \\value -> value + 1\n\
             def apply :: own (i64 -> i64) -> own i64 -> i64 = \\callback value -> callback value\n\
             export def sum :: own i64 -> i64 = \\value -> apply increment value\n"
        ));
        assert!(function(&ir, "sum").contains("@tz.specialized."), "{ir}");
    }
}

#[test]
fn scalar_reuse_does_not_share_owned_results_or_reference_reads_or_float_calls() {
    for source in [
        "pure def f :: own i64 -> [i64] = \\value -> [value]\n\
         export def sum :: own i64 -> i64 = \\value -> { let left = f value; let right = f value; left[0] + right[0] }\n",
        "pure def f :: ref i64 -> i64 = \\value -> (deref value) * 2 + 1\n\
         export def sum :: own i64 -> i64 = \\value -> f (ref value) + f (ref value)\n",
        "pure def f :: own f64 -> f64 = \\value -> value * 2.0 + 1.0\n\
         export def sum :: own f64 -> f64 = \\value -> f value + f value\n",
    ] {
        let ir = ir(source);
        assert_eq!(
            function(&ir, "sum")
                .lines()
                .filter(|line| line.contains("call ") && line.contains("@tz.fn.Main.f("))
                .count(),
            2,
            "{ir}"
        );
    }
}
