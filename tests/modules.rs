use tsuzuri::check::{Type, TypedExprKind};
use tsuzuri::{analyze_modules, llvm};

const POINT: &str = "
record Point { x: f64, y: f64 }

fn distance(point: Point) -> f64 {
    sqrt(point.x * point.x + point.y * point.y)
}

export fn hypotenuse(x: f64, y: f64) -> f64 {
    distance(Point { x: x, y: y })
}
";

#[test]
fn loads_and_protects_local_package_graphs() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tsuzuri::{check::ModuleOrigin, driver::Project};
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-packages-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let write_package = |name: &str, dependencies: &str, file: &str, source: &str| {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(
            root.join(name).join("Tsuzuri.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n[dependencies]\n{dependencies}"
            ),
        )
        .unwrap();
        fs::write(root.join(name).join(file), source).unwrap();
    };
    write_package(
        "app",
        "geometry-core = { path = \"../geometry-core\" }\nother = { path = \"../other\" }",
        "Main.tz",
        "def main :: i64\nfn main = GeometryCore.Point.value() + Other.Library.value()",
    );
    write_package(
        "geometry-core",
        "",
        "Point.tz",
        "def value :: i64\nfn value = Option.get (Option.Some 42)\nprivate def hidden :: i64\nfn hidden = 0",
    );
    fs::write(
        root.join("geometry-core/Main.tz"),
        "def main :: i64\nfn main = 99",
    )
    .unwrap();
    write_package(
        "other",
        "geometry-core = { path = \"../geometry-core\" }",
        "Library.tz",
        "def value :: i64\nfn value = GeometryCore.Main.main() - 99",
    );
    let app = root.join("app");
    let project = Project::load(&app).unwrap();
    let module = project.analyze().unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].qualified_name(),
        "Main.main"
    );
    assert_eq!(project.manifests.len(), 3);
    assert_eq!(
        project
            .sources
            .iter()
            .filter(|source| source.origin == ModuleOrigin::User)
            .count(),
        4
    );
    assert!(
        project
            .sources
            .iter()
            .any(|source| source.name == "GeometryCore.Point"
                && source.package.as_ref().unwrap().name == "geometry-core")
    );
    let overlays = std::collections::BTreeMap::from([(
        root.join("geometry-core/Point.tz"),
        "def value :: i64\nfn value = false".to_owned(),
    )]);
    assert_eq!(
        Project::load_with_overlays(&app, &overlays)
            .unwrap()
            .analyze()
            .unwrap_err()
            .code,
        "E1003"
    );
    for wasm in [false, true] {
        assert_eq!(
            llvm::emit_target(&module, llvm::Entry::Console, wasm).unwrap(),
            llvm::emit_target(&project.analyze().unwrap(), llvm::Entry::Console, wasm).unwrap()
        );
    }
    for file in [
        "geometry-core/Point.tz",
        "geometry-core/Tsuzuri.toml",
        "app/Tsuzuri.toml",
    ] {
        let before = fs::read(root.join(file)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                app.to_str().unwrap(),
                "--emit",
                "llvm",
                "-o",
                root.join(file).to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2003"));
        assert_eq!(fs::read(root.join(file)).unwrap(), before);
    }
    fs::write(app.join("Main.tz"), "GeometryCore.Point.hidden()").unwrap();
    assert_eq!(
        Project::load(&app).unwrap().analyze().unwrap_err().code,
        "E1022"
    );
    fs::write(app.join("Main.tz"), "0").unwrap();
    fs::write(
        root.join("geometry-core/Point.tz"),
        "def broken :: i64\nfn broken = false",
    )
    .unwrap();
    let project = Project::load(&app).unwrap();
    assert_eq!(
        project.source_for(&project.analyze().unwrap_err()).path,
        fs::canonicalize(root.join("geometry-core/Point.tz")).unwrap()
    );
    write_package(
        "geometry-core",
        "app = { path = \"../app\" }",
        "Point.tz",
        "",
    );
    let error = Project::load(&app).unwrap_err();
    assert_eq!(error.diagnostic.code, "E1011");
    assert!(error.diagnostic.message.contains("cyclic"));
    write_package("geometry-core", "", "Point.tz", "");
    fs::create_dir(root.join("duplicate")).unwrap();
    fs::copy(
        root.join("geometry-core/Tsuzuri.toml"),
        root.join("duplicate/Tsuzuri.toml"),
    )
    .unwrap();
    write_package(
        "other",
        "geometry-core = { path = \"../duplicate\" }",
        "Library.tz",
        "",
    );
    assert_eq!(Project::load(&app).unwrap_err().diagnostic.code, "E1011");
    write_package("other", "", "Library.tz", "");
    fs::create_dir(app.join("GeometryCore")).unwrap();
    fs::write(app.join("GeometryCore/Local.tz"), "").unwrap();
    assert_eq!(Project::load(&app).unwrap_err().diagnostic.code, "E1011");
    fs::remove_dir_all(app.join("GeometryCore")).unwrap();
    fs::create_dir(app.join("Local")).unwrap();
    fs::write(
        app.join("Local/Tsuzuri.toml"),
        "[package]\nname = \"local\"\nversion = \"1\"\n",
    )
    .unwrap();
    fs::write(app.join("Local/Value.tz"), "def value :: i64\nfn value = 7").unwrap();
    write_package(
        "app",
        "local = { path = \"Local\" }",
        "Main.tz",
        "Local.Value.value()",
    );
    let project = Project::load(&app).unwrap();
    project.analyze().unwrap();
    assert_eq!(
        project
            .sources
            .iter()
            .filter(|source| source.name == "Local.Value")
            .count(),
        1
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("geometry-core"), root.join("link")).unwrap();
        write_package(
            "app",
            "geometry-core = { path = \"../link\" }",
            "Main.tz",
            "0",
        );
        assert_eq!(Project::load(&app).unwrap_err().diagnostic.code, "E1011");
        fs::remove_file(root.join("link")).unwrap();
        fs::remove_file(app.join("Tsuzuri.toml")).unwrap();
        std::os::unix::fs::symlink(
            root.join("geometry-core/Tsuzuri.toml"),
            app.join("Tsuzuri.toml"),
        )
        .unwrap();
        assert_eq!(Project::load(&app).unwrap_err().diagnostic.code, "E1011");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn module_paths_map_to_bounded_dotted_names() {
    let module = analyze_modules(&[("Geometry/Point.tz", "fn value() -> i64 { 42 }")]).unwrap();
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.qualified_name() == "Geometry.Point.value")
    );
    for path in [
        "../Point.tz",
        "/Point.tz",
        "Geometry.Point.tz",
        "Bad-Name/Point.tz",
        "Geometry/fn/Point.tz",
        "Geometry/_/Point.tz",
        "Option/Point.tz",
        "Geometry/Task/Point.tz",
    ] {
        assert_eq!(
            analyze_modules(&[(path, "fn value() -> i64 { 42 }")])
                .unwrap_err()
                .code,
            "E1011",
            "{path}"
        );
    }
    let deep = format!("{}Point.tz", "Nested/".repeat(16));
    assert_eq!(analyze_modules(&[(&deep, "")]).unwrap_err().code, "E1017");
    let long = format!("{}.tz", "A".repeat(256));
    assert_eq!(analyze_modules(&[(&long, "")]).unwrap_err().code, "E1017");
    assert_eq!(
        analyze_modules(&[("Geometry/Point.tz", ""), ("Geometry/Point.tt", "")])
            .unwrap_err()
            .code,
        "E1011"
    );
}

