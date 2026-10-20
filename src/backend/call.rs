//! Calls: arguments and results through the plan of `abi.rs`.

use cranelift_codegen::ir::{self, InstBuilder, SigRef, Value};
use cranelift_codegen::isa::CallConv;

use super::abi::{self, Abi, ParamPlan, RetPlan};
use super::error::{Res, bail};
use super::func::{CallArg, Callee, Inst, NONE, Op};
use super::lower::{Lower, Place, SymClass, Val};
use super::module::{Ext, SymKind};
use super::types::Ty;

enum Target {
    Direct(ir::FuncRef),
    Indirect(SigRef, Value),
}

impl<'w, 'a> Lower<'w, 'a> {
    /// Lowers a `musttail call` and the `ret` of its value that follows as one `return_call`,
    /// which leaves this frame before the callee runs (PM09), and returns whether it did. It does
    /// when the two functions have the tail convention (`abi::plan_tail`: every parameter by value,
    /// so nothing the callee reads is in this frame) and the call is a plain call by name. Any other
    /// call returns `Ok(false)`, and the caller lowers an ordinary call and its `ret`: the program
    /// runs as it should, but this backend does not keep its stack from growing.
    pub(super) fn tail_call(&mut self, call: Inst<'a>, ret: Inst<'a>) -> Res<bool> {
        let Inst::Call {
            dst,
            callee: Callee::Direct(id),
            start,
            len,
            variadic: false,
            ..
        } = call
        else {
            return Ok(false);
        };
        let Inst::Ret { val, .. } = ret else {
            return Ok(false);
        };
        let returns_the_call = if dst == NONE {
            val == Op::NONE
        } else {
            val == Op::Local(dst)
        };
        let SymClass::Function(callee) = &self.syms[id as usize].class else {
            return Ok(false);
        };
        let supported = |abi: &Abi| {
            abi.sig.call_conv == CallConv::Tail
                && abi
                    .params
                    .iter()
                    .all(|plan| !matches!(plan, ParamPlan::Indirect))
                && !matches!(abi.ret, RetPlan::Sret)
        };
        let args = &self.func.call_args[start as usize..(start + len) as usize];
        if !returns_the_call
            || !supported(self.abi)
            || !supported(callee)
            || callee.params.len() != args.len()
            || self.abi.sig.returns != callee.sig.returns
            || args.iter().any(|arg| arg.byval.is_some() || arg.sret)
        {
            return Ok(false);
        }
        let mut values: Vec<Value> = Vec::new();
        for (arg, plan) in args.iter().zip(&callee.params) {
            let value = self.val(arg.op, arg.ty)?;
            match plan {
                ParamPlan::Scalar => values.push(self.scalar(value)?),
                ParamPlan::Leaves(_) => values.extend(self.leaves(value)?),
                ParamPlan::Indirect => unreachable!("a tail call has no parameter in memory"),
            }
        }
        let function = self.func_ref(id, true)?;
        self.b.ins().return_call(function, &values);
        Ok(true)
    }
}

impl<'w, 'a> Lower<'w, 'a> {
    pub(super) fn call(
        &mut self,
        ret: Ty,
        callee: Callee<'a>,
        start: u32,
        len: u32,
        variadic: bool,
    ) -> Res<Val> {
        let func = self.func;
        let args = &func.call_args[start as usize..(start + len) as usize];
        match callee {
            Callee::Intrinsic(name) => self.intrinsic(name, ret, args),
            Callee::Direct(id) => {
                let syms = self.syms;
                let SymClass::Function(abi) = &syms[id as usize].class else {
                    bail!(
                        "a call to @{} which is not a function",
                        self.module.syms[id as usize].name
                    );
                };
                let declared_variadic = matches!(
                    &self.module.syms[id as usize].kind,
                    SymKind::Function { sig, .. } if sig.variadic
                );
                if variadic || declared_variadic {
                    bail!(
                        "a call of the variadic function @{}",
                        self.module.syms[id as usize].name
                    );
                }
                if abi.params.len() != args.len() {
                    bail!(
                        "a call of @{} with {} arguments for {} parameters",
                        self.module.syms[id as usize].name,
                        args.len(),
                        abi.params.len()
                    );
                }
                let function = self.func_ref(id, true)?;
                self.emit_call(Target::Direct(function), abi, ret, args)
            }
            Callee::Indirect(op) => {
                if variadic {
                    bail!("a call of a variadic function through a pointer");
                }
                // The callee may be a function that other objects define, whose convention for these is the platform's.
                if args.iter().any(|arg| arg.byval.is_some() || arg.sret) {
                    bail!("a call through a pointer with a byval or sret argument");
                }
                let params: Vec<(Ty, Ext)> = args.iter().map(|arg| (arg.ty, arg.ext)).collect();
                let abi = abi::plan(self.types, self.target, ret, Ext::None, &params, false)?;
                let pointer = self.scalar_op(op, Ty::Ptr)?;
                let signature = self.b.import_signature(abi.sig.clone());
                self.emit_call(Target::Indirect(signature, pointer), &abi, ret, args)
            }
        }
    }

    fn emit_call(&mut self, target: Target, abi: &Abi, ret: Ty, args: &[CallArg]) -> Res<Val> {
        if abi.external && args.iter().any(|arg| arg.byval.is_some() || arg.sret) {
            bail!("a byval or sret argument of a call of an external function");
        }
        let pointer = self.target.pointer;
        let mut values: Vec<Value> = Vec::new();
        let mut result_address = None;
        if matches!(abi.ret, RetPlan::Sret) {
            let slot = self.new_slot_for(ret)?;
            let address = self.b.ins().stack_addr(pointer, slot, 0);
            values.push(address);
            result_address = Some(address);
        }
        for (arg, plan) in args.iter().zip(&abi.params) {
            let mut value = self.val(arg.op, arg.ty)?;
            if let Some(copied) = arg.byval {
                // The callee owns a private copy of what the pointer points to.
                let size = self.types.size_of(copied)?;
                let align = self.types.align_of(copied)?.max(8);
                let slot = self.new_slot(size, align)?;
                let copy = self.b.ins().stack_addr(pointer, slot, 0);
                let source = self.scalar(value)?;
                self.copy_memory(copy, source, size)?;
                value = Val::S(copy);
            }
            match plan {
                ParamPlan::Scalar => values.push(self.scalar(value)?),
                ParamPlan::Leaves(_) => values.extend(self.leaves(value)?),
                ParamPlan::Indirect => match value {
                    Val::Ref(address) => values.push(address),
                    _ => bail!("a large aggregate argument that is not in memory"),
                },
            }
        }
        let call = match target {
            Target::Direct(function) => self.b.ins().call(function, &values),
            Target::Indirect(signature, callee) => {
                self.b.ins().call_indirect(signature, callee, &values)
            }
        };
        let results: Vec<Value> = self.b.inst_results(call).to_vec();
        Ok(match &abi.ret {
            RetPlan::Void => {
                if ret == Ty::Void {
                    Val::Void
                } else {
                    self.multi(&[])
                }
            }
            RetPlan::Scalar(_) => Val::S(results[0]),
            RetPlan::Leaves(_) => self.leaves_to_val(ret, &results),
            RetPlan::Sret => {
                let address = result_address.expect("an sret call has a result address");
                if self.is_large(ret)? {
                    Val::Ref(address)
                } else {
                    self.load_value(Place::Base(address, 0), ret)?
                }
            }
        })
    }
}
