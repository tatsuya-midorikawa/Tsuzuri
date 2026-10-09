use std::collections::BTreeSet;
use std::fmt::Write;

use rustc_apfloat::ieee::{Half, Single};
use rustc_apfloat::{Float, FloatConvert};

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
        Type::Bool | Type::Integer(32 | 64, _) | Type::Binary(16 | 32 | 64)
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
            matches!(
                name,
                "init"
                    | "map"
                    | "init_relaxed"
                    | "map_relaxed"
                    | "init_on"
                    | "map_on"
                    | "from_array"
            )
            .then_some((id, name))
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
                    "GPU operations cannot escape as function values; use direct full application",
                    expression.span,
                ));
            }
            if let TypedExprKind::Call(callee, arguments) = &expression.kind
                && let TypedExprKind::Function(FunctionRef::User(id)) = &callee.kind
                && let Some(name) = apis.get(id)
            {
                if arguments.len() != module.functions[*id].signature.parameters.len() {
                    return Err(unsupported(
                        "GPU operations require direct full application",
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
                    let callback = &arguments[if name.starts_with("init") { 2 } else { 1 }];
                    let target = match &callback.kind {
                        TypedExprKind::Function(FunctionRef::User(id)) => *id,
                        TypedExprKind::Closure(id, captures) if captures.is_empty() => *id,
                        _ => {
                            return Err(unsupported(
                                "GPU init/map operations require a known function or capture-free lambda",
                                callback.span,
                            ));
                        }
                    };
                    let kernel = extract_kernel(module, target)?;
                    // A device-aware call carries whether it was relaxed in its kernel number.
                    let relaxed = name.ends_with("_relaxed")
                        || arguments
                            .last()
                            .and_then(crate::gpu_devices::marked)
                            .unwrap_or(false);
                    if relaxed {
                        kernel.wgsl_relaxed()?;
                    }
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

    /// The strict WGSL of the kernel: 32-bit integer lanes that match the CPU reference bit for bit.
    pub fn wgsl(&self) -> Result<String, Diagnostic> {
        self.emit_wgsl(false)
    }

    /// The relaxed WGSL of the kernel (F09): f32, i32, and i32u lanes, evaluated with the
    /// floating-point freedoms of WGSL. Its first line names the contract.
    pub fn wgsl_relaxed(&self) -> Result<String, Diagnostic> {
        self.emit_wgsl(true)
    }

    fn emit_wgsl(&self, relaxed: bool) -> Result<String, Diagnostic> {
        let root = &self.module.functions[self.function];
        if !relaxed
            && (!matches!(root.signature.parameters[0], Type::Integer(32, _))
                || !matches!(root.signature.result, Type::Integer(32, _)))
        {
            return Err(unsupported(
                "strict WebGPU kernels require i32 or i32u buffer lanes; 64-bit lanes are unavailable and f32 or f16 lanes need --emit wgsl-relaxed",
                root.span,
            ));
        }
        let input = wgsl_type(&root.signature.parameters[0], relaxed, root.span)?;
        let output = wgsl_type(&root.signature.result, relaxed, root.span)?;
        let mut text = String::new();
        if relaxed {
            if input == "bool" || output == "bool" {
                return Err(unsupported(RELAXED_BOOL_LANE, root.span));
            }
            let _ = writeln!(
                text,
                "// tsuzuri-gpu float=relaxed input={input} output={output}"
            );
        }
        let mut body = String::new();
        let _ = write!(
            body,
            "struct Params {{ length: u32 }}\n@group(0) @binding(0) var<storage, read> input_values: array<{input}>;\n@group(0) @binding(1) var<storage, read_write> output_values: array<{output}>;\n@group(0) @binding(2) var<uniform> params: Params;\n"
        );
        for id in &self.functions {
            let function = &self.module.functions[*id];
            let mut emitter = Wgsl {
                text: String::new(),
                next: 0,
                relaxed,
                depth: 1,
            };
            let mut parameters = Vec::new();
            for local in &function.parameters {
                let ty = wgsl_type(&local.ty, relaxed, local.span)?;
                if local.mutable {
                    // A WGSL parameter cannot be assigned: the body gets a variable of the local's name.
                    parameters.push(format!("param_{}: {ty}", local.id));
                    emitter.emit(
                        local.span,
                        &format!("var local_{0}: {ty} = param_{0};", local.id),
                    )?;
                } else {
                    parameters.push(format!("local_{}: {ty}", local.id));
                }
            }
            let parameters = parameters.join(", ");
            let result = wgsl_type(&function.signature.result, relaxed, function.span)?;
            let value = emitter.expression(&function.body)?;
            let _ = writeln!(
                body,
                "fn kernel_{id}({parameters}) -> {result} {{\n{}return {value};\n}}",
                emitter.text
            );
        }
        let id = self.function;
        let _ = writeln!(
            body,
            "@compute @workgroup_size(256)\nfn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}(input_values[invocation.x]); }}\n}}\n@compute @workgroup_size(256)\nfn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {{\nif (invocation.x < params.length) {{ output_values[invocation.x] = kernel_{id}({input}(invocation.x)); }}\n}}"
        );
        // WGSL wants `enable f16;` before the first declaration, so it follows once the whole kernel is known.
        if body.contains("f16") {
            text.push_str("enable f16;\n");
        }
        text.push_str(&body);
        Ok(text)
    }
}

const RELAXED_TYPES: &str = "relaxed WebGPU kernels support f16, f32, i32, and i32u values; WGSL has no f64 or 64-bit integers";
const RELAXED_BOOL_LANE: &str =
    "relaxed WebGPU buffer lanes must be f16, f32, i32, or i32u; bool lanes are unavailable";
const RELAXED_CASTS: &str =
    "relaxed WGSL cannot reproduce Tsuzuri float-to-integer or f64 casts; compute them on the CPU";

fn wgsl_type(ty: &Type, relaxed: bool, span: Span) -> Result<&'static str, Diagnostic> {
    match ty {
        Type::Integer(32, true) => Ok("i32"),
        Type::Integer(32, false) => Ok("u32"),
        Type::Bool => Ok("bool"),
        Type::Binary(32) if relaxed => Ok("f32"),
        Type::Binary(16) if relaxed => Ok("f16"),
        _ if relaxed => Err(unsupported(RELAXED_TYPES, span)),
        _ => Err(unsupported(
            "WGSL cannot preserve this scalar type; use the explicit CPU reference path",
            span,
        )),
    }
}

/// The f32 bit pattern of a literal. The checker keeps an f32 literal as the LLVM hexadecimal
/// of the f64 that holds it, so the conversion is exact.
fn f32_bits(text: &str) -> u32 {
    let bits = u64::from_str_radix(text.trim_start_matches("0x"), 16)
        .expect("a float literal is an LLVM hexadecimal constant");
    (f64::from_bits(bits) as f32).to_bits()
}

/// The f32 bit pattern of the same value as an f16 literal, which the checker keeps as the decimal
/// number of its 16 bits. Every f16 value is an f32 value, so the conversion is exact.
fn f16_bits_as_f32(text: &str) -> u32 {
    let bits: u128 = text
        .parse()
        .expect("an f16 literal is the decimal number of its bits");
    let widened: Single = Half::from_bits(bits).convert(&mut false).value;
    widened.to_bits() as u32
}

fn half_or_single(ty: &Type) -> bool {
    matches!(ty, Type::Binary(16 | 32))
}

/// The deepest statement nesting that a WGSL implementation accepts (Tint: "statement nesting depth /
/// chaining length exceeds limit of 127"). A function body is depth 1, a statement is one deeper than the
/// block that holds it, and the blocks of an `if` are one deeper than the `if`, so every `if` (also each
/// `else if` arm, which this emitter nests in the `else` block) and every right operand of `&&` or `||`
/// costs two levels.
const MAX_STATEMENT_DEPTH: usize = 127;

fn too_deep(span: Span) -> Diagnostic {
    Diagnostic::new(
        "E1017",
        format!(
            "GPU kernel exceeds {MAX_STATEMENT_DEPTH} levels of WGSL statement nesting; each nested if, else-if arm, and right operand of && or || takes two levels, so move the rest of a long chain into a function"
        ),
        span,
    )
}

struct Wgsl {
    text: String,
    next: usize,
    relaxed: bool,
    /// The depth of the block that the next statement goes into: 1 for the function body.
    depth: usize,
}

impl Wgsl {
    fn fresh(&mut self) -> String {
        let name = format!("value_{}", self.next);
        self.next += 1;
        name
    }

    /// Writes statements (one per line) into the current block, unless WGSL cannot nest them that deep.
    fn emit(&mut self, span: Span, lines: &str) -> Result<(), Diagnostic> {
        if self.depth + 1 > MAX_STATEMENT_DEPTH {
            return Err(too_deep(span));
        }
        self.text.push_str(lines);
        self.text.push('\n');
        Ok(())
    }

    /// Enters the blocks of an `if` whose statement was just written.
    fn enter(&mut self, span: Span) -> Result<(), Diagnostic> {
        self.depth += 2;
        if self.depth > MAX_STATEMENT_DEPTH {
            return Err(too_deep(span));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 2;
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
            TypedExprKind::Float(text) if self.relaxed && expression.ty == Type::Binary(32) => {
                format!("bitcast<f32>({}u)", f32_bits(text))
            }
            TypedExprKind::Float(text) if self.relaxed && expression.ty == Type::Binary(16) => {
                format!("f16(bitcast<f32>({}u))", f16_bits_as_f32(text))
            }
            TypedExprKind::Unit => return Ok(String::new()),
            TypedExprKind::Local(id) => format!("local_{id}"),
            TypedExprKind::Unary(operator, operand) => {
                let value = self.expression(operand)?;
                match operator {
                    UnaryOp::Negate if expression.ty == Type::Integer(32, true) => {
                        format!("bitcast<i32>(0u - bitcast<u32>({value}))")
                    }
                    UnaryOp::Negate if self.relaxed && half_or_single(&expression.ty) => {
                        format!("(-{value})")
                    }
                    UnaryOp::Negate => format!("(0u - {value})"),
                    UnaryOp::Not => format!("(!{value})"),
                    UnaryOp::BitNot => format!("(~{value})"),
                    UnaryOp::Plus => unreachable!("unary plus is checked to its operand"),
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
                    self.emit(
                        span,
                        &format!("var {name}: bool = {left_value};\nif ({condition}) {{"),
                    )?;
                    self.enter(span)?;
                    let right_value = self.expression(right)?;
                    self.emit(span, &format!("{name} = {right_value};"))?;
                    self.text.push_str("}\n");
                    self.leave();
                    name
                } else {
                    let right_value = self.expression(right)?;
                    let signed = left.ty == Type::Integer(32, true);
                    let float = self.relaxed && half_or_single(&left.ty);
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
                        BinaryOp::Divide if float => "/",
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
                        BinaryOp::Power => {
                            return Err(unsupported(
                                "WGSL kernels do not support '**'; multiply explicitly",
                                span,
                            ));
                        }
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
                    (Type::Integer(32, _) | Type::Binary(16), Type::Binary(32)) if self.relaxed => {
                        format!("f32({value})")
                    }
                    (Type::Integer(32, _) | Type::Binary(32), Type::Binary(16)) if self.relaxed => {
                        format!("f16({value})")
                    }
                    (Type::Binary(_), _) | (_, Type::Binary(_)) if self.relaxed => {
                        return Err(unsupported(RELAXED_CASTS, span));
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
                        let declaration = format!(
                            "{kind} local_{}: {} = {value};",
                            local.id,
                            wgsl_type(&local.ty, self.relaxed, local.span)?
                        );
                        self.emit(local.span, &declaration)?;
                    }
                }
                self.expression(result)?
            }
            TypedExprKind::Assign(place, value) => {
                let TypedExprKind::Local(id) = &place.kind else {
                    return Err(unsupported("GPU assignment requires a scalar local", span));
                };
                let value = self.expression(value)?;
                self.emit(span, &format!("local_{id} = {value};"))?;
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
                    let declaration = format!(
                        "var {name}: {};",
                        wgsl_type(&expression.ty, self.relaxed, span)?
                    );
                    self.emit(span, &declaration)?;
                }
                self.emit(span, &format!("if ({condition}) {{"))?;
                self.enter(span)?;
                let value = self.expression(then_branch)?;
                if expression.ty != Type::Unit {
                    self.emit(span, &format!("{name} = {value};"))?;
                }
                self.text.push_str("} else {\n");
                let value = self.expression(else_branch)?;
                if expression.ty != Type::Unit {
                    self.emit(span, &format!("{name} = {value};"))?;
                }
                self.text.push_str("}\n");
                self.leave();
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
                    if self.relaxed {
                        "expression is not supported by relaxed WGSL generation"
                    } else {
                        "expression is not supported by strict WGSL generation"
                    },
                    span,
                ));
            }
        };
        if expression.ty == Type::Unit {
            return Ok(String::new());
        }
        let name = self.fresh();
        let declaration = format!(
            "let {name}: {} = {value};",
            wgsl_type(&expression.ty, self.relaxed, span)?
        );
        self.emit(span, &declaration)?;
        Ok(name)
    }
}
