use tsuzuri::{analyze, llvm};

#[test]
fn debug_borrows_prints_and_transfers_trace_values() {
    let module = analyze("export def answer :: i64\nfn answer = { let text = \"hello\"; Debug.print text; let moved = Debug.trace text; moved.length + Debug.trace 37 }").unwrap();
    let native = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
    assert!(native.contains("@tz.debug.write"));
    assert!(native.contains("call i64 @write(i32 2"));
    let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    assert!(!wasm.contains("@write("));
    assert!(!wasm.contains("wasm-import-module"));
    assert!(wasm.contains("@tz.builtin.Debug.__print_string"));
    assert!(wasm.contains("@tz.free"));
    let opted = llvm::emit_with_options(
        &module,
        llvm::EmitOptions {
            entry: llvm::Entry::Library,
            wasm: true,
            debug_output: true,
            allocator: llvm::Allocator::System,
        },
    )
    .unwrap();
    assert!(opted.contains("\"wasm-import-module\"=\"tsuzuri_debug\""));
    assert!(opted.contains("\"wasm-import-name\"=\"write\""));
    assert!(!opted.contains("@write("));
    assert_eq!(
        analyze("Debug.__print_string \"private\"")
            .unwrap_err()
            .code,
        "E1022"
    );
    assert_eq!(
        analyze(
            "record Hidden { value: i64 }\nlet hidden = Hidden { value: 1 }\nDebug.print hidden"
        )
        .unwrap_err()
        .code,
        "E1005"
    );
    assert_eq!(
        analyze("let original = \"text\"\nlet moved = Debug.trace original\noriginal.length")
            .unwrap_err()
            .code,
        "E1012"
    );
}

#[test]
fn unused_debug_code_does_not_add_runtime() {
    let module = analyze("42").unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_with_options(
            &module,
            llvm::EmitOptions {
                entry: llvm::Entry::Library,
                wasm,
                debug_output: true,
                allocator: llvm::Allocator::System,
            },
        )
        .unwrap();
        assert!(!ir.contains("@tz.debug"));
        assert!(!ir.contains("@write("));
        assert!(!ir.contains("tsuzuri_debug"));
    }
}
