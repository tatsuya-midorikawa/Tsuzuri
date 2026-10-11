//! The native targets that `--target` can name, and what a build needs to know about them.
//!
//! A [`Triple`] is one of a closed list of 64-bit little-endian targets, spelled as Rust spells
//! them. A [`Platform`] answers the questions the driver asks where it used to ask `cfg!(windows)`
//! and friends: without a triple it describes the machine the compiler runs on and answers with
//! the same `cfg!` as before, with a triple it answers from the table below. 32-bit targets are not
//! in the list: native code generation assumes 64-bit pointers.

use std::cell::Cell;
use std::env;
use std::ffi::{OsStr, OsString};

use crate::driver::Target;

/// The operating system of a native target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Ios,
    Linux,
    Android,
    FreeBsd,
    Windows,
    /// No operating system: freestanding objects only.
    BareMetal,
}

/// The instruction set of a native target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Aarch64,
    X86_64,
    Riscv64,
}

/// A target that `--target` names for native output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Triple {
    Aarch64AppleDarwin,
    X86_64AppleDarwin,
    Aarch64LinuxGnu,
    X86_64LinuxGnu,
    Aarch64WindowsMsvc,
    X86_64WindowsMsvc,
    Aarch64LinuxMusl,
    X86_64LinuxMusl,
    Riscv64LinuxGnu,
    Riscv64LinuxMusl,
    Aarch64WindowsGnu,
    X86_64WindowsGnu,
    Aarch64FreeBsd,
    X86_64FreeBsd,
    Aarch64LinuxAndroid,
    X86_64LinuxAndroid,
    Aarch64AppleIos,
    Aarch64AppleIosSim,
    X86_64AppleIos,
    Aarch64BareMetal,
    X86_64BareMetal,
    Riscv64BareMetal,
}

/// One row of the target table.
#[derive(Clone, Copy)]
struct Spec {
    /// The `--target` spelling.
    name: &'static str,
    /// The Clang `--target` value. Apple and Android triples carry the oldest OS version the
    /// runtime is built for, so that the result does not depend on the SDK that happens to be found.
    clang: &'static str,
    /// Further Clang arguments the target needs for code generation.
    arguments: &'static [&'static str],
    os: Os,
    arch: Arch,
    /// A Windows target that links against the MSVC runtime (the others use MinGW).
    msvc: bool,
}

const fn spec(
    name: &'static str,
    clang: &'static str,
    os: Os,
    arch: Arch,
    arguments: &'static [&'static str],
) -> Spec {
    Spec {
        name,
        clang,
        arguments,
        os,
        arch,
        msvc: false,
    }
}

const fn msvc(name: &'static str, clang: &'static str, arch: Arch) -> Spec {
    Spec {
        name,
        clang,
        arguments: &[],
        os: Os::Windows,
        arch,
        msvc: true,
    }
}

impl Triple {
    /// Every target, in the order of the diagnostics.
    pub const ALL: [Triple; 22] = [
        Self::Aarch64AppleDarwin,
        Self::X86_64AppleDarwin,
        Self::Aarch64LinuxGnu,
        Self::X86_64LinuxGnu,
        Self::Aarch64WindowsMsvc,
        Self::X86_64WindowsMsvc,
        Self::Aarch64LinuxMusl,
        Self::X86_64LinuxMusl,
        Self::Riscv64LinuxGnu,
        Self::Riscv64LinuxMusl,
        Self::Aarch64WindowsGnu,
        Self::X86_64WindowsGnu,
        Self::Aarch64FreeBsd,
        Self::X86_64FreeBsd,
        Self::Aarch64LinuxAndroid,
        Self::X86_64LinuxAndroid,
        Self::Aarch64AppleIos,
        Self::Aarch64AppleIosSim,
        Self::X86_64AppleIos,
        Self::Aarch64BareMetal,
        Self::X86_64BareMetal,
        Self::Riscv64BareMetal,
    ];

