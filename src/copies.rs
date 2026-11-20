//! The implicit copies of Copy values that own heap storage (A15), from the
//! consuming reads that the ownership checker sees after specialization.

use std::collections::BTreeSet;

use crate::check::{CheckedFunction, CheckedModule, ModuleOrigin, Type};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};

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
    /// An unaliased whole local, or a Copy root reached only through fields.
    /// The single-use proof accepts both; backward liveness accepts only `CopyKind::Local`.
    pub local: Option<usize>,
}

/// Every implicit copy of every function, once per position and function.
pub fn sites(module: &CheckedModule) -> Vec<CopySite> {
    sites_of(module, crate::ownership::copy_reads(module), |_| true)
}

/// The implicit copies of the functions that `keep` accepts, from `reads`, their consuming reads
/// by function.
fn sites_of(
    module: &CheckedModule,
    reads: Vec<Vec<CopyRead>>,
    keep: impl Fn(&CheckedFunction) -> bool,
) -> Vec<CopySite> {
    let mut sites = Vec::new();
    for (index, reads) in reads.into_iter().enumerate() {
        let function = &module.functions[index];
        if !keep(function) {
            continue;
        }
        let single = crate::llvm::call_specialization::single_use_locals(&function.body);
        let last_uses = crate::llvm::last_use::LastUses::analyze(function, module);
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
            if read.local.is_some_and(|id| {
                (single.contains(&id)
                    || (read.kind == CopyKind::Local && last_uses.moves_read_at(read.span, id)))
                    && !lent.contains(&id)
            }) {
                continue;
            }
            if read.local.is_none()
                && matches!(
                    read.kind,
                    CopyKind::Local | CopyKind::Payload | CopyKind::Field
                )
                && last_uses.moves_binding(read.span)
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
    costly_sites_with(module, None)
}

fn is_user(function: &CheckedFunction) -> bool {
    function.origin.module == ModuleOrigin::User
}

/// `costly_sites` from the call facts of the module's ownership check when they are given; it
/// analyzes only the functions of user modules, the only ones it reports (PB08 O4).
fn costly_sites_with(
    module: &CheckedModule,
    facts: Option<&crate::ownership::CopyFacts>,
) -> Vec<CopySite> {
    let reads = crate::ownership::copy_reads_of_users(module, facts);
    let mut seen = BTreeSet::new();
    let mut sites: Vec<_> = sites_of(module, reads, is_user)
        .into_iter()
        .filter(|site| {
            site.cost == CopyCost::Length
                && seen.insert((site.span.source, site.span.start, site.span.end))
        })
        .collect();
    sites.sort_by_key(|site| (site.span.source, site.span.start));
    sites
}

/// The language server's checked module (PB08 O4(b)), with the call facts of its ownership check
/// when nothing rewrote the bodies after it (`gpu_devices::lower_kernels`). The module is read
/// only but for its warnings, which the facts do not depend on, so the facts always describe its
/// functions; the public `costly_sites` takes any module, which a caller may have changed, and
/// computes them again.
pub(crate) struct EditorModule {
    module: CheckedModule,
    facts: Option<crate::ownership::CopyFacts>,
}

impl EditorModule {
    pub(crate) fn new(module: CheckedModule, facts: Option<crate::ownership::CopyFacts>) -> Self {
        Self { module, facts }
    }

    pub(crate) fn module(&self) -> &CheckedModule {
        &self.module
    }

    pub(crate) fn take_warnings(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.module.warnings)
    }

    /// `costly_sites` of the module, from the facts of its check when it kept them.
    pub(crate) fn costly_sites(&self) -> Vec<CopySite> {
        costly_sites_with(&self.module, self.facts.as_ref())
    }
}

