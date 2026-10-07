//! The implicit copies of Copy values that own heap storage (A15), from the
//! consuming reads that the ownership checker sees after specialization.

use std::collections::BTreeSet;

use crate::check::{CheckedModule, ModuleOrigin, Type};
use crate::diagnostic::{Diagnostic, Span};

/// One implicit copy in a specialized function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopySite {
    /// The index in `CheckedModule::functions`.
    pub function: usize,
    /// The copied expression; a pattern binding's name for a copy into a binding.
    pub span: Span,
    /// The concrete copied type.
    pub ty: Type,
    pub kind: CopyKind,
    pub cost: CopyCost,
}

/// What the copied expression reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyKind {
    Local,
    Field,
    Element,
    Tail,
    Payload,
    Dereference,
    /// An element or part of a temporary value.
    Temporary,
}

/// What a copy costs at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyCost {
    /// The value holds arrays or lists, which are copied element by element.
    Length,
    /// Only function environments are copied.
    Environment,
}

/// A consuming read of a Copy value that needs a drop, as the ownership checker sees it.
pub(crate) struct CopyRead {
    pub span: Span,
    pub ty: Type,
    pub kind: CopyKind,
    /// The local read as a whole and not through a match or loop alias. Its only use moves.
    pub local: Option<usize>,
}

/// Every implicit copy of every function, once per position and function.
pub fn sites(module: &CheckedModule) -> Vec<CopySite> {
    let mut sites = Vec::new();
    for (index, reads) in crate::ownership::copy_reads(module).into_iter().enumerate() {
        let function = &module.functions[index];
        let single = crate::llvm::call_specialization::single_use_locals(&function.body);
        // The body of an owned function borrows its captures, so even their only use copies (B07).
        let lent: BTreeSet<_> = if function.owned_captures {
            function.parameters[..function.capture_count]
                .iter()
                .map(|parameter| parameter.id)
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut seen = BTreeSet::new();
        for read in reads {
            if read
                .local
                .is_some_and(|id| single.contains(&id) && !lent.contains(&id))
            {
                continue;
            }
            if !seen.insert((read.span.source, read.span.start, read.span.end)) {
                continue;
            }
            sites.push(CopySite {
                function: index,
                cost: cost(module, &read.ty),
                span: read.span,
                ty: read.ty,
                kind: read.kind,
            });
        }
    }
    sites
}

/// The positions in user modules that copy arrays or lists, once per position
/// however many specializations copy there, in source order. `W1006` and the
/// editor's inlay hints report these.
pub fn costly_sites(module: &CheckedModule) -> Vec<CopySite> {
    let mut seen = BTreeSet::new();
    let mut sites: Vec<_> = sites(module)
        .into_iter()
        .filter(|site| {
            site.cost == CopyCost::Length
                && module.functions[site.function].origin.module == ModuleOrigin::User
                && seen.insert((site.span.source, site.span.start, site.span.end))
        })
        .collect();
    sites.sort_by_key(|site| (site.span.source, site.span.start));
    sites
}

/// `W1006` at each of the `costly_sites`.
pub fn warnings(module: &CheckedModule) -> Vec<Diagnostic> {
    costly_sites(module)
        .into_iter()
        .map(|site| Diagnostic::warning("W1006", message(&site.ty), site.span))
        .collect()
}

fn cost(module: &CheckedModule, ty: &Type) -> CopyCost {
    let types = module.types();
    // Copy types are never recursive, so the walk ends.
    let mut pending = vec![ty.clone()];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Array(_) | Type::List(_) => return CopyCost::Length,
            // A Copy dyn value clones its stored value into a new allocation (A14 Phase 2).
            Type::Dyn(_) => return CopyCost::Length,
            Type::Tuple(elements) => pending.extend(elements),
            // A fixed-length array copies inline, like a tuple of its elements (A16).
            Type::FixedArray(element, _) => pending.push(*element),
            Type::Record(id, arguments) => pending.extend(types.record_fields(id, &arguments)),
            Type::Union(id, arguments) => {
                pending.extend(types.union_payloads(id, &arguments).into_iter().flatten())
            }
            _ => {}
        }
    }
    CopyCost::Environment
}

pub(crate) fn message(ty: &Type) -> &'static str {
    match ty {
        Type::Array(_) => {
            "implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit"
        }
        Type::List(_) => {
            "implicit copy of a list allocates a new node for every element; borrow it with 'ref', or call 'List.copy' to make the copy explicit"
        }
        Type::Dyn(_) => {
            "implicit copy of a dyn value allocates and clones the stored value; borrow it with 'ref', or use the value only once so that it moves"
        }
        _ => {
            "implicit copy of a value that contains arrays or lists copies all of them; borrow it with 'ref', or use the value only once so that it moves"
        }
    }
}
