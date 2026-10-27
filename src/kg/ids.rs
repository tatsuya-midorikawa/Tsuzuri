//! The IDs of the nodes of the knowledge graph (G26 appendix B, D-60). An ID is the full name of a
//! declaration: the namespace and module joined by `::`, a `.` before the item. It does not depend
//! on positions; the second and later of equal IDs in source order get `#2`, `#3`, ...

use std::collections::{BTreeMap, BTreeSet};

use crate::check::{Type, TypeContext};

pub(crate) fn item_id(module: &str, name: &str) -> String {
    format!("{module}.{name}")
}

pub(crate) fn instance_id(class: &str, head: &str) -> String {
    format!("instance:{class}<{head}>")
}

pub(crate) fn test_id(module: &str, name: &str) -> String {
    format!("test:{module}.{name}")
}

pub(crate) fn bench_id(module: &str, name: &str) -> String {
    format!("bench:{module}.{name}")
}

/// The final IDs of `raw` IDs given with their source positions `(source, start)`, in the given
/// order: the first of equal IDs in source order keeps its ID, and the later ones get `#2`, `#3`,
/// skipping a suffix that another ID already spells.
pub(crate) fn disambiguate(raw: &[(String, (usize, usize))]) -> Vec<String> {
    let taken: BTreeSet<&str> = raw.iter().map(|(id, _)| id.as_str()).collect();
    let mut order: Vec<usize> = (0..raw.len()).collect();
    order.sort_by(|&a, &b| (raw[a].0.as_str(), raw[a].1, a).cmp(&(raw[b].0.as_str(), raw[b].1, b)));
    let mut result = vec![String::new(); raw.len()];
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for index in order {
        let id = raw[index].0.as_str();
        let count = counts.entry(id).or_insert(0);
        *count += 1;
        if *count == 1 {
            result[index] = id.to_owned();
            assigned.insert(id.to_owned());
            continue;
        }
        let mut suffix = *count;
        let unique = loop {
            let candidate = format!("{id}#{suffix}");
            if !taken.contains(candidate.as_str()) && !assigned.contains(&candidate) {
                break candidate;
            }
            suffix += 1;
        };
        *count = suffix;
        assigned.insert(unique.clone());
        result[index] = unique;
    }
    result
}

