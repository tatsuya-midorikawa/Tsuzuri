use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    ir
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn composes_tasks_with_bind_return_and_return_from() {
    accepts(
        "def value :: i64 -> Task i64
         fn value n = task { return n + 1 }
         let computation = task {
             let! a = value 19
             let! b: i64 = value 21
             do! task { assert (a == 20); }
             return! task { return a + b }
         }
         Task.run computation",
    );
    accepts("Task.run task { if true { return 42 } else { return 0 } }");
    accepts("Task.run task { let! mut n = task { 1 }; n = n + 1; return n }");
    accepts("Task.run task {}");
    accepts("Task.run task { do! task {}; }");
    accepts("Task.run task {\r\nlet! n = task { 42 }\r\nreturn n\r\n}");
}

#[test]
fn supports_owned_captures_local_borrows_and_closed_function_results() {
    accepts(
        "let text = \"owned\"
         let work = task { let r = &text; return clone_string r }
         Task.run work",
    );
    accepts(
        "let offset = 20
         let f: i64 -> i64 = n -> n + offset
         let work = task { f 22 }
         Task.run work",
    );
    accepts(
        "let work: Task (i64 -> i64) = task {
             let text = \"hello\";
             return n -> n + text.length
         }
         let f = Task.run work
         f 37",
    );
    accepts(
        "let work: Task (&i64 -> i64) = task { r -> *r }
         let f = Task.run work
         let n = 42
         f (&n)",
    );
    accepts(
        "let first = task { \"a\" }
         let second = task { let! text = first; return text + \"b\" }
         Task.run second",
    );
}

