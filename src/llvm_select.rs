use super::*;
use crate::check::{SelectWait, SelectWaitArm};

/// One case of `tsuzuri_tz_select` (`tzr_scase`, TZ01): the channel block, the item's slot,
/// whether it sends, padding, and the runtime's sudog.
const CASE: &str = "{ ptr, ptr, i32, i32, [6 x i64] }";
const CASE_BYTES: usize = 72;
/// Cases of a select kept in the frame; a larger select takes them from the heap, so that a select
/// of many arms cannot step over the guard of a tzroutine's stack.
const FRAME_CASES: usize = 64;

impl FunctionEmitter<'_, '_> {
    /// The waiting part of a `select` (TZ04): evaluates the arms in order, waits in the tzroutine
    /// runtime, drops the values of the sends that were not chosen and what `recv _` got, and gives
    /// `(arm, Maybe<item> of each kept receive)`.
    pub(super) fn select_wait(&mut self, wait: &SelectWait, ty: &Type) -> String {
        let Type::Tuple(fields) = ty else {
            unreachable!("a select gives a tuple")
        };
        let count = wait.arms.len();
        let tuple = self.ty(ty);
        let (chosen, status, slots) = if count == 0 {
            // Only `default`: nothing to wait for.
            let default = wait.default.expect("a select has an arm");
            (default.to_string(), String::new(), Vec::new())
        } else {
            self.select_cases(wait)
        };
        let mut result = self.value(format!(
            "insertvalue {tuple} zeroinitializer, i64 {chosen}, 0"
        ));
        if fields.len() == 2 && fields[1] == Type::Unit {
            return result;
        }
        for (field, maybe) in fields.iter().enumerate().skip(1) {
            let position = wait
                .arms
                .iter()
                .position(|arm| arm.field == Some(field))
                .expect("a field belongs to a receive arm");
            let arm = &wait.arms[position];
            let value = self.select_received(arm, maybe, &chosen, &status, &slots[position]);
            result = self.value(format!(
                "insertvalue {tuple} {result}, {} {value}, {field}",
                self.ty(maybe)
            ));
        }
        result
    }

