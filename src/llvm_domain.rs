//! B10 Phase 2: the code of `Domain.__run` and `Domain.__message`. A failure domain is
//! `tsuzuri_domain_run` of src/runtime/trap.c, which calls a thunk under a frame of its own: a trap
//! inside jumps back to it, and every block that the call allocated is freed (the program's heap is the
//! tracked one of `--trap-mode return`).
use super::*;

impl FunctionEmitter<'_, '_> {
    /// `Domain.__run body`: `Ok (body ())`, or `Error (site, kind)` when the call trapped. The closure
    /// `%arg0` is borrowed by the call, whose captures are copied inside the domain, so a trap never loses
    /// a block of the caller; it is dropped once, outside the domain, on both paths (ticket appendix C).
    pub(super) fn domain_run(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let value_type = instance.types.first().cloned().unwrap_or(Type::Unit);
        let result = ty.after_arguments(1);
        let Type::Union(result_id, arguments) = &result else {
            unreachable!("Domain.__run returns a Result")
        };
        let error_type = arguments[1].clone();
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
        let function_type = Type::function(vec![Type::Unit], value_type.clone());
        let value_llvm = self.ty(&value_type);
        let context_type = format!("{{ %tz.closure, {value_llvm} }}");
        let thunk = format!(
            "@tz.domain.thunk.{}",
            mangled_type(&value_type, self.module)
        );
        if let Some(marks) = &mut self.globals.traps {
            marks
                .sources
                .entry(thunk.clone())
                .or_insert((self.current_span, false));
        }
        // The thunk that the runtime calls: it runs the closure and stores the value in the context.
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
        let closure_slot = emitter.value(format!(
            "getelementptr {context_type}, ptr %context, i32 0, i32 0"
        ));
        let closure = emitter.value(format!("load %tz.closure, ptr {closure_slot}"));
        let value = emitter
            .apply_value(&closure, &function_type, Some((&Type::Unit, "0")), true)
            .0;
        let value_slot = emitter.value(format!(
            "getelementptr {context_type}, ptr %context, i32 0, i32 1"
        ));
        emitter.instruction(format!("store {value_llvm} {value}, ptr {value_slot}"));
        emitter.instruction("ret void");
        let definition = emitter.auxiliary(&format!("void {thunk}(ptr %context)"));
        self.intrinsics.insert(definition);
        self.intrinsics
            .insert("declare i32 @tsuzuri_domain_run(ptr, ptr, ptr)".into());
        let context = self.fresh();
        self.allocas
            .push(format!("{context} = alloca {context_type}, align 16"));
        let info = self.fresh();
        self.allocas
            .push(format!("{info} = alloca {{ i32, i32 }}, align 4"));
        let closure_slot = self.value(format!(
            "getelementptr {context_type}, ptr {context}, i32 0, i32 0"
        ));
        self.instruction(format!("store %tz.closure %arg0, ptr {closure_slot}"));
        let status = self.value(format!(
            "call i32 @tsuzuri_domain_run(ptr {thunk}, ptr {context}, ptr {info})"
        ));
        let output = self.spill(&result, "zeroinitializer");
        let success = self.value(format!("icmp eq i32 {status}, 0"));
        let ok = self.label();
        let error = self.label();
        let done = self.label();
        self.branch(&success, &ok, &error);
        self.begin(&ok);
        let value_slot = self.value(format!(
            "getelementptr {context_type}, ptr {context}, i32 0, i32 1"
        ));
        let value = self.value(format!("load {value_llvm}, ptr {value_slot}"));
        let constructed = self.construct_value(&result, ok_case, Some((value, value_llvm)));
        self.instruction(format!(
            "store {} {constructed}, ptr {output}",
            self.ty(&result)
        ));
        self.jump(&done);
        self.begin(&error);
        let site_slot = self.value(format!(
            "getelementptr {{ i32, i32 }}, ptr {info}, i32 0, i32 0"
        ));
        let site = self.value(format!("load i32, ptr {site_slot}"));
        let site = self.value(format!("zext i32 {site} to i64"));
        let kind_slot = self.value(format!(
            "getelementptr {{ i32, i32 }}, ptr {info}, i32 0, i32 1"
        ));
        let kind = self.value(format!("load i32, ptr {kind_slot}"));
        let kind = self.value(format!("zext i32 {kind} to i64"));
        let tuple_type = self.ty(&error_type);
        let first = self.value(format!(
            "insertvalue {tuple_type} zeroinitializer, i64 {site}, 0"
        ));
        let pair = self.value(format!("insertvalue {tuple_type} {first}, i64 {kind}, 1"));
        let constructed = self.construct_value(&result, error_case, Some((pair, tuple_type)));
        self.instruction(format!(
            "store {} {constructed}, ptr {output}",
            self.ty(&result)
        ));
        self.jump(&done);
        self.begin(&done);
        self.drop_value(&function_type, "%arg0");
        self.value(format!("load {}, ptr {output}", self.ty(&result)))
    }

    /// `Domain.__message site`: the text of the site from the table that the trap instrumentation adds
    /// (`@tz.trap.text`); empty for a site that the table does not hold. A build without the instrumentation
    /// has no table, and a site there has no text.
    pub(super) fn domain_message(&mut self) -> String {
        if self.globals.traps.is_none() {
            self.intrinsics.insert(
                "define internal ptr @tz.trap.text(i32 %site, ptr %length) cold nounwind {\nentry:\n  store i64 0, ptr %length\n  ret ptr null\n}\n"
                    .into(),
            );
        }
        let length = self.fresh();
        self.allocas.push(format!("{length} = alloca i64, align 8"));
        let site = self.value("trunc i64 %arg0 to i32");
        let text = self.value(format!("call ptr @tz.trap.text(i32 {site}, ptr {length})"));
        let bytes = self.value(format!("load i64, ptr {length}"));
        let string = self.ty(&Type::String);
        self.value(format!(
            "call {string} @tz.string.from_utf8(ptr {text}, i64 {bytes})"
        ))
    }
}
