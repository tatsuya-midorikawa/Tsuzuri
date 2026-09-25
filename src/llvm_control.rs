use super::*;
use crate::check::{PatternStep, TypedMatchArm};

/// A match whose arms select on constants, lowered to one `switch`.
struct SwitchPlan {
    /// The type of the switched value: the subject's, or `i32` for a union tag.
    selector: Type,
    /// Whether the selector is the union tag of the subject.
    tag: bool,
    /// The first arm for each constant.
    cases: BTreeMap<u128, usize>,
    default: Option<usize>,
    /// Whether some arm binds a union payload.
    payloads: bool,
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn while_loop(&mut self, condition: &TypedExpr, body: &TypedExpr) {
        let test = self.label();
        let work = self.label();
        let exit = self.label();
        self.jump(&test);
        self.begin(&test);
        let condition = self.expression(condition);
        self.branch(&condition, &work, &exit);
        self.begin(&work);
        self.expression(body);
        self.jump(&test);
        self.hint_loop(body);
        self.begin(&exit);
    }

    pub(super) fn range_loop(
        &mut self,
        local: &Local,
        start: &TypedExpr,
        step: &TypedExpr,
        finish: &TypedExpr,
        body: &TypedExpr,
    ) {
        let first = self.expression(start);
        let stride = self.expression(step);
        let last = self.expression(finish);
        let Type::Integer(bits, signed) = local.ty else {
            unreachable!("integer range checked")
        };
        let ty = self.ty(&local.ty);
        let one = matches!(step.kind, TypedExprKind::Int(1));
        let minus_one =
            signed && matches!(step.kind, TypedExprKind::Int(n) if n == u128::MAX >> (128 - bits));
        let test = self.label();
        let work = self.label();
        let advance = self.label();
        let exit = self.label();
        let current = self.fresh();
        let next = self.fresh();
        self.bind_local(local, &first);
        if bits < 64 && (one || minus_one) {
            self.narrow_range_loop(local, &first, &last, minus_one, body);
            return;
        }
        if one || minus_one {
            let order = match (signed, minus_one) {
                (true, false) => "sle",
                (true, true) => "sge",
                (false, _) => "ule",
            };
            let nonempty = self.value(format!("icmp {order} {ty} {first}, {last}"));
            let entry = self.block.clone();
            self.branch(&nonempty, &work, &exit);
            self.begin(&work);
            self.instruction(format!(
                "{current} = phi {ty} [ {first}, %{entry} ], [ {next}, %{advance} ]"
            ));
            self.instruction(format!(
                "store {ty} {current}, ptr {}",
                self.locals[&local.id]
            ));
            self.expression(body);
            // Test the inclusive endpoint before incrementing, including MIN/MAX.
            let done = self.value(format!("icmp eq {ty} {current}, {last}"));
            self.branch(&done, &exit, &advance);
            self.begin(&advance);
            self.instruction(format!("{next} = add {ty} {current}, {stride}"));
            self.jump(&work);
            self.hint_loop(body);
        } else {
            let nonzero = self.value(format!("icmp ne {ty} {stride}, 0"));
            self.guard(&nonzero);
            let positive = if signed {
                self.value(format!("icmp sgt {ty} {stride}, 0"))
            } else {
                "true".into()
            };
            let entry = self.block.clone();
            self.jump(&test);
            self.begin(&test);
            self.instruction(format!(
                "{current} = phi {ty} [ {first}, %{entry} ], [ {next}, %{advance} ]"
            ));
            let low = self.value(format!(
                "icmp {} {ty} {current}, {last}",
                if signed { "sle" } else { "ule" }
            ));
            let within = if signed {
                let high = self.value(format!("icmp sge {ty} {current}, {last}"));
                self.value(format!("select i1 {positive}, i1 {low}, i1 {high}"))
            } else {
                low
            };
            self.branch(&within, &work, &exit);
            self.begin(&work);
            self.instruction(format!(
                "store {ty} {current}, ptr {}",
                self.locals[&local.id]
            ));
            self.expression(body);
            self.jump(&advance);
            self.begin(&advance);
            let intrinsic = format!(
                "@llvm.{}add.with.overflow.i{bits}",
                if signed { "s" } else { "u" }
            );
            self.intrinsics
                .insert(format!("declare {{ {ty}, i1 }} {intrinsic}({ty}, {ty})"));
            let sum = self.value(format!(
                "call {{ {ty}, i1 }} {intrinsic}({ty} {current}, {ty} {stride})"
            ));
            self.instruction(format!("{next} = extractvalue {{ {ty}, i1 }} {sum}, 0"));
            let overflow = self.value(format!("extractvalue {{ {ty}, i1 }} {sum}, 1"));
            self.branch(&overflow, &exit, &test);
            self.hint_loop(body);
        }
        self.begin(&exit);
    }