    const fn spec(self) -> Spec {
        use Arch::{Aarch64, Riscv64, X86_64};
        match self {
            Self::Aarch64AppleDarwin => spec(
                "aarch64-apple-darwin",
                "arm64-apple-macosx12.0.0",
                Os::MacOs,
                Aarch64,
                &[],
            ),
            Self::X86_64AppleDarwin => spec(
                "x86_64-apple-darwin",
                "x86_64-apple-macosx12.0.0",
                Os::MacOs,
                X86_64,
                &[],
            ),
            Self::Aarch64LinuxGnu => spec(
                "aarch64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                Os::Linux,
                Aarch64,
                &[],
            ),
            Self::X86_64LinuxGnu => spec(
                "x86_64-unknown-linux-gnu",
                "x86_64-unknown-linux-gnu",
                Os::Linux,
                X86_64,
                &[],
            ),
            Self::Aarch64WindowsMsvc => msvc(
                "aarch64-pc-windows-msvc",
                "aarch64-pc-windows-msvc",
                Aarch64,
            ),
            Self::X86_64WindowsMsvc => {
                msvc("x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc", X86_64)
            }
            Self::Aarch64LinuxMusl => spec(
                "aarch64-unknown-linux-musl",
                "aarch64-unknown-linux-musl",
                Os::Linux,
                Aarch64,
                &[],
            ),
            Self::X86_64LinuxMusl => spec(
                "x86_64-unknown-linux-musl",
                "x86_64-unknown-linux-musl",
                Os::Linux,
                X86_64,
                &[],
            ),
            Self::Riscv64LinuxGnu => spec(
                "riscv64gc-unknown-linux-gnu",
                "riscv64-unknown-linux-gnu",
                Os::Linux,
                Riscv64,
                &[],
            ),
            Self::Riscv64LinuxMusl => spec(
                "riscv64gc-unknown-linux-musl",
                "riscv64-unknown-linux-musl",
                Os::Linux,
                Riscv64,
                &[],
            ),
            Self::Aarch64WindowsGnu => spec(
                "aarch64-pc-windows-gnu",
                "aarch64-w64-windows-gnu",
                Os::Windows,
                Aarch64,
                &[],
            ),
            Self::X86_64WindowsGnu => spec(
                "x86_64-pc-windows-gnu",
                "x86_64-w64-windows-gnu",
                Os::Windows,
                X86_64,
                &[],
            ),
            Self::Aarch64FreeBsd => spec(
                "aarch64-unknown-freebsd",
                "aarch64-unknown-freebsd14",
                Os::FreeBsd,
                Aarch64,
                &[],
            ),
            Self::X86_64FreeBsd => spec(
                "x86_64-unknown-freebsd",
                "x86_64-unknown-freebsd14",
                Os::FreeBsd,
                X86_64,
                &[],
            ),
            Self::Aarch64LinuxAndroid => spec(
                "aarch64-linux-android",
                "aarch64-linux-android21",
                Os::Android,
                Aarch64,
                &[],
            ),
            Self::X86_64LinuxAndroid => spec(
                "x86_64-linux-android",
                "x86_64-linux-android21",
                Os::Android,
                X86_64,
                &[],
            ),
            Self::Aarch64AppleIos => spec(
                "aarch64-apple-ios",
                "arm64-apple-ios13.0",
                Os::Ios,
                Aarch64,
                &[],
            ),
            // The first iOS release that runs on an Apple silicon simulator.
            Self::Aarch64AppleIosSim => spec(
                "aarch64-apple-ios-sim",
                "arm64-apple-ios14.0-simulator",
                Os::Ios,
                Aarch64,
                &[],
            ),
            Self::X86_64AppleIos => spec(
                "x86_64-apple-ios",
                "x86_64-apple-ios13.0-simulator",
                Os::Ios,
                X86_64,
                &[],
            ),
            Self::Aarch64BareMetal => spec(
                "aarch64-unknown-none",
                "aarch64-unknown-none-elf",
                Os::BareMetal,
                Aarch64,
                &[],
            ),
            // The red zone below the stack pointer, which an interrupt handler would overwrite, is
            // turned off by marking every function in the IR: Clang ignores `-mno-red-zone` for IR.
            Self::X86_64BareMetal => spec(
                "x86_64-unknown-none",
                "x86_64-unknown-none-elf",
                Os::BareMetal,
                X86_64,
                &[],
            ),
            // Clang's default for this OS-less triple is soft-float without the F and D extensions.
            Self::Riscv64BareMetal => spec(
                "riscv64gc-unknown-none-elf",
                "riscv64-unknown-none-elf",
                Os::BareMetal,
                Riscv64,
                &["-march=rv64gc", "-mabi=lp64d"],
            ),
        }
    }

