//! Which std functions carry a doc example (G34 Phase 3). The floors only go up.
//!
//! Every public function of `std/` whose name starts with a lowercase letter has an example in its documentation
//! that is not `ignore`, and the builder operation `Bind` of each `.tc` module has one that uses the computation
//! expression. `tsuzuri test --doc std` runs them; this file counts them. A ticket that adds a std module adds it to
//! `FLOORS`, and a ticket that adds a public std function gives it an example (`_features/_completed/G34-doc-tests.md`).

use std::collections::{BTreeMap, BTreeSet};

use tsuzuri::doctest::{self, Expectation, OwnerKind};
use tsuzuri::parser;
use tsuzuri::stdlib::SOURCES;
use tsuzuri::syntax::{Edition, Visibility};

/// (module, public functions that have an example): every module of `stdlib::SOURCES`, in alphabetical order.
/// Each entry is the count when the module got its examples, and stays as a record.
const FLOORS: &[(&str, usize)] = &[
    ("Alloc", 3),
    ("Arena", 10),
    ("Array", 154),
    ("Async", 9),
    ("Atomic", 0),
    ("Bench", 2),
    ("BigInt", 4),
    ("Cbor", 4),
    ("Channel", 0),
    ("Char", 6),
    ("Debug", 2),
    ("Dir", 4),
    ("Domain", 1),
    ("Env", 3),
    ("Exception", 0),
    ("File", 14),
    ("Format", 2),
    ("Gen", 24),
    ("Gpu", 9),
    ("HashMap", 24),
    ("HashSet", 17),
    ("IO", 14),
    ("Json", 28),
    ("List", 126),
    ("Map", 16),
    ("Math", 1),
    ("Matrix", 18),
    ("MatrixView", 29),
    ("Maybe", 13),
    ("Monoid", 0),
    ("Monoids", 0),
    ("Mutex", 0),
    ("Net", 40),
    ("Os", 6),
    ("Owned", 0),
    ("Parallel", 1),
    ("Path", 4),
    ("Process", 1),
    ("Random", 5),
    ("Regex", 25),
    ("RegexText", 0),
    ("Result", 14),
    ("Seq", 7),
    ("Set", 15),
    ("String", 21),
    ("Tensor", 27),
    ("Test", 3),
    ("Text", 0),
    ("Time", 3),
    ("Unicode", 24),
    ("Utf8Char", 6),
    ("Utf8String", 20),
    ("Vec", 68),
];

/// Public functions that need no example, with the reason.
const EXEMPT: &[(&str, &[&str], &str)] = &[(
    "Json",
    &[
        "begin_object",
        "encode_field",
        "end_object",
        "encode_case",
        "encode_tag",
        "expect_object",
        "decode_field",
        "keep_error",
        "case_index",
        "decode_payload",
    ],
    "the protocol of the code that `derive(Json)` generates, not API",
)];

/// The `.tc` modules whose `Bind` carries at least this many examples.
const BUILDER_FLOORS: &[(&str, usize)] = &[("Async", 1), ("IO", 1), ("Maybe", 1), ("Result", 1)];

struct Tally {
    /// The public functions that must have an example.
    functions: Vec<String>,
    /// The public builder operations (a `.tc` module's `def` that starts with an uppercase letter).
    builders: usize,
    with_example: BTreeSet<String>,
    bind_examples: usize,
}

fn module_of(path: &str) -> &str {
    path.trim_start_matches("std/")
        .split('.')
        .next()
        .expect("a std path has a module name")
}

fn exempt(module: &str) -> &'static [&'static str] {
    EXEMPT
        .iter()
        .find(|(name, _, _)| *name == module)
        .map_or(&[][..], |(_, names, _)| *names)
}

