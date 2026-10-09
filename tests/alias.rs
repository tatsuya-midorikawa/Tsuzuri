//! D-43: `@alias name` in a `.tc` builder file names the builder in `name { ... }`.

use tsuzuri::check::ModuleOrigin;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::{analyze_modules, analyze_modules_with_semantics, llvm};

/// A builder that passes values through, declaring `header` before its operations.
fn builder(header: &str) -> String {
    format!(
        "{header}\n\ndef Return :: 'a -> 'a\nfn Return value = value\n\ndef ReturnFrom :: 'a -> 'a\nfn ReturnFrom value = value\n\ndef Bind :: 'a -> ('a -> 'b) -> 'b\nfn Bind value next = next value\n\ndef Zero :: unit\nfn Zero = ()\n"
    )
}

fn ir(sources: &[(&str, &str)]) -> String {
    let module = analyze_modules(sources)
        .unwrap_or_else(|error| panic!("{sources:?}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

fn rejected(sources: &[(&str, &str)], code: &str) -> Diagnostic {
    let error = analyze_modules(sources).expect_err(&format!("{sources:?}"));
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    error
}

fn async_is_loaded(source: &str) -> bool {
    analyze_modules(&[("Main.tz", source)])
        .unwrap()
        .records
        .iter()
        .any(|record| record.name == "Async.Async" && record.origin == ModuleOrigin::Std)
}

#[test]
fn the_std_async_builder_answers_to_async() {
    let program = |name: &str| {
        format!(
            "export def paused :: i64 -> i64\nfn paused n = Async.run ({name} {{ do! Async.yield (); let! now = Async.now (); return now + n }})"
        )
    };
    assert_eq!(
        ir(&[("Main.tz", &program("Async"))]),
        ir(&[("Main.tz", &program("async"))]),
        "an alias is only another spelling"
    );
    // An empty body is a computation too, not a record.
    ir(&[(
        "Main.tz",
        "export def run :: i64 -> i64\nfn run n =\n    let _empty = async {}\n    Async.run (async { return n })",
    )]);
}

#[test]
fn an_alias_word_alone_loads_the_std_builder() {
    assert!(async_is_loaded(
        "export def make :: i64 -> i64\nfn make n =\n    let _unused = async { return n }\n    n"
    ));
    assert!(!async_is_loaded(
        "export def make :: i64 -> i64\nfn make n =\n    let asynchronous = n\n    asynchronous"
    ));
    // A local that carries the alias's name changes nothing: the name before `{` is a builder.
    assert!(async_is_loaded(
        "export def make :: i64 -> i64\nfn make n =\n    let async = n\n    async + 1"
    ));
}

#[test]
fn a_user_builder_declares_aliases() {
    let identity = builder("@alias identity\n@alias same");
    let program = |name: &str| {
        format!("export def one :: i64 -> i64\nfn one n = {name} {{ let! a = n; return a + 1 }}")
    };
    let canonical = ir(&[
        ("Main.tz", &program("Identity")),
        ("Identity.tc", &identity),
    ]);
    for alias in ["identity", "same"] {
        assert_eq!(
            ir(&[("Main.tz", &program(alias)), ("Identity.tc", &identity)]),
            canonical,
            "{alias}"
        );
    }
    // The empty body, `Zero` of the builder.
    ir(&[
        (
            "Main.tz",
            "export def nothing :: i64 -> i64\nfn nothing n =\n    let _unit = identity {}\n    n",
        ),
        ("Identity.tc", &identity),
    ]);
}

#[test]
fn aliases_follow_the_namespaces_that_their_builders_are_in_sight_in() {
    let parser = builder("namespace Acme::Tools\n\n@alias parse");
    let other = builder("namespace Acme::Other\n\n@alias parse");
    let main = |header: &str| {
        format!("{header}export def one :: i64 -> i64\nfn one n = parse {{ return n }}")
    };
    // Out of sight without `using`: only the builder's own name reaches it.
    let error = rejected(&[("Main.tz", &main("")), ("Parser.tc", &parser)], "E1018");
    assert_eq!(
        error.message,
        "unknown computation builder 'parse'; declare '@alias parse' in the .tc file of its builder, or write the builder's name"
    );
    ir(&[
        (
            "Main.tz",
            "export def one :: i64 -> i64\nfn one n = Acme::Tools::Parser { return n }",
        ),
        ("Parser.tc", &parser),
    ]);
    ir(&[
        ("Main.tz", &main("using Acme::Tools\n\n")),
        ("Parser.tc", &parser),
    ]);
    // Inside its namespace, with no `using`.
    ir(&[
        (
            "Tools.tz",
            "namespace Acme::Tools\n\nexport def one :: i64 -> i64\nfn one n = parse { return n }",
        ),
        ("Parser.tc", &parser),
    ]);
    // Two builders in sight with one alias: neither wins, and a builder's name chooses.
    let both = main("using Acme::Tools\nusing Acme::Other\n\n");
    let error = rejected(
        &[
            ("Main.tz", &both),
            ("Parser.tc", &parser),
            ("Other.tc", &other),
        ],
        "E1004",
    );
    assert!(
        error
            .message
            .starts_with("builder alias 'parse' is ambiguous: the builders '")
            && error.message.contains("'Acme::Tools::Parser'")
            && error.message.contains("'Acme::Other::Other'")
            && error.message.ends_with(
                "declare it; write the name of the builder you mean, or rename an alias"
            ),
        "{}",
        error.message
    );
    ir(&[
        (
            "Main.tz",
            "using Acme::Tools\nusing Acme::Other\n\nexport def one :: i64 -> i64\nfn one n = Acme::Other::Other { return n }",
        ),
        ("Parser.tc", &parser),
        ("Other.tc", &other),
    ]);
    // Builders of one namespace that share an alias cannot both be chosen by it.
    let twin = builder("@alias identity");
    let identity = builder("@alias identity");
    rejected(
        &[
            (
                "Main.tz",
                "export def one :: i64 -> i64\nfn one n = identity { return n }",
            ),
            ("Identity.tc", &identity),
            ("Twin.tc", &twin),
        ],
        "E1004",
    );
}

#[test]
fn user_aliases_take_a_name_before_std_aliases() {
    let mine = builder("@alias async");
    let program =
        |name: &str| format!("export def one :: i64 -> i64\nfn one n = {name} {{ return n + 1 }}");
    // `async` is the user's builder `Mine`, as it would be spelled; std's `Async` is not involved.
    assert_eq!(
        ir(&[("Main.tz", &program("async")), ("Mine.tc", &mine)]),
        ir(&[("Main.tz", &program("Mine")), ("Mine.tc", &mine)])
    );
    // The std builder stays reachable by its name next to it.
    ir(&[
        (
            "Main.tz",
            "export def two :: i64 -> i64\nfn two n = Async.run (Async { return n + 2 }) + (async { return n })",
        ),
        ("Mine.tc", &mine),
    ]);
}

#[test]
fn malformed_alias_declarations_are_rejected() {
    for (header, message) in [
        (
            "@alias Upper",
            "a builder alias starts with a lowercase letter, as in '@alias async'; 'Upper' would collide with module and record names",
        ),
        (
            "@alias _private",
            "a builder alias starts with a lowercase letter, as in '@alias async'",
        ),
        (
            "@alias task",
            "'@alias' takes a builder alias such as 'async': an identifier that is not a reserved word",
        ),
        (
            "@alias 5",
            "'@alias' takes a builder alias such as 'async': an identifier that is not a reserved word",
        ),
        (
            "@alias try",
            "'try' is a contextual keyword and cannot be a builder alias",
        ),
        (
            "@alias",
            "write the alias name on the same line as '@alias', as in '@alias async'",
        ),
        (
            "@alias\nmine",
            "write the alias name on the same line as '@alias', as in '@alias async'",
        ),
        (
            "@alias mine extra",
            "expected a line break after the builder alias",
        ),
        (
            "@alias mine\n@alias mine",
            "builder alias 'mine' is declared twice in this file",
        ),
        (
            "/// A doc comment\n@alias mine",
            "doc comments attach to API declarations such as 'def', not to a builder alias",
        ),
    ] {
        let source = builder(header);
        let error = rejected(&[("Main.tz", "0"), ("Mine.tc", &source)], "E0002");
        assert_eq!(error.message, message, "{header}");
    }
    // Only a builder file names a builder.
    let error = rejected(
        &[("Main.tz", "@alias mine\n\ndef one :: i64\nfn one = 1")],
        "E1018",
    );
    assert_eq!(
        error.message,
        "'@alias mine' names a computation builder, so it belongs in a .tc file; move it to the builder's .tc file or remove it"
    );
}

#[test]
fn an_unknown_lowercase_builder_points_to_aliases() {
    let error = rejected(
        &[(
            "Main.tz",
            "export def one :: i64 -> i64\nfn one n = nosuch { return n }",
        )],
        "E1018",
    );
    assert_eq!(
        error.message,
        "unknown computation builder 'nosuch'; declare '@alias nosuch' in the .tc file of its builder, or write the builder's name"
    );
    // A capitalized unknown builder keeps its message.
    let error = rejected(
        &[(
            "Main.tz",
            "export def one :: i64 -> i64\nfn one n = Nosuch { return n }",
        )],
        "E1018",
    );
    assert_eq!(
        error.message,
        "unknown computation builder 'Nosuch'; define its operations in Nosuch.tc"
    );
}

#[test]
fn the_language_server_analysis_resolves_aliases_with_all_of_std_loaded() {
    // The editor analyzes with every std module loaded, opt-in ones included.
    let source = "export def paused :: i64 -> i64\nfn paused n = Async.run (async { do! Async.yield (); return n + 1 })";
    let (module, _index) = analyze_modules_with_semantics(&[("Main.tz", source)])
        .unwrap_or_else(|error| panic!("{error:?}"));
    llvm::emit(&module, llvm::Entry::Library).unwrap();
    // The alias is the std builder's, whichever modules sit beside it.
    let mine = builder("@alias mine");
    analyze_modules_with_semantics(&[
        (
            "Main.tz",
            "export def one :: i64 -> i64\nfn one n = mine { return n }",
        ),
        ("Mine.tc", &mine),
    ])
    .unwrap_or_else(|error| panic!("{error:?}"));
}

#[test]
fn an_alias_does_not_name_a_module() {
    // `async.run` would be a module path; the alias is only a builder name.
    let error = analyze_modules(&[(
        "Main.tz",
        "export def one :: i64 -> i64\nfn one n = async.run (Async { return n })",
    )])
    .expect_err("an alias is not a module");
    assert_eq!(error.code, "E1002");
    assert_eq!(error.message, "unknown value 'async'");
}
