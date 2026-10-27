//! The std index of `tsuzuri guide std` (G21 Phase 3): one line per public function of the
//! embedded std sources and of the builtins that programs call, made from the parsed sources and
//! `Builtin::scheme`, never from their documentation (G21 appendix J). The output is ASCII,
//! deterministic, and within `LIMIT_STD_MODULE` per module page and `LIMIT_STD_ALL` in all.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::check::{Builtin, BuiltinScheme, BuiltinType, TypeContext};
use crate::docgen::{constraints_text, parameters_text, region_text, signature, type_text};
use crate::guide::{LIMIT_STD_ALL, LIMIT_STD_MODULE, chars};
use crate::syntax::{
    ConstraintExpr, ConstraintName, Edition, Ident, Program, Provenance, TypeExpr, TypeExprKind,
    Visibility,
};

/// The second line of a module page and of `std-all`.
const LEGEND: &str = "ref T borrows (read), ref mut T borrows (write), own T or a plain T moves \
(or copies if Copy). Opt-in modules load when named.";

/// The module of the builtins whose names have no `.`, listed last.
const PRELUDE: &str = "prelude";

/// One module of the index: its type lines and its function lines, each sorted.
#[derive(Clone)]
pub(crate) struct ModuleIndex {
    pub name: String,
    pub opt_in: bool,
    pub types: Vec<String>,
    pub functions: Vec<String>,
}

/// The index of every std module and of the builtins, in module order (`prelude` last).
pub(crate) fn collect() -> Vec<ModuleIndex> {
    let builtins: Vec<(&str, BuiltinScheme)> = Builtin::ALL
        .iter()
        .map(|builtin| (builtin.name(), builtin.scheme()))
        .collect();
    collect_from(crate::stdlib::SOURCES, &builtins)
}

/// The index as `edition` names the std, collected once per process for every supported edition:
/// `--list` renders every std topic.
fn index(edition: Edition) -> &'static [ModuleIndex] {
    static INDEX: OnceLock<BTreeMap<Edition, Vec<ModuleIndex>>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            let base = collect();
            Edition::SUPPORTED
                .iter()
                .map(|edition| (*edition, with_edition(&base, *edition)))
                .collect()
        })
        .get(&edition)
        .map_or(&[], Vec::as_slice)
}

/// `base` with the entry points of `check::BORROWING_APIS`: in every edition the owning name
/// (`Vec.push_owned`) has the legacy signature of the plain one, and in an edition whose
/// parameters borrow (2027) the plain name (`Vec.push`) has the signature of the borrowing one
/// (`Vec.push_mut`), as the checker resolves them.
fn with_edition(base: &[ModuleIndex], edition: Edition) -> Vec<ModuleIndex> {
    let signature = |name: &str| -> Option<String> {
        let module = name.split_once('.').map(|(module, _)| module)?;
        let prefix = format!("{name} :: ");
        base.iter()
            .find(|index| index.name == module)?
            .functions
            .iter()
            .find_map(|line| line.strip_prefix(&prefix).map(str::to_owned))
    };
    let mut modules = base.to_vec();
    for (value, read, owned) in crate::check::BORROWING_APIS {
        let (Some(legacy), Some(module)) = (
            signature(value),
            value.split_once('.').map(|(module, _)| module),
        ) else {
            continue;
        };
        let Some(index) = modules.iter_mut().find(|index| index.name == module) else {
            continue;
        };
        if edition.borrows_parameters()
            && let Some(borrowing) = signature(read)
        {
            let prefix = format!("{value} :: ");
            for line in &mut index.functions {
                if line.starts_with(&prefix) {
                    *line = format!("{value} :: {borrowing}");
                }
            }
        }
        let owning = format!("{owned} :: ");
        if !index.functions.iter().any(|line| line.starts_with(&owning)) {
            index.functions.push(format!("{owned} :: {legacy}"));
        }
        index.functions.sort();
    }
    modules
}

/// `tsuzuri guide std`: one line per module.
pub(crate) fn render_modules(edition: Edition) -> String {
    modules_text(index(edition))
}

/// `tsuzuri guide std MODULE`: every function of one module; `None` for an unknown module.
pub(crate) fn render_module(module: &str, edition: Edition) -> Option<String> {
    index(edition)
        .iter()
        .find(|index| index.name == module)
        .map(|index| module_page(index, LIMIT_STD_MODULE))
}

/// `tsuzuri guide --topic std-all`: every function of every module.
pub(crate) fn render_all(edition: Edition) -> String {
    all_text(index(edition), LIMIT_STD_ALL)
}

/// The lines of one module while it is collected, keyed so that a builtin replaces a source
/// line of the same qualified name and the order is the name order.
#[derive(Default)]
struct Entries {
    /// (declared name, line).
    types: Vec<(String, String)>,
    /// Qualified name (`Array.fold`, `sqrt`) to its line.
    functions: BTreeMap<String, String>,
}

