use super::*;
use crate::check::SharedOperation;

/// The heap block behind a shared pointer to a value of one type (C10 Phase 2): the strong
/// count, the weak count, and the value. The strong pointers together hold one weak count, so
/// the block lives until the value is dropped and every `Weak` is gone. A value that stores a
/// recursive type may hold a pointer to another such block, so its block starts with the
/// `%tz.rec.header` links and its drop is queued on the pending list of the recursive drop loop
/// instead of recursing.
struct Block {
    ty: String,
    strong: usize,
    weak: usize,
    value: usize,
    deferred: bool,
}

fn shared_block(value: &Type, module: &CheckedModule) -> Block {
    let llvm = llvm_type(value, module);
    if value.reaches_recursive(&module.types()) {
        Block {
            ty: format!("{{ ptr, ptr, i64, i64, {llvm} }}"),
            strong: 2,
            weak: 3,
            value: 4,
            deferred: true,
        }
    } else {
        Block {
            ty: format!("{{ i64, i64, {llvm} }}"),
            strong: 0,
            weak: 1,
            value: 2,
            deferred: false,
        }
    }
}

/// The drop loop action of a deferred block, which drops the value and the strong pointers'
/// weak count.
fn action(value: &Type, atomic: bool, module: &CheckedModule) -> String {
    format!(
        "@\"tz.shared.drop.{}.{}\"",
        if atomic { "arc" } else { "rc" },
        canonical_type(value, module)
    )
}

/// The strong pointer of a `Rc`/`Arc` builtin's first type argument.
fn strong(value: &Type, kind: SharedKind) -> Type {
    Type::Shared(Box::new(value.clone()), kind)
}

/// Defines the drop actions of the deferred blocks that the program releases.
pub(super) fn emit_action(
    value: &Type,
    atomic: bool,
    module: &CheckedModule,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut drop = FunctionEmitter::new(
        module,
        &module.functions[0],
        0,
        builtins,
        intrinsics,
        globals,
        specializations,
    );
    drop.drop_pending = Some("%pending".into());
    let layout = shared_block(value, module);
    if value.needs_drop(&module.types()) {
        let pointer = drop.shared_field(&layout, "%node", layout.value);
        let stored = drop.value(format!("load {}, ptr {pointer}", drop.ty(value)));
        drop.drop_value(value, &stored);
    }
    drop.release_shared_block(&layout, atomic, "%node");
    drop.instruction("ret void");
    drop.auxiliary(&format!(
        "void {}(ptr %node, ptr %pending)",
        action(value, atomic, module)
    ))
}

impl FunctionEmitter<'_, '_> {
    fn shared_field(&mut self, layout: &Block, block: &str, field: usize) -> String {
        self.value(format!(
            "getelementptr inbounds {}, ptr {block}, i32 0, i32 {field}",
            layout.ty
        ))
    }

    /// Reads a count. `Arc` counts change on other tasks, so they are read atomically.
    fn shared_load(&mut self, counter: &str, atomic: bool) -> String {
        if atomic {
            self.value(format!("load atomic i64, ptr {counter} monotonic, align 8"))
        } else {
            self.value(format!("load i64, ptr {counter}, align 8"))
        }
    }

    /// Adds one to a strong or weak count and traps when the count would pass `i64::MAX`.
    /// An increment needs no ordering: the new pointer comes from one the task already holds.
    fn shared_increment(&mut self, counter: &str, atomic: bool) {
        let previous = if atomic {
            self.value(format!(
                "atomicrmw add ptr {counter}, i64 1 monotonic, align 8"
            ))
        } else {
            self.shared_load(counter, false)
        };
        let next = self.value(format!("add i64 {previous}, 1"));
        let valid = self.value(format!("icmp sgt i64 {next}, 0"));
        self.guard(&valid, TrapKind::NumericRuntime);
        if !atomic {
            self.instruction(format!("store i64 {next}, ptr {counter}, align 8"));
        }
    }

    /// Subtracts one from a count and returns whether it reached zero. The `Arc` decrement
    /// releases this task's accesses, and the task that reaches zero acquires them all before
    /// it destroys anything (`shared_acquire`).
    fn shared_decrement(&mut self, counter: &str, atomic: bool) -> String {
        if atomic {
            let previous = self.value(format!(
                "atomicrmw sub ptr {counter}, i64 1 release, align 8"
            ));
            self.value(format!("icmp eq i64 {previous}, 1"))
        } else {
            let previous = self.shared_load(counter, false);
            let next = self.value(format!("sub i64 {previous}, 1"));
            self.instruction(format!("store i64 {next}, ptr {counter}, align 8"));
            self.value(format!("icmp eq i64 {next}, 0"))
        }
    }

    /// Orders the destruction after every other task's last access to an `Arc` block: an
    /// acquire fence, which pairs with the release decrements.
    fn shared_acquire(&mut self, atomic: bool) {
        if atomic {
            self.instruction("fence acquire");
        }
    }

