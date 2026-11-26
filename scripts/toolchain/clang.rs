use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

/// What the compiler asked for with `--target=<Clang target>`, as the two compilers need it.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub wasm: bool,
    /// Zig's target for linking and for compiling C, if Zig has one.
    pub zig: Option<String>,
    /// The target of the LLVM Clang that compiles IR.
    pub llvm: String,
}

/// The Clang targets that the compiler passes for `--target TRIPLE`, with Zig's name for each. Zig
/// ships the C library of every one of them, using MinGW for the Windows MSVC triples.
const NATIVE_TARGETS: [(&str, &str); 9] = [
    ("arm64-apple-macosx12.0.0", "aarch64-macos.12.0"),
    ("aarch64-unknown-linux-gnu", "aarch64-linux-gnu"),
    ("x86_64-unknown-linux-gnu", "x86_64-linux-gnu"),
    ("aarch64-unknown-linux-musl", "aarch64-linux-musl"),
    ("x86_64-unknown-linux-musl", "x86_64-linux-musl"),
    ("aarch64-w64-windows-gnu", "aarch64-windows-gnu"),
    ("x86_64-w64-windows-gnu", "x86_64-windows-gnu"),
    ("aarch64-pc-windows-msvc", "aarch64-windows-gnu"),
    ("x86_64-pc-windows-msvc", "x86_64-windows-gnu"),
];

fn parse_macos_version(text: &str) -> Option<(u16, u16, u16)> {
    if text.is_empty() {
        return None;
    }
    let mut parts = text.split('.');
    let part = |s: &str| -> Option<u16> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok())?
    };
    let major = part(parts.next()?)?;
    let minor = match parts.next() {
        Some(s) => part(s)?,
        None => 0,
    };
    let patch = match parts.next() {
        Some(s) => part(s)?,
        None => 0,
    };
    if parts.next().is_some() || major < 12 {
        return None;
    }
    Some((major, minor, patch))
}