    /// Fills the cases, waits, and settles the arms that were not chosen; returns the chosen arm's
    /// number as an `i64`, the runtime's `ok` and the arms' item slots.
    fn select_cases(&mut self, wait: &SelectWait) -> (String, String, Vec<String>) {
        let count = wait.arms.len();
        let heap = count > FRAME_CASES;
        let (cases, numbers) = if heap {
            let cases = self.value(format!("call ptr @tz.alloc(i64 {})", count * CASE_BYTES));
            let numbers = self.value(format!("call ptr @tz.alloc(i64 {})", count * 4));
            (cases, numbers)
        } else {
            let cases = self.fresh();
            self.allocas
                .push(format!("{cases} = alloca [{count} x {CASE}], align 16"));
            let numbers = self.fresh();
            self.allocas
                .push(format!("{numbers} = alloca [{count} x i32], align 4"));
            (cases, numbers)
        };
        let filled = self.slot(&Type::I64);
        let ok = self.fresh();
        self.allocas.push(format!("{ok} = alloca i32, align 4"));
        self.instruction(format!("store i64 0, ptr {filled}"));
        // Each arm's item slot, and whether a send arm's value is in its slot.
        let mut slots = Vec::with_capacity(count);
        let mut sent = Vec::with_capacity(count);
        for arm in &wait.arms {
            slots.push(self.slot(&arm.item));
            sent.push(arm.send.as_ref().map(|_| {
                let flag = self.fresh();
                self.allocas.push(format!("{flag} = alloca i1, align 1"));
                self.instruction(format!("store i1 false, ptr {flag}"));
                flag
            }));
        }
        for (position, arm) in wait.arms.iter().enumerate() {
            let next = self.label();
            if let Some(guard) = &arm.guard {
                let enabled = self.expression(guard);
                let on = self.label();
                self.branch(&enabled, &on, &next);
                self.begin(&on);
            }
            // A false guard skips the channel and the value (TSK-021).
            let temporaries = self.temporaries.len();
            let channel = self.expression(&arm.channel);
            let Type::Reference(record, false) = &arm.channel.ty else {
                unreachable!("a select arm borrows its channel")
            };
            let (block, _) = self.channel_block(record, &channel);
            if let Some(value) = &arm.send {
                let item = self.expression(value);
                self.forget_temporary(&item);
                self.instruction(format!(
                    "store {} {item}, ptr {}",
                    self.ty(&arm.item),
                    slots[position]
                ));
                let flag = sent[position].as_ref().expect("a send arm has a flag");
                self.instruction(format!("store i1 true, ptr {flag}"));
            }
            let index = self.value(format!("load i64, ptr {filled}"));
            let case = self.value(format!(
                "getelementptr inbounds {CASE}, ptr {cases}, i64 {index}"
            ));
            let fields = [
                ("ptr", block.as_str(), 0),
                ("ptr", slots[position].as_str(), 1),
                ("i32", if arm.send.is_some() { "1" } else { "0" }, 2),
                ("i32", "0", 3),
            ];
            for (ty, value, field) in fields {
                let pointer = self.value(format!(
                    "getelementptr inbounds {CASE}, ptr {case}, i32 0, i32 {field}"
                ));
                self.instruction(format!("store {ty} {value}, ptr {pointer}"));
            }
            let number = self.value(format!(
                "getelementptr inbounds i32, ptr {numbers}, i64 {index}"
            ));
            self.instruction(format!("store i32 {}, ptr {number}", arm.arm));
            let more = self.value(format!("add i64 {index}, 1"));
            self.instruction(format!("store i64 {more}, ptr {filled}"));
            self.jump(&next);
            self.temporaries.truncate(temporaries);
            self.begin(&next);
        }
        if wait.default.is_none() {
            // A select that waits is a wait (TZ02: never inside Mutex.with_lock).
            self.routine_unlocked();
        }
        self.intrinsics
            .insert("declare i64 @tsuzuri_tz_select(ptr, i64, i32, ptr)".into());
        let total = self.value(format!("load i64, ptr {filled}"));
        let blocking = i32::from(wait.default.is_none());
        let index = self.value(format!(
            "call i64 @tsuzuri_tz_select(ptr {cases}, i64 {total}, i32 {blocking}, ptr {ok})"
        ));
        // -2: the deadlock detector ended the wait of this thread; it said why.
        let alive = self.value(format!("icmp ne i64 {index}, -2"));
        self.guard(&alive, TrapKind::Assert);
        let chosen = match wait.default {
            Some(default) => {
                let none = self.value(format!("icmp eq i64 {index}, -1"));
                let at = self.value(format!("select i1 {none}, i64 0, i64 {index}"));
                let number = self.select_number(&numbers, &at);
                self.value(format!("select i1 {none}, i64 {default}, i64 {number}"))
            }
            None => self.select_number(&numbers, &index),
        };
        if heap {
            self.instruction(format!("call void @tz.free(ptr {cases})"));
            self.instruction(format!("call void @tz.free(ptr {numbers})"));
        }
        let status = self.value(format!("load i32, ptr {ok}"));
        for (position, arm) in wait.arms.iter().enumerate() {
            let this = self.value(format!("icmp eq i64 {chosen}, {}", arm.arm));
            if let Some(flag) = &sent[position] {
                // The value of a send that was not chosen is still in its slot.
                let filled = self.value(format!("load i1, ptr {flag}"));
                let other = self.value(format!("xor i1 {this}, true"));
                let drop = self.value(format!("and i1 {filled}, {other}"));
                self.select_drop_if(&drop, &arm.item, &slots[position]);
                // A chosen send to a closed channel traps, as Channel.send does.
                let closed = self.value(format!("icmp eq i32 {status}, 0"));
                let failed = self.value(format!("and i1 {this}, {closed}"));
                let tell = self.label();
                let fine = self.label();
                self.branch(&failed, &tell, &fine);
                self.begin(&tell);
                self.intrinsics
                    .insert("declare void @tsuzuri_tz_report(i32)".into());
                self.instruction("call void @tsuzuri_tz_report(i32 1)");
                self.emit_trap(TrapKind::Assert);
                self.instruction("unreachable");
                self.begin(&fine);
            } else if arm.field.is_none() {
                // `recv _`: what came is dropped before the body.
                let got = self.value(format!("icmp eq i32 {status}, 1"));
                let drop = self.value(format!("and i1 {this}, {got}"));
                self.select_drop_if(&drop, &arm.item, &slots[position]);
            }
        }
        (chosen, status, slots)
    }

    /// The arm number at `index` of the filled cases, as an `i64`.
    fn select_number(&mut self, numbers: &str, index: &str) -> String {
        let pointer = self.value(format!(
            "getelementptr inbounds i32, ptr {numbers}, i64 {index}"
        ));
        let number = self.value(format!("load i32, ptr {pointer}"));
        self.value(format!("sext i32 {number} to i64"))
    }

    /// Drops the item in `slot` when `condition` holds.
    fn select_drop_if(&mut self, condition: &str, item: &Type, slot: &str) {
        if !item.needs_drop(&self.module.types()) {
            return;
        }
        let yes = self.label();
        let done = self.label();
        self.branch(condition, &yes, &done);
        self.begin(&yes);
        let value = self.value(format!("load {}, ptr {slot}", self.ty(item)));
        self.drop_value(item, &value);
        self.jump(&done);
        self.begin(&done);
    }

    /// What a kept receive arm gives: `Some item` when it was chosen and got one, else `None`.
    fn select_received(
        &mut self,
        arm: &SelectWaitArm,
        maybe: &Type,
        chosen: &str,
        status: &str,
        slot: &str,
    ) -> String {
        let this = self.value(format!("icmp eq i64 {chosen}, {}", arm.arm));
        let got = self.value(format!("icmp eq i32 {status}, 1"));
        let received = self.value(format!("and i1 {this}, {got}"));
        let some = self.label();
        let none = self.label();
        let done = self.label();
        self.branch(&received, &some, &none);
        self.begin(&some);
        let item = self.value(format!("load {}, ptr {slot}", self.ty(&arm.item)));
        let present = self.channel_case(maybe, "Some", Some((&arm.item, &item)));
        let some_block = self.block.clone();
        self.jump(&done);
        self.begin(&none);
        let absent = self.channel_case(maybe, "None", None);
        let none_block = self.block.clone();
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi {} [ {present}, %{some_block} ], [ {absent}, %{none_block} ]",
            self.ty(maybe)
        ))
    }
}
