//! Standard library sources embedded in the compiler (GUIDE D-07).

/// Embedded std sources as `(virtual path, text)` in load order. Programs
/// always see them after the user sources.
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Arena.tz", include_str!("../std/Arena.tz")),
    ("std/Array.tz", include_str!("../std/Array.tz")),
    ("std/BigInt.tz", include_str!("../std/BigInt.tz")),
    ("std/Cbor.tz", include_str!("../std/Cbor.tz")),
    ("std/Char.tz", include_str!("../std/Char.tz")),
    ("std/Debug.tz", include_str!("../std/Debug.tz")),
    ("std/Dir.tz", include_str!("../std/Dir.tz")),
    ("std/Env.tz", include_str!("../std/Env.tz")),
    ("std/Exception.tz", include_str!("../std/Exception.tz")),
    ("std/File.tz", include_str!("../std/File.tz")),
    ("std/Format.tz", include_str!("../std/Format.tz")),
    ("std/Gpu.tz", include_str!("../std/Gpu.tz")),
    ("std/HashMap.tz", include_str!("../std/HashMap.tz")),
    ("std/HashSet.tz", include_str!("../std/HashSet.tz")),
    ("std/IO.tc", include_str!("../std/IO.tc")),
    ("std/Json.tz", include_str!("../std/Json.tz")),
    ("std/List.tz", include_str!("../std/List.tz")),
    ("std/Map.tz", include_str!("../std/Map.tz")),
    ("std/Math.tz", include_str!("../std/Math.tz")),
    ("std/Maybe.tc", include_str!("../std/Maybe.tc")),
    ("std/Os.tz", include_str!("../std/Os.tz")),
    ("std/Owned.tz", include_str!("../std/Owned.tz")),
    ("std/Parallel.tz", include_str!("../std/Parallel.tz")),
    ("std/Process.tz", include_str!("../std/Process.tz")),
    ("std/Path.tz", include_str!("../std/Path.tz")),
    ("std/Random.tz", include_str!("../std/Random.tz")),
    ("std/Regex.tz", include_str!("../std/Regex.tz")),
    ("std/Result.tc", include_str!("../std/Result.tc")),
    ("std/Seq.tz", include_str!("../std/Seq.tz")),
    ("std/Set.tz", include_str!("../std/Set.tz")),
    ("std/String.tz", include_str!("../std/String.tz")),
    ("std/Test.tz", include_str!("../std/Test.tz")),
    ("std/Time.tz", include_str!("../std/Time.tz")),
    ("std/Unicode.tz", include_str!("../std/Unicode.tz")),
    ("std/Utf8Char.tz", include_str!("../std/Utf8Char.tz")),
    ("std/Utf8String.tz", include_str!("../std/Utf8String.tz")),
    ("std/Vec.tz", include_str!("../std/Vec.tz")),
];

/// Module names reserved for the standard library, whether or not a source
/// ships yet; user files cannot use them as module stems.
pub const RESERVED_MODULES: &[&str] = &[
    "Maybe",
    "Result",
    "Array",
    "List",
    "Vec",
    "String",
    "Utf8String",
    "Char",
    "Utf8Char",
    "Math",
    "Int",
    "Debug",
    "Parallel",
    "Simd",
    "Map",
    "Set",
    "HashMap",
    "HashSet",
    "Seq",
    "Test",
    "Gpu",
    "IO",
    "Owned",
    "File",
    "Dir",
    "Path",
    "Env",
    "Time",
    "Random",
    "Os",
    "Process",
    "Format",
    "Exception",
    "BigInt",
    "FixedArray",
    "Dyn",
    "Arena",
    "Rc",
    "Arc",
    "Regex",
    "Unicode",
    "Json",
    "Cbor",
];

/// The namespace of every std module, as `std::Maybe`. User code cannot
/// declare modules in it.
pub const NAMESPACE: &str = "std";

pub fn is_reserved_module(name: &str) -> bool {
    RESERVED_MODULES.contains(&name)
}

pub(crate) fn opaque_record(name: &str) -> bool {
    matches!(
        name,
        "Map.Map"
            | "Map.Entry"
            | "Set.Set"
            | "HashMap.HashMap"
            | "HashMap.Entry"
            | "HashSet.HashSet"
            | "Seq.Seq"
            | "Gpu.Device"
            | "Gpu.Buffer"
            | "IO.IO"
            | "Owned.Function"
            | "Random.Pcg"
            | "File.Handle"
            | "BigInt.BigInt"
            | "Arena.Arena"
            | "Arena.Handle"
            | "Arena.Slot"
            | "Regex.Regex"
            | "Json.Reader"
            | "Json.Writer"
    )
}

/// The module name of a flat std source path such as `std/Maybe.tc`, or
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
        assert_eq!(module_name("std/Maybe.tc"), Some("Maybe"));
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
        assert_eq!(RESERVED_MODULES.len(), 43);
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
    fn io_actions_are_opaque_and_composable() {
        let module = crate::analyze(
            "let action = IO { let! value = IO.pure 40; do! IO.pure (); return value + 2 }\n()",
        )
        .unwrap_or_else(|error| panic!("{}", error.message));
        crate::llvm::emit(&module, crate::llvm::Entry::Console).unwrap();
        let error = crate::analyze("let action = IO.pure 42\nTask.run action.work").unwrap_err();
        assert!(error.message.contains("opaque"), "{}", error.message);
        for source in [
            "IO.__read_line ()",
            "let call = IO.__write\n()",
            "IO { work: \\() -> 42 }",
        ] {
            assert_eq!(
                crate::analyze(source).unwrap_err().code,
                "E1022",
                "{source}"
            );
        }
        for source in [
            "IO { let! line = IO.read_line (); do! IO.write_line (Maybe.default_value \"\" line) }",
            "IO { for number in [1, 2] do do! IO.write_line number }",
            "IO { while false do do! IO.write_line 1 }",
            "IO {\n    let! left = IO.pure 20\n    and! right = IO.pure 22\n    return left + right\n}",
        ] {
            let module =
                crate::analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
            for wasm in [false, true] {
                let ir =
                    crate::llvm::emit_target(&module, crate::llvm::Entry::Library, wasm).unwrap();
                if crate::llvm::io_entry(&module) {
                    assert!(ir.contains("define i32 @tsuzuri_main()"));
                    assert!(!ir.contains("define i32 @main()"));
                    assert!(!ir.contains("@tz.console.write"));
                }
            }
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
