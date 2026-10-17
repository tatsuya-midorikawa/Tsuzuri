//! The machine code the translator makes: which instruction set, which object format, which calling
//! convention, and the Cranelift settings that follow from them. Nothing here looks at the CPU the
//! compiler happens to run on, so the same IR gives the same object everywhere.

use std::str::FromStr;
use std::sync::Arc;

use cranelift_codegen::ir::{self, types};
use cranelift_codegen::isa::{self, CallConv, TargetIsa};
use cranelift_codegen::settings::{self, Configurable};
use target_lexicon::Triple;

use super::error::{Failure, Res, bail};
use crate::target::Platform;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Arch {
    Aarch64,
    X86_64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    MachO,
    Elf,
    Coff,
}

#[derive(Clone)]
pub(crate) struct Target {
    pub arch: Arch,
    pub format: Format,
    pub pointer: ir::Type,
    pub call_conv: CallConv,
    /// The words a function returns in registers; more go through a pointer.
    pub max_return_words: usize,
    /// Whether a call to a function of another object file may be a plain branch to the symbol.
    /// Mach-O linkers make a stub for it; ELF links go through the GOT.
    pub direct_imports: bool,
}

impl Target {
    pub(crate) fn new(platform: Platform) -> Res<Self> {
        let arch = if platform.aarch64() {
            Arch::Aarch64
        } else if platform.arch_name() == "x86_64" {
            Arch::X86_64
        } else {
            bail!("the architecture {}", platform.arch_name());
        };
        let format = if platform.mach_o() {
            Format::MachO
        } else if platform.windows() {
            Format::Coff
        } else if platform.linux_kernel() {
            Format::Elf
        } else {
            bail!("the operating system {}", platform.os_name());
        };
        let triple = Self::triple_of(arch, format);
        let call_conv = CallConv::triple_default(&triple);
        Ok(Self {
            arch,
            format,
            pointer: types::I64,
            call_conv,
            max_return_words: if format == Format::Coff { 1 } else { 2 },
            direct_imports: format == Format::MachO,
        })
    }

    fn triple_of(arch: Arch, format: Format) -> Triple {
        let text = match (arch, format) {
            (Arch::Aarch64, Format::MachO) => "aarch64-apple-darwin",
            (Arch::X86_64, Format::MachO) => "x86_64-apple-darwin",
            (Arch::Aarch64, Format::Elf) => "aarch64-unknown-linux-gnu",
            (Arch::X86_64, Format::Elf) => "x86_64-unknown-linux-gnu",
            (Arch::Aarch64, Format::Coff) => "aarch64-pc-windows-msvc",
            (Arch::X86_64, Format::Coff) => "x86_64-pc-windows-msvc",
        };
        Triple::from_str(text).expect("a built-in triple")
    }

    pub(crate) fn triple(&self) -> Triple {
        Self::triple_of(self.arch, self.format)
    }

    /// The code generator for this target. `verify` runs Cranelift's IR verifier on every function.
    pub(crate) fn isa(&self, verify: bool) -> Res<Arc<dyn TargetIsa>> {
        let mut flags = settings::builder();
        let mut set = |name: &str, value: &str| {
            flags
                .set(name, value)
                .map_err(|error| Failure::new(format!("the Cranelift setting {name}: {error}")))
        };
        set("opt_level", "none")?;
        set("is_pic", "true")?;
        set("enable_verifier", if verify { "true" } else { "false" })?;
        // A NaN keeps its payload through a copy and a store, as it does in LLVM's code.
        set("enable_nan_canonicalization", "false")?;
        set("enable_probestack", "true")?;
        set("probestack_strategy", "inline")?;
        set("unwind_info", "false")?;
        set("preserve_frame_pointers", "true")?;
        set(
            "tls_model",
            match self.format {
                Format::MachO => "macho",
                Format::Elf => "elf_gd",
                Format::Coff => "coff",
            },
        )?;
        let builder = isa::lookup(self.triple())
            .map_err(|error| Failure::new(format!("the target {}: {error}", self.triple())))?;
        builder
            .finish(settings::Flags::new(flags))
            .map_err(|error| Failure::new(format!("the target {}: {error}", self.triple())))
    }
}
