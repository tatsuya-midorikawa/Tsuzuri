use tsuzuri::{analyze, llvm, trap::TrapSource};

#[test]
fn only_native_i64_standard_sum_requests_cpu_dispatch() {
    let source = "export def sum :: ref [i64] -> i64\nfn sum values = Array.sum values\nexport def floating :: ref [f64] -> f64\nfn floating values = Array.sum values";
    let module = analyze(source).unwrap();
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
    };
    let sources: Vec<_> = std::iter::once(TrapSource {
        path: "Main.tz",
        text: source,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect();
    let native = llvm::emit_native_build(&module, options, &sources, None, false).unwrap();
    assert!(native.ir.contains("call i64 @tsuzuri_cpu_sum_i64"));
    assert_eq!(
        native
            .ir
            .matches("declare i64 @tsuzuri_cpu_sum_i64")
            .count(),
        1
    );
    assert!(native.ir.contains("fadd double"));
    assert_eq!(
        native.ir,
        llvm::emit_native_build(&module, options, &sources, None, false)
            .unwrap()
            .ir
    );
    assert!(
        !llvm::emit_target(&module, llvm::Entry::Library, true)
            .unwrap()
            .contains("tsuzuri_cpu")
    );
    assert!(
        !llvm::emit(&module, llvm::Entry::Library)
            .unwrap()
            .contains("tsuzuri_cpu")
    );
    assert!(
        !llvm::emit_native_build(&module, options, &sources[..1], None, false)
            .unwrap()
            .ir
            .contains("tsuzuri_cpu")
    );
    let ordinary = analyze("export def answer :: i64\nfn answer = 42").unwrap();
    assert!(
        !llvm::emit_native_build(&ordinary, options, &sources, None, false)
            .unwrap()
            .ir
            .contains("tsuzuri_cpu")
    );
}
