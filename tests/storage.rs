use tsuzuri::{analyze, analyze_modules, llvm};

fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    ir
}

fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .find(&format!(" @tz.fn.Main.{name}("))
        .unwrap_or_else(|| panic!("missing {name}\n{ir}"));
    let body = &ir[start..];
    &body[..body.find("\n}\n").unwrap()]
}

fn entry(body: &str) -> &str {
    body.split("\nloop:\n").next().unwrap()
}

fn rejects(source: &str, code: &str, message: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{source}\n{}",
        error.message
    );
}

#[test]
fn new_literals_are_heap_collections() {
    for source in [
        "def f :: i64\nfn f = { let a = new [1, 2]; let l = new [|3, 4|]; let e: [i64] = new []; a[1] + l[0] + e.length }",
        "def f :: [[i64]]\nfn f = new [[1], [2, 3]]",
        "def f :: i64\nfn f = new [1, 2][0] + (new [|1|]).length",
        "def f :: i64 -> [i64]\nfn f n = { let size = n; new [i64](size, i -> i) }",
        "def f :: [|i64|]\nfn f = new [|i64|] (2, i -> i)",
        "def f :: [i64 -> i64]\nfn f = new [x -> x + 1]",
    ] {
        emit(source, false);
    }
    for (source, code, message) in [
        (
            "record P { x: i64 }\ndef f :: P\nfn f = new P { x: 1 }",
            "E0002",
            "after 'new'",
        ),
        (
            "def f :: string\nfn f = new \"text\"",
            "E0002",
            "after 'new'",
        ),
        ("def f :: i64\nfn f = new 1", "E0002", "after 'new'"),
        (
            "def f :: [i64]\nfn f = new [i64]",
            "E0002",
            "new [i64](length, initializer)",
        ),
        (
            "def f :: i64\nfn f = { let a = new []; a.length }",
            "E1004",
            "type annotation",
        ),
    ] {
        rejects(source, code, message);
    }
}

#[test]
fn bound_literals_use_stack_storage() {
    let source = "def f :: i64
         fn f = {
             let a = [1, 2, 3];
             let l = [|4l, 5l|];
             let s = \"text\";
             let nested = [[1], [2, 3]];
             let mixed = [new [6], [7]];
             a[0] + l.length + s.length + nested[1][0] + mixed[0][0] + mixed[1][0]
         }";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        let body = function(&ir, "f");
        for storage in [
            "alloca [3 x i64]",
            "alloca { ptr, i64, i64, [2 x i64] }",
            "alloca [2 x %tz.array]",
            "alloca [2 x i64]",
        ] {
            assert!(entry(body).contains(storage), "{storage}\n{body}");
        }
        assert!(!body.contains("@tz.string.new"), "{body}");
        // Only the explicit `new [6]` allocates.
        assert_eq!(body.matches("ptr @tz.alloc(").count(), 1, "{body}");
    }
}

#[test]
fn temporary_operands_use_stack_storage() {
    let ir = emit(
        "def f :: i64
         fn f = {
             let mut t = 0;
             for v in [1, 2] do t = t + v;
             for v in [|3, 4|] do t = t + v;
             let s = \"abc\";
             t + [5, 6][1] + [|7|].length + \"abc\".length + (if s == \"abc\" then 1 else 0)
         }",
        false,
    );
    let body = function(&ir, "f");
    assert!(!body.contains("@tz.alloc"), "{body}");
    assert!(!body.contains("@tz.string.new"), "{body}");
}