    /// The `--target` spelling. It is Rust's name for the target; Clang's differs for some.
    pub const fn name(self) -> &'static str {
        self.spec().name
    }

    /// The target by its `--target` spelling. There are no aliases and the case matters.
    pub fn parse(text: &str) -> Option<Triple> {
        Self::ALL.into_iter().find(|triple| triple.name() == text)
    }

    /// Every spelling of `--target`, separated by commas, for diagnostics.
    pub fn names() -> String {
        Self::ALL
            .iter()
            .map(|triple| triple.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The Clang `--target` value.
    pub const fn clang_target(self) -> &'static str {
        self.spec().clang
    }

    /// The Clang arguments that go with [`Triple::clang_target`] in every Clang run.
    pub const fn clang_arguments(self) -> &'static [&'static str] {
        self.spec().arguments
    }

    pub const fn os(self) -> Os {
        self.spec().os
    }

    pub const fn arch(self) -> Arch {
        self.spec().arch
    }

    /// The architecture as `std::env::consts::ARCH` spells it.
    pub const fn arch_name(self) -> &'static str {
        match self.spec().arch {
            Arch::Aarch64 => "aarch64",
            Arch::X86_64 => "x86_64",
            Arch::Riscv64 => "riscv64",
        }
    }

    /// A Windows target that uses the MSVC runtime. The other Windows targets use MinGW.
    pub const fn is_msvc(self) -> bool {
        self.spec().msvc
    }

    /// A target without an operating system, which only freestanding output serves.
    pub const fn is_bare_metal(self) -> bool {
        matches!(self.spec().os, Os::BareMetal)
    }

    /// The target of the machine the compiler runs on, if it is one of these.
    pub fn host() -> Option<Triple> {
        let arm = cfg!(target_arch = "aarch64");
        let x64 = cfg!(target_arch = "x86_64");
        let riscv = cfg!(target_arch = "riscv64");
        let pick = |on_arm: Triple, on_x64: Triple| arm.then_some(on_arm).or(x64.then_some(on_x64));
        if cfg!(target_os = "macos") {
            pick(Self::Aarch64AppleDarwin, Self::X86_64AppleDarwin)
        } else if cfg!(all(target_os = "linux", target_env = "gnu")) {
            pick(Self::Aarch64LinuxGnu, Self::X86_64LinuxGnu)
                .or(riscv.then_some(Self::Riscv64LinuxGnu))
        } else if cfg!(all(target_os = "linux", target_env = "musl")) {
            pick(Self::Aarch64LinuxMusl, Self::X86_64LinuxMusl)
                .or(riscv.then_some(Self::Riscv64LinuxMusl))
        } else if cfg!(all(windows, target_env = "msvc")) {
            pick(Self::Aarch64WindowsMsvc, Self::X86_64WindowsMsvc)
        } else if cfg!(all(windows, target_env = "gnu")) {
            pick(Self::Aarch64WindowsGnu, Self::X86_64WindowsGnu)
        } else if cfg!(target_os = "freebsd") {
            pick(Self::Aarch64FreeBsd, Self::X86_64FreeBsd)
        } else if cfg!(target_os = "android") {
            pick(Self::Aarch64LinuxAndroid, Self::X86_64LinuxAndroid)
        } else {
            None
        }
    }
}

/// The message for a `--target` value that names no target.
pub fn unknown_target_message(value: &str) -> String {
    let arch = value.split('-').next().unwrap_or_default();
    let narrow = matches!(
        arch,
        "arm" | "i386" | "i486" | "i586" | "i686" | "mips" | "mipsel" | "powerpc" | "sparc"
    ) || arch.starts_with("armv")
        || arch.starts_with("thumb")
        || arch.starts_with("riscv32");
    format!(
        "unknown target '{value}'; {}use native, wasm32, wasm64, or one of {}",
        if narrow {
            "32-bit native targets are not supported because native code generation assumes 64-bit pointers; "
        } else {
            ""
        },
        Triple::names()
    )
}

