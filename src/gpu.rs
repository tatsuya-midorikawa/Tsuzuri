use std::collections::BTreeSet;
use std::fmt::Write;

use crate::{
    check::{CheckedModule, FunctionRef, Type, TypedExpr, TypedExprKind},
    diagnostic::{Diagnostic, Span},
    llvm,
    syntax::{BinaryOp, UnaryOp},
};

#[derive(Debug)]
pub struct GpuKernel<'a> {
    module: &'a CheckedModule,
    function: usize,
    functions: BTreeSet<usize>,
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Bool | Type::Integer(32 | 64, _) | Type::Binary(32 | 64)
    )
}

fn unsupported(message: &str, span: Span) -> Diagnostic {
    Diagnostic::new("E1018", message, span)
}

pub(crate) fn validate_calls(module: &CheckedModule) -> Result<(), Diagnostic> {
    let apis: std::collections::BTreeMap<_, _> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.module == "Gpu" && function.origin.module == crate::check::ModuleOrigin::Std
        })
        .filter_map(|(id, function)| {
            let name = function.name.split('.').next().unwrap();
            matches!(name, "init" | "map" | "from_array").then_some((id, name))
        })
        .collect();
    if apis.is_empty() {
        return Ok(());
    }
    for function in &module.functions {
        if function.origin.module == crate::check::ModuleOrigin::Std {
            continue;
        }
        let mut pending = vec![(&function.body, false)];
        while let Some((expression, direct_callee)) = pending.pop() {
            if let TypedExprKind::Function(FunctionRef::User(id)) = &expression.kind
                && apis.contains_key(id)
                && !direct_callee
            {
                return Err(unsupported(
                    "Gpu.init/map/from_array cannot escape as function values; use direct full application",
                    expression.span,
                ));
            }
            if let TypedExprKind::Call(callee, arguments) = &expression.kind
                && let TypedExprKind::Function(FunctionRef::User(id)) = &callee.kind
                && let Some(name) = apis.get(id)
            {
                if arguments.len() != module.functions[*id].signature.parameters.len() {
                    return Err(unsupported(
                        "Gpu.init/map/from_array require direct full application",
                        expression.span,
                    ));
                }
                let Type::Record(_, types) = &expression.ty else {
                    return Err(unsupported(
                        "GPU operations require a concrete buffer result",
                        expression.span,
                    ));
                };
                if !types
                    .first()
                    .is_some_and(|ty| scalar(ty) && *ty != Type::Bool)
                {
                    return Err(unsupported(
                        "GPU buffers require 32/64-bit integer or binary floating-point scalar elements",
                        expression.span,
                    ));
                }
                if *name != "from_array" {
                    let callback = &arguments[if *name == "init" { 2 } else { 1 }];
                    let target = match &callback.kind {
                        TypedExprKind::Function(FunctionRef::User(id)) => *id,
                        TypedExprKind::Closure(id, captures) if captures.is_empty() => *id,
                        _ => {
                            return Err(unsupported(
                                "Gpu.init/map require a known function or capture-free lambda",
                                callback.span,
                            ));
                        }
                    };
                    extract_kernel(module, target)?;
                }
            }
            if let TypedExprKind::Call(callee, arguments) = &expression.kind {
                pending.push((callee, true));
                pending.extend(arguments.iter().map(|argument| (argument, false)));
            } else {
                pending.extend(
                    expression
                        .children()
                        .into_iter()
                        .map(|child| (child, false)),
                );
            }
        }
    }
    Ok(())
}