#[test]
fn hierarchical_names_resolve_functions_types_cases_classes_and_builders() {
    let main = "def use_point :: Geometry.Point.Point -> i64\nfn use_point point = Geometry.Traits.Score.score (&point)\n\
        let point = Geometry.Point.Point { x: 40 }\n\
        let value = Geometry.Builder { return use_point point }\n\
        let extra = match Geometry.Point.Payload value with | Geometry.Point.Value.Payload inner -> inner\n\
        let checked = match extra with | Geometry.Patterns.Even -> extra | _ -> 0\n\
        Geometry.Point.distance (Geometry.Point.Point { x: checked }) + Geometry.Point.Offset";
    let module = analyze_modules(&[
        ("Geometry/Point.tz", "record Point { x: i64 }\nunion Value = Payload of i64\nconst Offset: i64 = 2\nfn distance(point: Point) -> i64 { point.x }\ninstance Geometry.Traits.Score<Point> { fn score point = point.x }"),
        ("Geometry/Traits.tt", "class Score<'a> { def score :: &'a -> i64 }"),
        ("Geometry/Builder.tc", "def Return :: 'a -> 'a\nfn Return value = value"),
        ("Geometry/Patterns.tz", "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) value = value % 2 == 0"),
        ("Main.tz", main),
    ]).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Console, wasm).unwrap();
        assert!(ir.contains("@tz.fn.Geometry.Point.distance"));
        assert!(ir.contains("%tz.record.Geometry.Point.Point"));
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Console, wasm).unwrap()
        );
    }
    let shadowed = "record Inner { distance: i64 -> i64 }\nrecord Outer { Point: Inner }\nlet Geometry = Outer { Point: Inner { distance: \\value -> value + 1 } }\nGeometry.Point.distance 41";
    analyze_modules(&[
        (
            "Geometry/Point.tz",
            "fn distance(value: i64) -> i64 { value }",
        ),
        ("Main.tz", shadowed),
    ])
    .unwrap();
    for (declaration, expression, code) in [
        (
            "fn value() -> i64 { 1 }",
            "Geometry.Point.missing()",
            "E1002",
        ),
        (
            "private def value :: i64\nfn value = 1",
            "Geometry.Point.value()",
            "E1022",
        ),
    ] {
        assert_eq!(
            analyze_modules(&[("Geometry/Point.tz", declaration), ("Main.tz", expression)])
                .unwrap_err()
                .code,
            code
        );
    }
}

