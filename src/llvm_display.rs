use super::*;

impl FunctionEmitter<'_, '_> {
    fn display_element(&mut self, method: &TypedExpr, pointer: &str) -> String {
        let Type::Function(parameters, _) = &method.ty else {
            unreachable!()
        };
        let Type::Reference(element, _) = &parameters[0] else {
            unreachable!()
        };
        let value = self.borrowed_element(element, pointer);
        self.comparison_values(method, &[value])
    }

    pub(super) fn structural_display(&mut self, arguments: &[TypedExpr]) -> String {
        let Type::Reference(ty, _) = &arguments[0].ty else {
            unreachable!()
        };
        let input = self.expression(&arguments[0]);
        let methods = &arguments[1..];
        let (parts, kind) = if let Type::Tuple(elements) = ty.as_ref() {
            let (parts, data) = self.allocate_array(&Type::String, &elements.len().to_string());
            for (index, method) in methods.iter().enumerate() {
                let pointer = self.value(format!(
                    "getelementptr inbounds {}, ptr {input}, i32 0, i32 {index}",
                    self.ty(ty)
                ));
                let text = self.display_element(method, &pointer);
                let target = self.element_pointer(&Type::String, &data, &index.to_string());
                self.instruction(format!("store %tz.string {text}, ptr {target}"));
            }
            (parts, 0)
        } else {
            let (element, linked) = match ty.as_ref() {
                Type::Array(element) => (element.as_ref(), false),
                Type::List(element) => (element.as_ref(), true),
                _ => unreachable!(),
            };
            let aggregate = self.ty(ty);
            let input = if linked {
                self.value(format!("load {aggregate}, ptr {input}"))
            } else {
                input
            };
            let data = self.value(format!("extractvalue {aggregate} {input}, 0"));
            let length = self.value(format!("extractvalue {aggregate} {input}, 1"));
            let (parts, target) = self.allocate_array(&Type::String, &length);
            let cursor =
                linked.then(|| self.spill(&Type::Reference(Box::new(Type::Unit), false), &data));
            self.array_loop(&length, |emitter, index| {
                let pointer = if let Some(cursor) = &cursor {
                    let node = emitter.value(format!("load ptr, ptr {cursor}"));
                    let next = emitter.value(format!("load ptr, ptr {node}"));
                    emitter.instruction(format!("store ptr {next}, ptr {cursor}"));
                    emitter.list_element_pointer(element, &node)
                } else {
                    emitter.element_pointer(element, &data, index)
                };
                let text = emitter.display_element(&methods[0], &pointer);
                let output = emitter.element_pointer(&Type::String, &target, index);
                emitter.instruction(format!("store %tz.string {text}, ptr {output}"));
            });
            (parts, if linked { 2 } else { 1 })
        };
        self.value(format!(
            "call %tz.string @tz.display.join(%tz.array {parts}, i32 {kind})"
        ))
    }

    pub(super) fn display_quoted(&mut self, ty: &Type) -> String {
        let mut temporary = None;
        let (data, length, kind) = match ty {
            Type::String | Type::Utf8String => {
                let text = self.value(format!("load {}, ptr %arg0", self.ty(ty)));
                let mut data = self.value(format!("extractvalue {} {text}, 0", self.ty(ty)));
                let mut length = self.value(format!("extractvalue {} {text}, 1", self.ty(ty)));
                if *ty == Type::Utf8String {
                    let decoded = self.value(format!(
                        "call %tz.string @tz.string.from_utf8(ptr {data}, i64 {length})"
                    ));
                    data = self.value(format!("extractvalue %tz.string {decoded}, 0"));
                    length = self.value(format!("extractvalue %tz.string {decoded}, 1"));
                    temporary = Some(data.clone());
                }
                (data, length, usize::from(*ty == Type::Utf8String))
            }
            Type::Char => ("%arg0".into(), "1".into(), 2),
            Type::Utf8Char => {
                let scalar = self.value("load i32, ptr %arg0");
                let supplementary = self.value(format!("icmp ugt i32 {scalar}, 65535"));
                let offset = self.value(format!("sub i32 {scalar}, 65536"));
                let high = self.value(format!("lshr i32 {offset}, 10"));
                let high = self.value(format!("add i32 {high}, 55296"));
                let first = self.value(format!(
                    "select i1 {supplementary}, i32 {high}, i32 {scalar}"
                ));
                let first = self.value(format!("trunc i32 {first} to i16"));
                let low = self.value(format!("and i32 {offset}, 1023"));
                let low = self.value(format!("add i32 {low}, 56320"));
                let low = self.value(format!("trunc i32 {low} to i16"));
                let data = self.slot(&Type::Tuple(vec![Type::Integer(16, false); 2]));
                self.instruction(format!("store i16 {first}, ptr {data}"));
                let next = self.value(format!("getelementptr inbounds i16, ptr {data}, i64 1"));
                self.instruction(format!("store i16 {low}, ptr {next}"));
                let length = self.value(format!("select i1 {supplementary}, i64 2, i64 1"));
                (data, length, 3)
            }
            _ => unreachable!(),
        };
        let result = self.value(format!(
            "call %tz.string @tz.display.quote(ptr {data}, i64 {length}, i32 {kind})"
        ));
        if let Some(data) = temporary {
            self.instruction(format!("call void @tz.free(ptr {data})"));
        }
        result
    }
}