/// The ID text of a type (G26 appendix B2): records, unions and handles by the ID of their
/// declaration, so that equal names of different packages stay apart; the other types as
/// `Type::display` writes them, with function components parenthesized.
pub(crate) fn type_id_text(
    ty: &Type,
    types: &TypeContext<'_>,
    modules: &BTreeMap<String, String>,
) -> String {
    let declaration = |qualified: &str| match qualified.rsplit_once('.') {
        Some((key, name)) => {
            let module = modules
                .get(key)
                .cloned()
                .unwrap_or_else(|| key.replace('.', "::"));
            item_id(&module, name)
        }
        None => qualified.to_owned(),
    };
    let nested = |inner: &Type| {
        let text = type_id_text(inner, types, modules);
        if matches!(inner, Type::Function(..)) {
            format!("({text})")
        } else {
            text
        }
    };
    let arguments = |args: &[Type]| {
        if args.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                args.iter()
                    .map(|arg| type_id_text(arg, types, modules))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    };
    match ty {
        Type::Record(id, args) => declaration(&types.records[*id].name) + &arguments(args),
        Type::Union(id, args) => declaration(&types.unions[*id].name) + &arguments(args),
        Type::Handle(name) => declaration(name),
        Type::Array(element) => format!("[{}]", type_id_text(element, types, modules)),
        Type::ArrayView(element) => format!("[{}..]", type_id_text(element, types, modules)),
        Type::List(element) => format!("[|{}|]", type_id_text(element, types, modules)),
        Type::Vec(element) => format!("Vec<{}>", type_id_text(element, types, modules)),
        Type::FixedArray(element, length) => format!(
            "[{}; {}]",
            type_id_text(element, types, modules),
            length.display(types)
        ),
        Type::Tuple(elements) => format!(
            "({})",
            elements.iter().map(nested).collect::<Vec<_>>().join(" * ")
        ),
        Type::Task(result) => format!("Task<{}>", type_id_text(result, types, modules)),
        Type::Shared(value, kind) => {
            format!("{}<{}>", kind.name(), type_id_text(value, types, modules))
        }
        Type::Reference(inner, mutable) => {
            format!(
                "ref {}{}",
                if *mutable { "mut " } else { "" },
                nested(inner)
            )
        }
        Type::Function(parameters, result) => {
            let text = if parameters.is_empty() {
                format!("fn() -> {}", type_id_text(result, types, modules))
            } else {
                parameters
                    .iter()
                    .map(nested)
                    .chain([type_id_text(result, types, modules)])
                    .collect::<Vec<_>>()
                    .join(" -> ")
            };
            if parameters.pure {
                format!("pure ({text})")
            } else {
                text
            }
        }
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..)
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
        | Type::Dyn(_) => ty.display(types),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_ids_per_the_decision_table() {
        let module = "Acme::Kg::Shapes";
        assert_eq!(item_id(module, "area"), "Acme::Kg::Shapes.area");
        assert_eq!(
            item_id(&item_id(module, "Point"), "x"),
            "Acme::Kg::Shapes.Point.x"
        );
        assert_eq!(
            item_id("Acme::Kg::Traits.Sized", "size"),
            "Acme::Kg::Traits.Sized.size"
        );
        let instance = instance_id("Acme::Kg::Traits.Sized", "Acme::Kg::Shapes.Point");
        assert_eq!(
            instance,
            "instance:Acme::Kg::Traits.Sized<Acme::Kg::Shapes.Point>"
        );
        assert_eq!(
            item_id(&instance, "size"),
            "instance:Acme::Kg::Traits.Sized<Acme::Kg::Shapes.Point>.size"
        );
        assert_eq!(
            test_id("Acme::Kg::Main", "total adds"),
            "test:Acme::Kg::Main.total adds"
        );
        assert_eq!(bench_id("Main", "b"), "bench:Main.b");
        assert_eq!(item_id("Acme::Kg::Main", "$entry"), "Acme::Kg::Main.$entry");
        assert_eq!(item_id("Main", "(|Even|Odd|)"), "Main.(|Even|Odd|)");
    }

    #[test]
    fn suffixes_duplicate_ids_in_source_order() {
        let raw = vec![
            ("test:Main.same".to_owned(), (0, 50)),
            ("test:Main.same".to_owned(), (0, 10)),
            ("bench:Main.same".to_owned(), (0, 70)),
            ("test:Main.same#2".to_owned(), (1, 0)),
            ("test:Main.same".to_owned(), (0, 90)),
        ];
        assert_eq!(
            disambiguate(&raw),
            [
                "test:Main.same#3",
                "test:Main.same",
                "bench:Main.same",
                "test:Main.same#2",
                "test:Main.same#4"
            ]
        );
    }

    #[test]
    fn type_id_text_covers_every_type_variant() {
        use crate::check::{FunctionParameters, SharedKind};
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let modules = BTreeMap::new();
        let text = |ty: &Type| type_id_text(ty, &types, &modules);
        let i64 = Type::Integer(64, true);
        let function = |parameters: Vec<Type>, result: Type| {
            Type::Function(
                Box::new(FunctionParameters {
                    types: parameters,
                    conventions: Box::new([]),
                    pure: false,
                }),
                Box::new(result),
            )
        };
        let unary = function(vec![i64.clone()], Type::Bool);
        assert_eq!(text(&Type::Variable("a".into())), "'a");
        assert_eq!(text(&Type::Variable("#N".into())), "N");
        assert_eq!(text(&Type::Array(Box::new(i64.clone()))), "[i64]");
        assert_eq!(text(&Type::ArrayView(Box::new(i64.clone()))), "[i64..]");
        assert_eq!(text(&Type::List(Box::new(i64.clone()))), "[|i64|]");
        assert_eq!(text(&Type::Vec(Box::new(i64.clone()))), "Vec<i64>");
        assert_eq!(
            text(&Type::FixedArray(
                Box::new(i64.clone()),
                Box::new(Type::Length(4))
            )),
            "[i64; 4]"
        );
        assert_eq!(
            text(&Type::Tuple(vec![i64.clone(), unary.clone()])),
            "(i64 * (i64 -> bool))"
        );
        assert_eq!(text(&Type::Task(Box::new(Type::Unit))), "Task<unit>");
        assert_eq!(
            text(&Type::Shared(Box::new(Type::String), SharedKind::RcWeak)),
            "Rc.Weak<string>"
        );
        assert_eq!(
            text(&Type::Reference(Box::new(unary.clone()), true)),
            "ref mut (i64 -> bool)"
        );
        assert_eq!(
            text(&function(vec![unary.clone()], Type::Unit)),
            "(i64 -> bool) -> unit"
        );
        assert_eq!(text(&function(Vec::new(), Type::Unit)), "fn() -> unit");
        assert_eq!(text(&Type::Handle("Shapes.Handle".into())), "Shapes.Handle");
        for scalar in [
            i64,
            Type::Binary(64),
            Type::Decimal(64),
            Type::Bool,
            Type::Unit,
            Type::Char,
            Type::Utf8Char,
            Type::String,
            Type::Utf8String,
            Type::Length(3),
        ] {
            assert_eq!(text(&scalar), scalar.display(&types));
        }
    }
}
