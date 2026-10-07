use tsuzuri::driver::{BuildOptions, Emit};

#[test]
fn debug_options_are_opt_in_and_reject_header_output() {
    assert!(!BuildOptions::default().debug_info);
    let options = BuildOptions {
        debug_info: true,
        emit: Emit::Llvm,
        ..BuildOptions::default()
    };
    options.validate().unwrap();
    assert_eq!(
        BuildOptions {
            emit: Emit::Header,
            ..options
        }
        .validate()
        .unwrap_err()
        .code,
        "E2000"
    );
}

#[test]
fn debug_metadata_is_deterministic_and_keeps_source_types() {
    use tsuzuri::{analyze, llvm, trap::TrapSource};
    let source = "record Point { x: i64, label: string }\nexport def answer :: i64 -> i64\nfn answer input = { let point = Point { x: input + 2, label: \"value\" }; point.x }";
    let module = analyze(source).unwrap();
    let sources = [TrapSource {
        path: "sources/Main.tz",
        text: source,
    }];
    for wasm in [false, true] {
        let options = llvm::EmitOptions {
            entry: llvm::Entry::Library,
            wasm,
            debug_output: false,
            allocator: llvm::Allocator::System,
        };
        let plain = llvm::emit_with_options(&module, options).unwrap();
        assert!(!plain.contains("!DICompileUnit"));
        for traps in [false, true] {
            let first =
                llvm::emit_with_debug_info(&module, options, &sources, false, traps).unwrap();
            let second =
                llvm::emit_with_debug_info(&module, options, &sources, false, traps).unwrap();
            assert_eq!(first.ir, second.ir);
            for expected in [
                "!DICompileUnit",
                "!DISubprogram",
                "!DILocation(line: 3",
                "!DILocalVariable(name: \"input\"",
                "!DILocalVariable(name: \"point\"",
                "DW_TAG_member, name: \"label\"",
                "@llvm.dbg.declare",
            ] {
                assert!(first.ir.contains(expected), "{expected}");
            }
            assert_eq!(
                first
                    .ir
                    .matches("!DICompositeType(tag: DW_TAG_structure_type, name: \"Main.Point\"")
                    .count(),
                1
            );
        }
    }
}