fn collect_from(sources: &[(&str, &str)], builtins: &[(&str, BuiltinScheme)]) -> Vec<ModuleIndex> {
    let mut modules: BTreeMap<String, Entries> = BTreeMap::new();
    for (path, text) in sources {
        let Some(module) = crate::stdlib::module_name(path) else {
            continue;
        };
        // The embedded std parses whenever a program is checked, so this never fails.
        let parsed = crate::parser::parse_with_source(text, 0, Edition::LATEST);
        debug_assert!(parsed.is_ok(), "{path} does not parse");
        let Ok(program) = parsed else {
            continue;
        };
        source_entries(
            module,
            &program,
            modules.entry(module.to_owned()).or_default(),
        );
    }
    for (name, scheme) in builtins {
        if internal_builtin(name) {
            continue;
        }
        let module = name.split_once('.').map_or(PRELUDE, |(module, _)| module);
        let line = format!("{name} :: {}", builtin_signature(scheme));
        modules
            .entry(module.to_owned())
            .or_default()
            .functions
            .insert((*name).to_owned(), line);
    }
    let mut index: Vec<ModuleIndex> = modules
        .into_iter()
        .map(|(name, entries)| {
            let mut types = entries.types;
            types.sort();
            let mut functions: Vec<String> = entries.functions.into_values().collect();
            functions.sort();
            ModuleIndex {
                opt_in: crate::stdlib::is_opt_in(&name),
                name,
                types: types.into_iter().map(|(_, line)| line).collect(),
                functions,
            }
        })
        .collect();
    // ASCII order already puts the lowercase `prelude` after every module; keep it last anyway.
    index.sort_by_key(|module| module.name == PRELUDE);
    index
}

/// A builtin that programs do not call by name: an internal hook (`Array.__set`,
/// `$builtin.display`) or a test-only builtin of this crate's unit tests.
fn internal_builtin(name: &str) -> bool {
    name.contains("__") || name.starts_with('$') || (cfg!(test) && name.starts_with("Int.test_"))
}

/// Whether a source function or method named `name` belongs in the index: not `private`, not a
/// builder operation (`Bind`, `Return`, ...), not an internal `__` name.
fn listed(name: &str, visibility: Visibility) -> bool {
    visibility != Visibility::Private
        && !name.starts_with(|first: char| first.is_ascii_uppercase())
        && !name.contains("__")
}

/// `Module.name :: <signature>`, one line, from `docgen::signature`.
fn function_line<'a>(
    qualified: &str,
    regions: &[Ident],
    constraints: &[ConstraintExpr],
    parameters: impl Iterator<Item = &'a TypeExpr>,
    result: &'a TypeExpr,
) -> String {
    let text = signature("def", qualified, regions, constraints, parameters, result);
    text.strip_prefix("def ")
        .unwrap_or(&text)
        .replace("\n    ", " ")
}

fn source_entries(module: &str, program: &Program, entries: &mut Entries) {
    for function in &program.functions {
        if listed(&function.name.text, function.visibility) {
            let qualified = format!("{module}.{}", function.name.text);
            let line = function_line(
                &qualified,
                &function.regions,
                &function.constraints,
                function.parameters.iter().map(|parameter| &parameter.ty),
                &function.result,
            );
            entries.functions.entry(qualified).or_insert(line);
        }
    }
    for function in &program.externs {
        if listed(&function.name.text, function.visibility) {
            let qualified = format!("{module}.{}", function.name.text);
            let line = function_line(
                &qualified,
                &function.regions,
                &function.constraints,
                function.parameters.iter(),
                &function.result,
            );
            entries.functions.entry(qualified).or_insert(line);
        }
    }
    for class in &program.classes {
        // The class's own constraint comes first: `class Sized<'a>`'s `size` is
        // `Sized<'a> => ...`. Superclasses appear only on the class's type line.
        let own = ConstraintExpr {
            name: ConstraintName::Class(class.name.clone()),
            ty: TypeExpr {
                kind: TypeExprKind::Variable(class.variable.text.clone()),
                span: class.variable.span,
            },
        };
        for method in &class.methods {
            if listed(&method.name.text, method.visibility) {
                let qualified = format!("{}.{}", class.name.text, method.name.text);
                let constraints: Vec<ConstraintExpr> = std::iter::once(own.clone())
                    .chain(method.constraints.iter().cloned())
                    .collect();
                let line = function_line(
                    &qualified,
                    &method.regions,
                    &constraints,
                    method.parameters.iter(),
                    &method.result,
                );
                entries.functions.entry(qualified).or_insert(line);
            }
        }
    }
    type_entries(module, program, &mut entries.types);
}

