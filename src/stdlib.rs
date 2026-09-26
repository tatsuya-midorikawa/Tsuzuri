//! Standard library sources embedded in the compiler (GUIDE D-07).

/// Embedded std sources as `(virtual path, text)` in load order. Programs
/// always see them after the user sources.
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Math.tz", include_str!("../std/Math.tz")),
    ("std/Option.tc", include_str!("../std/Option.tc")),
    ("std/Result.tc", include_str!("../std/Result.tc")),
    ("std/String.tz", include_str!("../std/String.tz")),
    ("std/Utf8String.tz", include_str!("../std/Utf8String.tz")),
];

/// Module names reserved for the standard library, whether or not a source
/// ships yet; user files cannot use them as module stems.
pub const RESERVED_MODULES: &[&str] = &[
    "Option",
    "Result",
    "Array",
    "List",
    "Vec",
    "String",
    "Utf8String",
    "Char",
    "Math",
    "Int",
    "Debug",
    "Parallel",
    "Simd",
    "Map",
    "Set",
    "Test",
    "Gpu",
];

pub fn is_reserved_module(name: &str) -> bool {
    RESERVED_MODULES.contains(&name)
}

/// The module name of a flat std source path such as `std/Option.tc`, or
/// `None` for paths outside `std/`, nested paths, and unknown extensions.
pub fn module_name(path: &str) -> Option<&str> {
    let (stem, extension) = path.strip_prefix("std/")?.rsplit_once('.')?;
    (!stem.is_empty()
        && !stem.contains(['/', '\\', '.'])
        && crate::syntax::SourceKind::from_extension(extension).is_some())
    .then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::Builtin;

    #[test]
    fn names_flat_std_paths_only() {
        assert_eq!(module_name("std/Math.tz"), Some("Math"));
        assert_eq!(module_name("std/Option.tc"), Some("Option"));
        assert_eq!(module_name("std/Classes.tt"), Some("Classes"));
        for path in [
            "std/Nested/Bad.tz",
            "Math.tz",
            "std/Math",
            "std/Math.rs",
            "std/.tz",
            "std/A.B.tz",
            "lib/std/Math.tz",
        ] {
            assert_eq!(module_name(path), None, "{path}");
        }
    }

    #[test]
    fn reserves_the_d07_table() {
        assert_eq!(RESERVED_MODULES.len(), 17);
        assert!(RESERVED_MODULES.iter().all(|name| is_reserved_module(name)));
        assert!(!is_reserved_module("Task"));
        assert!(!is_reserved_module("Main"));
        assert!(!is_reserved_module("math"));
    }

    #[test]
    fn embedded_sources_use_reserved_flat_modules() {
        for (path, _) in SOURCES {
            let name = module_name(path).unwrap_or_else(|| panic!("invalid std path {path}"));
            assert!(is_reserved_module(name), "{path} must be a reserved module");
        }
    }

    #[test]
    fn std_definitions_never_shadow_builtin_functions() {
        for (id, (path, text)) in SOURCES.iter().enumerate() {
            let module = module_name(path).unwrap();
            let program = crate::parser::parse_with_source(text, id)
                .unwrap_or_else(|error| panic!("{path}: {}", error.message));
            for function in &program.functions {
                let qualified = format!("{module}.{}", function.name.text);
                assert!(
                    Builtin::ALL
                        .iter()
                        .all(|builtin| builtin.name() != qualified),
                    "{path} defines builtin '{qualified}'"
                );
            }
        }
    }
}
