use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::check::{
    Builtin, BuiltinInstance, CheckedFunction, CheckedModule, FunctionRef, Local, ModuleOrigin,
    Type, TypedExpr, TypedExprKind,
};
use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::{BinaryOp, UnaryOp};

#[path = "call_specialization.rs"]
mod call_specialization;
#[path = "llvm_control.rs"]
mod control;
#[path = "llvm_frame.rs"]
mod frame;
use call_specialization::{ClosureTarget, Specialization, Specializations};
use frame::Frame;

/// The builtin instances that emitted code calls, with their concrete
/// callee types.
type Builtins = BTreeMap<BuiltinInstance, Type>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Library,
    Console,
}

pub fn emit(module: &CheckedModule, entry: Entry) -> Result<String, Diagnostic> {
    emit_target(module, entry, false)
}

pub fn emit_target(module: &CheckedModule, entry: Entry, wasm: bool) -> Result<String, Diagnostic> {
    if entry == Entry::Console {
        validate_main(module)?;
    }
    let mut output = String::from(
        "; Tsuzuri - deterministic LLVM IR\nsource_filename = \"tsuzuri\"\n%tz.string = type { ptr, i64 }\n%tz.array = type { ptr, i64 }\n%tz.list = type { ptr, i64 }\n%tz.closure = type { ptr, ptr, ptr, ptr }\n",
    );
    let types = module.types();
    let reachable = reachable_functions(module);
    let emitted: Vec<bool> = (0..module.functions.len())
        .map(|id| emit_function(id, &reachable, module))
        .collect();
    let named = named_types(module, &emitted);
    let generic = |ty: &&Type| matches!(ty, Type::Record(_, arguments) | Type::Union(_, arguments) if !arguments.is_empty());
    let record_types = (0..module.records.len())
        .filter(|id| module.records[*id].parameters.is_empty())
        .map(|id| Type::Record(id, Box::default()))
        .filter(|ty| {
            let Type::Record(id, _) = ty else {
                unreachable!()
            };
            module.records[*id].origin == ModuleOrigin::User || named.contains(ty)
        })
        .chain(
            named
                .iter()
                .filter(generic)
                .filter(|ty| matches!(ty, Type::Record(..)))
                .cloned(),
        );
    for ty in record_types {
        let Type::Record(id, arguments) = &ty else {
            unreachable!("only record types are collected")
        };
        let _ = writeln!(
            output,
            "{} = type {{ {} }}",
            llvm_type(&ty, module),
            types
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| llvm_type(ty, module))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let union_types = (0..module.unions.len())
        .filter(|id| module.unions[*id].parameters.is_empty())
        .map(|id| Type::Union(id, Box::default()))
        .filter(|ty| {
            let Type::Union(id, _) = ty else {
                unreachable!()
            };
            module.unions[*id].origin == ModuleOrigin::User || named.contains(ty)
        })
        .chain(
            named
                .iter()
                .filter(generic)
                .filter(|ty| matches!(ty, Type::Union(..)))
                .cloned(),
        );
    for ty in union_types {
        let Type::Union(id, arguments) = &ty else {
            unreachable!("only union types are collected")
        };
        let layout = match union_layout(*id, arguments, module) {
            UnionLayout::Enum => "i32".into(),
            UnionLayout::Common(payload) => format!("{{ i32, {} }}", llvm_type(&payload, module)),
            UnionLayout::General(count) => format!("{{ i32, [{count} x i128] }}"),
        };
        let _ = writeln!(output, "{} = type {layout}", llvm_type(&ty, module));
    }
    output.push_str("\ndeclare void @llvm.trap()\n\n");
    let mut builtins = Builtins::new();
    let mut intrinsics = BTreeSet::new();
    let mut globals = Globals {
        wasm,
        ..Globals::default()
    };
    let mut specializations = Specializations::new(module);
    for (id, function) in module.functions.iter().enumerate() {
        if !emitted[id] {
            continue;
        }
        let emitter = FunctionEmitter::new(
            module,
            function,
            id,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        );
        output.push_str(&emitter.emit());
        output.push_str(&closure_wrappers(
            module,
            function,
            id,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        ));
        if function.exported {
            output.push_str(&export_wrapper(function, module));
        }
    }
    let mut next = 0;
    while let Some(key) = specializations.requests.get(next).cloned() {
        let emitter = FunctionEmitter::new(
            module,
            &module.functions[key.function],
            key.function,
            &mut builtins,
            &mut intrinsics,
            &mut globals,
            &mut specializations,
        )
        .specialized(next, &key);
        output.push_str(&emitter.emit());
        next += 1;
    }
    for (instance, ty) in &builtins {
        output.push_str(&emit_builtin(instance, ty, module, &mut intrinsics));
    }
    for intrinsic in intrinsics {
        let _ = writeln!(output, "{intrinsic}");
    }
    if entry == Entry::Console {
        output.push_str(&console_main(module));
    }
    for global in globals.definitions {
        output.push_str(&global);
        output.push('\n');
    }
    if output.contains("@tsuzuri_task_parallel(") {
        output.push_str(if wasm {
            include_str!("runtime/task-wasm.ll")
        } else {
            "declare void @tsuzuri_task_parallel(ptr, ptr, i64)\n"
        });
    }
    if output.contains("@tz_soft_") {
        output = output.replace("declare void @llvm.trap()\n", "");
        output.push_str(include_str!("runtime/numeric.ll"));
    }
    if output.contains("@tz.closure.") {
        output.push_str(include_str!("runtime/closure.ll"));
    }
    if output.contains("@tz.string.") || output.contains("@tz.free") || output.contains("@tz.alloc")
    {
        output.push_str(include_str!("runtime/string.ll"));
        output.push_str(if wasm {
            include_str!("runtime/heap-wasm.ll")
        } else {
            include_str!("runtime/heap-native.ll")
        });
    }
    Ok(output)
}

pub fn header(module: &CheckedModule) -> String {
    let mut output = String::from(
        "/* Generated by Tsuzuri. bool uses int32_t. Narrow integers use normalized 32-bit ABI values. */\n\
         #pragma once\n\
         #include <stdint.h>\n\n\
         #ifdef __cplusplus\n\
         extern \"C\" {\n\
         #endif\n\n",
    );
    for function in &module.functions {
        if !function.exported {
            continue;
        }
        let parameters = if function.signature.parameters.is_empty() {
            "void".into()
        } else {
            function
                .signature
                .parameters
                .iter()
                .enumerate()
                .map(|(id, ty)| format!("{} arg{id}", c_type(ty)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            output,
            "{} tz_{}({parameters});",
            c_type(&function.signature.result),
            function.name
        );
    }
    output.push_str("\n#ifdef __cplusplus\n}\n#endif\n");
    output
}

struct Globals {
    definitions: Vec<String>,
    next_metadata: usize,
    wasm: bool,
}

impl Default for Globals {
    fn default() -> Self {
        static FIRST_METADATA: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        let next_metadata = *FIRST_METADATA.get_or_init(|| {
            include_str!("runtime/numeric.ll")
                .lines()
                .filter_map(|line| {
                    line.strip_prefix('!')?
                        .split_once('=')?
                        .0
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
                .max()
                .map_or(0, |id| id + 1)
        });
        Self {
            definitions: Vec::new(),
            next_metadata,
            wasm: false,
        }
    }
}

fn environment_type(function: &CheckedFunction, count: usize, module: &CheckedModule) -> String {
    format!(
        "{{ {} }}",
        function.signature.parameters[..count]
            .iter()
            .map(|ty| llvm_type(ty, module))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn closure_wrappers(
    module: &CheckedModule,
    function: &CheckedFunction,
    id: usize,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut output = String::new();
    let name = function.qualified_name();
    let arity = function.parameters.len();
    for count in function.capture_count..arity.max(function.capture_count + 1) {
        let environment = environment_type(function, count, module);
        if count != 0 {
            if !function.is_task
                && function.signature.parameters[..count]
                    .iter()
                    .all(|ty| ty.can_capture(&module.types()))
            {
                let mut clone = FunctionEmitter::new(
                    module,
                    function,
                    id,
                    builtins,
                    intrinsics,
                    globals,
                    specializations,
                );
                let allocation = clone.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({environment}, ptr null, i32 1) to i64))"));
                for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
                    let pointer = clone.value(format!(
                        "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
                    ));
                    let value = clone.value(format!("load {}, ptr {pointer}", clone.ty(ty)));
                    let value = clone.clone_value(ty, &value);
                    let target = clone.value(format!(
                        "getelementptr inbounds {environment}, ptr {allocation}, i32 0, i32 {index}"
                    ));
                    clone.instruction(format!("store {} {value}, ptr {target}", clone.ty(ty)));
                }
                clone.instruction(format!("ret ptr {allocation}"));
                output.push_str(
                    &clone.auxiliary(&format!("ptr @tz.env.clone.{name}.{count}(ptr %env)")),
                );
            }

            let mut drop = FunctionEmitter::new(
                module,
                function,
                id,
                builtins,
                intrinsics,
                globals,
                specializations,
            );
            for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
                if ty.needs_drop(&module.types()) {
                    let pointer = drop.value(format!(
                        "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
                    ));
                    let value = drop.value(format!("load {}, ptr {pointer}", drop.ty(ty)));
                    drop.drop_value(ty, &value);
                }
            }
            drop.instruction("call void @tz.free(ptr %env)");
            drop.instruction("ret void");
            output
                .push_str(&drop.auxiliary(&format!("void @tz.env.drop.{name}.{count}(ptr %env)")));
        }
        let mut apply = FunctionEmitter::new(
            module,
            function,
            id,
            builtins,
            intrinsics,
            globals,
            specializations,
        );
        let mut values = Vec::new();
        for (index, ty) in function.signature.parameters[..count].iter().enumerate() {
            let pointer = apply.value(format!(
                "getelementptr inbounds {environment}, ptr %env, i32 0, i32 {index}"
            ));
            values.push(apply.value(format!("load {}, ptr {pointer}", apply.ty(ty))));
        }
        if count < arity {
            values.push("%argument".into());
        }
        if count != 0 {
            apply.instruction("call void @tz.free(ptr %env)");
        }
        let (value, result) = if values.len() == arity {
            let arguments = values
                .iter()
                .zip(&function.signature.parameters)
                .map(|(value, ty)| format!("{} {value}", apply.ty(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            let value = apply.value(format!(
                "call {} @tz.fn.{name}({arguments})",
                apply.ty(&function.signature.result)
            ));
            (value, function.signature.result.clone())
        } else {
            let value = apply.make_closure(id, &values);
            let result = Type::function(
                function.signature.parameters[values.len()..].to_vec(),
                function.signature.result.clone(),
            );
            (value, result)
        };
        apply.instruction(format!("ret {} {value}", apply.ty(&result)));
        let argument = if count < arity {
            format!(
                ", {} %argument",
                apply.ty(&function.signature.parameters[count])
            )
        } else {
            String::new()
        };
        output.push_str(&apply.auxiliary(&format!(
            "{} @tz.apply.{name}.{count}(ptr %env{argument})",
            llvm_type(&result, module)
        )));
    }
    output
}

fn c_type(ty: &Type) -> String {
    match ty {
        Type::Integer(bits, signed) => {
            format!("{}int{}_t", if *signed { "" } else { "u" }, (*bits).max(32))
        }
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Bool => "int32_t".into(),
        Type::Unit => "void".into(),
        _ => unreachable!("the type checker enforces scalar exports"),
    }
}

fn llvm_type(ty: &Type, module: &CheckedModule) -> String {
    match ty {
        Type::Integer(bits, _) | Type::Decimal(bits) | Type::Binary(bits @ (16 | 128)) => {
            format!("i{bits}")
        }
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Binary(_) => unreachable!("binary widths checked"),
        Type::Bool => "i1".into(),
        Type::Unit => "i8".into(),
        Type::String => "%tz.string".into(),
        Type::Record(id, arguments) if arguments.is_empty() => {
            format!("%tz.record.{}", module.records[*id].name)
        }
        Type::Record(..) => format!("%\"tz.record.{}\"", canonical_type(ty, module)),
        Type::Union(..) => format!("%\"tz.union.{}\"", canonical_type(ty, module)),
        Type::Array(_) => "%tz.array".into(),
        Type::List(_) => "%tz.list".into(),
        Type::Tuple(elements) => format!(
            "{{ {} }}",
            elements
                .iter()
                .map(|ty| llvm_type(ty, module))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Function(..) | Type::Task(_) => "%tz.closure".into(),
        Type::Reference(..) => "ptr".into(),
        Type::Variable(_) | Type::Infer(_) => unreachable!("polymorphism is resolved before LLVM"),
    }
}

/// The injective Tsuzuri spelling of a concrete type used in LLVM type names.
fn canonical_type(ty: &Type, module: &CheckedModule) -> String {
    let list = |types: &[Type]| {
        types
            .iter()
            .map(|ty| canonical_type(ty, module))
            .collect::<Vec<_>>()
            .join(",")
    };
    match ty {
        Type::Record(id, arguments) if arguments.is_empty() => module.records[*id].name.clone(),
        Type::Record(id, arguments) => {
            format!("{}[{}]", module.records[*id].name, list(arguments))
        }
        Type::Union(id, arguments) if arguments.is_empty() => module.unions[*id].name.clone(),
        Type::Union(id, arguments) => {
            format!("{}[{}]", module.unions[*id].name, list(arguments))
        }
        Type::Array(element) => format!("array[{}]", canonical_type(element, module)),
        Type::List(element) => format!("list[{}]", canonical_type(element, module)),
        Type::Tuple(elements) => format!("tuple[{}]", list(elements)),
        Type::Function(parameters, result) => format!(
            "fn[{}->{}]",
            list(parameters),
            canonical_type(result, module)
        ),
        Type::Reference(value, false) => format!("ref[{}]", canonical_type(value, module)),
        Type::Reference(value, true) => format!("refmut[{}]", canonical_type(value, module)),
        Type::Task(result) => format!("task[{}]", canonical_type(result, module)),
        Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Bool
        | Type::Unit
        | Type::String => ty.display(&module.types()),
        Type::Variable(_) | Type::Infer(_) => unreachable!("polymorphism is resolved before LLVM"),
    }
}

/// How a union instance stores its tag and payload.
enum UnionLayout {
    /// Every case is nullary, so the value is its `i32` tag.
    Enum,
    /// Every payload has the LLVM type of this one: `{ i32, T }`.
    Common(Type),
    /// Payloads of different LLVM types share `{ i32, [K x i128] }` storage
    /// that holds the largest payload.
    General(usize),
}

/// Size and alignment in bytes of a type's LLVM representation on 64-bit
/// targets with 16-byte `i128`, which bound those of wasm32 and older layouts.
fn storage_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    let aggregate = |fields: &mut dyn Iterator<Item = (usize, usize)>| {
        let (size, align) = fields.fold((0usize, 1usize), |(size, align), (field, field_align)| {
            (
                size.next_multiple_of(field_align) + field,
                align.max(field_align),
            )
        });
        (size.next_multiple_of(align), align)
    };
    match ty {
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            let bytes = usize::from(*bits).div_ceil(8);
            (bytes, bytes)
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::String | Type::Array(_) | Type::List(_) => (16, 8),
        Type::Function(..) | Type::Task(_) => (32, 8),
        Type::Reference(..) => (8, 8),
        Type::Tuple(elements) => {
            aggregate(&mut elements.iter().map(|ty| storage_layout(ty, module)))
        }
        Type::Record(id, arguments) => aggregate(
            &mut module
                .types()
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| storage_layout(ty, module)),
        ),
        Type::Union(id, arguments) => match union_layout(*id, arguments, module) {
            UnionLayout::Enum => (4, 4),
            UnionLayout::Common(payload) => {
                aggregate(&mut [(4, 4), storage_layout(&payload, module)].into_iter())
            }
            UnionLayout::General(count) => (16 + 16 * count, 16),
        },
        Type::Variable(_) | Type::Infer(_) => unreachable!("polymorphism is resolved before LLVM"),
    }
}

fn union_layout(id: usize, arguments: &[Type], module: &CheckedModule) -> UnionLayout {
    let payloads: Vec<_> = module
        .types()
        .union_payloads(id, arguments)
        .into_iter()
        .flatten()
        .collect();
    let Some(first) = payloads.first() else {
        return UnionLayout::Enum;
    };
    let llvm = llvm_type(first, module);
    if payloads.iter().all(|ty| llvm_type(ty, module) == llvm) {
        return UnionLayout::Common(first.clone());
    }
    let bytes = payloads
        .iter()
        .map(|ty| storage_layout(ty, module).0)
        .max()
        .unwrap_or(0);
    UnionLayout::General(bytes.div_ceil(16))
}

/// The functions that the program needs: user functions, exports, and the
/// entry point, and every function they refer to (GUIDE D-22). Unused std
/// functions and the helpers generated for them are left out.
fn reachable_functions(module: &CheckedModule) -> BTreeSet<usize> {
    fn references(expression: &TypedExpr, pending: &mut Vec<usize>) {
        if let TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::Closure(id, _) =
            &expression.kind
        {
            pending.push(*id);
        }
        for child in expression.children() {
            references(child, pending);
        }
    }
    let mut pending: Vec<usize> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(id, function)| {
            (function.origin.module == ModuleOrigin::User && function.origin.test.is_none())
                || function.exported
                || module.entry == Some(*id)
        })
        .map(|(id, _)| id)
        .collect();
    let mut reachable = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if reachable.insert(id) {
            references(&module.functions[id].body, &mut pending);
        }
    }
    reachable
}

/// Whether `emit_target` defines a function: user-origin functions always,
/// and std functions only when reachable.
fn emit_function(id: usize, reachable: &BTreeSet<usize>, module: &CheckedModule) -> bool {
    module.functions[id].origin.module == ModuleOrigin::User || reachable.contains(&id)
}

/// Record and union types, generic instances included, reachable from
/// user-origin type declarations and emitted functions, including those
/// nested in other types' fields and payloads, in deterministic order.
fn named_types(module: &CheckedModule, emitted: &[bool]) -> BTreeSet<Type> {
    fn visit(ty: &Type, pending: &mut Vec<Type>) {
        match ty {
            Type::Record(..) | Type::Union(..) => pending.push(ty.clone()),
            Type::Array(ty) | Type::List(ty) | Type::Task(ty) | Type::Reference(ty, _) => {
                visit(ty, pending)
            }
            Type::Tuple(types) => types.iter().for_each(|ty| visit(ty, pending)),
            Type::Function(parameters, result) => {
                parameters.iter().for_each(|ty| visit(ty, pending));
                visit(result, pending);
            }
            _ => {}
        }
    }
    fn walk(expression: &TypedExpr, pending: &mut Vec<Type>) {
        visit(&expression.ty, pending);
        let mut local = |local: &Local| visit(&local.ty, pending);
        match &expression.kind {
            TypedExprKind::Block { bindings, .. } => {
                bindings.iter().for_each(|(binding, _)| local(binding))
            }
            TypedExprKind::ForRange { local: bound, .. } => local(bound),
            TypedExprKind::ForEach {
                owner,
                local: bound,
                ..
            } => {
                local(owner);
                local(bound);
            }
            TypedExprKind::Match {
                local: subject,
                arms,
                ..
            } => {
                local(subject);
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    for step in &alternative.steps {
                        if let crate::check::PatternStep::Bind(bound, _) = step {
                            local(bound);
                        }
                    }
                    alternative
                        .bindings
                        .iter()
                        .for_each(|(binding, _)| local(binding));
                }
            }
            TypedExprKind::Lambda {
                parameters,
                captures,
                ..
            } => parameters.iter().chain(captures).for_each(local),
            _ => {}
        }
        for child in expression.children() {
            walk(child, pending);
        }
    }
    let mut pending = Vec::new();
    for (id, record) in module.records.iter().enumerate() {
        if record.parameters.is_empty() && record.origin == ModuleOrigin::User {
            for ty in module.types().record_fields(id, &[]) {
                visit(&ty, &mut pending);
            }
        }
    }
    for (id, union) in module.unions.iter().enumerate() {
        if union.parameters.is_empty() && union.origin == ModuleOrigin::User {
            for ty in module.types().union_payloads(id, &[]).into_iter().flatten() {
                visit(&ty, &mut pending);
            }
        }
    }
    for (function, _) in module
        .functions
        .iter()
        .zip(emitted)
        .filter(|(_, emitted)| **emitted)
    {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
            .chain(function.parameters.iter().map(|parameter| &parameter.ty))
        {
            visit(ty, &mut pending);
        }
        walk(&function.body, &mut pending);
    }
    let mut instances = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        let nested: Vec<_> = match &ty {
            Type::Record(id, arguments) => module.types().record_fields(*id, arguments),
            Type::Union(id, arguments) => module
                .types()
                .union_payloads(*id, arguments)
                .into_iter()
                .flatten()
                .collect(),
            _ => unreachable!("only record and union types are pending"),
        };
        if instances.insert(ty) {
            nested.iter().for_each(|ty| visit(ty, &mut pending));
        }
    }
    instances
}

fn abi_type(ty: &Type) -> String {
    match ty {
        Type::Integer(8 | 16 | 32, _) | Type::Bool => "i32".into(),
        Type::Integer(64, _) => "i64".into(),
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Unit => "void".into(),
        _ => unreachable!("the type checker enforces scalar exports"),
    }
}

fn validate_main(module: &CheckedModule) -> Result<(), Diagnostic> {
    let main = module
        .entry
        .map(|id| &module.functions[id])
        .ok_or_else(|| {
            Diagnostic::new(
                "E2004",
                "an executable requires top-level entry-point code or 'fn main' in Main.tz; use '--emit object' for a library",
                Span::default(),
            )
        })?;
    if !main.parameters.is_empty()
        || (!main.signature.result.is_scalar()
            && !matches!(main.signature.result, Type::Unit | Type::String))
    {
        return Err(Diagnostic::new(
            "E2004",
            "the Main.tz entry point must take no arguments and return a number, bool, unit, or string",
            main.span,
        ));
    }
    Ok(())
}

struct FunctionEmitter<'a, 'b> {
    module: &'a CheckedModule,
    function: &'a CheckedFunction,
    function_id: usize,
    symbol: String,
    specializations: &'b mut Specializations,
    known_closures: BTreeMap<usize, ClosureTarget>,
    borrowed_locals: BTreeSet<usize>,
    single_use: BTreeSet<usize>,
    borrowed_worker: bool,
    builtins: &'b mut Builtins,
    intrinsics: &'b mut BTreeSet<String>,
    lines: Vec<String>,
    allocas: Vec<String>,
    locals: BTreeMap<usize, String>,
    next_value: usize,
    next_block: usize,
    block: String,
    back_edges: Vec<(String, Vec<String>)>,
    scopes: Vec<Vec<(String, Type)>>,
    /// Stack parts that each slot and local (including match aliases) may hold.
    frame_slots: BTreeMap<String, Vec<Frame>>,
    frame_locals: BTreeMap<usize, Vec<Frame>>,
    globals: &'b mut Globals,
}

struct BorrowedCall {
    target: ClosureTarget,
    symbol: String,
    captures: Vec<String>,
    cleanup: Vec<(Type, String, Vec<Frame>)>,
}

impl<'a, 'b> FunctionEmitter<'a, 'b> {
    fn new(
        module: &'a CheckedModule,
        function: &'a CheckedFunction,
        function_id: usize,
        builtins: &'b mut Builtins,
        intrinsics: &'b mut BTreeSet<String>,
        globals: &'b mut Globals,
        specializations: &'b mut Specializations,
    ) -> Self {
        Self {
            module,
            function,
            function_id,
            symbol: format!("@tz.fn.{}", function.qualified_name()),
            specializations,
            known_closures: BTreeMap::new(),
            borrowed_locals: BTreeSet::new(),
            single_use: BTreeSet::new(),
            borrowed_worker: false,
            builtins,
            intrinsics,
            globals,
            lines: Vec::new(),
            allocas: Vec::new(),
            locals: BTreeMap::new(),
            next_value: 0,
            next_block: 0,
            block: "entry".into(),
            back_edges: Vec::new(),
            scopes: vec![Vec::new()],
            frame_slots: BTreeMap::new(),
            frame_locals: BTreeMap::new(),
        }
    }

    fn specialized(mut self, id: usize, key: &Specialization) -> Self {
        self.symbol = format!("@tz.specialized.{id}");
        self.borrowed_worker = key.borrowed != 0;
        // The caller retains these immutable payloads; owning temporaries still use normal drops.
        for parameter in &self.function.parameters[..key.borrowed] {
            self.borrowed_locals.insert(parameter.id);
        }
        for (index, target) in &key.callbacks {
            let local = self.function.parameters[*index].id;
            self.borrowed_locals.insert(local);
            self.known_closures.insert(local, *target);
        }
        self
    }

    fn auxiliary(self, signature: &str) -> String {
        let mut output = format!("define internal {signature} nounwind {{\nentry:\n");
        for line in self.allocas {
            let _ = writeln!(output, "  {line}");
        }
        for line in self.lines {
            let _ = writeln!(output, "{line}");
        }
        output.push_str("}\n\n");
        output
    }

    fn emit(mut self) -> String {
        self.single_use = call_specialization::single_use_locals(&self.function.body);
        self.block = "loop".into();
        for (index, parameter) in self.function.parameters.iter().enumerate() {
            self.bind_local(parameter, &format!("%p{index}"));
        }
        self.tail(&self.function.body);
        let parameters = self
            .function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| format!("{} %arg{index}", self.ty(&parameter.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut output = format!(
            "define internal {} {}({parameters}) nounwind {{\nentry:\n",
            self.ty(&self.function.signature.result),
            self.symbol
        );
        for alloca in self.allocas {
            let _ = writeln!(output, "  {alloca}");
        }
        output.push_str("  br label %loop\nloop:\n");
        for (index, parameter) in self.function.parameters.iter().enumerate() {
            let mut incoming = format!("[ %arg{index}, %entry ]");
            for (block, values) in &self.back_edges {
                let _ = write!(incoming, ", [ {}, %{block} ]", values[index]);
            }
            let _ = writeln!(
                output,
                "  %p{index} = phi {} {incoming}",
                llvm_type(&parameter.ty, self.module)
            );
        }
        for line in self.lines {
            output.push_str(&line);
            output.push('\n');
        }
        output.push_str("}\n\n");
        output
    }

    fn ty(&self, ty: &Type) -> String {
        llvm_type(ty, self.module)
    }

    fn fresh(&mut self) -> String {
        let value = format!("%v{}", self.next_value);
        self.next_value += 1;
        value
    }

    fn instruction(&mut self, text: impl Into<String>) {
        self.lines.push(format!("  {}", text.into()));
    }

    fn value(&mut self, instruction: impl Into<String>) -> String {
        let value = self.fresh();
        self.instruction(format!("{value} = {}", instruction.into()));
        value
    }

    fn label(&mut self) -> String {
        let block = format!("b{}", self.next_block);
        self.next_block += 1;
        block
    }

    fn begin(&mut self, block: &str) {
        self.lines.push(format!("{block}:"));
        self.block = block.to_owned();
    }

    fn jump(&mut self, block: &str) {
        self.instruction(format!("br label %{block}"));
    }

    fn branch(&mut self, condition: &str, yes: &str, no: &str) {
        self.instruction(format!("br i1 {condition}, label %{yes}, label %{no}"));
    }

    fn hint_loop(&mut self, body: &TypedExpr) {
        fn uses(expression: &TypedExpr, id: usize) -> bool {
            matches!(expression.kind, TypedExprKind::Local(local) if local == id)
                || expression
                    .children()
                    .into_iter()
                    .any(|child| uses(child, id))
        }
        fn reduction(expression: &TypedExpr, module: &CheckedModule) -> bool {
            if let TypedExprKind::Assign(place, value) = &expression.kind {
                if let (TypedExprKind::Local(id), Some((operator, left, right))) = (
                    &place.kind,
                    call_specialization::binary_operation(value, module),
                ) {
                    if matches!(
                        operator,
                        BinaryOp::Add
                            | BinaryOp::Multiply
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                    ) && value.ty.is_integer()
                        && ((matches!(left.kind, TypedExprKind::Local(local) if local == *id)
                            && !uses(right, *id))
                            || (matches!(right.kind, TypedExprKind::Local(local) if local == *id)
                                && !uses(left, *id)))
                    {
                        return true;
                    }
                }
            }
            expression
                .children()
                .into_iter()
                .any(|child| reduction(child, module))
        }
        fn assignments(expression: &TypedExpr) -> usize {
            usize::from(matches!(expression.kind, TypedExprKind::Assign(..)))
                + expression
                    .children()
                    .into_iter()
                    .map(assignments)
                    .sum::<usize>()
        }
        fn small(expression: &TypedExpr, remaining: &mut usize) -> bool {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            expression
                .children()
                .into_iter()
                .all(|child| small(child, remaining))
        }
        if !small(body, &mut 64) || assignments(body) != 1 || !reduction(body, self.module) {
            return;
        }
        let id = self.globals.next_metadata;
        self.globals.next_metadata += 2;
        self.globals.definitions.push(format!(
            "!{id} = distinct !{{!{id}, !{}}}\n!{} = !{{!\"llvm.loop.unroll.enable\"}}",
            id + 1,
            id + 1
        ));
        let branch = self.lines.last_mut().expect("loop back edge");
        debug_assert!(branch.trim_start().starts_with("br "));
        let _ = write!(branch, ", !llvm.loop !{id}");
    }

    fn guard(&mut self, valid: &str) {
        let success = self.label();
        let failure = self.label();
        self.branch(valid, &success, &failure);
        self.begin(&failure);
        self.instruction("call void @llvm.trap()");
        self.instruction("unreachable");
        self.begin(&success);
    }

    fn slot(&mut self, ty: &Type) -> String {
        let slot = self.fresh();
        self.allocas
            .push(format!("{slot} = alloca {}, align 16", self.ty(ty)));
        slot
    }

    fn bind_local(&mut self, local: &Local, value: &str) {
        let slot = self.slot(&local.ty);
        self.instruction(format!("store {} {value}, ptr {slot}", self.ty(&local.ty)));
        self.locals.insert(local.id, slot.clone());
        if local.ty.needs_drop(&self.module.types()) && !self.borrowed_locals.contains(&local.id) {
            self.scopes
                .last_mut()
                .unwrap()
                .push((slot, local.ty.clone()));
        }
    }

    fn bind(&mut self, bindings: &[(Local, TypedExpr)]) {
        for (local, expression) in bindings {
            let target = (!local.mutable)
                .then(|| call_specialization::target(expression, &self.known_closures, self.module))
                .flatten();
            let (value, frames) = self.frame_value(expression);
            self.bind_local(local, &value);
            self.bind_frames(local, frames);
            if let Some(target) = target {
                self.known_closures.insert(local.id, target);
            }
        }
    }

    fn is_self(&self, callee: &TypedExpr) -> bool {
        matches!(callee.kind, TypedExprKind::Function(FunctionRef::User(id)) if id == self.function_id)
            && !self.borrowed_worker
            && !self
                .function
                .signature
                .parameters
                .iter()
                .any(|ty| ty.carries_loans(&self.module.types()))
    }

    fn tail_arguments(&mut self, arguments: &[TypedExpr]) -> Vec<String> {
        if self.globals.wasm {
            return arguments
                .iter()
                .map(|argument| self.expression(argument))
                .collect();
        }
        enum Argument {
            Value(String),
            Arithmetic(String),
        }
        let mut pending = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let operation = call_specialization::binary_operation(argument, self.module).filter(
                |(operator, _, _)| {
                    argument.ty.is_integer()
                        && matches!(operator, BinaryOp::Add | BinaryOp::Subtract)
                },
            );
            if let Some((operator, left, right)) = operation {
                // Snapshot operands now; only the nontrapping wrapping operation moves to the latch.
                let left = self.expression(left);
                let right = self.expression(right);
                let opcode = if operator == BinaryOp::Add {
                    "add"
                } else {
                    "sub"
                };
                pending.push(Argument::Arithmetic(format!(
                    "{opcode} {} {left}, {right}",
                    self.ty(&argument.ty)
                )));
            } else {
                pending.push(Argument::Value(self.expression(argument)));
            }
        }
        pending
            .into_iter()
            .map(|argument| match argument {
                Argument::Value(value) => value,
                Argument::Arithmetic(instruction) => self.value(instruction),
            })
            .collect()
    }

    fn tail(&mut self, expression: &TypedExpr) {
        match &expression.kind {
            TypedExprKind::Match { local, value, arms } => {
                self.match_expression(local, value, arms, &expression.ty, true, None);
            }
            TypedExprKind::Block { bindings, result } => {
                self.scopes.push(Vec::new());
                self.bind(bindings);
                self.tail(result);
                self.scopes.pop();
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition);
                let yes = self.label();
                let no = self.label();
                self.branch(&condition, &yes, &no);
                self.begin(&yes);
                self.tail(then_branch);
                self.begin(&no);
                self.tail(else_branch);
            }
            TypedExprKind::Call(callee, arguments)
                if self.is_self(callee) && arguments.len() == self.function.parameters.len() =>
            {
                let values = self.tail_arguments(arguments);
                self.drop_all();
                self.back_edges.push((self.block.clone(), values));
                self.jump("loop");
                self.hint_loop(&self.function.body);
            }
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee)
                if self.is_self(callee) && self.function.parameters.len() == 1 =>
            {
                let values = self.tail_arguments(std::slice::from_ref(argument.as_ref()));
                self.drop_all();
                self.back_edges.push((self.block.clone(), values));
                self.jump("loop");
                self.hint_loop(&self.function.body);
            }
            _ => {
                let value = self.expression(expression);
                self.drop_all();
                self.instruction(format!("ret {} {value}", self.ty(&expression.ty)));
            }
        }
    }

    fn expression(&mut self, expression: &TypedExpr) -> String {
        self.expression_mode(expression, true)
    }

    /// Reads a place. A taken value is cloned (Copy) or moved; a moved value leaves this frame's
    /// storage unless `relocate` is false and the caller destroys it immediately.
    fn read_place(&mut self, expression: &TypedExpr, take: bool, relocate: bool) -> String {
        let slot = self.place(expression);
        let value = self.value(format!("load {}, ptr {slot}", self.ty(&expression.ty)));
        if take && expression.ty.needs_drop(&self.module.types()) {
            if self.clones_on_take(expression) {
                return self.clone_value(&expression.ty, &value);
            }
            self.instruction(format!(
                "store {} zeroinitializer, ptr {slot}",
                self.ty(&expression.ty)
            ));
            if relocate {
                let frames = self.frame_of_place(expression);
                return self.relocate(&expression.ty, &value, &frames);
            }
        }
        value
    }

    /// Taking a Copy place copies it, except at the only use of an owned local.
    fn clones_on_take(&self, expression: &TypedExpr) -> bool {
        let last_use = matches!(expression.kind, TypedExprKind::Local(id)
            if self.single_use.contains(&id) && !self.borrowed_locals.contains(&id));
        expression.ty.is_copy(&self.module.types()) && !last_use
    }

    fn string_constant(&mut self, text: &str) -> String {
        let name = format!("@tz.literal.{}", self.globals.definitions.len());
        let escaped = text
            .bytes()
            .map(|byte| format!("\\{byte:02X}"))
            .collect::<String>();
        self.globals.definitions.push(format!(
            "{name} = private unnamed_addr constant [{} x i8] c\"{escaped}\"",
            text.len()
        ));
        name
    }

    fn expression_mode(&mut self, expression: &TypedExpr, take: bool) -> String {
        if Self::is_place(expression) {
            return self.read_place(expression, take, true);
        }
        match &expression.kind {
            TypedExprKind::Int(value) => value.to_string(),
            TypedExprKind::GenericFunction(..)
            | TypedExprKind::Method(..)
            | TypedExprKind::GenericInteger(..)
            | TypedExprKind::GenericFloat(_)
            | TypedExprKind::Lambda { .. }
            | TypedExprKind::CaseConstructor { .. } => {
                unreachable!("polymorphism is resolved before LLVM")
            }
            TypedExprKind::Construct {
                case_id, payload, ..
            } => self.construct(&expression.ty, *case_id, payload.as_deref()),
            TypedExprKind::UnionTag(value) => self.union_tag(value),
            TypedExprKind::UnionPayload { value, .. } => {
                let union = self.expression(value);
                // The remainder of a union is its tag, which owns nothing.
                self.payload_value(&value.ty, &union, &expression.ty)
            }
            TypedExprKind::Float(value) => value.clone(),
            TypedExprKind::Bool(value) => if *value { "1" } else { "0" }.into(),
            TypedExprKind::Unit => "0".into(),
            TypedExprKind::While { condition, body } => {
                self.while_loop(condition, body);
                "0".into()
            }
            TypedExprKind::ForRange {
                local,
                start,
                step,
                finish,
                body,
            } => {
                self.range_loop(local, start, step, finish, body);
                "0".into()
            }
            TypedExprKind::ForEach {
                local,
                source,
                body,
                ..
            } => {
                self.for_each(local, source, body);
                "0".into()
            }
            TypedExprKind::Match { local, value, arms } => {
                self.match_expression(local, value, arms, &expression.ty, false, None)
            }
            TypedExprKind::String(text) => {
                let name = self.string_constant(text);
                self.value(format!(
                    "call %tz.string @tz.string.new(ptr {name}, i64 {})",
                    text.len()
                ))
            }
            TypedExprKind::Local(_)
            | TypedExprKind::Dereference(_)
            | TypedExprKind::ListTail(..) => {
                unreachable!("places handled above")
            }
            TypedExprKind::Borrow(value, mutable) => {
                let slot = self.place(value);
                let frames = self.frame_of_place(value);
                if *mutable && !frames.is_empty() {
                    // The borrower may replace and drop the value, so it must own heap storage.
                    let ty = self.ty(&value.ty);
                    let current = self.value(format!("load {ty}, ptr {slot}"));
                    let moved = self.relocate(&value.ty, &current, &frames);
                    self.instruction(format!("store {ty} {moved}, ptr {slot}"));
                }
                slot
            }
            TypedExprKind::Assign(place, value) => {
                let value = self.expression(value);
                let slot = self.place(place);
                self.drop_slot(&slot, &place.ty);
                self.instruction(format!("store {} {value}, ptr {slot}", self.ty(&place.ty)));
                "0".into()
            }
            TypedExprKind::Cast(value) => self.cast(value, &expression.ty),
            TypedExprKind::Function(reference) => match reference {
                FunctionRef::User(id) => self.make_closure(*id, &[]),
                FunctionRef::Builtin(_) => unreachable!("builtin values are lifted before LLVM"),
            },
            TypedExprKind::Closure(id, captures) => {
                let values: Vec<_> = captures
                    .iter()
                    .map(|capture| self.expression(capture))
                    .collect();
                self.make_closure(*id, &values)
            }
            TypedExprKind::Unary(operator, operand) => {
                let value = self.expression(operand);
                let ty = self.ty(&operand.ty);
                self.value(match operator {
                    UnaryOp::Negate if matches!(operand.ty, Type::Binary(32 | 64)) => {
                        format!("fneg {ty} {value}")
                    }
                    UnaryOp::Negate if operand.ty.is_float() => {
                        let bits = match operand.ty {
                            Type::Binary(bits) | Type::Decimal(bits) => bits,
                            _ => unreachable!(),
                        };
                        format!("xor {ty} {value}, {}", 1u128 << (bits - 1))
                    }
                    UnaryOp::Negate => format!("sub {ty} 0, {value}"),
                    UnaryOp::Not => format!("xor i1 {value}, 1"),
                    UnaryOp::BitNot => format!("xor {ty} {value}, -1"),
                })
            }
            TypedExprKind::Binary(BinaryOp::And | BinaryOp::Or, left, right) => {
                let TypedExprKind::Binary(operator, _, _) = expression.kind else {
                    unreachable!()
                };
                self.short_circuit(operator, left, right)
            }
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee) => {
                if matches!(callee.kind, TypedExprKind::Function(_)) {
                    return self.call(callee, std::slice::from_ref(argument.as_ref()));
                }
                let value = self.expression(argument);
                if let Some(call) = self.prepare_known_call(callee, 1) {
                    let result = self.emit_borrowed_call(&call, &[value]);
                    self.finish_borrowed_call(&call);
                    return result;
                }
                let function = self.expression(callee);
                self.apply_value(&function, &callee.ty, Some((&argument.ty, &value)))
                    .0
            }
            TypedExprKind::Binary(operator, left, right) => self.binary(*operator, left, right),
            TypedExprKind::Call(callee, arguments) => self.call(callee, arguments),
            TypedExprKind::TaskRun(task) => {
                let task = self.expression(task);
                let code = self.value(format!("extractvalue %tz.closure {task}, 0"));
                let environment = self.value(format!("extractvalue %tz.closure {task}, 1"));
                self.value(format!(
                    "call {} {code}(ptr {environment})",
                    self.ty(&expression.ty)
                ))
            }
            TypedExprKind::TaskParallel(tasks) => self.parallel_tasks(tasks, &expression.ty),
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition);
                let yes = self.label();
                let no = self.label();
                let merge = self.label();
                self.branch(&condition, &yes, &no);
                self.begin(&yes);
                let then_value = self.expression(then_branch);
                let then_end = self.block.clone();
                self.jump(&merge);
                self.begin(&no);
                let else_value = self.expression(else_branch);
                let else_end = self.block.clone();
                self.jump(&merge);
                self.begin(&merge);
                self.value(format!(
                    "phi {} [ {then_value}, %{then_end} ], [ {else_value}, %{else_end} ]",
                    self.ty(&expression.ty),
                ))
            }
            TypedExprKind::Block { bindings, result } => {
                self.scopes.push(Vec::new());
                self.bind(bindings);
                let value = self.expression(result);
                let scope = self.scopes.pop().unwrap();
                self.drop_scope(&scope);
                value
            }
            TypedExprKind::Record(fields) => {
                let mut record = "zeroinitializer".into();
                for (index, field) in fields {
                    let value = self.expression(field);
                    record = self.value(format!(
                        "insertvalue {} {record}, {} {value}, {index}",
                        self.ty(&expression.ty),
                        self.ty(&field.ty)
                    ));
                }
                record
            }
            TypedExprKind::Tuple(elements) => {
                let mut tuple = "zeroinitializer".into();
                for (index, element) in elements.iter().enumerate() {
                    let value = self.expression(element);
                    tuple = self.value(format!(
                        "insertvalue {} {tuple}, {} {value}, {index}",
                        self.ty(&expression.ty),
                        self.ty(&element.ty)
                    ));
                }
                tuple
            }
            TypedExprKind::Array(elements) | TypedExprKind::List(elements)
                if elements.is_empty() =>
            {
                // An empty literal owns no storage; dropping its null buffer is a no-op.
                "zeroinitializer".into()
            }
            TypedExprKind::Array(_) | TypedExprKind::List(_) => self.heap_collection(expression),
            TypedExprKind::NewLiteral(literal) => self.heap_collection(literal),
            TypedExprKind::NewArray(length, initializer) => {
                let length = self.expression(length);
                let direct = self.prepare_known_call(initializer, 1);
                let callee = direct.is_none().then(|| self.expression(initializer));
                let Type::Array(element) = &expression.ty else {
                    unreachable!()
                };
                let (array, data) = self.allocate_array(element, &length);
                self.array_loop(&length, |emitter, index| {
                    let value = if let Some(call) = &direct {
                        emitter.emit_borrowed_call(call, &[index.to_owned()])
                    } else {
                        let function =
                            emitter.clone_value(&initializer.ty, callee.as_ref().unwrap());
                        emitter
                            .apply_value(&function, &initializer.ty, Some((&Type::I64, index)))
                            .0
                    };
                    let pointer = emitter.element_pointer(element, &data, index);
                    emitter.instruction(format!(
                        "store {} {value}, ptr {pointer}",
                        emitter.ty(element)
                    ));
                });
                if let Some(call) = &direct {
                    self.finish_borrowed_call(call);
                } else {
                    self.drop_value(&initializer.ty, callee.as_ref().unwrap());
                }
                array
            }
            TypedExprKind::NewList(length, initializer) => {
                let length = self.expression(length);
                let direct = self.prepare_known_call(initializer, 1);
                let callee = direct.is_none().then(|| self.expression(initializer));
                let Type::List(element) = &expression.ty else {
                    unreachable!()
                };
                self.allocation_size(&self.list_node_type(element), &length);
                let (head, tail) = self.list_builder();
                self.array_loop(&length, |emitter, index| {
                    let value = if let Some(call) = &direct {
                        emitter.emit_borrowed_call(call, &[index.to_owned()])
                    } else {
                        let function =
                            emitter.clone_value(&initializer.ty, callee.as_ref().unwrap());
                        emitter
                            .apply_value(&function, &initializer.ty, Some((&Type::I64, index)))
                            .0
                    };
                    emitter.append_list(element, &tail, &value);
                });
                if let Some(call) = &direct {
                    self.finish_borrowed_call(call);
                } else {
                    self.drop_value(&initializer.ty, callee.as_ref().unwrap());
                }
                self.finish_list(&head, &length)
            }
            TypedExprKind::Field(record, index) => {
                let value = self.expression(record);
                let field = self.value(format!(
                    "extractvalue {} {value}, {index}",
                    self.ty(&record.ty)
                ));
                if record.ty.needs_drop(&self.module.types()) {
                    let remainder = self.value(format!(
                        "insertvalue {} {value}, {} zeroinitializer, {index}",
                        self.ty(&record.ty),
                        self.ty(&expression.ty)
                    ));
                    self.drop_value(&record.ty, &remainder);
                }
                field
            }
            TypedExprKind::Index(string, index) if string.ty == Type::String => {
                let (value, frames) = self.read_operand(string);
                let index = self.expression(index);
                let data = self.value(format!("extractvalue %tz.string {value}, 0"));
                let length = self.value(format!("extractvalue %tz.string {value}, 1"));
                let valid = self.value(format!("icmp ult i64 {index}, {length}"));
                self.guard(&valid);
                let pointer = self.value(format!(
                    "getelementptr inbounds i8, ptr {data}, i64 {index}"
                ));
                let byte = self.value(format!("load i8, ptr {pointer}"));
                self.release_operand(string, &value, &frames);
                byte
            }
            TypedExprKind::Index(array, index) => {
                let (value, frames) = self.read_operand(array);
                let index = self.expression(index);
                let (Type::Array(element) | Type::List(element)) = &array.ty else {
                    unreachable!()
                };
                let pointer = self.checked_element_pointer(&array.ty, &value, &index);
                let extracted = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
                let result = self.clone_value(element, &extracted);
                self.release_operand(array, &value, &frames);
                result
            }
            TypedExprKind::Length(array) => {
                let (value, frames) = self.read_operand(array);
                let length = self.value(format!("extractvalue {} {value}, 1", self.ty(&array.ty)));
                self.release_operand(array, &value, &frames);
                length
            }
            TypedExprKind::StringLength(string) => {
                let (value, frames) = self.read_operand(string);
                let length = self.value(format!("extractvalue %tz.string {value}, 1"));
                self.release_operand(string, &value, &frames);
                length
            }
        }
    }

    /// Builds an array or list literal on the heap, as for `new [...]` and escaping temporaries.
    fn heap_collection(&mut self, expression: &TypedExpr) -> String {
        match (&expression.kind, &expression.ty) {
            (TypedExprKind::Array(elements), Type::Array(element)) => {
                let (array, data) = self.allocate_array(element, &elements.len().to_string());
                for (index, value) in elements.iter().enumerate() {
                    let value = self.expression(value);
                    let pointer = self.element_pointer(element, &data, &index.to_string());
                    self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
                }
                array
            }
            (TypedExprKind::List(elements), Type::List(element)) => {
                let (head, tail) = self.list_builder();
                for value in elements {
                    let value = self.expression(value);
                    self.append_list(element, &tail, &value);
                }
                self.finish_list(&head, &elements.len().to_string())
            }
            _ => unreachable!("'new' literals are array or list literals"),
        }
    }

    fn is_place(expression: &TypedExpr) -> bool {
        match &expression.kind {
            TypedExprKind::Local(_) | TypedExprKind::Dereference(_) => true,
            TypedExprKind::Field(value, _)
            | TypedExprKind::ListTail(value, _)
            | TypedExprKind::UnionPayload { value, .. } => Self::is_place(value),
            TypedExprKind::Index(value, _) => {
                matches!(value.ty, Type::Array(_) | Type::List(_)) && Self::is_place(value)
            }
            _ => false,
        }
    }

    fn place(&mut self, expression: &TypedExpr) -> String {
        match &expression.kind {
            TypedExprKind::Local(id) => self.locals[id].clone(),
            TypedExprKind::Dereference(value) => self.expression_mode(value, false),
            TypedExprKind::Field(value, index) => {
                let slot = self.place(value);
                self.value(format!(
                    "getelementptr inbounds {}, ptr {slot}, i32 0, i32 {index}",
                    self.ty(&value.ty)
                ))
            }
            TypedExprKind::UnionPayload { value, .. } => {
                let slot = self.place(value);
                self.payload_pointer(&value.ty, &slot)
            }
            TypedExprKind::ListTail(value, count) => {
                let slot = self.place(value);
                let list = self.value(format!("load %tz.list, ptr {slot}"));
                let length = self.value(format!("extractvalue %tz.list {list}, 1"));
                let valid = self.value(format!("icmp uge i64 {length}, {count}"));
                self.guard(&valid);
                let head = self.value(format!("extractvalue %tz.list {list}, 0"));
                let tail = self.list_loop(&head, &count.to_string(), |_, _| {});
                let length = self.value(format!("sub i64 {length}, {count}"));
                let descriptor = self.value(format!(
                    "insertvalue %tz.list zeroinitializer, ptr {tail}, 0"
                ));
                let descriptor = self.value(format!(
                    "insertvalue %tz.list {descriptor}, i64 {length}, 1"
                ));
                let slot = self.slot(&value.ty);
                self.instruction(format!("store %tz.list {descriptor}, ptr {slot}"));
                slot
            }
            TypedExprKind::Index(value, index) => {
                let slot = self.place(value);
                let array = self.value(format!("load {}, ptr {slot}", self.ty(&value.ty)));
                let index = self.expression(index);
                self.checked_element_pointer(&value.ty, &array, &index)
            }
            _ => unreachable!("borrow checker requires an addressable place"),
        }
    }

    fn drop_value(&mut self, ty: &Type, value: &str) {
        match ty {
            Type::Function(..) | Type::Task(_) => {
                self.instruction(format!("call void @tz.closure.drop(%tz.closure {value})"))
            }
            Type::String => {
                let pointer = self.value(format!("extractvalue %tz.string {value}, 0"));
                self.instruction(format!("call void @tz.free(ptr {pointer})"));
            }
            Type::Record(id, arguments) => {
                let fields = self.module.types().record_fields(*id, arguments);
                for (index, field) in fields.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        self.drop_value(field, &extracted);
                    }
                }
            }
            Type::Tuple(elements) => {
                for (index, field) in elements.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        self.drop_value(field, &extracted);
                    }
                }
            }
            Type::Union(..) => {
                let cases = self.owning_cases(ty);
                if cases.is_empty() {
                    return;
                }
                let spilled = matches!(self.union_layout(ty), UnionLayout::General(_))
                    .then(|| self.spill(ty, value));
                let (labels, done) = self.case_switch(ty, value, &cases);
                for ((_, payload), label) in cases.into_iter().zip(labels) {
                    self.begin(&label);
                    let extracted = match &spilled {
                        Some(slot) => {
                            let pointer = self.payload_pointer(ty, slot);
                            self.value(format!("load {}, ptr {pointer}", self.ty(&payload)))
                        }
                        None => self.value(format!("extractvalue {} {value}, 1", self.ty(ty))),
                    };
                    self.drop_value(&payload, &extracted);
                    self.jump(&done);
                }
                self.begin(&done);
            }
            Type::Array(element) => {
                let data = self.value(format!("extractvalue %tz.array {value}, 0"));
                if element.needs_drop(&self.module.types()) {
                    let length = self.value(format!("extractvalue %tz.array {value}, 1"));
                    self.array_loop(&length, |emitter, index| {
                        let pointer = emitter.element_pointer(element, &data, index);
                        let extracted =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        emitter.drop_value(element, &extracted);
                    });
                }
                self.instruction(format!("call void @tz.free(ptr {data})"));
            }
            Type::List(element) => {
                let head = self.value(format!("extractvalue %tz.list {value}, 0"));
                let length = self.value(format!("extractvalue %tz.list {value}, 1"));
                self.list_loop(&head, &length, |emitter, node| {
                    if element.needs_drop(&emitter.module.types()) {
                        let pointer = emitter.list_element_pointer(element, node);
                        let value =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        emitter.drop_value(element, &value);
                    }
                    emitter.instruction(format!("call void @tz.free(ptr {node})"));
                });
            }
            _ => {}
        }
    }
    fn clone_value(&mut self, ty: &Type, value: &str) -> String {
        match ty {
            Type::Task(_) => unreachable!("single-use tasks cannot be cloned"),
            Type::String => {
                let pointer = self.value(format!("extractvalue %tz.string {value}, 0"));
                let length = self.value(format!("extractvalue %tz.string {value}, 1"));
                self.value(format!(
                    "call %tz.string @tz.string.new(ptr {pointer}, i64 {length})"
                ))
            }
            Type::Function(..) => self.value(format!(
                "call %tz.closure @tz.closure.clone(%tz.closure {value})"
            )),
            Type::Record(id, arguments) => {
                let mut result = value.to_owned();
                let fields = self.module.types().record_fields(*id, arguments);
                for (index, field) in fields.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let field_value =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        let copy = self.clone_value(field, &field_value);
                        result = self.value(format!(
                            "insertvalue {} {result}, {} {copy}, {index}",
                            self.ty(ty),
                            self.ty(field)
                        ));
                    }
                }
                result
            }
            Type::Tuple(elements) => {
                let mut result = value.to_owned();
                for (index, field) in elements.iter().enumerate() {
                    if field.needs_drop(&self.module.types()) {
                        let extracted =
                            self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
                        let copy = self.clone_value(field, &extracted);
                        result = self.value(format!(
                            "insertvalue {} {result}, {} {copy}, {index}",
                            self.ty(ty),
                            self.ty(field)
                        ));
                    }
                }
                result
            }
            Type::Union(..) => {
                let cases = self.owning_cases(ty);
                if cases.is_empty() {
                    return value.to_owned();
                }
                // The copy starts as the original and receives a cloned payload in place.
                let slot = self.spill(ty, value);
                let (labels, done) = self.case_switch(ty, value, &cases);
                for ((_, payload), label) in cases.into_iter().zip(labels) {
                    self.begin(&label);
                    let pointer = self.payload_pointer(ty, &slot);
                    let llvm = self.ty(&payload);
                    let original = self.value(format!("load {llvm}, ptr {pointer}"));
                    let copy = self.clone_value(&payload, &original);
                    self.instruction(format!("store {llvm} {copy}, ptr {pointer}"));
                    self.jump(&done);
                }
                self.begin(&done);
                self.value(format!("load {}, ptr {slot}", self.ty(ty)))
            }
            Type::Array(element) => {
                let data = self.value(format!("extractvalue %tz.array {value}, 0"));
                let length = self.value(format!("extractvalue %tz.array {value}, 1"));
                let (result, target) = self.allocate_array(element, &length);
                self.array_loop(&length, |emitter, index| {
                    let source = emitter.element_pointer(element, &data, index);
                    let element_value =
                        emitter.value(format!("load {}, ptr {source}", emitter.ty(element)));
                    let copy = emitter.clone_value(element, &element_value);
                    let destination = emitter.element_pointer(element, &target, index);
                    emitter.instruction(format!(
                        "store {} {copy}, ptr {destination}",
                        emitter.ty(element)
                    ));
                });
                result
            }
            Type::List(element) => {
                let source = self.value(format!("extractvalue %tz.list {value}, 0"));
                let length = self.value(format!("extractvalue %tz.list {value}, 1"));
                let (head, tail) = self.list_builder();
                self.list_loop(&source, &length, |emitter, node| {
                    let pointer = emitter.list_element_pointer(element, node);
                    let value =
                        emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                    let copy = emitter.clone_value(element, &value);
                    emitter.append_list(element, &tail, &copy);
                });
                self.finish_list(&head, &length)
            }
            _ => value.to_owned(),
        }
    }

    fn union_layout(&self, ty: &Type) -> UnionLayout {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union layouts belong to union types")
        };
        union_layout(*id, arguments, self.module)
    }

    /// The payload storage of the union stored at `slot`, typed by each case's load or store.
    fn payload_pointer(&mut self, ty: &Type, slot: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {slot}, i32 0, i32 1",
            self.ty(ty)
        ))
    }

    fn construct(&mut self, ty: &Type, case_id: usize, payload: Option<&TypedExpr>) -> String {
        let layout = self.union_layout(ty);
        if matches!(layout, UnionLayout::Enum) {
            return case_id.to_string();
        }
        let payload = payload.map(|payload| (self.expression(payload), self.ty(&payload.ty)));
        let llvm = self.ty(ty);
        let tagged = self.value(format!(
            "insertvalue {llvm} zeroinitializer, i32 {case_id}, 0"
        ));
        let Some((value, payload_type)) = payload else {
            return tagged;
        };
        if let UnionLayout::Common(_) = layout {
            return self.value(format!(
                "insertvalue {llvm} {tagged}, {payload_type} {value}, 1"
            ));
        }
        let slot = self.spill(ty, &tagged);
        let pointer = self.payload_pointer(ty, &slot);
        self.instruction(format!("store {payload_type} {value}, ptr {pointer}"));
        self.value(format!("load {llvm}, ptr {slot}"))
    }

    fn union_tag(&mut self, value: &TypedExpr) -> String {
        if Self::is_place(value) {
            // The tag is the first field, so it is also the value of an enum-like union.
            let slot = self.place(value);
            return self.value(format!("load i32, ptr {slot}"));
        }
        let (union, frames) = self.read_operand(value);
        let tag = if matches!(self.union_layout(&value.ty), UnionLayout::Enum) {
            union.clone()
        } else {
            self.value(format!("extractvalue {} {union}, 0", self.ty(&value.ty)))
        };
        self.release_operand(value, &union, &frames);
        tag
    }

    fn payload_value(&mut self, ty: &Type, union: &str, payload: &Type) -> String {
        match self.union_layout(ty) {
            UnionLayout::Enum => unreachable!("nullary cases have no payload"),
            UnionLayout::Common(_) => {
                self.value(format!("extractvalue {} {union}, 1", self.ty(ty)))
            }
            UnionLayout::General(_) => {
                let slot = self.spill(ty, union);
                let pointer = self.payload_pointer(ty, &slot);
                self.value(format!("load {}, ptr {pointer}", self.ty(payload)))
            }
        }
    }

    /// Cases whose payload owns resources, by tag.
    fn owning_cases(&self, ty: &Type) -> Vec<(usize, Type)> {
        let Type::Union(id, arguments) = ty else {
            unreachable!("union cases belong to union types")
        };
        let types = self.module.types();
        types
            .union_payloads(*id, arguments)
            .into_iter()
            .enumerate()
            .filter_map(|(case, payload)| Some((case, payload.filter(|ty| ty.needs_drop(&types))?)))
            .collect()
    }

    /// Branches on the tag of `value` to one new block per case; other tags reach the returned
    /// join block.
    fn case_switch(
        &mut self,
        ty: &Type,
        value: &str,
        cases: &[(usize, Type)],
    ) -> (Vec<String>, String) {
        let tag = self.value(format!("extractvalue {} {value}, 0", self.ty(ty)));
        let labels: Vec<_> = cases.iter().map(|_| self.label()).collect();
        let done = self.label();
        self.instruction(format!("switch i32 {tag}, label %{done} ["));
        for ((case, _), label) in cases.iter().zip(&labels) {
            self.instruction(format!("  i32 {case}, label %{label}"));
        }
        self.instruction("]");
        (labels, done)
    }

    fn allocate_array(&mut self, element: &Type, length: &str) -> (String, String) {
        let bytes = self.allocation_size(&self.ty(element), length);
        let empty = self.value(format!("icmp eq i64 {length}, 0"));
        let bytes = self.value(format!("select i1 {empty}, i64 1, i64 {bytes}"));
        let data = self.value(format!("call ptr @tz.alloc(i64 {bytes})"));
        let array = self.value(format!(
            "insertvalue %tz.array zeroinitializer, ptr {data}, 0"
        ));
        let array = self.value(format!("insertvalue %tz.array {array}, i64 {length}, 1"));
        (array, data)
    }

    fn allocation_size(&mut self, ty: &str, length: &str) -> String {
        let size = format!("ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64)");
        let zero = self.value(format!("icmp eq i64 {size}, 0"));
        let stride = self.value(format!("select i1 {zero}, i64 1, i64 {size}"));
        let limit = self.value(format!("udiv i64 {}, {stride}", i64::MAX));
        // Unsigned comparison rejects negative lengths as well as byte-size overflow.
        let valid = self.value(format!("icmp ule i64 {length}, {limit}"));
        self.guard(&valid);
        self.value(format!("mul i64 {length}, {stride}"))
    }

    fn element_pointer(&mut self, element: &Type, data: &str, index: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {data}, i64 {index}",
            self.ty(element)
        ))
    }

    fn checked_element_pointer(&mut self, ty: &Type, collection: &str, index: &str) -> String {
        let length = self.value(format!("extractvalue {} {collection}, 1", self.ty(ty)));
        let valid = self.value(format!("icmp ult i64 {index}, {length}"));
        self.guard(&valid);
        let data = self.value(format!("extractvalue {} {collection}, 0", self.ty(ty)));
        match ty {
            Type::Array(element) => self.element_pointer(element, &data, index),
            Type::List(element) => {
                let node = self.list_loop(&data, index, |_, _| {});
                self.list_element_pointer(element, &node)
            }
            _ => unreachable!("indexing requires a collection"),
        }
    }

    fn list_node_type(&self, element: &Type) -> String {
        format!("{{ ptr, {} }}", self.ty(element))
    }

    fn list_element_pointer(&mut self, element: &Type, node: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {node}, i32 0, i32 1",
            self.list_node_type(element)
        ))
    }

    fn list_builder(&mut self) -> (String, String) {
        let pointer = Type::Reference(Box::new(Type::Unit), false);
        let head = self.slot(&pointer);
        let tail = self.slot(&pointer);
        self.instruction(format!("store ptr null, ptr {head}"));
        self.instruction(format!("store ptr {head}, ptr {tail}"));
        (head, tail)
    }

    fn append_list(&mut self, element: &Type, tail: &str, value: &str) {
        let node = self.value(format!(
            "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
            self.list_node_type(element)
        ));
        self.instruction(format!("store ptr null, ptr {node}"));
        let pointer = self.list_element_pointer(element, &node);
        self.instruction(format!("store {} {value}, ptr {pointer}", self.ty(element)));
        let previous = self.value(format!("load ptr, ptr {tail}"));
        self.instruction(format!("store ptr {node}, ptr {previous}"));
        // The first field of each node is the next pointer, also used as the builder's tail slot.
        self.instruction(format!("store ptr {node}, ptr {tail}"));
    }

    fn finish_list(&mut self, head: &str, length: &str) -> String {
        let head = self.value(format!("load ptr, ptr {head}"));
        let list = self.value(format!(
            "insertvalue %tz.list zeroinitializer, ptr {head}, 0"
        ));
        self.value(format!("insertvalue %tz.list {list}, i64 {length}, 1"))
    }

    fn list_loop(
        &mut self,
        head: &str,
        length: &str,
        body: impl FnOnce(&mut Self, &str),
    ) -> String {
        let entry = self.block.clone();
        let condition = self.label();
        let element = self.label();
        let advance = self.label();
        let exit = self.label();
        let index = self.fresh();
        let next_index = self.fresh();
        let node = self.fresh();
        let next_node = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{index} = phi i64 [ 0, %{entry} ], [ {next_index}, %{advance} ]"
        ));
        self.instruction(format!(
            "{node} = phi ptr [ {head}, %{entry} ], [ {next_node}, %{advance} ]"
        ));
        let more = self.value(format!("icmp ult i64 {index}, {length}"));
        self.branch(&more, &element, &exit);
        self.begin(&element);
        // Read the link before the callback, which may free this node.
        self.instruction(format!("{next_node} = load ptr, ptr {node}"));
        body(self, &node);
        self.jump(&advance);
        self.begin(&advance);
        self.instruction(format!("{next_index} = add i64 {index}, 1"));
        self.jump(&condition);
        self.begin(&exit);
        node
    }

    fn array_loop(&mut self, length: &str, body: impl FnOnce(&mut Self, &str)) {
        let entry = self.block.clone();
        let condition = self.label();
        let element = self.label();
        let advance = self.label();
        let exit = self.label();
        let index = self.fresh();
        let next = self.fresh();
        self.jump(&condition);
        self.begin(&condition);
        self.instruction(format!(
            "{index} = phi i64 [ 0, %{entry} ], [ {next}, %{advance} ]"
        ));
        let more = self.value(format!("icmp ult i64 {index}, {length}"));
        self.branch(&more, &element, &exit);
        self.begin(&element);
        body(self, &index);
        self.jump(&advance);
        self.begin(&advance);
        self.instruction(format!("{next} = add i64 {index}, 1"));
        self.jump(&condition);
        self.begin(&exit);
    }

    fn make_closure(&mut self, id: usize, values: &[String]) -> String {
        let function = &self.module.functions[id];
        let count = values.len();
        let environment = if count == 0 {
            "null".to_owned()
        } else {
            let ty = environment_type(function, count, self.module);
            let environment = self.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64))"));
            for (index, value) in values.iter().enumerate() {
                let pointer = self.value(format!(
                    "getelementptr inbounds {ty}, ptr {environment}, i32 0, i32 {index}"
                ));
                self.instruction(format!(
                    "store {} {value}, ptr {pointer}",
                    self.ty(&function.signature.parameters[index])
                ));
            }
            environment
        };
        self.closure_descriptor(id, count, &environment)
    }

    fn closure_descriptor(&mut self, id: usize, count: usize, environment: &str) -> String {
        let function = &self.module.functions[id];
        let name = function.qualified_name();
        let value = self.value(format!(
            "insertvalue %tz.closure zeroinitializer, ptr @tz.apply.{name}.{count}, 0"
        ));
        let value = self.value(format!(
            "insertvalue %tz.closure {value}, ptr {environment}, 1"
        ));
        if count == 0 {
            return value;
        }
        let value = if !function.is_task
            && function.signature.parameters[..count]
                .iter()
                .all(|ty| ty.can_capture(&self.module.types()))
        {
            self.value(format!(
                "insertvalue %tz.closure {value}, ptr @tz.env.clone.{name}.{count}, 2"
            ))
        } else {
            value
        };
        self.value(format!(
            "insertvalue %tz.closure {value}, ptr @tz.env.drop.{name}.{count}, 3"
        ))
    }

    fn stack_closure(&mut self, target: ClosureTarget, values: &[String]) -> String {
        if values.is_empty() {
            return self.closure_descriptor(target.function, 0, "null");
        }
        let function = &self.module.functions[target.function];
        let environment_ty = environment_type(function, target.bound, self.module);
        let environment = self.fresh();
        self.allocas
            .push(format!("{environment} = alloca {environment_ty}, align 16"));
        for (index, value) in values.iter().enumerate() {
            let pointer = self.value(format!(
                "getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}"
            ));
            self.instruction(format!(
                "store {} {value}, ptr {pointer}",
                self.ty(&function.signature.parameters[index])
            ));
        }
        self.closure_descriptor(target.function, target.bound, &environment)
    }

    fn parallel_tasks(&mut self, tasks: &TypedExpr, result: &Type) -> String {
        let tasks = self.expression(tasks);
        let source = self.value(format!("extractvalue %tz.array {tasks}, 0"));
        let length = self.value(format!("extractvalue %tz.array {tasks}, 1"));
        let Type::Array(element) = result else {
            unreachable!()
        };
        let (array, target) = self.allocate_array(element, &length);
        let ty = self.ty(element);
        let callback = format!("@tz.task.item.{}", ty.trim_start_matches('%'));
        self.intrinsics.insert(format!(
            "define internal void {callback}(ptr %context, i64 %index) nounwind {{\n\
             entry:\n\
               %source = load ptr, ptr %context\n\
               %target.slot = getelementptr inbounds {{ ptr, ptr }}, ptr %context, i32 0, i32 1\n\
               %target = load ptr, ptr %target.slot\n\
               %slot = getelementptr inbounds %tz.closure, ptr %source, i64 %index\n\
               %task = load %tz.closure, ptr %slot\n\
               %code = extractvalue %tz.closure %task, 0\n\
               %env = extractvalue %tz.closure %task, 1\n\
               %result = call {ty} %code(ptr %env)\n\
               %destination = getelementptr inbounds {ty}, ptr %target, i64 %index\n\
               store {ty} %result, ptr %destination\n\
               ret void\n\
             }}\n"
        ));
        let context = self.fresh();
        self.allocas
            .push(format!("{context} = alloca {{ ptr, ptr }}, align 16"));
        self.instruction(format!("store ptr {source}, ptr {context}"));
        let slot = self.value(format!(
            "getelementptr inbounds {{ ptr, ptr }}, ptr {context}, i32 0, i32 1"
        ));
        self.instruction(format!("store ptr {target}, ptr {slot}"));
        self.instruction(format!(
            "call void @tsuzuri_task_parallel(ptr {callback}, ptr {context}, i64 {length})"
        ));
        self.instruction(format!("call void @tz.free(ptr {source})"));
        array
    }

    fn apply_value(
        &mut self,
        callee: &str,
        ty: &Type,
        argument: Option<(&Type, &str)>,
    ) -> (String, Type) {
        let code = self.value(format!("extractvalue %tz.closure {callee}, 0"));
        let environment = self.value(format!("extractvalue %tz.closure {callee}, 1"));
        let result = ty.after_arguments(usize::from(argument.is_some()));
        let argument = argument.map_or_else(String::new, |(ty, value)| {
            format!(", {} {value}", self.ty(ty))
        });
        let value = self.value(format!(
            "call {} {code}(ptr {environment}{argument})",
            self.ty(&result)
        ));
        (value, result)
    }

    fn call(&mut self, callee: &TypedExpr, arguments: &[TypedExpr]) -> String {
        if let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind {
            let function = &self.module.functions[id];
            if arguments.len() == 1 && call_specialization::is_identity(function) {
                return self.expression(&arguments[0]);
            }
        }
        if !matches!(callee.kind, TypedExprKind::Function(_)) {
            if let Some(target) =
                call_specialization::target(callee, &self.known_closures, self.module)
            {
                if target.bound + arguments.len()
                    == self.module.functions[target.function].parameters.len()
                    && !arguments
                        .iter()
                        .any(|argument| call_specialization::may_mutate(argument, self.module))
                    && self.specializations.can_borrow(target, self.module)
                {
                    if let Some(value) = self.borrowed_call(callee, target, arguments) {
                        return value;
                    }
                }
            }
        }
        let known = match &callee.kind {
            &TypedExprKind::Function(FunctionRef::User(id)) => {
                let function = &self.module.functions[id];
                if arguments.len() < function.parameters.len() {
                    let values: Vec<_> = arguments
                        .iter()
                        .map(|argument| self.expression(argument))
                        .collect();
                    return self.make_closure(id, &values);
                }
                let mut callbacks = Vec::new();
                if arguments.len() == function.parameters.len() {
                    for index in self.specializations.eligible[id].clone() {
                        if arguments[index + 1..]
                            .iter()
                            .any(|argument| call_specialization::may_mutate(argument, self.module))
                        {
                            continue;
                        }
                        if let Some(target) = call_specialization::target(
                            &arguments[index],
                            &self.known_closures,
                            self.module,
                        ) {
                            if self.specializations.can_borrow(target, self.module) {
                                callbacks.push((index, target));
                            }
                        }
                    }
                }
                let symbol = if callbacks.is_empty() {
                    format!("@tz.fn.{}", function.qualified_name())
                } else if let Some(variant) = self.specializations.request(Specialization {
                    function: id,
                    callbacks: callbacks.clone(),
                    borrowed: 0,
                }) {
                    format!("@tz.specialized.{variant}")
                } else {
                    callbacks.clear();
                    format!("@tz.fn.{}", function.qualified_name())
                };
                Some((symbol, function.signature.clone(), callbacks))
            }
            TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                let count = instance.builtin.scheme().parameters.len();
                debug_assert!(
                    count <= arguments.len(),
                    "builtin wrappers apply every parameter"
                );
                let Type::Function(parameters, _) = &callee.ty else {
                    unreachable!("a builtin is a function")
                };
                let signature = crate::check::Signature {
                    parameters: parameters[..count].to_vec(),
                    result: callee.ty.after_arguments(count),
                };
                self.builtins
                    .entry(instance.clone())
                    .or_insert_with(|| callee.ty.clone());
                Some((builtin_symbol(instance, self.module), signature, Vec::new()))
            }
            _ => None,
        };
        let (mut value, mut ty, consumed) = if let Some((symbol, signature, callbacks)) = known {
            let count = signature.parameters.len();
            let mut values = Vec::new();
            let mut cleanup = Vec::new();
            for (index, argument) in arguments[..count].iter().enumerate() {
                let value =
                    if let Some((_, target)) = callbacks.iter().find(|(slot, _)| *slot == index) {
                        let argument = call_specialization::transparent(argument, self.module);
                        if Self::is_place(argument) {
                            self.expression_mode(argument, false)
                        } else {
                            let captures = self.capture_values(argument);
                            self.capture_cleanup(*target, &captures, &mut cleanup);
                            let captures: Vec<_> =
                                captures.into_iter().map(|(value, _)| value).collect();
                            self.stack_closure(*target, &captures)
                        }
                    } else {
                        self.expression(argument)
                    };
                values.push(format!("{} {value}", self.ty(&argument.ty)));
            }
            let values = values.join(", ");
            let value = self.value(format!(
                "call {} {symbol}({values})",
                self.ty(&signature.result)
            ));
            for (ty, value, frames) in cleanup.iter().rev() {
                self.drop_framed(ty, value, frames);
            }
            (value, signature.result, count)
        } else {
            let value = self.expression(callee);
            if arguments.is_empty() {
                return self.apply_value(&value, &callee.ty, None).0;
            }
            (value, callee.ty.clone(), 0)
        };
        for argument in &arguments[consumed..] {
            let next = self.expression(argument);
            (value, ty) = self.apply_value(&value, &ty, Some((&argument.ty, &next)));
        }
        value
    }

    fn capture_values(&mut self, expression: &TypedExpr) -> Vec<(String, Vec<Frame>)> {
        let expression = call_specialization::transparent(expression, self.module);
        match &expression.kind {
            // Known temporary closures only lend their captures to a borrowing worker; the caller
            // drops them after the call, so stack values need not move to the heap.
            TypedExprKind::Closure(_, values) | TypedExprKind::Call(_, values) => values
                .iter()
                .map(|value| self.take_operand(value))
                .collect(),
            TypedExprKind::Function(_) => Vec::new(),
            _ => unreachable!("known temporary closures have explicit captures"),
        }
    }

    fn capture_cleanup(
        &self,
        target: ClosureTarget,
        captures: &[(String, Vec<Frame>)],
        cleanup: &mut Vec<(Type, String, Vec<Frame>)>,
    ) {
        for ((value, frames), ty) in captures
            .iter()
            .zip(&self.module.functions[target.function].signature.parameters[..target.bound])
        {
            if ty.needs_drop(&self.module.types()) {
                cleanup.push((ty.clone(), value.clone(), frames.clone()));
            }
        }
    }

    fn borrowed_call(
        &mut self,
        callee: &TypedExpr,
        target: ClosureTarget,
        arguments: &[TypedExpr],
    ) -> Option<String> {
        let call = self.prepare_borrowed_call(callee, target)?;
        let arguments: Vec<_> = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect();
        let result = self.emit_borrowed_call(&call, &arguments);
        self.finish_borrowed_call(&call);
        Some(result)
    }

    fn prepare_known_call(&mut self, expression: &TypedExpr, arity: usize) -> Option<BorrowedCall> {
        let target = call_specialization::target(expression, &self.known_closures, self.module)?;
        if target.bound + arity != self.module.functions[target.function].parameters.len()
            || !self.specializations.can_borrow(target, self.module)
        {
            return None;
        }
        self.prepare_borrowed_call(expression, target)
    }

    fn prepare_borrowed_call(
        &mut self,
        callee: &TypedExpr,
        target: ClosureTarget,
    ) -> Option<BorrowedCall> {
        let callee = call_specialization::transparent(callee, self.module);
        let function = &self.module.functions[target.function];
        let symbol = if target.bound == 0 {
            format!("@tz.fn.{}", function.qualified_name())
        } else {
            let id = self.specializations.request(Specialization {
                function: target.function,
                callbacks: Vec::new(),
                borrowed: target.bound,
            })?;
            format!("@tz.specialized.{id}")
        };
        let mut cleanup = Vec::new();
        let captures = if Self::is_place(callee) {
            let value = self.expression_mode(callee, false);
            let environment = self.value(format!("extractvalue %tz.closure {value}, 1"));
            let environment_ty = environment_type(function, target.bound, self.module);
            let mut values = Vec::new();
            for (index, ty) in function.signature.parameters[..target.bound]
                .iter()
                .enumerate()
            {
                let pointer = self.value(format!(
                    "getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}",
                ));
                values.push(self.value(format!("load {}, ptr {pointer}", self.ty(ty))));
            }
            values
        } else {
            let values = self.capture_values(callee);
            self.capture_cleanup(target, &values, &mut cleanup);
            values.into_iter().map(|(value, _)| value).collect()
        };
        Some(BorrowedCall {
            target,
            symbol,
            captures,
            cleanup,
        })
    }

    fn emit_borrowed_call(&mut self, call: &BorrowedCall, arguments: &[String]) -> String {
        let function = &self.module.functions[call.target.function];
        let values = call
            .captures
            .iter()
            .chain(arguments)
            .zip(&function.signature.parameters)
            .map(|(value, ty)| format!("{} {value}", self.ty(ty)))
            .collect::<Vec<_>>()
            .join(", ");
        self.value(format!(
            "call {} {}({values})",
            self.ty(&function.signature.result),
            call.symbol
        ))
    }

    fn finish_borrowed_call(&mut self, call: &BorrowedCall) {
        for (ty, value, frames) in call.cleanup.iter().rev() {
            self.drop_framed(ty, value, frames);
        }
    }

    fn drop_slot(&mut self, slot: &str, ty: &Type) {
        if ty.needs_drop(&self.module.types()) {
            let value = self.value(format!("load {}, ptr {slot}", self.ty(ty)));
            let frames = self.frame_slots.get(slot).cloned().unwrap_or_default();
            self.drop_framed(ty, &value, &frames);
            self.instruction(format!("store {} zeroinitializer, ptr {slot}", self.ty(ty)));
        }
    }

    fn drop_scope(&mut self, scope: &[(String, Type)]) {
        for (slot, ty) in scope.iter().rev() {
            self.drop_slot(slot, ty);
        }
    }

    fn drop_all(&mut self) {
        for scope in self.scopes.clone().iter().rev() {
            self.drop_scope(scope);
        }
    }

    fn spill(&mut self, ty: &Type, value: &str) -> String {
        let slot = self.slot(ty);
        self.instruction(format!("store {} {value}, ptr {slot}", self.ty(ty)));
        slot
    }

    fn cast(&mut self, expression: &TypedExpr, to: &Type) -> String {
        let value = self.expression(expression);
        if expression.ty == *to {
            return value;
        }
        if let (Type::Integer(from, signed), Type::Integer(bits, _)) = (&expression.ty, to) {
            if from == bits {
                return value;
            }
            let instruction = if bits < from {
                "trunc"
            } else if *signed {
                "sext"
            } else {
                "zext"
            };
            return self.value(format!(
                "{instruction} {} {value} to {}",
                self.ty(&expression.ty),
                self.ty(to)
            ));
        }
        let instruction = match (&expression.ty, to) {
            (Type::Integer(8 | 16 | 32 | 64, true), Type::Binary(32 | 64)) => Some("sitofp"),
            (Type::Integer(8 | 16 | 32 | 64, false), Type::Binary(32 | 64)) => Some("uitofp"),
            (Type::Binary(32), Type::Binary(64)) => Some("fpext"),
            (Type::Binary(64), Type::Binary(32)) => Some("fptrunc"),
            _ => None,
        };
        if let Some(instruction) = instruction {
            return self.value(format!(
                "{instruction} {} {value} to {}",
                self.ty(&expression.ty),
                self.ty(to)
            ));
        }
        if let (Type::Binary(from @ (32 | 64)), Type::Integer(bits @ (8 | 16 | 32 | 64), signed)) =
            (&expression.ty, to)
        {
            let intrinsic = saturating_cast(*from, *bits, *signed, self.intrinsics);
            return self.value(format!(
                "call {} @{intrinsic}({} {value})",
                self.ty(to),
                self.ty(&expression.ty)
            ));
        }
        let input = self.spill(&expression.ty, &value);
        let output = self.slot(to);
        self.instruction(format!(
            "call void @tz_soft_cast(ptr {output}, ptr {input}, i32 {}, i32 {})",
            numeric_kind(&expression.ty),
            numeric_kind(to)
        ));
        self.value(format!("load {}, ptr {output}", self.ty(to)))
    }

    fn short_circuit(&mut self, operator: BinaryOp, left: &TypedExpr, right: &TypedExpr) -> String {
        let left = self.expression(left);
        let left_end = self.block.clone();
        let evaluate_right = self.label();
        let merge = self.label();
        let shortcut = if operator == BinaryOp::And {
            self.branch(&left, &evaluate_right, &merge);
            "0"
        } else {
            self.branch(&left, &merge, &evaluate_right);
            "1"
        };
        self.begin(&evaluate_right);
        let right = self.expression(right);
        let right_end = self.block.clone();
        self.jump(&merge);
        self.begin(&merge);
        self.value(format!(
            "phi i1 [ {shortcut}, %{left_end} ], [ {right}, %{right_end} ]"
        ))
    }

    fn binary(&mut self, operator: BinaryOp, left: &TypedExpr, right: &TypedExpr) -> String {
        use BinaryOp::*;
        if left.ty == Type::String {
            let take = operator == Add;
            // Concatenation reads both operands and then destroys them, so stack strings stay put.
            let (lhs, left_frames) = if take {
                self.take_operand(left)
            } else {
                self.read_operand(left)
            };
            let (rhs, right_frames) = if take {
                self.take_operand(right)
            } else {
                self.read_operand(right)
            };
            let result = if take {
                self.value(format!(
                    "call %tz.string @tz.string.concat(%tz.string {lhs}, %tz.string {rhs})"
                ))
            } else {
                self.value(format!(
                    "call i1 @tz.string.equal(%tz.string {lhs}, %tz.string {rhs})"
                ))
            };
            if take {
                self.drop_framed(&left.ty, &lhs, &left_frames);
                self.drop_framed(&right.ty, &rhs, &right_frames);
            } else {
                self.release_operand(left, &lhs, &left_frames);
                self.release_operand(right, &rhs, &right_frames);
            }
            return if operator == NotEqual {
                self.value(format!("xor i1 {result}, 1"))
            } else {
                result
            };
        }
        let lhs = self.expression(left);
        let mut rhs = self.expression(right);
        let ty = self.ty(&left.ty);
        if matches!(left.ty, Type::Decimal(_) | Type::Binary(16 | 128)) {
            let a = self.spill(&left.ty, &lhs);
            let b = self.spill(&right.ty, &rhs);
            let kind = numeric_kind(&left.ty);
            if matches!(operator, Add | Subtract | Multiply | Divide) {
                let op = match operator {
                    Add => 0,
                    Subtract => 1,
                    Multiply => 2,
                    _ => 3,
                };
                let output = self.slot(&left.ty);
                self.instruction(format!(
                    "call void @tz_soft_op(ptr {output}, ptr {a}, ptr {b}, i32 {kind}, i32 {op})"
                ));
                return self.value(format!("load {ty}, ptr {output}"));
            }
            let order = self.value(format!(
                "call i32 @tz_soft_cmp(ptr {a}, ptr {b}, i32 {kind})"
            ));
            let (predicate, value) = match operator {
                Equal => ("eq", 0),
                NotEqual => ("ne", 0),
                Less => ("slt", 0),
                LessEqual => ("sle", 0),
                Greater => ("eq", 1),
                GreaterEqual => ("ule", 1),
                _ => unreachable!("software float operator checked"),
            };
            return self.value(format!("icmp {predicate} i32 {order}, {value}"));
        }
        if matches!(operator, Divide | Remainder) && left.ty.is_integer() {
            let mut valid = self.value(format!("icmp ne {ty} {rhs}, 0"));
            if let Type::Integer(bits, true) = left.ty {
                let minimum = self.value(format!("icmp eq {ty} {lhs}, {}", 1u128 << (bits - 1)));
                let negative_one = self.value(format!("icmp eq {ty} {rhs}, -1"));
                let overflow = self.value(format!("and i1 {minimum}, {negative_one}"));
                let no_overflow = self.value(format!("xor i1 {overflow}, 1"));
                valid = self.value(format!("and i1 {valid}, {no_overflow}"));
            }
            self.guard(&valid);
        }
        if matches!(operator, ShiftLeft | ShiftRight | ShiftRightUnsigned) {
            let Type::Integer(bits, _) = left.ty else {
                unreachable!()
            };
            rhs = self.value(format!("and {ty} {rhs}, {}", bits - 1));
        }
        let unsigned = matches!(left.ty, Type::Integer(_, false));
        let instruction = if left.ty.is_float() {
            match operator {
                Add => "fadd",
                Subtract => "fsub",
                Multiply => "fmul",
                Divide => "fdiv",
                Equal => "fcmp oeq",
                NotEqual => "fcmp une",
                Less => "fcmp olt",
                LessEqual => "fcmp ole",
                Greater => "fcmp ogt",
                GreaterEqual => "fcmp oge",
                _ => unreachable!("float operator checked"),
            }
        } else {
            match operator {
                Add => "add",
                Subtract => "sub",
                Multiply => "mul",
                Divide => {
                    if unsigned {
                        "udiv"
                    } else {
                        "sdiv"
                    }
                }
                Remainder => {
                    if unsigned {
                        "urem"
                    } else {
                        "srem"
                    }
                }
                Equal => "icmp eq",
                NotEqual => "icmp ne",
                Less => {
                    if unsigned {
                        "icmp ult"
                    } else {
                        "icmp slt"
                    }
                }
                LessEqual => {
                    if unsigned {
                        "icmp ule"
                    } else {
                        "icmp sle"
                    }
                }
                Greater => {
                    if unsigned {
                        "icmp ugt"
                    } else {
                        "icmp sgt"
                    }
                }
                GreaterEqual => {
                    if unsigned {
                        "icmp uge"
                    } else {
                        "icmp sge"
                    }
                }
                BitAnd => "and",
                BitOr => "or",
                BitXor => "xor",
                ShiftLeft => "shl",
                ShiftRight => {
                    if unsigned {
                        "lshr"
                    } else {
                        "ashr"
                    }
                }
                ShiftRightUnsigned => "lshr",
                _ => unreachable!("operator checked or handled separately"),
            }
        };
        self.value(format!("{instruction} {} {lhs}, {rhs}", self.ty(&left.ty)))
    }
}

