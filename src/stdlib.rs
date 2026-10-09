//! Standard library sources embedded in the compiler (GUIDE D-07).

/// Embedded std sources as `(virtual path, text)` in load order. Programs
/// always see them after the user sources.
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Arena.tz", include_str!("../std/Arena.tz")),
    ("std/Array.tz", include_str!("../std/Array.tz")),
    ("std/Async.tc", include_str!("../std/Async.tc")),
    ("std/Bench.tz", include_str!("../std/Bench.tz")),
    ("std/BigInt.tz", include_str!("../std/BigInt.tz")),
    ("std/Cbor.tz", include_str!("../std/Cbor.tz")),
    ("std/Char.tz", include_str!("../std/Char.tz")),
    ("std/Debug.tz", include_str!("../std/Debug.tz")),
    ("std/Dir.tz", include_str!("../std/Dir.tz")),
    ("std/Env.tz", include_str!("../std/Env.tz")),
    ("std/Exception.tz", include_str!("../std/Exception.tz")),
    ("std/File.tz", include_str!("../std/File.tz")),
    ("std/Format.tz", include_str!("../std/Format.tz")),
    ("std/Gen.tz", include_str!("../std/Gen.tz")),
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
    "Bench",
    "Gen",
    "Async",
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
            | "Gen.Gen"
            | "Async.Async"
            | "Async.Next"
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

/// A std module that only a program naming it can reach (D-40). Commands that
/// build or check a program load it only when a user source contains one of
/// `names`, so programs that do not use it neither type-check nor emit it.
/// User code names its declarations only qualified (`Json.Value`, `Arena.Handle`).
pub(crate) struct OptIn {
    pub module: &'static str,
    /// The identifiers that can refer to the module from user code: its name and
    /// the builtin classes it gives instances for types it does not declare.
    /// `opt_in_modules_are_reached_only_through_their_names` keeps the list complete.
    pub names: &'static [&'static str],
    /// The opt-in modules that its own source refers to.
    pub uses: &'static [&'static str],
}

pub(crate) const OPT_IN: &[OptIn] = &[
    OptIn {
        module: "Arena",
        names: &["Arena"],
        uses: &[],
    },
    OptIn {
        module: "Regex",
        names: &["Regex"],
        uses: &["Unicode"],
    },
    OptIn {
        module: "Unicode",
        names: &["Unicode"],
        uses: &[],
    },
    OptIn {
        module: "Json",
        names: &["Json", "Encode", "Decode"],
        uses: &[],
    },
    OptIn {
        module: "Cbor",
        names: &["Cbor"],
        uses: &["Json"],
    },
    OptIn {
        module: "Bench",
        names: &["Bench"],
        uses: &[],
    },
    OptIn {
        module: "Gen",
        names: &["Gen"],
        uses: &[],
    },
    OptIn {
        module: "Async",
        names: &["Async"],
        uses: &[],
    },
];

/// Whether `module` is an opt-in std module, whose declarations user code names
/// only qualified (D-40).
pub(crate) fn is_opt_in(module: &str) -> bool {
    OPT_IN.iter().any(|opt_in| opt_in.module == module)
}

