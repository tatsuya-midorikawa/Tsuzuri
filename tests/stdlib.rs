use std::collections::BTreeSet;

use tsuzuri::check::{CheckedModule, ModuleOrigin, Provenance};
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::{analyze, analyze_modules, analyze_modules_with_std, llvm};

const USER: &[(&str, &str)] = &[("Main", "export def answer :: i64\nfn answer = 42")];

fn accepts(sources: &[(&str, &str)], std: &[(&str, &str)]) -> CheckedModule {
    analyze_modules_with_std(sources, std)
        .unwrap_or_else(|error| panic!("{sources:?}\n{}: {}", error.code, error.message))
}

fn rejects(
    sources: &[(&str, &str)],
    std: &[(&str, &str)],
    code: &str,
    message: &str,
) -> Diagnostic {
    let error = analyze_modules_with_std(sources, std).expect_err(&format!("{sources:?}"));
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{sources:?}\n{}",
        error.message
    );
    error
}

/// Native library IR; the IR of both targets must be deterministic with
/// deduplicated declarations.
fn library(module: &CheckedModule) -> String {
    let [native, _] = [false, true].map(|wasm| {
        let ir = llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(module, llvm::Entry::Library, wasm).unwrap()
        );
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: BTreeSet<_> = declarations.iter().collect();
        assert_eq!(unique.len(), declarations.len(), "{ir}");
        ir
    });
    native
}

#[test]
fn loads_the_embedded_standard_library_everywhere() {
    for module in [
        analyze("Math.zero()").unwrap(),
        analyze_modules(&[("Main.tz", "Math.zero()")]).unwrap(),
    ] {
        assert!(
            module
                .functions
                .iter()
                .any(|function| function.origin.module == ModuleOrigin::Std
                    && function.module == "Math"
                    && function.name == "zero")
        );
    }
    // Only reachable std functions reach the IR, private helpers included.
    let ir = library(&analyze("export def answer :: f64\nfn answer = Math.zero()").unwrap());
    assert!(
        ir.contains("define internal double @tz.fn.Math.zero("),
        "{ir}"
    );
    assert!(
        ir.contains("define internal double @tz.fn.Math.identity_f64("),
        "{ir}"
    );
    let ir = library(&analyze("42").unwrap());
    assert!(!ir.contains("Math"), "{ir}");
    assert!(!ir.contains("@tz.builtin."), "{ir}");
    // An empty custom library replaces the embedded one.
    let module = accepts(USER, &[]);
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.origin.module == ModuleOrigin::User)
    );
    rejects(&[("Main", "Math.zero()")], &[], "E1002", "");
}

#[test]
fn keeps_user_ir_independent_of_unused_std_code() {
    let sources = &[
        (
            "Main",
            "record Point { x: i64, y: i64 }\n\
             def norm :: Point -> i64\nfn norm p = p.x * p.x + p.y * p.y\n\
             export def root :: f64 -> f64\nfn root x = sqrt x\n\
             export def answer :: i64\nfn answer = norm (Point { x: 3, y: 4 }) + Other.value()",
        ),
        ("Other", "def value :: i64\nfn value = 17"),
    ];
    let without = library(&accepts(sources, &[]));
    assert_eq!(library(&analyze_modules(sources).unwrap()), without);
    let custom = [(
        "std/Extra.tz",
        "record Box { value: i64 }\nunion Flag = On | Off\n\
         def make :: i64 -> Box\nfn make value = Box { value: value }",
    )];
    assert_eq!(library(&accepts(sources, &custom)), without);
}

#[test]
fn injects_custom_std_sources_after_user_sources() {
    let std = [("std/Custom.tz", "def value :: i64\nfn value = 1")];
    let module = accepts(
        &[(
            "Main",
            "export def answer :: i64\nfn answer = Custom.value() + 41",
        )],
        &std,
    );
    assert!(library(&module).contains("@tz.fn.Custom.value"));
    // Std sources are type-checked even when unused, and their spans index
    // the inputs in order: user sources first.
    let error = rejects(
        &[("Main", "0"), ("Other", "def x :: i64\nfn x = 0")],
        &[("std/Broken.tz", "def broken :: i64\nfn broken = false")],
        "E1003",
        "",
    );
    assert_eq!(error.span.source, Some(2));
    // Std private helpers serve std code only.
    rejects(
        &[("Main", "Math.identity_f64 1.0")],
        tsuzuri::stdlib::SOURCES,
        "E1022",
        "",
    );
    rejects(
        &[("Main", "Int.missing 1")],
        tsuzuri::stdlib::SOURCES,
        "E1002",
        "",
    );
}