/// What `--target` selected: the code generator, and for native output the named target.
pub fn parse_target(value: &str) -> Result<(Target, Option<Triple>), String> {
    match value {
        "native" => Ok((Target::Native, None)),
        "wasm32" | "wasm32-unknown-unknown" => Ok((Target::Wasm32, None)),
        "wasm64" | "wasm64-unknown-unknown" => Ok((Target::Wasm64, None)),
        _ => Triple::parse(value)
            .map(|triple| (Target::Native, Some(triple)))
            .ok_or_else(|| unknown_target_message(value)),
    }
}

/// The questions a native build asks about where its output runs.
///
/// Without a triple the answers describe the machine the compiler runs on, exactly as the
/// `cfg!` checks they replace; with a triple they come from the target table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform(Option<Triple>);

impl Platform {
    /// The machine the compiler runs on: `--target native`.
    pub const HOST: Platform = Platform(None);

    pub const fn new(triple: Option<Triple>) -> Self {
        Self(triple)
    }

    pub const fn triple(self) -> Option<Triple> {
        self.0
    }

    /// Whether this is a named target other than the one of the machine the compiler runs on.
    pub fn cross(self) -> bool {
        self.0.is_some_and(|triple| Some(triple) != Triple::host())
    }

    /// COFF output: `.exe`, `.obj` and `.dll`, no C math library, the Win32 task runtime.
    pub const fn windows(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.os(), Os::Windows),
            None => cfg!(windows),
        }
    }

    /// Windows output that uses the MSVC runtime.
    pub const fn msvc(self) -> bool {
        match self.0 {
            Some(triple) => triple.is_msvc(),
            None => cfg!(windows),
        }
    }

    /// Mach-O output, whose debug information a dSYM file carries.
    pub const fn mach_o(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.os(), Os::MacOs | Os::Ios),
            None => cfg!(target_os = "macos"),
        }
    }

    /// Linux with glibc or musl: the C library whose `dlopen` older glibc keeps in `libdl`.
    pub const fn linux(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.os(), Os::Linux),
            None => cfg!(target_os = "linux"),
        }
    }

    /// A Linux kernel, with or without its usual C library.
    pub const fn linux_kernel(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.os(), Os::Linux | Os::Android),
            None => cfg!(target_os = "linux"),
        }
    }

    /// POSIX: the signal handlers and threads of the runtime.
    pub const fn unix(self) -> bool {
        match self.0 {
            Some(triple) => !matches!(triple.os(), Os::Windows | Os::BareMetal),
            None => cfg!(unix),
        }
    }

    /// The architecture as `std::env::consts::ARCH` spells it.
    pub const fn arch_name(self) -> &'static str {
        match self.0 {
            Some(triple) => triple.arch_name(),
            None => env::consts::ARCH,
        }
    }

    pub const fn aarch64(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.arch(), Arch::Aarch64),
            None => cfg!(target_arch = "aarch64"),
        }
    }

    /// The CPU levels of `syntax::CPU_TARGETS` (bit `L` for level `L`) that `src/runtime/cpu.c`
    /// detects: x86-64 outside Windows, and AArch64 on a Linux kernel. Elsewhere a `@cpu`
    /// function keeps only its portable version.
    pub const fn cpu_levels(self) -> u8 {
        match self.0 {
            Some(triple) => match (triple.arch(), triple.os()) {
                (Arch::X86_64, Os::Windows) => 0,
                (Arch::X86_64, _) => 0b1110,
                (Arch::Aarch64, Os::Linux | Os::Android) => 0b11_0000,
                _ => 0,
            },
            None => {
                if cfg!(all(target_arch = "x86_64", not(windows))) {
                    0b1110
                } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
                    0b11_0000
                } else {
                    0
                }
            }
        }
    }

    /// `Math.fma` is the fused instruction, which every AArch64 CPU has. Everywhere else it is the
    /// exactly rounded software routine, because `llvm.fma` may become a call to a C library
    /// function of unknown accuracy.
    pub const fn hardware_fma(self) -> bool {
        self.aarch64()
    }

    /// Whether every function is compiled without a red zone: x86-64 without an operating system,
    /// where interrupts may run on the stack of the code they interrupt.
    pub const fn no_red_zone(self) -> bool {
        match self.0 {
            Some(triple) => matches!((triple.os(), triple.arch()), (Os::BareMetal, Arch::X86_64)),
            None => false,
        }
    }

    /// Whether `src/runtime/os.c` has an implementation for the target: POSIX with the headers of
    /// macOS and Linux. Windows waits for G10, and the iOS SDK has no `sys/random.h`.
    pub const fn os_apis(self) -> bool {
        match self.0 {
            Some(triple) => !matches!(triple.os(), Os::Windows | Os::Ios | Os::BareMetal),
            None => !cfg!(windows),
        }
    }

    /// Whether `src/runtime/net.c` has an implementation for the target.
    pub const fn net(self) -> bool {
        match self.0 {
            Some(triple) => matches!(triple.os(), Os::MacOs | Os::Linux | Os::Windows),
            None => cfg!(any(target_os = "macos", target_os = "linux", windows)),
        }
    }

    /// Whether `src/runtime/async.c` has an implementation for the target.
    pub const fn reactor(self) -> bool {
        self.net()
    }

    /// Whether the target has threads for `Task.parallel`.
    pub const fn threads(self) -> bool {
        match self.0 {
            Some(triple) => !triple.is_bare_metal(),
            None => cfg!(any(unix, windows)),
        }
    }

    /// The extension of an executable, which is empty off Windows.
    pub const fn executable_extension(self) -> &'static str {
        if self.windows() { "exe" } else { "" }
    }

    pub const fn object_extension(self) -> &'static str {
        if self.windows() { "obj" } else { "o" }
    }

    pub const fn shared_extension(self) -> &'static str {
        if self.windows() {
            "dll"
        } else if self.mach_o() {
            "dylib"
        } else {
            "so"
        }
    }
}