    fn narrow_range_loop(
        &mut self,
        local: &Local,
        first: &str,
        last: &str,
        descending: bool,
        body: &TypedExpr,
    ) {
        let Type::Integer(_, signed) = local.ty else {
            unreachable!()
        };
        let extension = if signed { "sext" } else { "zext" };
        let ty = self.ty(&local.ty);
        let first = self.value(format!("{extension} {ty} {first} to i64"));
        let last = self.value(format!("{extension} {ty} {last} to i64"));
        let entry = self.block.clone();
        let test = self.label();
        let work = self.label();
        let advance = self.label();
        let exit = self.label();
        let current = self.fresh();
        let next = self.fresh();
        self.jump(&test);
        self.begin(&test);
        self.instruction(format!(
            "{current} = phi i64 [ {first}, %{entry} ], [ {next}, %{advance} ]"
        ));
        let valid = self.value(format!(
            "icmp {} i64 {current}, {last}",
            if descending { "sge" } else { "sle" }
        ));
        self.branch(&valid, &work, &exit);
        self.begin(&work);
        let narrowed = self.value(format!("trunc i64 {current} to {ty}"));
        // The body is bounded by extended narrow endpoints; expose this proven range without overflow flags.
        let restored = self.value(format!("{extension} {ty} {narrowed} to i64"));
        let in_range = self.value(format!("icmp eq i64 {current}, {restored}"));
        self.intrinsics
            .insert("declare void @llvm.assume(i1)".into());
        self.instruction(format!("call void @llvm.assume(i1 {in_range})"));
        self.instruction(format!(
            "store {ty} {narrowed}, ptr {}",
            self.locals[&local.id]
        ));
        self.expression(body);
        self.jump(&advance);
        self.begin(&advance);
        // A narrow range's first out-of-range value still fits i64, including signed/unsigned extrema.
        self.instruction(format!(
            "{next} = add i64 {current}, {}",
            if descending { -1 } else { 1 }
        ));
        self.jump(&test);
        self.hint_loop(body);
        self.begin(&exit);
    }

    pub(super) fn for_each(&mut self, local: &Local, source: &TypedExpr, body: &TypedExpr) {
        let (collection, frames) = self.read_operand(source);
        let data = self.value(format!(
            "extractvalue {} {collection}, 0",
            self.ty(&source.ty)
        ));
        let length = self.value(format!(
            "extractvalue {} {collection}, 1",
            self.ty(&source.ty)
        ));
        self.borrowed_locals.insert(local.id);
        if let Type::List(element) = &source.ty {
            self.list_loop(&data, &length, |emitter, node| {
                let pointer = emitter.list_element_pointer(element, node);
                emitter.locals.insert(local.id, pointer);
                emitter.expression(body);
            });
        } else {
            self.array_loop(&length, |emitter, index| {
                let pointer = emitter.element_pointer(&local.ty, &data, index);
                emitter.locals.insert(local.id, pointer);
                emitter.expression(body);
            });
        }
        self.borrowed_locals.remove(&local.id);
        self.release_operand(source, &collection, &frames);
    }

