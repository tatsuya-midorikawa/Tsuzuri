use tsuzuri::{analyze, llvm};

#[test]
fn parallel_apis_are_typed_and_keep_their_ownership_boundary() {
    for source in [
        "let values = Parallel.init 10 (index -> index + 1)\nParallel.sum (ref values)",
        "let values = [1, 2, 3]\nlet result = Parallel.map (value -> value + 1) (ref values[1..3])\nParallel.reduce 0 (left -> right -> left - right) (ref result)",
        "let values = [\"hello\", \"world\"]\nlet result = Parallel.map_ref (value -> value.length) (ref values)\nArray.sum (ref result)",
        "let result = Parallel.init 2 (index -> value -> index + value)\nresult.length",
        "def make :: i64 -> i64 -> i64 -> i64\nfn make offset index value = offset + index + value\nlet result = Parallel.init 2 (make 10)\nresult.length",
        "let functions = [value -> value + 1]\nlet result = Parallel.init 2 functions[0]\nresult.length",
        "let callback = if true then (value -> value + 1) else (value -> value + 2)\nlet result = Parallel.init 2 callback\nresult.length",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
    for source in [
        "let number = 1\nlet borrowed = ref number\nParallel.init 2 (index -> deref borrowed + index)",
        "let values = [1, 2]\nParallel.map_ref (value -> ignored -> deref value + ignored) (ref values)",
        "def apply :: (i64 -> i64) -> [i64]\nfn apply callback = Parallel.init 2 callback",
        "let operation = Parallel.init\noperation 2 (index -> index)",
        "let number = 1\nlet borrowed = ref number\nlet callback = value -> value + deref borrowed\nlet functions = [callback]\nParallel.map (function -> function 1) (ref functions)",
        "def apply :: ref [i64 -> i64] -> [i64]\nfn apply functions = Parallel.map (function -> function 1) functions",
        "let values = [1, 2]\nParallel.map_ref (value -> new [i64 -> i64](2, index -> delta -> deref value + index + delta)) (ref values)",
        "let values = [1, 2]\nParallel.map_ref (value -> { let result = delta -> deref value + delta; if true then result else result }) (ref values)",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1013", "{source}");
    }
    assert_eq!(
        analyze("let values = [\"x\"]\nParallel.map (value -> value.length) (ref values)")
            .unwrap_err()
            .code,
        "E1005"
    );
    assert_eq!(
        analyze("let values = [\"x\"]\nParallel.sum (ref values)")
            .unwrap_err()
            .code,
        "E1005"
    );
}

#[test]
fn parallel_codegen_uses_fixed_chunks_and_the_existing_runtime() {
    let module = analyze("export def calculate :: i64 -> i64\nfn calculate length = { let offset = 3; let values = Parallel.init length (index -> index + offset); Parallel.sum (ref values) }").unwrap();
    let ir = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    assert!(ir.contains("@tsuzuri_task_parallel"));
    assert!(ir.contains("@tz.parallel.chunk."));
    assert!(ir.contains("@tz.specialized."));
    assert!(ir.contains(", 4096") && ir.contains(", 1024"));
    assert!(!ir.contains("call ptr @tz.alloc(i64 32)"));
}
