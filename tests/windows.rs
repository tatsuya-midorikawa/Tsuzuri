use std::collections::BTreeSet;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, llvm};

#[test]
fn windows_x64_i128_helpers_follow_the_win64_abi() {
    let weak_names = |source: &str, prefix: &str| -> BTreeSet<String> {
        source
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .map(|rest| rest.split('(').next().unwrap().to_owned())
            .collect()
    };
    let wasm_names = weak_names(
        include_str!("../src/runtime/wasm.ll"),
        "define weak hidden i128 @__",
    );
    let wrapper_names = weak_names(
        include_str!("../src/runtime/i128-windows-x64.ll"),
        "define weak hidden <2 x i64> @__",
    );
    assert_eq!(wasm_names.len(), 9);
    assert_eq!(wasm_names, wrapper_names);

    let module = analyze("export def answer :: i64\nfn answer = 42\n").unwrap();
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    for triple in [Triple::X86_64WindowsMsvc, Triple::X86_64WindowsGnu] {
        let _scope = PlatformScope::enter(Platform::new(Some(triple)));
        let ir =
            llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap();
        assert_eq!(
            ir,
            llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap()
        );
        assert_eq!(
            ir.matches("define weak hidden <2 x i64> @__").count(),
            9,
            "{triple:?}"
        );
        assert!(!ir.contains("define weak hidden i128 @__"), "{triple:?}");
        assert_eq!(
            ir.matches("define internal i128 @tz.runtime.").count(),
            10,
            "{triple:?}"
        );
        for name in &wasm_names {
            assert!(
                ir.contains(&format!("define weak hidden <2 x i64> @__{name}(ptr ")),
                "missing wrapper {name}"
            );
            assert!(
                ir.contains(&format!("define internal i128 @tz.runtime.{name}(")),
                "missing internal body {name}"
            );
        }
    }
    let _arm64 = PlatformScope::enter(Platform::new(Some(Triple::Aarch64WindowsMsvc)));
    let arm_ir =
        llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap();
    for name in &wasm_names {
        assert!(
            arm_ir.contains(&format!("define weak hidden i128 @__{name}(")),
            "Arm64 Windows keeps wasm.ll definitions: {name}"
        );
    }
    assert!(!arm_ir.contains("define weak hidden <2 x i64> @__"));
}