    /// Recognizes arms that only compare the subject, or a union subject's tag, with constants.
    fn switch_plan(&self, local: &Local, arms: &[TypedMatchArm]) -> Option<SwitchPlan> {
        let tag = match local.ty {
            Type::Integer(..) | Type::Bool | Type::Unit => false,
            Type::Union(..) => true,
            _ => return None,
        };
        let subject =
            |value: &TypedExpr| matches!(value.kind, TypedExprKind::Local(id) if id == local.id);
        let mut cases = BTreeMap::new();
        let mut default = None;
        let mut payloads = false;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            // Switched arms bind storage at the same address in every alternative.
            let mut paths = BTreeMap::new();
            for alternative in &arm.alternatives {
                for (binding, value) in &alternative.bindings {
                    let path = Self::binding_path(value, local.id)?;
                    payloads |= !path.is_empty();
                    if paths.entry(binding.id).or_insert_with(|| path.clone()) != &path {
                        return None;
                    }
                }
                let condition = match alternative.steps.as_slice() {
                    [] => None,
                    [PatternStep::Test(condition)] => Some(&condition.kind),
                    _ => return None,
                };
                match condition {
                    None | Some(TypedExprKind::Bool(true)) => {
                        default.get_or_insert(index);
                    }
                    Some(TypedExprKind::Binary(BinaryOp::Equal, left, right))
                        if match &left.kind {
                            TypedExprKind::UnionTag(value) => tag && subject(value),
                            _ => !tag && subject(left),
                        } =>
                    {
                        let constant = match right.kind {
                            TypedExprKind::Int(n) => n,
                            TypedExprKind::Bool(b) => u128::from(b),
                            TypedExprKind::Unit => 0,
                            _ => return None,
                        };
                        if default.is_none() {
                            cases.entry(constant).or_insert(index);
                        }
                    }
                    _ => return None,
                }
            }
        }
        Some(SwitchPlan {
            selector: if tag {
                Type::Integer(32, true)
            } else {
                local.ty.clone()
            },
            tag,
            cases,
            default,
            payloads,
        })
    }

    /// The storage path of a pattern binding below the subject or its union payload storage,
    /// with the type that each step projects from.
    fn binding_path(value: &TypedExpr, subject: usize) -> Option<Vec<(Option<usize>, Type)>> {
        match &value.kind {
            TypedExprKind::Local(id) if *id == subject => Some(Vec::new()),
            TypedExprKind::UnionPayload { value: union, .. } if matches!(union.kind, TypedExprKind::Local(id) if id == subject) => {
                Some(vec![(None, union.ty.clone())])
            }
            TypedExprKind::Field(record, index) => {
                let mut path = Self::binding_path(record, subject)?;
                path.push((Some(*index), record.ty.clone()));
                Some(path)
            }
            _ => None,
        }
    }

    /// The switched value of `matched`: the value itself, or its union tag.
    fn selector(&mut self, plan: &SwitchPlan, matched: &TypedExpr) -> String {
        if plan.tag {
            self.union_tag(matched)
        } else {
            self.expression_mode(matched, false)
        }
    }

    fn constant_result(expression: &TypedExpr) -> Option<String> {
        match &expression.kind {
            TypedExprKind::Int(value) => Some(value.to_string()),
            TypedExprKind::Bool(value) => Some(u8::from(*value).to_string()),
            TypedExprKind::Unit => Some("0".into()),
            TypedExprKind::Block { bindings, result } if bindings.is_empty() => {
                Self::constant_result(result)
            }
            _ => None,
        }
    }

    fn lookup_match(
        &mut self,
        local: &Local,
        matched: &TypedExpr,
        arms: &[TypedMatchArm],
        result_type: &Type,
    ) -> Option<String> {
        if !matches!(result_type, Type::Integer(..) | Type::Bool | Type::Unit) {
            return None;
        }
        let plan = self.switch_plan(local, arms)?;
        if plan.payloads {
            return None;
        }
        let default = Self::constant_result(&arms[plan.default?].body)?;
        let cases = &plan.cases;
        let first = *cases.keys().next()?;
        let last = *cases.keys().next_back()?;
        let width = last.checked_sub(first)?.checked_add(1)?;
        if !(4..=256).contains(&width) || width > (cases.len() as u128) * 2 {
            return None;
        }
        let width = (width as usize).next_power_of_two();
        let mut constants = vec![default.clone(); width];
        for (key, arm) in cases {
            constants[(key - first) as usize] = Self::constant_result(&arms[*arm].body)?;
        }
        let source = self.selector(&plan, matched);
        let source_type = self.ty(&plan.selector);
        let offset = self.value(format!("sub {source_type} {source}, {first}"));
        let valid = self.value(format!("icmp ule {source_type} {offset}, {}", width - 1));
        let yes = self.label();
        let no = self.label();
        let merge = self.label();
        self.branch(&valid, &yes, &no);
        self.begin(&yes);
        let index = match plan.selector {
            Type::Integer(64, _) => offset,
            Type::Integer(128, _) => self.value(format!("trunc i128 {offset} to i64")),
            _ => self.value(format!("zext {source_type} {offset} to i64")),
        };
        let ty = self.ty(result_type);
        let name = format!("@tz.match.table.{}", self.globals.definitions.len());
        self.globals.definitions.push(format!(
            "{name} = private unnamed_addr constant [{width} x {ty}] [{}]",
            constants
                .into_iter()
                .map(|value| format!("{ty} {value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let pointer = self.value(format!(
            "getelementptr inbounds [{width} x {ty}], ptr {name}, i64 0, i64 {index}"
        ));
        let result = self.value(format!("load {ty}, ptr {pointer}"));
        let result_block = self.block.clone();
        self.jump(&merge);
        self.begin(&no);
        self.jump(&merge);
        self.begin(&merge);
        Some(self.value(format!(
            "phi {ty} [ {result}, %{result_block} ], [ {default}, %{no} ]"
        )))
    }

    pub(super) fn match_expression(
        &mut self,
        local: &Local,
        matched: &TypedExpr,
        arms: &[TypedMatchArm],
        result_type: &Type,
        tail: bool,
        mut delivered: Option<&mut Vec<Frame>>,
    ) -> String {
        if let Some(result) = self.lookup_match(local, matched, arms, result_type) {
            if tail {
                self.drop_all();
                self.instruction(format!("ret {} {result}", self.ty(result_type)));
            }
            return result;
        }
        self.scopes.push(Vec::new());
        if Self::is_place(matched) {
            let pointer = self.place(matched);
            self.locals.insert(local.id, pointer);
            self.borrowed_locals.insert(local.id);
            let frames = self.frame_of_place(matched);
            if !frames.is_empty() {
                self.frame_locals.insert(local.id, frames);
            }
        } else {
            let (value, frames) = self.frame_value(matched);
            self.bind_local(local, &value);
            self.bind_frames(local, frames);
        }
        let done = self.label();
        let failure = self.label();
        let labels: Vec<_> = arms.iter().map(|_| self.label()).collect();
        let switch = self.switch_plan(local, arms);
        if let Some(plan) = &switch {
            // The union tag is the first field, so one load reads it from the subject's slot.
            let subject = self.value(format!(
                "load {}, ptr {}",
                self.ty(&plan.selector),
                self.locals[&local.id]
            ));
            let default = plan
                .default
                .map_or(failure.as_str(), |index| labels[index].as_str());
            self.instruction(format!(
                "switch {} {subject}, label %{default} [",
                self.ty(&plan.selector)
            ));
            for (constant, index) in &plan.cases {
                self.instruction(format!(
                    "  {} {constant}, label %{}",
                    self.ty(&plan.selector),
                    labels[*index]
                ));
            }
            self.instruction("]");
        } else {
            self.jump(&labels[0]);
        }
        let mut incoming = Vec::new();
        for (index, arm) in arms.iter().enumerate() {
            self.begin(&labels[index]);
            let next_arm = labels.get(index + 1).unwrap_or(&failure);
            let bindings = &arm.alternatives[0].bindings;
            self.scopes.push(Vec::new());
            let mut prepared = BTreeSet::new();
            for alternative in &arm.alternatives {
                for step in &alternative.steps {
                    if let PatternStep::Bind(local, _) = step {
                        if prepared.insert(local.id) {
                            self.bind_local(local, "zeroinitializer");
                        }
                    }
                }
            }
            let temporaries = self.scopes.last().unwrap().clone();
            if switch.is_none() {
                let ready = self.label();
                let pointers: BTreeMap<_, _> = bindings
                    .iter()
                    .map(|(local, _)| {
                        (
                            local.id,
                            self.slot(&Type::Reference(Box::new(local.ty.clone()), false)),
                        )
                    })
                    .collect();
                for (index, alternative) in arm.alternatives.iter().enumerate() {
                    let no = if index + 1 == arm.alternatives.len() {
                        next_arm.clone()
                    } else {
                        self.label()
                    };
                    for step in &alternative.steps {
                        match step {
                            PatternStep::Bind(local, expression) => {
                                let value = self.expression(expression);
                                self.instruction(format!(
                                    "store {} {value}, ptr {}",
                                    self.ty(&local.ty),
                                    self.locals[&local.id]
                                ));
                            }
                            PatternStep::Test(condition) => {
                                let yes = self.label();
                                let failed = self.label();
                                let condition = self.expression(condition);
                                self.branch(&condition, &yes, &failed);
                                self.begin(&failed);
                                self.clear_pattern_temporaries(&temporaries);
                                self.jump(&no);
                                self.begin(&yes);
                            }
                        }
                    }
                    for (binding, projection) in &alternative.bindings {
                        let pointer = self.place(projection);
                        self.instruction(format!(
                            "store ptr {pointer}, ptr {}",
                            pointers[&binding.id]
                        ));
                    }
                    self.jump(&ready);
                    if index + 1 != arm.alternatives.len() {
                        self.begin(&no);
                    }
                }
                self.begin(&ready);
                for (binding, _) in bindings {
                    let pointer = self.value(format!("load ptr, ptr {}", pointers[&binding.id]));
                    self.locals.insert(binding.id, pointer);
                }
            } else {
                // Every alternative of a switched arm binds the subject or its payload storage.
                for (binding, projection) in bindings {
                    let pointer = self.place(projection);
                    self.locals.insert(binding.id, pointer);
                }
            }
            for (binding, _) in bindings {
                self.borrowed_locals.insert(binding.id);
                // A binding aliases part of the scrutinee in whichever alternative matched.
                let frames: Vec<Frame> = arm
                    .alternatives
                    .iter()
                    .flat_map(|alternative| &alternative.bindings)
                    .filter(|(other, _)| other.id == binding.id)
                    .flat_map(|(_, projection)| self.frame_of_place(projection))
                    .collect();
                if !frames.is_empty() {
                    self.frame_locals.insert(binding.id, frames);
                }
            }
            if let Some(guard) = &arm.guard {
                let condition = self.expression(guard);
                let success = self.label();
                let failed = self.label();
                self.branch(&condition, &success, &failed);
                self.begin(&failed);
                self.clear_pattern_temporaries(&temporaries);
                self.jump(next_arm);
                self.begin(&success);
            }
            self.scopes.push(Vec::new());
            let types = self.module.types();
            let views: Vec<usize> = bindings
                .iter()
                .filter(|(binding, _)| arm.views(binding, &types))
                .map(|(binding, _)| binding.id)
                .collect();
            for (binding, _) in bindings {
                if views.contains(&binding.id) {
                    // The body keeps reading the matched storage in place.
                    continue;
                }
                let expression = TypedExpr {
                    kind: TypedExprKind::Local(binding.id),
                    ty: binding.ty.clone(),
                    span: binding.span,
                };
                let value = self.expression(&expression);
                self.borrowed_locals.remove(&binding.id);
                self.frame_locals.remove(&binding.id);
                self.bind_local(binding, &value);
            }
            if tail {
                self.tail(&arm.body);
            } else {
                let value = match delivered.as_deref_mut() {
                    Some(frames) => {
                        let (value, arm_frames) = self.frame_inner(&arm.body);
                        frames.extend(arm_frames);
                        value
                    }
                    None => self.expression(&arm.body),
                };
                let scope = self.scopes.last().unwrap().clone();
                self.drop_scope(&scope);
                self.drop_scope(&temporaries);
                incoming.push(format!("[ {value}, %{} ]", self.block));
                self.jump(&done);
            }
            for id in &views {
                self.borrowed_locals.remove(id);
                self.frame_locals.remove(id);
            }
            self.scopes.pop();
            self.scopes.pop();
        }
        self.begin(&failure);
        self.instruction("call void @llvm.trap()");
        self.instruction("unreachable");
        self.borrowed_locals.remove(&local.id);
        self.frame_locals.remove(&local.id);
        let scope = self.scopes.pop().unwrap();
        if tail {
            return String::new();
        }
        self.begin(&done);
        let result = self.value(format!(
            "phi {} {}",
            self.ty(result_type),
            incoming.join(", ")
        ));
        self.drop_scope(&scope);
        result
    }

    fn clear_pattern_temporaries(&mut self, temporaries: &[(String, Type)]) {
        for (slot, ty) in temporaries.iter().rev() {
            self.drop_slot(slot, ty);
            self.instruction(format!("store {} zeroinitializer, ptr {slot}", self.ty(ty)));
        }
    }
}
