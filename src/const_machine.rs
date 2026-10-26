//! The explicit-stack evaluator behind constants and `const def` calls (D11).
//!
//! It runs a typed body by value, like the code that the compiler emits for it, without Rust
//! recursion: pending work is a stack of `Task`s, intermediate results are a stack of `Value`s, and
//! a call is a `Frame`. Every step and every copy of a value is charged to the budget that the
//! whole program shares, so a program that does not end, or that builds too much, stops with
//! `E1026` instead of exhausting the compiler.

use super::*;

/// A value of the evaluator: a literal tree of `Int`, `Float`, `String`, `Bool`, `Unit`, `Array`,
/// `Tuple`, `Record`, and `Construct` nodes, and its measures.
#[derive(Clone)]
pub(super) struct Value {
    pub(super) expr: TypedExpr,
    /// The nodes of the tree.
    pub(super) size: usize,
    /// The levels of the tree; a leaf is one.
    pub(super) depth: usize,
}

impl Value {
    fn leaf(kind: TypedExprKind, template: &TypedExpr) -> Self {
        Self {
            expr: TypedExpr {
                kind,
                ty: template.ty.clone(),
                span: template.span,
            },
            size: 1,
            depth: 1,
        }
    }

    /// A value of a tree that is already built.
    pub(super) fn measured(expr: TypedExpr) -> Self {
        let (size, depth) = measure(&expr);
        Self { expr, size, depth }
    }

    fn relabel(mut self, template: &TypedExpr) -> Self {
        self.expr.ty = template.ty.clone();
        self.expr.span = template.span;
        self
    }
}

/// The nodes and the levels of a literal tree.
fn measure(expression: &TypedExpr) -> (usize, usize) {
    let (mut size, mut depth) = (0, 0);
    let mut pending = vec![(expression, 1)];
    while let Some((node, level)) = pending.pop() {
        size += 1;
        depth = depth.max(level);
        pending.extend(node.children().into_iter().map(|child| (child, level + 1)));
    }
    (size, depth)
}

struct Frame {
    locals: BTreeMap<usize, Value>,
    /// The height of the value stack when the call began, without its arguments.
    stack: usize,
}

/// The elements that a `for` loop still enumerates.
enum Source {
    Items(std::vec::IntoIter<TypedExpr>),
    Units(std::vec::IntoIter<u16>),
    Bytes(std::vec::IntoIter<u8>),
}

enum Loop<'a> {
    While(&'a TypedExpr),
    /// The next value of the loop variable, and whether it is still inside the range.
    Range {
        expression: &'a TypedExpr,
        current: u128,
        step: u128,
        finish: u128,
        active: bool,
    },
    Each {
        expression: &'a TypedExpr,
        source: Source,
    },
}

/// A position in a `match`: the alternative of the arm that is being tried, and the step of it.
#[derive(Clone, Copy)]
struct Cursor<'a> {
    expression: &'a TypedExpr,
    arm: usize,
    alternative: usize,
    step: usize,
}