/// `W1006` at each of the `costly_sites`.
pub fn warnings(module: &CheckedModule) -> Vec<Diagnostic> {
    costly_sites(module)
        .into_iter()
        .map(|site| {
            Diagnostic::of(
                DiagnosticKind::ImplicitCopyOfLargeValue,
                message(&site.ty),
                site.span,
            )
        })
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
            "implicit copy of a list allocates and copies every element; borrow it with 'ref', or call 'List.copy' to make the copy explicit"
        }
        Type::Dyn(_) => {
            "implicit copy of a dyn value allocates and clones the stored value; borrow it with 'ref', or use the value only once so that it moves"
        }
        _ => {
            "implicit copy of a value that contains arrays or lists copies all of them; borrow it with 'ref', or use the value only once so that it moves"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::Edition;

    /// The inputs of tests/copy_cost.rs, all 19 of them.
    const SOURCES: [&str; 19] = [
        // tests/copy_cost.rs `REUSED`.
        "let a = [1, 2, 3]\nlet b = a\nArray.length (ref a) + Array.length (ref b)",
        // tests/copy_cost.rs `MOVED`.
        "let a = [1, 2, 3]\nlet b = a\nArray.length (ref b)",
        // tests/copy_cost.rs `MOVED_TWICE`.
        "let a = [1, 2, 3]\nlet b = a\nlet c = b\nArray.length (ref c)",
        // tests/copy_cost.rs `SCALAR`.
        "let n = 5\nlet m = n\nn + m",
        // tests/copy_cost.rs `SCALAR_RECORD`.
        "record Point { value: i64 }\nlet p = Point { value: 1 }\nlet q = p\np.value + q.value",
        // tests/copy_cost.rs `FIELD`.
        "record Bag { items: [i64] }\nlet bag = Bag { items: [1, 2] }\nlet xs = bag.items\nlet ys = bag.items\nArray.length (ref xs) + Array.length (ref ys)",
        // tests/copy_cost.rs `LIST_REUSED`.
        "let a = [|1, 2, 3|]\nlet b = a\nList.length (ref a) + List.length (ref b)",
        // tests/copy_cost.rs `ARGUMENT_KEPT`.
        "def total :: [i64] -> i64\nfn total values = Array.length (ref values)\nlet a = [1, 2]\ntotal a + Array.length (ref a)",
        // tests/copy_cost.rs `ARGUMENT_MOVED`.
        "def total :: [i64] -> i64\nfn total values = Array.length (ref values)\nlet a = [1, 2]\ntotal a",
        // tests/copy_cost.rs `AGGREGATE_COPY`.
        "record Bag { items: [i64] }\nlet bag = Bag { items: [1] }\nlet other = bag\nArray.length (ref bag.items) + Array.length (ref other.items)",
        // tests/copy_cost.rs `DEREFERENCE`.
        "def copy_array :: ref [i64] -> [i64]\nfn copy_array values = *values\nlet a = [1, 2, 3]\nlet b = copy_array (ref a)\nArray.length (ref a) + Array.length (ref b)",
        // tests/copy_cost.rs `CLOSURE`.
        "let offset = 1\nlet add = x -> x + offset\nlet f = add\nf 1 + add 2",
        // tests/copy_cost.rs `SPECIALIZED`.
        "def dup :: Copy<'a> => 'a -> ('a * 'a)\nfn dup value = (value, value)\nlet a = dup [1, 2]\nlet b = dup [1.5]\n0",
        // tests/copy_cost.rs `EXPLICIT`.
        "let a = [1, 2, 3]\nlet b = Array.copy (ref a)\nArray.length (ref a) + Array.length (ref b)",
        // tests/copy_cost.rs `EXPLICIT_LIST`.
        "let a = [|1, 2, 3|]\nlet b = List.copy (ref a)\nList.length (ref a) + List.length (ref b)",
        // tests/copy_cost.rs `BINDING`.
        "let pair = ([1, 2], 3)\nlet count = match pair with\n    | (items, n) -> Array.length (ref items) + n\nlet r = ref pair\ncount",
        // tests/copy_cost.rs `ELEMENT`.
        "let rows = [[1, 2], [3]]\nlet mut total = 0\nfor row in rows do\n    let kept = row\n    total = total + Array.length (ref kept)\ntotal",
        // tests/copy_cost.rs `DERIVED`.
        "record Bag { items: [i64], tags: [|i64|] } deriving (Eq, Ord, Default, Hash, Display)\nlet a = Bag { items: [1, 2], tags: [|3|] }\nlet b: Bag = Default.default()\nlet same = if a == b then 1 else 0\nlet order = if a < b then 1 else 0\nlet shown = Display.display a\nsame + order + shown.length + (Hash.hash (ref a) as i64)",
        // tests/copy_cost.rs `OWNED`.
        "let items = [1, 2, 3]\nlet count = Owned.function (\\extra -> { let kept = items; Array.length (ref kept) + extra })\nOwned.call (ref count) 1",
    ];

    /// tests/gpu.rs `DEVICE_PROGRAM`, whose calls of `Gpu.map` and `Gpu.init` `lower_kernels`
    /// numbers, with an implicit copy of an array.
    const DEVICE: &str = "def mix :: i32 -> i32\nfn mix value = value * 3 + 1\ndef poly :: f32 -> f32\nfn poly value = value * value + value\nlet device = Result.get (Gpu.request Gpu.WebGpu)\nlet a = Gpu.to_array (Gpu.init (&device) 8 mix)\nlet b = Gpu.to_array (Gpu.map (&device) mix (Gpu.init (&device) 8 mix))\nlet floats: [f32] = [1.0, 2.0]\nlet c = Gpu.to_array (Gpu.map_relaxed (&device) poly (Gpu.from_array (&device) (&floats)))\nlet d = Gpu.to_array (Gpu.map (&device) (\\item -> item + 1.0) (Gpu.from_array (&device) (&floats)))\nlet kept = a\nArray.sum (&a) + Array.sum (&b) + Array.sum (&kept)";

    /// The language server's analysis of `source` with the std modules that it names.
    fn editor(source: &str) -> EditorModule {
        let std_sources = crate::stdlib::sources_for([source]);
        let input = |path, text, origin| crate::SourceInput {
            path,
            text,
            origin,
            namespace: "",
            edition: Edition::E2026,
            package: None,
        };
        let inputs: Vec<_> = std::iter::once(input("Main", source, ModuleOrigin::User))
            .chain(
                std_sources
                    .iter()
                    .map(|(path, text)| input(path, text, ModuleOrigin::Std)),
            )
            .collect();
        let mut index = crate::check::semantic::SemanticIndex::default();
        crate::analyze_inputs_for_editor(
            &inputs,
            &mut index,
            crate::parallel::Parallelism::SEQUENTIAL,
        )
        .unwrap_or_else(|errors| panic!("{source}\n{:?}", errors.diagnostics))
    }

    /// `costly_sites` as it was before PB08: the sites of every function, std included, then those
    /// of user functions.
    fn reference(module: &CheckedModule) -> Vec<CopySite> {
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

    fn call_facts() -> usize {
        crate::ownership::CALL_FACTS.with(std::cell::Cell::get)
    }

    /// PB08 O4: the costly sites from the analysis of the user functions only, and from the call
    /// facts of the check when the module kept them, are those of every function from facts
    /// computed again.
    #[test]
    fn costly_sites_from_the_check_facts_are_the_recomputed_ones() {
        let mut reported = 0;
        for source in SOURCES {
            let editor = editor(source);
            assert!(editor.facts.is_some(), "{source}");
            let expected = reference(editor.module());
            reported += expected.len();
            let before = call_facts();
            assert_eq!(editor.costly_sites(), expected, "{source}");
            assert_eq!(
                call_facts(),
                before,
                "{source}: the facts were computed again"
            );
            assert_eq!(costly_sites(editor.module()), expected, "{source}");
            assert_eq!(call_facts(), before + 1, "{source}");
        }
        assert!(reported >= 10, "{reported}");
        // Device calls: `lower_kernels` numbered them after the check, which keeps no facts.
        let editor = editor(DEVICE);
        assert!(editor.facts.is_none());
        assert!(editor.module().gpu.kernels.len() >= 2);
        let expected = reference(editor.module());
        assert!(!expected.is_empty());
        let before = call_facts();
        assert_eq!(editor.costly_sites(), expected);
        assert_eq!(call_facts(), before + 1);
        assert_eq!(costly_sites(editor.module()), expected);
    }
}
