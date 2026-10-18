//! The fast debug backend for `wasm32` (PB05, D13): the `-O0` LLVM IR text is translated to a
//! relocatable WebAssembly object that `wasm-ld` links with the objects of the runtime, instead of
//! being compiled by Clang. The front half is the one of the native translator (`super::module`,
//! `super::parse`, `super::types`, `super::data`); only the lowering differs.
//!
//! WebAssembly has no registers to allocate and no instructions to select, so the lowering is a
//! direct emission: every value is a few locals, every instruction a few opcodes. The object has the
//! sections that Clang's have, with the relocations that `wasm-ld` needs, so that it links with the
//! prebuilt runtime objects as they are.

mod abi;
mod bin;
mod intrinsics;
mod lower;
mod mem;
mod object;
mod ops;

use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::diagnostic::Diagnostic;
use crate::parallel::{Parallelism, ordered_for_each};
use crate::timings::Phase;

use self::abi::Abi;
use self::object::{
    BINDING_LOCAL, BINDING_WEAK, Body, EXPLICIT_NAME, Import, Kind, Object, Reloc, Segment, Symbol,
    UNDEFINED, VISIBILITY_HIDDEN,
};
use super::data::{self, Init};
use super::error::{Failure, FxMap, Res, bail};
use super::func;
use super::module::{self, Linkage, Module, SymKind};
use super::parse;
use super::types::Types;

/// Placeholders for the symbols of the stack pointer and of the table in the relocations that the
/// bodies make before it is known whether the object has them.
const STACK_POINTER: u32 = u32::MAX - 1;
const TABLE: u32 = u32::MAX - 2;

/// What a worker needs to see of the module, shared by every worker.
struct Shared<'a> {
    module: &'a Module<'a>,
    infos: &'a [WInfo],
    sp_symbol: u32,
    table_symbol: u32,
    /// Whether the C ABI of the linked runtime returns a 128-bit integer as two results.
    multivalue: bool,
}

struct FnInfo {
    abi: Abi,
    /// The entry of the symbol table.
    symbol: u32,
    /// The index in the function space of the object.
    index: u32,
}

struct DataInfo {
    symbol: u32,
}

enum WInfo {
    Func(FnInfo),
    Data(DataInfo),
    /// A symbol that the build does not need.
    Skip,
}

/// Translates the textual LLVM IR of a `wasm32` `-O0` build into the bytes of a relocatable object.
/// `multivalue` is whether the compiler that builds the rest of the program returns a 128-bit
/// integer from a function with two results (its `wasm32` generic CPU does, `-mno-multivalue` does
/// not); the routines of the runtime that the IR defines and the runtime objects must agree on it.
pub(crate) fn translate(
    ir: &str,
    parallelism: Parallelism,
    multivalue: bool,
) -> Result<Vec<u8>, Diagnostic> {
    run(ir, parallelism, multivalue).map_err(Failure::into_diagnostic)
}

/// Whether the runtime objects that are already compiled, with the compiler that built them, return a
/// 128-bit integer as two results: the first of them that calls a routine of the compiler runtime
/// says; with none that does, the choice is free, and two results (the default of a recent Clang) it is.
pub(crate) fn multivalue_of_objects<'a>(objects: impl IntoIterator<Item = &'a [u8]>) -> bool {
    objects
        .into_iter()
        .find_map(object::i128_results_are_multivalue)
        .unwrap_or(true)
}

fn flags_of(linkage: Linkage, hidden: bool) -> u32 {
    let mut flags = match linkage {
        Linkage::Local => BINDING_LOCAL,
        Linkage::External => 0,
        Linkage::Weak => BINDING_WEAK,
    };
    if hidden && linkage != Linkage::Local {
        flags |= VISIBILITY_HIDDEN;
    }
    flags
}

/// The text of the string attribute `key` of a declaration, if it has it.
fn string_attribute<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\"=\"");
    let start = line.find(&marker)? + marker.len();
    let end = line[start..].find('"')?;
    Some(&line[start..start + end])
}

/// The module and the name that an undefined function is imported by: `wasm-import-module` and
/// `wasm-import-name` of the declaration, else the module `env` and the symbol's own name.
fn import_of<'a>(text: &'a str, header: usize) -> (Option<&'a str>, Option<&'a str>) {
    if header == module::SYNTHETIC {
        return (None, None);
    }
    let rest = &text[header..];
    let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
    (
        string_attribute(line, "wasm-import-module"),
        string_attribute(line, "wasm-import-name"),
    )
}

