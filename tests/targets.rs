//! Native targets named with `--target TRIPLE` (G15): the IR and the objects they produce, the
//! tools they ask for, and the runner that executes what this machine cannot.
//!
//! Nothing here runs a foreign executable. The expected values come from the object formats'
//! own specifications (ELF gABI and its RISC-V psABI, Apple's `<mach-o/loader.h>` and
//! `<mach/machine.h>`, Microsoft's PE/COFF) and not from the compiler's output.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::driver::{self, BuildOptions, Emit, Project};
use tsuzuri::target::{Arch, Os, Platform, PlatformScope, Triple};
use tsuzuri::{analyze, llvm, trap::TrapSource};

#[path = "../scripts/toolchain/clang.rs"]
#[allow(dead_code)]
mod shim;

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-targets-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

const ANSWER: &str = "export def answer :: i64\nfn answer = 42\n";

const DOT: &str = "@cpu [\"sse4.2\", \"avx2\", \"sve\"]
export def dot :: ref [f64] -> ref [f64] -> f64
fn dot left right =
    let mut total: f64x4 = Simd.splat 0.0
    let mut index = 0
    while index + 4 <= left.length do
        let first: f64x4 = Simd.load left index
        let second: f64x4 = Simd.load right index
        total = total + first * second
        index = index + 4
    let mut sum = Simd.sum_lanes total
    while index < left.length do
        sum = sum + left[index] * right[index]
        index = index + 1
    sum

export def fused :: f64 -> f64 -> f64 -> f64
fn fused first second third = Math.fma first second third
";

fn sources(source: &str) -> Vec<TrapSource<'_>> {
    std::iter::once(TrapSource {
        path: "Main.tz",
        text: source,
    })
    .chain(
        tsuzuri::stdlib::SOURCES
            .iter()
            .map(|(path, text)| TrapSource { path, text }),
    )
    .collect()
}

/// The native IR of `source` as it is emitted for `platform`, with the CPU dispatch of a build.
fn native_ir(source: &str, platform: Platform) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{error:?}"));
    let _scope = PlatformScope::enter(platform);
    llvm::emit_native_build(
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
        &sources(source),
        None,
        false,
    )
    .unwrap()
    .ir
}

#[test]
fn native_ir_follows_the_target_it_is_emitted_for() {
    let mut classes: BTreeMap<(bool, bool, u8), Vec<(Triple, String)>> = BTreeMap::new();
    for triple in Triple::ALL {
        let platform = Platform::new(Some(triple));
        let ir = native_ir(DOT, platform);
        // The fused instruction exists for AArch64; elsewhere `Math.fma` is the exact software routine.
        let hardware = triple.arch() == Arch::Aarch64;
        assert_eq!(ir.contains("@llvm.fma.f64"), hardware, "{}", triple.name());
        assert_eq!(
            ir.contains("call void @tz_soft_fma("),
            !hardware,
            "{}",
            triple.name()
        );
        // The allocator without ELF-style weak hooks is the one of COFF targets.
        let windows = triple.os() == Os::Windows;
        assert_eq!(
            ir.contains("@tsuzuri_trap_hooks = weak hidden global"),
            !windows,
            "{}",
            triple.name()
        );
        // `@cpu` functions get the versions that `src/runtime/cpu.c` detects on the target: SSE4.2
        // and AVX2 with CPUID on x86-64 off Windows, SVE with the auxiliary vector on a Linux kernel.
        let expected: &[&str] = match (triple.arch(), triple.os()) {
            (Arch::X86_64, Os::Windows) => &[],
            (Arch::X86_64, _) => &["sse4.2", "avx2"],
            (Arch::Aarch64, Os::Linux | Os::Android) => &["sve"],
            _ => &[],
        };
        for level in ["sse4.2", "avx2", "sve"] {
            assert_eq!(
                ir.contains(&format!("@\"tz.fn.Main.dot.cpu.{level}\"")),
                expected.contains(&level),
                "{} {level}",
                triple.name()
            );
        }
        if expected.is_empty() {
            assert!(!ir.contains(".cpu."), "{}", triple.name());
        }
        classes
            .entry((windows, hardware, platform.cpu_levels()))
            .or_default()
            .push((triple, ir));
    }
    // Nothing else depends on the target: the targets of one class get the same IR.
    for (class, members) in &classes {
        for (triple, ir) in members {
            assert_eq!(
                *ir,
                members[0].1,
                "{} differs from {} in {class:?}",
                triple.name(),
                members[0].0.name()
            );
        }
    }
    assert!(classes.len() > 3);
}

