//! The layouts of the record and union types (G26 phase 6): for each user type of the graph's
//! modules that the final functions use (and each non-generic one), the native 64-bit bounds of its
//! size and alignment that code generation models (`layout_bound`), the slot of each field, and the
//! tag and niche of a union. They are bounds, not measurements.

use std::collections::{BTreeMap, BTreeSet};

use super::ids::type_id_text;
use super::model::{LayoutRow, LayoutShape};
use crate::check::kg_capture::Capture;
use crate::check::{CheckedModule, Type, TypedExpr};
use crate::llvm::layout::{self, Niche, UnionLayout};

/// Adds the record and union types in `ty` (its arguments and components included).
fn collect_types(ty: &Type, found: &mut BTreeSet<Type>) {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Record(_, args) | Type::Union(_, args) => {
                if found.insert(ty.clone()) {
                    pending.extend(args.iter());
                }
            }
            Type::Array(inner)
            | Type::ArrayView(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Task(inner)
            | Type::Reference(inner, _)
            | Type::Shared(inner, _) => pending.push(inner),
            Type::FixedArray(element, length) => {
                pending.push(element);
                pending.push(length);
            }
            Type::Tuple(elements) => pending.extend(elements.iter()),
            Type::Function(parameters, result) => {
                pending.extend(parameters.iter());
                pending.push(result);
            }
            Type::Application(head, arguments) => {
                pending.push(head);
                pending.extend(arguments.iter());
            }
            Type::Error
            | Type::Variable(_)
            | Type::Infer(_)
            | Type::Partial(_)
            | Type::Integer(..)
            | Type::Binary(_)
            | Type::Decimal(_)
            | Type::Simd(_)
            | Type::Bool
            | Type::Unit
            | Type::Char
            | Type::Utf8Char
            | Type::String
            | Type::Utf8String
            | Type::Length(_)
            | Type::Handle(_)
            | Type::Dyn(_) => {}
        }
    }
}

fn concrete(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Variable(_) | Type::Infer(_) | Type::Partial(_) | Type::Error => return false,
            Type::Record(_, args) | Type::Union(_, args) => pending.extend(args.iter()),
            Type::Array(inner)
            | Type::ArrayView(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Task(inner)
            | Type::Reference(inner, _)
            | Type::Shared(inner, _) => pending.push(inner),
            Type::FixedArray(element, length) => {
                pending.push(element);
                pending.push(length);
            }
            Type::Tuple(elements) => pending.extend(elements.iter()),
            Type::Function(parameters, result) => {
                pending.extend(parameters.iter());
                pending.push(result);
            }
            Type::Application(..) => return false,
            _ => {}
        }
    }
    true
}

/// The layout rows of the graph's modules, by module key and type ID.
pub(crate) fn build(module: &CheckedModule, capture: &Capture) -> BTreeMap<String, Vec<LayoutRow>> {
    let _target = layout::target_scope(false);
    let types = module.types();
    let keys: BTreeSet<&str> = capture
        .modules
        .iter()
        .map(|module| module.key.as_str())
        .collect();
    let declarations: BTreeSet<&str> = capture
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, "record" | "union"))
        .map(|node| node.id.as_str())
        .collect();
    let mut found = BTreeSet::new();
    for function in &module.functions {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
            .chain(function.parameters.iter().map(|local| &local.ty))
        {
            collect_types(ty, &mut found);
        }
        let mut stack: Vec<&TypedExpr> = vec![&function.body];
        while let Some(expression) = stack.pop() {
            collect_types(&expression.ty, &mut found);
            stack.extend(expression.children());
        }
    }
    for (id, record) in module.records.iter().enumerate() {
        if record.parameters.is_empty() {
            found.insert(Type::Record(id, Box::new([])));
        }
    }
    for (id, union) in module.unions.iter().enumerate() {
        if union.parameters.is_empty() {
            found.insert(Type::Union(id, Box::new([])));
        }
    }
    let mut rows: BTreeMap<String, BTreeMap<String, LayoutRow>> = BTreeMap::new();
    for ty in found {
        if !concrete(&ty) {
            continue;
        }
        let name = match &ty {
            Type::Record(id, _) => &module.records[*id].name,
            Type::Union(id, _) => &module.unions[*id].name,
            _ => continue,
        };
        let Some((key, declared)) = name.rsplit_once('.').filter(|(key, _)| keys.contains(key))
        else {
            continue;
        };
        // Only the declarations of the graph: a generated type has no node.
        let declaration = capture
            .module_ids
            .get(key)
            .map(|module| super::ids::item_id(module, declared));
        if !declaration.is_some_and(|declaration| declarations.contains(declaration.as_str())) {
            continue;
        }
        let key = key.to_owned();
        let lower = layout::layout_bound(&ty, module, true);
        let upper = layout::layout_bound(&ty, module, false);
        let (kind, shape) = match &ty {
            Type::Record(id, args) => {
                let slots = layout::record_slots(*id, args, module);
                let fields = module.records[*id]
                    .fields
                    .iter()
                    .zip(types.record_fields(*id, args))
                    .enumerate()
                    .map(|(index, ((name, _), field))| {
                        (
                            name.clone(),
                            field.display(&types),
                            slots.as_ref().map_or(index, |slots| slots[index]),
                        )
                    })
                    .collect();
                (
                    "record",
                    LayoutShape::Record {
                        fields,
                        reordered: slots.is_some(),
                    },
                )
            }
            Type::Union(id, args) => {
                let cases = module.unions[*id]
                    .cases
                    .iter()
                    .zip(types.union_payloads(*id, args))
                    .map(|((name, _), payload)| {
                        (name.clone(), payload.map(|payload| payload.display(&types)))
                    })
                    .collect();
                let (shape, niche) = if types.recursive(&ty) {
                    ("node", None)
                } else {
                    match layout::union_layout(*id, args, module) {
                        UnionLayout::Enum => ("enum", None),
                        UnionLayout::Common(_) => ("common", None),
                        UnionLayout::General { .. } => ("general", None),
                        UnionLayout::Niche { niche, .. } => (
                            "niche",
                            Some(match niche {
                                Niche::NegativeLength => "negative_length",
                                Niche::NullPointer => "null_pointer",
                                Niche::NullDescriptor => "null_descriptor",
                            }),
                        ),
                    }
                };
                (
                    "union",
                    LayoutShape::Union {
                        layout: shape,
                        tag_bytes: (shape != "niche").then(|| layout::tag_width(&ty, module)),
                        niche,
                        cases,
                    },
                )
            }
            _ => continue,
        };
        let id = type_id_text(&ty, &types, &capture.module_ids);
        rows.entry(key).or_default().insert(
            id.clone(),
            LayoutRow {
                id,
                kind,
                ty: ty.display(&types),
                size: (lower.0, upper.0),
                align: (lower.1, upper.1),
                shape,
                drop_flag: layout::drop_flag(&ty, module).is_some(),
            },
        );
    }
    rows.into_iter()
        .map(|(key, rows)| (key, rows.into_values().collect()))
        .collect()
}