    /// Drops one strong or weak pointer. The last strong pointer drops the value, then the
    /// weak count that the strong pointers hold; a deferred block does both in its drop action.
    pub(super) fn drop_shared(&mut self, value: &Type, kind: SharedKind, block: &str) {
        let layout = shared_block(value, self.module);
        let atomic = kind.atomic();
        // Moved-out storage is zero, like the other owned values.
        let moved = self.value(format!("icmp eq ptr {block}, null"));
        let live = self.label();
        let done = self.label();
        self.branch(&moved, &done, &live);
        self.begin(&live);
        if kind.weak() {
            self.release_shared_block(&layout, atomic, block);
        } else {
            let counter = self.shared_field(&layout, block, layout.strong);
            let last = self.shared_decrement(&counter, atomic);
            let destroy = self.label();
            self.branch(&last, &destroy, &done);
            self.begin(&destroy);
            self.shared_acquire(atomic);
            if layout.deferred {
                self.globals.shared_types.insert((value.clone(), atomic));
                let slot = self.shared_field(&layout, block, 1);
                self.instruction(format!(
                    "store ptr {}, ptr {slot}",
                    action(value, atomic, self.module)
                ));
                if let Some(pending) = &self.drop_pending {
                    self.instruction(format!(
                        "call void @tz.rec.enqueue(ptr {block}, ptr {pending})"
                    ));
                } else {
                    self.instruction(format!("call void @tz.rec.drop(ptr {block})"));
                }
            } else {
                if value.needs_drop(&self.module.types()) {
                    let pointer = self.shared_field(&layout, block, layout.value);
                    let stored = self.value(format!("load {}, ptr {pointer}", self.ty(value)));
                    self.drop_value(value, &stored);
                }
                self.release_shared_block(&layout, atomic, block);
            }
        }
        self.jump(&done);
        self.begin(&done);
    }

    /// Drops one weak count and frees the block with the last one.
    fn release_shared_block(&mut self, layout: &Block, atomic: bool, block: &str) {
        let counter = self.shared_field(layout, block, layout.weak);
        let last = self.shared_decrement(&counter, atomic);
        let free = self.label();
        let done = self.label();
        self.branch(&last, &free, &done);
        self.begin(&free);
        self.shared_acquire(atomic);
        self.instruction(format!("call void @tz.free(ptr {block})"));
        self.jump(&done);
        self.begin(&done);
    }

    /// Copying a function value that captured a shared pointer shares it.
    pub(super) fn clone_shared(&mut self, value: &Type, kind: SharedKind, block: &str) -> String {
        let layout = shared_block(value, self.module);
        let field = if kind.weak() {
            layout.weak
        } else {
            layout.strong
        };
        let counter = self.shared_field(&layout, block, field);
        self.shared_increment(&counter, kind.atomic());
        block.to_owned()
    }

    pub(super) fn shared_builtin(
        &mut self,
        instance: &BuiltinInstance,
        signature: &Type,
    ) -> String {
        let value = &instance.types[0];
        let kind = instance
            .builtin
            .shared_kind()
            .expect("a shared pointer builtin");
        let atomic = kind.atomic();
        let layout = shared_block(value, self.module);
        let operation = instance.builtin.shared_operation();
        let result = signature.after_arguments(instance.builtin.scheme().parameters.len());
        // Every operation but `new` and `try_unwrap` borrows its pointer.
        let block = if matches!(operation, SharedOperation::New | SharedOperation::TryUnwrap) {
            "%arg0".to_owned()
        } else {
            self.value("load ptr, ptr %arg0")
        };
        match operation {
            SharedOperation::New => {
                let block = self.value(format!(
                    "call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({}, ptr null, i32 1) to i64))",
                    layout.ty
                ));
                // Nothing else sees the block yet, so plain stores initialize `Arc` counts too.
                for field in [layout.strong, layout.weak] {
                    let counter = self.shared_field(&layout, &block, field);
                    self.instruction(format!("store i64 1, ptr {counter}, align 8"));
                }
                let pointer = self.shared_field(&layout, &block, layout.value);
                self.instruction(format!("store {} %arg0, ptr {pointer}", self.ty(value)));
                block
            }
            SharedOperation::Share => {
                let counter = self.shared_field(&layout, &block, layout.strong);
                self.shared_increment(&counter, atomic);
                block
            }
            SharedOperation::Get => {
                let pointer = self.shared_field(&layout, &block, layout.value);
                self.borrowed_element(value, &pointer)
            }
            SharedOperation::StrongCount => {
                let counter = self.shared_field(&layout, &block, layout.strong);
                self.shared_load(&counter, atomic)
            }
            SharedOperation::WeakCount => {
                // The strong pointers' own weak count is not a `Weak`.
                let counter = self.shared_field(&layout, &block, layout.weak);
                let count = self.shared_load(&counter, atomic);
                self.value(format!("sub i64 {count}, 1"))
            }
            SharedOperation::Downgrade => {
                let counter = self.shared_field(&layout, &block, layout.weak);
                self.shared_increment(&counter, atomic);
                block
            }
            SharedOperation::PtrEq => {
                let other = self.value("load ptr, ptr %arg1");
                self.value(format!("icmp eq ptr {block}, {other}"))
            }
            SharedOperation::TryUnwrap => {
                self.shared_try_unwrap(value, kind, &layout, &block, &result)
            }
            SharedOperation::Upgrade => self.shared_upgrade(value, kind, &layout, &block, &result),
        }
    }