fn numeric_kind(ty: &Type) -> u16 {
    match ty {
        Type::Binary(16) => 0,
        Type::Binary(32) => 1,
        Type::Binary(64) => 2,
        Type::Binary(128) => 3,
        Type::Decimal(32) => 4,
        Type::Decimal(64) => 5,
        Type::Decimal(128) => 6,
        Type::Integer(bits, signed) => 16 + (bits / 8).ilog2() as u16 + if *signed { 0 } else { 8 },
        _ => unreachable!("numeric type checked"),
    }
}

fn export_wrapper(function: &CheckedFunction, module: &CheckedModule) -> String {
    let parameters = function
        .signature
        .parameters
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("{} %arg{index}", abi_type(ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let result = &function.signature.result;
    let mut output = format!(
        "define {} @tz_{}({parameters}) nounwind {{\nentry:\n",
        abi_type(result),
        function.name
    );
    let mut arguments = Vec::new();
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        if *ty == Type::Bool {
            let _ = writeln!(output, "  %b{index} = icmp ne i32 %arg{index}, 0");
            arguments.push(format!("i1 %b{index}"));
        } else if let Type::Integer(bits @ (8 | 16), _) = ty {
            let _ = writeln!(output, "  %n{index} = trunc i32 %arg{index} to i{bits}");
            arguments.push(format!("i{bits} %n{index}"));
        } else {
            arguments.push(format!("{} %arg{index}", llvm_type(ty, module)));
        }
    }
    let _ = writeln!(
        output,
        "  %result = call {} @tz.fn.{}({})",
        llvm_type(result, module),
        function.qualified_name(),
        arguments.join(", ")
    );
    match result {
        Type::Bool => output.push_str("  %bool = zext i1 %result to i32\n  ret i32 %bool\n"),
        Type::Unit => output.push_str("  ret void\n"),
        Type::Integer(bits @ (8 | 16), signed) => {
            let _ = writeln!(
                output,
                "  %wide = {} i{bits} %result to i32\n  ret i32 %wide",
                if *signed { "sext" } else { "zext" }
            );
        }
        _ => {
            let _ = writeln!(output, "  ret {} %result", llvm_type(result, module));
        }
    }
    output.push_str("}\n\n");
    output
}

fn saturating_cast(
    from: u16,
    bits: u16,
    signed: bool,
    intrinsics: &mut BTreeSet<String>,
) -> String {
    let sign = if signed { "s" } else { "u" };
    let intrinsic = format!("llvm.fpto{sign}i.sat.i{bits}.f{from}");
    let source = if from == 32 { "float" } else { "double" };
    intrinsics.insert(format!("declare i{bits} @{intrinsic}({source})"));
    intrinsic
}

/// The LLVM symbol of a builtin instance: `@tz.builtin.name`, followed for a
/// polymorphic builtin by its type arguments as unquoted, injective names.
fn builtin_symbol(instance: &BuiltinInstance, module: &CheckedModule) -> String {
    let mut symbol = format!("@tz.builtin.{}", instance.builtin.name());
    if !instance.types.is_empty() {
        symbol.push('.');
        symbol.push_str(
            &instance
                .types
                .iter()
                .map(|ty| mangled_type(ty, module))
                .collect::<Vec<_>>()
                .join("$C"),
        );
    }
    symbol
}

/// `canonical_type` spelled with LLVM identifier characters only; `$` never
/// occurs in canonical types, so the escapes keep the spelling injective.
fn mangled_type(ty: &Type, module: &CheckedModule) -> String {
    canonical_type(ty, module)
        .replace("->", "$A")
        .replace('[', "$L")
        .replace(']', "$R")
        .replace(',', "$C")
}

/// Defines one builtin instance, whose callee type `ty` is concrete.
fn emit_builtin(
    instance: &BuiltinInstance,
    ty: &Type,
    module: &CheckedModule,
    intrinsics: &mut BTreeSet<String>,
) -> String {
    let builtin = instance.builtin;
    let name = builtin.name();
    let symbol = builtin_symbol(instance, module);
    let count = builtin.scheme().parameters.len();
    let result = llvm_type(&ty.after_arguments(count), module);
    if builtin == Builtin::Unreachable {
        return format!(
            "define internal {result} {symbol}(i8 %unit) noreturn nounwind {{\n\
             entry:\n  call void @llvm.trap()\n  unreachable\n}}\n\n"
        );
    }
    #[cfg(test)]
    if let Some(definition) = test_builtin(instance, ty, &symbol, &result, module) {
        return definition;
    }
    match builtin {
        Builtin::ToFloat => format!(
            "define internal double @tz.builtin.{name}(i64 %x) nounwind {{\n\
             entry:\n  %r = sitofp i64 %x to double\n  ret double %r\n}}\n\n"
        ),
        Builtin::ToInt => {
            let intrinsic = saturating_cast(64, 64, true, intrinsics);
            format!(
                "define internal i64 @tz.builtin.{name}(double %x) nounwind {{\n\
                 entry:\n  %r = call i64 @{intrinsic}(double %x)\n  ret i64 %r\n}}\n\n"
            )
        }
        Builtin::Assert => format!(
            "define internal i8 @tz.builtin.{name}(i1 %x) nounwind {{\n\
             entry:\n  br i1 %x, label %ok, label %fail\n\
             fail:\n  call void @llvm.trap()\n  unreachable\n\
             ok:\n  ret i8 0\n}}\n\n"
        ),
        Builtin::CloneString => format!(
            "define internal %tz.string @tz.builtin.{name}(ptr %x) nounwind {{\n\
             entry:\n  %s = load %tz.string, ptr %x\n\
             %p = extractvalue %tz.string %s, 0\n  %n = extractvalue %tz.string %s, 1\n\
             %r = call %tz.string @tz.string.new(ptr %p, i64 %n)\n  ret %tz.string %r\n}}\n\n"
        ),
        _ => {
            let intrinsic = match builtin {
                Builtin::Sqrt => "sqrt",
                Builtin::Floor => "floor",
                Builtin::Ceil => "ceil",
                Builtin::Abs => "fabs",
                _ => unreachable!(),
            };
            intrinsics.insert(format!("declare double @llvm.{intrinsic}.f64(double)"));
            format!(
                "define internal double @tz.builtin.{name}(double %x) nounwind {{\n\
                 entry:\n  %r = call double @llvm.{intrinsic}.f64(double %x)\n\
                 ret double %r\n}}\n\n"
            )
        }
    }
}

/// Defines the test-only builtins that exercise polymorphic instances.
#[cfg(test)]
fn test_builtin(
    instance: &BuiltinInstance,
    ty: &Type,
    symbol: &str,
    result: &str,
    module: &CheckedModule,
) -> Option<String> {
    let Type::Function(parameters, _) = ty else {
        unreachable!("a builtin is a function")
    };
    let parameter = llvm_type(&parameters[0], module);
    let body = match instance.builtin {
        Builtin::TestAdd => {
            return Some(format!(
                "define internal {result} {symbol}({parameter} %x, {parameter} %y) nounwind {{\n\
                 entry:\n  %r = add {parameter} %x, %y\n  ret {result} %r\n}}\n\n"
            ));
        }
        Builtin::TestUnsigned => format!("ret {result} %x"),
        Builtin::TestWiden => {
            let Type::Integer(_, signed) = instance.types[0] else {
                unreachable!("Int.test_widen takes an integer")
            };
            let extend = if signed { "sext" } else { "zext" };
            format!("%r = {extend} {parameter} %x to {result}\n  ret {result} %r")
        }
        _ => return None,
    };
    Some(format!(
        "define internal {result} {symbol}({parameter} %x) nounwind {{\nentry:\n  {body}\n}}\n\n"
    ))
}

fn console_main(module: &CheckedModule) -> String {
    let main = &module.functions[module.entry.unwrap()];
    let ty = &main.signature.result;
    let buffered = matches!(
        ty,
        Type::String | Type::Integer(128, _) | Type::Binary(16 | 128) | Type::Decimal(_)
    );
    let mut output = match ty {
        Type::Integer(8 | 16 | 32 | 64, signed) => format!(
            "@tz.fmt = private unnamed_addr constant [6 x i8] c\"%ll{}\\0A\\00\"\n",
            if *signed { "d" } else { "u" }
        ),
        Type::Binary(64) => {
            "@tz.fmt = private unnamed_addr constant [7 x i8] c\"%.17g\\0A\\00\"\n".into()
        }
        Type::Binary(32) => {
            "@tz.fmt = private unnamed_addr constant [6 x i8] c\"%.9g\\0A\\00\"\n".into()
        }
        Type::Bool => String::from(
            "@tz.fmt = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"\n\
             @tz.true = private unnamed_addr constant [5 x i8] c\"true\\00\"\n\
             @tz.false = private unnamed_addr constant [6 x i8] c\"false\\00\"\n",
        ),
        _ => String::new(),
    };
    if buffered {
        output.push_str(include_str!("runtime/console.ll"));
    } else if *ty != Type::Unit {
        output.push_str("declare i32 @printf(ptr, ...)\n\n");
    }
    let _ = writeln!(
        output,
        "define i32 @main() {{\nentry:\n  %result = call {} @tz.fn.{}()",
        llvm_type(ty, module),
        main.qualified_name()
    );
    match ty {
        Type::Integer(bits @ (8 | 16 | 32 | 64), signed) => {
            let value = if *bits < 64 {
                let _ = writeln!(
                    output,
                    "  %wide = {} i{bits} %result to i64",
                    if *signed { "sext" } else { "zext" }
                );
                "%wide"
            } else {
                "%result"
            };
            let _ = writeln!(
                output,
                "  %printed = call i32 (ptr, ...) @printf(ptr @tz.fmt, i64 {value})"
            );
        }
        Type::Binary(32 | 64) => {
            let value = if *ty == Type::Binary(32) {
                output.push_str("  %wide = fpext float %result to double\n");
                "%wide"
            } else {
                "%result"
            };
            let _ = writeln!(
                output,
                "  %printed = call i32 (ptr, ...) @printf(ptr @tz.fmt, double {value})"
            );
        }
        Type::Bool => {
            output.push_str(
                "  %text = select i1 %result, ptr @tz.true, ptr @tz.false\n\
                   %printed = call i32 (ptr, ...) @printf(ptr @tz.fmt, ptr %text)\n",
            );
        }
        Type::String => {
            output.push_str("  %data = extractvalue %tz.string %result, 0\n  %length = extractvalue %tz.string %result, 1\n  %printed = call i32 @tz.console.write(ptr %data, i64 %length)\n  call void @tz.free(ptr %data)\n");
        }
        Type::Unit => {}
        _ if ty.is_numeric() => {
            let _ = writeln!(
                output,
                "  %slot = alloca {}, align 16\n  %buffer = alloca [64 x i8], align 16\n  store {} %result, ptr %slot\n  %count = call i32 @tz_soft_format(ptr %buffer, ptr %slot, i32 {})\n  %length = zext i32 %count to i64\n  %printed = call i32 @tz.console.write(ptr %buffer, i64 %length)",
                llvm_type(ty, module),
                llvm_type(ty, module),
                numeric_kind(ty)
            );
        }
        _ => unreachable!("main validated"),
    }
    if *ty == Type::Unit {
        output.push_str("  ret i32 0\n");
    } else {
        output.push_str(
            "  %failed = icmp slt i32 %printed, 0\n  %exit = zext i1 %failed to i32\n  ret i32 %exit\n",
        );
    }
    output.push_str("}\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;

    #[test]
    fn emits_explicit_tail_loop_and_checked_arithmetic() {
        let module = analyze(
            "fn rec sum(n: i64, acc: i64) -> i64 {
                if n == 0 { acc / 2 } else { sum(n - 1, acc + n) }
             }
             export fn main() -> i64 { sum(100, 0) }",
        )
        .unwrap();
        let ir = emit(&module, Entry::Console).unwrap();
        let body = ir
            .split("define internal i64 @tz.fn.Main.sum(")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains("call i64 @tz.fn.Main.sum"));
        assert!(ir.contains("phi i64"));
        assert!(ir.contains("call void @llvm.trap()"));
        assert!(ir.contains("define i64 @tz_main()"));
        assert!(ir.contains("define i32 @main()"));
        assert!(!ir.contains(" nsw "));
        assert_eq!(ir, emit(&module, Entry::Console).unwrap());
    }

    #[test]
    fn puts_array_storage_before_the_tail_loop() {
        let module = analyze(
            "fn rec f(n: i64, a: [i64]) -> i64 {
                let first = a[n & 1];
                if n == 0 { first } else { f(n - 1, [first, 2]) }
             }",
        )
        .unwrap();
        let ir = emit(&module, Entry::Library).unwrap();
        assert!(ir.find("alloca").unwrap() < ir.find("br label %loop").unwrap());
        let body = ir
            .split("define internal i64 @tz.fn.Main.f(")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains("call i64 @tz.fn.Main.f"));
    }

    #[test]
    fn emits_portable_bool_abi_and_c_header() {
        let module = analyze("export fn not(x: bool) -> bool { !x }").unwrap();
        let ir = emit(&module, Entry::Library).unwrap();
        assert!(ir.contains("define i32 @tz_not(i32 %arg0)"));
        assert!(ir.contains("icmp ne i32 %arg0, 0"));
        assert!(header(&module).contains("int32_t tz_not(int32_t arg0);"));
        assert!(emit(&module, Entry::Console).is_err());
    }

    fn checked(source: &str) -> CheckedModule {
        analyze(source)
            .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
    }

    fn library(source: &str) -> String {
        let module = checked(source);
        let ir = emit(&module, Entry::Library).unwrap();
        assert_eq!(ir, emit(&module, Entry::Library).unwrap(), "{source}");
        ir
    }

    #[test]
    fn applies_multi_argument_constrained_builtins_like_functions() {
        let add = "define internal i32 @tz.builtin.Int.test_add.i32(i32 %x, i32 %y) nounwind";
        for source in [
            "export def f :: i32 -> i32\nfn f x = Int.test_add x 1i32",
            "export def f :: i32 -> i32\nfn f x = {\n    let add = Int.test_add x;\n    add 2i32\n}",
            "export def f :: i32 -> i32\nfn f x = x |> Int.test_add 3i32",
            "def twice :: (i32 -> i32 -> i32) -> i32 -> i32\nfn twice g x = g x x\n\
             export def f :: i32 -> i32\nfn f x = twice Int.test_add x",
        ] {
            let ir = library(source);
            assert_eq!(ir.matches(add).count(), 1, "{source}\n{ir}");
            assert!(ir.contains("add i32 %x, %y"), "{source}");
        }
        // A generic higher-order function receives one instance per type.
        let ir = library(
            "def apply :: ('a -> 'a -> 'a) -> 'a -> 'a\nfn apply g x = g x x\n\
             export def small :: i32 -> i32\nfn small x = apply Int.test_add x\n\
             export def large :: i64 -> i64\nfn large x = apply Int.test_add x\n\
             export def again :: i32 -> i32\nfn again x = Int.test_add x x",
        );
        assert_eq!(ir.matches(add).count(), 1, "{ir}");
        assert_eq!(
            ir.matches("define internal i64 @tz.builtin.Int.test_add.i64(i64 %x, i64 %y)")
                .count(),
            1,
            "{ir}"
        );
        let error = analyze("def f :: f64 -> f64\nfn f x = Int.test_add x x").unwrap_err();
        assert_eq!(error.code, "E1005", "{}", error.message);
    }

    #[test]
    fn solves_unsigned_and_widened_builtin_results_at_call_sites() {
        let ir = library(
            "export def unsigned :: i32 -> i32u\nfn unsigned x = Int.test_unsigned x\n\
             export def widen :: i32 -> i64\nfn widen x = Int.test_widen x\n\
             def widen_unsigned :: i64u -> i128u\nfn widen_unsigned x = x |> Int.test_widen\n\
             let defaulted: i128 = Int.test_widen 1\n0",
        );
        for definition in [
            "define internal i32 @tz.builtin.Int.test_unsigned.i32(i32 %x) nounwind",
            "define internal i64 @tz.builtin.Int.test_widen.i32(i32 %x) nounwind",
            "define internal i128 @tz.builtin.Int.test_widen.i64u(i64 %x) nounwind",
            "define internal i128 @tz.builtin.Int.test_widen.i64(i64 %x) nounwind",
        ] {
            assert_eq!(ir.matches(definition).count(), 1, "{definition}\n{ir}");
        }
        assert!(ir.contains("sext i32 %x to i64"));
        assert!(ir.contains("zext i64 %x to i128"));
        for (source, code, message) in [
            (
                "def f :: i32 -> i32\nfn f x = Int.test_unsigned x",
                "E1003",
                "",
            ),
            ("let x: i64 = Int.test_widen 1\n0", "E1003", ""),
            (
                "def f :: i128 -> i128\nfn f x = Int.test_widen x",
                "E1015",
                "128-bit integers have no wider integer type",
            ),
            (
                "def f :: Integer 'a => 'a -> 'a\nfn f x = {\n    let _ = Int.test_widen x;\n    x\n}",
                "E1015",
                "generic code cannot use it",
            ),
            (
                "def f :: f64 -> f64\nfn f x = Int.test_unsigned x",
                "E1005",
                "Integer f64",
            ),
        ] {
            let error = analyze(source).expect_err(source);
            assert_eq!(error.code, code, "{source}\n{}", error.message);
            assert!(
                error.message.contains(message),
                "{source}\n{}",
                error.message
            );
        }
    }

    #[test]
    fn emits_one_unreachable_instance_per_result_type() {
        let ir = library(
            "export def f :: bool -> i64\nfn f b = if b then 1 else unreachable ()\n\
             export def g :: bool -> i64\nfn g b = if b then 2 else unreachable ()\n\
             def h :: bool -> string\nfn h b = if b then \"x\" else unreachable ()",
        );
        for definition in [
            "define internal i64 @tz.builtin.unreachable.i64(i8 %unit) noreturn nounwind",
            "define internal %tz.string @tz.builtin.unreachable.string(i8 %unit) noreturn nounwind",
        ] {
            assert_eq!(ir.matches(definition).count(), 1, "{definition}\n{ir}");
        }
        let library = library("export def answer :: i64\nfn answer = 42");
        assert!(!library.contains("@tz.builtin."), "{library}");
        assert!(!library.contains("@tz.fn.Math."), "{library}");
    }
}