thread_local! {
    static CURRENT: Cell<Platform> = const { Cell::new(Platform::HOST) };
}

/// The platform whose native IR the current thread is emitting: [`Platform::HOST`] unless a
/// [`PlatformScope`] is alive.
pub fn current_platform() -> Platform {
    CURRENT.with(Cell::get)
}

/// Makes native IR emission on this thread follow `platform` until it is dropped, for the parts
/// of the IR that depend on the target: the allocator of COFF targets, the CPU levels of `@cpu`
/// functions, and the fused multiply-add of `Math.fma`.
#[must_use = "the platform applies only while the scope is alive"]
pub struct PlatformScope {
    previous: Platform,
}

impl PlatformScope {
    pub fn enter(platform: Platform) -> Self {
        Self {
            previous: CURRENT.with(|current| current.replace(platform)),
        }
    }
}

impl Drop for PlatformScope {
    fn drop(&mut self) {
        CURRENT.with(|current| current.set(self.previous));
    }
}

/// The command that runs the executables of a target this machine cannot run: `qemu-user`,
/// Wine, `arch -x86_64`, or a wrapper that starts an emulator or a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignRunner {
    pub program: OsString,
    /// The arguments before the executable's path.
    pub arguments: Vec<OsString>,
}

/// The variable that names the [`ForeignRunner`].
pub const RUNNER_VARIABLE: &str = "TSUZURI_RUNNER";

impl ForeignRunner {
    /// Splits `text` at white space into the program and its leading arguments, without quoting or
    /// expansion. A blank text is no runner. A value that is not UTF-8 is the program alone.
    pub fn parse(text: &OsStr) -> Option<ForeignRunner> {
        let Some(words) = text.to_str() else {
            return Some(ForeignRunner {
                program: text.to_owned(),
                arguments: Vec::new(),
            });
        };
        let mut words = words.split_whitespace();
        let program = words.next()?;
        Some(ForeignRunner {
            program: program.into(),
            arguments: words.map(OsString::from).collect(),
        })
    }

