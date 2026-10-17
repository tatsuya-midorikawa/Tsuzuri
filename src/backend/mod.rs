//! The fast debug backend (PB05): the `-O0` LLVM IR text that `src/llvm*.rs` emits is translated to an
//! object file with Cranelift instead of being compiled by Clang. Every LLVM instruction is mapped to
//! Cranelift IR one by one, so ownership, drops, traps and evaluation order keep the single lowering of
//! `src/llvm*.rs`. A construct that the translator does not know is E2002, never a different meaning.
//!
//! The module is scanned once on the calling thread, which finds the symbols a build needs and
//! declares them in the object. The bodies are then read, lowered and compiled one function at a time
//! on worker threads, and handed back in the order of the IR, so that the object does not depend on
//! how many threads there were.

mod abi;
mod call;
mod data;
mod error;
mod func;
mod intrinsics;
mod lex;
mod lower;
mod mem;
mod module;
mod ops;
mod parse;
mod target;
mod types;

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::{self, UserFuncName};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{
    DataDescription, DataId, FuncId, Linkage as ObjectLinkage, Module as _, ModuleReloc,
};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::diagnostic::Diagnostic;
use crate::parallel::{Parallelism, ordered_for_each};
use crate::target::Platform;
use crate::timings::Phase;

use self::error::{Failure, Res, bail};
use self::lower::{SymClass, SymInfo};
use self::module::{Linkage, Module, SymKind};
use self::target::{Format, Target};
use self::types::Types;

/// What reading a module found; for tests of the reader.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReadStats {
    pub functions: usize,
    pub reachable_functions: usize,
    pub globals: usize,
    pub instructions: usize,
}

/// How to translate.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// The machine the object is for.
    pub platform: Platform,
    /// Worker threads, at least 1.
    pub threads: usize,
    /// Run Cranelift's verifier on every function.
    pub verify: bool,
}

impl Options {
    /// The machine the compiler runs on.
    pub fn host(threads: usize) -> Self {
        Self {
            platform: Platform::HOST,
            threads: threads.max(1),
            verify: cfg!(debug_assertions),
        }
    }
}

/// Reads every function of the module without lowering it. It is the translator's front end alone:
/// the tests use it to see that every IR the compiler and the runtime produce is understood.
pub fn read_module(ir: &str) -> Result<ReadStats, Diagnostic> {
    let module = module::scan(ir).map_err(Failure::into_diagnostic)?;
    let reachable = module.reachable();
    let mut cx = func::Cx {
        module: &module,
        types: Types::overlay(&module.types, ir),
    };
    let mut stats = ReadStats::default();
    for (id, sym) in module.syms.iter().enumerate() {
        match &sym.kind {
            SymKind::Function {
                header,
                body_end: Some(_),
                ..
            } => {
                stats.functions += 1;
                stats.reachable_functions += usize::from(reachable[id]);
                let func = parse::parse_function(&mut cx, *header)
                    .map_err(|failure| failure.in_function(sym.name).into_diagnostic())?;
                stats.instructions += func.insts.len();
            }
            SymKind::Global { init: Some(_), .. } => stats.globals += 1,
            _ => {}
        }
    }
    Ok(stats)
}

/// Translates the textual LLVM IR of a `-O0` build into the bytes of a relocatable object.
pub fn compile(ir: &str, options: &Options) -> Result<Vec<u8>, Diagnostic> {
    translate(
        ir,
        options.platform,
        Parallelism::fixed(options.threads),
        options.verify,
    )
}

/// The size, alignment and field offsets of the type `ty` of the named types `definitions`, as the
/// translator lays them out. The tests compare them with what Clang computes.
#[doc(hidden)]
pub fn layout_of(definitions: &str, ty: &str) -> Result<(u64, u32, Vec<u64>), Diagnostic> {
    let layout = || -> Res<(u64, u32, Vec<u64>)> {
        let module = module::scan(definitions)?;
        let mut types = Types::overlay(&module.types, definitions);
        let text = format!("{ty}\n");
        let mut lexer = lex::Lexer::new(&text, 0);
        let parsed = types.parse_type(&mut lexer)?;
        let size = types.size_of(parsed)?;
        let align = types.align_of(parsed)?;
        let mut offsets = Vec::new();
        while let Some(offset) = types.field_offset(parsed, offsets.len()) {
            offsets.push(offset);
        }
        Ok((size, align, offsets))
    };
    layout().map_err(Failure::into_diagnostic)
}