pub fn extract_kernel(
    module: &CheckedModule,
    function: usize,
) -> Result<GpuKernel<'_>, Diagnostic> {
    let root = module
        .functions
        .get(function)
        .ok_or_else(|| unsupported("GPU kernel function does not exist", Span::default()))?;
    if root.signature.parameters.len() != 1
        || !scalar(&root.signature.parameters[0])
        || !scalar(&root.signature.result)
    {
        return Err(unsupported(
            "GPU kernels require one scalar parameter and a scalar result",
            root.span,
        ));
    }
    let mut functions = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut pending = vec![(function, 0, false)];
    let mut nodes = 0;
    while let Some((id, depth, leaving)) = pending.pop() {
        if leaving {
            active.remove(&id);
            functions.insert(id);
            continue;
        }
        let function = &module.functions[id];
        if active.contains(&id) {
            return Err(unsupported(
                "GPU kernels cannot call recursive functions",
                function.span,
            ));
        }
        if functions.contains(&id) {
            continue;
        }
        if depth >= 128 || functions.len() + active.len() >= 1024 {
            return Err(Diagnostic::new(
                "E1017",
                "GPU kernel exceeds 128 call levels or 1024 functions",
                function.span,
            ));
        }
        if function.capture_count != 0
            || function.is_task
            || !scalar(&function.signature.result)
            || !function.signature.parameters.iter().all(scalar)
        {
            return Err(unsupported(
                "GPU kernel calls must have scalar signatures and no captures",
                function.span,
            ));
        }
        active.insert(id);
        pending.push((id, depth, true));
        let mut expressions = vec![&function.body];
        while let Some(expression) = expressions.pop() {
            nodes += 1;
            if nodes > 65536 {
                return Err(Diagnostic::new(
                    "E1017",
                    "GPU kernel exceeds 65536 expression nodes",
                    expression.span,
                ));
            }
            if !scalar(&expression.ty) && expression.ty != Type::Unit {
                return Err(unsupported(
                    "GPU kernels cannot allocate or carry nonscalar values",
                    expression.span,
                ));
            }
            match &expression.kind {
                TypedExprKind::Int(_)
                | TypedExprKind::Float(_)
                | TypedExprKind::Bool(_)
                | TypedExprKind::Unit
                | TypedExprKind::Local(_) => {}
                TypedExprKind::Unary(..)
                | TypedExprKind::Binary(..)
                | TypedExprKind::Cast(_)
                | TypedExprKind::If { .. }
                | TypedExprKind::Block { .. } => {
                    expressions.extend(expression.children());
                }
                TypedExprKind::Assign(place, value)
                    if matches!(place.kind, TypedExprKind::Local(_)) =>
                {
                    expressions.push(value);
                }
                TypedExprKind::Call(callee, arguments) => {
                    let called = match &callee.kind {
                        TypedExprKind::Function(FunctionRef::User(called)) => *called,
                        TypedExprKind::Closure(called, captures) if captures.is_empty() => *called,
                        _ => {
                            return Err(unsupported(
                                "GPU kernels require direct known calls without captures",
                                callee.span,
                            ));
                        }
                    };
                    if arguments.len() != module.functions[called].signature.parameters.len() {
                        return Err(unsupported(
                            "GPU kernel calls must be fully applied",
                            callee.span,
                        ));
                    }
                    pending.push((called, depth + 1, false));
                    expressions.extend(arguments);
                }
                _ => {
                    return Err(unsupported(
                        "GPU kernels do not support allocation, borrows, host calls, loops, tasks, or assertions",
                        expression.span,
                    ));
                }
            }
        }
    }
    Ok(GpuKernel {
        module,
        function,
        functions,
    })
}