/// The type lines of a module: records, unions, type aliases, constants, classes and extern
/// types, each on one line.
fn type_entries(module: &str, program: &Program, types: &mut Vec<(String, String)>) {
    for record in &program.records {
        if record.visibility == Visibility::Private {
            continue;
        }
        let head = format!(
            "record {}{}{}",
            record.name.text,
            parameters_text(&record.parameters),
            region_text(&record.regions)
        );
        let line = if crate::stdlib::opaque_record(&format!("{module}.{}", record.name.text)) {
            format!("{head} (opaque)")
        } else if record.fields.is_empty() {
            format!("{head} {{}}")
        } else {
            let fields: Vec<String> = record
                .fields
                .iter()
                .map(|field| format!("{}: {}", field.name.text, type_text(&field.ty)))
                .collect();
            format!("{head} {{ {} }}", fields.join(", "))
        };
        types.push((record.name.text.clone(), line));
    }
    for union in &program.unions {
        if union.visibility == Visibility::Private || union.name.provenance == Provenance::Generated
        {
            continue;
        }
        let cases: Vec<String> = union
            .cases
            .iter()
            .map(|case| match &case.payload {
                Some(payload) => format!("{} of {}", case.name.text, type_text(payload)),
                None => case.name.text.clone(),
            })
            .collect();
        let line = format!(
            "union {}{} = {}",
            union.name.text,
            parameters_text(&union.parameters),
            cases.join(" | ")
        );
        types.push((union.name.text.clone(), line));
    }
    for alias in &program.type_aliases {
        if alias.visibility == Visibility::Private {
            continue;
        }
        let line = format!(
            "type {}{} = {}",
            alias.name.text,
            parameters_text(&alias.parameters),
            type_text(&alias.target)
        );
        types.push((alias.name.text.clone(), line));
    }
    for constant in &program.constants {
        if constant.visibility == Visibility::Private {
            continue;
        }
        let line = format!(
            "const {module}.{}: {}",
            constant.name.text,
            type_text(&constant.ty)
        );
        types.push((constant.name.text.clone(), line));
    }
    for class in &program.classes {
        let methods: Vec<&str> = class
            .methods
            .iter()
            .filter(|method| method.visibility != Visibility::Private)
            .map(|method| method.name.text.as_str())
            .collect();
        let kind = if class.kind == crate::syntax::Kind::Type {
            String::new()
        } else {
            format!(": {}", class.kind.display())
        };
        let line = format!(
            "class {}{}<'{}{kind}> {{ {} }}",
            constraints_text(&class.superclasses),
            class.name.text,
            class.variable.text,
            methods.join(", ")
        );
        types.push((class.name.text.clone(), line));
    }
    for extern_type in &program.extern_types {
        if extern_type.visibility == Visibility::Private {
            continue;
        }
        let line = format!("extern type {}", extern_type.name.text);
        types.push((extern_type.name.text.clone(), line));
    }
}

/// A builtin's scheme as source code would write its signature:
/// `(C1<'a>, C2<'b>) => P1 -> P2 -> R`.
fn builtin_signature(scheme: &BuiltinScheme) -> String {
    let constraints: Vec<String> = scheme
        .constraints
        .iter()
        .map(|constraint| format!("{}<{}>", constraint.class, builtin_type(&constraint.ty)))
        .collect();
    let constraints = match constraints.as_slice() {
        [] => String::new(),
        [constraint] => format!("{constraint} => "),
        _ => format!("({}) => ", constraints.join(", ")),
    };
    let types: Vec<String> = scheme
        .parameters
        .iter()
        .chain([&scheme.result])
        .map(builtin_type)
        .collect();
    format!("{constraints}{}", types.join(" -> "))
}

fn builtin_types(types: &[BuiltinType], separator: &str) -> String {
    types
        .iter()
        .map(builtin_type)
        .collect::<Vec<_>>()
        .join(separator)
}

