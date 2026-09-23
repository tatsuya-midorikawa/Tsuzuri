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
             let l = [|4, 5|];
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
            "alloca [2 x { ptr, i64 }]",
            "alloca [2 x %tz.array]",
            "alloca [2 x i64]",
        ] {
            assert!(entry(body).contains(storage), "{storage}\n{body}");
        }
        assert!(!body.contains("@tz.string.new"), "{body}");
        // Only the explicit `new [6]` allocates.
        assert_eq!(body.matches("call ptr @tz.alloc").count(), 1, "{body}");
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
    assert!(stack.contains("call ptr @tz.alloc"), "{stack}");
    let heap = function(&ir, "heap");
    assert!(heap.contains("call ptr @tz.alloc"), "{heap}");
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
    for relocation in ["call ptr @tz.alloc", "@tz.string.new"] {
        let position = body.find(relocation).unwrap_or_else(|| panic!("{body}"));
        assert!(position < call, "{relocation}\n{body}");
    }
}

#[test]
fn oversized_literals_keep_the_heap() {
    let literal = |count: usize| {
        let values = (0..count)
            .map(|value| value.to_string())
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
    assert!(body.contains("call ptr @tz.alloc"));
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
    assert_eq!(body.matches("call ptr @tz.alloc").count(), 1, "{body}");
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
            before.contains("alloca [2 x { ptr, %tz.string }]"),
            "{body}"
        );
        assert!(!after.contains(" alloca "), "{body}");
        assert!(after.contains("br label %loop"), "{body}");
        assert!(!body.contains("@tz.alloc"), "{body}");
    }
}
