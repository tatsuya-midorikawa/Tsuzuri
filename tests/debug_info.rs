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

/// The `-g` IR of `source` as `Main.tz`, emitted twice to check that it is deterministic.
fn debug_ir(source: &str, entry: tsuzuri::llvm::Entry, wasm: bool) -> String {
    use tsuzuri::{analyze, llvm, trap::TrapSource};
    let module =
        analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let sources = [TrapSource {
        path: "sources/Main.tz",
        text: source,
    }];
    let options = llvm::EmitOptions {
        entry,
        wasm,
        debug_output: false,
        allocator: llvm::Allocator::System,
    };
    let first = llvm::emit_with_debug_info(&module, options, &sources, false, false)
        .unwrap()
        .ir;
    let second = llvm::emit_with_debug_info(&module, options, &sources, false, false)
        .unwrap()
        .ir;
    assert_eq!(first, second);
    first
}

#[test]
fn debug_subprogram_names_are_readable() {
    let source = "def twice :: Add<'a> -> 'a = \\v -> v + v\nexport def answer :: i64 -> i64\nfn answer input =\n    let add = \\offset -> input + offset\n    let small = twice 2.0\n    let waited = Task.run (task { return input })\n    add (twice waited) + (small as i64)\n\nanswer 40\n";
    for wasm in [false, true] {
        for entry in [tsuzuri::llvm::Entry::Library, tsuzuri::llvm::Entry::Console] {
            let ir = debug_ir(source, entry, wasm);
            assert!(!ir.contains("linkageName:"), "{ir}");
            assert_eq!(
                ir.matches("!DISubprogram(name: \"Main.answer\",").count(),
                1
            );
            // Both instances of the generic function take its source name.
            assert_eq!(ir.matches("!DISubprogram(name: \"Main.twice\",").count(), 2);
            assert!(
                !ir.lines()
                    .any(|line| line.contains("!DISubprogram(") && line.contains("$mono")),
                "{ir}"
            );
            assert!(ir.contains("!DISubprogram(name: \"Main.answer.lambda@4:15\","));
            assert!(ir.contains("!DISubprogram(name: \"Main.answer.task@6:27\","));
            // Glue such as the builtin wrapper of `Task.run` has no source, so no subprogram.
            assert!(ir.contains("define internal i64 @tz.fn.$builtin.Task.run."));
            assert!(!ir.contains("!DISubprogram(name: \"$builtin."), "{ir}");
            // The export and entry wrappers take their symbol names and are artificial.
            let mut wrappers = vec!["tz_answer"];
            if entry == tsuzuri::llvm::Entry::Console {
                wrappers.push("main");
            }
            for wrapper in &wrappers {
                let prefix = format!("!DISubprogram(name: \"{wrapper}\",");
                let line = ir
                    .lines()
                    .find(|line| line.contains(&prefix))
                    .unwrap_or_else(|| panic!("{prefix}\n{ir}"));
                assert!(line.contains("flags: DIFlagArtificial, spFlags:"), "{line}");
            }
            assert_eq!(ir.matches("DIFlagArtificial").count(), wrappers.len());
        }
    }
}

#[test]
fn debug_binding_stores_use_declaration_lines() {
    let source = "export def answer :: i64 -> i64\nfn answer input =\n    let first = input + 1\n    let second = first * 2\n    second + first\n";
    for wasm in [false, true] {
        let ir = debug_ir(source, tsuzuri::llvm::Entry::Library, wasm);
        let body = ir
            .split("define internal i64 @tz.fn.Main.answer(")
            .nth(1)
            .and_then(|rest| rest.split("\n}\n").next())
            .unwrap();
        // The ids of the locations at each `line:column`.
        let locations = |line: usize, column: usize| -> Vec<String> {
            let prefix = format!(" = !DILocation(line: {line}, column: {column}, ");
            ir.lines()
                .filter(|text| text.contains(&prefix))
                .map(|text| text.split(' ').next().unwrap().to_owned())
                .collect()
        };
        for (line, column) in [(3, 9), (4, 9)] {
            let ids = locations(line, column);
            assert!(
                body.lines()
                    .any(|text| text.trim_start().starts_with("store i64 ")
                        && ids.iter().any(|id| text.ends_with(&format!(", !dbg {id}")))),
                "no binding store at {line}:{column}\n{body}"
            );
        }
        // The parameter's store belongs to the prologue, like Clang's.
        let parameter = body
            .lines()
            .find(|text| text.trim_start().starts_with("store i64 %p0, "))
            .unwrap();
        assert!(!parameter.contains("!dbg"), "{parameter}");
    }
}

