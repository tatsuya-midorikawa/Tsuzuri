//! The bytes of a global's initializer and the addresses inside it that the linker fills in.

use super::error::{Failure, Res, bail};
use super::func::{Const, Cx};
use super::lex::Lexer;
use super::types::{Compound, Ty};

pub(crate) struct DataReloc {
    pub offset: u32,
    pub sym: u32,
    pub addend: i64,
}

pub(crate) enum Init {
    /// `size` bytes of zero, which an object file stores as a size alone.
    Zero(u64),
    Bytes {
        bytes: Vec<u8>,
        relocs: Vec<DataReloc>,
    },
}

/// Reads the initializer of type `ty` that starts at `start`.
pub(crate) fn evaluate<'a>(cx: &mut Cx<'a>, ty: Ty, start: usize) -> Res<Init> {
    let mut lexer = Lexer::new(cx.module.text, start);
    let constant = cx.parse_const(&mut lexer, ty)?;
    let size = cx.types.size_of(ty)?;
    if matches!(constant, Const::Zero) {
        return Ok(Init::Zero(size));
    }
    let mut bytes = vec![0u8; size as usize];
    let mut relocs = Vec::new();
    write(cx, &constant, ty, 0, &mut bytes, &mut relocs)?;
    Ok(Init::Bytes { bytes, relocs })
}

fn put(bytes: &mut [u8], at: u64, data: &[u8]) -> Res<()> {
    let at = at as usize;
    match bytes.get_mut(at..at + data.len()) {
        Some(slot) => {
            slot.copy_from_slice(data);
            Ok(())
        }
        None => bail!("an initializer longer than its type"),
    }
}

fn write<'a>(
    cx: &mut Cx<'a>,
    constant: &Const,
    ty: Ty,
    at: u64,
    bytes: &mut [u8],
    relocs: &mut Vec<DataReloc>,
) -> Res<()> {
    let offset = u32::try_from(at).map_err(|_| Failure::new("a global this large"))?;
    match (constant, ty) {
        (Const::Zero, _) => Ok(()),
        (Const::Int(value), Ty::Int(bits)) => {
            let size = super::types::int_store_size(bits) as usize;
            let value = if bits >= 128 {
                *value as u128
            } else {
                (*value as u128) & ((1u128 << bits) - 1)
            };
            put(bytes, at, &value.to_le_bytes()[..size])
        }
        (Const::Float(bits), Ty::Double) => put(bytes, at, &bits.to_le_bytes()),
        (Const::Float(bits), Ty::Float) => put(
            bytes,
            at,
            &(f64::from_bits(*bits) as f32).to_bits().to_le_bytes(),
        ),
        (
            Const::Addr {
                sym: None,
                offset: address,
            },
            Ty::Ptr | Ty::Int(64),
        ) => put(bytes, at, &(*address as u64).to_le_bytes()),
        (Const::Int(value), Ty::Ptr) => put(bytes, at, &(*value as u64).to_le_bytes()),
        (
            Const::Addr {
                sym: Some(sym),
                offset: addend,
            },
            Ty::Ptr | Ty::Int(64),
        ) => {
            relocs.push(DataReloc {
                offset,
                sym: *sym,
                addend: *addend,
            });
            Ok(())
        }
        (Const::Bytes(data), Ty::Compound(_)) => put(bytes, at, data),
        (Const::Agg(items), Ty::Compound(_)) => {
            let shape = cx
                .types
                .shape(ty)
                .cloned()
                .ok_or_else(|| Failure::new("an aggregate constant of a scalar type"))?;
            match shape {
                Compound::Struct { fields, .. } => {
                    if fields.len() != items.len() {
                        bail!(
                            "a struct initializer with {} fields for {}",
                            items.len(),
                            fields.len()
                        );
                    }
                    for (index, (item, &field)) in items.iter().zip(fields.iter()).enumerate() {
                        let field_at = at
                            + cx.types
                                .field_offset(ty, index)
                                .ok_or_else(|| Failure::new("a struct field out of range"))?;
                        write(cx, item, field, field_at, bytes, relocs)?;
                    }
                    Ok(())
                }
                Compound::Array { elem, .. } | Compound::Vector { elem, .. } => {
                    let stride = cx.types.size_of(elem)?;
                    for (index, item) in items.iter().enumerate() {
                        write(cx, item, elem, at + index as u64 * stride, bytes, relocs)?;
                    }
                    Ok(())
                }
            }
        }
        (other, _) => bail!(
            "the initializer {other:?} of type {}",
            cx.types.describe(ty)
        ),
    }
}