/// One type of a builtin's scheme. Every variant is written out: a new variant must choose its
/// surface syntax here (G21 stop condition 7).
fn builtin_type(ty: &BuiltinType) -> String {
    match ty {
        BuiltinType::Var(name) => format!("'{name}"),
        // A primitive type, never a project's record or union, so no declarations are needed.
        BuiltinType::Concrete(ty) => ty.display(&TypeContext {
            records: &[],
            unions: &[],
        }),
        BuiltinType::Std {
            module: _,
            name,
            args,
        } => {
            if args.is_empty() {
                (*name).to_owned()
            } else {
                format!("{name}<{}>", builtin_types(args, ", "))
            }
        }
        BuiltinType::Array(element) => format!("[{}]", builtin_type(element)),
        BuiltinType::ArrayView(element) => format!("[{}..]", builtin_type(element)),
        BuiltinType::List(element) => format!("[|{}|]", builtin_type(element)),
        BuiltinType::Vec(element) => format!("Vec<{}>", builtin_type(element)),
        BuiltinType::Tuple(elements) => format!("({})", builtin_types(elements, " * ")),
        BuiltinType::Task(result) => format!("Task<{}>", builtin_type(result)),
        BuiltinType::Shared(value, kind) => format!("{}<{}>", kind.name(), builtin_type(value)),
        BuiltinType::Reference(target, false) => format!("ref {}", builtin_type(target)),
        BuiltinType::Reference(target, true) => format!("ref mut {}", builtin_type(target)),
        BuiltinType::Function(parameters, result) => {
            if parameters.is_empty() {
                format!("(fn() -> {})", builtin_type(result))
            } else {
                format!(
                    "({} -> {})",
                    builtin_types(parameters, " -> "),
                    builtin_type(result)
                )
            }
        }
        // Types computed from a type, which source code cannot write: the unsigned and the wider
        // integer of an integer type, and the lane (scalar) and mask types of a SIMD vector type.
        // `Simd.splat :: SimdVector<'a> => Lane<'a> -> 'a` takes a scalar; `of_lanes4` also
        // requires a vector of 4 lanes, `Lane<'a, 4>`.
        BuiltinType::UnsignedOf(integer) => format!("Unsigned<{}>", builtin_type(integer)),
        BuiltinType::WidenOf(integer) => format!("Wide<{}>", builtin_type(integer)),
        BuiltinType::SimdLane(vector, Some(lanes)) => {
            format!("Lane<{}, {lanes}>", builtin_type(vector))
        }
        BuiltinType::SimdLane(vector, None) => format!("Lane<{}>", builtin_type(vector)),
        BuiltinType::SimdMask(vector) => format!("Mask<{}>", builtin_type(vector)),
        BuiltinType::FixedArrayOf(element) => format!("[{}; N]", builtin_type(element)),
    }
}

/// `tsuzuri guide std`.
fn modules_text(modules: &[ModuleIndex]) -> String {
    let mut text = String::from("# std modules\n");
    for module in modules {
        text.push_str(&format!(
            "{:<12} {:>4} functions{}\n",
            module.name,
            module.functions.len(),
            if module.opt_in { "  (opt-in)" } else { "" }
        ));
    }
    text.push_str("Details: tsuzuri guide std NAME. Everything: tsuzuri guide --topic std-all.\n");
    text
}