#[test]
fn debug_scalars_and_pointers_use_tsuzuri_shapes() {
    let source = "export def answer :: i64 -> i64\nfn answer input =\n    let letter = 'A'\n    let wide = u8'\\u{e9}'\n    let chain = [|input, 2|]\n    let add = \\offset -> input + offset\n    let nothing = ()\n    let marks = (if letter == 'A' then 1 else 0) + (if wide == u8'\\u{e9}' then 1 else 0)\n    add chain.length + marks\n";
    for wasm in [false, true] {
        let ir = debug_ir(source, tsuzuri::llvm::Entry::Library, wasm);
        for expected in [
            "DW_TAG_typedef, name: \"i64\"",
            "DW_TAG_typedef, name: \"char\"",
            "DW_TAG_typedef, name: \"utf8char\"",
            "DW_TAG_typedef, name: \"unit\"",
            "!DIBasicType(name: \"char\", size: 16, encoding: DW_ATE_UTF)",
            "!DIBasicType(name: \"utf8char\", size: 32, encoding: DW_ATE_UTF)",
            "!DISubroutineType(types: !{null})",
            "DW_TAG_member, name: \"code\"",
        ] {
            assert!(ir.contains(expected), "{expected}\n{ir}");
        }
        assert!(!ir.contains("name: \"ref unit\""), "{ir}");
        // bool keeps its base type, which debuggers already show as `bool`.
        assert!(!ir.contains("DW_TAG_typedef, name: \"bool\""));
    }
}

/// The `offset:` in bits of member `member` of the composite type named `owner`.
fn member_offset(ir: &str, owner: &str, member: &str) -> usize {
    let marker = format!("!DICompositeType(tag: DW_TAG_structure_type, name: \"{owner}\",");
    let owner = ir
        .lines()
        .find(|line| line.contains(&marker))
        .unwrap_or_else(|| panic!("{marker}\n{ir}"));
    let id = owner.split(' ').next().unwrap();
    let scope = format!("name: \"{member}\", scope: {id},");
    let line = ir
        .lines()
        .find(|line| line.contains("DW_TAG_member") && line.contains(&scope))
        .unwrap_or_else(|| panic!("{scope}\n{ir}"));
    line.split("offset: ")
        .nth(1)
        .unwrap()
        .trim_end_matches(')')
        .parse()
        .unwrap()
}

const UNIONS: &str = "union Shape = Empty | Circle of f64 | Rect of f64 * f64\nunion Color = Red | Green\nunion Tree = Leaf | Node of Tree * i64 * Tree\n\nexport def answer :: i64 -> i64\nfn answer input =\n    let color = Green\n    let shape = Rect (1.5, 2.0)\n    let tree = Node (Leaf, input, Leaf)\n    let maybe = Maybe.Some input\n    let chain = [|input, 2|]\n    let size = match shape with | Rect (w, _) -> w as i64 | _ -> 0\n    let leaf = match tree with | Node (_, value, _) -> value | Leaf -> 0\n    let first = match color with | Green -> 1 | Red -> 0\n    size + leaf + first + Maybe.default_value 0 maybe + chain.length\n";

#[test]
fn debug_unions_expose_cases_and_payloads() {
    for wasm in [false, true] {
        let ir = debug_ir(UNIONS, tsuzuri::llvm::Entry::Library, wasm);
        for expected in [
            "DW_TAG_enumeration_type, name: \"Main.Color\"",
            "!DIEnumerator(name: \"Green\", value: 1)",
            "DW_TAG_enumeration_type, name: \"Main.Shape.$tag\"",
            "DW_TAG_member, name: \"$tag\"",
            "DW_TAG_union_type, name: \"Main.Shape.$payload\"",
            "DW_TAG_member, name: \"Rect\"",
            "DW_TAG_member, name: \"Circle\"",
            "DW_TAG_structure_type, name: \"Main.Tree.node\"",
            "DW_TAG_typedef, name: \"Main.Tree\"",
            "DW_TAG_union_type, name: \"Maybe<i64>.$payload\"",
            "DW_TAG_structure_type, name: \"[|i64|].node\"",
        ] {
            assert!(ir.contains(expected), "{expected}\n{ir}");
        }
        // A nullary case has no payload member.
        assert!(!ir.contains("DW_TAG_member, name: \"Empty\""));
        assert!(!ir.contains("DW_TAG_member, name: \"Leaf\""));
    }
}

#[test]
fn debug_union_offsets_match_storage() {
    // Offsets in bits from `union_layout` and the node `{ ptr, ptr, ptr, i32, payload }`.
    for (wasm, tag, payload) in [(false, 192, 256), (true, 96, 128)] {
        let ir = debug_ir(UNIONS, tsuzuri::llvm::Entry::Library, wasm);
        // `General`: payloads of different LLVM types share `[1 x i128]` after the tag.
        assert_eq!(member_offset(&ir, "Main.Shape", "$tag"), 0);
        assert_eq!(member_offset(&ir, "Main.Shape", "$payload"), 128);
        // `Common`: `{ i32, i64 }`.
        assert_eq!(member_offset(&ir, "Maybe<i64>", "$payload"), 64);
        assert_eq!(member_offset(&ir, "Main.Tree.node", "next"), 0);
        assert_eq!(member_offset(&ir, "Main.Tree.node", "$tag"), tag);
        assert_eq!(member_offset(&ir, "Main.Tree.node", "$payload"), payload);
        // The list node `{ ptr, i64 }`.
        assert_eq!(member_offset(&ir, "[|i64|].node", "value"), 64);
    }
}

