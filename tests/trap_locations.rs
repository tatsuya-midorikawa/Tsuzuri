use tsuzuri::{
    diagnostic::Span,
    trap::{TrapKind, TrapSite, TrapSource, side_table},
};

#[test]
fn emitted_guards_record_source_sites_without_success_path_reports() {
    let source = "export def divide :: i64 -> i64 -> i64\nfn divide left right = left / right\nexport def bound :: i64 -> i64\nfn bound index = { let values = [1, 2]; values[index] }\nexport def verify :: bool -> unit\nfn verify flag = assert flag";
    let module = tsuzuri::analyze(source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    for wasm in [false, true] {
        let options = tsuzuri::llvm::EmitOptions {
            entry: tsuzuri::llvm::Entry::Library,
            wasm,
            debug_output: false,
        };
        let ordinary = tsuzuri::llvm::emit_with_options(&module, options).unwrap();
        assert_eq!(
            ordinary,
            tsuzuri::llvm::emit_target(&module, options.entry, wasm).unwrap()
        );
        let first = tsuzuri::llvm::emit_with_trap_info(&module, options, &sources).unwrap();
        let second = tsuzuri::llvm::emit_with_trap_info(&module, options, &sources).unwrap();
        assert_eq!(first.ir, second.ir);
        assert_eq!(first.trap_sites, second.trap_sites);
        for kind in [
            TrapKind::Assert,
            TrapKind::IntegerDivisionByZero,
            TrapKind::IntegerDivisionOverflow,
            TrapKind::BoundsCheck,
        ] {
            assert!(
                first.trap_sites.iter().any(|site| site.kind == kind),
                "{kind:?}"
            );
        }
        assert!(first.ir.contains("@tz.trap.report"));
        assert!(!first.ir.contains("!tz.site"));
        assert!(!ordinary.contains("@tz.trap.report"));
        if wasm {
            assert!(first.ir.contains("@tsuzuri_trap_site"));
        }
    }
}

#[cfg(unix)]
#[test]
fn run_reports_probable_stack_exhaustion() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-stack-cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    // The multiplication keeps the recursion non-tail, so LLVM cannot turn it into a loop.
    fs::write(
        root.join("Main.tz"),
        "def rec depth :: i64 -> i64\nfn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n\n\nexport def deep :: i64 -> i64\nfn deep n = depth n\n\ndef main :: i64 = deep 100000000\n",
    )
    .unwrap();
    for optimization in ["-O0", "-O3"] {
        for json in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
            command.arg("run").arg(&root);
            if json {
                command.arg("--json");
            }
            let result = command.arg(optimization).output().unwrap();
            assert_eq!(result.status.code(), Some(1));
            let stderr = String::from_utf8(result.stderr).unwrap();
            assert!(stderr.contains("E2005"), "{stderr}");
            assert!(
                stderr.contains("the stack was probably exhausted by deep recursion"),
                "{stderr}"
            );
            assert!(!stderr.contains("may have trapped"), "{stderr}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trap_table_is_deterministic_and_uses_the_source_map() {
    let sources = [
        TrapSource {
            path: "Main.tz",
            text: "42",
        },
        TrapSource {
            path: "quoted\"file.tz",
            text: "let value = 1\nassert false",
        },
    ];
    let sites = [TrapSite {
        id: 1,
        kind: TrapKind::Assert,
        span: Span::new(14, 26).in_source(1),
    }];
    let table = side_table(&sites, &sources).unwrap();
    assert_eq!(table, side_table(&sites, &sources).unwrap());
    assert!(table.contains("\"kind\":\"assertion failed\""));
    assert!(table.contains("\"path\":\"quoted\\\"file.tz\""));
    assert!(table.contains("\"line\":2,\"column\":1"));
    assert_eq!(
        sites[0].message(&sources).unwrap(),
        "trap: assertion failed at quoted\"file.tz:2:1"
    );
    assert!(side_table(&sites, &[]).is_err());
}

#[test]
fn typed_builtins_keep_guard_kinds_and_user_call_sites() {
    let source = "export def replace :: i64 -> i64\nfn replace index = { let values = Array.set [1, 2] index 3; values.length }\nexport def reserve :: i64 -> i64\nfn reserve count = { let values: Vec<i64> = Vec.with_capacity count; Vec.length (ref values) }\nexport def decode :: i64 -> i64\nfn decode index = { let text = u8\"text\"; match Utf8String.decode_at (ref text) index with | (_, next) -> next }\nexport def units :: i64\nfn units = (String.from_code_units [1i16u]).length";
    let module = tsuzuri::analyze(source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let output = tsuzuri::llvm::emit_with_trap_info(
        &module,
        tsuzuri::llvm::EmitOptions {
            entry: tsuzuri::llvm::Entry::Library,
            wasm: true,
            debug_output: false,
        },
        &sources,
    )
    .unwrap();
    for (kind, expression) in [
        (TrapKind::BoundsCheck, "Array.set [1, 2] index 3"),
        (TrapKind::AllocationSize, "Vec.with_capacity count"),
        (
            TrapKind::BoundsCheck,
            "Utf8String.decode_at (ref text) index",
        ),
        (TrapKind::AllocationSize, "String.from_code_units [1i16u]"),
    ] {
        assert!(
            output.trap_sites.iter().any(|site| site.kind == kind
                && source[site.span.start..site.span.end].contains(expression)),
            "{kind:?}: {:?}",
            output.trap_sites
        );
    }
}

#[test]
fn cli_reports_json_locations_and_preserves_failed_build_artifacts() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-trap-cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = root.join("Main.tz");
    fs::write(
        &source,
        "let message = \"before failure\"\nDebug.print message\nassert false",
    )
    .unwrap();
    for optimization in ["-O0", "-O3"] {
        let result = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("run")
            .arg(&source)
            .args(["--json", optimization])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(stderr.contains("\"code\":\"E2005\""), "{stderr}");
        assert!(
            stderr.contains("trap: assertion failed at") && stderr.contains("Main.tz:3:1"),
            "{stderr}"
        );
        assert!(stderr.contains("before failure"));
    }
    fs::write(
        &source,
        "export def answer :: i64 -> i64\nfn answer value = 42 / value",
    )
    .unwrap();
    let output = root.join("module.wasm");
    let table = tsuzuri::driver::trap_sidecar_path(&output);
    let compile = |enabled: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .arg("build")
            .arg(&source)
            .args(["--target", "wasm32", "-o"])
            .arg(&output);
        if enabled {
            command.arg("--trap-info");
        }
        command.output().unwrap()
    };
    assert!(compile(false).status.success());
    assert!(!table.exists());
    let enabled = compile(true);
    assert!(
        enabled.status.success(),
        "{}",
        String::from_utf8_lossy(&enabled.stderr)
    );
    let original_output = fs::read(&output).unwrap();
    let original_table = fs::read(&table).unwrap();
    assert!(compile(true).status.success());
    assert_eq!(fs::read(&table).unwrap(), original_table);
    fs::write(&source, "export def answer :: i64\nfn answer = true").unwrap();
    assert!(!compile(true).status.success());
    assert_eq!(fs::read(&output).unwrap(), original_output);
    assert_eq!(fs::read(&table).unwrap(), original_table);
    fs::write(&source, "export def answer :: i64\nfn answer = 42").unwrap();
    fs::remove_file(&table).unwrap();
    fs::hard_link(&source, &table).unwrap();
    let result = compile(true);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("E2003"));
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "export def answer :: i64\nfn answer = 42"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trap_context_preserves_collections_closures_and_parallel_task_abis() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-trap-runtime-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let input = root.join("Main.tz");
    let source = "export def stress :: i64\nfn stress =\n    let source = new [3, 1, 2]\n    let sorted = Array.sort (ref source)\n    let linked = Array.to_list (ref sorted)\n    let mapped = List.map (ref linked) (value -> value + 1)\n    let values = Vec.push (Vec.of_array sorted) 4\n    let tasks = new [Task<i64>](4, index -> task { return index + 1 })\n    let results = Task.run (Task.parallel tasks)\n    assert (source[0] == 3)\n    List.fold (ref mapped) 0 (total -> value -> total + value) + Vec.length (ref values) + Array.sum (ref results)\n\
        export def pattern_failure :: unit\nfn pattern_failure = for [only] in [[1, 2]] do assert (only == 0)\n\
        export def step_failure :: i64 -> unit\nfn step_failure step = for _value in 0 .. step .. 10 do ()\n\
        export def vec_failure :: i64 -> i64\nfn vec_failure index = { let values = Vec.push Vec.empty() 1; deref (Vec.at (ref values) index) }\n\
        stress()";
    fs::write(&input, source).unwrap();
    let project = tsuzuri::driver::Project::load(&input).unwrap();
    let module = project.analyze().unwrap();
    for optimization in [0, 3] {
        let native = root.join(format!("stress-{optimization}"));
        tsuzuri::driver::build(
            &module,
            &project,
            &native,
            tsuzuri::driver::BuildOptions {
                trap_info: true,
                optimization,
                ..Default::default()
            },
        )
        .unwrap();
        let output = Command::new(&native).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "23\n");
        let wasm = root.join(format!("stress-{optimization}.wasm"));
        tsuzuri::driver::build(
            &module,
            &project,
            &wasm,
            tsuzuri::driver::BuildOptions {
                target: tsuzuri::driver::Target::Wasm32,
                emit: tsuzuri::driver::Emit::Wasm,
                trap_info: true,
                optimization,
                ..Default::default()
            },
        )
        .unwrap();
        let script = root.join("check.mjs");
        fs::write(&script, "import assert from 'node:assert/strict'; import {readFileSync} from 'node:fs'; const module = new WebAssembly.Module(readFileSync(process.argv[2])); assert.deepEqual(WebAssembly.Module.imports(module), []); const sites=JSON.parse(readFileSync(process.argv[2]+'.trap.json')).sites; const fresh=()=>new WebAssembly.Instance(module).exports; assert.equal(fresh().tz_stress(),23n); for(const [name,args,kind] of [['pattern_failure',[],'pattern mismatch'],['step_failure',[0n],'range step is zero'],['vec_failure',[2n],'index out of bounds']]) { const api=fresh(); assert.throws(()=>api['tz_'+name](...args),WebAssembly.RuntimeError); const site=sites.find(site=>site.id===api.tsuzuri_trap_site()); assert.ok(site,name); assert.equal(site.kind,kind,name); }\n").unwrap();
        let output = Command::new("node")
            .arg(&script)
            .arg(&wasm)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parallel_callbacks_report_their_source_traps() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-parallel-traps-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let input = root.join("Main.tz");
    fs::write(&input, "export def fail :: i64\nfn fail = (Parallel.init 8193 (index -> { assert (index != 4096); index })).length\nfail()").unwrap();
    let project = tsuzuri::driver::Project::load(&input).unwrap();
    let module = project.analyze().unwrap();
    for optimization in [0, 3] {
        let executable = root.join(format!("native-{optimization}"));
        tsuzuri::driver::build(
            &module,
            &project,
            &executable,
            tsuzuri::driver::BuildOptions {
                optimization,
                trap_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        let output = Command::new(&executable).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("trap: assertion failed at"));
        let wasm = root.join(format!("parallel-{optimization}.wasm"));
        tsuzuri::driver::build(
            &module,
            &project,
            &wasm,
            tsuzuri::driver::BuildOptions {
                target: tsuzuri::driver::Target::Wasm32,
                emit: tsuzuri::driver::Emit::Wasm,
                optimization,
                trap_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        let script = "const fs=require('node:fs'),assert=require('node:assert/strict');const module=new WebAssembly.Module(fs.readFileSync(process.argv[1]));assert.deepEqual(WebAssembly.Module.imports(module),[]);const api=new WebAssembly.Instance(module).exports;assert.throws(()=>api.tz_fail(),WebAssembly.RuntimeError);const sites=JSON.parse(fs.readFileSync(process.argv[1]+'.trap.json')).sites;assert.equal(sites.find(site=>site.id===api.tsuzuri_trap_site()).kind,'assertion failed');";
        let output = Command::new("node")
            .args(["-e", script])
            .arg(&wasm)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trap_aware_ir_executes_on_native_and_wasm_at_both_levels() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let source = "export def divide :: i64 -> i64 -> i64\nfn divide left right = left / right\n\
        export def bound :: i64 -> i64\nfn bound index = { let values = [1, 2]; values[index] }\n\
        export def verify :: bool -> unit\nfn verify flag = assert flag\n\
        export def indirect :: bool -> unit\nfn indirect flag = { let check = assert; check flag }\n\
        export def allocate :: i64 -> i64\nfn allocate length = { let values = new [i64](length, index -> index); values.length }\n\
        export def soft :: i64 -> i64\nfn soft number = { let text = to_string (number as d128); let parsed: Option<d128> = Parse.parse (ref text); Option.get parsed as i64 }\n\
        export def closure :: i64 -> i64\nfn closure number = { let values = new [number, number + 1]; let read = index -> values[index]; let copied = read; copied 0 + read 1 }";
    let module = tsuzuri::analyze(source).unwrap();
    let sources = [TrapSource {
        path: "Main.tz",
        text: source,
    }];
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-traps-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let run = |command: &mut Command| {
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{command:?}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    let host = root.join("host.c");
    fs::write(&host, "#include <assert.h>\n#include <stdint.h>\n#include <stdbool.h>\nextern int64_t tz_divide(int64_t,int64_t), tz_bound(int64_t), tz_allocate(int64_t), tz_soft(int64_t), tz_closure(int64_t);\nextern void tz_verify(bool), tz_indirect(bool);\nint main(int argc,char **argv) { if(argc==1) { assert(tz_divide(84,2)==42); assert(tz_bound(1)==2); tz_verify(true); tz_indirect(true); assert(tz_allocate(2)==2); assert(tz_soft(42)==42); assert(tz_closure(20)==41); return 0; } switch(argv[1][0]) { case '0': tz_verify(false); break; case '1': tz_divide(1,0); break; case '2': tz_divide(INT64_MIN,-1); break; case '3': tz_bound(2); break; case '4': tz_allocate(-1); break; case '5': tz_indirect(false); break; } return 9; }").unwrap();
    let native_ir = root.join("native.ll");
    fs::write(
        &native_ir,
        tsuzuri::llvm::emit_with_trap_info(
            &module,
            tsuzuri::llvm::EmitOptions {
                entry: tsuzuri::llvm::Entry::Library,
                wasm: false,
                debug_output: false,
            },
            &sources,
        )
        .unwrap()
        .ir,
    )
    .unwrap();
    let wasm_ir = root.join("wasm.ll");
    let emitted = tsuzuri::llvm::emit_with_trap_info(
        &module,
        tsuzuri::llvm::EmitOptions {
            entry: tsuzuri::llvm::Entry::Library,
            wasm: true,
            debug_output: false,
        },
        &sources,
    )
    .unwrap();
    fs::write(
        &wasm_ir,
        format!("{}\n{}", emitted.ir, include_str!("../src/runtime/wasm.ll")),
    )
    .unwrap();
    fs::write(
        root.join("sites.json"),
        side_table(&emitted.trap_sites, &sources).unwrap(),
    )
    .unwrap();
    let script = root.join("check.mjs");
    fs::write(&script, "import assert from 'node:assert/strict'; import {readFileSync} from 'node:fs';\nconst module = new WebAssembly.Module(readFileSync(process.argv[2])); assert.deepEqual(WebAssembly.Module.imports(module), []); const sites=JSON.parse(readFileSync(process.argv[3])).sites; const fresh=()=>new WebAssembly.Instance(module).exports; const api=fresh(); assert.equal(api.tz_divide(84n,2n),42n); assert.equal(api.tz_soft(42n),42n); assert.equal(api.tz_closure(20n),41n); api.tz_indirect(1); for(const [name,args,kind] of [['verify',[0],'assertion failed'],['divide',[1n,0n],'integer division by zero'],['divide',[-9223372036854775808n,-1n],'integer division overflow'],['divide',[-9223372036854775808n,0n],'integer division by zero'],['bound',[2n],'index out of bounds'],['allocate',[-1n],'allocation size overflow'],['allocate',[3000000n],'allocation failed'],['indirect',[0],'assertion failed']]) { const api=fresh(); assert.throws(()=>api['tz_'+name](...args),WebAssembly.RuntimeError,name); const site=sites.find(site=>site.id===api.tsuzuri_trap_site()); assert.ok(site,name); assert.equal(site.kind,kind,name); assert.equal(site.path,'Main.tz'); }\n").unwrap();
    for optimization in ["-O0", "-O3"] {
        let native = root.join(format!("native{optimization}"));
        run(Command::new(&clang)
            .args([optimization, "-Wno-override-module"])
            .arg(&native_ir)
            .arg(&host)
            .args(["-lm", "-o"])
            .arg(&native));
        run(&mut Command::new(&native));
        for (index, kind) in [
            TrapKind::Assert,
            TrapKind::IntegerDivisionByZero,
            TrapKind::IntegerDivisionOverflow,
            TrapKind::BoundsCheck,
            TrapKind::AllocationSize,
            TrapKind::Assert,
        ]
        .into_iter()
        .enumerate()
        {
            let output = Command::new(&native)
                .arg(index.to_string())
                .output()
                .unwrap();
            assert!(!output.status.success());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.contains(&format!("trap: {} at Main.tz:", kind.description())),
                "{index}: {stderr}"
            );
        }
        let object = root.join("wasm.o");
        let wasm = root.join("module.wasm");
        run(Command::new(&clang)
            .args([
                "--target=wasm32-unknown-unknown",
                "-mbulk-memory",
                optimization,
                "-Wno-override-module",
                "-c",
            ])
            .arg(&wasm_ir)
            .arg("-o")
            .arg(&object));
        let linker = std::env::var_os("TSUZURI_WASM_LD").unwrap_or_else(|| "wasm-ld".into());
        run(Command::new(linker)
            .args([
                "--no-entry",
                "--stack-first",
                "-z",
                "stack-size=1048576",
                "--max-memory=16777216",
                "--export=tz_divide",
                "--export=tz_bound",
                "--export=tz_verify",
                "--export=tz_indirect",
                "--export=tz_allocate",
                "--export=tz_soft",
                "--export=tz_closure",
                "--export=tsuzuri_trap_site",
            ])
            .arg(&object)
            .arg("-o")
            .arg(&wasm));
        run(Command::new("node")
            .arg(&script)
            .arg(&wasm)
            .arg(root.join("sites.json")));
    }
    fs::remove_dir_all(root).unwrap();
}