#[test]
fn supports_polymorphic_tasks_first_class_builtins_and_aggregates() {
    accepts(
        "record Pending { work: Task string }
         def pure :: 'a -> Task 'a
         fn pure value = task { value }
         def forward :: Task 'a -> Task 'a
         fn forward work = task { return! work }
         let pending = Pending { work: forward (pure \"owned\") }
         let run: Task string -> string = Task.run
         run pending.work",
    );
    accepts(
        "def apply :: ('a -> 'b) -> 'a -> 'b
         fn apply f x = f x
         let parallel: [Task i64] -> Task [i64] = Task.parallel
         let results = apply Task.run (parallel [task { 20 }, task { 22 }])
         results[0] + results[1]",
    );
    accepts(
        "def finish :: Task i64 -> i64 -> i64
         fn finish work n = Task.run work + n
         let work = task { 40 }
         2 |> finish work",
    );
    accepts(
        "let works = new [Task i64](10, i -> task { i * i })
         let results = Task.run (Task.parallel works)
         results[9]",
    );
    accepts(
        "let jobs: [Task i64] = []\nlet results = Task.run (Task.parallel jobs)\nresults.length",
    );
    accepts("let jobs = [|task { \"one\" }, task { \"two\" }|]\njobs.length");
}

#[test]
fn preserves_module_task_types_and_diagnostics() {
    let module = analyze_modules(&[
        (
            "Worker",
            "def work :: i64 -> Task i64\nfn work n = task { n * n }",
        ),
        ("Main", "Task.run (Worker.work 7)"),
    ])
    .unwrap();
    llvm::emit(&module, llvm::Entry::Console).unwrap();
    let error = analyze_modules(&[
        ("Main", "0"),
        (
            "Worker",
            "def bad :: &i64 -> Task i64\nfn bad n = task { *n }",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1013");
    assert_eq!(error.span.source, Some(1));
    assert_eq!(analyze_modules(&[("Task", "")]).unwrap_err().code, "E1011");
}

#[test]
fn rejects_task_syntax_outside_computations_and_non_task_binds() {
    for source in [
        "let! x = task { 1 }\nx",
        "{ return 1 }",
        "{ do! task {} }",
        "task { return 1; return 2 }",
        "task { return 1; let x = 2; x }",
        "task { let f = n -> { let! x = task { n }; x }; f 1 }",
        "task { do task {} }",
    ] {
        rejects(source, "E0002");
    }
    rejects("task { let! x = 1; x }", "E1003");
    rejects("task { return! 1 }", "E1003");
    rejects("task { do! task { 1 }; }", "E1003");
    rejects("Task.run 1", "E1003");
    rejects("Task.parallel [1, 2]", "E1003");
    rejects("Task.missing task { 1 }", "E1002");
    rejects("let work = task { 1 }\nwork()", "E1005");
    rejects("export def work :: Task i64\nfn work = task { 1 }", "E1008");
}

#[test]
fn rejects_duplicate_execution_and_reusable_task_captures() {
    rejects("let mut n = 1\ntask { n = 2; n }", "E1014");
    rejects(
        "let work = task { 1 }\nlet first = Task.run work\nTask.run work",
        "E1012",
    );
    rejects(
        "let work = task { 1 }\nTask.run (Task.parallel [work, work])",
        "E1012",
    );
    rejects(
        "let work = task { 1 }\nlet next = task { Task.run work }\nTask.run work",
        "E1012",
    );
    rejects("let works = [task { 1 }]\nTask.run works[0]", "E1012");
    rejects(
        "let work = task { 1 }\nlet r = &work\nTask.run (*r)",
        "E1012",
    );
    rejects(
        "let work = task { 1 }\nlet f: unit -> i64 = u -> Task.run work\nf ()",
        "E1005",
    );
    rejects(
        "def finish :: Task i64 -> i64 -> i64
         fn finish work n = Task.run work + n
         let f = finish task { 1 }
         f 2",
        "E1005",
    );
    rejects(
        "def twice :: Copy 'a => 'a -> 'a
         fn twice value = value
         twice task { 1 }",
        "E1005",
    );
}

#[test]
fn rejects_references_in_task_captures_and_results_including_generic_uses() {
    for source in [
        "let n = 1\nlet r = &n\ntask { *r }",
        "let mut n = 1\nlet r = &mut n\ntask { *r = 2; }",
        "let n = 1\nlet refs = [&n]\ntask { *refs[0] }",
        "let n = 1\nlet refs = [|&n|]\ntask { *refs[0] }",
        "task { let n = 1; &n }",
        "task { let n = 1; [&n] }",
        "def bad :: Task &i64\nfn bad = task { let n = 1; &n }",
        "def make :: 'a -> Task 'a\nfn make value = task { value }\nlet n = 1\nmake (&n)",
        "def wrap :: Task 'a -> Task 'a\nfn wrap work = work\ndef bad :: Task &i64 -> unit\nfn bad work = { wrap work; }",
    ] {
        rejects(source, "E1013");
    }
}

#[test]
fn rejects_borrows_hidden_in_function_environments_and_aggregates() {
    for value in ["f", "[f]", "[|f|]", "Callback { call: f }"] {
        rejects(
            &format!(
                "record Callback {{ call: i64 -> i64 }}
                 let n = 1
                 let r = &n
                 let f: i64 -> i64 = x -> x + *r
                 let captured = {value}
                 task {{ captured }}"
            ),
            "E1013",
        );
    }
    rejects(
        "let work: Task (i64 -> i64) = task {
             let n = 1;
             let r = &n;
             return x -> x + *r
         }
         Task.run work",
        "E1013",
    );
    rejects(
        "def unknown :: (i64 -> i64) -> Task i64
         fn unknown f = task { f 0 }",
        "E1013",
    );
}

#[test]
fn emits_non_clonable_tasks_and_distinct_native_and_wasm_group_backends() {
    let source = "let jobs = [task { 20 }, task { 22 }]\nlet xs = Task.run (Task.parallel jobs)\nxs[0] + xs[1]";
    let ir = accepts(source);
    assert!(ir.contains("declare void @tsuzuri_task_parallel(ptr, ptr, i64)"));
    assert!(!ir.contains("@tz.env.clone.$task."));
    assert!(ir.contains("@tz.task.item.i64"));
    let wasm = llvm::emit_target(&analyze(source).unwrap(), llvm::Entry::Library, true).unwrap();
    assert!(wasm.contains("define internal void @tsuzuri_task_parallel"));
    assert!(!wasm.contains("pthread"));
    assert!(!wasm.contains("declare void @tsuzuri_task_parallel"));
}

#[test]
fn bounds_nested_task_syntax_and_types() {
    rejects(&format!("def work :: {}i64", "Task ".repeat(200)), "E0002");
    rejects(
        &format!("{}0{}", "task { ".repeat(200), " }".repeat(200)),
        "E0002",
    );
}

#[test]
fn supports_task_instances_and_reserves_the_task_namespace() {
    accepts(
        "instance Add (Task i64) {
             fn add left right = task { let! x = left; let! y = right; return x + y }
         }
         Task.run (task { 20 } + task { 22 })",
    );
    rejects("record Task { value: i64 }", "E1001");
    rejects("class Task 'a { def run :: 'a -> 'a }", "E1001");
}

#[test]
fn runtime_fixture_lowers_for_both_targets() {
    accepts(include_str!("fixtures/tasks/Main.tz"));
}
