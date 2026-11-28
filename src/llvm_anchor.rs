use super::*;

const ANCHOR_CREATE: &str = "declare i64 @tsuzuri_tz_anchor_create(ptr, ptr, ptr, ptr, ptr)";
const ANCHOR_BEGIN: &str = "declare i32 @tsuzuri_tz_anchor_begin(i64, ptr, i32, ptr)";
const ANCHOR_END: &str = "declare i32 @tsuzuri_tz_anchor_end(i64, i32)";
const ANCHOR_RELEASE: &str = "declare void @tsuzuri_tz_anchor_release(i64)";
const ANCHOR_CREATE_SHARED: &str =
    "declare i64 @tsuzuri_tz_anchor_create_shared(ptr, ptr, ptr, ptr, ptr)";
const ANCHOR_BEGIN_SHARED: &str = "declare i32 @tsuzuri_tz_anchor_begin_shared(i64, ptr, ptr)";
const ANCHOR_DETACH: &str = "declare void @tsuzuri_tz_anchor_detach(i64)";
const ANCHOR_RELEASE_SHARED: &str = "declare i32 @tsuzuri_tz_anchor_release_shared(i64)";

/// Emits the unique type descriptor (`unnamed_addr` omitted so `llvm_split` keeps one symbol
/// across codegen units) and box drop helper for an anchored value type `value`.
pub(super) fn emit_type_helpers(
    value: &Type,
    module: &CheckedModule,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mangled = mangled_type(value, module);
    let mut output = format!("@tz.anchor.type.{mangled} = internal constant i8 0\n");
    let mut drop = FunctionEmitter::new(
        module,
        &module.functions[0],
        0,
        builtins,
        intrinsics,
        globals,
        specializations,
    );
    if value.needs_drop(&module.types()) {
        let stored = drop.value(format!("load {}, ptr %box", drop.ty(value)));
        drop.drop_value(value, &stored);
    }
    drop.instruction("call void @tz.free(ptr %box)");
    drop.instruction("ret void");
    output.push_str(&drop.auxiliary(&format!("void @tz.anchor.drop.{mangled}(ptr %box)")));
    output
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn anchor_builtin(&mut self, instance: &BuiltinInstance, ty: &Type) -> String {
        let Type::Function(parameters, _) = ty else {
            unreachable!("a builtin has function type")
        };
        let count = instance.builtin.scheme().parameters.len();
        let result = ty.after_arguments(count);
        match instance.builtin {
            Builtin::AnchorRelease => {
                self.intrinsics.insert(ANCHOR_RELEASE.into());
                self.instruction("call void @tsuzuri_tz_anchor_release(i64 %arg0)");
                "0".to_owned()
            }
            Builtin::AnchorDetach => {
                self.intrinsics.insert(ANCHOR_DETACH.into());
                self.instruction("call void @tsuzuri_tz_anchor_detach(i64 %arg0)");
                "0".to_owned()
            }
            Builtin::AnchorReleaseShared => {
                self.intrinsics.insert(ANCHOR_RELEASE_SHARED.into());
                let released = self.value("call i32 @tsuzuri_tz_anchor_release_shared(i64 %arg0)");
                self.value(format!("icmp ne i32 {released}, 0"))
            }
            Builtin::AnchorCreate | Builtin::AnchorCreateShared => {
                let (declaration, create) = if instance.builtin == Builtin::AnchorCreateShared {
                    (ANCHOR_CREATE_SHARED, "tsuzuri_tz_anchor_create_shared")
                } else {
                    (ANCHOR_CREATE, "tsuzuri_tz_anchor_create")
                };
                let value_ty = codegen_identity(&instance.types[0]);
                let mangled = mangled_type(&value_ty, self.module);
                self.globals.anchor_types.insert(value_ty.clone());
                let llvm_val = self.ty(&value_ty);
                let raw =
                    format!("ptrtoint (ptr getelementptr ({llvm_val}, ptr null, i32 1) to i64)");
                let empty = self.value(format!("icmp eq i64 {raw}, 0"));
                let size = self.value(format!("select i1 {empty}, i64 1, i64 {raw}"));
                let boxed = self.value(format!("call ptr @tz.alloc(i64 {size})"));
                self.instruction(format!("store {llvm_val} %arg0, ptr {boxed}"));
                self.intrinsics.insert(declaration.into());
                let token = self.value(format!(
                    "call i64 @{create}(ptr {boxed}, ptr @tz.anchor.type.{mangled}, ptr @tz.anchor.drop.{mangled}, ptr @tz.alloc, ptr @tz.free)"
                ));
                let handle = self.value(format!(
                    "insertvalue {} zeroinitializer, i64 {token}, 0",
                    self.ty(&result)
                ));
                self.mark_live(&result, handle)
            }
            Builtin::AnchorRead | Builtin::AnchorReadShared => {
                let value_ty = codegen_identity(&instance.types[0]);
                let out_ty = &instance.types[1];
                let mangled = mangled_type(&value_ty, self.module);
                self.globals.anchor_types.insert(value_ty.clone());
                self.intrinsics.insert(ANCHOR_END.into());
                let box_slot = self.fresh();
                self.allocas
                    .push(format!("{box_slot} = alloca ptr, align 8"));
                self.instruction(format!("store ptr null, ptr {box_slot}"));
                // A shared read begins on any thread; its end is the end of a read (M09 Phase 3).
                let status = if instance.builtin == Builtin::AnchorReadShared {
                    self.intrinsics.insert(ANCHOR_BEGIN_SHARED.into());
                    self.value(format!(
                        "call i32 @tsuzuri_tz_anchor_begin_shared(i64 %arg1, ptr @tz.anchor.type.{mangled}, ptr {box_slot})"
                    ))
                } else {
                    self.intrinsics.insert(ANCHOR_BEGIN.into());
                    self.value(format!(
                        "call i32 @tsuzuri_tz_anchor_begin(i64 %arg1, ptr @tz.anchor.type.{mangled}, i32 0, ptr {box_slot})"
                    ))
                };
                let ok_cond = self.value(format!("icmp eq i32 {status}, 0"));
                let ok_bb = self.label();
                let err_bb = self.label();
                let join_bb = self.label();
                self.branch(&ok_cond, &ok_bb, &err_bb);

                self.begin(&ok_bb);
                let box_ptr = self.value(format!("load ptr, ptr {box_slot}"));
                let borrow_ty = Type::Reference(Box::new(value_ty), false);
                let (produced, _) =
                    self.apply_value("%arg0", &parameters[0], Some((&borrow_ty, &box_ptr)), false);
                let _alive = self.value("call i32 @tsuzuri_tz_anchor_end(i64 %arg1, i32 0)");
                let (ok_case, err_case) = self.anchor_result_cases(&result);
                let ok_result =
                    self.construct_value(&result, ok_case, Some((produced, self.ty(out_ty))));
                let ok_end = self.block.clone();
                self.jump(&join_bb);

                self.begin(&err_bb);
                if parameters[0].needs_drop(&self.module.types()) {
                    self.drop_value(&parameters[0], "%arg0");
                }
                let err_result = self.anchor_error_result(&result, err_case, &status);
                let err_end = self.block.clone();
                self.jump(&join_bb);

                self.begin(&join_bb);
                self.value(format!(
                    "phi {} [ {ok_result}, %{ok_end} ], [ {err_result}, %{err_end} ]",
                    self.ty(&result)
                ))
            }
            Builtin::AnchorUpdate => {
                let value_ty = codegen_identity(&instance.types[0]);
                let mangled = mangled_type(&value_ty, self.module);
                self.globals.anchor_types.insert(value_ty.clone());
                self.intrinsics.insert(ANCHOR_BEGIN.into());
                self.intrinsics.insert(ANCHOR_END.into());
                let box_slot = self.fresh();
                self.allocas
                    .push(format!("{box_slot} = alloca ptr, align 8"));
                self.instruction(format!("store ptr null, ptr {box_slot}"));
                let status = self.value(format!(
                    "call i32 @tsuzuri_tz_anchor_begin(i64 %arg1, ptr @tz.anchor.type.{mangled}, i32 1, ptr {box_slot})"
                ));
                let ok_cond = self.value(format!("icmp eq i32 {status}, 0"));
                let ok_bb = self.label();
                let err_bb = self.label();
                let join_bb = self.label();
                self.branch(&ok_cond, &ok_bb, &err_bb);

                let (ok_case, err_case) = self.anchor_result_cases(&result);

                self.begin(&ok_bb);
                let box_ptr = self.value(format!("load ptr, ptr {box_slot}"));
                let llvm_val = self.ty(&value_ty);
                let old_val = self.value(format!("load {llvm_val}, ptr {box_ptr}"));
                let (new_val, _) =
                    self.apply_value("%arg0", &parameters[0], Some((&value_ty, &old_val)), false);
                let alive = self.value("call i32 @tsuzuri_tz_anchor_end(i64 %arg1, i32 1)");
                let live_cond = self.value(format!("icmp ne i32 {alive}, 0"));
                let store_bb = self.label();
                let stale_bb = self.label();
                self.branch(&live_cond, &store_bb, &stale_bb);

                self.begin(&store_bb);
                self.instruction(format!("store {llvm_val} {new_val}, ptr {box_ptr}"));
                let ok_result =
                    self.construct_value(&result, ok_case, Some(("0".to_owned(), "i8".to_owned())));
                let store_end = self.block.clone();
                self.jump(&join_bb);

                self.begin(&stale_bb);
                if value_ty.needs_drop(&self.module.types()) {
                    self.drop_value(&value_ty, &new_val);
                }
                self.instruction(format!("call void @tz.free(ptr {box_ptr})"));
                let stale_status = "1";
                let stale_result = self.anchor_error_result(&result, err_case, stale_status);
                let stale_end = self.block.clone();
                self.jump(&join_bb);

                self.begin(&err_bb);
                if parameters[0].needs_drop(&self.module.types()) {
                    self.drop_value(&parameters[0], "%arg0");
                }
                let err_result = self.anchor_error_result(&result, err_case, &status);
                let err_end = self.block.clone();
                self.jump(&join_bb);

                self.begin(&join_bb);
                self.value(format!(
                    "phi {} [ {ok_result}, %{store_end} ], [ {stale_result}, %{stale_end} ], [ {err_result}, %{err_end} ]",
                    self.ty(&result)
                ))
            }
            _ => unreachable!("not an Anchor builtin"),
        }
    }

    fn anchor_result_cases(&self, result: &Type) -> (usize, usize) {
        let Type::Union(id, _) = result else {
            unreachable!("Anchor result is std::Result")
        };
        let cases = &self.module.unions[*id].cases;
        let ok = cases.iter().position(|(name, _)| name == "Ok").unwrap();
        let err = cases.iter().position(|(name, _)| name == "Error").unwrap();
        (ok, err)
    }

    fn anchor_error_result(&mut self, result: &Type, err_case: usize, status: &str) -> String {
        let Type::Union(_, args) = result else {
            unreachable!("Anchor result is std::Result")
        };
        let err_ty = &args[1];
        let stale = self.construct_value(err_ty, 0, None);
        let wrong_type = self.construct_value(err_ty, 1, None);
        let wrong_thread = self.construct_value(err_ty, 2, None);
        let busy = self.construct_value(err_ty, 3, None);
        let llvm_err = self.ty(err_ty);
        let is_2 = self.value(format!("icmp eq i32 {status}, 2"));
        let is_3 = self.value(format!("icmp eq i32 {status}, 3"));
        let is_4 = self.value(format!("icmp eq i32 {status}, 4"));
        let s1 = self.value(format!(
            "select i1 {is_2}, {llvm_err} {wrong_type}, {llvm_err} {stale}"
        ));
        let s2 = self.value(format!(
            "select i1 {is_3}, {llvm_err} {wrong_thread}, {llvm_err} {s1}"
        ));
        let chosen = self.value(format!(
            "select i1 {is_4}, {llvm_err} {busy}, {llvm_err} {s2}"
        ));
        self.construct_value(result, err_case, Some((chosen, llvm_err)))
    }
}