pub(crate) fn translate(
    ir: &str,
    platform: Platform,
    parallelism: Parallelism,
    verify: bool,
) -> Result<Vec<u8>, Diagnostic> {
    run(ir, platform, parallelism, verify).map_err(Failure::into_diagnostic)
}

/// The function bodies that a worker needs to see, shared by every worker.
struct Shared<'a> {
    module: &'a Module<'a>,
    infos: &'a [SymInfo],
    target: &'a Target,
    isa: Arc<dyn TargetIsa>,
    work: &'a [u32],
}

struct Compiled {
    bytes: Vec<u8>,
    alignment: u64,
    relocs: Vec<ModuleReloc>,
}

struct State {
    context: Context,
    builder: FunctionBuilderContext,
    control: ControlPlane,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn object_linkage(linkage: Linkage, defined: bool) -> ObjectLinkage {
    if !defined {
        return ObjectLinkage::Import;
    }
    match linkage {
        Linkage::Local => ObjectLinkage::Local,
        Linkage::External => ObjectLinkage::Export,
        Linkage::Weak => ObjectLinkage::Preemptible,
    }
}

fn run(ir: &str, platform: Platform, parallelism: Parallelism, verify: bool) -> Res<Vec<u8>> {
    let target = Target::new(platform)?;
    let isa = target.isa(verify)?;
    let scan = Phase::start("backend.scan");
    let module = module::scan(ir)?;
    let needed = module.reachable();
    let types = Types::overlay(&module.types, ir);
    let mut builder = ObjectBuilder::new(
        isa.clone(),
        "tsuzuri",
        cranelift_module::default_libcall_names(),
    )
    .map_err(|error| Failure::new(format!("the object writer: {error}")))?;
    if target.format != Format::MachO {
        // So that the linker's garbage collection can drop what nothing uses, as Clang's
        // `-ffunction-sections -fdata-sections` allow; Mach-O subsections already do.
        builder.per_function_section(true);
        builder.per_data_object_section(true);
    }
    let mut object = ObjectModule::new(builder);

    // Declare every symbol that is needed, in the order of the IR.
    let mut infos: Vec<SymInfo> = Vec::with_capacity(module.syms.len());
    let mut work: Vec<u32> = Vec::new();
    let mut globals: Vec<u32> = Vec::new();
    for (id, sym) in module.syms.iter().enumerate() {
        if !needed[id] {
            infos.push(SymInfo {
                class: SymClass::Unsupported("a symbol the build does not need"),
                id: 0,
                colocated: false,
            });
            continue;
        }
        let name = sym
            .object_name()
            .map_err(|failure| failure.in_function(sym.name))?;
        match &sym.kind {
            SymKind::Function {
                sig,
                header,
                body_end,
            } => {
                let defined = body_end.is_some();
                // What the translator adds calls the C ABI of the platform with 128-bit integers.
                let external =
                    *header != module::SYNTHETIC && (sym.linkage != Linkage::Local || !defined);
                let abi = abi::plan(&types, &target, sig.ret, sig.ret_ext, &sig.params, external)
                    .map_err(|failure| failure.in_function(sym.name))?;
                let function = object
                    .declare_function(&name, object_linkage(sym.linkage, defined), &abi.sig)
                    .map_err(|error| Failure::new(format!("the symbol @{}: {error}", sym.name)))?;
                infos.push(SymInfo {
                    class: SymClass::Function(Box::new(abi)),
                    id: function.as_u32(),
                    colocated: defined && !(sym.linkage == Linkage::Weak && !sym.hidden),
                });
                if defined {
                    work.push(id as u32);
                }
            }
            SymKind::Global {
                constant,
                tls,
                init,
                ..
            } => {
                let defined = init.is_some();
                let data = object
                    .declare_data(&name, object_linkage(sym.linkage, defined), !constant, *tls)
                    .map_err(|error| Failure::new(format!("the symbol @{}: {error}", sym.name)))?;
                infos.push(SymInfo {
                    class: SymClass::Data { tls: *tls },
                    id: data.as_u32(),
                    colocated: defined && !(sym.linkage == Linkage::Weak && !sym.hidden),
                });
                if defined {
                    globals.push(id as u32);
                }
            }
            SymKind::Unsupported(what) => {
                bail!("{what} (@{})", sym.name);
            }
        }
    }
    drop(scan);

    // Lower and compile the bodies on the workers; define each in the order of the IR.
    let compile_phase = Phase::start("backend.compile");
    let shared = Shared {
        module: &module,
        infos: &infos,
        target: &target,
        isa: isa.clone(),
        work: &work,
    };
    let workers = parallelism.workers(work.len(), 8);
    let mut failure: Option<Failure> = None;
    ordered_for_each(
        work.len(),
        workers,
        8,
        None,
        |index| compile_function(&shared, index),
        |index, item| match item.get() {
            Ok(compiled) => {
                let id = FuncId::from_u32(infos[work[index] as usize].id);
                match object.define_function_bytes(
                    id,
                    compiled.alignment,
                    &compiled.bytes,
                    &compiled.relocs,
                ) {
                    Ok(()) => ControlFlow::Continue(()),
                    Err(error) => {
                        failure = Some(Failure::new(format!("the object writer: {error}")));
                        ControlFlow::Break(())
                    }
                }
            }
            Err(error) => {
                failure = Some(error);
                ControlFlow::Break(())
            }
        },
    );
    drop(compile_phase);
    if let Some(failure) = failure {
        return Err(failure);
    }

    let object_phase = Phase::start("backend.object");
    let mut cx = func::Cx {
        module: &module,
        types,
    };
    for &id in &globals {
        let sym = &module.syms[id as usize];
        let SymKind::Global {
            ty,
            init: Some((start, _)),
            align,
            ..
        } = &sym.kind
        else {
            continue;
        };
        let init = data::evaluate(&mut cx, *ty, *start)
            .map_err(|failure| failure.in_function(sym.name))?;
        let mut description = DataDescription::new();
        let natural = u64::from(
            cx.types
                .align_of(*ty)
                .map_err(|f| f.in_function(sym.name))?,
        );
        description.set_align(u64::from(*align).max(natural).max(1));
        match init {
            // Mach-O has no weak definitions in zerofill sections: Clang writes the zeros out.
            data::Init::Zero(size)
                if sym.linkage == Linkage::Weak && target.format == Format::MachO =>
            {
                description.define(vec![0; size as usize].into_boxed_slice());
            }
            data::Init::Zero(size) => description.define_zeroinit(size as usize),
            data::Init::Bytes { bytes, relocs } => {
                description.define(bytes.into_boxed_slice());
                for reloc in relocs {
                    let target_info = &infos[reloc.sym as usize];
                    match &target_info.class {
                        SymClass::Function(_) => {
                            if reloc.addend != 0 {
                                bail!(
                                    "an offset into the function @{} in @{}",
                                    module.syms[reloc.sym as usize].name,
                                    sym.name
                                );
                            }
                            let function = object.declare_func_in_data(
                                FuncId::from_u32(target_info.id),
                                &mut description,
                            );
                            description.write_function_addr(reloc.offset, function);
                        }
                        SymClass::Data { .. } => {
                            let global = object.declare_data_in_data(
                                DataId::from_u32(target_info.id),
                                &mut description,
                            );
                            description.write_data_addr(reloc.offset, global, reloc.addend);
                        }
                        SymClass::Unsupported(what) => {
                            bail!("{what} in the initializer of @{}", sym.name)
                        }
                    }
                }
            }
        }
        object
            .define_data(DataId::from_u32(infos[id as usize].id), &description)
            .map_err(|error| Failure::new(format!("the global @{}: {error}", sym.name)))?;
    }

    let mut product = object.finish();
    for (id, sym) in module.syms.iter().enumerate() {
        if !needed[id] || !sym.hidden || !sym.is_defined() {
            continue;
        }
        let symbol = match &sym.kind {
            SymKind::Function { .. } => product.function_symbol(FuncId::from_u32(infos[id].id)),
            SymKind::Global { .. } => product.data_symbol(DataId::from_u32(infos[id].id)),
            SymKind::Unsupported(_) => continue,
        };
        product.object.symbol_mut(symbol).scope =
            cranelift_object::object::write::SymbolScope::Linkage;
    }
    if target.format == Format::MachO {
        // Without a platform load command the linker warns that it assumes macOS.
        let mut version = cranelift_object::object::write::MachOBuildVersion::default();
        version.platform = cranelift_object::object::macho::PLATFORM_MACOS;
        let minimum = match target.arch {
            target::Arch::Aarch64 => 11 << 16,
            target::Arch::X86_64 => (10 << 16) | (13 << 8),
        };
        version.minos = minimum;
        version.sdk = minimum;
        product.object.set_macho_build_version(version);
    }
    let bytes = product
        .emit()
        .map_err(|error| Failure::new(format!("the object writer: {error}")))?;
    drop(object_phase);
    Ok(bytes)
}

fn compile_function(shared: &Shared<'_>, index: usize) -> Res<Compiled> {
    let id = shared.work[index];
    let sym = &shared.module.syms[id as usize];
    // A few instruction-selection rules of Cranelift stop with `unimplemented!`; that is a refusal
    // of the translator like any other, not a crash of the compiler.
    let attempt = catch_unwind(AssertUnwindSafe(|| {
        STATE.with(|cell| {
            let mut slot = cell.borrow_mut();
            let state = slot.get_or_insert_with(|| State {
                context: Context::new(),
                builder: FunctionBuilderContext::new(),
                control: ControlPlane::default(),
            });
            let result = compile_in(shared, state, id);
            if result.is_err() {
                // A function that failed leaves the builder's context half used.
                *slot = None;
            }
            result
        })
    }));
    let result = attempt.unwrap_or_else(|payload| {
        STATE.with(|cell| *cell.borrow_mut() = None);
        let detail = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(Failure::new(format!(
            "an operation that Cranelift does not implement for this target ({detail})"
        )))
    });
    result.map_err(|failure| failure.in_function(sym.name))
}

fn compile_in(shared: &Shared<'_>, state: &mut State, id: u32) -> Res<Compiled> {
    let module = shared.module;
    let sym = &module.syms[id as usize];
    let SymKind::Function { header, .. } = &sym.kind else {
        bail!("a body that is not a function");
    };
    let info = &shared.infos[id as usize];
    let SymClass::Function(abi) = &info.class else {
        bail!("a function without a signature");
    };
    let mut cx = func::Cx {
        module,
        types: Types::overlay(&module.types, module.text),
    };
    let parsed = parse::parse_function(&mut cx, *header)?;
    state.context.clear();
    state.context.func =
        ir::Function::with_name_signature(UserFuncName::user(0, info.id), abi.sig.clone());
    lower::lower_function(
        &mut state.context.func,
        &mut state.builder,
        module,
        &mut cx.types,
        shared.infos,
        shared.target,
        shared.isa.frontend_config(),
        &parsed,
        abi,
    )?;
    state
        .context
        .compile(&*shared.isa, &mut state.control)
        .map_err(|error| Failure::new(format!("a Cranelift error: {:?}", error.inner)))?;
    let code = state
        .context
        .compiled_code()
        .ok_or_else(|| Failure::new("a function Cranelift did not compile"))?;
    let alignment = u64::from(code.buffer.alignment);
    let bytes = code.code_buffer().to_vec();
    let function = FuncId::from_u32(info.id);
    let relocs = code
        .buffer
        .relocs()
        .iter()
        .map(|reloc| ModuleReloc::from_mach_reloc(reloc, &state.context.func, function))
        .collect();
    Ok(Compiled {
        bytes,
        alignment,
        relocs,
    })
}