fn tally() -> BTreeMap<String, Tally> {
    let mut modules = BTreeMap::new();
    for (index, (path, text)) in SOURCES.iter().enumerate() {
        let module = module_of(path);
        let program = parser::parse_with_source(text, index, Edition::LATEST)
            .unwrap_or_else(|error| panic!("{path}: {}", error.message));
        let public: Vec<&str> = program
            .functions
            .iter()
            .filter(|function| function.visibility == Visibility::Public)
            .map(|function| function.name.text.as_str())
            .collect();
        let builders = public
            .iter()
            .filter(|name| name.starts_with(|c: char| c.is_ascii_uppercase()))
            .count();
        let functions = public
            .iter()
            .filter(|name| !name.starts_with(|c: char| c.is_ascii_uppercase()))
            .filter(|name| !exempt(module).contains(name))
            .map(|name| (*name).to_owned())
            .collect();
        let examples = doctest::examples_in_source(text, Edition::LATEST, index, module)
            .unwrap_or_else(|errors| {
                let messages: Vec<String> = errors
                    .iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect();
                panic!("{path}: {}", messages.join("; "))
            });
        let live: Vec<&doctest::Example> = examples
            .iter()
            .filter(|example| example.expectation != Expectation::Ignore)
            .collect();
        let with_example = live
            .iter()
            .filter(|example| example.owner_kind == OwnerKind::Function)
            .map(|example| example.owner.clone())
            .collect();
        let bind_examples = live
            .iter()
            .filter(|example| example.owner == "Bind")
            .count();
        modules.insert(
            module.to_owned(),
            Tally {
                functions,
                builders,
                with_example,
                bind_examples,
            },
        );
    }
    modules
}

fn covered(tally: &Tally) -> usize {
    tally
        .functions
        .iter()
        .filter(|name| tally.with_example.contains(*name))
        .count()
}

#[test]
fn std_examples_meet_the_floors() {
    let modules = tally();
    let floors: BTreeMap<&str, usize> = FLOORS.iter().copied().collect();
    let (mut functions, mut examples) = (0, 0);
    let mut below = Vec::new();
    for (module, tally) in &modules {
        let count = covered(tally);
        println!("{module}: {} / {count}", tally.functions.len());
        functions += tally.functions.len();
        examples += count;
        if let Some(floor) = floors.get(module.as_str()).filter(|floor| count < **floor) {
            below.push(format!(
                "{module}: {count} functions with an example, the floor is {floor}"
            ));
        }
    }
    println!("total: {functions} / {examples}");
    let missing: Vec<&String> = modules
        .keys()
        .filter(|module| !floors.contains_key(module.as_str()))
        .collect();
    let extra: Vec<&&str> = floors
        .keys()
        .filter(|module| !modules.contains_key(**module))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "FLOORS must list every std module: missing {missing:?}, not in std {extra:?}"
    );
    let names: Vec<&str> = FLOORS.iter().map(|(name, _)| *name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "FLOORS is in alphabetical order");
    assert!(below.is_empty(), "{}", below.join("\n"));
}

#[test]
fn exemptions_stay_below_one_fifth_of_the_public_functions() {
    let modules = tally();
    let exempt: usize = EXEMPT.iter().map(|(_, names, _)| names.len()).sum();
    let builders: usize = modules.values().map(|tally| tally.builders).sum();
    let public: usize = modules
        .values()
        .map(|tally| tally.functions.len() + tally.builders)
        .sum::<usize>()
        + exempt;
    assert!(
        (exempt + builders) * 5 < public,
        "{exempt} exempt and {builders} builder functions of {public} public ones"
    );
    for (module, names, _) in EXEMPT {
        let tally = &modules[*module];
        for name in *names {
            assert!(
                !tally.functions.iter().any(|function| function == name),
                "{module}.{name} is exempt and still counted"
            );
        }
    }
}

#[test]
fn every_std_example_has_valid_notation() {
    // `tally` panics with the notation errors of a source.
    let modules = tally();
    assert_eq!(modules.len(), SOURCES.len());
}

#[test]
fn builder_modules_show_a_computation_expression() {
    let modules = tally();
    for (module, floor) in BUILDER_FLOORS {
        let tally = &modules[*module];
        assert!(
            tally.builders > 0,
            "{module} has no builder operation; remove it from BUILDER_FLOORS"
        );
        assert!(
            tally.bind_examples >= *floor,
            "{module}.Bind has {} examples, the floor is {floor}",
            tally.bind_examples
        );
    }
}

#[test]
fn every_public_std_function_has_an_example() {
    let modules = tally();
    let missing: Vec<String> = modules
        .iter()
        .flat_map(|(module, tally)| {
            tally
                .functions
                .iter()
                .filter(|name| !tally.with_example.contains(*name))
                .map(move |name| format!("{module}.{name}"))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "{} public std functions have no example (add one in the form of \
         _tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md): {}",
        missing.len(),
        missing.join(", ")
    );
}