fn lines(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// `tsuzuri guide std MODULE`, at most `limit` characters: when the whole page is longer, the
/// function lines are dropped from the end and a last line counts them.
fn module_page(module: &ModuleIndex, limit: usize) -> String {
    let mut text = format!("# {}\n{LEGEND}\n\n{}", module.name, lines(&module.types));
    let full = chars(&text)
        + module
            .functions
            .iter()
            .map(|line| chars(line) + 1)
            .sum::<usize>();
    if full <= limit {
        text.push_str(&lines(&module.functions));
        return text;
    }
    let mut kept = module.functions.len();
    let mut size = full;
    let more = loop {
        let Some(last) = kept.checked_sub(1) else {
            break format!(
                "... {} more: tsuzuri guide --topic std-all\n",
                module.functions.len()
            );
        };
        kept = last;
        size -= chars(&module.functions[kept]) + 1;
        let more = format!(
            "... {} more: tsuzuri guide --topic std-all\n",
            module.functions.len() - kept
        );
        if size + chars(&more) <= limit {
            break more;
        }
    };
    text.push_str(&lines(&module.functions[..kept]));
    text.push_str(&more);
    debug_assert!(
        chars(&text) <= limit,
        "the type lines of {} overflow",
        module.name
    );
    text
}

/// `tsuzuri guide --topic std-all`, at most `limit` characters: while it is longer, the body of
/// one module after another becomes one line: first the modules whose own page is complete, then
/// those whose page is truncated (std-all is where the rest of their functions is), each group
/// opt-in modules first and then largest first.
fn all_text(modules: &[ModuleIndex], limit: usize) -> String {
    let header = format!("# std index\n{LEGEND}\n");
    let headings: Vec<String> = modules
        .iter()
        .map(|module| format!("\n## {} ({})\n", module.name, module.functions.len()))
        .collect();
    let mut bodies: Vec<String> = modules
        .iter()
        .map(|module| format!("{}{}", lines(&module.types), lines(&module.functions)))
        .collect();
    let mut total = chars(&header)
        + headings.iter().map(|text| chars(text)).sum::<usize>()
        + bodies.iter().map(|text| chars(text)).sum::<usize>();
    let mut order: Vec<usize> = (0..modules.len()).collect();
    order.sort_by_key(|&position| {
        let module = &modules[position];
        let size: usize = module.functions.iter().map(|line| chars(line)).sum();
        let truncated = chars(&module_page(module, usize::MAX)) > LIMIT_STD_MODULE;
        (
            truncated,
            !module.opt_in,
            std::cmp::Reverse(size),
            module.name.as_str(),
        )
    });
    for position in order {
        if total <= limit {
            break;
        }
        let name = &modules[position].name;
        let collapsed = format!(
            "{name}: {} functions; run 'tsuzuri guide std {name}'\n",
            modules[position].functions.len()
        );
        total = total - chars(&bodies[position]) + chars(&collapsed);
        bodies[position] = collapsed;
    }
    debug_assert!(
        total <= limit,
        "std-all overflows even with every module collapsed"
    );
    let mut text = header;
    for (heading, body) in headings.iter().zip(&bodies) {
        text.push_str(heading);
        text.push_str(body);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{BuiltinConstraint, SharedKind, Type};

    fn var(name: &'static str) -> BuiltinType {
        BuiltinType::Var(name)
    }

    fn boxed(ty: BuiltinType) -> Box<BuiltinType> {
        Box::new(ty)
    }

    fn scheme(
        parameters: Vec<BuiltinType>,
        result: BuiltinType,
        constraints: Vec<(&'static str, BuiltinType)>,
    ) -> BuiltinScheme {
        BuiltinScheme {
            parameters,
            result,
            variables: Vec::new(),
            constraints: constraints
                .into_iter()
                .map(|(class, ty)| BuiltinConstraint { class, ty })
                .collect(),
        }
    }

    fn builtin(name: &str) -> BuiltinScheme {
        Builtin::ALL
            .iter()
            .find(|builtin| builtin.name() == name)
            .unwrap_or_else(|| panic!("no builtin {name}"))
            .scheme()
    }

    fn module<'a>(index: &'a [ModuleIndex], name: &str) -> &'a ModuleIndex {
        index
            .iter()
            .find(|module| module.name == name)
            .unwrap_or_else(|| panic!("no module {name}"))
    }

    /// The lines of `module` that start with `qualified :: `.
    fn lines_of<'a>(index: &'a [ModuleIndex], name: &str, qualified: &str) -> Vec<&'a str> {
        let prefix = format!("{qualified} :: ");
        module(index, name)
            .functions
            .iter()
            .filter(|line| line.starts_with(&prefix))
            .map(String::as_str)
            .collect()
    }

    /// A synthetic module whose function lines are `Name.fNNNN :: own i64 -> i64`.
    #[test]
    fn the_edition_chooses_the_entry_point_of_a_plain_name() {
        let base = [ModuleIndex {
            name: "Vec".to_owned(),
            opt_in: false,
            types: Vec::new(),
            functions: vec![
                "Vec.push :: 'a -> Vec<'a> -> Vec<'a>".to_owned(),
                "Vec.push_mut :: 'a -> ref mut Vec<'a> -> unit".to_owned(),
            ],
        }];
        let lines = |edition| with_edition(&base, edition)[0].functions.clone();
        assert_eq!(
            lines(Edition::E2026),
            [
                "Vec.push :: 'a -> Vec<'a> -> Vec<'a>",
                "Vec.push_mut :: 'a -> ref mut Vec<'a> -> unit",
                "Vec.push_owned :: 'a -> Vec<'a> -> Vec<'a>",
            ]
        );
        assert_eq!(
            lines(Edition::E2027),
            [
                "Vec.push :: 'a -> ref mut Vec<'a> -> unit",
                "Vec.push_mut :: 'a -> ref mut Vec<'a> -> unit",
                "Vec.push_owned :: 'a -> Vec<'a> -> Vec<'a>",
            ]
        );
    }

    fn probe(name: &str, opt_in: bool, count: usize) -> ModuleIndex {
        ModuleIndex {
            name: name.to_owned(),
            opt_in,
            types: Vec::new(),
            functions: (0..count)
                .map(|index| format!("{name}.f{index:04} :: own i64 -> i64"))
                .collect(),
        }
    }

    fn joined(lines: &[String]) -> String {
        lines.iter().map(|line| format!("{line}\n")).collect()
    }

    #[test]
    fn builtin_signatures_render_every_variant() {
        let every = scheme(
            vec![
                var("a"),
                BuiltinType::Concrete(Type::I64),
                BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![var("a")],
                },
                BuiltinType::Std {
                    module: "Probe",
                    name: "Plain",
                    args: Vec::new(),
                },
                BuiltinType::Array(boxed(var("a"))),
                BuiltinType::Reference(boxed(BuiltinType::ArrayView(boxed(var("a")))), true),
                BuiltinType::List(boxed(var("a"))),
                BuiltinType::Vec(boxed(var("a"))),
                BuiltinType::Tuple(vec![var("a"), BuiltinType::Concrete(Type::Bool)]),
                BuiltinType::Task(boxed(var("a"))),
                BuiltinType::Shared(boxed(var("a")), SharedKind::RcWeak),
                BuiltinType::Shared(boxed(var("a")), SharedKind::Arc),
                BuiltinType::Reference(boxed(var("a")), false),
                BuiltinType::Function(vec![var("a"), var("b")], boxed(var("c"))),
                BuiltinType::Function(Vec::new(), boxed(var("a"))),
                BuiltinType::UnsignedOf(boxed(var("a"))),
                BuiltinType::WidenOf(boxed(var("a"))),
                BuiltinType::SimdLane(boxed(var("a")), Some(4)),
                BuiltinType::SimdLane(boxed(var("a")), None),
                BuiltinType::SimdMask(boxed(var("a"))),
            ],
            BuiltinType::FixedArrayOf(boxed(var("a"))),
            vec![("Integer", var("a")), ("Copy", var("b"))],
        );
        assert_eq!(
            builtin_signature(&every),
            "(Integer<'a>, Copy<'b>) => 'a -> i64 -> Maybe<'a> -> Plain -> ['a] -> ref mut ['a..] \
             -> [|'a|] -> Vec<'a> -> ('a * bool) -> Task<'a> -> Rc.Weak<'a> -> Arc<'a> -> ref 'a \
             -> ('a -> 'b -> 'c) -> (fn() -> 'a) -> Unsigned<'a> -> Wide<'a> -> Lane<'a, 4> \
             -> Lane<'a> -> Mask<'a> -> ['a; N]"
        );
        // One constraint goes without parentheses; none leaves no `=>`.
        let one = scheme(vec![var("a")], var("a"), vec![("Float", var("a"))]);
        assert_eq!(builtin_signature(&one), "Float<'a> => 'a -> 'a");
        let none = scheme(Vec::new(), BuiltinType::Concrete(Type::Bool), Vec::new());
        assert_eq!(builtin_signature(&none), "bool");
        // Real builtins, derived by hand from their arms in `Builtin::scheme`.
        for (name, expected) in [
            ("Vec.push", "'a -> Vec<'a> -> Vec<'a>"),
            ("Vec.push_mut", "'a -> ref mut Vec<'a> -> unit"),
            ("Vec.get", "Copy<'a> => i64 -> ref Vec<'a> -> Maybe<'a>"),
            ("Int.abs", "SignedInteger<'a> => 'a -> 'a"),
            ("Int.widening_mul", "Integer<'a> => 'a -> 'a -> Wide<'a>"),
            ("Math.pi", "Float<'a> => 'a"),
            ("Rc.new", "'a -> Rc<'a>"),
            ("Arc.downgrade", "ref Arc<'a> -> Arc.Weak<'a>"),
            ("Simd.splat", "SimdVector<'a> => Lane<'a> -> 'a"),
            (
                "Simd.of_lanes2",
                "SimdVector<'a> => Lane<'a, 2> -> Lane<'a, 2> -> 'a",
            ),
            ("Simd.eq", "SimdNumeric<'a> => 'a -> 'a -> Mask<'a>"),
            ("sqrt", "f64 -> f64"),
        ] {
            assert_eq!(builtin_signature(&builtin(name)), expected, "{name}");
        }
    }

    #[test]
    fn collect_excludes_private_builder_and_internal_names() {
        let source = "def visible :: own i64 -> i64\nfn visible n = n\n\n\
                      private def hidden :: own i64 -> i64\nfn hidden n = n\n\n\
                      def Bind :: own i64 -> i64\nfn Bind n = n\n\n\
                      def raw__hook :: own i64 -> i64\nfn raw__hook n = n\n";
        let unit = || scheme(Vec::new(), BuiltinType::Concrete(Type::I64), Vec::new());
        let builtins = [
            ("Probe.__hook", unit()),
            ("$builtin.probe", unit()),
            ("Probe.native", unit()),
        ];
        let index = collect_from(&[("std/Probe.tc", source)], &builtins);
        assert_eq!(index.len(), 1, "an excluded builtin adds no module");
        assert_eq!(
            index[0].functions,
            ["Probe.native :: i64", "Probe.visible :: own i64 -> i64"]
        );

        let index = collect();
        assert!(lines_of(&index, "Maybe", "Maybe.Bind").is_empty());
        assert!(lines_of(&index, "Maybe", "Maybe.BindReturn").is_empty());
        assert!(lines_of(&index, "Os", "Os.__read").is_empty());
        // `private def rec iter_from` in std/Vec.tz and `private def reverse_in_place` in
        // std/Array.tz.
        assert!(lines_of(&index, "Vec", "Vec.iter_from").is_empty());
        assert!(lines_of(&index, "Array", "Array.reverse_in_place").is_empty());
        assert!(lines_of(&index, "Int", "Int.test_add").is_empty());
        assert_eq!(
            lines_of(&index, "Maybe", "Maybe.is_some"),
            ["Maybe.is_some :: ref Maybe<'a> -> bool"],
            "`&(Maybe<'a>)` in std/Maybe.tc reads as `ref`"
        );
        for module in &index {
            assert!(!module.name.starts_with('$'), "{}", module.name);
            for line in &module.functions {
                let name = line.split(" :: ").next().unwrap();
                assert!(!name.contains("__"), "{line}");
            }
        }
    }

    #[test]
    fn collect_merges_a_source_and_a_builtin_with_the_same_name() {
        let source = "def abs :: own f64 -> f64\nfn abs x = x\n\n\
                      def twice :: own f64 -> f64\nfn twice x = x + x\n";
        let index = collect_from(
            &[("std/Math.tz", source)],
            &[("Math.abs", builtin("Math.abs"))],
        );
        assert_eq!(index.len(), 1);
        assert_eq!(
            index[0].functions,
            [
                "Math.abs :: Float<'a> => 'a -> 'a",
                "Math.twice :: own f64 -> f64"
            ]
        );
        assert_eq!(
            lines_of(&collect(), "Math", "Math.abs"),
            ["Math.abs :: Float<'a> => 'a -> 'a"]
        );
    }

    #[test]
    fn collect_lists_class_methods() {
        let source = "class Sized<'a> {\n    def size :: ref 'a -> i64\n    \
                      def same :: ref 'a -> ref 'a -> bool\n}\n";
        let index = collect_from(&[("std/Probe.tt", source)], &[]);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "Probe");
        assert_eq!(
            index[0].functions,
            [
                "Sized.same :: Sized<'a> => ref 'a -> ref 'a -> bool",
                "Sized.size :: Sized<'a> => ref 'a -> i64",
            ]
        );
        assert_eq!(index[0].types, ["class Sized<'a> { size, same }"]);

        // A superclass is only on the class's line; a method's own constraints follow the class.
        let ranked = "class Eq<'v> => Ranked<'v> {\n    \
                      def rank :: Display<'b> => ref 'v -> ref 'b -> i64\n}\n";
        let index = collect_from(&[("std/Probe.tt", ranked)], &[]);
        assert_eq!(
            index[0].functions,
            ["Ranked.rank :: (Ranked<'v>, Display<'b>) => ref 'v -> ref 'b -> i64"]
        );
        assert_eq!(index[0].types, ["class Eq<'v> => Ranked<'v> { rank }"]);

        // The parser makes every method public, so mark one private by hand.
        let mut program =
            crate::parser::parse_with_source(source, 0, Edition::LATEST).expect("parses");
        program.classes[0].methods[1].visibility = Visibility::Private;
        let mut entries = Entries::default();
        source_entries("Probe", &program, &mut entries);
        assert_eq!(
            entries.functions.into_values().collect::<Vec<_>>(),
            ["Sized.size :: Sized<'a> => ref 'a -> i64"]
        );
        assert_eq!(
            entries.types,
            [("Sized".to_owned(), "class Sized<'a> { size }".to_owned())]
        );
    }

    #[test]
    fn render_module_truncates_with_a_more_line() {
        let small = ModuleIndex {
            name: "Small".into(),
            opt_in: false,
            types: vec!["record Small { n: i64 }".into()],
            functions: vec!["Small.a :: i64".into(), "Small.b :: own i64 -> i64".into()],
        };
        assert_eq!(
            module_page(&small, LIMIT_STD_MODULE),
            format!(
                "# Small\n{LEGEND}\n\nrecord Small {{ n: i64 }}\nSmall.a :: i64\n\
                 Small.b :: own i64 -> i64\n"
            )
        );

        // 300 lines of 29 characters and a newline: 9,000 characters.
        let mut large = probe("Probe", false, 300);
        assert_eq!(large.functions[0].len(), 29);
        large.types = vec!["record Probe { n: i64 }".into()];
        let head = format!("# Probe\n{LEGEND}\n\nrecord Probe {{ n: i64 }}\n");
        let more = |dropped: usize| format!("... {dropped} more: tsuzuri guide --topic std-all\n");
        let kept = (0..=300)
            .rev()
            .find(|&kept| head.len() + 30 * kept + more(300 - kept).len() <= LIMIT_STD_MODULE)
            .unwrap();
        assert!(kept < 300);
        let page = module_page(&large, LIMIT_STD_MODULE);
        assert_eq!(
            page,
            format!(
                "{head}{}{}",
                joined(&large.functions[..kept]),
                more(300 - kept)
            )
        );
        assert!(chars(&page) <= LIMIT_STD_MODULE);
        assert!(page.ends_with(&format!(
            "Probe.f{:04} :: own i64 -> i64\n... {} more: tsuzuri guide --topic std-all\n",
            kept - 1,
            300 - kept
        )));
    }

    #[test]
    fn render_all_collapses_opt_in_modules_first() {
        let header = format!("# std index\n{LEGEND}\n");
        let section = |module: &ModuleIndex| {
            format!(
                "\n## {} ({})\n{}",
                module.name,
                module.functions.len(),
                joined(&module.functions)
            )
        };
        let collapsed = |name: &str, count: usize| {
            format!(
                "\n## {name} ({count})\n{name}: {count} functions; run 'tsuzuri guide std {name}'\n"
            )
        };

        // 29,000 + 15,000 + 9,300 + 280 characters of lines. `Wee`, whose own page is complete,
        // is collapsed first; then the largest opt-in module whose page is truncated (`Large`),
        // and the rest fits, so `Core` and `Medium` stay.
        let index = [
            probe("Core", false, 1000),
            probe("Large", true, 500),
            probe("Medium", true, 300),
            probe("Wee", true, 10),
        ];
        let text = all_text(&index, LIMIT_STD_ALL);
        assert_eq!(
            text,
            format!(
                "{header}{}{}{}{}",
                section(&index[0]),
                collapsed("Large", 500),
                section(&index[2]),
                collapsed("Wee", 10)
            )
        );
        assert!(chars(&text) <= LIMIT_STD_ALL);

        // When every opt-in module is collapsed and the text is still too long, the other modules
        // follow in the same order.
        let index = [
            probe("Core", false, 1600),
            probe("Large", true, 500),
            probe("Medium", true, 300),
            probe("Wee", true, 10),
        ];
        assert_eq!(
            all_text(&index, LIMIT_STD_ALL),
            format!(
                "{header}{}{}{}{}",
                collapsed("Core", 1600),
                collapsed("Large", 500),
                collapsed("Medium", 300),
                collapsed("Wee", 10)
            )
        );

        // Equal sizes go by name: `Beta` before `Zeta`.
        let index = [
            probe("Beta", true, 600),
            probe("Core", false, 800),
            probe("Zeta", true, 600),
        ];
        assert_eq!(
            all_text(&index, LIMIT_STD_ALL),
            format!(
                "{header}{}{}{}",
                collapsed("Beta", 600),
                section(&index[1]),
                section(&index[2])
            )
        );

        // A text within the limit is not collapsed at all.
        let index = [probe("Core", false, 10), probe("Wee", true, 10)];
        assert_eq!(
            all_text(&index, LIMIT_STD_ALL),
            format!("{header}{}{}", section(&index[0]), section(&index[1]))
        );
    }

    #[test]
    fn the_std_index_is_ascii_within_the_limits_and_follows_the_sources() {
        let index = collect();
        let names: Vec<&str> = index.iter().map(|module| module.name.as_str()).collect();
        for (path, _) in crate::stdlib::SOURCES {
            let name = crate::stdlib::module_name(path).unwrap();
            assert!(names.contains(&name), "{name}");
        }
        for name in ["Int", "Simd", "Rc", "Arc", "FixedArray", "Dyn", "Task"] {
            assert!(names.contains(&name), "{name}");
        }
        assert_eq!(names.last(), Some(&PRELUDE));
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert!(module(&index, "Json").opt_in && !module(&index, "Array").opt_in);

        for &edition in Edition::SUPPORTED {
            let list = render_modules(edition);
            assert!(list.is_ascii());
            assert_eq!(list.lines().count(), index.len() + 2);
            for name in &names {
                let page = render_module(name, edition).unwrap();
                assert!(page.is_ascii(), "{name}");
                assert!(chars(&page) <= LIMIT_STD_MODULE, "{name}");
            }
            let all = render_all(edition);
            assert!(all.is_ascii());
            assert!(chars(&all) <= LIMIT_STD_ALL);
            assert_eq!(render_all(edition), all, "deterministic");
            assert_eq!(render_module("Nope", edition), None);
            assert_eq!(
                render_module("array", edition),
                None,
                "module names are case-sensitive"
            );
        }

        // The independent reference: the `def fold ::` line of std/Array.tz.
        let fold = include_str!("../std/Array.tz")
            .lines()
            .find_map(|line| line.strip_prefix("def fold :: "))
            .unwrap();
        assert_eq!(
            lines_of(&index, "Array", "Array.fold"),
            [format!("Array.fold :: {fold}")]
        );
        assert_eq!(lines_of(&index, "prelude", "sqrt"), ["sqrt :: f64 -> f64"]);
    }
}
