//! Anonymous unions `(A | B)` (A24): the generic declarations behind them, the canonical order of
//! their members, and how they read in diagnostics.
//!
//! An anonymous union of n members is `Type::Union(id, members)`, where `id` is the synthesized
//! declaration `$anonymous.Union$n<$t0, ..>` whose case k holds `$t{k}` (D2), and the members are in
//! the canonical order of `order` (D3). Every other part of the compiler sees an instance of a generic
//! union, so ownership, layout, drop, specialization and code generation need nothing new.

use super::*;

/// The pseudo-module of the synthesized declarations. `$` cannot start or appear in a written name,
/// so no user name collides with them and no source can name them.
pub(super) const MODULE: &str = "$anonymous";

/// The most members that an anonymous union has after flattening (D9).
pub(super) const MAX_MEMBERS: usize = 32;

/// `Union$n`, the declaration of the anonymous unions of `members` members.
pub(super) fn declaration_name(members: usize) -> String {
    format!("Union${members}")
}

/// Whether a qualified declaration name belongs to `MODULE`.
pub(super) fn is_anonymous_name(qualified: &str) -> bool {
    qualified
        .strip_prefix(MODULE)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// The declarations `Union$2` to `Union$32` of `MODULE`. The checker adds them after the user unions
/// when some module writes an anonymous union or a type test; otherwise nothing changes.
pub(super) fn declarations() -> Vec<UnionDecl> {
    let generated = |text: String| Ident {
        text,
        span: Span::default(),
        provenance: Provenance::Generated,
    };
    (2..=MAX_MEMBERS)
        .map(|members| {
            let name = declaration_name(members);
            let cases = (0..members)
                .map(|case| UnionCaseDecl {
                    name: generated(format!("{name}${case}")),
                    payload: Some(TypeExpr {
                        kind: TypeExprKind::Variable(format!("$t{case}")),
                        span: Span::default(),
                    }),
                    json: None,
                })
                .collect();
            UnionDecl {
                doc: None,
                visibility: Visibility::Public,
                name: generated(name),
                parameters: (0..members)
                    .map(|parameter| generated(format!("$t{parameter}")))
                    .collect(),
                cases,
                derives: Vec::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_generic_unions_of_2_to_32_members() {
        let declarations = declarations();
        assert_eq!(declarations.len(), 31);
        for (index, union) in declarations.iter().enumerate() {
            let members = index + 2;
            assert_eq!(union.name.text, format!("Union${members}"));
            assert_eq!(union.name.provenance, Provenance::Generated);
            assert_eq!(union.visibility, Visibility::Public);
            assert!(union.derives.is_empty() && union.doc.is_none());
            let parameters: Vec<_> = union.parameters.iter().map(|p| p.text.as_str()).collect();
            let expected: Vec<_> = (0..members).map(|k| format!("$t{k}")).collect();
            assert_eq!(parameters, expected);
            assert_eq!(union.cases.len(), members);
            for (case, declaration) in union.cases.iter().enumerate() {
                assert_eq!(declaration.name.text, format!("Union${members}${case}"));
                assert!(matches!(
                    &declaration.payload,
                    Some(TypeExpr { kind: TypeExprKind::Variable(name), .. }) if *name == format!("$t{case}")
                ));
            }
        }
        // The cases of all declarations together (D9).
        let cases: usize = declarations.iter().map(|union| union.cases.len()).sum();
        assert_eq!(cases, 527);
    }

    #[test]
    fn names_belong_to_the_pseudo_module() {
        assert!(is_anonymous_name("$anonymous.Union$2"));
        assert!(!is_anonymous_name("$anonymousX.Union$2"));
        assert!(!is_anonymous_name("Main.Union"));
        // `$` is followed by a lowercase letter or a digit, never one of the escapes of
        // `mangled_type` (`$A`, `$L`, `$R`, `$C`, `$P`).
        for union in declarations() {
            let qualified = format!("{MODULE}.{}", union.name.text);
            for text in std::iter::once(qualified)
                .chain(union.parameters.iter().map(|p| p.text.clone()))
                .chain(union.cases.iter().map(|case| case.name.text.clone()))
            {
                for (at, _) in text.match_indices('$') {
                    let next = text[at + 1..].chars().next().unwrap();
                    assert!(next.is_ascii_lowercase() || next.is_ascii_digit(), "{text}");
                }
            }
        }
    }
}