    /// `Ok value` when this is the only strong pointer, else `Error` with the pointer back.
    fn shared_try_unwrap(
        &mut self,
        value: &Type,
        kind: SharedKind,
        layout: &Block,
        block: &str,
        result: &Type,
    ) -> String {
        let counter = self.shared_field(layout, block, layout.strong);
        let only = if kind.atomic() {
            // Claims the value only while no other strong pointer exists; a `Weak` cannot
            // upgrade a zero count, so no task gets the value back.
            let pair = self.value(format!(
                "cmpxchg ptr {counter}, i64 1, i64 0 monotonic monotonic, align 8"
            ));
            self.value(format!("extractvalue {{ i64, i1 }} {pair}, 1"))
        } else {
            let count = self.shared_load(&counter, false);
            self.value(format!("icmp eq i64 {count}, 1"))
        };
        let unwrap = self.label();
        let keep = self.label();
        let done = self.label();
        self.branch(&only, &unwrap, &keep);
        self.begin(&unwrap);
        if !kind.atomic() {
            self.instruction(format!("store i64 0, ptr {counter}, align 8"));
        }
        self.shared_acquire(kind.atomic());
        let pointer = self.shared_field(layout, block, layout.value);
        let moved = self.value(format!("load {}, ptr {pointer}", self.ty(value)));
        self.release_shared_block(layout, kind.atomic(), block);
        let ok = self.std_case(result, "Ok", Some((value, &moved)));
        let unwrapped = self.block.clone();
        self.jump(&done);
        self.begin(&keep);
        let error = self.std_case(result, "Error", Some((&strong(value, kind), block)));
        let kept = self.block.clone();
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi {} [ {ok}, %{unwrapped} ], [ {error}, %{kept} ]",
            self.ty(result)
        ))
    }

    /// `Some` new strong pointer while the value is alive, else `None`. An `Arc` adds the
    /// strong count only if no task has dropped it to zero in between.
    fn shared_upgrade(
        &mut self,
        value: &Type,
        kind: SharedKind,
        layout: &Block,
        block: &str,
        result: &Type,
    ) -> String {
        let strong_type = strong(value, kind.strong());
        let counter = self.shared_field(layout, block, layout.strong);
        let some = self.label();
        let none = self.label();
        let done = self.label();
        if kind.atomic() {
            let initial = self.shared_load(&counter, true);
            let entry = self.block.clone();
            let check = self.label();
            let attempt = self.label();
            let retry = self.label();
            self.jump(&check);
            self.begin(&check);
            let current = self.fresh();
            let observed = self.fresh();
            self.instruction(format!(
                "{current} = phi i64 [ {initial}, %{entry} ], [ {observed}, %{retry} ]"
            ));
            let dead = self.value(format!("icmp eq i64 {current}, 0"));
            self.branch(&dead, &none, &attempt);
            self.begin(&attempt);
            let next = self.value(format!("add i64 {current}, 1"));
            let valid = self.value(format!("icmp sgt i64 {next}, 0"));
            self.guard(&valid, TrapKind::NumericRuntime);
            let pair = self.value(format!(
                "cmpxchg ptr {counter}, i64 {current}, i64 {next} acquire monotonic, align 8"
            ));
            self.instruction(format!("{observed} = extractvalue {{ i64, i1 }} {pair}, 0"));
            let success = self.value(format!("extractvalue {{ i64, i1 }} {pair}, 1"));
            self.branch(&success, &some, &retry);
            self.begin(&retry);
            self.jump(&check);
        } else {
            let count = self.shared_load(&counter, false);
            let dead = self.value(format!("icmp eq i64 {count}, 0"));
            let alive = self.label();
            self.branch(&dead, &none, &alive);
            self.begin(&alive);
            self.shared_increment(&counter, false);
            self.jump(&some);
        }
        self.begin(&some);
        let present = self.std_case(result, "Some", Some((&strong_type, block)));
        let present_block = self.block.clone();
        self.jump(&done);
        self.begin(&none);
        let absent = self.std_case(result, "None", None);
        let absent_block = self.block.clone();
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi {} [ {present}, %{present_block} ], [ {absent}, %{absent_block} ]",
            self.ty(result)
        ))
    }

    /// A value of the std `Maybe` or `Result` union `result` with the named case.
    fn std_case(&mut self, result: &Type, case: &str, payload: Option<(&Type, &str)>) -> String {
        let Type::Union(id, _) = result else {
            unreachable!("shared pointer results are std unions")
        };
        let index = self.module.unions[*id]
            .cases
            .iter()
            .position(|(name, _)| name == case)
            .expect("std case exists");
        let payload = payload.map(|(ty, value)| (value.to_owned(), self.ty(ty)));
        self.construct_value(result, index, payload)
    }
}