fn zig_macos_version((major, minor, patch): (u16, u16, u16)) -> String {
    if patch == 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

/// Maps the `--target=` value of a run, if it has one, and the machine the launcher runs on.
///
/// Without a value the machine decides: macOS 12 or later, musl Linux, and MinGW Windows. The MSVC
/// targets name the MSVC runtime, which Zig does not have: only a Windows launcher accepts them,
/// and it links MinGW for them, as it always has for the target the compiler passes there.
pub fn resolve(target: Option<&str>, os: &str, arch: &str) -> Result<Request, String> {
    resolve_with_macos(target, None, None, os, arch)
}

pub fn resolve_with_macos(
    target: Option<&str>,
    mmacosx_min: Option<&str>,
    deployment_env: Option<&str>,
    os: &str,
    arch: &str,
) -> Result<Request, String> {
    if os == "macos" && arch != "aarch64" {
        return Err("the macOS toolchain requires ARM64".to_owned());
    }
    match target {
        Some("wasm32-unknown-unknown") => Ok(Request {
            wasm: true,
            zig: Some("wasm32-freestanding".to_owned()),
            llvm: "wasm32-unknown-unknown".to_owned(),
        }),
        Some(value) => {
            if value
                .strip_prefix("x86_64-apple-")
                .is_some_and(|system| system.starts_with("darwin") || system.starts_with("macos"))
            {
                return Err("the macOS toolchain supports only ARM64 targets".to_owned());
            }
            if let Some(version_str) = value
                .strip_prefix("arm64-apple-macosx")
                .or_else(|| value.strip_prefix("aarch64-apple-macosx"))
            {
                let Some(version) = parse_macos_version(version_str) else {
                    return Err(format!(
                        "invalid macOS target version '{version_str}' in '{value}'; expected 12.0 or later"
                    ));
                };
                return Ok(Request {
                    wasm: false,
                    zig: Some(format!("aarch64-macos.{}", zig_macos_version(version))),
                    llvm: value.to_owned(),
                });
            }
            let windows_msvc = value.ends_with("-pc-windows-msvc");
            let architecture = value.split('-').next().unwrap_or_default();
            if windows_msvc && os != "windows" {
                return Err(format!(
                    "the bundled toolchain links MinGW, not the MSVC runtime of {value}; build {architecture}-pc-windows-gnu instead"
                ));
            }
            let zig = NATIVE_TARGETS
                .iter()
                .find(|(clang, _)| *clang == value)
                .map(|(_, zig)| (*zig).to_owned());
            Ok(Request {
                wasm: false,
                zig,
                llvm: if windows_msvc {
                    format!("{architecture}-w64-windows-gnu")
                } else {
                    value.to_owned()
                },
            })
        }
        None => {
            let architecture = match arch {
                "x86_64" => "x86_64",
                "aarch64" => "aarch64",
                _ => return Err("the bundled native toolchain requires x64 or ARM64".to_owned()),
            };
            let (zig_system, llvm_system) = match os {
                "macos" => {
                    let chosen = mmacosx_min
                        .or_else(|| deployment_env.filter(|s| !s.is_empty()))
                        .unwrap_or("12.0");
                    let Some(ver) = parse_macos_version(chosen) else {
                        return Err(format!(
                            "invalid macOS minimum version '{chosen}'; expected 12.0 or later"
                        ));
                    };
                    (
                        format!("macos.{}", zig_macos_version(ver)),
                        format!("apple-macosx{}.{}.{}", ver.0, ver.1, ver.2),
                    )
                }
                "linux" => ("linux-musl".to_owned(), "unknown-linux-musl".to_owned()),
                "windows" => ("windows-gnu".to_owned(), "w64-windows-gnu".to_owned()),
                _ => return Err("unsupported Tsuzuri toolchain operating system".to_owned()),
            };
            Ok(Request {
                wasm: false,
                zig: Some(format!("{architecture}-{zig_system}")),
                llvm: format!("{arch}-{llvm_system}"),
            })
        }
    }
}

/// The arguments that Zig answers with a warning: it brings its own C library, and always links
/// with LLD.
fn is_dropped(argument: &str) -> bool {
    argument == "-fuse-ld=lld" || argument == "-Werror=incompatible-sysroot"
}

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
    let requested = arguments
        .iter()
        .position(|argument| {
            argument
                .to_str()
                .is_some_and(|text| text.starts_with("--target="))
        })
        .map(|position| arguments.remove(position));
    let mmacosx_min = arguments
        .iter()
        .position(|argument| {
            argument
                .to_str()
                .is_some_and(|text| text.starts_with("-mmacosx-version-min="))
        })
        .map(|position| arguments.remove(position));
    let env_deployment = env::var("MACOSX_DEPLOYMENT_TARGET").ok();
    let request = match resolve_with_macos(
        requested
            .as_ref()
            .and_then(|argument| argument.to_str())
            .and_then(|text| text.strip_prefix("--target=")),
        mmacosx_min
            .as_ref()
            .and_then(|argument| argument.to_str())
            .and_then(|text| text.strip_prefix("-mmacosx-version-min=")),
        env_deployment.as_deref(),
        env::consts::OS,
        env::consts::ARCH,
    ) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    arguments.retain(|argument| !argument.to_str().is_some_and(is_dropped));
    if arguments.iter().any(|argument| {
        argument
            .to_str()
            .is_some_and(|text| text == "-isysroot" || text.starts_with("--sysroot"))
    }) {
        eprintln!(
            "the bundled toolchain brings its own C library and cannot use a sysroot; unset TSUZURI_SYSROOT, or set TSUZURI_CLANG to a Clang that uses it"
        );
        return ExitCode::FAILURE;
    }
    if arguments.iter().any(|argument| argument == "--version")
        && !Command::new(&clang)
            .arg("--version")
            .status()
            .is_ok_and(|status| status.success())
    {
        return ExitCode::FAILURE;
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
        compilation.args(["-target", &request.llvm, "-x", "ir", "-c"]);
        for argument in &arguments {
            let text = argument.to_string_lossy();
            if text.starts_with("-O")
                || text.starts_with("-W")
                || text.starts_with("-m")
                || text == "-g"
                || text == "-fPIC"
                // `-flto=thin` makes this compile write bitcode with the ThinLTO summary (PR08).
                || text.starts_with("-flto")
                || text == "-ffunction-sections"
                || text == "-fdata-sections"
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
    let Some(zig_target) = request.zig else {
        if let Some(directory) = temporary {
            let _ = fs::remove_dir_all(directory);
        }
        eprintln!(
            "the bundled toolchain has no C library for {}; set TSUZURI_CLANG to a Clang with that target's SDK",
            request.llvm
        );
        return ExitCode::FAILURE;
    };
    // Zig 0.16.0 crashes while linking on ARM64 Windows hosts, so that bundle ships llvm-mingw.
    let mut command = if cfg!(all(windows, target_arch = "aarch64")) && !request.wasm {
        if !(request.llvm.starts_with("aarch64-") && request.llvm.ends_with("-windows-gnu")) {
            if let Some(directory) = temporary {
                let _ = fs::remove_dir_all(directory);
            }
            eprintln!(
                "the bundled ARM64 toolchain links ARM64 Windows only, not {}",
                request.llvm
            );
            return ExitCode::FAILURE;
        }
        let sysroot = root.join("mingw");
        let mut command = Command::new(&clang);
        command
            .args(["-target", &request.llvm, "--sysroot"])
            .arg(&sysroot)
            .arg("-resource-dir")
            .arg(sysroot.join("lib").join("clang").join("21"));
        if !arguments
            .iter()
            .any(|argument| argument == "-c" || argument == "-S" || argument == "-E")
        {
            let mut linker = OsString::from("--ld-path=");
            linker.push(root.join("bin").join("ld.lld.exe"));
            command
                .args(["-fuse-ld=lld", "-rtlib=compiler-rt", "-unwindlib=none"])
                .arg(linker);
        }
        command
    } else {
        let mut command = Command::new(zig);
        command.args(["cc", "-target", &zig_target]);
        command
    };
    let result = execute(command.args(arguments));
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
