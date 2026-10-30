//! The structured tags of the std documentation (G35): every public function of a module in
//! `TAGGED` states its cost with `@complexity`, `@since` names a released version, and `@see` names
//! an existing std module or public function. `scripts/gen-docs.mjs` builds the complexity tables of
//! the reference from the same tags and checks their grammar.

use std::collections::BTreeSet;

use tsuzuri::parser;
use tsuzuri::stdlib::SOURCES;
use tsuzuri::syntax::{Documentation, Edition, FunctionDecl, Visibility};

/// The modules whose public functions all carry `@complexity`. The list only grows.
const TAGGED: [&str; 8] = [
    "Array", "Vec", "Map", "Set", "HashMap", "HashSet", "List", "String",
];

fn module_of(path: &str) -> &str {
    path.trim_start_matches("std/")
        .split('.')
        .next()
        .expect("a std path has a module name")
}

/// (module, public functions) of every std source.
fn public_functions() -> Vec<(&'static str, Vec<FunctionDecl>)> {
    SOURCES
        .iter()
        .enumerate()
        .map(|(index, (path, text))| {
            let program = parser::parse_with_source(text, index, Edition::LATEST)
                .unwrap_or_else(|error| panic!("{path}: {}", error.message));
            let functions = program
                .functions
                .into_iter()
                .filter(|function| function.visibility == Visibility::Public)
                .collect();
            (module_of(path), functions)
        })
        .collect()
}

fn version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    });
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

#[test]
fn tagged_modules_have_a_complexity_on_every_public_function() {
    let modules = public_functions();
    let mut missing = Vec::new();
    for module in TAGGED {
        let (_, functions) = modules
            .iter()
            .find(|(name, _)| *name == module)
            .unwrap_or_else(|| panic!("TAGGED names {module}, which is not a std module"));
        for function in functions {
            let complexity = function.doc.as_ref().and_then(Documentation::complexity);
            if complexity.is_none_or(str::is_empty) {
                missing.push(format!("{module}.{}", function.name.text));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "public functions without @complexity: {}",
        missing.join(", ")
    );
}

#[test]
fn since_tags_are_versions_not_newer_than_the_crate() {
    let crate_version = version(env!("CARGO_PKG_VERSION")).expect("the crate version");
    for (module, functions) in public_functions() {
        for function in functions {
            let Some(since) = function.doc.as_ref().and_then(Documentation::since) else {
                continue;
            };
            let parsed = version(since).unwrap_or_else(|| {
                panic!(
                    "{module}.{}: @since {since:?} is not X.Y.Z",
                    function.name.text
                )
            });
            assert!(
                parsed <= crate_version,
                "{module}.{}: @since {since} is newer than the compiler",
                function.name.text
            );
        }
    }
}

#[test]
fn see_tags_name_existing_functions() {
    let modules = public_functions();
    let known_modules: BTreeSet<&str> = modules.iter().map(|(module, _)| *module).collect();
    let known: BTreeSet<String> = modules
        .iter()
        .flat_map(|(module, functions)| {
            functions
                .iter()
                .map(move |function| format!("{module}.{}", function.name.text))
        })
        .collect();
    let mut broken = Vec::new();
    for (module, functions) in &modules {
        for function in functions {
            let Some(doc) = &function.doc else { continue };
            for target in doc.see() {
                if !known.contains(target) && !known_modules.contains(target) {
                    broken.push(format!("{module}.{}: @see {target}", function.name.text));
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "unknown @see targets: {}",
        broken.join(", ")
    );
}
