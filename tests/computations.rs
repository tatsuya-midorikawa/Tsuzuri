use tsuzuri::{analyze_modules, llvm};

const IDENTITY: &str = include_str!("fixtures/computations/Identity.tc");
const CHOICE: &str = include_str!("fixtures/computations/Choice.tc");
const FLOW: &str = include_str!("fixtures/computations/Flow.tc");
const LAZY: &str = include_str!("fixtures/computations/Lazy.tc");
const TEXT: &str = include_str!("fixtures/computations/Text.tc");

fn sources(main: &str) -> [(&str, &str); 6] {
    [
        ("Identity.tc", IDENTITY),
        ("Choice.tc", CHOICE),
        ("Flow.tc", FLOW),
        ("Lazy.tc", LAZY),
        ("Text.tc", TEXT),
        ("Main.tz", main),
    ]
}

fn accepts(source: &str) -> String {
    let module = analyze_modules(&sources(source))
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    ir
}

fn rejects(source: &str, code: &str) -> tsuzuri::diagnostic::Diagnostic {
    let error = analyze_modules(&sources(source)).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error
}

#[test]
fn composes_generic_bind_return_return_from_and_unit_bind() {
    accepts(
        "Identity {
             let! first = Identity { return 20 }
             let! mut second: i64 = 21
             second = second + 1
             do! Identity {}
             return! Identity { return first + second }
         }",
    );
    accepts("Identity {\r\nlet! n = 42\r\nreturn n\r\n}");
    accepts("Identity { let _ = 1; }");
    accepts("Identity { let! _ = 1; }");
    accepts("Identity { do! (); }");
    accepts("let result: [i32] = Identity { return [] }\nresult.length");
    accepts(
        "def pure :: 'a -> 'a
         fn pure value = Identity { return value }
         let number: i32 = pure 42
         let text = pure \"owned\"
         number",
    );
}

#[test]
fn preserves_ordinary_bindings_shadowing_annotations_and_curried_results() {
    accepts(
        "let Identity = 1
         let Bind = 2
         Identity {
             let! Identity = 20
             let! Bind = 22
             let! Bind: i64 = Bind
             return Identity + Bind
         }",
    );
    accepts(
        "let f: i64 -> i64 = Identity { return n -> n + 1 }
         f 41",
    );
    accepts(
        "let work = Identity { return task { return 42 } }
         Task.run work",
    );
    accepts(
        "let text = \"hello\"
         Identity { let! size = text.length; return size }",
    );
}

#[test]
fn translates_yield_combine_branches_loops_delay_and_run() {
    let ir = accepts(
        "Flow {
             yield 1
             yield! 2
             if false { yield 100 }
             if true { yield 3 } else if false { yield 100 } else { yield 200 }
             for n in [4, 5, 6] { yield n }
             while false { yield 1000 }
             return 21
         }",
    );
    for operation in [
        "Delay",
        "Run",
        "Combine",
        "For",
        "While",
        "Yield",
        "YieldFrom",
    ] {
        assert!(
            ir.contains(&format!("@tz.fn.Flow.{operation}")),
            "{operation}"
        );
    }
    accepts("Flow {}");
    accepts("Flow { if false { return 1 } }");
    accepts("Flow { let x = if true { 40 } else { 0 }; return x + 2 }");
    accepts("Flow { for _ in [] { yield 1 }; yield 42 }");
    accepts("Flow { assert true; }");
    accepts("Flow { do! (); return 42 }");
    accepts("let delayed: unit -> i64 = Lazy { return 42 }\ndelayed ()");
}

#[test]
fn supports_nested_builders_records_and_builtin_tasks() {
    accepts(
        "let result = Choice {
             let! x = Choice { return 20 }
             return Identity { return x + 22 }
         }
         result.value",
    );
    accepts("Task.run task { let! n = task { return Identity { return 42 } }; return n }");
    accepts("Identity { return Task.run task { return 42 } }");
    accepts(
        "record Empty {}
         record R { value: i64 }
         let _ = Empty {}
         let _ = R { value: Identity { return 42 } }
         let _ = Choice.Result { valid: false, value: 0 }
         Identity {}",
    );
    accepts("Flow { yield Flow { yield 20 }; return Identity { return 22 } }");
}

