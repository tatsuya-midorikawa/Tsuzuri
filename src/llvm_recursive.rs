use super::*;

pub(super) fn node_type(ty: &Type, module: &CheckedModule) -> String {
    format!("%\"tz.rec.node.{}\"", canonical_type(ty, module))
}

fn action(ty: &Type, module: &CheckedModule, operation: &str) -> String {
    format!("@\"tz.{operation}.rec.{}\"", canonical_type(ty, module))
}

fn empty_case(ty: &Type, module: &CheckedModule) -> Option<usize> {
    let Type::Union(id, _) = ty else {
        unreachable!()
    };
    // Every value of a Drop union owns a node, so null only marks moved-out storage (B07).
    if module.unions[*id].user_drop {
        return None;
    }
    module.unions[*id]
        .cases
        .iter()
        .position(|(_, payload)| payload.is_none())
}

pub(super) fn emit_helpers(
    module: &CheckedModule,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut output = String::new();
    let mut completed = BTreeSet::new();
    while let Some(ty) = globals
        .recursive_types
        .iter()
        .find(|ty| !completed.contains(*ty))
        .cloned()
    {
        completed.insert(ty.clone());
        let function = &module.functions[0];
        let Type::Union(id, arguments) = &ty else {
            unreachable!()
        };
        let payloads: Vec<_> = module
            .types()
            .union_payloads(*id, arguments)
            .into_iter()
            .enumerate()
            .filter_map(|(index, ty)| ty.map(|ty| (index, ty)))
            .collect();
        let mut drop = FunctionEmitter::new(
            module,
            function,
            0,
            builtins,
            intrinsics,
            globals,
            specializations,
        );
        drop.drop_pending = Some("%pending".into());
        // The user drop runs once per node, before the node's children are queued (B07 D8).
        let node = if module.unions[*id].user_drop {
            drop.call_user_drop(&ty, "%node")
        } else {
            "%node".to_owned()
        };
        let cases: Vec<_> = payloads
            .iter()
            .filter(|(_, ty)| ty.needs_drop(&module.types()))
            .cloned()
            .collect();
        let (labels, done) = drop.case_switch(&ty, &node, &cases);
        for ((_, payload), label) in cases.iter().zip(labels) {
            drop.begin(&label);
            let pointer = drop.recursive_payload(&ty, &node);
            let value = drop.value(format!("load {}, ptr {pointer}", drop.ty(payload)));
            drop.drop_value(payload, &value);
            drop.jump(&done);
        }
        drop.begin(&done);
        drop.instruction(format!("call void @tz.free(ptr {node})"));
        drop.instruction("ret void");
        let drop_name = action(&ty, module, "drop");
        output.push_str(&drop.auxiliary(&format!("void {drop_name}(ptr %node, ptr %pending)")));
        if ty.can_capture(&module.types()) {
            let mut create = FunctionEmitter::new(
                module,
                function,
                0,
                builtins,
                intrinsics,
                globals,
                specializations,
            );
            let node = create.value(format!(
                "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
                node_type(&ty, module)
            ));
            let head = create.value("load ptr, ptr %pending");
            create.recursive_header(
                &ty,
                &node,
                &head,
                &action(&ty, module, "clone.step"),
                "%source",
            );
            create.instruction(format!("store ptr {node}, ptr %pending"));
            create.instruction(format!("ret ptr {node}"));
            output.push_str(&create.auxiliary(&format!(
                "ptr {}(ptr %source, ptr %pending)",
                action(&ty, module, "clone.new")
            )));
            let mut clone = FunctionEmitter::new(
                module,
                function,
                0,
                builtins,
                intrinsics,
                globals,
                specializations,
            );
            clone.clone_pending = Some("%pending".into());
            let source_tag = clone.recursive_tag(&ty, "%source");
            let destination_tag = clone.value(format!(
                "getelementptr inbounds {}, ptr %node, i32 0, i32 3",
                node_type(&ty, module)
            ));
            clone.instruction(format!("store i32 {source_tag}, ptr {destination_tag}"));
            let (labels, done) = clone.case_switch(&ty, "%source", &payloads);
            for ((_, payload), label) in payloads.iter().zip(labels) {
                clone.begin(&label);
                let source = clone.recursive_payload(&ty, "%source");
                let value = clone.value(format!("load {}, ptr {source}", clone.ty(payload)));
                let value = clone.clone_value(payload, &value);
                let target = clone.recursive_payload(&ty, "%node");
                clone.instruction(format!("store {} {value}, ptr {target}", clone.ty(payload)));
                clone.jump(&done);
            }
            clone.begin(&done);
            clone.recursive_header(
                &ty,
                "%node",
                "null",
                &drop_name,
                &action(&ty, module, "clone.new"),
            );
            clone.instruction("ret void");
            output.push_str(&clone.auxiliary(&format!(
                "void {}(ptr %node, ptr %source, ptr %pending)",
                action(&ty, module, "clone.step")
            )));
        }
    }
    output
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn recursive_construct(
        &mut self,
        ty: &Type,
        case: usize,
        payload: Option<(String, String)>,
    ) -> String {
        self.globals.recursive_types.insert(ty.clone());
        if empty_case(ty, self.module) == Some(case) {
            return "null".into();
        }
        let node = self.value(format!(
            "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
            node_type(ty, self.module)
        ));
        let clone = if ty.can_capture(&self.module.types()) {
            action(ty, self.module, "clone.new")
        } else {
            "null".into()
        };
        self.recursive_header(ty, &node, "null", &action(ty, self.module, "drop"), &clone);
        let tag = self.value(format!(
            "getelementptr inbounds {}, ptr {node}, i32 0, i32 3",
            node_type(ty, self.module)
        ));
        self.instruction(format!("store i32 {case}, ptr {tag}"));
        if let Some((value, payload_type)) = payload {
            let pointer = self.recursive_payload(ty, &node);
            self.instruction(format!("store {payload_type} {value}, ptr {pointer}"));
        }
        node
    }

    fn recursive_header(
        &mut self,
        ty: &Type,
        node: &str,
        next: &str,
        action: &str,
        auxiliary: &str,
    ) {
        for (index, value) in [next, action, auxiliary].into_iter().enumerate() {
            let pointer = self.value(format!(
                "getelementptr inbounds {}, ptr {node}, i32 0, i32 {index}",
                node_type(ty, self.module)
            ));
            self.instruction(format!("store ptr {value}, ptr {pointer}"));
        }
    }

    pub(super) fn recursive_payload(&mut self, ty: &Type, node: &str) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {node}, i32 0, i32 4",
            node_type(ty, self.module)
        ))
    }

    pub(super) fn recursive_tag(&mut self, ty: &Type, node: &str) -> String {
        if let Some(case) = empty_case(ty, self.module) {
            let empty = self.value(format!("icmp eq ptr {node}, null"));
            let none = self.label();
            let some = self.label();
            let done = self.label();
            self.branch(&empty, &none, &some);
            self.begin(&none);
            self.jump(&done);
            self.begin(&some);
            let pointer = self.value(format!(
                "getelementptr inbounds {}, ptr {node}, i32 0, i32 3",
                node_type(ty, self.module)
            ));
            let tag = self.value(format!("load i32, ptr {pointer}"));
            self.jump(&done);
            self.begin(&done);
            self.value(format!("phi i32 [ {case}, %{none} ], [ {tag}, %{some} ]"))
        } else {
            let pointer = self.value(format!(
                "getelementptr inbounds {}, ptr {node}, i32 0, i32 3",
                node_type(ty, self.module)
            ));
            self.value(format!("load i32, ptr {pointer}"))
        }
    }
}