#[test]
fn windows_weak_definitions_are_comdat_any() {
    let owned = analyze(
        "export def make_tag :: i64 -> [ubyte]\nfn make_tag n = Array.init n (\\i -> (65 + i) as ubyte)\n",
    )
    .unwrap();
    let soft = analyze(
        "export def half_sum :: i64 -> i64\nfn half_sum n = ((n as f16) + 1.5f16) as i64\n",
    )
    .unwrap();
    let base = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };

    let check_ir = |ir: &str| -> Vec<String> {
        let mut expected = Vec::new();
        let mut seen = BTreeSet::new();
        for line in ir.lines() {
            if line.starts_with("define weak ") {
                let name = line
                    .split_once('@')
                    .and_then(|(_, rest)| rest.split_once('('))
                    .map(|(name, _)| name)
                    .expect("weak function name");
                assert!(
                    line.contains(" comdat {") || line.contains(" comdat !"),
                    "missing comdat on weak function: {line}"
                );
                if seen.insert(name.to_owned()) {
                    expected.push(name.to_owned());
                }
            } else if let Some((lhs, rhs)) = line.split_once(" = ")
                && lhs.starts_with('@')
                && rhs.starts_with("weak ")
            {
                let name = &lhs[1..];
                assert!(
                    line.contains(", comdat"),
                    "missing comdat on weak global: {line}"
                );
                if seen.insert(name.to_owned()) {
                    expected.push(name.to_owned());
                }
            }
        }
        let comdat_lines: Vec<String> = ir
            .lines()
            .filter_map(|line| {
                line.strip_prefix('$')
                    .and_then(|rest| rest.strip_suffix(" = comdat any"))
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(
            comdat_lines, expected,
            "comdat declarations must match weak definitions in order"
        );
        expected
    };

    for triple in [
        Triple::X86_64WindowsMsvc,
        Triple::X86_64WindowsGnu,
        Triple::Aarch64WindowsMsvc,
    ] {
        let _scope = PlatformScope::enter(Platform::new(Some(triple)));
        let sys_ir =
            llvm::emit_with_export_style(&owned, base, llvm::ExportStyle::Dllexport).unwrap();
        assert_eq!(
            sys_ir,
            llvm::emit_with_export_style(&owned, base, llvm::ExportStyle::Dllexport).unwrap()
        );
        let sys_names = check_ir(&sys_ir);
        assert!(sys_names.contains(&"tsuzuri_alloc".to_owned()));
        assert!(sys_names.contains(&"tsuzuri_free".to_owned()));
        assert!(sys_names.contains(&"__multi3".to_owned()));

        let small_options = llvm::EmitOptions {
            allocator: llvm::Allocator::Small,
            ..base
        };
        let small_ir =
            llvm::emit_with_export_style(&owned, small_options, llvm::ExportStyle::Dllexport)
                .unwrap();
        let small_names = check_ir(&small_ir);
        assert!(small_names.contains(&"tz_small_shared".to_owned()));
        assert!(small_names.contains(&"tz_small_stats".to_owned()));
        assert!(
            small_ir.contains(
                "@tz_small_shared = weak hidden global ptr @tz_small_own, comdat, align 8"
            ),
            "{small_ir}"
        );

        let soft_ir =
            llvm::emit_with_export_style(&soft, base, llvm::ExportStyle::Dllexport).unwrap();
        let soft_names = check_ir(&soft_ir);
        assert!(soft_names.contains(&"tz_soft_op".to_owned()));
    }

    let posix_ir = llvm::emit_with_options(&owned, base).unwrap();
    assert!(!posix_ir.contains("comdat"));
    let wasm_ir =
        llvm::emit_with_options(&owned, llvm::EmitOptions { wasm: true, ..base }).unwrap();
    assert!(!wasm_ir.contains("comdat"));
}

#[test]
fn emits_windows_exports_without_changing_default_or_wasm() {
    let module = analyze(
        "export def answer :: i64\nfn answer = { let value = 42; Debug.print (&value); value }",
    )
    .unwrap();
    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        debug_output: false,
        runtime_objects: false,
        wasm_simd128: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };
    let ir = llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap();
    assert_eq!(
        ir,
        llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap()
    );
    assert!(ir.contains("define dllexport i64 @tz_answer("));
    assert!(ir.contains("declare i32 @_write(i32, ptr, i32)"));
    assert!(!ir.contains("declare i64 @write("));
    assert_eq!(
        ir.matches("declare void @OutputDebugStringW(ptr)").count(),
        1
    );
    assert_eq!(
        ir.matches("define internal void @tz.debug.write(").count(),
        1
    );
    let posix_ir = llvm::emit_with_options(&module, options).unwrap();
    assert!(!posix_ir.contains("dllexport"));
    assert!(!posix_ir.contains("OutputDebugStringW"));
    assert_eq!(
        llvm::emit_with_export_style(
            &module,
            llvm::EmitOptions {
                wasm: true,
                ..options
            },
            llvm::ExportStyle::Dllexport
        )
        .unwrap_err()
        .code,
        "E2000"
    );
    let main = analyze("42").unwrap();
    let ir = llvm::emit_with_export_style(
        &main,
        llvm::EmitOptions {
            entry: llvm::Entry::Console,
            ..options
        },
        llvm::ExportStyle::Dllexport,
    )
    .unwrap();
    assert!(ir.contains("%binary_mode = call i32 @_setmode(i32 1, i32 32768)"));
}

#[test]
fn cross_links_with_microsoft_sdk_when_requested() {
    let Some(sdk) = std::env::var_os("TSUZURI_WINDOWS_SDK") else {
        return;
    };
    let sdk = std::path::PathBuf::from(sdk);
    // The tzroutine runtime, which runs Parallel, Routine.scope, Mutex, ErrGroup and DataChannel
    // natively (TZ12).
    for source in [
        "export def answer :: i64\nfn answer =\n    let value = 42\n    Debug.print (&value)\n    let group: ErrGroup<string> = ErrGroup.create ()\n    ErrGroup.go group (Owned.function (\\() -> Result.Ok ()))\n    let values = Parallel.init 1 (\\_index -> value)\n    match ErrGroup.wait group with\n    | Result.Ok _ -> values[0]\n    | Result.Error _ -> 0",
        "export def answer :: i64\nfn answer =\n    let lock = Mutex.create 40i64\n    let seen = Routine.scope (ref lock) 2 (\\shared index -> Mutex.with_lock shared (\\value -> { deref value = deref value + 1; index }))\n    Mutex.into_inner lock + Array.length (ref seen) - 2",
        "export def answer :: i64\nfn answer =\n    match DataChannel.bounded 2 with\n    | (sender, receiver) ->\n        let _sent = DataChannel.send (ref sender) 42\n        match DataChannel.recv (ref receiver) with\n        | Maybe.Some item -> item\n        | Maybe.None -> 0",
    ] {
        cross_link(&sdk, source);
    }
}

