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
fn package_namespaces_name_root_and_dependency_modules() {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tsuzuri::driver::Project;
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-namespaces-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    write(
        "app/Tsuzuri.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nnamespace = \"Acme.App\"\n[dependencies]\nacme-tools = { path = \"../tools\" }\n",
    );
    write(
        "tools/Tsuzuri.toml",
        "[package]\nname = \"acme-tools\"\nversion = \"0.1.0\"\nnamespace = \"Acme.Tools\"\n",
    );
    write("tools/Text.tz", "def width :: i64 -> i64 = \\x -> x * 10\n");
    write(
        "app/Shapes/Square.tz",
        "def side :: i64 -> i64 = \\x -> x\n",
    );
    write(
        "app/Main.tz",
        "namespace Acme.App\n\nusing Acme.Tools\n\ndef main :: i64 = \\() -> Text.width 1 + Acme.Tools.Text.width 2 + Shapes.Square.side 3 + Acme.App.Shapes.Square.side 4\n",
    );
    let project = Project::load(&root.join("app")).unwrap();
    let module = project.analyze().unwrap();
    // Files of the package namespace keep their path names inside the compiler.
    assert_eq!(
        module.functions[module.entry.unwrap()].qualified_name(),
        "Main.main"
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for symbol in [
        "@tz.fn.Main.main()",
        "@tz.fn.Acme.Tools.Text.width(",
        "@tz.fn.Shapes.Square.side(",
    ] {
        assert!(ir.contains(symbol), "{symbol}");
    }
    // A root directory may not reuse a dependency namespace.
    write("app/Acme/Tools/Extra.tz", "");
    let error = Project::load(&root.join("app")).unwrap_err();
    assert_eq!(error.diagnostic.code, "E1011");
    assert!(error.diagnostic.message.contains("dependency namespace"));
    fs::remove_dir_all(root.join("app/Acme")).unwrap();
    // A folder without a manifest uses its name as the package namespace.
    write(
        "plain-dir/Main.tz",
        "namespace PlainDir\n\ndef main :: i64 = \\() -> Util.one ()\n",
    );
    write("plain-dir/Util.tz", "def one :: unit -> i64 = \\_ -> 1\n");
    let module = Project::load(&root.join("plain-dir"))
        .unwrap()
        .analyze()
        .unwrap();
    assert_eq!(
        module.functions[module.entry.unwrap()].qualified_name(),
        "Main.main"
    );
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
    for name in [
        "",
        "_",
        "fn",
        "Bad-Name",
        "Nested.Module",
        "日本語",
        "module",
        "main",
        "lower",
    ] {
        let error = analyze_modules(&[(name, "fn value() -> i64 { 1 }")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}");
    }
    assert!(
        analyze_modules(&[("lower", "")])
            .unwrap_err()
            .message
            .contains("for example to 'Lower'")
    );
    assert_eq!(
        analyze_modules(&[("Same", ""), ("Same", "")])
            .unwrap_err()
            .code,
        "E1011"
    );
    for source in [
        "module Nested {}",
        "namespace Nested {}",
        "module Main",
        "def x :: i64 = 1\nnamespace Nested",
        "def x :: i64 = 1\nusing Nested",
        "using Nested\nnamespace Nested",
        "namespace Nested\nusing Nested Other",
    ] {
        assert_eq!(
            analyze_modules(&[("Main", source)]).unwrap_err().code,
            "E0002",
            "{source}"
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
            ("Module", "fn value() -> i64 { 42 }"),
            ("Main", "let module = 1\nlet namespace = 2\nlet using = 3\nmodule + namespace + using + Module.value()"),
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
        vec![("Main", "fn main() -> i64 { 1 }\nlet value = 42")],
    ] {
        assert_eq!(analyze_modules(&sources).unwrap_err().code, "E2004");
    }
    for source in [
        "fn main(x: i64) -> i64 { x }",
        "record R {} fn main() -> R { R {} }",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        assert_eq!(
            llvm::emit(&module, llvm::Entry::Console).unwrap_err().code,
            "E2004"
        );
    }
    // Top-level code shows any other value through `Display`, or drops it.
    for (source, shown) in [("[1, 2]", Type::String), ("record R {}\nR {}", Type::Unit)] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        assert_eq!(
            module.functions[module.entry.unwrap()].signature.result,
            shown
        );
        llvm::emit(&module, llvm::Entry::Console).unwrap();
    }
}

const NS_SHAPE: &str = "namespace Sample\n\nunion Shape =\n    | Circle of f64\n    | Rect of f64 * f64\n\nunion Maybe<'a> = None | Some of 'a\n\ndef area :: Shape -> f64 = \\shape ->\n    match shape with\n    | Circle r -> r * r * 3.0\n    | Rect (w, h) -> w * h\n";
const NS_POINT: &str = "namespace Sample\n\nrecord Point { x: f64, y: f64 }\n\ndef sum :: Point -> f64 = \\point -> point.x + point.y\n";

#[test]
fn namespaces_qualify_modules_and_module_named_types() {
    let main = "namespace Sample\n\ndef main :: f64 = \\() ->\n    let p = Sample.Point { x: 1.0, y: 2.0 }\n    let q: Point = Point { x: 3.0, y: 4.0 }\n    let maybe: Sample.Shape.Maybe<i64> = Sample.Shape.Some 1\n    Sample.Shape.area (Sample.Shape.Rect (3.0, 4.0)) + Shape.area (Rect (1.0, 2.0)) + Sample.Point.sum p + Point.sum q\n";
    let module =
        analyze_modules(&[("Shape", NS_SHAPE), ("Point", NS_POINT), ("Main", main)]).unwrap();
    let entry = &module.functions[module.entry.unwrap()];
    assert_eq!(entry.qualified_name(), "Sample.Main.main");
    assert_eq!(entry.signature.result, Type::F64);
    assert!(
        llvm::emit(&module, llvm::Entry::Console)
            .unwrap()
            .contains("@tz.fn.Sample.Main.main()")
    );
    // A namespace holds only modules, and a module-named type does not nest again.
    for (source, code) in [
        (
            "def main :: f64 = \\() -> Sample.area (Sample.Shape.Rect (1.0, 1.0))",
            "E1002",
        ),
        (
            "def p :: Sample.Point.Point.Point -> f64 = \\p -> p.x",
            "E1004",
        ),
        (
            "def main :: f64 = \\() -> Missing.Shape.area (Sample.Shape.Rect (1.0, 1.0))",
            "E1002",
        ),
    ] {
        let main = format!("namespace Sample\n\n{source}\n");
        let error = analyze_modules(&[("Shape", NS_SHAPE), ("Point", NS_POINT), ("Main", &main)])
            .unwrap_err();
        assert_eq!(error.code, code, "{source}: {}", error.message);
    }
    // Inner namespaces see the enclosing ones; outer namespaces qualify inner modules.
    let inner = "namespace Sample.Codebase\n\ndef twice :: f64 -> f64 = \\x -> Shape.area (Rect (x, 2.0))\n";
    let main = "namespace Sample\n\ndef main :: f64 = \\() -> Codebase.Foo.twice 1.0 + Sample.Codebase.Foo.twice 2.0\n";
    analyze_modules(&[("Shape", NS_SHAPE), ("Foo", inner), ("Main", main)]).unwrap();
    // The same full name from two files.
    let error = analyze_modules(&[
        ("Shape", NS_SHAPE),
        ("Other/Shape", NS_SHAPE),
        (
            "Main",
            "namespace Sample\n\ndef main :: f64 = \\() -> 0.0\n",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1011");
    assert!(
        error.message.contains("'Sample.Shape'"),
        "{}",
        error.message
    );
    assert_eq!(error.span.source, Some(1));
}

#[test]
fn using_imports_the_modules_of_a_namespace() {
    let features = "namespace Sample.Features\n\ndef scale :: i64 -> i64 = \\x -> x * 2\n";
    let other = "namespace Other\n\ndef scale :: i64 -> i64 = \\x -> x * 3\n";
    let own = "namespace Sample\n\ndef scale :: i64 -> i64 = \\x -> x * 5\n";
    let main = |usings: &str, body: &str| {
        format!("namespace Sample\n{usings}\ndef main :: i64 = \\() -> {body}\n")
    };
    let entry_calls = |module: &tsuzuri::check::CheckedModule| -> String {
        let ir = llvm::emit(module, llvm::Entry::Console).unwrap();
        let body = ir.split("@tz.fn.Sample.Main.main()").nth(1).unwrap();
        body[..body.find("\n}").unwrap()].to_owned()
    };
    let module = analyze_modules(&[
        ("Features/Math", features),
        ("Main", &main("using Sample.Features\n", "Math.scale 21")),
    ])
    .unwrap();
    assert!(entry_calls(&module).contains("@tz.fn.Sample.Features.Math.scale("));
    // `using` resolves relative to the file's namespace, and closer modules win.
    let module = analyze_modules(&[
        ("Features/Math", features),
        ("Math", own),
        ("Main", &main("using Features\n", "Math.scale 21")),
    ])
    .unwrap();
    assert!(entry_calls(&module).contains("@tz.fn.Sample.Math.scale("));
    // Two imported modules with one name are ambiguous until qualified.
    let sources = |body: &str| {
        [
            ("Features/Math".to_owned(), features.to_owned()),
            ("Other/Math".to_owned(), other.to_owned()),
            (
                "Main".to_owned(),
                main("using Sample.Features\nusing Other\n", body),
            ),
        ]
    };
    let ambiguous = sources("Math.scale 21");
    let pairs: Vec<_> = ambiguous
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let error = analyze_modules(&pairs).unwrap_err();
    assert_eq!(error.code, "E1004");
    assert!(
        error
            .message
            .contains("'Sample.Features.Math', 'Other.Math'"),
        "{}",
        error.message
    );
    let qualified = sources("Other.Math.scale 21 + Features.Math.scale 1");
    let pairs: Vec<_> = qualified
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    analyze_modules(&pairs).unwrap();
    for (usings, needle) in [
        ("using Nowhere\n", "names no namespace"),
        (
            "using Sample.Features.Math\n",
            "is a module, not a namespace",
        ),
        (
            "using Features\nusing Sample.Features\n",
            "duplicate using 'Sample.Features'",
        ),
    ] {
        let error = analyze_modules(&[("Features/Math", features), ("Main", &main(usings, "0"))])
            .unwrap_err();
        assert_eq!(error.code, "E1011", "{usings}");
        assert!(
            error.message.contains(needle),
            "{usings}: {}",
            error.message
        );
    }
}

#[test]
fn unit_lambda_definitions_take_no_parameters() {
    for source in [
        "def main :: i32 = \\() ->\n    let x = 40\n    x + 2\n",
        "def main :: i32 = \\() -> 42\n",
    ] {
        let module = analyze_modules(&[("Main", source)]).unwrap();
        let entry = &module.functions[module.entry.unwrap()];
        assert!(entry.parameters.is_empty(), "{source}");
        assert_eq!(entry.signature.result, Type::I32, "{source}");
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
        // An unannotated literal is an `i32`; an `i64` annotation or function fixes `i64`.
        assert!(
            matches!(entry.signature.result, Type::Integer(32 | 64, true)),
            "{source}"
        );
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