#[test]
fn the_host_platform_and_its_named_triple_emit_the_same_ir() {
    let Some(host) = Triple::host() else {
        return;
    };
    // An Android host is `target_os = "android"`, which the host checks do not treat as Linux.
    if host.os() == Os::Android {
        return;
    }
    assert_eq!(
        native_ir(DOT, Platform::HOST),
        native_ir(DOT, Platform::new(Some(host)))
    );
}

fn load(directory: &Path, text: &str) -> (Project, tsuzuri::check::CheckedModule) {
    let path = directory.join("Answer.tz");
    fs::write(&path, text).unwrap();
    let project = Project::load(&path).unwrap();
    let module = project.analyze().unwrap();
    (project, module)
}

#[test]
fn windows_targets_rewrite_exports_on_any_host() {
    let directory = scratch("exports");
    let (project, module) = load(&directory, ANSWER);
    for triple in Triple::ALL
        .into_iter()
        .filter(|triple| !triple.is_bare_metal())
    {
        let output = directory.join(format!("{}.ll", triple.name()));
        driver::build(
            &module,
            &project,
            &output,
            BuildOptions {
                emit: Emit::Llvm,
                triple: Some(triple),
                cache: false,
                ..BuildOptions::default()
            },
        )
        .unwrap();
        let ir = fs::read_to_string(&output).unwrap();
        let windows = triple.os() == Os::Windows;
        assert_eq!(
            ir.contains("define dllexport i64 @tz_answer("),
            windows,
            "{}",
            triple.name()
        );
    }
    // Without a named target the rewrite is the host's, as it always was.
    let output = directory.join("host.ll");
    driver::build(
        &module,
        &project,
        &output,
        BuildOptions {
            emit: Emit::Llvm,
            cache: false,
            ..BuildOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&output)
            .unwrap()
            .contains("define dllexport i64 @tz_answer("),
        cfg!(windows)
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn msvc_targets_carry_codeview_on_any_host() {
    // The bundled launcher links MinGW and keeps DWARF only, so a run through it says nothing.
    if std::env::var("TSUZURI_CLANG").is_ok_and(|clang| clang.contains("tsuzuri-clang")) {
        return;
    }
    let directory = scratch("codeview");
    let (project, module) = load(&directory, ANSWER);
    for triple in Triple::ALL
        .into_iter()
        .filter(|triple| !triple.is_bare_metal())
    {
        let output = directory.join(format!("{}.ll", triple.name()));
        driver::build(
            &module,
            &project,
            &output,
            BuildOptions {
                emit: Emit::Llvm,
                debug_info: true,
                triple: Some(triple),
                cache: false,
                ..BuildOptions::default()
            },
        )
        .unwrap();
        let ir = fs::read_to_string(&output).unwrap();
        assert!(ir.contains("!llvm.dbg.cu"), "{}", triple.name());
        assert_eq!(
            ir.contains("!\"CodeView\""),
            triple.is_msvc(),
            "{}",
            triple.name()
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn bare_metal_x86_64_objects_have_no_red_zone() {
    let directory = scratch("redzone");
    let (project, module) = load(&directory, ANSWER);
    for triple in Triple::ALL
        .into_iter()
        .filter(|triple| triple.is_bare_metal())
    {
        let output = directory.join(format!("{}.ll", triple.name()));
        driver::build(
            &module,
            &project,
            &output,
            BuildOptions {
                emit: Emit::Llvm,
                freestanding: true,
                allocator: llvm::Allocator::Host,
                triple: Some(triple),
                cache: false,
                ..BuildOptions::default()
            },
        )
        .unwrap();
        let ir = fs::read_to_string(&output).unwrap();
        let defines: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("define "))
            .collect();
        assert!(!defines.is_empty());
        for line in defines {
            assert_eq!(
                line.contains(" noredzone"),
                triple.arch() == Arch::X86_64,
                "{}: {line}",
                triple.name()
            );
        }
    }
    // The marking keeps debug-info attachments after the attributes, where LLVM expects them.
    let output = directory.join("debug.ll");
    driver::build(
        &module,
        &project,
        &output,
        BuildOptions {
            emit: Emit::Llvm,
            freestanding: true,
            allocator: llvm::Allocator::Host,
            debug_info: true,
            triple: Some(Triple::X86_64BareMetal),
            cache: false,
            ..BuildOptions::default()
        },
    )
    .unwrap();
    let ir = fs::read_to_string(&output).unwrap();
    assert!(
        ir.lines()
            .any(|line| line.starts_with("define ") && line.contains(" noredzone !dbg !")),
        "{ir}"
    );
    fs::remove_dir_all(directory).unwrap();
}

/// Whether `clang --print-targets` lists a code generator named `backend`.
fn has_backend(clang: &str, backend: &str) -> bool {
    Command::new(clang)
        .arg("--print-targets")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.split_whitespace().next() == Some(backend))
        })
        .unwrap_or(false)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// The `LC_BUILD_VERSION` of a Mach-O object: its platform and minimum OS version.
fn mach_o_build_version(bytes: &[u8]) -> (u32, u32) {
    const LC_BUILD_VERSION: u32 = 0x32;
    let mut offset = 32;
    for _ in 0..u32_at(bytes, 16) {
        let (command, size) = (u32_at(bytes, offset), u32_at(bytes, offset + 4) as usize);
        if command == LC_BUILD_VERSION {
            return (u32_at(bytes, offset + 8), u32_at(bytes, offset + 12));
        }
        offset += size;
    }
    panic!("no LC_BUILD_VERSION");
}

#[test]
fn objects_have_the_machine_of_their_target() {
    let clang = std::env::var("TSUZURI_CLANG").unwrap_or_else(|_| "clang".to_owned());
    let directory = scratch("objects");
    let (project, module) = load(&directory, ANSWER);
    let mut checked = Vec::new();
    for triple in Triple::ALL {
        let backend = match triple.arch() {
            Arch::Aarch64 => "aarch64",
            Arch::X86_64 => "x86-64",
            Arch::Riscv64 => "riscv64",
        };
        // Apple's Clang has no RISC-V code generator; CI has LLVM's.
        if !has_backend(&clang, backend) {
            assert!(
                std::env::var_os("CI").is_none(),
                "{clang} cannot generate code for {backend}"
            );
            eprintln!(
                "skipping {}: {clang} has no {backend} backend",
                triple.name()
            );
            continue;
        }
        let output = directory.join(format!("{}.o", triple.name()));
        driver::build(
            &module,
            &project,
            &output,
            BuildOptions {
                emit: Emit::Object,
                freestanding: triple.is_bare_metal(),
                allocator: if triple.is_bare_metal() {
                    llvm::Allocator::Host
                } else {
                    llvm::Allocator::System
                },
                triple: Some(triple),
                cache: false,
                ..BuildOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("{}: {}", triple.name(), error.message));
        let bytes = fs::read(&output).unwrap();
        match triple.os() {
            Os::MacOs | Os::Ios => {
                // MH_MAGIC_64 as little-endian bytes, CPU_TYPE_ARM64 or CPU_TYPE_X86_64, MH_OBJECT.
                assert_eq!(bytes[..4], [0xcf, 0xfa, 0xed, 0xfe], "{}", triple.name());
                assert_eq!(
                    u32_at(&bytes, 4),
                    if triple.arch() == Arch::Aarch64 {
                        0x0100_000c
                    } else {
                        0x0100_0007
                    },
                    "{}",
                    triple.name()
                );
                assert_eq!(u32_at(&bytes, 12), 1);
                // PLATFORM_MACOS 1, PLATFORM_IOS 2, PLATFORM_IOSSIMULATOR 7; `xxxx.yy.zz` nibbles.
                let (platform, minimum) = mach_o_build_version(&bytes);
                let expected = match triple {
                    Triple::Aarch64AppleDarwin | Triple::X86_64AppleDarwin => (1, 12 << 16),
                    Triple::Aarch64AppleIos => (2, 13 << 16),
                    Triple::Aarch64AppleIosSim => (7, 14 << 16),
                    Triple::X86_64AppleIos => (7, 13 << 16),
                    _ => unreachable!(),
                };
                assert_eq!((platform, minimum), expected, "{}", triple.name());
            }
            Os::Windows => {
                // IMAGE_FILE_MACHINE_AMD64 and IMAGE_FILE_MACHINE_ARM64: a COFF object has no DOS header.
                assert_eq!(
                    u16_at(&bytes, 0),
                    if triple.arch() == Arch::Aarch64 {
                        0xaa64
                    } else {
                        0x8664
                    },
                    "{}",
                    triple.name()
                );
            }
            Os::Linux | Os::Android | Os::FreeBsd | Os::BareMetal => {
                // ELFCLASS64, ELFDATA2LSB, ET_REL, and EM_AARCH64 183, EM_X86_64 62, EM_RISCV 243.
                assert_eq!(bytes[..4], *b"\x7fELF", "{}", triple.name());
                assert_eq!((bytes[4], bytes[5]), (2, 1), "{}", triple.name());
                assert_eq!(u16_at(&bytes, 16), 1);
                assert_eq!(
                    u16_at(&bytes, 18),
                    match triple.arch() {
                        Arch::Aarch64 => 183,
                        Arch::X86_64 => 62,
                        Arch::Riscv64 => 243,
                    },
                    "{}",
                    triple.name()
                );
                // ELFOSABI_FREEBSD marks FreeBSD; the others use ELFOSABI_NONE.
                assert_eq!(
                    bytes[7],
                    if triple.os() == Os::FreeBsd { 9 } else { 0 },
                    "{}",
                    triple.name()
                );
                if triple.arch() == Arch::Riscv64 {
                    // EF_RISCV_RVC 0x1 and the float ABI field 0x6: EF_RISCV_FLOAT_ABI_DOUBLE 0x4. Clang
                    // gives a bare-metal target soft floats unless the compiler asks for the D extension.
                    let flags = u32_at(&bytes, 48);
                    assert_eq!(flags & 0x1, 1, "{}", triple.name());
                    assert_eq!(flags & 0x6, 0x4, "{} {flags:#x}", triple.name());
                }
            }
        }
        checked.push(triple.name());
    }
    assert!(!checked.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn the_bundled_launcher_maps_every_target_the_compiler_can_name() {
    // `resolve(target, host OS, host architecture)`.
    for triple in Triple::ALL {
        let clang = triple.clang_target();
        let linux_host = shim::resolve(Some(clang), "linux", "x86_64");
        let windows_host = shim::resolve(Some(clang), "windows", "x86_64");
        // The compiler's hints advise the bundled toolchain exactly for the targets it links.
        assert_eq!(
            windows_host.as_ref().unwrap().zig.is_some(),
            triple.has_bundled_c_library(),
            "{}",
            triple.name()
        );
        match triple.os() {
            // Zig has no C library for these, but their IR still compiles to objects.
            Os::Android | Os::Ios | Os::BareMetal => {
                for resolved in [&linux_host, &windows_host] {
                    let request = resolved.as_ref().unwrap();
                    assert_eq!(
                        (request.wasm, request.zig.as_deref(), request.llvm.as_str()),
                        (false, None, clang)
                    );
                }
            }
            _ => {
                let request = windows_host.as_ref().unwrap();
                let zig = request.zig.as_deref().unwrap();
                // Zig's target is "<arch>-<os>[-<abi>]": the same machine as the triple.
                let (arch, rest) = zig.split_once('-').unwrap();
                assert_eq!(arch, triple.arch_name(), "{}", triple.name());
                let system = match triple.os() {
                    Os::MacOs => "macos",
                    Os::Linux => "linux",
                    Os::FreeBsd => "freebsd",
                    Os::Windows => "windows",
                    _ => unreachable!(),
                };
                assert!(rest.starts_with(system), "{}: {zig}", triple.name());
                if triple.os() == Os::Linux {
                    assert!(zig.ends_with(if triple.name().ends_with("musl") {
                        "musl"
                    } else {
                        "gnu"
                    }));
                }
                if triple.is_msvc() {
                    // Only a Windows launcher links MinGW for an MSVC target, and it says so elsewhere.
                    assert_eq!(
                        request.llvm,
                        format!("{}-w64-windows-gnu", triple.arch_name())
                    );
                    let message = linux_host.unwrap_err();
                    assert!(
                        message.contains("MinGW") && message.contains("-pc-windows-gnu"),
                        "{message}"
                    );
                } else {
                    assert_eq!(request.llvm, clang, "{}", triple.name());
                    assert_eq!(linux_host.unwrap(), *request);
                }
            }
        }
    }
    // Without a target, the machine decides exactly as it always has.
    for (os, arch, zig, llvm) in [
        (
            "macos",
            "aarch64",
            "aarch64-macos.12.0",
            "aarch64-apple-macosx12.0.0",
        ),
        (
            "macos",
            "x86_64",
            "x86_64-macos.12.0",
            "x86_64-apple-macosx12.0.0",
        ),
        (
            "linux",
            "aarch64",
            "aarch64-linux-musl",
            "aarch64-unknown-linux-musl",
        ),
        (
            "linux",
            "x86_64",
            "x86_64-linux-musl",
            "x86_64-unknown-linux-musl",
        ),
        (
            "windows",
            "aarch64",
            "aarch64-windows-gnu",
            "aarch64-w64-windows-gnu",
        ),
        (
            "windows",
            "x86_64",
            "x86_64-windows-gnu",
            "x86_64-w64-windows-gnu",
        ),
    ] {
        let request = shim::resolve(None, os, arch).unwrap();
        assert_eq!(
            (request.wasm, request.zig.as_deref(), request.llvm.as_str()),
            (false, Some(zig), llvm)
        );
    }
    assert!(shim::resolve(None, "linux", "riscv64").is_err());
    assert!(shim::resolve(None, "freebsd", "x86_64").is_err());
    let wasm = shim::resolve(Some("wasm32-unknown-unknown"), "macos", "aarch64").unwrap();
    assert_eq!(
        (wasm.wasm, wasm.zig.as_deref(), wasm.llvm.as_str()),
        (true, Some("wasm32-freestanding"), "wasm32-unknown-unknown")
    );
}

#[cfg(unix)]
mod fake_toolchain {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Output;

    /// A project directory and the scripts that stand in for Clang and for the runner.
    pub struct Setup {
        pub root: PathBuf,
        pub log: PathBuf,
        pub clang: PathBuf,
        pub runner: PathBuf,
    }

    fn script(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub fn setup(name: &str, main: &str) -> Setup {
        let root = scratch(name);
        fs::write(root.join("Main.tz"), main).unwrap();
        let log = root.join("log.txt");
        // Records one line per run and writes a program in place of the executable: it prints
        // "ran" without arguments, and fails when it is asked for test 1.
        let clang = root.join("fake-clang");
        script(
            &clang,
            r#"#!/bin/sh
for argument in "$@"; do
  if [ "$argument" = "--version" ]; then echo "fake clang version 21.0.0"; exit 0; fi
done
echo "clang $*" >> "$FAKE_LOG"
output=""
previous=""
for argument in "$@"; do
  if [ "$previous" = "-o" ]; then output="$argument"; fi
  previous="$argument"
done
cat > "$output" <<'PROGRAM'
#!/bin/sh
case "$1" in
  "") echo ran;;
  1) echo "boom" >&2; exit 1;;
esac
PROGRAM
chmod +x "$output"
"#,
        );
        // Records its arguments, drops its own leading option, and runs the executable that follows.
        let runner = root.join("fake-runner");
        script(
            &runner,
            "#!/bin/sh\necho \"runner $*\" >> \"$FAKE_LOG\"\nshift\nexec \"$@\"\n",
        );
        Setup {
            root,
            log,
            clang,
            runner,
        }
    }

    impl Setup {
        pub fn command(&self, arguments: &[&str]) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
            command
                .args(arguments)
                .arg(&self.root)
                .env("TSUZURI_CLANG", &self.clang)
                .env("TSUZURI_CACHE_DIR", self.root.join("cache"))
                .env("FAKE_LOG", &self.log)
                .env_remove("TSUZURI_RUNNER")
                .env_remove("TSUZURI_SYSROOT");
            command
        }

        pub fn log(&self) -> String {
            fs::read_to_string(&self.log).unwrap_or_default()
        }

        pub fn runner_command(&self) -> String {
            format!("{} --marker", self.runner.display())
        }
    }

    pub fn text(output: &Output) -> (String, String) {
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

#[cfg(unix)]
fn foreign() -> Triple {
    Triple::ALL
        .into_iter()
        .find(|triple| Some(*triple) != Triple::host() && triple.os() == Os::Linux)
        .unwrap()
}

#[cfg(unix)]
#[test]
fn a_foreign_target_runs_through_the_runner_and_only_then() {
    use fake_toolchain::{setup, text};
    let setup = setup("run", "let total = 40 + 2\ntotal\n");
    let triple = foreign();
    // Without a runner the program is not even built.
    let output = setup
        .command(&["run", "--no-cache", "--target", triple.name()])
        .output()
        .unwrap();
    let (stdout, stderr) = text(&output);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(
        stderr.contains(&format!(
            "error[E2000]: cannot run {0} programs on this machine; run the program on a {0} machine, set TSUZURI_RUNNER to a command that runs {0} executables (such as qemu-user, Wine, or an emulator wrapper), or use 'tsuzuri build --target {0}'",
            triple.name()
        )),
        "{stderr}"
    );
    assert_eq!(setup.log(), "");
    // A blank runner is no runner.
    let blank = setup
        .command(&["run", "--no-cache", "--target", triple.name()])
        .env("TSUZURI_RUNNER", "  ")
        .output()
        .unwrap();
    assert_eq!(blank.status.code(), Some(1));
    assert_eq!(setup.log(), "");
    // A runner that does not exist is named in the error, not the program it was to start.
    let missing = setup
        .command(&["run", "--no-cache", "--target", triple.name()])
        .env("TSUZURI_RUNNER", "/no/such/runner --flag")
        .output()
        .unwrap();
    let (_, stderr) = text(&missing);
    assert_eq!(missing.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("cannot start the runner of TSUZURI_RUNNER '/no/such/runner': "),
        "{stderr}"
    );
    // The program was built, and nothing was started.
    assert!(setup.log().starts_with("clang "), "{}", setup.log());
    assert!(!setup.log().contains("runner "), "{}", setup.log());
    fs::remove_file(&setup.log).unwrap();
    // With one, the program is built for the target, cross-linked, and started through the runner.
    let output = setup
        .command(&["run", "--no-cache", "--target", triple.name()])
        .env("TSUZURI_RUNNER", setup.runner_command())
        .env("TSUZURI_SYSROOT", "/fake/sysroot")
        .output()
        .unwrap();
    let (stdout, stderr) = text(&output);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(stdout, "ran\n");
    let log = setup.log();
    let compile = log.lines().next().unwrap();
    assert!(
        compile.contains(&format!("--target={}", triple.clang_target()))
            && compile.contains("--sysroot=/fake/sysroot")
            && compile.contains("-fuse-ld=lld")
            && compile.contains("-fPIC"),
        "{compile}"
    );
    let runner = log
        .lines()
        .find(|line| line.starts_with("runner "))
        .unwrap();
    assert!(runner.starts_with("runner --marker "), "{runner}");
    // The runner receives the executable and then the program's arguments, of which `run` has none.
    assert!(runner.ends_with("/program"), "{runner}");
    // The machine's own target needs no runner, and a runner is not used for it.
    if let Some(host) = Triple::host() {
        let before = setup.log();
        let output = setup
            .command(&["run", "--no-cache", "--target", host.name()])
            .env("TSUZURI_RUNNER", setup.runner_command())
            .output()
            .unwrap();
        let (stdout, stderr) = text(&output);
        assert!(output.status.success(), "{stderr}");
        assert_eq!(stdout, "ran\n");
        let added = &setup.log()[before.len()..];
        assert!(!added.contains("runner "), "{added}");
        assert!(added.contains(&format!("--target={}", host.clang_target())));
        assert!(!added.contains("-fuse-ld=lld"));
    }
    fs::remove_dir_all(setup.root).unwrap();
}

#[cfg(unix)]
#[test]
fn a_foreign_target_tests_through_the_runner_and_reports_each_test() {
    use fake_toolchain::{setup, text};
    let setup = setup(
        "test",
        "test \"first\" = assert true\ntest \"second\" = assert true\n",
    );
    let triple = foreign();
    let refused = setup
        .command(&["test", "--target", triple.name()])
        .output()
        .unwrap();
    let (_, stderr) = text(&refused);
    assert_eq!(refused.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "cannot run {0} tests on this machine; run 'tsuzuri test' on a {0} machine, set TSUZURI_RUNNER to a command that runs {0} executables",
            triple.name()
        )),
        "{stderr}"
    );
    assert_eq!(setup.log(), "");
    // A runner that does not exist is named in the error, and no test is reported as failed.
    let missing = setup
        .command(&["test", "--target", triple.name()])
        .env("TSUZURI_RUNNER", "/no/such/runner")
        .output()
        .unwrap();
    let (_, stderr) = text(&missing);
    assert_eq!(missing.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("cannot start the runner of TSUZURI_RUNNER '/no/such/runner': "),
        "{stderr}"
    );
    assert!(!stderr.contains("not ok"), "{stderr}");
    fs::remove_file(&setup.log).unwrap();
    let output = setup
        .command(&["test", "--target", triple.name(), "--json"])
        .env("TSUZURI_RUNNER", setup.runner_command())
        .output()
        .unwrap();
    let (stdout, stderr) = text(&output);
    // The fake program fails test 1 and passes test 0, so the run fails and names both.
    assert_eq!(output.status.code(), Some(1), "{stdout}\n{stderr}");
    assert!(stdout.contains("\"passed\":1,\"failed\":1"), "{stdout}");
    let log = setup.log();
    let runs: Vec<_> = log
        .lines()
        .filter(|line| line.starts_with("runner "))
        .collect();
    assert_eq!(runs.len(), 2, "{log}");
    let mut indices = Vec::new();
    for run in &runs {
        assert!(run.starts_with("runner --marker "), "{run}");
        assert!(run.contains("/tests "), "{run}");
        indices.push(run.rsplit(' ').next().unwrap());
    }
    indices.sort_unstable();
    assert_eq!(indices, ["0", "1"]);
    // Coverage reads files that the runner's program writes, and a debug runner is a native program.
    for arguments in [
        vec![
            "test",
            "--target",
            triple.name(),
            "--coverage",
            "coverage.lcov",
        ],
        vec![
            "test",
            "--target",
            triple.name(),
            "--index",
            "0",
            "-g",
            "-o",
            "debug-runner",
        ],
    ] {
        let output = setup
            .command(&arguments)
            .env("TSUZURI_RUNNER", setup.runner_command())
            .output()
            .unwrap();
        let (_, stderr) = text(&output);
        assert_eq!(output.status.code(), Some(1), "{arguments:?}\n{stderr}");
        assert!(
            stderr.contains("needs a target this machine runs"),
            "{stderr}"
        );
    }
    // A benchmark measures the machine it runs on.
    let bench = setup
        .command(&["bench", "--target", triple.name()])
        .env("TSUZURI_RUNNER", setup.runner_command())
        .output()
        .unwrap();
    assert!(text(&bench).1.contains("cannot run"), "{}", text(&bench).1);
    fs::remove_dir_all(setup.root).unwrap();
}

#[cfg(unix)]
#[test]
fn runtimes_a_target_lacks_are_refused_before_any_tool_runs() {
    use fake_toolchain::{setup, text};
    const OS: &str = "def main :: unit -> i32 = \\() ->\n    let! home = Env.var \"HOME\"\n    0\n";
    const NET: &str = "def main :: unit -> i32 = \\() ->\n    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))\n    let! outcome = Async.block_on (Net.connect_async address (Maybe.Some 100))\n    do! IO.write_line (Result.is_ok (ref outcome))\n    0\n";
    for (source, name) in [(OS, "os"), (NET, "net")] {
        let setup = setup(name, source);
        for triple in Triple::ALL
            .into_iter()
            .filter(|triple| !triple.is_bare_metal())
        {
            // A Clang that does not exist: a build that passes the checks fails to start it.
            let output = setup
                .command(&["build", "--no-cache", "--json", "--target", triple.name()])
                .env("TSUZURI_CLANG", setup.root.join("missing-clang"))
                .output()
                .unwrap();
            let (_, stderr) = text(&output);
            let platform = Platform::new(Some(triple));
            let supported = if name == "os" {
                platform.os_apis()
            } else {
                platform.net()
            };
            assert_eq!(
                output.status.code(),
                Some(1),
                "{name} {}: {stderr}",
                triple.name()
            );
            if supported {
                assert!(
                    stderr.contains("cannot execute") && stderr.contains("missing-clang"),
                    "{name} {}: {stderr}",
                    triple.name()
                );
            } else {
                assert!(
                    stderr.contains("\"code\":\"E2002\""),
                    "{name} {}: {stderr}",
                    triple.name()
                );
                assert!(
                    !stderr.contains("cannot execute"),
                    "{name} {}: {stderr}",
                    triple.name()
                );
            }
        }
        // The gates read the target's table, not the machine the compiler runs on.
        let message = |triple: Triple| {
            let output = setup
                .command(&["build", "--no-cache", "--target", triple.name()])
                .env("TSUZURI_CLANG", setup.root.join("missing-clang"))
                .output()
                .unwrap();
            text(&output).1
        };
        if name == "os" {
            for triple in [Triple::Aarch64AppleIos, Triple::X86_64LinuxAndroid] {
                assert!(message(triple).contains(&format!(
                    "the File, Dir, Env, Time, Random, and Process operating-system APIs are not available for {}: src/runtime/os.c is written for the headers of macOS, Linux, and FreeBSD",
                    triple.name()
                )));
            }
            assert!(message(Triple::X86_64WindowsGnu).contains(
                "the File, Dir, Env, Time, Random, and Process operating-system APIs are not supported on Windows yet (G10)"
            ));
        } else {
            assert!(message(Triple::X86_64FreeBsd).contains(
                "the Net socket API is only available for macOS, Linux, and Windows targets; x86_64-unknown-freebsd has no implementation of it"
            ));
        }
        fs::remove_dir_all(setup.root).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn a_sysroot_the_target_cannot_read_is_refused_before_any_tool_runs() {
    use fake_toolchain::{setup, text};
    let setup = setup("sysroot", "def main :: unit -> i32 = \\() -> 0\n");
    // An MSVC target and a bare-metal target read no sysroot, whatever the output is.
    for (triple, extra) in [
        (Triple::X86_64WindowsMsvc, vec!["--emit", "object"]),
        (
            Triple::Riscv64BareMetal,
            vec!["--emit", "object", "--freestanding", "--allocator", "host"],
        ),
    ] {
        let mut arguments = vec!["build", "--no-cache", "--json", "--target", triple.name()];
        arguments.extend(extra);
        let output = setup
            .command(&arguments)
            .env("TSUZURI_CLANG", setup.root.join("missing-clang"))
            .env("TSUZURI_SYSROOT", "/some/sysroot")
            .output()
            .unwrap();
        let (_, stderr) = text(&output);
        assert_eq!(output.status.code(), Some(2), "{}: {stderr}", triple.name());
        assert!(
            stderr.contains("\"code\":\"E2000\"")
                && stderr.contains(&format!(
                    "TSUZURI_SYSROOT does not apply to {}",
                    triple.name()
                )),
            "{}: {stderr}",
            triple.name()
        );
        assert!(!stderr.contains("cannot execute"), "{stderr}");
    }
    fs::remove_dir_all(setup.root).unwrap();
}

#[cfg(unix)]
#[test]
fn a_different_triple_or_sysroot_never_reuses_a_cached_build() {
    use fake_toolchain::{setup, text};
    // A float result keeps the numeric runtime in the build: an integer's text has no object of its own.
    let setup = setup("cache", "let total = 40.0 + 2.0\ntotal\n");
    let mut targets = Triple::ALL
        .into_iter()
        .filter(|triple| triple.os() == Os::Linux || triple.os() == Os::FreeBsd);
    let (first, second) = (targets.next().unwrap(), targets.next().unwrap());
    let build = |triple: Triple, sysroot: &str| {
        let output = setup
            .command(&["build", "--target", triple.name(), "-o", "program"])
            .current_dir(&setup.root)
            .env("TSUZURI_SYSROOT", sysroot)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output).1);
        setup
            .log()
            .lines()
            .filter(|line| line.starts_with("clang "))
            .count()
    };
    // Each cold configuration compiles its numeric object, then the user module.
    assert_eq!(build(first, "/a"), 2);
    // The same build again is a cache hit: the compiler runs no Clang.
    assert_eq!(build(first, "/a"), 2);
    // Another sysroot, another target, or both make another artifact.
    assert_eq!(build(first, "/b"), 4);
    assert_eq!(build(second, "/b"), 6);
    assert_eq!(build(second, "/b"), 6);
    assert_eq!(build(first, "/a"), 6);
    fs::remove_dir_all(setup.root).unwrap();
}