#[test]
fn preserves_owned_values_and_borrow_lifetimes() {
    accepts(
        "let text = \"hello\"
         let result = Text { let! prefix = text; return prefix + \" world\" }
         result.length",
    );
    accepts(
        "let n = 42
         let r = &n
         let result: &i64 = Identity { return r }
         *result",
    );
    let error = rejects(
        "let n = 42\nlet r = &n\nlet result: &i64 = Identity { let! value = r; return value }\n*result",
        "E1013",
    );
    assert_eq!(error.span.source, Some(0));
    rejects(
        "let text = \"owned\"\nlet _ = Text { return text }\ntext",
        "E1012",
    );
    rejects(
        "let mut n = 1\nIdentity { let! x = 1; n = x; return n }",
        "E1014",
    );
    rejects(
        "let mut n = 1\nlet r = &mut n\nIdentity { let! x = 1; return *r + x }",
        "E1005",
    );
    rejects(
        "let work = task { 42 }\nIdentity { let! x = 1; return Task.run work + x }",
        "E1005",
    );
    rejects("Identity { let! x = 1; return &x }", "E1013");
    rejects("let r = Lazy { let x = 1; return &x }\nr ()", "E1013");
    rejects(
        "let mut n = 1
         let r = &n
         let later = Lazy { return *r }
         let _ = { n = 2; }
         later ()",
        "E1014",
    );
}

#[test]
fn reports_missing_operations_and_wrong_signatures_at_the_use_site() {
    for (source, operation) in [
        ("Identity { yield 1 }", "Yield"),
        ("Identity { yield! 1 }", "YieldFrom"),
        ("Identity { for n in [1] { return n } }", "For"),
        ("Identity { while false { return () } }", "Delay"),
        ("Identity { if true { return () }; return () }", "Combine"),
    ] {
        let error = rejects(source, "E1018");
        assert!(error.message.contains(operation), "{}", error.message);
        assert_eq!(error.span.source, Some(5));
    }
    rejects("Missing { return 42 }", "E1018");
    rejects("Choice { let! n = 42; return n }", "E1003");
    rejects("Identity { let! n: bool = 42; return n }", "E1005");
    rejects("Identity { do! 42; return () }", "E1005");
    rejects("Flow { while 42 { yield 1 } }", "E1003");
    rejects("Identity { 42 }", "E1003");
    rejects("Identity { if 1 { return 1 } else { return 2 } }", "E1003");
}

#[test]
fn requires_exact_operation_names_arities_and_thunk_types_without_defaults() {
    for (builder, main, code, message) in [
        (
            "def Return :: 'a -> 'a\nfn Return value = value",
            "Builder {}",
            "E1018",
            "Zero",
        ),
        (
            "def Return :: 'a -> 'a\nfn Return value = value",
            "Builder { return! 42 }",
            "E1018",
            "ReturnFrom",
        ),
        (
            "def Yield :: i64 -> i64\nfn Yield value = value",
            "Builder { yield 1; yield 2 }",
            "E1018",
            "Combine",
        ),
        (
            "def Zero :: unit -> unit\nfn Zero value = value",
            "Builder {}",
            "E1006",
            "argument",
        ),
        (
            "def Return :: i64\nfn Return = 42",
            "Builder { return 1 }",
            "E1006",
            "argument",
        ),
        (
            "def Return :: i64 -> i64\nfn Return n = n\ndef Delay :: (i64 -> i64) -> i64\nfn Delay body = body 0",
            "Builder { return 42 }",
            "E1003",
            "unit",
        ),
        (
            "def Return :: i64 -> i64\nfn Return n = n\ndef Run :: bool -> i64\nfn Run flag = 42",
            "Builder { return 42 }",
            "E1003",
            "bool",
        ),
    ] {
        let error = analyze_modules(&[("Builder.tc", builder), ("Main.tz", main)]).unwrap_err();
        assert_eq!(error.code, code, "{main}: {}", error.message);
        assert!(error.message.contains(message), "{main}: {}", error.message);
        assert_eq!(error.span.source, Some(1));
    }
    let error = analyze_modules(&[
        (
            "Builder.tc",
            "def Return :: i64 -> i64\nfn Return n = n\ndef Return :: i64 -> i64",
        ),
        ("Main.tz", "42"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1001");
}

#[test]
fn supports_partial_worker_definitions_and_custom_iteration_source_types() {
    let module = analyze_modules(&[
        (
            "Builder.tc",
            "def Return :: i64 -> i64
             let Return = value -> value
             def Bind :: i64 -> (i64 -> i64) -> i64
             fn Bind value = { let adjusted = value + 1; next -> next adjusted }
             def For :: [|i64|] -> (i64 -> i64) -> i64
             fn For values body = body values[0]",
        ),
        (
            "Main.tz",
            "Builder { for value in [|40|] { let! next = value; return next + 1 } }",
        ),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}

#[test]
fn keeps_computation_keywords_out_of_ordinary_expressions_and_lambdas() {
    for source in [
        "return 1",
        "yield 1",
        "for n in [1] { n }",
        "while false {}",
        "Identity { return 1; return 2 }",
        "Identity { return! 1; let x = 2 }",
        "Identity { let f = n -> { return n }; return f 1 }",
        "Identity { let x = { return 1 }; return x }",
        "task { yield 1 }",
        "Identity { let x = 1 let y = 2; return x + y }",
        "Identity { do () }",
    ] {
        rejects(source, "E0002");
    }
}

#[test]
fn enforces_file_kinds_and_one_module_or_builder_per_stem() {
    for (name, source, code) in [
        ("Classes.tz", "class C 'a { def f :: 'a -> 'a }", "E1018"),
        ("Builder.tc", "class C 'a { def f :: 'a -> 'a }", "E1018"),
        ("Classes.tt", "record R {}", "E1018"),
        ("Classes.tt", "def f :: i64\nfn f = 1", "E1018"),
        (
            "Classes.tt",
            "instance Add bool { fn add a b = a }",
            "E1018",
        ),
        ("Main.tt", "42", "E1018"),
        (
            "Main.tc",
            "def Return :: i64 -> i64\nfn Return n = n\n42",
            "E2004",
        ),
        ("Empty.tc", "", "E1018"),
        ("Helper.tc", "def helper :: i64\nfn helper = 1", "E1018"),
    ] {
        let error = analyze_modules(&[(name, source)]).unwrap_err();
        assert_eq!(error.code, code, "{name}: {}", error.message);
        assert_eq!(error.span.source, Some(0));
    }
    let error = analyze_modules(&[("Same.tz", ""), ("Same.tt", "")]).unwrap_err();
    assert_eq!(error.code, "E1011");
    let error = analyze_modules(&[("Same.tc", IDENTITY), ("Same.tz", "")]).unwrap_err();
    assert_eq!(error.code, "E1011");
    let error = analyze_modules(&[
        ("Identity.tz", IDENTITY),
        ("Main.tz", "Identity { return 1 }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1018");
    let error = analyze_modules(&[("Task.tc", IDENTITY)]).unwrap_err();
    assert_eq!(error.code, "E1011");
}

#[test]
fn resolves_multiple_type_classes_from_one_file_in_code_and_builders() {
    let module = analyze_modules(&[
        ("Main.tz", "Scored { return 42i32 }"),
        (
            "Scored.tc",
            "def Return :: (Traits.Score 'a, Traits.Size 'a) => &'a -> i64
             fn Return value = Traits.Score.score value + Traits.Size.size value",
        ),
        (
            "Traits.tt",
            "class Score 'a { def score :: &'a -> i64 }
             class Size 'a { def size :: &'a -> i64 }",
        ),
        (
            "Instances.tz",
            "instance Traits.Score i32 { fn score n = (*n) as i64 }
             instance Traits.Size i32 { fn size n = 0 }",
        ),
    ]);
    assert!(module.is_err(), "Return requires a borrow, not a value");
    let module = analyze_modules(&[
        ("Main.tz", "let n = 42i32\nScored { return &n }"),
        (
            "Scored.tc",
            "def Return :: (Traits.Score 'a, Traits.Size 'a) => &'a -> i64
             fn Return value = Traits.Score.score value + Traits.Size.size value",
        ),
        (
            "Traits.tt",
            "class Score 'a { def score :: &'a -> i64 }
             class Size 'a { def size :: &'a -> i64 }",
        ),
        (
            "Instances.tz",
            "instance Traits.Score i32 { fn score n = (*n) as i64 }
             instance Traits.Size i32 { fn size n = 0 }",
        ),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
}

#[test]
fn checks_unused_builder_bodies_and_does_not_use_tc_main_as_an_entry() {
    let error = analyze_modules(&[
        ("Bad.tc", "def Return :: i64 -> i64\nfn Return n = true"),
        ("Main.tz", "42"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1003");
    assert_eq!(error.span.source, Some(0));
    let module = analyze_modules(&[(
        "Main.tc",
        "def Return :: 'a -> 'a\nfn Return n = n\ndef main :: i64\nfn main = 42",
    )])
    .unwrap();
    assert!(module.entry.is_none());
}

#[test]
fn bounds_computation_syntax_and_expansion() {
    rejects(
        &format!("{}1{}", "Identity { return ".repeat(200), " }".repeat(200)),
        "E0002",
    );
    rejects(
        &format!("Identity {{ {}return 1 }}", "let! x = 1; ".repeat(200)),
        "E0002",
    );
    rejects(
        &format!("Flow {{ {}yield 1 }}", "yield 1; ".repeat(200)),
        "E0002",
    );
    rejects(
        &format!(
            "Flow {{ {}{{ yield 1 }} }}",
            "if false { yield 0 } else ".repeat(200)
        ),
        "E0002",
    );
    accepts(&format!(
        "Identity {{ {}return 42 }}",
        "let x = 1; ".repeat(200)
    ));
}

#[test]
fn bounds_nested_builder_expansion_not_just_source_syntax() {
    accepts(&format!(
        "{}42{}",
        "Flow { return ".repeat(25),
        " }".repeat(25)
    ));
    for source in [
        format!("{}42{}", "Flow { return ".repeat(26), " }".repeat(26)),
        format!("{}42{}", "Flow { return ".repeat(32), " }".repeat(32)),
        format!(
            "{}{}42{}{}",
            "Identity.Return(".repeat(32),
            "Flow { return ".repeat(20),
            " }".repeat(20),
            ")".repeat(32)
        ),
    ] {
        tsuzuri::parser::parse(&source).unwrap();
        let error = analyze_modules(&sources(&source)).err();
        assert!(
            error.is_some(),
            "nested generated continuations must respect the depth limit"
        );
        assert_eq!(error.unwrap().code, "E0002");
    }
}

#[test]
fn runtime_fixture_lowers_deterministically_for_both_targets() {
    let mut sources = sources(include_str!("fixtures/computations/Main.tz")).to_vec();
    sources.extend([
        (
            "Optimization.tz",
            include_str!("fixtures/computations/Optimization.tz"),
        ),
        ("Skew.tc", include_str!("fixtures/computations/Skew.tc")),
        ("Traits.tt", include_str!("fixtures/computations/Traits.tt")),
        ("First.tc", include_str!("fixtures/computations/First.tc")),
        ("Once.tc", include_str!("fixtures/computations/Once.tc")),
        (
            "RunOnly.tc",
            include_str!("fixtures/computations/RunOnly.tc"),
        ),
    ]);
    let module = analyze_modules(&sources).unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
}

#[test]
fn deterministic_computation_mutations_do_not_panic() {
    let mut seed = 0x1428_5714_u32;
    for original in [
        "Identity { let! value: i64 = 20; do! (); return value + 22 }",
        "Flow { for n in [1, 2] { yield n }; while false { yield 3 }; return 4 }",
        "Choice { if true { return 42 } else { let! n = Choice {}; return n } }",
        "let body = Lazy { return Text { yield \"a\"; yield! \"b\" } }\n(body ()).length",
    ] {
        for _ in 0..500 {
            let mut bytes = original.as_bytes().to_vec();
            for _ in 0..3 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let position = seed as usize % bytes.len();
                match seed % 3 {
                    0 => {
                        bytes.remove(position);
                    }
                    1 => bytes.insert(position, (32 + seed % 95) as u8),
                    _ => bytes[position] = (32 + seed % 95) as u8,
                }
            }
            let source = String::from_utf8(bytes).unwrap();
            if let Ok(module) = analyze_modules(&sources(&source)) {
                llvm::emit(&module, llvm::Entry::Library).unwrap();
            }
        }
    }
}
