//! How a function's parameters and result travel: a function of its LLVM type alone, so that a
//! definition, a direct call and an indirect call agree without sharing anything.
//!
//! A function of the module's own (internal linkage) passes a small aggregate as one parameter per
//! scalar and a large one by the address of the caller's copy, and returns at most two words in
//! registers; the rest returns through a pointer the caller passes first. A function that other
//! object files can see or call has scalar parameters and result only, which is the C convention.

use cranelift_codegen::ir::{self, AbiParam, ArgumentExtension, Signature, types};
use cranelift_codegen::isa::CallConv;

use super::error::{Res, bail};
use super::module::Ext;
use super::target::Target;
use super::types::{Ty, Types};

#[derive(Clone, Debug)]
pub(crate) enum ParamPlan {
    Scalar,
    /// The scalars of an aggregate or a vector: one parameter each.
    Leaves(Vec<ir::Type>),
    /// A large aggregate: the address of the caller's copy.
    Indirect,
}

#[derive(Clone, Debug)]
pub(crate) enum RetPlan {
    Void,
    Scalar(ir::Type),
    /// At most two words, returned in registers.
    Leaves(Vec<ir::Type>),
    /// The caller passes the address of the result as the first parameter.
    Sret,
}

#[derive(Clone, Debug)]
pub(crate) struct Abi {
    pub params: Vec<ParamPlan>,
    pub ret: RetPlan,
    pub sig: Signature,
    /// Whether other object files can see or call the function: it follows the C convention.
    pub external: bool,
}

/// The Cranelift type of a scalar; an integer is kept in the smallest type that holds it.
pub(crate) fn scalar_type(types: &Types<'_>, target: &Target, ty: Ty) -> Res<ir::Type> {
    Ok(match ty {
        Ty::Int(bits) => match bits {
            1..=8 => types::I8,
            9..=16 => types::I16,
            17..=32 => types::I32,
            33..=64 => types::I64,
            _ => types::I128,
        },
        Ty::Float => types::F32,
        Ty::Double => types::F64,
        Ty::Ptr => target.pointer,
        Ty::Half => bail!("the type half"),
        other => bail!("a value of type {}", types.describe(other)),
    })
}

fn extension(ext: Ext) -> ArgumentExtension {
    match ext {
        Ext::None => ArgumentExtension::None,
        Ext::Sext => ArgumentExtension::Sext,
        Ext::Zext => ArgumentExtension::Uext,
    }
}

fn is_scalar(ty: Ty) -> bool {
    matches!(ty, Ty::Int(_) | Ty::Float | Ty::Double | Ty::Ptr | Ty::Half)
}

/// The Cranelift types of the scalars `ty` flattens to.
pub(crate) fn leaf_types(types: &Types<'_>, target: &Target, ty: Ty) -> Res<Vec<ir::Type>> {
    let mut leaves = Vec::new();
    types.leaf_layout(ty, 0, &mut leaves)?;
    leaves
        .into_iter()
        .map(|leaf| scalar_type(types, target, leaf.ty))
        .collect()
}

fn words(ty: ir::Type) -> usize {
    if ty == types::I128 { 2 } else { 1 }
}

/// Plans a signature. `external` is true for what other object files can see or call.
pub(crate) fn plan(
    types: &Types<'_>,
    target: &Target,
    ret: Ty,
    ret_ext: Ext,
    params: &[(Ty, Ext)],
    external: bool,
) -> Res<Abi> {
    plan_in(types, target, ret, ret_ext, params, external, false)
}

/// The plan of a function that a `musttail` call enters or leaves (PM09): the tail convention of
/// Cranelift, which lets the callee take the stack arguments of its caller's frame. Every parameter
/// travels by value, and so does the result, which is never a pointer to the caller's frame:
/// a large aggregate parameter or a result in memory has no such plan, and an error says so.
pub(crate) fn plan_tail(
    types: &Types<'_>,
    target: &Target,
    ret: Ty,
    ret_ext: Ext,
    params: &[(Ty, Ext)],
) -> Res<Abi> {
    plan_in(types, target, ret, ret_ext, params, false, true)
}

fn plan_in(
    types: &Types<'_>,
    target: &Target,
    ret: Ty,
    ret_ext: Ext,
    params: &[(Ty, Ext)],
    external: bool,
    tail: bool,
) -> Res<Abi> {
    let mut sig = Signature::new(if tail {
        CallConv::Tail
    } else {
        target.call_conv
    });
    let mut plans = Vec::with_capacity(params.len());
    let mut return_plan = RetPlan::Void;
    if ret != Ty::Void {
        if is_scalar(ret) {
            let ty = scalar_type(types, target, ret)?;
            if external && ty == types::I128 {
                bail!("a 128-bit integer in the signature of an external function");
            }
            return_plan = RetPlan::Scalar(ty);
        } else if external {
            bail!(
                "the result type {} of an external function",
                types.describe(ret)
            );
        } else {
            let leaves = types.leaves(ret)?;
            if leaves == 0 {
                return_plan = RetPlan::Void;
            } else {
                let kinds = leaf_types(types, target, ret)?;
                let total: usize = kinds.iter().map(|&ty| words(ty)).sum();
                return_plan = if !types.is_large(ret)? && total <= target.max_return_words {
                    RetPlan::Leaves(kinds)
                } else if tail {
                    bail!("a result in memory in the signature of a tail call");
                } else {
                    RetPlan::Sret
                };
            }
        }
    }
    match &return_plan {
        RetPlan::Void => {}
        RetPlan::Scalar(ty) => {
            let mut param = AbiParam::new(*ty);
            if external && ty.is_int() && ty.bits() < 32 {
                param.extension = extension(ret_ext);
            }
            sig.returns.push(param);
        }
        RetPlan::Leaves(kinds) => {
            for &ty in kinds {
                sig.returns.push(AbiParam::new(ty));
            }
        }
        RetPlan::Sret => sig.params.push(AbiParam::new(target.pointer)),
    }
    for &(ty, ext) in params {
        if is_scalar(ty) {
            let kind = scalar_type(types, target, ty)?;
            if external && kind == types::I128 {
                bail!("a 128-bit integer in the signature of an external function");
            }
            let mut param = AbiParam::new(kind);
            if kind.is_int() && kind.bits() < 32 {
                param.extension = extension(ext);
            }
            sig.params.push(param);
            plans.push(ParamPlan::Scalar);
        } else if external {
            bail!(
                "the parameter type {} of an external function",
                types.describe(ty)
            );
        } else {
            if types.is_large(ty)? {
                if tail {
                    bail!(
                        "the large aggregate parameter {} in the signature of a tail call",
                        types.describe(ty)
                    );
                }
                sig.params.push(AbiParam::new(target.pointer));
                plans.push(ParamPlan::Indirect);
            } else {
                let kinds = leaf_types(types, target, ty)?;
                for &kind in &kinds {
                    sig.params.push(AbiParam::new(kind));
                }
                plans.push(ParamPlan::Leaves(kinds));
            }
        }
    }
    Ok(Abi {
        params: plans,
        ret: return_plan,
        sig,
        external,
    })
}