#[test]
fn prefers_user_declarations_and_hides_them_from_std() {
    let std = [
        (
            "std/Custom.tz",
            "record Point { flag: bool }\n\
             def flagged :: Point\nfn flagged = Point { flag: true }\n\
             instance Pretty<bool> {\n    fn pretty value = value\n}",
        ),
        (
            "std/Show.tt",
            "class Pretty<'a> {\n    def pretty :: 'a -> bool\n}",
        ),
    ];
    let user = [
        (
            "Main",
            "record Local { y: i64 }\n\
             instance Pretty<i64> {\n    fn pretty value = value\n}\n\
             export def answer :: i64\nfn answer = {\n    let p = Point { x: 41 };\n    p.x + 1\n}",
        ),
        ("Geometry", "record Point { x: i64 }"),
        (
            "Traits.tt",
            "class Pretty<'a> {\n    def pretty :: 'a -> i64\n}",
        ),
    ];
    accepts(&user, &std);
    // Without a user declaration, the std one is found.
    accepts(&[("Main", "let p = Point { flag: false }\n0")], &std);
    // Std code cannot see user records, unions, classes, or functions.
    for (source, code) in [
        ("def origin :: Local\nfn origin = Local { y: 0 }", "E1004"),
        (
            "def origin :: Geometry.Point\nfn origin = Geometry.Point { x: 0 }",
            "E1004",
        ),
        (
            "instance Traits.Pretty<string> {\n    fn pretty value = 0\n}",
            "E1016",
        ),
        (
            "instance Pretty<string> {\n    fn pretty value = 0\n}",
            "E1016",
        ),
        ("def value :: i64\nfn value = Helpers.value()", "E1002"),
    ] {
        let std = [("std/Probe.tz", source)];
        rejects(
            &[
                ("Main", "record Local { y: i64 }\n0"),
                ("Geometry", "record Point { x: i64 }"),
                (
                    "Traits.tt",
                    "class Pretty<'a> {\n    def pretty :: 'a -> i64\n}",
                ),
                ("Helpers", "def value :: i64\nfn value = 1"),
            ],
            &std,
            code,
            "",
        );
    }
    // Std modules name each other's types with qualification.
    accepts(
        &[(
            "Main",
            "export def answer :: i64\nfn answer = (Custom.square 3).side",
        )],
        &[
            ("std/Shapes.tz", "record Square { side: i64 }"),
            (
                "std/Custom.tz",
                "def square :: i64 -> Shapes.Square\nfn square side = Shapes.Square { side: side }",
            ),
        ],
    );
}

#[test]
fn rejects_reserved_modules_and_invalid_std_sources() {
    for (path, source) in [
        ("Option", ""),
        ("Option.tz", ""),
        ("Int.tz", "def f :: i64\nfn f = 1"),
        ("Debug.tc", ""),
        ("Result.tz", ""),
        ("Utf8String.tz", ""),
    ] {
        for sources in [vec![(path, source)], vec![("Main.tz", "0"), (path, source)]] {
            let error = rejects(&sources, &[], "E1011", "reserved for the standard library");
            assert_eq!(error.span.source, Some(sources.len() - 1), "{path}");
        }
    }
    // Functions may still use reserved module names.
    accepts(
        &[(
            "Main",
            "def Int :: i64\nfn Int = 1\nexport def answer :: i64\nfn answer = Int()",
        )],
        &[],
    );
    for path in ["std/Nested/Bad.tz", "Math.tz", "std/Bad"] {
        rejects(
            USER,
            &[(path, "def f :: i64\nfn f = 1")],
            "E1011",
            "invalid standard library path",
        );
    }
    rejects(
        USER,
        &[("std/Custom.tz", "export def f :: i64\nfn f = 1")],
        "E1018",
        "the standard library cannot export functions",
    );
    rejects(
        &[("Main", "0"), ("Custom.tz", "")],
        &[("std/Custom.tz", "")],
        "E1011",
        "",
    );
}

#[test]
fn prunes_unused_std_functions_types_and_helpers() {
    let std = [
        (
            "std/Unused.tz",
            "record Box { value: i64 }\nunion Flag = On | Off\n\
             def make :: i64 -> Box\nfn make value = Box { value: value }\n\
             def flag :: bool -> Flag\nfn flag b = if b then On else Off\n\
             def root :: f64 -> f64\nfn root x = sqrt x\n\
             def never :: unit -> i64\nfn never u = unreachable u\n\
             def adder :: i64 -> i64 -> i64\nfn adder x = {\n    let add = y -> x + y;\n    add\n}",
        ),
        (
            "std/Used.tz",
            "record Pair { left: i64, right: i64 }\n\
             def sum :: i64 -> i64\nfn sum x = {\n    let pair = Pair { left: x, right: 1 };\n    pair.left + pair.right\n}\n\
             def adder :: i64 -> i64 -> i64\nfn adder x = {\n    let add = y -> x + y;\n    add\n}",
        ),
    ];
    let module = accepts(
        &[(
            "Main",
            "export def answer :: i64\nfn answer = Used.sum 20 + Used.adder 10 11",
        )],
        &std,
    );
    let ir = library(&module);
    for present in [
        "@tz.fn.Used.sum(",
        "@tz.fn.Used.adder(",
        "tz.record.Used.Pair",
    ] {
        assert!(ir.contains(present), "{present}\n{ir}");
    }
    for absent in ["Unused", "@tz.builtin.", "Flag"] {
        assert!(!ir.contains(absent), "{absent}\n{ir}");
    }
    // Generated helpers inherit their owner's module origin and name it as
    // their parent.
    let generated: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.origin.provenance == Provenance::Generated)
        .collect();
    assert!(!generated.is_empty());
    for function in &generated {
        let parent = function
            .origin
            .parent
            .expect("generated helpers have a parent");
        assert_eq!(
            function.origin.module, module.functions[parent].origin.module,
            "{}",
            function.name
        );
    }
    assert!(
        generated
            .iter()
            .any(|function| function.origin.module == ModuleOrigin::Std)
    );
}

#[test]
fn keeps_builtins_and_task_functions_working() {
    let module = analyze(
        "export def answer :: i64\nfn answer = {\n    assert (sqrt 4.0 == 2.0);\n    \
         let s = \"text\";\n    let t = clone_string ref s;\n    \
         Task.run (task { 20 }) + (Task.run (Task.parallel [task { 22 }]))[0] + t.length - 4\n}",
    )
    .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
    let ir = library(&module);
    for symbol in [
        "@tz.builtin.sqrt",
        "@tz.builtin.assert",
        "@tz.builtin.clone_string",
    ] {
        assert!(ir.contains(symbol), "{symbol}\n{ir}");
    }
}