/// The embedded std sources that a program made of `texts` (its user sources)
/// can reach, in load order: every module that is not opt-in, and the opt-in
/// modules that the texts name directly or through another opt-in module.
pub fn sources_for<'a>(
    texts: impl IntoIterator<Item = &'a str>,
) -> Vec<(&'static str, &'static str)> {
    let mut words = std::collections::BTreeSet::new();
    for text in texts {
        let bytes = text.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            // Every identifier token is a maximal run of these bytes, or the part of a
            // run after leading digits; scanning both over-approximates the tokens.
            if bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' {
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if start < index && bytes[start].is_ascii_uppercase() {
                    words.insert(&text[start..index]);
                }
            } else {
                index += 1;
            }
        }
    }
    let mut needed: Vec<&str> = OPT_IN
        .iter()
        .filter(|module| module.names.iter().any(|name| words.contains(name)))
        .map(|module| module.module)
        .collect();
    let mut next = 0;
    while next < needed.len() {
        let module = OPT_IN
            .iter()
            .find(|module| module.module == needed[next])
            .expect("opt-in modules use opt-in modules");
        for used in module.uses {
            if !needed.contains(used) {
                needed.push(used);
            }
        }
        next += 1;
    }
    SOURCES
        .iter()
        .filter(|(path, _)| {
            module_name(path).is_none_or(|name| {
                !OPT_IN.iter().any(|module| module.module == name) || needed.contains(&name)
            })
        })
        .copied()
        .collect()
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

    /// The public names of `source` by namespace: types, union cases, classes,
    /// and active-pattern cases, the names an unqualified use can resolve to.
    fn public_names(source: &str) -> Vec<(&'static str, String)> {
        use crate::syntax::Visibility;
        let program = crate::parser::parse(source).expect("std sources parse");
        let mut names = Vec::new();
        for record in &program.records {
            if record.visibility == Visibility::Public {
                names.push(("type", record.name.text.clone()));
            }
        }
        for union in &program.unions {
            if union.visibility == Visibility::Public {
                names.push(("type", union.name.text.clone()));
                for case in &union.cases {
                    names.push(("case", case.name.text.clone()));
                }
            }
        }
        for alias in &program.type_aliases {
            if alias.visibility == Visibility::Public {
                names.push(("type", alias.name.text.clone()));
            }
        }
        for handle in &program.extern_types {
            if handle.visibility == Visibility::Public {
                names.push(("type", handle.name.text.clone()));
            }
        }
        for class in &program.classes {
            names.push(("class", class.name.text.clone()));
        }
        for pattern in &program.active_patterns {
            for case in &pattern.cases {
                names.push(("pattern", case.text.clone()));
            }
        }
        names
    }

    /// Whether `source` has the identifier token `word` (comments and strings do not
    /// count), anywhere or only where it is not a member after `.` or `::`.
    fn mentions_as(source: &str, word: &str, unqualified: bool) -> bool {
        use crate::syntax::TokenKind;
        let tokens = crate::lexer::lex(source).expect("std sources lex");
        tokens.iter().enumerate().any(|(index, token)| {
            matches!(&token.kind, TokenKind::Ident(name) if name == word)
                && !(unqualified
                    && index > 0
                    && matches!(
                        tokens[index - 1].kind,
                        TokenKind::Dot | TokenKind::DoubleColon
                    ))
        })
    }

    fn mentions(source: &str, word: &str) -> bool {
        mentions_as(source, word, false)
    }

    #[test]
    fn opt_in_modules_are_reached_only_through_their_names() {
        for (path, source) in SOURCES {
            let name = module_name(path).unwrap();
            if !is_opt_in(name) {
                // An always-loaded module must not need an opt-in module.
                for module in OPT_IN {
                    assert!(
                        !mentions(source, module.module),
                        "{path} names {}",
                        module.module
                    );
                }
            }
        }
        for module in OPT_IN {
            let (path, source) = SOURCES
                .iter()
                .find(|(path, _)| module_name(path) == Some(module.module))
                .unwrap();
            assert!(module.names.contains(&module.module), "{path}");
            // Instances for types declared elsewhere are reached through their class.
            let program = crate::parser::parse(source).unwrap();
            for instance in &program.instances {
                use crate::syntax::TypeExprKind;
                let head = match &instance.ty.kind {
                    TypeExprKind::Named(name) => Some(name.as_str()),
                    TypeExprKind::Apply(name, _) => Some(name.text.as_str()),
                    _ => None,
                };
                let own = head.is_some_and(|head| {
                    let head = head
                        .strip_prefix(&format!("{}.", module.module))
                        .unwrap_or(head);
                    program
                        .records
                        .iter()
                        .any(|record| record.name.text == head)
                        || program.unions.iter().any(|union| union.name.text == head)
                        || program
                            .type_aliases
                            .iter()
                            .any(|alias| alias.name.text == head)
                });
                assert!(
                    own || module.names.contains(&instance.class.text.as_str()),
                    "{path}: instance {} is for a type it does not declare; add '{}' to its OptIn names",
                    instance.class.text,
                    instance.class.text
                );
            }
            // The modules it refers to, by name or through a bare std name that only
            // the other module declares, are exactly its `uses`.
            let declared = public_names(source);
            for other in OPT_IN {
                if other.module != module.module {
                    let (_, other_source) = SOURCES
                        .iter()
                        .find(|(path, _)| module_name(path) == Some(other.module))
                        .unwrap();
                    let named = mentions(source, other.module)
                        || public_names(other_source).iter().any(|(_, name)| {
                            mentions_as(source, name, true)
                                && !declared.iter().any(|(_, declared)| declared == name)
                        });
                    assert_eq!(
                        named,
                        module.uses.contains(&other.module),
                        "{path} and {}",
                        other.module
                    );
                }
            }
        }
    }

    #[test]
    fn every_std_module_type_checks_when_named() {
        // A comment that names every opt-in module loads all of them.
        let names: Vec<&str> = OPT_IN.iter().map(|module| module.module).collect();
        let source = format!("// {}\n0\n", names.join(" "));
        assert_eq!(sources_for([source.as_str()]), SOURCES.to_vec());
        crate::analyze_modules(&[("Main.tz", &source)])
            .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    }

    #[test]
    fn user_code_names_opt_in_declarations_only_qualified() {
        let check = |source: &str| crate::analyze_modules(&[("Main.tz", source)]);
        // Qualified names work.
        check("export def f :: i64\nfn f =\n    match Json.Null with\n    | Json.Null -> 1\n    | _ -> 0\n")
            .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
        // A bare case or type of an opt-in module does not resolve, even when it is loaded.
        let error = check("export def f :: i64\nfn f =\n    let value: Json.Value = Null\n    match value with\n    | Json.Null -> 1\n    | _ -> 0\n")
            .unwrap_err();
        assert!(
            matches!(error.code, "E1002" | "E1004"),
            "{}: {}",
            error.code,
            error.message
        );
        let error = check("def f :: Category -> i64\nfn f _ = 1\n// Unicode\n0\n").unwrap_err();
        assert_eq!(error.code, "E1004", "{}", error.message);
        // So loading them never makes a bare always-loaded name ambiguous.
        check("def kind :: ErrorKind -> i64\nfn kind _ = 1\nlet uses = \"Json Regex\"\n0\n")
            .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
        check("def size :: ref Handle -> i64\nfn size _ = 1\n// Arena\n0\n")
            .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    }

    #[test]
    fn sources_for_loads_named_opt_in_modules_and_their_uses() {
        let loaded = |text: &str| -> Vec<&str> {
            sources_for([text])
                .iter()
                .filter_map(|(path, _)| module_name(path))
                .filter(|name| OPT_IN.iter().any(|module| module.module == *name))
                .collect()
        };
        let always = SOURCES.len() - OPT_IN.len();
        assert_eq!(sources_for(["42\n"]).len(), always);
        assert_eq!(loaded("42\n"), Vec::<&str>::new());
        // A word inside a longer identifier is not a mention.
        assert_eq!(
            loaded("let JsonText = 1\nlet regex_count = Arenas"),
            Vec::<&str>::new()
        );
        assert_eq!(loaded("Regex.compile (ref text)"), ["Regex", "Unicode"]);
        assert_eq!(loaded("x |> Cbor.encode"), ["Cbor", "Json"]);
        assert_eq!(loaded("record P { x: i64 } deriving (Encode)"), ["Json"]);
        assert_eq!(loaded("let a: Arena<i64> = Arena.empty()"), ["Arena"]);
        // Bare names of opt-in declarations never resolve to them, so they load nothing.
        assert_eq!(
            loaded("match c with | Lu -> 1 | Null -> 0 | _ -> 2"),
            Vec::<&str>::new()
        );
        assert_eq!(loaded("let x = 1Regex"), ["Regex", "Unicode"]);
        assert_eq!(loaded("42"), Vec::<&str>::new());
        // The load order is the embedded order.
        let all = sources_for(OPT_IN.iter().map(|module| module.module));
        assert_eq!(all, SOURCES.to_vec());
    }

    #[test]
    fn reserves_the_d07_table() {
        assert_eq!(RESERVED_MODULES.len(), 46);
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
