//! How a function's parameters and result travel in WebAssembly: a function of its LLVM type
//! alone, so that a definition, a direct call and an indirect call agree.
//!
//! A scalar is one WebAssembly value, a 128-bit integer two (low word first), as in the C ABI of
//! `wasm32`. A function of the module's own passes a small aggregate as one value per scalar and a
//! large one by the address of the caller's copy, and returns anything of more than one value
//! through a pointer the caller passes first. A function that other object files can see or call
//! has scalar parameters and results only; a 128-bit result of one is the pair of values that the
//! compiler runtime of Clang uses when its target has multi-value results, and a pointer first
//! when it has not.

use super::super::error::{Res, bail};
use super::super::module::Ext;
use super::super::types::{Compound, Ty, Types};
use super::bin::Vt;

#[derive(Clone, Debug)]
pub(crate) enum ParamPlan {
    /// The values of the scalars of the parameter, in order.
    Leaves(Vec<Vt>),
    /// A large aggregate: the address of the caller's copy.
    Indirect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetPlan {
    Void,
    /// The results, one value each.
    Leaves(Vec<Vt>),
    /// The caller passes the address of the result as the first parameter.
    Sret,
}

#[derive(Clone, Debug)]
pub(crate) struct Abi {
    pub params: Vec<ParamPlan>,
    pub ret: RetPlan,
    pub ext: Vec<Ext>,
    pub ret_ext: Ext,
    /// Whether other object files can see or call the function: it follows the C convention.
    pub external: bool,
    pub wasm_params: Vec<Vt>,
    pub wasm_results: Vec<Vt>,
}

fn scalar(types: &Types<'_>, ty: Ty, out: &mut Vec<Vt>) -> Res<()> {
    match ty {
        Ty::Int(bits) => match bits {
            1..=32 => out.push(Vt::I32),
            33..=64 => out.push(Vt::I64),
            128 => out.extend([Vt::I64, Vt::I64]),
            _ => bail!("an integer of {bits} bits"),
        },
        Ty::Float => out.push(Vt::F32),
        Ty::Double => out.push(Vt::F64),
        Ty::Ptr => out.push(Vt::I32),
        Ty::Half => bail!("the type half"),
        other => bail!("a value of type {}", types.describe(other)),
    }
    Ok(())
}

/// The WebAssembly values that a value of `ty` is, in the order of its scalars in memory.
pub(crate) fn flat(types: &Types<'_>, ty: Ty) -> Res<Vec<Vt>> {
    let mut out = Vec::new();
    flatten(types, ty, &mut out)?;
    Ok(out)
}

fn flatten(types: &Types<'_>, ty: Ty, out: &mut Vec<Vt>) -> Res<()> {
    match ty {
        Ty::Void => Ok(()),
        Ty::Compound(_) => match types.shape(ty).cloned() {
            Some(Compound::Struct { fields, .. }) => {
                for &field in fields.iter() {
                    flatten(types, field, out)?;
                }
                Ok(())
            }
            Some(Compound::Array { len, elem }) => {
                let mut one = Vec::new();
                flatten(types, elem, &mut one)?;
                let total = (one.len() as u64).saturating_mul(len);
                if total > 1 << 20 {
                    bail!("an array of {len} elements as a value");
                }
                for _ in 0..len {
                    out.extend_from_slice(&one);
                }
                Ok(())
            }
            Some(Compound::Vector { len, elem }) => {
                for _ in 0..len {
                    scalar(types, elem, out)?;
                }
                Ok(())
            }
            None => bail!("a compound type without a shape"),
        },
        other => scalar(types, other, out),
    }
}

fn is_scalar(ty: Ty) -> bool {
    matches!(ty, Ty::Int(_) | Ty::Float | Ty::Double | Ty::Ptr | Ty::Half)
}

/// Plans a signature. `external` is true for what other object files can see or call;
/// `multivalue` says that the C ABI of the target returns a 128-bit integer as two results.
pub(crate) fn plan(
    types: &Types<'_>,
    ret: Ty,
    ret_ext: Ext,
    params: &[(Ty, Ext)],
    external: bool,
    multivalue: bool,
) -> Res<Abi> {
    let mut wasm_params = Vec::new();
    let mut wasm_results = Vec::new();
    let ret_plan = if ret == Ty::Void {
        RetPlan::Void
    } else if is_scalar(ret) {
        let values = flat(types, ret)?;
        if values.len() == 1 || (external && multivalue) {
            wasm_results.extend_from_slice(&values);
            RetPlan::Leaves(values)
        } else {
            RetPlan::Sret
        }
    } else if external {
        bail!(
            "the result type {} of an external function",
            types.describe(ret)
        );
    } else if types.leaves(ret)? == 0 {
        RetPlan::Void
    } else if types.is_large(ret)? {
        RetPlan::Sret
    } else {
        let values = flat(types, ret)?;
        if values.len() == 1 {
            wasm_results.extend_from_slice(&values);
            RetPlan::Leaves(values)
        } else {
            RetPlan::Sret
        }
    };
    if ret_plan == RetPlan::Sret {
        wasm_params.push(Vt::I32);
    }
    let mut plans = Vec::with_capacity(params.len());
    for &(ty, _) in params {
        if is_scalar(ty) {
            let values = flat(types, ty)?;
            wasm_params.extend_from_slice(&values);
            plans.push(ParamPlan::Leaves(values));
        } else if external {
            bail!(
                "the parameter type {} of an external function",
                types.describe(ty)
            );
        } else if types.is_large(ty)? {
            wasm_params.push(Vt::I32);
            plans.push(ParamPlan::Indirect);
        } else {
            let values = flat(types, ty)?;
            wasm_params.extend_from_slice(&values);
            plans.push(ParamPlan::Leaves(values));
        }
    }
    Ok(Abi {
        params: plans,
        ret: ret_plan,
        ext: params.iter().map(|&(_, ext)| ext).collect(),
        ret_ext,
        external,
        wasm_params,
        wasm_results,
    })
}
