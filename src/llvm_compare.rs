use super::*;

impl FunctionEmitter<'_, '_> {
    fn comparison_method(&mut self, method: &TypedExpr, left: &str, right: &str) -> String {
        let Type::Function(parameters, _) = &method.ty else {
            unreachable!()
        };
        let Type::Reference(element, _) = &parameters[0] else {
            unreachable!()
        };
        let values = [
            self.borrowed_element(element, left),
            self.borrowed_element(element, right),
        ];
        self.comparison_values(method, &values)
    }

    pub(super) fn comparison_values(&mut self, method: &TypedExpr, values: &[String]) -> String {
        let TypedExprKind::Function(FunctionRef::User(id)) = method.kind else {
            unreachable!()
        };
        let target = &self.module.functions[id];
        let Type::Function(parameters, _) = &method.ty else {
            unreachable!()
        };
        let count = target.parameters.len();
        let arguments = parameters
            .iter()
            .zip(values)
            .take(count)
            .map(|(ty, value)| format!("{} {value}", self.ty(ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut result = self.value(format!(
            "call {} @tz.fn.{}({arguments})",
            self.ty(&target.signature.result),
            target.qualified_name()
        ));
        let mut ty = target.signature.result.clone();
        for (parameter, value) in parameters.iter().zip(values).skip(count) {
            (result, ty) = self.apply_value(&result, &ty, Some((parameter, value)), false);
        }
        result
    }

    fn comparison_field(
        &mut self,
        operator: BinaryOp,
        methods: &[TypedExpr],
        pointers: [&str; 2],
        result: &str,
        done: &str,
    ) {
        let equal = self.comparison_method(&methods[0], pointers[0], pointers[1]);
        let next = self.label();
        let different = self.label();
        self.branch(&equal, &next, &different);
        self.begin(&different);
        let compared = match operator {
            BinaryOp::Equal => "false".into(),
            BinaryOp::NotEqual => "true".into(),
            _ => self.comparison_method(&methods[1], pointers[0], pointers[1]),
        };
        self.instruction(format!("store i1 {compared}, ptr {result}"));
        self.jump(done);
        self.begin(&next);
    }

    pub(super) fn structural_compare(
        &mut self,
        operator: BinaryOp,
        arguments: &[TypedExpr],
    ) -> String {
        let Type::Reference(ty, _) = &arguments[0].ty else {
            unreachable!()
        };
        let left = self.expression(&arguments[0]);
        let right = self.expression(&arguments[1]);
        let methods = &arguments[2..];
        let equality = matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual);
        let result = self.spill(
            &Type::Bool,
            if matches!(
                operator,
                BinaryOp::Equal | BinaryOp::LessEqual | BinaryOp::GreaterEqual
            ) {
                "true"
            } else {
                "false"
            },
        );
        let done = self.label();
        match ty.as_ref() {
            Type::Tuple(elements) => {
                let stride = if equality { 1 } else { 2 };
                for index in 0..elements.len() {
                    let left = self.value(format!(
                        "getelementptr inbounds {}, ptr {left}, i32 0, i32 {index}",
                        self.ty(ty)
                    ));
                    let right = self.value(format!(
                        "getelementptr inbounds {}, ptr {right}, i32 0, i32 {index}",
                        self.ty(ty)
                    ));
                    self.comparison_field(
                        operator,
                        &methods[index * stride..][..stride],
                        [&left, &right],
                        &result,
                        &done,
                    );
                }
                self.jump(&done);
            }
            Type::Array(element) | Type::List(element) => {
                let linked = matches!(ty.as_ref(), Type::List(_));
                let aggregate = self.ty(ty);
                let left = if linked {
                    self.value(format!("load {aggregate}, ptr {left}"))
                } else {
                    left
                };
                let right = if linked {
                    self.value(format!("load {aggregate}, ptr {right}"))
                } else {
                    right
                };
                let left_data = self.value(format!("extractvalue {aggregate} {left}, 0"));
                let right_data = self.value(format!("extractvalue {aggregate} {right}, 0"));
                let left_length = self.value(format!("extractvalue {aggregate} {left}, 1"));
                let right_length = self.value(format!("extractvalue {aggregate} {right}, 1"));
                if equality {
                    let same_length =
                        self.value(format!("icmp eq i64 {left_length}, {right_length}"));
                    let same = self.label();
                    let different = self.label();
                    self.branch(&same_length, &same, &different);
                    self.begin(&different);
                    self.instruction(format!(
                        "store i1 {}, ptr {result}",
                        operator == BinaryOp::NotEqual
                    ));
                    self.jump(&done);
                    self.begin(&same);
                }
                let shorter = self.value(format!("icmp ult i64 {left_length}, {right_length}"));
                let length = self.value(format!(
                    "select i1 {shorter}, i64 {left_length}, i64 {right_length}"
                ));
                let start = self.block.clone();
                let test = self.label();
                let body = self.label();
                let advance = self.label();
                let finish = self.label();
                let index = self.fresh();
                let increment = self.fresh();
                let left_node = self.fresh();
                let right_node = self.fresh();
                let left_next = self.fresh();
                let right_next = self.fresh();
                self.jump(&test);
                self.begin(&test);
                self.instruction(format!(
                    "{index} = phi i64 [ 0, %{start} ], [ {increment}, %{advance} ]"
                ));
                if linked {
                    self.instruction(format!("{left_node} = phi ptr [ {left_data}, %{start} ], [ {left_next}, %{advance} ]"));
                    self.instruction(format!("{right_node} = phi ptr [ {right_data}, %{start} ], [ {right_next}, %{advance} ]"));
                }
                let more = self.value(format!("icmp ult i64 {index}, {length}"));
                self.branch(&more, &body, &finish);
                self.begin(&body);
                let node_type = format!("{{ ptr, {} }}", self.ty(element));
                let left = if linked {
                    self.value(format!(
                        "getelementptr inbounds {node_type}, ptr {left_node}, i32 0, i32 1"
                    ))
                } else {
                    self.element_pointer(element, &left_data, &index)
                };
                let right = if linked {
                    self.value(format!(
                        "getelementptr inbounds {node_type}, ptr {right_node}, i32 0, i32 1"
                    ))
                } else {
                    self.element_pointer(element, &right_data, &index)
                };
                self.comparison_field(operator, methods, [&left, &right], &result, &done);
                self.jump(&advance);
                self.begin(&advance);
                if linked {
                    self.instruction(format!("{left_next} = load ptr, ptr {left_node}"));
                    self.instruction(format!("{right_next} = load ptr, ptr {right_node}"));
                }
                self.instruction(format!("{increment} = add i64 {index}, 1"));
                self.jump(&test);
                self.begin(&finish);
                if !equality {
                    let predicate = match operator {
                        BinaryOp::Less => "ult",
                        BinaryOp::LessEqual => "ule",
                        BinaryOp::Greater => "ugt",
                        BinaryOp::GreaterEqual => "uge",
                        _ => unreachable!(),
                    };
                    let compared = self.value(format!(
                        "icmp {predicate} i64 {left_length}, {right_length}"
                    ));
                    self.instruction(format!("store i1 {compared}, ptr {result}"));
                }
                self.jump(&done);
            }
            _ => unreachable!(),
        }
        self.begin(&done);
        self.value(format!("load i1, ptr {result}"))
    }
}