#[test]
fn recursively_loads_sources_and_preserves_explicit_project_roots() {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tsuzuri::{check::ModuleOrigin, driver::Project};
    let directory = std::env::temp_dir().join(format!(
        "tsuzuri-hierarchy-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(directory.join("Geometry")).unwrap();
    fs::create_dir_all(directory.join(".hidden")).unwrap();
    fs::write(directory.join("Main.tz"), "Geometry.Point.value()").unwrap();
    fs::write(
        directory.join("Geometry/Point.tz"),
        "fn value() -> i64 { 42 }",
    )
    .unwrap();
    fs::write(
        directory.join("Geometry/Main.tz"),
        "fn main() -> i64 { 99 }",
    )
    .unwrap();
    fs::write(directory.join(".hidden/Bad.tz"), "bad syntax").unwrap();
    let project = Project::load(&directory).unwrap();
    project.analyze().unwrap();
    let users: Vec<_> = project
        .sources
        .iter()
        .filter(|source| source.origin == ModuleOrigin::User)
        .map(|source| source.name.as_str())
        .collect();
    assert_eq!(users, ["Geometry.Main", "Geometry.Point", "Main"]);
    assert_eq!(
        project.sources[1].relative_path,
        std::path::Path::new("Geometry/Point.tz")
    );
    let nested = Project::load(&directory.join("Geometry/Point.tz")).unwrap();
    assert!(nested.sources.iter().any(|source| source.name == "Point"));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(directory.join("Main.tz"), directory.join("Link.tz")).unwrap();
        assert_eq!(
            Project::load(&directory).unwrap_err().diagnostic.code,
            "E1011"
        );
        fs::remove_file(directory.join("Link.tz")).unwrap();
        std::os::unix::fs::symlink(&directory, directory.join("Loop")).unwrap();
        assert_eq!(
            Project::load(&directory).unwrap_err().diagnostic.code,
            "E1011"
        );
        fs::remove_file(directory.join("Loop")).unwrap();
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn source_discovery_enforces_file_directory_and_depth_limits() {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tsuzuri::driver::Project;
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-module-limits-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Main.tz"), "0").unwrap();
    let mut path = root.clone();
    for _ in 0..15 {
        path.push("Nested");
        fs::create_dir(&path).unwrap();
    }
    fs::write(path.join("Value.tz"), "").unwrap();
    Project::load(&root).unwrap().analyze().unwrap();
    fs::create_dir(path.join("TooDeep")).unwrap();
    assert_eq!(Project::load(&root).unwrap_err().diagnostic.code, "E1017");
    fs::remove_dir_all(root.join("Nested")).unwrap();
    for index in 0..4095 {
        fs::write(root.join(format!("Value{index}.tz")), "").unwrap();
    }
    assert_eq!(
        Project::load(&root)
            .unwrap()
            .sources
            .iter()
            .filter(|source| source.origin == tsuzuri::check::ModuleOrigin::User)
            .count(),
        4096
    );
    fs::write(root.join("TooMany.tz"), "").unwrap();
    assert_eq!(Project::load(&root).unwrap_err().diagnostic.code, "E1017");
    fs::remove_file(root.join("TooMany.tz")).unwrap();
    for index in 0..4095 {
        fs::remove_file(root.join(format!("Value{index}.tz"))).unwrap();
    }
    for index in 0..1023 {
        fs::create_dir(root.join(format!("Directory{index}"))).unwrap();
    }
    Project::load(&root).unwrap();
    fs::create_dir(root.join("TooMany")).unwrap();
    assert_eq!(Project::load(&root).unwrap_err().diagnostic.code, "E1017");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn accepts_the_point_example_with_top_level_bindings() {
    let module = analyze_modules(&[
        ("Point", POINT),
        (
            "Main",
            "let p = Point { x: 10.0, y: 20.5}\nlet d = Point.distance(p)",
        ),
    ])
    .unwrap();
    let entry = &module.functions[module.entry.unwrap()];
    assert_eq!(entry.signature.result, Type::Unit);
    assert_eq!(module.records[0].name, "Point.Point");
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call double @tz.fn.Point.distance"));
    assert!(ir.contains("call i8 @tz.fn.Main.$entry()"));
    assert!(ir.contains("define double @tz_hypotenuse"));
    assert!(!ir.contains("@tz_distance("));
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Console).unwrap());
}

#[test]
fn resolves_qualified_function_values_pipelines_and_lexical_shadowing() {
    let module = analyze_modules(&[
        (
            "Main",
            "record Callback { distance: fn(Point.Point) -> f64 }
             fn apply(f: fn(Point) -> f64, p: Point) -> f64 { p |> f }
             fn main() -> f64 {
                 let p: Point.Point = Point.Point { x: 3.0, y: 4.0 };
                 let f = Point.distance;
                 let Point = Callback { distance: f };
                 apply(Point.distance, p)
             }",
        ),
        ("Point", POINT),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("@tz.fn.Point.distance"));
    assert!(ir.contains("getelementptr inbounds %tz.record.Main.Callback"));
}

#[test]
fn keeps_same_named_functions_and_records_in_separate_modules() {
    let module = analyze_modules(&[
        (
            "Left",
            "record Value { x: i64 }
             fn value(v: Value) -> i64 { v.x }
             fn make() -> Value { Value { x: 20 } }",
        ),
        (
            "Right",
            "record Value { x: i64 }
             fn value(v: Value) -> i64 { v.x + 1 }
             fn make() -> Value { Value { x: 21 } }",
        ),
        (
            "Main",
            "fn main() -> i64 {
                 let left: Left.Value = Left.make();
                 let right = Right.Value { x: 21 };
                 Left.value(left) + Right.value(right)
             }",
        ),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for symbol in [
        "%tz.record.Left.Value = type",
        "%tz.record.Right.Value = type",
        "@tz.fn.Left.value",
        "@tz.fn.Right.value",
    ] {
        assert!(ir.contains(symbol), "{symbol}");
    }
    let error = analyze_modules(&[
        ("Left", "record Value { x: i64 }"),
        ("Right", "record Value { x: i64 }"),
        ("Main", "fn f(value: Value) -> i64 { value.x }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004");
    assert!(error.message.contains("ambiguous"));
    assert!(error.message.contains("Left.Value"));
    assert!(error.message.contains("Right.Value"));
    assert_eq!(error.span.source, Some(2));

    let error = analyze_modules(&[
        ("Left", "record Value { x: i64 }"),
        ("Right", "record Value { x: i64 }"),
        ("Main", "fn f(value: Left.Value) -> Right.Value { value }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1003");
    assert!(
        error
            .message
            .contains("expected Right.Value, found Left.Value")
    );
}

#[test]
fn resolves_cross_module_recursion_and_qualified_self_tail_calls() {
    let module = analyze_modules(&[
        (
            "Even",
            "fn rec accepts(n: i64) -> bool { if n == 0 { true } else { Odd.accepts(n - 1) } }",
        ),
        (
            "Odd",
            "fn rec accepts(n: i64) -> bool { if n == 0 { false } else { Even.accepts(n - 1) } }",
        ),
        (
            "Loop",
            "fn rec sum(n: i64, a: [i64]) -> i64 {
                 let value = a[n & 1];
                 if n == 0 { value } else { Loop.sum(n - 1, [value + 1, value]) }
             }
             fn rec down(n: i64) -> i64 { if n == 0 { 0 } else { n - 1 |> Loop.down } }",
        ),
        ("Main", "Even.accepts(100)"),
    ])
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i1 @tz.fn.Odd.accepts"));
    assert!(ir.contains("call i1 @tz.fn.Even.accepts"));
    for name in ["sum", "down"] {
        let body = ir
            .split(&format!("define internal i64 @tz.fn.Loop.{name}("))
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        assert!(!body.contains(&format!("call i64 @tz.fn.Loop.{name}")));
    }
    let sum = ir.split("@tz.fn.Loop.sum(").nth(1).unwrap();
    assert!(sum.find("alloca").unwrap() < sum.find("br label %loop").unwrap());
}

#[test]
fn rejects_unqualified_foreign_functions_and_missing_members() {
    for source in [
        "fn main() -> f64 { distance(Point { x: 3.0, y: 4.0 }) }",
        "fn main() -> f64 { Point.missing() }",
        "fn main() -> f64 { Missing.distance() }",
        "fn main() -> f64 { Point.sqrt(4.0) }",
    ] {
        let error = analyze_modules(&[("Point", POINT), ("Main", source)]).unwrap_err();
        assert_eq!(error.code, "E1002", "{source}: {}", error.message);
        assert_eq!(error.span.source, Some(1));
    }
}

#[test]
fn validates_module_names_and_preserves_unique_export_abi() {
    for name in ["", "_", "fn", "Bad-Name", "Nested.Module", "日本語"] {
        let error = analyze_modules(&[(name, "fn value() -> i64 { 1 }")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}");
    }
    assert_eq!(
        analyze_modules(&[("Same", ""), ("Same", "")])
            .unwrap_err()
            .code,
        "E1011"
    );
    for source in ["module Nested {}", "namespace Nested", "module Main"] {
        assert_eq!(
            analyze_modules(&[("Main", source)]).unwrap_err().code,
            "E0002"
        );
    }
    let error = analyze_modules(&[
        ("A", "export fn value() -> i64 { 1 }"),
        ("B", "export fn value() -> i64 { 2 }"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1001");
    assert_eq!(error.span.source, Some(1));
    assert!(error.message.contains("tz_value"));
    assert!(
        analyze_modules(&[
            ("module", "fn value() -> i64 { 42 }"),
            ("Main", "module.value()"),
        ])
        .is_ok()
    );
}

#[test]
fn uses_only_main_as_the_application_entry_point() {
    let module = analyze_modules(&[
        ("Other", "fn main() -> bool { true }"),
        ("Main", "fn main() -> i64 { 42 }"),
    ])
    .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].qualified_name(),
        "Main.main"
    );
    assert!(
        llvm::emit(&module, llvm::Entry::Console)
            .unwrap()
            .contains("call i64 @tz.fn.Main.main()")
    );

    let library = analyze_modules(&[("Other", "fn main() -> i64 { 42 }")]).unwrap();
    assert_eq!(
        llvm::emit(&library, llvm::Entry::Console).unwrap_err().code,
        "E2004"
    );
    for sources in [
        vec![("Other", "let x = 42")],
        vec![("main", "42")],
        vec![("Main", "fn main() -> i64 { 1 }\nlet value = 42")],
    ] {
        assert_eq!(analyze_modules(&sources).unwrap_err().code, "E2004");
    }
    for source in [
        "fn main(x: i64) -> i64 { x }",
        "record R {} fn main() -> R { R {} }",
        "[1, 2]",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        assert_eq!(
            llvm::emit(&module, llvm::Entry::Console).unwrap_err().code,
            "E2004"
        );
    }
}

#[test]
fn supports_entry_bindings_with_newlines_or_semicolons_and_a_final_result() {
    for source in [
        "let x = 40\nlet x = x + 2\nx",
        "let x = 40; let x = x + 2; x",
        "let x: i64 = 40\r\nlet y = x + 2\r\ny",
        "let x = 40 // comment\nlet y = x + 2 // comment\n y",
        "let x = 40\n(x + 2)",
        "let x = 40\n-x",
        "let x = 40 +\n2\nx",
        "let x = (40\n+ 2)\nx",
        "let x = if true {\n40\n} else {\n0\n}\nx + 2",
        "fn f() -> i64 { 42 }\nlet g = f\n{ g() }",
        "fn f() -> i64 { 42 }\nlet g = Main.f\n{ g() }",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        let entry = &module.functions[module.entry.unwrap()];
        assert_eq!(entry.signature.result, Type::I64);
        assert!(matches!(entry.body.kind, TypedExprKind::Block { .. }));
    }
    for source in [
        "let x = 1 let y = 2",
        "fn f() -> i64 { let x = 1\nx }",
        "let x = 1\nfn f() -> i64 { x }",
    ] {
        assert_eq!(
            analyze_modules(&[("Main", source)]).unwrap_err().code,
            "E0002",
            "{source}"
        );
    }
}

#[test]
fn keeps_source_identity_for_lexing_parsing_types_and_recursive_layouts() {
    for (source, code) in [
        ("fn value() -> i64 { ? }", "E0001"),
        ("fn value() -> i64 {", "E0002"),
        ("fn value() -> i64 { true }", "E1003"),
        ("record R { r: R }", "E1010"),
    ] {
        let error = analyze_modules(&[("Main", "42"), ("Other", source)]).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.span.source, Some(1));
        assert!(error.span.end <= source.len());
    }
    let error =
        analyze_modules(&[("A", "record R { r: B.R }"), ("B", "record R { r: A.R }")]).unwrap_err();
    assert_eq!(error.code, "E1010");
}