fn run(ir: &str, parallelism: Parallelism, multivalue: bool) -> Res<Vec<u8>> {
    let scan = Phase::start("backend.scan");
    let module = module::scan_with(ir, 4)?;
    let needed = module.reachable();
    let types = Types::overlay(&module.types, ir);

    // The function space has the imports first; count them to number the definitions.
    let imports = module
        .syms
        .iter()
        .enumerate()
        .filter(|(id, sym)| {
            needed[*id] && matches!(sym.kind, SymKind::Function { body_end: None, .. })
        })
        .count() as u32;

    // The imports are numbered, and listed in the symbol table, in the order that the code first
    // refers to them (then the data, then those that nothing refers to), as Clang does; the
    // module that the link makes lists its imports in that order.
    let is_import = |id: usize| {
        needed[id]
            && matches!(
                module.syms[id].kind,
                SymKind::Function { body_end: None, .. }
            )
    };
    let mut rank: FxMap<u32, u32> = FxMap::default();
    let mut note = |name: &str| {
        if let Some(&id) = module.by_name.get(name)
            && is_import(id as usize)
        {
            let next = rank.len() as u32;
            rank.entry(id).or_insert(next);
        }
    };
    for defined_functions in [true, false] {
        for (id, sym) in module.syms.iter().enumerate() {
            let defined = matches!(
                sym.kind,
                SymKind::Function {
                    body_end: Some(_),
                    ..
                }
            );
            let global = matches!(sym.kind, SymKind::Global { init: Some(_), .. });
            if needed[id]
                && (if defined_functions { defined } else { global })
                && let Some((start, end)) = module.extent(sym)
            {
                module::for_each_reference(ir, start, end, &mut note);
            }
        }
    }
    for id in 0..module.syms.len() {
        if is_import(id) {
            let next = rank.len() as u32;
            rank.entry(id as u32).or_insert(next);
        }
    }
    let needed_symbols = needed.iter().filter(|&&needed| needed).count();
    let mut symbols: Vec<Option<Symbol>> = (0..needed_symbols).map(|_| None).collect();
    let mut import_list: Vec<Option<Import>> = (0..imports).map(|_| None).collect();
    let non_imports = needed_symbols as u32 - imports;

    let mut object = Object::default();
    let mut infos: Vec<WInfo> = Vec::with_capacity(module.syms.len());
    let mut work: Vec<u32> = Vec::new();
    let mut globals: Vec<u32> = Vec::new();
    let (mut next_defined, mut next_segment, mut next_symbol) = (0u32, 0u32, 0u32);
    for (id, sym) in module.syms.iter().enumerate() {
        if !needed[id] {
            infos.push(WInfo::Skip);
            continue;
        }
        let name = sym
            .object_name()
            .map_err(|failure| failure.in_function(sym.name))?
            .into_owned();
        let symbol = if is_import(id) {
            non_imports + rank[&(id as u32)]
        } else {
            next_symbol += 1;
            next_symbol - 1
        };
        match &sym.kind {
            SymKind::Function {
                sig,
                header,
                body_end,
            } => {
                let defined = body_end.is_some();
                let external = sym.linkage != Linkage::Local || !defined;
                let abi = abi::plan(
                    &types,
                    sig.ret,
                    sig.ret_ext,
                    &sig.params,
                    external,
                    multivalue,
                )
                .map_err(|failure| failure.in_function(sym.name))?;
                let mut flags = flags_of(sym.linkage, sym.hidden);
                let index;
                if defined {
                    index = imports + next_defined;
                    next_defined += 1;
                    work.push(id as u32);
                } else {
                    index = rank[&(id as u32)];
                    let (from, field) = import_of(ir, *header);
                    // A declaration that names its import is an import that the link does not
                    // look for in another object: Clang marks its symbol as explicitly named.
                    if field.is_some() {
                        flags |= EXPLICIT_NAME;
                    }
                    let field = field.unwrap_or(&name).to_owned();
                    flags |= UNDEFINED;
                    import_list[index as usize] = Some(Import {
                        module: from.unwrap_or("env").to_owned(),
                        field,
                        signature: (abi.wasm_params.clone(), abi.wasm_results.clone()),
                    });
                }
                symbols[symbol as usize] = Some(Symbol::Function { name, flags, index });
                infos.push(WInfo::Func(FnInfo { abi, symbol, index }));
            }
            // A thread-local global is a plain one: a module without threads has the one thread.
            SymKind::Global { ty, init, .. } => {
                let mut flags = flags_of(sym.linkage, sym.hidden);
                let (segment, size) = if init.is_some() {
                    let size = types
                        .size_of(*ty)
                        .map_err(|failure| failure.in_function(sym.name))?;
                    globals.push(id as u32);
                    next_segment += 1;
                    (next_segment - 1, size as u32)
                } else {
                    flags |= UNDEFINED;
                    (0, 0)
                };
                symbols[symbol as usize] = Some(Symbol::Data {
                    name,
                    flags,
                    segment,
                    size,
                });
                infos.push(WInfo::Data(DataInfo { symbol }));
            }
            SymKind::Unsupported(what) => bail!("{what} (@{})", sym.name),
        }
    }
    object.symbols = symbols.into_iter().flatten().collect();
    object.imports = import_list.into_iter().flatten().collect();
    drop(scan);

    // Lower the bodies on the workers; take them back in the order of the IR.
    let compile_phase = Phase::start("backend.compile");
    let shared = Shared {
        module: &module,
        infos: &infos,
        sp_symbol: STACK_POINTER,
        table_symbol: TABLE,
        multivalue,
    };
    let workers = parallelism.workers(work.len(), 8);
    let mut failure: Option<Failure> = None;
    let mut bodies: Vec<Body> = Vec::with_capacity(work.len());
    ordered_for_each(
        work.len(),
        workers,
        8,
        None,
        |index| lower_function(&shared, work[index]),
        |_, item| match item.get() {
            Ok(body) => {
                bodies.push(body);
                ControlFlow::Continue(())
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
            constant,
            init: Some((start, _)),
            align,
            ..
        } = &sym.kind
        else {
            continue;
        };
        let init = data::evaluate(&mut cx, *ty, *start)
            .map_err(|failure| failure.in_function(sym.name))?;
        let natural = cx
            .types
            .align_of(*ty)
            .map_err(|failure| failure.in_function(sym.name))?;
        let align = (*align).max(natural).max(1);
        let name = sym.object_name().map_err(|f| f.in_function(sym.name))?;
        let (prefix, data, relocs) = match init {
            Init::Zero(size) => (".bss.", vec![0; size as usize], Vec::new()),
            Init::Bytes { bytes, relocs } => {
                let prefix = if *constant { ".rodata." } else { ".data." };
                (prefix, bytes, relocs)
            }
        };
        let mut segment_relocs = Vec::with_capacity(relocs.len());
        for reloc in relocs {
            let target = reloc.sym as usize;
            match &infos[target] {
                WInfo::Func(info) => {
                    if reloc.addend != 0 {
                        bail!(
                            "an offset into the function @{} in @{}",
                            module.syms[target].name,
                            sym.name
                        );
                    }
                    segment_relocs.push(Reloc {
                        offset: reloc.offset,
                        kind: Kind::TableIndexI32,
                        index: info.symbol,
                        addend: 0,
                    });
                }
                WInfo::Data(info) => {
                    let addend = i32::try_from(reloc.addend)
                        .map_err(|_| Failure::new("an offset this large in an initializer"))?;
                    segment_relocs.push(Reloc {
                        offset: reloc.offset,
                        kind: Kind::MemoryAddrI32,
                        index: info.symbol,
                        addend,
                    });
                }
                WInfo::Skip => bail!(
                    "a symbol that the build does not need in the initializer of @{}",
                    sym.name
                ),
            }
        }
        object.segments.push(Segment {
            name: format!("{prefix}{name}"),
            align: align.trailing_zeros() as u8,
            data,
            relocs: segment_relocs,
        });
    }

    // The symbols of the stack pointer and the table exist when a body uses them.
    let mut uses_stack_pointer = false;
    let mut uses_table = false;
    for body in &bodies {
        for reloc in &body.relocs {
            match reloc.kind {
                Kind::GlobalIndex => uses_stack_pointer = true,
                Kind::TableNumber => uses_table = true,
                _ => {}
            }
        }
    }
    // The functions whose address is taken take the slots of the table, in the order of the object.
    let mut function_of_symbol: Vec<Option<u32>> = vec![None; object.symbols.len()];
    for (symbol, entry) in object.symbols.iter().enumerate() {
        if let Symbol::Function { index, .. } = entry {
            function_of_symbol[symbol] = Some(*index);
        }
    }
    let mut address_taken: Vec<u32> = Vec::new();
    let relocs = bodies.iter().flat_map(|body| body.relocs.iter()).chain(
        object
            .segments
            .iter()
            .flat_map(|segment| segment.relocs.iter()),
    );
    for reloc in relocs {
        if matches!(reloc.kind, Kind::TableIndexSleb | Kind::TableIndexI32)
            && let Some(Some(index)) = function_of_symbol.get(reloc.index as usize)
        {
            address_taken.push(*index);
        }
    }
    address_taken.sort_unstable();
    address_taken.dedup();
    let sp_symbol = uses_stack_pointer.then(|| {
        object.symbols.push(Symbol::Global {
            flags: UNDEFINED,
            index: 0,
        });
        object.symbols.len() as u32 - 1
    });
    let table_symbol = uses_table.then(|| {
        object.symbols.push(Symbol::Table {
            flags: UNDEFINED | object::NO_STRIP,
            index: 0,
        });
        object.symbols.len() as u32 - 1
    });
    for body in &mut bodies {
        for reloc in &mut body.relocs {
            match (reloc.kind, reloc.index) {
                (Kind::GlobalIndex, STACK_POINTER) => reloc.index = sp_symbol.unwrap_or(0),
                (Kind::TableNumber, TABLE) => reloc.index = table_symbol.unwrap_or(0),
                _ => {}
            }
        }
    }
    for (id, body) in work.iter().zip(bodies) {
        let WInfo::Func(info) = &infos[*id as usize] else {
            bail!("a body without a function");
        };
        object.functions.push((
            (info.abi.wasm_params.clone(), info.abi.wasm_results.clone()),
            body,
        ));
    }
    object.uses_stack_pointer = uses_stack_pointer;
    object.uses_table = uses_table || !address_taken.is_empty();
    object.address_taken = address_taken;
    let bytes = object.finish();
    drop(object_phase);
    Ok(bytes)
}

/// Reads and lowers the body of one function. A refusal of the lowering is `Err`; so is a bug of it
/// that would stop the compiler, since a build falls back to Clang on either.
fn lower_function(shared: &Shared<'_>, id: u32) -> Res<Body> {
    let sym = &shared.module.syms[id as usize];
    let attempt = catch_unwind(AssertUnwindSafe(|| -> Res<Body> {
        let SymKind::Function { header, .. } = &sym.kind else {
            bail!("a body that is not a function");
        };
        let WInfo::Func(info) = &shared.infos[id as usize] else {
            bail!("a function without a signature");
        };
        let mut cx = func::Cx {
            module: shared.module,
            types: Types::overlay(&shared.module.types, shared.module.text),
        };
        let parsed = parse::parse_function(&mut cx, *header)?;
        lower::lower_function(shared, &mut cx.types, &parsed, info)
    }));
    let result = attempt.unwrap_or_else(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(Failure::new(format!(
            "an internal error of the WebAssembly translator ({detail})"
        )))
    });
    result.map_err(|failure| failure.in_function(sym.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_of(ir: &str, multivalue: bool) -> Vec<u8> {
        translate(ir, Parallelism::fixed(1), multivalue).expect("a translation")
    }

    #[test]
    fn a_variable_shift_of_128_bits_calls_the_routine_of_the_runtime() {
        let variable = "define i64 @f(i64 %a, i64 %b) {\n  %x = zext i64 %a to i128\n  %n = zext i64 %b to i128\n  %r = shl i128 %x, %n\n  %t = trunc i128 %r to i64\n  ret i64 %t\n}\n";
        // The routine is imported, with the results the way the runtime objects return them.
        assert_eq!(
            object::i128_results_are_multivalue(&object_of(variable, true)),
            Some(true)
        );
        assert_eq!(
            object::i128_results_are_multivalue(&object_of(variable, false)),
            Some(false)
        );
        // A constant amount is a few word operations and calls nothing.
        let constant = variable.replace("shl i128 %x, %n", "shl i128 %x, 64");
        assert_eq!(
            object::i128_results_are_multivalue(&object_of(&constant, true)),
            None
        );
    }

    #[test]
    fn imports_are_numbered_in_the_order_the_code_first_uses_them() {
        let ir = "declare i64 @aaa(i64)\ndeclare i64 @bbb(i64)\ndeclare i64 @ccc(i64)\ndefine i64 @g(i64 %x) {\n  %a = call i64 @ccc(i64 %x)\n  %b = call i64 @aaa(i64 %a)\n  %c = call i64 @bbb(i64 %b)\n  ret i64 %c\n}\n";
        let bytes = object_of(ir, true);
        let at = |name: &[u8]| {
            bytes
                .windows(name.len())
                .position(|window| window == name)
                .expect("the import is named")
        };
        // The import section comes before the symbol table, so the first mention is the import's.
        assert!(at(b"ccc") < at(b"aaa") && at(b"aaa") < at(b"bbb"));
    }

    #[test]
    fn an_object_starts_with_the_magic_and_has_the_linking_section() {
        let bytes = object_of("define i32 @f(i32 %a) {\n  ret i32 %a\n}\n", true);
        assert_eq!(&bytes[..8], b"\0asm\x01\0\0\0");
        assert!(bytes.windows(7).any(|window| window == b"linking"));
        assert!(bytes.windows(10).any(|window| window == b"reloc.CODE"));
    }
}
