use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    let executable = env::current_exe().expect("cannot locate the bundled Clang launcher");
    let root = executable.parent().unwrap().parent().unwrap();
    let zig = env::var_os("TSUZURI_ZIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join("zig")
                .join(if cfg!(windows) { "zig.exe" } else { "zig" })
        });
    let clang = env::var_os("TSUZURI_LLVM_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join("bin")
                .join(if cfg!(windows) { "clang.exe" } else { "clang" })
        });
    let mut arguments: Vec<_> = env::args_os().skip(1).collect();
    let wasm = arguments
        .iter()
        .any(|argument| argument == "--target=wasm32-unknown-unknown");
    arguments.retain(|argument| {
        argument != "--target=wasm32-unknown-unknown"
            && argument != "--target=x86_64-pc-windows-msvc"
            && argument != "--target=aarch64-pc-windows-msvc"
    });
    let target = if wasm {
        "wasm32-freestanding".to_owned()
    } else {
        let architecture = match env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => {
                eprintln!("the bundled native toolchain requires x64 or ARM64");
                return ExitCode::FAILURE;
            }
        };
        let system = match env::consts::OS {
            "macos" => "macos.12.0",
            "linux" => "linux-musl",
            "windows" => "windows-gnu",
            _ => {
                eprintln!("unsupported Tsuzuri toolchain operating system");
                return ExitCode::FAILURE;
            }
        };
        format!("{architecture}-{system}")
    };
    let llvm_target = if wasm {
        "wasm32-unknown-unknown".to_owned()
    } else {
        let system = match env::consts::OS {
            "macos" => "apple-macosx12.0.0",
            "linux" => "unknown-linux-musl",
            "windows" => "w64-windows-gnu",
            _ => unreachable!(),
        };
        format!("{}-{system}", env::consts::ARCH)
    };
    if arguments.iter().any(|argument| argument == "--version") {
        if !Command::new(&clang)
            .arg("--version")
            .status()
            .is_ok_and(|status| status.success())
        {
            return ExitCode::FAILURE;
        }
    }
    let mut temporary = None;
    if let Some(language) = arguments
        .windows(2)
        .position(|pair| pair[0] == "-x" && pair[1] == "ir")
    {
        arguments.drain(language..language + 2);
        let Some(input) = arguments.iter().position(|argument| {
            PathBuf::from(argument)
                .extension()
                .is_some_and(|extension| extension == "ll")
        }) else {
            eprintln!("missing LLVM IR input");
            return ExitCode::FAILURE;
        };
        let mut compilation = Command::new(&clang);
        compilation.args(["-target", &llvm_target, "-x", "ir", "-c"]);
        for argument in &arguments {
            let text = argument.to_string_lossy();
            if text.starts_with("-O")
                || text.starts_with("-W")
                || text.starts_with("-m")
                || text == "-g"
                || text == "-fPIC"
            {
                compilation.arg(argument);
            }
        }
        compilation.arg(&arguments[input]);
        if arguments.iter().any(|argument| argument == "-c") {
            if let Some(output) = arguments.iter().position(|argument| argument == "-o") {
                compilation.arg("-o").arg(&arguments[output + 1]);
            }
            return execute(&mut compilation);
        }
        let directory = env::temp_dir().join(format!(
            "tsuzuri-clang-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if let Err(error) = fs::create_dir(&directory) {
            eprintln!("cannot create Clang staging directory: {error}");
            return ExitCode::FAILURE;
        }
        let object = directory.join("module.o");
        compilation.arg("-o").arg(&object);
        let result = execute(&mut compilation);
        if result != ExitCode::SUCCESS {
            let _ = fs::remove_dir_all(&directory);
            return result;
        }
        arguments[input] = OsString::from(object);
        temporary = Some(directory);
    }
    let result = execute(
        Command::new(zig)
            .args(["cc", "-target", &target])
            .args(arguments),
    );
    if let Some(directory) = temporary {
        let _ = fs::remove_dir_all(directory);
    }
    result
}

fn execute(command: &mut Command) -> ExitCode {
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!(
                "'{}' failed ({status})",
                command.get_program().to_string_lossy()
            );
            // Truncating a Windows NTSTATUS such as 0xC0000005 could report success.
            match status.code() {
                Some(code @ 1..=255) => ExitCode::from(code as u8),
                _ => ExitCode::FAILURE,
            }
        }
        Err(error) => {
            eprintln!("cannot start the bundled toolchain: {error}");
            ExitCode::FAILURE
        }
    }
}