#[test]
fn debug_codeview_flag_joins_the_module_flags_for_windows_objects() {
    let source = "export def answer :: i64 -> i64\nfn answer input =\n    let shape: Maybe<i64> = Maybe.Some input\n    Maybe.default_value 0 shape + 2\n";
    let ir = debug_ir(source, tsuzuri::llvm::Entry::Library, false);
    let windows = tsuzuri::llvm::with_codeview(ir.clone());
    let id = ir
        .lines()
        .filter_map(|line| {
            line.strip_prefix('!')?
                .split_once(" = ")?
                .0
                .parse::<usize>()
                .ok()
        })
        .max()
        .unwrap()
        + 1;
    let flag = format!("\n!{id} = !{{i32 2, !\"CodeView\", i32 1}}\n");
    assert!(windows.contains(&flag), "{windows}");
    let flags = windows
        .lines()
        .find(|line| line.starts_with("!llvm.module.flags = "))
        .unwrap();
    assert!(flags.ends_with(&format!(", !{id}}}")), "{flags}");
    // Only the flag is added, once.
    assert_eq!(
        windows
            .replacen(&flag, "\n", 1)
            .replace(&format!(", !{id}}}"), "}"),
        ir
    );
    assert_eq!(tsuzuri::llvm::with_codeview(windows.clone()), windows);
    let module = tsuzuri::analyze(source).unwrap();
    let plain = tsuzuri::llvm::emit(&module, tsuzuri::llvm::Entry::Library).unwrap();
    assert_eq!(tsuzuri::llvm::with_codeview(plain.clone()), plain);
    // For an MSVC target LLVM writes CodeView, which becomes the PDB, beside the DWARF.
    let directory = std::env::temp_dir().join(format!("tsuzuri-codeview-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("module.ll");
    std::fs::write(&path, &windows).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        let object = directory.join(format!("{target}.obj"));
        let output = std::process::Command::new(&clang)
            .args(["-x", "ir", "-Wno-override-module", "-O0", "-g", "-c"])
            .arg(format!("--target={target}"))
            .arg(&path)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let bytes = std::fs::read(&object).unwrap();
        for section in [
            &b".debug$S"[..],
            b".debug$T",
            b".debug_info",
            b"Main.answer",
        ] {
            assert!(
                bytes.windows(section.len()).any(|window| window == section),
                "{target}: {}",
                String::from_utf8_lossy(section)
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn debug_test_runner_dispatch_is_an_artificial_subprogram() {
    use tsuzuri::{analyze, llvm, trap::TrapSource};
    let source = "test \"same\" = assert true\ntest \"body\" =\n    let total = 40 + 2\n    assert (total == 42)\n";
    let module =
        analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let sources = [TrapSource {
        path: "sources/Main.tz",
        text: source,
    }];
    for wasm in [false, true] {
        let plain = llvm::TestRunnerOptions {
            wasm,
            ..llvm::TestRunnerOptions::default()
        };
        let ir = llvm::emit_test_runner_with(&module, &[1], plain).unwrap();
        assert!(
            ir.contains("define i32 @tsuzuri_test_run(i32 %index) {\n"),
            "{ir}"
        );
        assert!(!ir.contains("!dbg"), "{ir}");
        for optimized in [false, true] {
            let options = llvm::TestRunnerOptions {
                debug: Some((&sources, optimized)),
                ..plain
            };
            let ir = llvm::emit_test_runner_with(&module, &[1], options).unwrap();
            assert_eq!(
                ir,
                llvm::emit_test_runner_with(&module, &[1], options).unwrap()
            );
            // From -O1 the dispatch inlines the test; the inlined body keeps its lines only
            // when the dispatch has a subprogram and the call has a location.
            let define = ir
                .lines()
                .find(|line| line.starts_with("define i32 @tsuzuri_test_run(i32 %index)"))
                .unwrap();
            let scope = define
                .strip_suffix(" {")
                .and_then(|line| line.rsplit_once(" !dbg "))
                .unwrap_or_else(|| panic!("{define}"))
                .1;
            let subprogram = ir
                .lines()
                .find(|line| line.starts_with(&format!("{scope} = distinct !DISubprogram(")))
                .unwrap_or_else(|| panic!("{scope}\n{ir}"));
            for part in [
                "name: \"tsuzuri_test_run\",",
                "line: 2,",
                "DIFlagArtificial",
            ] {
                assert!(subprogram.contains(part), "{part}\n{subprogram}");
            }
            let call = ir
                .lines()
                .find(|line| line.starts_with("  %result0 = call i8 @tz.fn.Main.$test.1()"))
                .unwrap_or_else(|| panic!("{ir}"));
            assert!(call.contains(", !dbg !"), "{call}");
        }
    }
}