    /// The runner that `TSUZURI_RUNNER` names, if it is set to something.
    pub fn from_environment() -> Option<ForeignRunner> {
        Self::parse(&env::var_os(RUNNER_VARIABLE)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for triple in Triple::ALL {
            assert_eq!(Triple::parse(triple.name()), Some(triple));
            assert!(seen.insert(triple.name()), "{}", triple.name());
        }
        assert_eq!(seen.len(), Triple::ALL.len());
        for alias in [
            "",
            "native",
            "wasm32",
            "arm64-apple-darwin",
            "x86_64-linux-gnu",
            "AARCH64-apple-darwin",
            "x86_64-unknown-linux-gnu ",
            "i686-pc-windows-msvc",
        ] {
            assert_eq!(Triple::parse(alias), None, "{alias:?}");
        }
    }

    #[test]
    fn every_target_has_64_bit_clang_arguments() {
        for triple in Triple::ALL {
            let clang = triple.clang_target();
            let spellings: &[&str] = match triple.arch() {
                Arch::Aarch64 => &["aarch64-", "arm64-"],
                Arch::X86_64 => &["x86_64-"],
                Arch::Riscv64 => &["riscv64-"],
            };
            assert!(
                spellings.iter().any(|prefix| clang.starts_with(prefix)),
                "{clang}"
            );
            assert_eq!(triple.is_msvc(), clang.ends_with("-windows-msvc"));
            assert_eq!(triple.is_bare_metal(), clang.ends_with("-none-elf"));
            for argument in triple.clang_arguments() {
                assert!(argument.starts_with("-m"), "{argument}");
            }
        }
        assert_eq!(
            Triple::Riscv64BareMetal.clang_arguments(),
            ["-march=rv64gc", "-mabi=lp64d"]
        );
    }

    #[test]
    fn parses_native_wasm_and_unknown_targets() {
        assert_eq!(parse_target("native"), Ok((Target::Native, None)));
        for (text, target) in [
            ("wasm32", Target::Wasm32),
            ("wasm32-unknown-unknown", Target::Wasm32),
            ("wasm64", Target::Wasm64),
            ("wasm64-unknown-unknown", Target::Wasm64),
        ] {
            assert_eq!(parse_target(text), Ok((target, None)));
        }
        assert_eq!(
            parse_target("x86_64-unknown-linux-gnu"),
            Ok((Target::Native, Some(Triple::X86_64LinuxGnu)))
        );
        let list = Triple::names();
        assert_eq!(
            parse_target("x86_64-unknown-haiku").unwrap_err(),
            format!(
                "unknown target 'x86_64-unknown-haiku'; use native, wasm32, wasm64, or one of {list}"
            )
        );
        for narrow in [
            "i686-pc-windows-msvc",
            "armv7-unknown-linux-gnueabihf",
            "arm",
            "riscv32imac-unknown-none-elf",
        ] {
            let message = parse_target(narrow).unwrap_err();
            assert!(message.contains("64-bit pointers"), "{message}");
            assert!(message.ends_with(&list));
        }
        for wide in [
            "arm64-apple-darwin",
            "aarch64-unknown-haiku",
            "x86_64-linux-gnu",
        ] {
            let message = parse_target(wide).unwrap_err();
            assert!(!message.contains("64-bit pointers"), "{message}");
        }
    }

    #[test]
    fn the_host_triple_agrees_with_the_host_checks() {
        let Some(host) = Triple::host() else {
            return;
        };
        let named = Platform::new(Some(host));
        assert!(!named.cross());
        assert_eq!(named.arch_name(), Platform::HOST.arch_name());
        // An Android host is `target_os = "android"`, which the host checks do not name.
        if host.os() == Os::Android {
            return;
        }
        assert_eq!(named.windows(), Platform::HOST.windows());
        assert_eq!(named.mach_o(), Platform::HOST.mach_o());
        assert_eq!(named.linux(), Platform::HOST.linux());
        assert_eq!(named.linux_kernel(), Platform::HOST.linux_kernel());
        assert_eq!(named.unix(), Platform::HOST.unix());
        assert_eq!(named.aarch64(), Platform::HOST.aarch64());
        assert_eq!(named.cpu_levels(), Platform::HOST.cpu_levels());
        assert_eq!(named.net(), Platform::HOST.net());
        assert_eq!(named.os_apis(), Platform::HOST.os_apis());
        assert_eq!(named.threads(), Platform::HOST.threads());
        // A Windows host asks Clang for the MSVC runtime even when the compiler itself is built for MinGW.
        if host.os() != Os::Windows || host.is_msvc() {
            assert_eq!(named.msvc(), Platform::HOST.msvc());
        }
    }

    #[test]
    fn platform_properties_follow_the_target_table() {
        use Triple::*;
        for triple in Triple::ALL {
            let platform = Platform::new(Some(triple));
            assert_eq!(platform.triple(), Some(triple));
            assert_eq!(platform.windows(), triple.os() == Os::Windows);
            assert_eq!(
                platform.executable_extension(),
                if platform.windows() { "exe" } else { "" }
            );
            assert_eq!(
                platform.object_extension(),
                if platform.windows() { "obj" } else { "o" }
            );
            assert_eq!(
                platform.unix(),
                !matches!(triple.os(), Os::Windows | Os::BareMetal)
            );
            assert_eq!(platform.hardware_fma(), triple.arch() == Arch::Aarch64);
            assert_eq!(platform.threads(), !triple.is_bare_metal());
        }
        for (triple, extension) in [
            (Aarch64AppleDarwin, "dylib"),
            (Aarch64AppleIosSim, "dylib"),
            (X86_64LinuxMusl, "so"),
            (Aarch64LinuxAndroid, "so"),
            (X86_64FreeBsd, "so"),
            (X86_64WindowsGnu, "dll"),
        ] {
            assert_eq!(Platform::new(Some(triple)).shared_extension(), extension);
        }
        // What `src/runtime/cpu.c` detects: CPUID on x86-64 off Windows, hwcap on a Linux AArch64.
        for (triple, levels) in [
            (X86_64LinuxGnu, 0b1110),
            (X86_64AppleDarwin, 0b1110),
            (X86_64FreeBsd, 0b1110),
            (X86_64LinuxAndroid, 0b1110),
            (X86_64WindowsMsvc, 0),
            (X86_64WindowsGnu, 0),
            (Aarch64LinuxGnu, 0b11_0000),
            (Aarch64LinuxMusl, 0b11_0000),
            (Aarch64LinuxAndroid, 0b11_0000),
            (Aarch64AppleDarwin, 0),
            (Aarch64FreeBsd, 0),
            (Aarch64WindowsMsvc, 0),
            (Riscv64LinuxGnu, 0),
        ] {
            assert_eq!(
                Platform::new(Some(triple)).cpu_levels(),
                levels,
                "{}",
                triple.name()
            );
        }
        // The socket and reactor runtimes are written for macOS, Linux, and Windows, and the
        // operating-system runtime for everything POSIX except iOS.
        for triple in Triple::ALL {
            let platform = Platform::new(Some(triple));
            assert_eq!(
                platform.net(),
                matches!(triple.os(), Os::MacOs | Os::Linux | Os::Windows),
                "{}",
                triple.name()
            );
            assert_eq!(platform.reactor(), platform.net());
            assert_eq!(
                platform.os_apis(),
                !matches!(triple.os(), Os::Windows | Os::Ios | Os::BareMetal),
                "{}",
                triple.name()
            );
        }
    }

    #[test]
    fn a_platform_scope_nests_and_restores() {
        assert_eq!(current_platform(), Platform::HOST);
        let linux = Platform::new(Some(Triple::X86_64LinuxGnu));
        let windows = Platform::new(Some(Triple::X86_64WindowsGnu));
        {
            let _outer = PlatformScope::enter(linux);
            assert_eq!(current_platform(), linux);
            {
                let _inner = PlatformScope::enter(windows);
                assert_eq!(current_platform(), windows);
            }
            assert_eq!(current_platform(), linux);
        }
        assert_eq!(current_platform(), Platform::HOST);
        let panicked = std::panic::catch_unwind(|| {
            let _scope = PlatformScope::enter(linux);
            panic!("unwinds through the scope");
        });
        assert!(panicked.is_err());
        assert_eq!(current_platform(), Platform::HOST);
    }

    #[test]
    fn a_runner_splits_at_white_space_only() {
        let runner =
            ForeignRunner::parse(OsStr::new("  qemu-aarch64   -L /usr/aarch64-linux-gnu "))
                .unwrap();
        assert_eq!(runner.program, OsString::from("qemu-aarch64"));
        assert_eq!(
            runner.arguments,
            ["-L", "/usr/aarch64-linux-gnu"].map(OsString::from)
        );
        let single = ForeignRunner::parse(OsStr::new("wine")).unwrap();
        assert_eq!(
            (single.program, single.arguments),
            ("wine".into(), Vec::new())
        );
        assert_eq!(ForeignRunner::parse(OsStr::new("")), None);
        assert_eq!(ForeignRunner::parse(OsStr::new(" \t\n")), None);
        // No quoting: a path with spaces needs a wrapper script.
        let quoted = ForeignRunner::parse(OsStr::new("'a b' c")).unwrap();
        assert_eq!(quoted.program, OsString::from("'a"));
    }
}
