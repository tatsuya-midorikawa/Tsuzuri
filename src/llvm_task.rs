use super::*;

impl FunctionEmitter<'_, '_> {
    pub(super) fn parallel_result_tasks(&mut self, tasks: &TypedExpr, result: &Type) -> String {
        let Type::Array(task) = &tasks.ty else {
            unreachable!()
        };
        let Type::Task(item) = task.as_ref() else {
            unreachable!()
        };
        let Type::Union(result_id, arguments) = result else {
            unreachable!()
        };
        let array_type = &arguments[0];
        let Type::Array(element) = array_type else {
            unreachable!()
        };
        let error_type = &arguments[1];
        let declaration = &self.module.unions[*result_id];
        let ok_case = declaration
            .cases
            .iter()
            .position(|(name, _)| name == "Ok")
            .unwrap();
        let error_case = declaration
            .cases
            .iter()
            .position(|(name, _)| name == "Error")
            .unwrap();
        let descriptor = self.expression(tasks);
        let source = self.value(format!("extractvalue %tz.array {descriptor}, 0"));
        let length = self.value(format!("extractvalue %tz.array {descriptor}, 1"));
        let (_, results) = self.allocate_array(item, &length);
        let (_, started) = self.allocate_array(&Type::Integer(8, false), &length);
        self.array_loop(&length, |emitter, index| {
            let pointer = emitter.value(format!("getelementptr i8, ptr {started}, i64 {index}"));
            emitter.instruction(format!("store i8 0, ptr {pointer}"));
        });
        let callback = format!("@tz.task.item.results.{}", mangled_type(item, self.module));
        if let Some(marks) = &mut self.globals.traps {
            marks
                .sources
                .entry(callback.clone())
                .or_insert((self.current_span, false));
        }
        let mut emitter = FunctionEmitter::new(
            self.module,
            self.function,
            self.function_id,
            self.builtins,
            self.intrinsics,
            self.globals,
            self.specializations,
        );
        emitter.current_span = self.current_span;
        let source_value = emitter.value("load ptr, ptr %context");
        let results_slot =
            emitter.value("getelementptr { ptr, ptr, ptr }, ptr %context, i32 0, i32 1");
        let results_value = emitter.value(format!("load ptr, ptr {results_slot}"));
        let started_slot =
            emitter.value("getelementptr { ptr, ptr, ptr }, ptr %context, i32 0, i32 2");
        let started_value = emitter.value(format!("load ptr, ptr {started_slot}"));
        let task_pointer = emitter.value(format!(
            "getelementptr %tz.closure, ptr {source_value}, i64 %index"
        ));
        let task_value = emitter.value(format!("load %tz.closure, ptr {task_pointer}"));
        let code = emitter.value(format!("extractvalue %tz.closure {task_value}, 0"));
        let environment = emitter.value(format!("extractvalue %tz.closure {task_value}, 1"));
        let value = emitter.value(format!(
            "call {} {code}(ptr {environment})",
            emitter.ty(item)
        ));
        let destination = emitter.value(format!(
            "getelementptr {}, ptr {results_value}, i64 %index",
            emitter.ty(item)
        ));
        emitter.instruction(format!(
            "store {} {value}, ptr {destination}",
            emitter.ty(item)
        ));
        let flag = emitter.value(format!("getelementptr i8, ptr {started_value}, i64 %index"));
        emitter.instruction(format!("store i8 1, ptr {flag}"));
        let tag = if emitter.module.types().recursive(item) {
            emitter.recursive_tag(item, &value)
        } else {
            emitter.value(format!("extractvalue {} {value}, 0", emitter.ty(item)))
        };
        let failed = emitter.value(format!("icmp eq i32 {tag}, {error_case}"));
        let failed = emitter.value(format!("zext i1 {failed} to i32"));
        emitter.instruction(format!("ret i32 {failed}"));
        let definition = emitter.auxiliary(&format!("i32 {callback}(ptr %context, i64 %index)"));
        self.intrinsics.insert(definition);
        let context = self.fresh();
        self.allocas
            .push(format!("{context} = alloca {{ ptr, ptr, ptr }}, align 16"));
        for (index, value) in [&source, &results, &started].into_iter().enumerate() {
            let slot = self.value(format!(
                "getelementptr {{ ptr, ptr, ptr }}, ptr {context}, i32 0, i32 {index}"
            ));
            self.instruction(format!("store ptr {value}, ptr {slot}"));
        }
        let failure = self.value(format!(
            "call i64 @tsuzuri_task_parallel_results(ptr {callback}, ptr {context}, i64 {length})"
        ));
        let success = self.value(format!("icmp eq i64 {failure}, -1"));
        let ok = self.label();
        let error = self.label();
        let done = self.label();
        let output = self.spill(result, "zeroinitializer");
        self.branch(&success, &ok, &error);
        self.begin(&ok);
        let (array, target) = self.allocate_array(element, &length);
        self.array_loop(&length, |emitter, index| {
            let pointer = emitter.value(format!(
                "getelementptr {}, ptr {results}, i64 {index}",
                emitter.ty(item)
            ));
            let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(item)));
            let payload = emitter.payload_value(item, &value, element);
            let destination = emitter.value(format!(
                "getelementptr {}, ptr {target}, i64 {index}",
                emitter.ty(element)
            ));
            emitter.instruction(format!(
                "store {} {payload}, ptr {destination}",
                emitter.ty(element)
            ));
            if emitter.module.types().recursive(item) {
                emitter.instruction(format!("call void @tz.free(ptr {value})"));
            }
        });
        let value = self.construct_value(result, ok_case, Some((array, self.ty(array_type))));
        self.instruction(format!("store {} {value}, ptr {output}", self.ty(result)));
        self.jump(&done);
        self.begin(&error);
        let pointer = self.value(format!(
            "getelementptr {}, ptr {results}, i64 {failure}",
            self.ty(item)
        ));
        let chosen = self.value(format!("load {}, ptr {pointer}", self.ty(item)));
        let payload = self.payload_value(item, &chosen, error_type);
        self.array_loop(&length, |emitter, index| {
            let flag = emitter.value(format!("getelementptr i8, ptr {started}, i64 {index}"));
            let initialized = emitter.value(format!("load i8, ptr {flag}"));
            let ran = emitter.value(format!("icmp ne i8 {initialized}, 0"));
            let drop_result = emitter.label();
            let drop_task = emitter.label();
            let next = emitter.label();
            emitter.branch(&ran, &drop_result, &drop_task);
            emitter.begin(&drop_result);
            let selected = emitter.value(format!("icmp eq i64 {index}, {failure}"));
            let discard = emitter.label();
            emitter.branch(&selected, &next, &discard);
            emitter.begin(&discard);
            let pointer = emitter.value(format!(
                "getelementptr {}, ptr {results}, i64 {index}",
                emitter.ty(item)
            ));
            let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(item)));
            emitter.drop_value(item, &value);
            emitter.jump(&next);
            emitter.begin(&drop_task);
            let pointer = emitter.value(format!(
                "getelementptr %tz.closure, ptr {source}, i64 {index}"
            ));
            let value = emitter.value(format!("load %tz.closure, ptr {pointer}"));
            emitter.drop_value(task, &value);
            emitter.jump(&next);
            emitter.begin(&next);
        });
        if self.module.types().recursive(item) {
            self.instruction(format!("call void @tz.free(ptr {chosen})"));
        }
        let value = self.construct_value(result, error_case, Some((payload, self.ty(error_type))));
        self.instruction(format!("store {} {value}, ptr {output}", self.ty(result)));
        self.jump(&done);
        self.begin(&done);
        for pointer in [source, results, started] {
            self.instruction(format!("call void @tz.free(ptr {pointer})"));
        }
        self.value(format!("load {}, ptr {output}", self.ty(result)))
    }
}
