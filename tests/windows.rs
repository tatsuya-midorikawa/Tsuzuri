use tsuzuri::{analyze, llvm};

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
    };
    let ir = llvm::emit_with_export_style(&module, options, llvm::ExportStyle::Dllexport).unwrap();
    assert!(ir.contains("define dllexport i64 @tz_answer("));
    assert!(ir.contains("declare i32 @_write(i32, ptr, i32)"));
    assert!(!ir.contains("declare i64 @write("));
    assert!(
        !llvm::emit_with_options(&module, options)
            .unwrap()
            .contains("dllexport")
    );
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
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let sdk = PathBuf::from(sdk);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-coff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = "export def answer :: i64\nfn answer = { let value = 42; Debug.print (&value); let jobs: [Task<Result<i64, string>>] = [task { Result.Ok value }]; let values = Result.get (Task.run (Task.parallel_results jobs)); values[0] }";
    let module = analyze(source).unwrap();
    let ir = llvm::emit_with_export_style(
        &module,
        llvm::EmitOptions {
            entry: llvm::Entry::Library,
            wasm: false,
            debug_output: false,
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
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/task.c"),
                "task.obj",
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
        for object in ["host.obj", "module.obj", "task.obj"] {
            command.arg(root.join(object));
        }
        check(&mut command);
        let bytes = fs::read(root.join("program.exe")).unwrap();
        assert_eq!(&bytes[..2], b"MZ");
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