enum Task<'a> {
    /// Evaluates the expression and pushes its value.
    Eval(&'a TypedExpr),
    /// Combines the values of the operands of the expression, which are on the stack.
    Finish(&'a TypedExpr),
    /// Gives the value on top of the stack the type and the position of the expression.
    Relabel(&'a TypedExpr),
    /// Chooses the branch of an `if` by the condition on the stack.
    Branch(&'a TypedExpr),
    /// Evaluates the right operand of `&&` or `||` unless the left value on the stack decides.
    Logic(&'a TypedExpr),
    /// Binds the value on the stack to a local, which the expression has no value of.
    Bind(usize),
    /// Stores the value on the stack in a local and pushes unit.
    Assign(usize, &'a TypedExpr),
    /// Drops the value on top of the stack.
    Discard,
    /// Ends a call: the result stays, the frame goes.
    Return,
    /// Starts the call of a `const def` whose arguments are on the stack.
    Invoke(&'a TypedExpr),
    /// Reads an element, field, or length of a local in place; its index operands are on the stack.
    Place(&'a TypedExpr),
    /// The loop that `break` and `continue` leave or restart, and the stack height at its start.
    Loop(Loop<'a>, usize),
    /// Tests the condition of a `while` loop on the stack.
    WhileTest(&'a TypedExpr),
    RangeStart(&'a TypedExpr),
    EachStart(&'a TypedExpr),
    /// Starts `new [T](n, f)` with the length on the stack.
    FillStart(&'a TypedExpr),
    /// Computes element `next` of `new [T](n, f)`; the elements so far are above `base`.
    Fill {
        expression: &'a TypedExpr,
        next: usize,
        length: usize,
        base: usize,
    },
    /// Binds the subject of a `match`, which is on the stack, and tries the first arm.
    Subject(&'a TypedExpr),
    Arm(Cursor<'a>),
    Test(Cursor<'a>),
    Guard(Cursor<'a>),
}

/// The last step into a place, and what the read yields.
#[derive(Clone, Copy)]
enum Read {
    Value,
    Length,
    StringLength,
    Tag,
}

enum Step<'a> {
    Field(usize),
    Payload,
    Index(&'a TypedExpr),
}

pub(super) struct Machine<'a> {
    pub(super) functions: &'a [CheckedFunction],
    pub(super) constants: &'a BTreeSet<usize>,
    pub(super) const_functions: &'a BTreeSet<usize>,
    pub(super) values: &'a BTreeMap<usize, Value>,
    pub(super) types: TypeContext<'a>,
    pub(super) remaining: &'a mut usize,
}

/// One run of the machine on one body.
struct Run<'a, 'm> {
    machine: &'m mut Machine<'a>,
    tasks: Vec<Task<'a>>,
    stack: Vec<Value>,
    frames: Vec<Frame>,
}

fn trapped(span: Span, message: &str) -> Diagnostic {
    failure(span, format!("const evaluation trapped: {message}"))
}

fn unsupported(span: Span) -> Diagnostic {
    failure(
        span,
        "this operation is not allowed in a const def; move it to a runtime def",
    )
}

impl<'a> Machine<'a> {
    /// Evaluates `body`, the initializer of a constant, as a function of no parameters.
    pub(super) fn run(&mut self, body: &'a TypedExpr) -> Result<Value, Diagnostic> {
        let mut run = Run {
            machine: self,
            tasks: vec![Task::Eval(body)],
            stack: Vec::new(),
            frames: vec![Frame {
                locals: BTreeMap::new(),
                stack: 0,
            }],
        };
        while let Some(task) = run.tasks.pop() {
            run.step(task)?;
        }
        let value = run.stack.pop();
        debug_assert!(run.stack.is_empty() && run.frames.len() == 1);
        value.ok_or_else(|| unsupported(body.span))
    }
}

impl<'a> Run<'a, '_> {
    fn charge(&mut self, amount: usize, span: Span) -> Result<(), Diagnostic> {
        match self.machine.remaining.checked_sub(amount) {
            Some(rest) => {
                *self.machine.remaining = rest;
                Ok(())
            }
            None => Err(limit(span)),
        }
    }

    fn pop(&mut self, span: Span) -> Result<Value, Diagnostic> {
        self.stack.pop().ok_or_else(|| unsupported(span))
    }

    fn take(&mut self, count: usize) -> Vec<Value> {
        let start = self.stack.len() - count;
        self.stack.split_off(start)
    }

    fn step(&mut self, task: Task<'a>) -> Result<(), Diagnostic> {
        match task {
            Task::Eval(expression) => self.eval(expression),
            Task::Finish(expression) => self.finish(expression),
            Task::Relabel(expression) => {
                let value = self.pop(expression.span)?;
                self.stack.push(value.relabel(expression));
                Ok(())
            }
            Task::Branch(expression) => self.branch(expression),
            Task::Logic(expression) => self.logic(expression),
            Task::Bind(id) => {
                let value = self.pop(Span::default())?;
                self.bind(id, value);
                Ok(())
            }
            Task::Assign(id, expression) => {
                let value = self.pop(expression.span)?;
                self.bind(id, value);
                self.unit(expression);
                Ok(())
            }
            Task::Discard => {
                self.stack.pop();
                Ok(())
            }
            Task::Return => {
                let result = self.pop(Span::default())?;
                let frame = self.frames.pop().expect("a call has a frame");
                self.stack.truncate(frame.stack);
                self.stack.push(result);
                Ok(())
            }
            Task::Invoke(expression) => self.invoke(expression),
            Task::Place(expression) => self.read_place(expression),
            Task::Loop(state, height) => self.run_loop(state, height),
            Task::WhileTest(expression) => self.while_test(expression),
            Task::RangeStart(expression) => self.range_start(expression),
            Task::EachStart(expression) => self.each_start(expression),
            Task::FillStart(expression) => self.fill_start(expression),
            Task::Fill {
                expression,
                next,
                length,
                base,
            } => self.fill(expression, next, length, base),
            Task::Subject(expression) => self.subject(expression),
            Task::Arm(cursor) => self.arm(cursor),
            Task::Test(cursor) => self.test(cursor),
            Task::Guard(cursor) => self.guard(cursor),
        }
    }

    fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("the root frame stays")
    }

    fn bind(&mut self, id: usize, value: Value) {
        self.frame().locals.insert(id, value);
    }

    fn unit(&mut self, template: &TypedExpr) {
        self.stack.push(Value::leaf(TypedExprKind::Unit, template));
    }

    #[inline(never)]
    fn eval(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        use TypedExprKind as K;
        let span = expression.span;
        self.charge(1, span)?;
        if let Some(id) = constant_id(expression).filter(|id| self.machine.constants.contains(id)) {
            return self.constant(id, span);
        }
        match &expression.kind {
            K::Int(_) | K::Float(_) | K::String(_) | K::Bool(_) | K::Unit => {
                self.stack
                    .push(Value::leaf(expression.kind.clone(), expression));
            }
            // A literal whose type inference settled; specialization would write it like this.
            K::GenericInteger(value, negative) => {
                let (kind, _) = Checker::integer_literal(
                    *value,
                    None,
                    Some(&expression.ty),
                    *negative,
                    span,
                    &self.machine.types,
                )?;
                self.stack.push(Value::leaf(kind, expression));
            }
            K::GenericFloat(value) => {
                let value = crate::numeric::float_literal(value, &expression.ty, span)?;
                self.stack
                    .push(Value::leaf(TypedExprKind::Float(value), expression));
            }
            K::Local(id) => {
                let size = self
                    .frame()
                    .locals
                    .get(id)
                    .map(|value| value.size)
                    .ok_or_else(|| unsupported(span))?;
                self.charge(size, span)?;
                let value = self.frame().locals[id].clone();
                self.stack.push(value.relabel(expression));
            }
            K::Unary(_, operand) | K::Cast(operand) | K::NewLiteral(operand) => {
                self.tasks.push(Task::Finish(expression));
                self.tasks.push(Task::Eval(operand));
            }
            K::Binary(BinaryOp::Pipe, ..) => return Err(unsupported(span)),
            K::Binary(BinaryOp::And | BinaryOp::Or, left, _) => {
                self.tasks.push(Task::Logic(expression));
                self.tasks.push(Task::Eval(left));
            }
            K::Binary(_, left, right) => {
                self.tasks.push(Task::Finish(expression));
                self.tasks.push(Task::Eval(right));
                self.tasks.push(Task::Eval(left));
            }
            K::If { condition, .. } => {
                self.tasks.push(Task::Branch(expression));
                self.tasks.push(Task::Eval(condition));
            }
            K::Block { bindings, result } => {
                self.tasks.push(Task::Relabel(expression));
                self.tasks.push(Task::Eval(result));
                for (local, value) in bindings.iter().rev() {
                    self.tasks.push(Task::Bind(local.id));
                    self.tasks.push(Task::Eval(value));
                }
            }
            K::While { .. } => {
                let height = self.stack.len();
                self.tasks.push(Task::Loop(Loop::While(expression), height));
            }
            K::ForRange {
                start,
                step,
                finish,
                ..
            } => {
                self.tasks.push(Task::RangeStart(expression));
                self.tasks.push(Task::Eval(finish));
                self.tasks.push(Task::Eval(step));
                self.tasks.push(Task::Eval(start));
            }
            K::ForEach { source, .. } => {
                self.tasks.push(Task::EachStart(expression));
                self.tasks.push(Task::Eval(source));
            }
            K::Match { value, .. } => {
                self.tasks.push(Task::Subject(expression));
                self.tasks.push(Task::Eval(value));
            }
            K::Break | K::Continue => {
                return self.jump(matches!(expression.kind, K::Break), expression);
            }
            K::Assign(target, value) => {
                let K::Local(id) = target.kind else {
                    return Err(unsupported(span));
                };
                self.tasks.push(Task::Assign(id, expression));
                self.tasks.push(Task::Eval(value));
            }
            K::NewArray(length, _) => {
                self.tasks.push(Task::FillStart(expression));
                self.tasks.push(Task::Eval(length));
            }
            K::Call(callee, arguments) => {
                let K::Function(FunctionRef::User(id)) = callee.kind else {
                    return Err(unsupported(span));
                };
                if !self.machine.const_functions.contains(&id)
                    || arguments.len() != self.machine.functions[id].parameters.len()
                {
                    return Err(unsupported(span));
                }
                self.tasks.push(Task::Invoke(expression));
                for argument in arguments.iter().rev() {
                    self.tasks.push(Task::Eval(argument));
                }
            }
            K::Index(..)
            | K::Field(..)
            | K::Length(_)
            | K::StringLength(_)
            | K::UnionTag(_)
            | K::UnionPayload { .. } => return self.eval_projection(expression),
            K::Array(values) | K::Tuple(values) => {
                self.tasks.push(Task::Finish(expression));
                for value in values.iter().rev() {
                    self.tasks.push(Task::Eval(value));
                }
            }
            K::Record(fields) => {
                self.tasks.push(Task::Finish(expression));
                for (_, value) in fields.iter().rev() {
                    self.tasks.push(Task::Eval(value));
                }
            }
            K::RecordUpdate { base, fields } => {
                self.tasks.push(Task::Finish(expression));
                for (_, value) in fields.iter().rev() {
                    self.tasks.push(Task::Eval(value));
                }
                self.tasks.push(Task::Eval(base));
            }
            K::Construct { payload, .. } => {
                self.tasks.push(Task::Finish(expression));
                if let Some(payload) = payload {
                    self.tasks.push(Task::Eval(payload));
                }
            }
            K::Error
            | K::Function(_)
            | K::GenericFunction(..)
            | K::Method(..)
            | K::TypeFunction { .. }
            | K::HostCall(..)
            | K::Lambda { .. }
            | K::Closure(..)
            | K::TaskRun(_)
            | K::TaskParallel(_)
            | K::TaskParallelResults(_)
            | K::Parallel(..)
            | K::StructuralCompare(..)
            | K::StructuralHash(_)
            | K::StructuralDisplay(_)
            | K::Interpolated(_)
            | K::List(_)
            | K::ListTail(..)
            | K::NewList(..)
            | K::Slice { .. }
            | K::Borrow(..)
            | K::BorrowOperand(_)
            | K::Dereference(_)
            | K::CaseConstructor { .. }
            | K::Checked(_)
            | K::Try(_)
            | K::RaisedException
            | K::Reraise
            | K::DynDispatch { .. }
            | K::Static(_) => return Err(unsupported(span)),
        }
        Ok(())
    }

    fn constant(&mut self, id: usize, span: Span) -> Result<(), Diagnostic> {
        let Some(value) = self.machine.values.get(&id) else {
            return Err(failure(
                span,
                "cyclic const definition; break the cycle or make one value a function",
            ));
        };
        let value = value.clone();
        self.charge(value.size, span)?;
        self.stack.push(value);
        Ok(())
    }

    /// Combines the values of the operands of `expression`, which `eval` evaluated left to right.
    #[inline(never)]
    fn finish(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        use TypedExprKind as K;
        let span = expression.span;
        match &expression.kind {
            K::Unary(operator, _) => {
                let operand = self.pop(span)?;
                let kind = unary(*operator, &operand.expr, span)?;
                self.stack.push(Value::leaf(kind, expression));
            }
            K::Cast(_) => {
                let operand = self.pop(span)?;
                let kind = cast(&operand.expr, &expression.ty, span)?;
                self.stack.push(Value::leaf(kind, expression));
            }
            K::Binary(operator, ..) => {
                let right = self.pop(span)?;
                let left = self.pop(span)?;
                let kind = binary(*operator, &left.expr, &right.expr, span)?;
                self.stack.push(Value::leaf(kind, expression));
            }
            K::NewLiteral(_) => {
                let value = self.pop(span)?;
                self.stack.push(value.relabel(expression));
            }
            K::Array(values) => {
                let items = self.take(values.len());
                let value = self.aggregate(expression, items, TypedExprKind::Array)?;
                self.stack.push(value);
            }
            K::Tuple(values) => {
                let items = self.take(values.len());
                let value = self.aggregate(expression, items, TypedExprKind::Tuple)?;
                self.stack.push(value);
            }
            K::Record(fields) => {
                let items = self.take(fields.len());
                let indices: Vec<usize> = fields.iter().map(|(index, _)| *index).collect();
                let value = self.aggregate(expression, items, |children| {
                    TypedExprKind::Record(indices.into_iter().zip(children).collect())
                })?;
                self.stack.push(value);
            }
            K::RecordUpdate { fields, .. } => {
                let updates = self.take(fields.len());
                let base = self.pop(span)?;
                let TypedExprKind::Record(existing) = base.expr.kind else {
                    return Err(unsupported(span));
                };
                let mut updates: BTreeMap<usize, Value> = fields
                    .iter()
                    .map(|(index, _)| *index)
                    .zip(updates)
                    .collect();
                let (indices, items): (Vec<usize>, Vec<Value>) = existing
                    .into_iter()
                    .map(|(index, value)| match updates.remove(&index) {
                        Some(update) => (index, update),
                        None => (index, Value::measured(value)),
                    })
                    .unzip();
                let value = self.aggregate(expression, items, |children| {
                    TypedExprKind::Record(indices.into_iter().zip(children).collect())
                })?;
                self.stack.push(value);
            }
            K::Construct {
                union_id,
                case_id,
                payload,
            } => {
                let items = self.take(usize::from(payload.is_some()));
                let (union_id, case_id, present) = (*union_id, *case_id, payload.is_some());
                let value =
                    self.aggregate(expression, items, |mut children| TypedExprKind::Construct {
                        union_id,
                        case_id,
                        payload: present.then(|| Box::new(children.remove(0))),
                    })?;
                self.stack.push(value);
            }
            K::Index(..)
            | K::Field(..)
            | K::Length(_)
            | K::StringLength(_)
            | K::UnionTag(_)
            | K::UnionPayload { .. } => return self.finish_projection(expression),
            _ => return Err(unsupported(span)),
        }
        Ok(())
    }

    /// An aggregate of `items`, whose tree may not nest deeper than the compiler allows.
    fn aggregate(
        &mut self,
        expression: &TypedExpr,
        items: Vec<Value>,
        make: impl FnOnce(Vec<TypedExpr>) -> TypedExprKind,
    ) -> Result<Value, Diagnostic> {
        let depth = 1 + items.iter().map(|item| item.depth).max().unwrap_or(0);
        if depth > MAX_NESTING {
            return Err(limit(expression.span));
        }
        let size = 1 + items.iter().map(|item| item.size).sum::<usize>();
        let children = items.into_iter().map(|item| item.expr).collect();
        Ok(Value {
            expr: TypedExpr {
                kind: make(children),
                ty: expression.ty.clone(),
                span: expression.span,
            },
            size,
            depth,
        })
    }

    fn truth(&mut self, span: Span) -> Result<bool, Diagnostic> {
        match self.pop(span)?.expr.kind {
            TypedExprKind::Bool(value) => Ok(value),
            _ => Err(failure(span, "a const condition must be bool")),
        }
    }

    fn branch(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::If {
            then_branch,
            else_branch,
            ..
        } = &expression.kind
        else {
            return Err(unsupported(expression.span));
        };
        let chosen = if self.truth(expression.span)? {
            then_branch
        } else {
            else_branch
        };
        self.tasks.push(Task::Relabel(expression));
        self.tasks.push(Task::Eval(chosen));
        Ok(())
    }

    fn logic(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::Binary(operator, _, right) = &expression.kind else {
            return Err(unsupported(expression.span));
        };
        let left = self.pop(expression.span)?;
        let decided = matches!(
            (operator, &left.expr.kind),
            (BinaryOp::And, TypedExprKind::Bool(false)) | (BinaryOp::Or, TypedExprKind::Bool(true))
        );
        if decided {
            self.stack.push(left.relabel(expression));
        } else {
            self.tasks.push(Task::Relabel(expression));
            self.tasks.push(Task::Eval(right));
        }
        Ok(())
    }

    fn invoke(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::Call(callee, arguments) = &expression.kind else {
            return Err(unsupported(expression.span));
        };
        let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
            return Err(unsupported(expression.span));
        };
        let arguments = self.take(arguments.len());
        self.call(id, arguments, expression.span)
    }

    fn call(&mut self, id: usize, arguments: Vec<Value>, span: Span) -> Result<(), Diagnostic> {
        if self.frames.len() > MAX_NESTING {
            return Err(failure(
                span,
                format!(
                    "const evaluation exceeds the compiler limit of {MAX_NESTING} nested const def calls; reduce the recursion depth or compute the value at runtime"
                ),
            ));
        }
        let functions: &'a [CheckedFunction] = self.machine.functions;
        let function = &functions[id];
        let locals = function
            .parameters
            .iter()
            .map(|parameter| parameter.id)
            .zip(arguments)
            .collect();
        self.frames.push(Frame {
            locals,
            stack: self.stack.len(),
        });
        self.tasks.push(Task::Return);
        self.tasks.push(Task::Eval(&function.body));
        Ok(())
    }

    /// `break` leaves the innermost loop and `continue` starts its next round; both drop the work
    /// that the round had pending.
    fn jump(&mut self, breaking: bool, expression: &TypedExpr) -> Result<(), Diagnostic> {
        while let Some(task) = self.tasks.pop() {
            if let Task::Loop(state, height) = task {
                self.stack.truncate(height);
                if breaking {
                    self.unit(expression);
                } else {
                    self.tasks.push(Task::Loop(state, height));
                }
                return Ok(());
            }
        }
        Err(unsupported(expression.span))
    }

    fn run_loop(&mut self, state: Loop<'a>, height: usize) -> Result<(), Diagnostic> {
        match state {
            Loop::While(expression) => {
                let TypedExprKind::While { condition, .. } = &expression.kind else {
                    return Err(unsupported(expression.span));
                };
                self.tasks.push(Task::Loop(Loop::While(expression), height));
                self.tasks.push(Task::WhileTest(expression));
                self.tasks.push(Task::Eval(condition));
            }
            Loop::Range {
                expression,
                current,
                step,
                finish,
                active,
            } => {
                let TypedExprKind::ForRange {
                    local,
                    body,
                    descending,
                    ..
                } = &expression.kind
                else {
                    return Err(unsupported(expression.span));
                };
                let Type::Integer(bits, signed) = local.ty else {
                    return Err(unsupported(expression.span));
                };
                if !active {
                    self.unit(expression);
                    return Ok(());
                }
                self.bind(
                    local.id,
                    Value::leaf(
                        TypedExprKind::Int(current),
                        &TypedExpr {
                            kind: TypedExprKind::Unit,
                            ty: local.ty.clone(),
                            span: expression.span,
                        },
                    ),
                );
                let (next, active) = advance(current, step, finish, bits, signed, *descending);
                self.tasks.push(Task::Loop(
                    Loop::Range {
                        expression,
                        current: next,
                        step,
                        finish,
                        active,
                    },
                    height,
                ));
                self.tasks.push(Task::Discard);
                self.tasks.push(Task::Eval(body));
            }
            Loop::Each { expression, source } => {
                let TypedExprKind::ForEach { local, body, .. } = &expression.kind else {
                    return Err(unsupported(expression.span));
                };
                let mut source = source;
                let item = match &mut source {
                    Source::Items(items) => items.next().map(Value::measured),
                    Source::Units(units) => units.next().map(|unit| {
                        Value::leaf(
                            TypedExprKind::Int(u128::from(unit)),
                            &TypedExpr {
                                kind: TypedExprKind::Unit,
                                ty: local.ty.clone(),
                                span: expression.span,
                            },
                        )
                    }),
                    Source::Bytes(bytes) => bytes.next().map(|byte| {
                        Value::leaf(
                            TypedExprKind::Int(u128::from(byte)),
                            &TypedExpr {
                                kind: TypedExprKind::Unit,
                                ty: local.ty.clone(),
                                span: expression.span,
                            },
                        )
                    }),
                };
                let Some(item) = item else {
                    self.unit(expression);
                    return Ok(());
                };
                self.bind(local.id, item);
                self.tasks
                    .push(Task::Loop(Loop::Each { expression, source }, height));
                self.tasks.push(Task::Discard);
                self.tasks.push(Task::Eval(body));
            }
        }
        Ok(())
    }

    fn while_test(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::While { body, .. } = &expression.kind else {
            return Err(unsupported(expression.span));
        };
        if self.truth(expression.span)? {
            self.tasks.push(Task::Discard);
            self.tasks.push(Task::Eval(body));
        } else {
            self.tasks.pop();
            self.unit(expression);
        }
        Ok(())
    }

    fn range_start(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::ForRange {
            local, descending, ..
        } = &expression.kind
        else {
            return Err(unsupported(expression.span));
        };
        let Type::Integer(bits, signed) = local.ty else {
            return Err(unsupported(expression.span));
        };
        let (Some(finish), Some(step), Some(start)) = (
            self.stack.pop().and_then(|value| integer(&value.expr)),
            self.stack.pop().and_then(|value| integer(&value.expr)),
            self.stack.pop().and_then(|value| integer(&value.expr)),
        ) else {
            return Err(unsupported(expression.span));
        };
        if step == 0 {
            return Err(trapped(
                expression.span,
                "range step is zero; use a nonzero step",
            ));
        }
        let height = self.stack.len();
        self.tasks.push(Task::Loop(
            Loop::Range {
                expression,
                current: start,
                step,
                finish,
                active: within(start, step, finish, bits, signed, *descending),
            },
            height,
        ));
        Ok(())
    }

    fn each_start(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let source = self.pop(expression.span)?;
        let source = match source.expr.kind {
            TypedExprKind::Array(items) => Source::Items(items.into_iter()),
            TypedExprKind::String(StringLiteral::Utf16(units)) => Source::Units(units.into_iter()),
            TypedExprKind::String(StringLiteral::Utf8(text)) => {
                Source::Bytes(text.into_bytes().into_iter())
            }
            _ => return Err(unsupported(expression.span)),
        };
        let height = self.stack.len();
        self.tasks
            .push(Task::Loop(Loop::Each { expression, source }, height));
        Ok(())
    }

    fn fill_start(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let span = expression.span;
        let length = self.pop(span)?;
        let Some(length) = integer(&length.expr) else {
            return Err(unsupported(span));
        };
        if length >> 63 != 0 {
            return Err(trapped(
                span,
                "negative array length; use a length of at least 0",
            ));
        }
        let length = length as usize;
        if length > *self.machine.remaining {
            return Err(limit(span));
        }
        self.tasks.push(Task::Fill {
            expression,
            next: 0,
            length,
            base: self.stack.len(),
        });
        Ok(())
    }

    fn fill(
        &mut self,
        expression: &'a TypedExpr,
        next: usize,
        length: usize,
        base: usize,
    ) -> Result<(), Diagnostic> {
        let span = expression.span;
        if next == length {
            let items = self.stack.split_off(base);
            let value = self.aggregate(expression, items, TypedExprKind::Array)?;
            self.stack.push(value);
            return Ok(());
        }
        let TypedExprKind::NewArray(_, initializer) = &expression.kind else {
            return Err(unsupported(span));
        };
        self.tasks.push(Task::Fill {
            expression,
            next: next + 1,
            length,
            base,
        });
        match &initializer.kind {
            TypedExprKind::Lambda {
                parameters, body, ..
            } if parameters.len() == 1 => {
                let index = Value::leaf(
                    TypedExprKind::Int(next as u128),
                    &TypedExpr {
                        kind: TypedExprKind::Unit,
                        ty: parameters[0].ty.clone(),
                        span,
                    },
                );
                self.bind(parameters[0].id, index);
                self.tasks.push(Task::Eval(body));
                Ok(())
            }
            TypedExprKind::Function(FunctionRef::User(id))
                if self.machine.const_functions.contains(id) =>
            {
                let index = Value::leaf(
                    TypedExprKind::Int(next as u128),
                    &TypedExpr {
                        kind: TypedExprKind::Unit,
                        ty: Type::I64,
                        span,
                    },
                );
                self.call(*id, vec![index], span)
            }
            _ => Err(unsupported(span)),
        }
    }

    fn subject(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let TypedExprKind::Match { local, .. } = &expression.kind else {
            return Err(unsupported(expression.span));
        };
        let value = self.pop(expression.span)?;
        self.bind(local.id, value);
        self.tasks.push(Task::Relabel(expression));
        self.tasks.push(Task::Arm(Cursor {
            expression,
            arm: 0,
            alternative: 0,
            step: 0,
        }));
        Ok(())
    }

    fn arms(cursor: Cursor<'a>) -> Result<&'a [TypedMatchArm], Diagnostic> {
        match &cursor.expression.kind {
            TypedExprKind::Match { arms, .. } => Ok(arms),
            _ => Err(unsupported(cursor.expression.span)),
        }
    }

    /// Tries the step `cursor.step` of the alternative: a failed test moves on to the next
    /// alternative, and then the next arm. An arm whose guard fails also moves on to the next arm.
    fn arm(&mut self, cursor: Cursor<'a>) -> Result<(), Diagnostic> {
        let arms = Self::arms(cursor)?;
        let Some(arm) = arms.get(cursor.arm) else {
            return Err(trapped(
                cursor.expression.span,
                "no pattern matched; handle every value or compute it at runtime",
            ));
        };
        let Some(alternative) = arm.alternatives.get(cursor.alternative) else {
            self.tasks.push(Task::Arm(Cursor {
                arm: cursor.arm + 1,
                alternative: 0,
                step: 0,
                ..cursor
            }));
            return Ok(());
        };
        match alternative.steps.get(cursor.step) {
            Some(PatternStep::Test(condition)) => {
                self.tasks.push(Task::Test(cursor));
                self.tasks.push(Task::Eval(condition));
            }
            Some(PatternStep::Bind(local, value)) => {
                self.tasks.push(Task::Arm(Cursor {
                    step: cursor.step + 1,
                    ..cursor
                }));
                self.tasks.push(Task::Bind(local.id));
                self.tasks.push(Task::Eval(value));
            }
            None => {
                match &arm.guard {
                    Some(guard) => {
                        self.tasks.push(Task::Guard(cursor));
                        self.tasks.push(Task::Eval(guard));
                    }
                    None => self.tasks.push(Task::Eval(&arm.body)),
                }
                for (local, projection) in alternative.bindings.iter().rev() {
                    self.tasks.push(Task::Bind(local.id));
                    self.tasks.push(Task::Eval(projection));
                }
            }
        }
        Ok(())
    }

    fn test(&mut self, cursor: Cursor<'a>) -> Result<(), Diagnostic> {
        let passed = self.truth(cursor.expression.span)?;
        self.tasks.push(Task::Arm(if passed {
            Cursor {
                step: cursor.step + 1,
                ..cursor
            }
        } else {
            Cursor {
                alternative: cursor.alternative + 1,
                step: 0,
                ..cursor
            }
        }));
        Ok(())
    }

    fn guard(&mut self, cursor: Cursor<'a>) -> Result<(), Diagnostic> {
        let arms = Self::arms(cursor)?;
        if self.truth(cursor.expression.span)? {
            self.tasks.push(Task::Eval(&arms[cursor.arm].body));
        } else {
            self.tasks.push(Task::Arm(Cursor {
                arm: cursor.arm + 1,
                alternative: 0,
                step: 0,
                ..cursor
            }));
        }
        Ok(())
    }

    /// The local that a chain of fields, payloads, and elements starts at, the steps from it, and
    /// what the read of the whole expression yields; `None` when the base is not a local.
    fn place_of(expression: &'a TypedExpr) -> Option<(usize, Vec<Step<'a>>, Read)> {
        use TypedExprKind as K;
        let (mut current, read) = match &expression.kind {
            K::Length(base) => (base.as_ref(), Read::Length),
            K::StringLength(base) => (base.as_ref(), Read::StringLength),
            K::UnionTag(base) => (base.as_ref(), Read::Tag),
            _ => (expression, Read::Value),
        };
        let mut steps = Vec::new();
        loop {
            match &current.kind {
                K::Local(id) => {
                    steps.reverse();
                    return Some((*id, steps, read));
                }
                K::Field(base, index) => {
                    steps.push(Step::Field(*index));
                    current = base;
                }
                K::UnionPayload { value, .. } => {
                    steps.push(Step::Payload);
                    current = value;
                }
                K::Index(base, index) => {
                    steps.push(Step::Index(index));
                    current = base;
                }
                K::Borrow(base, false) | K::BorrowOperand(base) | K::Dereference(base) => {
                    current = base;
                }
                _ => return None,
            }
        }
    }

    /// A field, element, length, or tag. A local is read in place, so that an element of a big
    /// array does not copy the array; any other base is evaluated whole first.
    fn eval_projection(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let Some((_, steps, _)) = Self::place_of(expression) else {
            self.tasks.push(Task::Finish(expression));
            use TypedExprKind as K;
            match &expression.kind {
                K::Index(base, index) => {
                    self.tasks.push(Task::Eval(index));
                    self.tasks.push(Task::Eval(base));
                }
                K::Field(base, _)
                | K::Length(base)
                | K::StringLength(base)
                | K::UnionTag(base)
                | K::UnionPayload { value: base, .. } => self.tasks.push(Task::Eval(base)),
                _ => return Err(unsupported(expression.span)),
            }
            return Ok(());
        };
        self.tasks.push(Task::Place(expression));
        for step in steps.iter().rev() {
            if let Step::Index(index) = step {
                self.tasks.push(Task::Eval(index));
            }
        }
        Ok(())
    }

    fn read_place(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        let span = expression.span;
        let Some((root, steps, read)) = Self::place_of(expression) else {
            return Err(unsupported(span));
        };
        let operands = steps
            .iter()
            .filter(|step| matches!(step, Step::Index(_)))
            .count();
        let mut indices = self.take(operands).into_iter();
        let frame = self.frames.last().expect("the root frame stays");
        let Some(local) = frame.locals.get(&root) else {
            return Err(unsupported(span));
        };
        let mut current = &local.expr;
        let mut scalar: Option<TypedExpr> = None;
        for step in &steps {
            if scalar.is_some() {
                return Err(unsupported(span));
            }
            match (step, &current.kind) {
                (Step::Field(index), TypedExprKind::Record(fields)) => {
                    let Some((_, field)) = fields.iter().find(|(field, _)| field == index) else {
                        return Err(unsupported(span));
                    };
                    current = field;
                }
                (Step::Field(index), TypedExprKind::Tuple(items)) => {
                    let Some(item) = items.get(*index) else {
                        return Err(unsupported(span));
                    };
                    current = item;
                }
                (
                    Step::Payload,
                    TypedExprKind::Construct {
                        payload: Some(payload),
                        ..
                    },
                ) => current = payload,
                (Step::Index(_), kind) => {
                    let index = indices.next().and_then(|value| integer(&value.expr));
                    let Some(index) = index else {
                        return Err(unsupported(span));
                    };
                    match kind {
                        // A fixed-length array literal is a tuple-like node of the array type.
                        TypedExprKind::Array(items) | TypedExprKind::Tuple(items) => {
                            let Some(item) = usize::try_from(index)
                                .ok()
                                .and_then(|index| items.get(index))
                            else {
                                return Err(out_of_bounds(span));
                            };
                            current = item;
                        }
                        TypedExprKind::String(text) => {
                            scalar = Some(string_unit(text, index, expression)?);
                        }
                        _ => return Err(unsupported(span)),
                    }
                }
                _ => return Err(unsupported(span)),
            }
        }
        let budget = *self.machine.remaining;
        let (value, cost) = match (scalar, read) {
            (Some(unit), Read::Value) => (Value::leaf(unit.kind, expression), 0),
            (Some(_), _) => return Err(unsupported(span)),
            (None, read) => match (read, &current.kind) {
                (Read::Value, _) => {
                    let (size, depth) = measure(current);
                    if size > budget {
                        return Err(limit(span));
                    }
                    let value = Value {
                        expr: current.clone(),
                        size,
                        depth,
                    };
                    (value.relabel(expression), size)
                }
                (Read::Length, TypedExprKind::Array(items) | TypedExprKind::Tuple(items)) => (
                    Value::leaf(TypedExprKind::Int(items.len() as u128), expression),
                    0,
                ),
                (Read::StringLength, TypedExprKind::String(text)) => (
                    Value::leaf(TypedExprKind::Int(string_length(text) as u128), expression),
                    0,
                ),
                (Read::Tag, TypedExprKind::Construct { case_id, .. }) => (
                    Value::leaf(TypedExprKind::Int(*case_id as u128), expression),
                    0,
                ),
                _ => return Err(unsupported(span)),
            },
        };
        self.charge(cost, span)?;
        self.stack.push(value);
        Ok(())
    }

    /// A field, element, length, or tag of a value that is not a local: the operands are on the
    /// stack, the base first.
    fn finish_projection(&mut self, expression: &'a TypedExpr) -> Result<(), Diagnostic> {
        use TypedExprKind as K;
        let span = expression.span;
        let value = match &expression.kind {
            K::Index(..) => {
                let index = self.pop(span)?;
                let base = self.pop(span)?;
                let Some(index) = integer(&index.expr) else {
                    return Err(unsupported(span));
                };
                match base.expr.kind {
                    K::Array(items) | K::Tuple(items) => {
                        let Some(item) = usize::try_from(index)
                            .ok()
                            .and_then(|index| items.into_iter().nth(index))
                        else {
                            return Err(out_of_bounds(span));
                        };
                        Value::measured(item).relabel(expression)
                    }
                    K::String(text) => {
                        Value::leaf(string_unit(&text, index, expression)?.kind, expression)
                    }
                    _ => return Err(unsupported(span)),
                }
            }
            K::Field(_, index) => {
                let base = self.pop(span)?;
                let child = match base.expr.kind {
                    K::Record(fields) => fields
                        .into_iter()
                        .find(|(field, _)| field == index)
                        .map(|(_, field)| field),
                    K::Tuple(items) => items.into_iter().nth(*index),
                    _ => None,
                };
                let Some(child) = child else {
                    return Err(unsupported(span));
                };
                Value::measured(child).relabel(expression)
            }
            K::UnionPayload { case_id, .. } => {
                let base = self.pop(span)?;
                match base.expr.kind {
                    K::Construct {
                        case_id: held,
                        payload: Some(payload),
                        ..
                    } if held == *case_id => Value::measured(*payload).relabel(expression),
                    _ => return Err(unsupported(span)),
                }
            }
            K::Length(_) => {
                let base = self.pop(span)?;
                let (K::Array(items) | K::Tuple(items)) = base.expr.kind else {
                    return Err(unsupported(span));
                };
                Value::leaf(K::Int(items.len() as u128), expression)
            }
            K::StringLength(_) => {
                let base = self.pop(span)?;
                let K::String(text) = base.expr.kind else {
                    return Err(unsupported(span));
                };
                Value::leaf(K::Int(string_length(&text) as u128), expression)
            }
            K::UnionTag(_) => {
                let base = self.pop(span)?;
                let K::Construct { case_id, .. } = base.expr.kind else {
                    return Err(unsupported(span));
                };
                Value::leaf(K::Int(case_id as u128), expression)
            }
            _ => return Err(unsupported(span)),
        };
        self.stack.push(value);
        Ok(())
    }
}

fn out_of_bounds(span: Span) -> Diagnostic {
    trapped(
        span,
        "index out of bounds; check the index against the length",
    )
}

/// The bits of an integer value, as `unary` and `binary` keep them.
fn integer(value: &TypedExpr) -> Option<u128> {
    match value.kind {
        TypedExprKind::Int(bits) => Some(bits),
        _ => None,
    }
}

fn string_length(text: &StringLiteral) -> usize {
    match text {
        StringLiteral::Utf16(units) => units.len(),
        StringLiteral::Utf8(text) => text.len(),
    }
}

/// The code unit or byte at `index` of a string, as the runtime reads it.
fn string_unit(
    text: &StringLiteral,
    index: u128,
    template: &TypedExpr,
) -> Result<TypedExpr, Diagnostic> {
    let unit = usize::try_from(index).ok().and_then(|index| match text {
        StringLiteral::Utf16(units) => units.get(index).map(|unit| u128::from(*unit)),
        StringLiteral::Utf8(text) => text.as_bytes().get(index).map(|byte| u128::from(*byte)),
    });
    match unit {
        Some(unit) => Ok(TypedExpr {
            kind: TypedExprKind::Int(unit),
            ty: template.ty.clone(),
            span: template.span,
        }),
        None => Err(out_of_bounds(template.span)),
    }
}

/// Whether `current` is still in the range that runs to `finish` by `step`: an unsigned range
/// goes up; a signed range goes up or down with the sign of the step.
/// Whether `current` is still in the range that ends at `finish`; a `downto` range is
/// `descending`, and its step `1` moves down.
fn within(
    current: u128,
    step: u128,
    finish: u128,
    bits: u16,
    signed_type: bool,
    descending: bool,
) -> bool {
    if !signed_type {
        return if descending {
            current >= finish
        } else {
            current <= finish
        };
    }
    if descending {
        return signed(current, bits) >= signed(finish, bits);
    }
    if signed(step, bits) > 0 {
        signed(current, bits) <= signed(finish, bits)
    } else {
        signed(current, bits) >= signed(finish, bits)
    }
}

/// The value after `current` and whether it is still in the range. A step that overflows the
/// type ends the range, as it does in the compiled loop.
fn advance(
    current: u128,
    step: u128,
    finish: u128,
    bits: u16,
    signed_type: bool,
    descending: bool,
) -> (u128, bool) {
    let next = if descending {
        if signed_type {
            let low = if bits == 128 {
                i128::MIN
            } else {
                -(1i128 << (bits - 1))
            };
            signed(current, bits)
                .checked_sub(signed(step, bits))
                .filter(|difference| *difference >= low)
                .map(|difference| difference as u128 & mask(bits))
        } else {
            current.checked_sub(step)
        }
    } else if signed_type {
        let sum = signed(current, bits).checked_add(signed(step, bits));
        sum.filter(|sum| bits == 128 || (-(1i128 << (bits - 1))..1i128 << (bits - 1)).contains(sum))
            .map(|sum| sum as u128 & mask(bits))
    } else {
        current.checked_add(step).filter(|sum| *sum <= mask(bits))
    };
    match next {
        Some(next) => (
            next,
            within(next, step, finish, bits, signed_type, descending),
        ),
        None => (current, false),
    }
}