/// Builds `source`, whose `answer` returns 42, with the Microsoft SDK and links it with `tzroutine.c`
/// (compiled where it is, next to the `tzroutine-windows.h` that it includes).
fn cross_link(sdk: &std::path::Path, source: &str) {
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-coff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let module = analyze(source).unwrap();
    let _scope = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
    let ir = llvm::emit_with_export_style(
        &module,
        llvm::EmitOptions {
            entry: llvm::Entry::Library,
            wasm: false,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: llvm::Allocator::System,
            f16: llvm::F16Path::Soft,
        },
        llvm::ExportStyle::Dllexport,
    )
    .unwrap();
    fs::write(root.join("module.ll"), ir).unwrap();
    fs::write(root.join("host.c"), "#include <stdint.h>\nextern int64_t tz_answer(void);\nint main(void) { return tz_answer() == 42 ? 0 : 1; }\n").unwrap();
    let check = |command: &mut Command| {
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{command:?}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    for optimization in ["-O0", "-O3"] {
        check(
            Command::new(&clang)
                .args([
                    "--target=x86_64-pc-windows-msvc",
                    "-Wno-override-module",
                    "-c",
                    optimization,
                ])
                .arg(root.join("module.ll"))
                .arg("-o")
                .arg(root.join("module.obj")),
        );
        for (source, object) in [
            (root.join("host.c"), "host.obj"),
            (
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/tzroutine.c"),
                "tzroutine.obj",
            ),
        ] {
            let mut command = Command::new(&clang);
            command.args([
                "--target=x86_64-pc-windows-msvc",
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
            ]);
            for include in [
                "crt/include",
                "sdk/include/ucrt",
                "sdk/include/shared",
                "sdk/include/um",
            ] {
                command.arg("-isystem").arg(sdk.join(include));
            }
            check(
                command
                    .arg("-c")
                    .arg(source)
                    .arg("-o")
                    .arg(root.join(object)),
            );
        }
        let mut command =
            Command::new(std::env::var_os("TSUZURI_LLD_LINK").unwrap_or_else(|| "lld-link".into()));
        command.args([
            "/entry:mainCRTStartup",
            "/subsystem:console",
            "/nodefaultlib:libcmt",
            "libcmt.lib",
            "libvcruntime.lib",
            "libucrt.lib",
            "kernel32.lib",
        ]);
        for library in ["crt/lib/x86_64", "sdk/lib/ucrt/x86_64", "sdk/lib/um/x86_64"] {
            command.arg(format!("/libpath:{}", sdk.join(library).display()));
        }
        command.arg(format!("/out:{}", root.join("program.exe").display()));
        for object in ["host.obj", "module.obj", "tzroutine.obj"] {
            command.arg(root.join(object));
        }
        check(&mut command);
        let bytes = fs::read(root.join("program.exe")).unwrap();
        assert_eq!(&bytes[..2], b"MZ");
    }
    fs::remove_dir_all(root).unwrap();
}

/// The socket runtime of the Net module (E09) compiles and links for Windows: the Winsock calls, on x86_64 (linked into an
/// executable) and on aarch64 (compiled to objects, because the SDK of the machine has no ARM64 import libraries). It does
/// not run: Windows execution is verified by the CI jobs.
#[test]
fn cross_links_the_net_runtime_with_microsoft_sdk_when_requested() {
    let Some(sdk) = std::env::var_os("TSUZURI_WINDOWS_SDK") else {
        return;
    };
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let sdk = PathBuf::from(sdk);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-net-coff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let! outcome = Net.connect address (Maybe.Some 100)
    do! IO.writeln (Result.is_ok (ref outcome))
    0
";
    let module = tsuzuri::analyze_modules(&[("Main.tz", source)]).unwrap();
    // The driver concatenates the runtimes of one program into one translation unit, with net.c first.
    let runtime: String = ["net.c", "io.c"]
        .iter()
        .map(|name| {
            fs::read_to_string(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/runtime")
                    .join(name),
            )
            .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("runtime.c"), runtime).unwrap();
    let check = |command: &mut Command| {
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{command:?}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let includes = [
        "crt/include",
        "sdk/include/ucrt",
        "sdk/include/shared",
        "sdk/include/um",
    ];
    for (triple, target, architecture) in [
        (
            Triple::X86_64WindowsMsvc,
            "x86_64-pc-windows-msvc",
            "x86_64",
        ),
        (
            Triple::Aarch64WindowsMsvc,
            "aarch64-pc-windows-msvc",
            "aarch64",
        ),
    ] {
        let ir = {
            let _scope = PlatformScope::enter(Platform::new(Some(triple)));
            llvm::emit_with_export_style(
                &module,
                llvm::EmitOptions {
                    entry: llvm::Entry::Console,
                    wasm: false,
                    debug_output: false,
                    runtime_objects: false,
                    wasm_simd128: false,
                    allocator: llvm::Allocator::System,
                    f16: llvm::F16Path::Soft,
                },
                llvm::ExportStyle::Dllexport,
            )
            .unwrap()
        };
        assert!(ir.contains("declare i64 @tsuzuri_net_open("));
        // The allocator of a Windows host has no POSIX-only hooks that a host of another kind adds to the IR.
        let ir = ir.replace(" weak hidden global ", " internal global ");
        let module_ll = root.join(format!("module-{architecture}.ll"));
        fs::write(&module_ll, ir).unwrap();
        for optimization in ["-O0", "-O3"] {
            check(
                Command::new(&clang)
                    .args([
                        &format!("--target={target}"),
                        "-Wno-override-module",
                        "-c",
                        optimization,
                    ])
                    .arg(&module_ll)
                    .arg("-o")
                    .arg(root.join(format!("module-{architecture}.obj"))),
            );
            let mut command = Command::new(&clang);
            command.args([
                &format!("--target={target}"),
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
            ]);
            for include in includes {
                command.arg("-isystem").arg(sdk.join(include));
            }
            check(
                command
                    .arg("-c")
                    .arg(root.join("runtime.c"))
                    .arg("-o")
                    .arg(root.join(format!("runtime-{architecture}.obj"))),
            );
            if architecture != "x86_64" {
                continue;
            }
            // The import library of Winsock comes from the object itself (a default-library request), not from here.
            let mut linker = Command::new(
                std::env::var_os("TSUZURI_LLD_LINK").unwrap_or_else(|| "lld-link".into()),
            );
            linker.args([
                "/entry:mainCRTStartup",
                "/subsystem:console",
                "/nodefaultlib:libcmt",
                "libcmt.lib",
                "libvcruntime.lib",
                "libucrt.lib",
                "kernel32.lib",
            ]);
            for library in ["crt/lib/x86_64", "sdk/lib/ucrt/x86_64", "sdk/lib/um/x86_64"] {
                linker.arg(format!("/libpath:{}", sdk.join(library).display()));
            }
            linker.arg(format!("/out:{}", root.join("program.exe").display()));
            linker.arg(root.join("module-x86_64.obj"));
            linker.arg(root.join("runtime-x86_64.obj"));
            check(&mut linker);
            let bytes = fs::read(root.join("program.exe")).unwrap();
            assert_eq!(&bytes[..2], b"MZ");
            // The program imports Winsock, so the library really was requested and resolved.
            assert!(
                bytes
                    .windows(b"WS2_32.dll".len())
                    .any(|window| window.eq_ignore_ascii_case(b"WS2_32.dll")),
                "the executable imports ws2_32"
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_outputs_protect_hardlinks_and_replace_existing_files() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-windows-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = root.join("Main.tz");
    let output = root.join("out.ll");
    fs::write(&source, "42").unwrap();
    fs::hard_link(&source, &output).unwrap();
    let execute = || {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                source.to_str().unwrap(),
                "--emit",
                "llvm",
                "-o",
                output.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let result = execute();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("E2003"));
    fs::remove_file(&output).unwrap();
    fs::write(&output, "old output").unwrap();
    assert!(execute().status.success());
    assert!(execute().status.success());
    assert_eq!(fs::read_to_string(&source).unwrap(), "42");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cross_links_the_os_runtime_with_microsoft_sdk_when_requested() {
    let Some(sdk) = std::env::var_os("TSUZURI_WINDOWS_SDK") else {
        return;
    };
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let sdk = PathBuf::from(sdk);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-os-coff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = "def main :: unit -> i32 = \\() ->
    let! _a = File.read_text \"missing.txt\"
    let! _b = Env.var \"PATH\"
    let! _t = Time.monotonic_ns ()
    let! _r = Random.bytes 8
    0
";
    let module = tsuzuri::analyze_modules(&[("Main.tz", source)]).unwrap();
    let runtime_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime");
    let os_src = fs::read_to_string(runtime_dir.join("os.c"))
        .unwrap()
        .replacen(
            "#include \"os-windows.h\"",
            &fs::read_to_string(runtime_dir.join("os-windows.h")).unwrap(),
            1,
        )
        .replacen(
            "#include \"arguments-split.h\"",
            &fs::read_to_string(runtime_dir.join("arguments-split.h")).unwrap(),
            1,
        );
    let io_src = fs::read_to_string(runtime_dir.join("io.c")).unwrap();
    fs::write(root.join("runtime.c"), format!("{os_src}\n{io_src}")).unwrap();
    let check = |command: &mut Command| {
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{command:?}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let includes = [
        "crt/include",
        "sdk/include/ucrt",
        "sdk/include/shared",
        "sdk/include/um",
    ];
    for (triple, target, architecture) in [
        (
            Triple::X86_64WindowsMsvc,
            "x86_64-pc-windows-msvc",
            "x86_64",
        ),
        (
            Triple::Aarch64WindowsMsvc,
            "aarch64-pc-windows-msvc",
            "aarch64",
        ),
    ] {
        let ir = {
            let _scope = PlatformScope::enter(Platform::new(Some(triple)));
            llvm::emit_with_export_style(
                &module,
                llvm::EmitOptions {
                    entry: llvm::Entry::Console,
                    wasm: false,
                    debug_output: false,
                    runtime_objects: false,
                    wasm_simd128: false,
                    allocator: llvm::Allocator::System,
                    f16: llvm::F16Path::Soft,
                },
                llvm::ExportStyle::Dllexport,
            )
            .unwrap()
        };
        assert!(ir.contains("declare i64 @tsuzuri_os_read("));
        let ir = ir.replace(" weak hidden global ", " internal global ");
        let module_ll = root.join(format!("module-{architecture}.ll"));
        fs::write(&module_ll, ir).unwrap();
        for optimization in ["-O0", "-O3"] {
            check(
                Command::new(&clang)
                    .args([
                        &format!("--target={target}"),
                        "-Wno-override-module",
                        "-c",
                        optimization,
                    ])
                    .arg(&module_ll)
                    .arg("-o")
                    .arg(root.join(format!("module-{architecture}.obj"))),
            );
            let mut command = Command::new(&clang);
            command.args([
                &format!("--target={target}"),
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
            ]);
            for include in includes {
                command.arg("-isystem").arg(sdk.join(include));
            }
            check(
                command
                    .arg("-c")
                    .arg(root.join("runtime.c"))
                    .arg("-o")
                    .arg(root.join(format!("runtime-{architecture}.obj"))),
            );
            if architecture != "x86_64" {
                continue;
            }
            let mut linker = Command::new(
                std::env::var_os("TSUZURI_LLD_LINK").unwrap_or_else(|| "lld-link".into()),
            );
            linker.args([
                "/entry:mainCRTStartup",
                "/subsystem:console",
                "/nodefaultlib:libcmt",
                "libcmt.lib",
                "libvcruntime.lib",
                "libucrt.lib",
                "kernel32.lib",
                "bcrypt.lib",
            ]);
            for library in ["crt/lib/x86_64", "sdk/lib/ucrt/x86_64", "sdk/lib/um/x86_64"] {
                linker.arg(format!("/libpath:{}", sdk.join(library).display()));
            }
            linker.arg(format!("/out:{}", root.join("program.exe").display()));
            linker.arg(root.join("module-x86_64.obj"));
            linker.arg(root.join("runtime-x86_64.obj"));
            check(&mut linker);
            let bytes = fs::read(root.join("program.exe")).unwrap();
            assert_eq!(&bytes[..2], b"MZ");
            assert!(
                bytes
                    .windows(b"bcrypt.dll".len())
                    .any(|window| window.eq_ignore_ascii_case(b"bcrypt.dll")),
                "the executable imports bcrypt.dll"
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}