impl GpuKernel<'_> {
    pub fn name(&self) -> String {
        self.module.functions[self.function].qualified_name()
    }

    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub fn cpu_reference(&self, wasm: bool) -> Result<String, Diagnostic> {
        llvm::emit_target(self.module, llvm::Entry::Library, wasm)
    }

    pub fn wgsl(&self) -> Result<String, Diagnostic> {
        let root = &self.module.functions[self.function];
        if !matches!(root.signature.parameters[0], Type::Integer(32, _))
            || !matches!(root.signature.result, Type::Integer(32, _))
        {
            return Err(unsupported(
                "strict WebGPU kernels require i32 or i32u buffer lanes; 64-bit and strict floating-point lanes are unavailable",
                root.span,
            ));
        }
        let input = wgsl_type(&root.signature.parameters[0], root.span)?;
        let output = wgsl_type(&root.signature.result, root.span)?;
        let mut text = format!(
            "struct Params {{ length: u32 }}\n@group(0) @binding(0) var<storage, read> input_values: array<{input}>;\n@group(0) @binding(1) var<storage, read_write> output_values: array<{output}>;\n@group(0) @binding(2) var<uniform> params: Params;\n"
        );
        for id in &self.functions {
            let function = &self.module.functions[*id];
            let mut emitter = Wgsl {
                text: String::new(),
                next: 0,
            };
            let parameters = function
                .parameters
                .iter()
                .map(|local| {
                    Ok(format!(
                        "local_{}: {}",
                        local.id,
                        wgsl_type(&local.ty, local.span)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?
                .join(", ");
            let result = wgsl_type(&function.signature.result, function.span)?;
            let value = emitter.expression(&function.body)?;
            let _ = writeln!(
                text,
                "fn kernel_{id}({parameters}) -> {result} {{\n{}return {value};\n}}",
                emitter.text
            );
        }
        let id = self.function;
        let _ = writeln!(
            text,
            "@compute @workgroup_size(256)\nfn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}(input_values[invocation.x]); }}\n}}\n@compute @workgroup_size(256)\nfn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}({input}(invocation.x)); }}\n}}"
        );
        Ok(text)
    }
}

fn wgsl_type(ty: &Type, span: Span) -> Result<&'static str, Diagnostic> {
    match ty {
        Type::Integer(32, true) => Ok("i32"),
        Type::Integer(32, false) => Ok("u32"),
        Type::Bool => Ok("bool"),
        _ => Err(unsupported(
            "WGSL cannot preserve this scalar type; use the explicit CPU reference path",
            span,
        )),
    }
}

struct Wgsl {
    text: String,
    next: usize,
}

impl Wgsl {
    fn fresh(&mut self) -> String {
        let name = format!("value_{}", self.next);
        self.next += 1;
        name
    }

    fn expression(&mut self, expression: &TypedExpr) -> Result<String, Diagnostic> {
        let span = expression.span;
        let value = match &expression.kind {
            TypedExprKind::Int(value) => {
                let value = format!("{}u", *value as u32);
                if expression.ty == Type::Integer(32, true) {
                    format!("bitcast<i32>({value})")
                } else {
                    value
                }
            }
            TypedExprKind::Bool(value) => value.to_string(),
            TypedExprKind::Unit => return Ok(String::new()),
            TypedExprKind::Local(id) => format!("local_{id}"),
            TypedExprKind::Unary(operator, operand) => {
                let value = self.expression(operand)?;
                match operator {
                    UnaryOp::Negate if expression.ty == Type::Integer(32, true) => {
                        format!("bitcast<i32>(0u - bitcast<u32>({value}))")
                    }
                    UnaryOp::Negate => format!("(0u - {value})"),
                    UnaryOp::Not => format!("(!{value})"),
                    UnaryOp::BitNot => format!("(~{value})"),
                }
            }
            TypedExprKind::Binary(operator, left, right) => {
                let left_value = self.expression(left)?;
                if matches!(operator, BinaryOp::And | BinaryOp::Or) {
                    let name = self.fresh();
                    let condition = if *operator == BinaryOp::And {
                        left_value.clone()
                    } else {
                        format!("!{left_value}")
                    };
                    let _ = writeln!(
                        self.text,
                        "var {name}: bool = {left_value};\nif ({condition}) {{"
                    );
                    let right_value = self.expression(right)?;
                    let _ = writeln!(self.text, "{name} = {right_value};\n}}");
                    name
                } else {
                    let right_value = self.expression(right)?;
                    let signed = left.ty == Type::Integer(32, true);
                    let bits = |value: &str, ty: &Type| {
                        if *ty == Type::Integer(32, true) {
                            format!("bitcast<u32>({value})")
                        } else {
                            value.to_owned()
                        }
                    };
                    let symbol = match operator {
                        BinaryOp::Add => "+",
                        BinaryOp::Subtract => "-",
                        BinaryOp::Multiply => "*",
                        BinaryOp::Equal => "==",
                        BinaryOp::NotEqual => "!=",
                        BinaryOp::Less => "<",
                        BinaryOp::LessEqual => "<=",
                        BinaryOp::Greater => ">",
                        BinaryOp::GreaterEqual => ">=",
                        BinaryOp::BitAnd => "&",
                        BinaryOp::BitOr => "|",
                        BinaryOp::BitXor => "^",
                        BinaryOp::ShiftLeft => "<<",
                        BinaryOp::ShiftRight | BinaryOp::ShiftRightUnsigned => ">>",
                        _ => {
                            return Err(unsupported(
                                "WGSL kernel division/remainder may trap on the CPU and are not supported",
                                span,
                            ));
                        }
                    };
                    if matches!(
                        operator,
                        BinaryOp::ShiftLeft | BinaryOp::ShiftRight | BinaryOp::ShiftRightUnsigned
                    ) {
                        let amount = format!("({} & 31u)", bits(&right_value, &right.ty));
                        if *operator == BinaryOp::ShiftRight {
                            format!("({left_value} >> {amount})")
                        } else {
                            let shifted =
                                format!("({} {symbol} {amount})", bits(&left_value, &left.ty));
                            if signed {
                                format!("bitcast<i32>({shifted})")
                            } else {
                                shifted
                            }
                        }
                    } else if signed
                        && matches!(
                            operator,
                            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply
                        )
                    {
                        format!(
                            "bitcast<i32>(bitcast<u32>({left_value}) {symbol} bitcast<u32>({right_value}))"
                        )
                    } else {
                        format!("({left_value} {symbol} {right_value})")
                    }
                }
            }
            TypedExprKind::Cast(operand) => {
                let value = self.expression(operand)?;
                match (&operand.ty, &expression.ty) {
                    (source, target) if source == target => value,
                    (Type::Integer(32, true), Type::Integer(32, false)) => {
                        format!("bitcast<u32>({value})")
                    }
                    (Type::Integer(32, false), Type::Integer(32, true)) => {
                        format!("bitcast<i32>({value})")
                    }
                    _ => {
                        return Err(unsupported(
                            "WGSL kernel casts require 32-bit integer operands",
                            span,
                        ));
                    }
                }
            }
            TypedExprKind::Block { bindings, result } => {
                for (local, initializer) in bindings {
                    let value = self.expression(initializer)?;
                    if local.ty != Type::Unit {
                        let kind = if local.mutable { "var" } else { "let" };
                        let _ = writeln!(
                            self.text,
                            "{kind} local_{}: {} = {value};",
                            local.id,
                            wgsl_type(&local.ty, local.span)?
                        );
                    }
                }
                self.expression(result)?
            }
            TypedExprKind::Assign(place, value) => {
                let TypedExprKind::Local(id) = &place.kind else {
                    return Err(unsupported("GPU assignment requires a scalar local", span));
                };
                let value = self.expression(value)?;
                let _ = writeln!(self.text, "local_{id} = {value};");
                return Ok(String::new());
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition)?;
                let name = self.fresh();
                if expression.ty != Type::Unit {
                    let _ = writeln!(
                        self.text,
                        "var {name}: {};",
                        wgsl_type(&expression.ty, span)?
                    );
                }
                let _ = writeln!(self.text, "if ({condition}) {{");
                let value = self.expression(then_branch)?;
                if expression.ty != Type::Unit {
                    let _ = writeln!(self.text, "{name} = {value};");
                }
                self.text.push_str("} else {\n");
                let value = self.expression(else_branch)?;
                if expression.ty != Type::Unit {
                    let _ = writeln!(self.text, "{name} = {value};");
                }
                self.text.push_str("}\n");
                name
            }
            TypedExprKind::Call(callee, arguments) => {
                let id = match &callee.kind {
                    TypedExprKind::Function(FunctionRef::User(id))
                    | TypedExprKind::Closure(id, _) => id,
                    _ => return Err(unsupported("GPU calls require known functions", span)),
                };
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                format!("kernel_{id}({})", arguments.join(", "))
            }
            _ => {
                return Err(unsupported(
                    "expression is not supported by strict WGSL generation",
                    span,
                ));
            }
        };
        if expression.ty == Type::Unit {
            return Ok(String::new());
        }
        let name = self.fresh();
        let _ = writeln!(
            self.text,
            "let {name}: {} = {value};",
            wgsl_type(&expression.ty, span)?
        );
        Ok(name)
    }
}