#[test]
fn escaping_stack_values_move_to_the_heap() {
    let ir = emit(
        "def stack :: [i64]
         fn stack = { let a = [1, 2]; a }
         def heap :: [i64]
         fn heap = { let a = new [1, 2]; a }
         def moved :: i64
         fn moved = { let a = [\"x\"]; let b = a; b.length }
         def empty :: [i64]
         fn empty = { let a: [i64] = []; a }",
        false,
    );
    let stack = function(&ir, "stack");
    assert!(entry(stack).contains("alloca [2 x i64]"), "{stack}");
    assert!(stack.contains("ptr @tz.alloc("), "{stack}");
    let heap = function(&ir, "heap");
    assert!(heap.contains("ptr @tz.alloc("), "{heap}");
    assert!(!heap.contains("alloca [2 x i64]"), "{heap}");
    let moved = function(&ir, "moved");
    assert!(moved.contains("@tz.string.new"), "{moved}");
    // Empty literals own no storage, so returning one never allocates.
    assert!(!function(&ir, "empty").contains("@tz.alloc"));
}

#[test]
fn mutable_borrows_move_stack_values_to_the_heap_first() {
    let ir = emit(
        "def replace :: &mut [string] -> unit
         fn replace a = { *a = new [\"x\"]; }
         def f :: i64
         fn f = { let mut a = [\"old\"]; replace (&mut a); a.length }",
        false,
    );
    let body = function(&ir, "f");
    let call = body.find("@tz.fn.Main.replace(").unwrap();
    for relocation in ["ptr @tz.alloc(", "@tz.string.new"] {
        let position = body.find(relocation).unwrap_or_else(|| panic!("{body}"));
        assert!(position < call, "{relocation}\n{body}");
    }
}

#[test]
fn oversized_literals_keep_the_heap() {
    let literal = |count: usize| {
        let values = (0..count)
            .map(|value| format!("{value}l"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("def f :: i64\nfn f = {{ let a = [{values}]; a.length }}")
    };
    let ir = emit(&literal(8192), false);
    let body = function(&ir, "f");
    assert!(entry(body).contains("alloca [8192 x i64]"));
    assert!(!body.contains("@tz.alloc"));
    let ir = emit(&literal(8193), false);
    let body = function(&ir, "f");
    assert!(!body.contains("alloca [8193 x i64]"));
    assert!(body.contains("ptr @tz.alloc("));
}

#[test]
fn builder_blocks_keep_the_storage_rules() {
    let module = analyze_modules(&[
        (
            "Identity.tc",
            include_str!("fixtures/computations/Identity.tc"),
        ),
        (
            "Main.tz",
            "def f :: i64
             fn f = Identity {
                 let local = [1, 2, 3]
                 let! heap = new [4, 5]
                 return local[2] + heap[1] + local.length
             }",
        ),
    ])
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    let body = function(&ir, "f");
    assert!(entry(body).contains("alloca [3 x i64]"), "{body}");
    // The explicit `new [4, 5]` is the only heap literal in the entry point.
    assert_eq!(body.matches("ptr @tz.alloc(").count(), 1, "{body}");
}

#[test]
fn stack_literals_stay_in_the_entry_block_of_tail_loops() {
    for wasm in [false, true] {
        let ir = emit(
            "def rec spin :: i64 -> i64 -> i64
             fn rec spin n acc = {
                 let values = [n, acc];
                 let names = [|\"a\", \"b\"|];
                 if n == 0 then acc + names.length else spin (n - 1) (acc + values[0])
             }",
            wasm,
        );
        let body = function(&ir, "spin");
        let (before, after) = body.split_once("\nloop:\n").unwrap();
        assert!(before.contains("alloca [2 x i64]"), "{body}");
        assert!(
            before.contains("alloca { ptr, i64, i64, [2 x %tz.string] }"),
            "{body}"
        );
        assert!(!after.contains(" alloca "), "{body}");
        assert!(after.contains("br label %loop"), "{body}");
        assert!(!body.contains("@tz.alloc"), "{body}");
    }
}

#[test]
fn literal_concatenation_is_one_constant() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: i64
                 fn f = { let a = \"ab\" + \"cd\" + \"ef\"; a.length + a[0] as i64 }
                 def bytes :: i64
                 fn bytes = { let a = u8\"ab\" + u8\"cd\"; a.length }",
            wasm,
        );
        for name in ["f", "bytes"] {
            let body = function(&ir, name);
            assert!(!body.contains(".concat"), "{body}");
            assert!(!body.contains("@tz.alloc"), "{body}");
        }
    }
}

#[test]
fn string_identity_keeps_literal_and_length_only_optimizations() {
    for wasm in [false, true] {
        let ir = emit(
            "def literal :: i64
             fn literal = (to_string (\"a\" + to_string \"😀\")).length
             def length :: string -> i64
             fn length value = (to_string (to_string (value + \"xyz\"))).length
             def local :: i64
             fn local = { let text = to_string (\"a\" + \"😀\"); text.length + text[0] as i64 }
             def moved :: string -> i64
             fn moved text = (to_string text).length",
            wasm,
        );
        for name in ["literal", "length", "local", "moved"] {
            let body = function(&ir, name);
            assert!(
                !body.contains(".concat") && !body.contains(".append"),
                "{body}"
            );
            assert!(
                !body.contains(".new(") && !body.contains("ptr @tz.alloc("),
                "{body}"
            );
            assert!(!body.contains("to_string"), "{body}");
        }
        let body = function(&ir, "length");
        assert!(body.contains("icmp ult i64"), "{body}");
        assert!(body.contains("9007199254740991"), "{body}");
        let body = function(&ir, "moved");
        assert!(body.contains("store %tz.string zeroinitializer"), "{body}");
    }
}

#[test]
fn length_only_concatenation_keeps_the_checks() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: string -> i64\nfn f a = (a + \"xyz\").length
                 def bytes :: utf8string -> i64\nfn bytes a = (a + u8\"xyz\").length",
            wasm,
        );
        for name in ["f", "bytes"] {
            let body = function(&ir, name);
            assert!(
                !body.contains(".concat") && !body.contains(".append"),
                "{body}"
            );
            assert!(body.contains("icmp ult i64"), "{body}");
            assert_eq!(body.contains("9007199254740991"), name == "f", "{body}");
        }
    }
}

#[test]
fn local_concatenation_uses_a_bounded_buffer() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: string -> i64
                 fn f a = { let joined = a + \"xyz\"; joined.length + joined[0] as i64 }
                 def bytes :: utf8string -> i64
                 fn bytes a = { let joined = a + u8\"xyz\"; joined.length + joined[0] as i64 }",
            wasm,
        );
        for (name, capacity) in [("f", 128), ("bytes", 256)] {
            let body = function(&ir, name);
            if cfg!(windows) && !wasm {
                assert!(!entry(body).contains("alloca [256 x i8]"), "{body}");
                assert!(!body.contains(".concat_into(ptr"), "{body}");
                assert!(
                    body.contains(".append(") || body.contains(".concat("),
                    "{body}"
                );
            } else {
                assert!(
                    entry(body).contains("alloca [256 x i8], align 16"),
                    "{body}"
                );
                assert!(body.contains(".concat_into(ptr"), "{body}");
                assert!(body.contains(&format!("i64 {capacity},")), "{body}");
                assert!(!body.contains(".append("), "{body}");
            }
        }
    }
}

#[test]
fn escaping_concatenation_keeps_the_heap() {
    for source in [
            "def f :: string -> string\nfn f a = { let joined = a + \"x\"; joined }",
            "def use :: string -> i64\nfn use text = text.length
             def f :: string -> i64\nfn f a = { let joined = a + \"x\"; use joined }",
            "def f :: string -> (unit -> i64)\nfn f a = { let joined = a + \"x\"; \\() -> joined.length }",
            "def replace :: ref mut string -> unit\nfn replace text = deref text = \"x\"
             def f :: string -> i64\nfn f a = { let mut joined = a + \"x\"; replace (ref mut joined); joined.length }",
        ] {
            for wasm in [false, true] {
                let ir = emit(source, wasm);
                let body = function(&ir, "f");
                assert!(!body.contains("alloca [256 x i8]"), "{body}");
                assert!(body.contains(".append(") || body.contains(".concat("), "{body}");
            }
        }
}

#[test]
fn buffer_budget_falls_back_to_the_heap() {
    let mut source = String::from("def f :: i64 -> i64\nfn f n = {\n");
    for i in 0..17 {
        source.push_str(&format!(
            "let s{i} = (if n > 0 then \"ab\" else \"abc\") + \"x\";\n"
        ));
    }
    source.push_str(
        &(0..17)
            .map(|i| format!("s{i}.length"))
            .collect::<Vec<_>>()
            .join(" + "),
    );
    source.push_str("\n}");
    for wasm in [false, true] {
        let ir = emit(&source, wasm);
        let body = function(&ir, "f");
        if cfg!(windows) && !wasm {
            assert_eq!(
                entry(body).matches("alloca [256 x i8]").count(),
                0,
                "{body}"
            );
            assert_eq!(body.matches(".concat_into(").count(), 0, "{body}");
            assert!(
                body.contains(".append(") || body.contains(".concat("),
                "{body}"
            );
        } else {
            assert_eq!(
                entry(body).matches("alloca [256 x i8]").count(),
                16,
                "{body}"
            );
            assert_eq!(body.matches(".concat_into(").count(), 16, "{body}");
            assert_eq!(body.matches(".concat(").count(), 1, "{body}");
        }
    }
}

#[test]
fn recursive_functions_keep_heap_storage() {
    let source = "def rec f :: i64 -> i64
             fn rec f n = {
                 let joined = (if n > 0 then \"ab\" else \"abc\") + \"x\";
                 let a = n + 1;
                 let b = n + 2;
                 let add = \\i -> i + a + b;
                 let value = joined.length + joined[0] as i64 + add 0;
                 if n == 0 then value else value + f (n - 1)
             }";
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        let body = function(&ir, "f");
        assert!(!body.contains(".concat_into"), "{body}");
        assert!(!entry(body).contains("alloca { i64, i64 }"), "{body}");
    }
}

#[test]
fn task_bodies_keep_heap_storage() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: i64 -> i64
             fn f n = Task.run task {
                 let joined = (if n > 0 then \"ab\" else \"abc\") + \"x\";
                 joined.length + joined[0] as i64
             }",
            wasm,
        );
        assert!(ir.contains("@tz.fn.$task."));
        assert!(!ir.contains("call %tz.string @tz.string.concat_into("));
    }
}

#[test]
fn local_closures_use_entry_block_environments() {
    for wasm in [false, true] {
        let ir = emit(
                "def f :: i64 -> i64
                 fn f n = { let a = n + 1; let b = n * 2; let add = \\i -> i + a + b; add 1 + add 2 }",
                wasm,
            );
        let body = function(&ir, "f");
        if cfg!(windows) && !wasm {
            assert!(!entry(body).contains("alloca { i64, i64 }"), "{body}");
            assert!(body.contains("@tz.alloc("), "{body}");
        } else {
            assert!(
                entry(body).contains("alloca { i64, i64 }, align 16"),
                "{body}"
            );
            assert!(!body.contains("@tz.alloc"), "{body}");
        }
    }
}

#[test]
fn escaping_closures_keep_heap_environments() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: i64 -> (i64 -> i64)
                 fn f n = { let a = n + 1; let b = n * 2; let add = \\i -> i + a + b; add }",
            wasm,
        );
        let body = function(&ir, "f");
        assert!(body.contains("ptr @tz.alloc("), "{body}");
        assert!(!entry(body).contains("alloca { i64, i64 }"), "{body}");
    }
}

#[test]
fn oversized_environments_keep_the_heap() {
    let mut source = String::from("def f :: i64 -> i64\nfn f n = {\n");
    for i in 0..65 {
        source.push_str(&format!("let a{i} = n + {i};\n"));
    }
    source.push_str("let add = \\i -> i + ");
    source.push_str(
        &(0..65)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" + "),
    );
    source.push_str(";\nadd 1 + add 2\n}");
    for wasm in [false, true] {
        let ir = emit(&source, wasm);
        assert!(function(&ir, "f").contains("ptr @tz.alloc("));
    }
}

#[test]
fn single_use_field_projections_transfer_their_buffer() {
    for wasm in [false, true] {
        let ir = emit(
                "record Holder { values: [i64] }
                 record Outer { holder: Holder }
                 def f :: i64
                 fn f = { let h = Outer { holder: Holder { values: new [1, 2] } }; let values = h.holder.values; values[0] }",
                wasm,
            );
        let body = function(&ir, "f");
        assert_eq!(body.matches("ptr @tz.alloc(").count(), 1, "{body}");
    }
}

#[test]
fn owned_heap_concatenation_reuses_only_unframed_left_operands() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: string -> string -> string
                 fn f left right = left + right
                 def literal :: string -> string
                 fn literal text = \"left\" + text
                 def bytes :: utf8string -> utf8string -> utf8string
                 fn bytes left right = left + right",
            wasm,
        );
        assert!(function(&ir, "f").contains("@tz.string.append("));
        assert!(function(&ir, "bytes").contains("@tz.utf8string.append("));
        assert!(function(&ir, "literal").contains("@tz.string.concat("));
        assert!(!function(&ir, "literal").contains("@tz.string.append("));
    }
}
