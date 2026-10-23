use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::api_diff::{self, Report};
use tsuzuri::driver::Project;

/// A fresh root with the given files, each at a path below it.
fn root(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-api-diff-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

fn report(old: &Path, new: &Path) -> Report {
    let api = |path: &Path| {
        let project = Project::load_for_api(path).unwrap();
        api_diff::api(&project).unwrap_or_else(|error| panic!("{error:?}"))
    };
    api_diff::compare(&api(old), &api(new))
}

/// `breaking removed def Main.f`: severity, action, kind and name of each change in order.
fn summary(report: &Report) -> Vec<String> {
    report
        .changes
        .iter()
        .map(|change| {
            let severity = if change.breaking {
                "breaking"
            } else {
                "non-breaking"
            };
            format!(
                "{severity} {} {} {}",
                change.action.name(),
                change.kind,
                change.name
            )
        })
        .collect()
}

/// The changes between two versions of the single module `Main` and of the files `others`.
fn compare(old: &str, new: &str, others: &[(&str, &str, &str)]) -> Vec<String> {
    let mut old_files = vec![("Main.tz", old)];
    let mut new_files = vec![("Main.tz", new)];
    for &(file, before, after) in others {
        old_files.push((file, before));
        new_files.push((file, after));
    }
    let (old, new) = (root("old", &old_files), root("new", &new_files));
    let lines = summary(&report(&old, &new));
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
    lines
}

#[test]
fn edition_only_changes_warn_about_implicit_parameter_conventions() {
    let old_manifest = "[package]\nname = \"api\"\nversion = \"0.1.0\"\nedition = \"2026\"\n";
    let new_manifest = old_manifest.replace("2026", "2027");
    let source = "def size :: string -> i64 = \\text -> text.length\n\
        def explicit :: ref string -> i64 = \\text -> text.length\n\
        def owned :: own string -> i64 = \\text -> text.length\n\
        const def constant :: string -> i64 = \\text -> text.length\n";
    let old = root(
        "edition-old",
        &[("Main.tz", source), ("Tsuzuri.toml", old_manifest)],
    );
    let new = root(
        "edition-new",
        &[("Main.tz", source), ("Tsuzuri.toml", &new_manifest)],
    );
    let changes = report(&old, &new);
    assert_eq!(changes.breaking(), 1, "{:?}", changes.changes);
    let change = &changes.changes[0];
    assert!(change.name.ends_with(".size"));
    assert!(change.note.contains("default parameter convention"));
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
}

#[test]
fn mutable_binding_changes_are_compared_for_each_parameter() {
    let borrowed =
        "def f :: string -> string -> i64 = \\left right -> left.length + right.length\n";
    let mutable = borrowed.replace("\\left", "\\mut left");
    check(borrowed, &mutable, &["breaking changed def Main.f"]);
    check(&mutable, borrowed, &["breaking changed def Main.f"]);

    let owned = borrowed.replace(":: string", ":: own string");
    let mutable_owned = owned.replace("\\left", "\\mut left");
    check(&owned, &mutable_owned, &[]);
}

fn check(old: &str, new: &str, expected: &[&str]) {
    assert_eq!(compare(old, new, &[]), expected, "{old}\n=>\n{new}");
}

#[test]
fn defs_are_removed_added_or_changed() {
    check(
        "def f :: i64 -> i64 = \\x -> x\ndef g :: i64 = 1\n",
        "def f :: i64 -> i32 = \\x -> 0\ndef h :: i64 = 1\n",
        &[
            "breaking changed def Main.f",
            "breaking removed def Main.g",
            "non-breaking added def Main.h",
        ],
    );
    // A parameter added, an argument made borrowed, a constraint dropped, a region dropped.
    for (old, new) in [
        (
            "def f :: i64 -> i64 = \\x -> x",
            "def f :: i64 -> i64 -> i64 = \\x y -> x",
        ),
        (
            "def f :: string -> i64 = \\x -> 1",
            "def f :: ref string -> i64 = \\x -> 1",
        ),
        (
            "def f :: Ord<'a> => 'a -> bool = \\x -> true",
            "def f :: 'a -> bool = \\x -> true",
        ),
        (
            "def f {r} :: ref {r} string -> ref {r} string = \\x -> x",
            "def f {r} :: ref {r} string -> string = \\x -> x",
        ),
        (
            "def f :: 'x -> 'y -> 'x = \\a b -> a",
            "def f :: 'x -> 'y -> 'y = \\a b -> b",
        ),
        ("def f :: i64 = 1", "def f :: unit -> i64 = \\() -> 1"),
    ] {
        check(old, new, &["breaking changed def Main.f"]);
    }
}

#[test]
fn adding_a_pure_contract_is_compatible_and_removing_it_is_breaking() {
    check(
        "def f :: i64 -> i64 = \\x -> x\n",
        "pure def f :: i64 -> i64 = \\x -> x\n",
        &["non-breaking changed def Main.f"],
    );
    check(
        "pure def f :: i64 -> i64 = \\x -> x\n",
        "def f :: i64 -> i64 = \\x -> x\n",
        &["breaking changed def Main.f"],
    );
}

#[test]
fn const_def_is_part_of_the_signature() {
    // A `const` initializer of another module may call a const def, so dropping `const` breaks it,
    // while making a def a const def only adds a use. The body is not part of the API.
    let constant = "const def f :: i64 -> i64\nfn f x = x * 2\n";
    let plain = "def f :: i64 -> i64\nfn f x = x * 2\n";
    check(constant, plain, &["breaking changed def Main.f"]);
    check(plain, constant, &["non-breaking changed def Main.f"]);
    check(constant, "const def f :: i64 -> i64\nfn f x = x + x\n", &[]);
    check(
        "export const def f :: i64 -> i64\nfn f x = x\n",
        "export def f :: i64 -> i64\nfn f x = x\n",
        &["breaking changed def Main.f"],
    );
    check(
        "def f :: i64 -> i64\nfn f x = x\n",
        "export const def f :: i64 -> i64\nfn f x = x\n",
        &["non-breaking changed def Main.f"],
    );
}

#[test]
fn names_of_variables_and_regions_and_the_shape_of_the_lambda_do_not_matter() {
    let old = "\
def a :: 'x -> 'y -> 'x = \\p q -> p
def b {r} :: ref {r} string -> ref {r} string = \\s -> s
def c :: i64 -> i64 -> i64 = \\x -> \\y -> x + y
def d :: (Ord<'a>, Eq<'a>) => 'a -> bool = \\v -> true
def e :: ('a -> 'b) -> 'a -> 'b = \\f x -> f x
record Box<'a, 'b> { first: 'a, second: 'b }
type Pair<'a> = ('a * 'a)
";
    let new = "\
def a :: 'p -> 'q -> 'p = \\p q -> p
def b {s} :: ref {s} string -> ref {s} string = \\s -> s
def c :: i64 -> i64 -> i64 = \\x -> add x
def d :: (Eq<'z>, Ord<'z>) => 'z -> bool = \\v -> true
def e :: ('m -> 'n) -> 'm -> 'n = \\f -> f
record Box<'x, 'y> { first: 'x, second: 'y }
type Pair<'q> = ('q * 'q)
";
    check(old, new, &[]);
    // The empty report is not for want of declarations, and a real change is still seen.
    assert_eq!(compare(old, "0", &[]).len(), 7);
    check(
        old,
        &new.replace("def a :: 'p -> 'q -> 'p", "def a :: 'p -> 'q -> 'q"),
        &["breaking changed def Main.a"],
    );
    // Swapping which variable a field has is a change.
    check(
        "record Box<'a, 'b> { first: 'a, second: 'b }",
        "record Box<'a, 'b> { first: 'b, second: 'a }",
        &[
            "breaking changed field Main.Box.first",
            "breaking changed field Main.Box.second",
        ],
    );
}

#[test]
fn a_thunk_is_not_an_extra_parameter() {
    // `fn() -> T` takes no argument, so a result of that type is not spliced into the
    // parameters, and a definition without parameters keeps its function result.
    check(
        "def make :: i64 -> i64 = \\n -> n\n",
        "def make :: i64 -> fn() -> i64 = \\n -> answer\n",
        &["breaking changed def Main.make"],
    );
    check(
        "def build :: i64 -> i64 = \\n -> n\n",
        "def build :: fn() -> (i64 -> i64)\nfn build = { let k = 1; n -> n + k }\n",
        &["breaking changed def Main.build"],
    );
    let class =
        |result: &str| format!("class Shape<'a> {{\n  def make :: ref 'a -> {result}\n}}\n");
    let lines = compare(
        "0",
        "0",
        &[(
            "Shape.tt",
            class("i64").as_str(),
            class("fn() -> i64").as_str(),
        )],
    );
    assert_eq!(lines, ["breaking changed method Shape.Shape.make"]);
    // A chain of arrows is one type however many of them the lambda takes.
    check(
        "def c :: i64 -> i64 -> i64 = \\x -> \\y -> x + y\n",
        "def c :: i64 -> i64 -> i64 = \\x -> add x\n",
        &[],
    );
}

#[test]
fn record_fields() {
    let old = "record P { x: i64, y: i64 }\n";
    for (new, expected) in [
        (
            "record P { x: i64, y: i64, z: i64 }",
            vec!["breaking added field Main.P.z"],
        ),
        (
            "record P { x: i64 }",
            vec!["breaking removed field Main.P.y"],
        ),
        (
            "record P { x: i64, y: i32 }",
            vec!["breaking changed field Main.P.y"],
        ),
        (
            "record P { y: i64, x: i64 }",
            vec!["breaking changed order Main.P"],
        ),
        (
            "record P { mut x: i64, y: i64 }",
            vec!["breaking changed field Main.P.x"],
        ),
        (
            "record P { @json \"px\" x: i64, y: i64 }",
            vec!["breaking changed field Main.P.x"],
        ),
        (
            "record P<'a> { x: i64, y: i64 }",
            vec!["breaking changed record Main.P"],
        ),
        (
            "union P = A | B",
            vec![
                "breaking removed record Main.P",
                "non-breaking added union Main.P",
            ],
        ),
    ] {
        check(old, new, &expected);
    }
}

#[test]
fn union_cases() {
    let old = "union Reading = Missing | Value of i64\n";
    for (new, expected) in [
        (
            "union Reading = Missing | Value of i64 | Other",
            vec!["breaking added case Main.Reading.Other"],
        ),
        (
            "union Reading = Missing",
            vec!["breaking removed case Main.Reading.Value"],
        ),
        (
            "union Reading = Missing | Value of i32",
            vec!["breaking changed case Main.Reading.Value"],
        ),
        (
            "union Reading = Value of i64 | Missing",
            vec!["breaking changed order Main.Reading"],
        ),
    ] {
        check(old, new, &expected);
    }
}

#[test]
fn class_methods_and_defaults() {
    let class = |variable: &str, methods: &str| {
        format!("class Shape<'{variable}> {{\n{methods}}}\n").replace("'_", &format!("'{variable}"))
    };
    let old = class(
        "a",
        "  def area :: ref '_ -> f64\n  def name :: ref '_ -> string\n  fn name s = \"x\"\n",
    );
    for (variable, methods, expected) in [
        (
            // A method without a default breaks the instances that exist; one with a default does not.
            "a",
            "  def area :: ref '_ -> f64\n  def name :: ref '_ -> string\n  def sides :: ref '_ -> i64\n  fn name s = \"x\"\n",
            vec!["breaking added method Shape.Shape.sides"],
        ),
        (
            "a",
            "  def area :: ref '_ -> f64\n  def name :: ref '_ -> string\n  def note :: ref '_ -> string\n  fn name s = \"x\"\n  fn note s = \"\"\n",
            vec!["non-breaking added method Shape.Shape.note"],
        ),
        (
            "a",
            "  def area :: ref '_ -> f64\n  def name :: ref '_ -> string\n",
            vec!["breaking changed method Shape.Shape.name"],
        ),
        (
            "a",
            "  def name :: ref '_ -> string\n  fn name s = \"x\"\n",
            vec!["breaking removed method Shape.Shape.area"],
        ),
        (
            "a",
            "  def area :: ref '_ -> f32\n  def name :: ref '_ -> string\n  fn name s = \"x\"\n",
            vec!["breaking changed method Shape.Shape.area"],
        ),
        // The order of the methods and the name of the class variable are not part of the API.
        (
            "b",
            "  def name :: ref '_ -> string\n  def area :: ref '_ -> f64\n  fn name s = \"x\"\n",
            vec![],
        ),
    ] {
        let new = class(variable, methods);
        let lines = compare("0", "0", &[("Shape.tt", old.as_str(), new.as_str())]);
        assert_eq!(lines, expected, "{new}");
    }
    // A default that appears, and the superclasses.
    let lines = compare(
        "0",
        "0",
        &[(
            "Shape.tt",
            "class Shape<'a> {\n  def name :: ref 'a -> string\n}\n",
            "class Shape<'a> {\n  def name :: ref 'a -> string\n  fn name s = \"x\"\n}\n",
        )],
    );
    assert_eq!(lines, ["non-breaking changed method Shape.Shape.name"]);
    let lines = compare(
        "0",
        "0",
        &[(
            "Rich.tt",
            "class Eq<'a> => Rich<'a> {\n  def rich :: ref 'a -> bool\n}\n",
            "class Ord<'a> => Rich<'a> {\n  def rich :: ref 'a -> bool\n}\n",
        )],
    );
    assert_eq!(lines, ["breaking changed class Rich.Rich"]);
}

#[test]
fn aliases_constants_externs_exports_and_derives() {
    for (old, new, expected) in [
        (
            "type Id = i64",
            "type Id = i32",
            vec!["breaking changed type Main.Id"],
        ),
        (
            "const N: i64 = 3",
            "const N: i32 = 3",
            vec!["breaking changed const Main.N"],
        ),
        // The value of a constant is not part of the API.
        ("const N: i64 = 3", "const N: i64 = 4", vec![]),
        (
            "extern def host :: i64 -> i64",
            "extern def host :: i64 -> i32",
            vec!["breaking changed extern def Main.host"],
        ),
        (
            "extern type Handle",
            "extern type Other",
            vec![
                "breaking removed extern type Main.Handle",
                "non-breaking added extern type Main.Other",
            ],
        ),
        // An export is a symbol of the C ABI: dropping it breaks hosts, adding one breaks nothing.
        (
            "export def f :: i32 -> i32 = \\x -> x",
            "def f :: i32 -> i32 = \\x -> x",
            vec!["breaking changed def Main.f"],
        ),
        (
            "def f :: i32 -> i32 = \\x -> x",
            "export def f :: i32 -> i32 = \\x -> x",
            vec!["non-breaking changed def Main.f"],
        ),
        (
            "record R { v: i64 } deriving (Eq, Ord)",
            "record R { v: i64 } deriving (Eq)",
            vec!["breaking removed derive Main.R.Ord"],
        ),
        (
            "record R { v: i64 } deriving (Eq)",
            "record R { v: i64 } deriving (Eq, Hash)",
            vec!["non-breaking added derive Main.R.Hash"],
        ),
        (
            "union U = A deriving (Eq, Ord)",
            "union U = A deriving (Ord)",
            vec!["breaking removed derive Main.U.Eq"],
        ),
    ] {
        check(old, new, &expected);
    }
}

#[test]
fn private_declarations_are_not_api() {
    check(
        "private def p :: i64 = 1\nprivate record R { x: i64 }\nprivate union U = A\nprivate type T = i64\nprivate const C: i64 = 1\n",
        "private def p :: i32 = 1\nprivate record R { y: i64 }\nprivate union U = B\n",
        &[],
    );
    // Becoming private is a removal; becoming public is an addition.
    check(
        "def p :: i64 = 1",
        "private def p :: i64 = 1",
        &["breaking removed def Main.p"],
    );
    check(
        "private def p :: i64 = 1",
        "def p :: i64 = 1",
        &["non-breaking added def Main.p"],
    );
}

#[test]
fn documentation_and_deprecation_are_not_breaking() {
    check(
        "/// Adds.\ndef f :: i64 = 1\n",
        "/// Adds one.\ndef f :: i64 = 1\n",
        &["non-breaking documented def Main.f"],
    );
    check(
        "def f :: i64 = 1\n",
        "/// Old.\n/// @deprecated use g\ndef f :: i64 = 1\n",
        &["non-breaking deprecated def Main.f"],
    );
    check(
        "/// @deprecated use g\nrecord R { x: i64 }\n",
        "record R { x: i64 }\n",
        &["non-breaking undeprecated record Main.R"],
    );
    // Only the note changed: a change of the documentation.
    check(
        "/// @deprecated use g\ndef f :: i64 = 1\n",
        "/// @deprecated use h\ndef f :: i64 = 1\n",
        &["non-breaking documented def Main.f"],
    );
    // The declaration changed and was deprecated: both are reported, the break first.
    check(
        "def f :: i64 = 1\n",
        "/// @deprecated\ndef f :: i32 = 1\n",
        &[
            "breaking changed def Main.f",
            "non-breaking deprecated def Main.f",
        ],
    );
}

#[test]
fn declared_length_parameters_are_renamed_but_constants_are_not() {
    // The name of a declared length parameter (`const N: i64`) is not part of the API.
    check(
        "record Grid<const N: i64> { cells: [i64; N] }\ntype Row<const N: i64> = [i64; N]\nunion Cell<const N: i64> = Full of [i64; N] | Empty\n",
        "record Grid<const M: i64> { cells: [i64; M] }\ntype Row<const M: i64> = [i64; M]\nunion Cell<const K: i64> = Full of [i64; K] | Empty\n",
        &[],
    );
    // Which parameter a field uses is.
    check(
        "record Pair<const N: i64, const M: i64> { first: [i64; N], second: [i64; M] }\n",
        "record Pair<const N: i64, const M: i64> { first: [i64; M], second: [i64; N] }\n",
        &[
            "breaking changed field Main.Pair.first",
            "breaking changed field Main.Pair.second",
        ],
    );
    // A name that no declaration declared is a constant, and another constant is another length.
    check(
        "const Small: i64 = 2\nconst Large: i64 = 4\nrecord Row { cells: [i64; Small] }\n",
        "const Small: i64 = 2\nconst Large: i64 = 4\nrecord Row { cells: [i64; Large] }\n",
        &["breaking changed field Main.Row.cells"],
    );
}

#[test]
fn instances_of_the_same_spelling_in_two_modules_are_two_instances() {
    let with = "record Point { x: i64 }\ninstance Show<Point> {\n    fn show p = \"\"\n}\n";
    let without = "record Point { x: i64 }\n";
    // Each module's instance is its own, so dropping one of two is not hidden by the other.
    assert_eq!(
        compare(with, with, &[("Other.tz", with, without)]),
        ["breaking removed instance Other.Show<Point>"]
    );
    assert_eq!(
        compare(without, with, &[("Other.tz", with, with)]),
        ["non-breaking added instance Main.Show<Point>"]
    );
}

#[test]
fn instances_active_patterns_and_builder_aliases() {
    check(
        "instance Show<Box<i64>> {\n    fn show b = \"\"\n}\n",
        "0",
        &["breaking removed instance Main.Show<Box<i64>>"],
    );
    check(
        "0",
        "instance Show<Box<i64>> {\n    fn show b = \"\"\n}\ninstance Eq<'a> => Show<Box<'a>> {\n    fn show b = \"\"\n}\n",
        &[
            "non-breaking added instance Main.Show<Box<'t0>>",
            "non-breaking added instance Main.Show<Box<i64>>",
        ],
    );
    // The constraints of an instance and the names of its variables.
    let conditional = |variable: &str, class: &str| {
        format!(
            "instance {class}<'{variable}> => Show<Box<'{variable}>> {{\n    fn show b = \"\"\n}}\n"
        )
    };
    check(&conditional("a", "Eq"), &conditional("b", "Eq"), &[]);
    check(
        &conditional("a", "Eq"),
        &conditional("a", "Ord"),
        &["breaking changed instance Main.Show<Box<'t0>>"],
    );
    // An active pattern is named by its cases.
    check(
        "def (|Even|Odd|) :: i64 -> 'T = \\n -> if n == 0 then Even else Odd\n",
        "def (|Even|Odd|) :: i64 -> 'T = \\n -> Even\n",
        &[],
    );
    check(
        "def (|Even|Odd|) :: i64 -> 'T = \\n -> Even\n",
        "def (|Even|Odd|Zero|) :: i64 -> 'T = \\n -> Even\n",
        &[
            "breaking removed def Main.(|Even|Odd|)",
            "non-breaking added def Main.(|Even|Odd|Zero|)",
        ],
    );
    let lines = compare(
        "0",
        "0",
        &[("Lazy.tc", "@alias lazy\n@alias later\n", "@alias lazy\n")],
    );
    assert_eq!(lines, ["breaking removed alias Lazy.later"]);
}

#[test]
fn reports_are_one_line_per_change_with_breaking_changes_first() {
    let old = root(
        "lines-old",
        &[(
            "Main.tz",
            "def a :: i64 = 1\ndef b :: i64 = 1\ndef z :: i64 = 1\nrecord R { x: i64 }\n",
        )],
    );
    let new = root(
        "lines-new",
        &[(
            "Main.tz",
            "def a :: i64 = 1\ndef b :: i32 = 1\ndef c :: i64 = 1\nrecord R { x: i64, y: i64 }\n",
        )],
    );
    let report = report(&old, &new);
    assert_eq!(
        report.render_text(),
        "\
breaking: added field Main.R.y: `y: i64` (constructions of the record without it stop compiling)
breaking: changed def Main.b: `def b :: i64` => `def b :: i32`
breaking: removed def Main.z: `def z :: i64`
non-breaking: added def Main.c: `def c :: i64`
summary: 3 breaking, 1 non-breaking
"
    );
    let json: Vec<_> = report.render_json().lines().map(str::to_owned).collect();
    assert_eq!(
        json,
        [
            r#"{"type":"change","severity":"breaking","action":"added","kind":"field","name":"Main.R.y","old":null,"new":"y: i64","note":"constructions of the record without it stop compiling"}"#,
            r#"{"type":"change","severity":"breaking","action":"changed","kind":"def","name":"Main.b","old":"def b :: i64","new":"def b :: i32","note":null}"#,
            r#"{"type":"change","severity":"breaking","action":"removed","kind":"def","name":"Main.z","old":"def z :: i64","new":null,"note":null}"#,
            r#"{"type":"change","severity":"non-breaking","action":"added","kind":"def","name":"Main.c","old":null,"new":"def c :: i64","note":null}"#,
            r#"{"type":"summary","breaking":3,"non_breaking":1,"omitted":0,"old_version":null,"new_version":null,"compatible_versions":null}"#,
        ]
    );
    // The same inputs give the same report.
    assert_eq!(report.render_text(), self::report(&old, &new).render_text());
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
}

#[test]
fn the_declarations_are_shown_as_written_in_each_version() {
    let old = root(
        "shown-old",
        &[("Main.tz", "def f :: 'a -> 'a = \\x -> x\n")],
    );
    let new = root(
        "shown-new",
        &[("Main.tz", "def f :: 'b -> 'b -> 'b = \\x y -> x\n")],
    );
    let lines = report(&old, &new).render_text();
    assert!(
        lines.contains("`def f :: 'a -> 'a` => `def f :: 'b -> 'b -> 'b`"),
        "{lines}"
    );
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
}

#[test]
fn modules_are_identified_by_namespace_and_name_but_a_missing_manifest_has_no_namespace() {
    // Without manifests, the folder names (which set the default namespace) do not matter.
    let old = root("folder-old", &[("Shapes.tz", "def area :: i64 = 1\n")]);
    let new = root("folder-new", &[("Shapes.tz", "def area :: i64 = 1\n")]);
    assert!(report(&old, &new).changes.is_empty());
    // A moved declaration is a removal and an addition.
    let moved = root("folder-moved", &[("Figures.tz", "def area :: i64 = 1\n")]);
    assert_eq!(
        summary(&report(&old, &moved)),
        [
            "breaking removed def Shapes.area",
            "non-breaking added def Figures.area"
        ]
    );
    // Declared and directory namespaces are part of the name.
    let nested = root(
        "folder-nested",
        &[
            ("Shapes.tz", "namespace Acme::Draw\n\ndef area :: i64 = 1\n"),
            ("Util/Maths.tz", "def twice :: i64 -> i64 = \\x -> x + x\n"),
        ],
    );
    assert_eq!(summary(&report(&nested, &nested)), Vec::<String>::new());
    let lines = report(&old, &nested).render_text();
    assert!(
        lines.contains("added def Acme::Draw::Shapes.area"),
        "{lines}"
    );
    assert!(lines.contains("added def Util::Maths.twice"), "{lines}");
    for path in [old, new, moved, nested] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

fn manifest(name: &str, version: &str, extra: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n{extra}")
}

#[test]
fn a_package_is_its_own_modules_under_its_namespace_and_versions_are_noted() {
    let library = "/// @deprecated use next\ndef old :: i64 -> i64 = \\x -> x\n";
    let old = root(
        "pkg-old",
        &[
            ("lib/Tsuzuri.toml", &manifest("legacy-lib", "1.2.0", "")),
            ("lib/Old.tz", library),
        ],
    );
    let new = root(
        "pkg-new",
        &[
            ("lib/Tsuzuri.toml", &manifest("legacy-lib", "1.3.0", "")),
            ("lib/Old.tz", "def next :: i64 -> i64 = \\x -> x\n"),
        ],
    );
    let (old_lib, new_lib) = (old.join("lib"), new.join("lib"));
    let report = report(&old_lib, &new_lib);
    assert_eq!(
        summary(&report),
        [
            "breaking removed def LegacyLib::Old.old",
            "non-breaking added def LegacyLib::Old.next",
        ]
    );
    // 1.3.0 is a compatible release of 1.2.0, so dependents that accept the one take the other.
    assert_eq!(report.compatible_versions(), Some(true));
    let text = report.render_text();
    assert!(
        text.ends_with(
            "summary: 1 breaking, 1 non-breaking\nnote: version 1.3.0 is compatible with 1.2.0, so registry dependents that accept 1.2.0 would take it; raise the major version (the minor version before 1.0.0) for a breaking change\n"
        ),
        "{text}"
    );
    assert!(report.render_json().ends_with(
        "\"old_version\":\"1.2.0\",\"new_version\":\"1.3.0\",\"compatible_versions\":true}\n"
    ));
    // A new major version carries no note; before 1.0.0 a new minor version is a new series.
    for (before, after, compatible) in [
        ("1.2.0", "2.0.0", false),
        ("0.3.1", "0.4.0", false),
        ("0.3.1", "0.3.2", true),
    ] {
        let (a, b) = (
            root(
                "versions-a",
                &[
                    ("Tsuzuri.toml", &manifest("a-b", before, "")),
                    ("Main.tz", "def f :: i64 = 1\n"),
                ],
            ),
            root(
                "versions-b",
                &[
                    ("Tsuzuri.toml", &manifest("a-b", after, "")),
                    ("Main.tz", "def g :: i64 = 1\n"),
                ],
            ),
        );
        let report = self::report(&a, &b);
        assert_eq!(
            report.compatible_versions(),
            Some(compatible),
            "{before} {after}"
        );
        assert_eq!(report.render_text().contains("note: version"), compatible);
        std::fs::remove_dir_all(a).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }
    // The default namespace is part of every name: renaming the package changes them all.
    let renamed = root(
        "pkg-renamed",
        &[
            (
                "lib/Tsuzuri.toml",
                &manifest("legacy-lib", "2.0.0", "namespace = \"Legacy\"\n"),
            ),
            ("lib/Old.tz", library),
        ],
    );
    let report = self::report(&old_lib, &renamed.join("lib"));
    assert_eq!(
        summary(&report),
        [
            "breaking removed def LegacyLib::Old.old",
            "breaking changed namespace legacy-lib",
            "non-breaking added def Legacy::Old.old",
        ]
    );
    for path in [old, new, renamed] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn only_the_root_package_counts_and_nothing_is_type_checked_or_fetched() {
    let main = "def broken :: i64 = missing_name + 1\n";
    let root = root(
        "deps",
        &[
            (
                "app/Tsuzuri.toml",
                &manifest(
                    "app",
                    "0.1.0",
                    "[dependencies]\nshapes = { path = \"../shapes\" }\nremote = { git = \"https://example.org/remote.git\", rev = \"0123456789abcdef0123456789abcdef01234567\" }\nregistered = { version = \"1.0.0\" }\n",
                ),
            ),
            ("app/Main.tz", main),
            ("shapes/Tsuzuri.toml", &manifest("shapes", "0.1.0", "")),
            ("shapes/Circle.tz", "def area :: i64 = 1\n"),
        ],
    );
    // No Tsuzuri.lock, no store, and the body does not check: the API is still read.
    let app = root.join("app");
    let report = report(&app, &app);
    assert!(report.changes.is_empty());
    let project = Project::load_for_api(&app).unwrap();
    assert!(project.root_package.is_some());
    let api_text = tsuzuri::api_diff::api(&project).is_ok();
    assert!(api_text);
    // A dependency's declarations are not the package's API.
    let empty = self::root(
        "deps-empty",
        &[
            ("Tsuzuri.toml", &manifest("app", "0.1.0", "")),
            ("Main.tz", "0\n"),
        ],
    );
    assert_eq!(
        summary(&self::report(&empty, &app)),
        ["non-breaking added def App::Main.broken"]
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(empty).unwrap();
}

fn tsuzuri(arguments: &[&std::ffi::OsStr]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn the_command_reports_on_standard_output_and_fails_on_a_breaking_change() {
    let old = root(
        "cli-old",
        &[("Main.tz", "def f :: i64 = 1\ndef g :: i64 = 1\n")],
    );
    let same = root(
        "cli-same",
        &[("Main.tz", "def f :: i64 = 1\ndef g :: i64 = 1\n")],
    );
    let added = root(
        "cli-added",
        &[(
            "Main.tz",
            "def f :: i64 = 1\ndef g :: i64 = 1\ndef h :: i64 = 1\n",
        )],
    );
    let broken = root("cli-broken", &[("Main.tz", "def f :: i64 = 1\n")]);
    let run = |old: &Path, new: &Path, flags: &[&str]| {
        let mut arguments: Vec<&std::ffi::OsStr> =
            vec!["api-diff".as_ref(), old.as_os_str(), new.as_os_str()];
        arguments.extend(flags.iter().map(std::ffi::OsStr::new));
        tsuzuri(&arguments)
    };
    let (status, stdout, stderr) = run(&old, &same, &[]);
    assert_eq!(
        (status, stdout.as_str(), stderr.as_str()),
        (Some(0), "summary: 0 breaking, 0 non-breaking\n", "")
    );
    let (status, stdout, stderr) = run(&old, &added, &[]);
    assert_eq!(status, Some(0), "{stderr}");
    assert_eq!(
        stdout,
        "non-breaking: added def Main.h: `def h :: i64`\nsummary: 0 breaking, 1 non-breaking\n"
    );
    let (status, stdout, stderr) = run(&old, &broken, &[]);
    assert_eq!(status, Some(1), "{stderr}");
    assert_eq!(
        stdout,
        "breaking: removed def Main.g: `def g :: i64`\nsummary: 1 breaking, 0 non-breaking\n"
    );
    assert_eq!(stderr, "");
    let (status, stdout, _) = run(&old, &broken, &["--json"]);
    assert_eq!(status, Some(1));
    assert_eq!(
        stdout,
        concat!(
            r#"{"type":"change","severity":"breaking","action":"removed","kind":"def","name":"Main.g","old":"def g :: i64","new":null,"note":null}"#,
            "\n",
            r#"{"type":"summary","breaking":1,"non_breaking":0,"omitted":0,"old_version":null,"new_version":null,"compatible_versions":null}"#,
            "\n"
        )
    );
    // Files stand for their directories, like every other command's input.
    let (status, stdout, _) = run(&old.join("Main.tz"), &broken.join("Main.tz"), &[]);
    assert_eq!(status, Some(1));
    assert!(stdout.contains("removed def Main.g"), "{stdout}");
    // The flag may come first, and `--` ends the flags.
    let (status, ..) = tsuzuri(&[
        "api-diff".as_ref(),
        "--json".as_ref(),
        old.as_os_str(),
        same.as_os_str(),
    ]);
    assert_eq!(status, Some(0));
    for path in [old, same, added, broken] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn the_command_rejects_bad_usage_inputs_and_sources() {
    let good = root("cli-good", &[("Main.tz", "def f :: i64 = 1\n")]);
    let bad = root("cli-bad", &[("Main.tz", "def f :: = \n")]);
    let empty = root("cli-empty", &[]);
    let missing = good.join("missing");
    let usage = "usage: tsuzuri api-diff old new [--json]";
    for arguments in [
        vec!["api-diff".as_ref()],
        vec!["api-diff".as_ref(), good.as_os_str()],
        vec![
            "api-diff".as_ref(),
            good.as_os_str(),
            good.as_os_str(),
            good.as_os_str(),
        ],
        vec![
            "api-diff".as_ref(),
            good.as_os_str(),
            good.as_os_str(),
            "--out".as_ref(),
        ],
    ] {
        let (status, stdout, stderr) = tsuzuri(&arguments);
        assert_eq!(status, Some(2), "{arguments:?}");
        assert!(stdout.is_empty(), "{stdout}");
        assert!(
            stderr.contains("error[E2000]") && stderr.contains(usage),
            "{stderr}"
        );
    }
    // With `--json`, the usage error is a JSON diagnostic, too.
    for arguments in [
        vec![
            "api-diff".as_ref(),
            good.as_os_str(),
            good.as_os_str(),
            "--json".as_ref(),
            "--json".as_ref(),
        ],
        vec![
            "api-diff".as_ref(),
            good.as_os_str(),
            good.as_os_str(),
            "--bogus".as_ref(),
            "--json".as_ref(),
        ],
    ] {
        let (status, stdout, stderr) = tsuzuri(&arguments);
        assert_eq!(status, Some(2), "{arguments:?}");
        assert!(stdout.is_empty(), "{stdout}");
        assert!(
            stderr.contains("\"code\":\"E2000\"") && stderr.contains(usage),
            "{stderr}"
        );
    }
    // A source that does not parse is reported where it is, on whichever side it is.
    for (old, new, side) in [(&bad, &good, "old"), (&good, &bad, "new")] {
        let (status, stdout, stderr) =
            tsuzuri(&["api-diff".as_ref(), old.as_os_str(), new.as_os_str()]);
        assert_eq!(status, Some(1), "{side}");
        assert!(stdout.is_empty());
        assert!(
            stderr.contains("Main.tz:1:") && stderr.contains("error[E0002]"),
            "{side}: {stderr}"
        );
    }
    // An input that is missing or has no sources is an error, not an empty API.
    let (status, _, stderr) =
        tsuzuri(&["api-diff".as_ref(), good.as_os_str(), missing.as_os_str()]);
    assert_eq!(status, Some(1));
    assert!(stderr.contains("error[E2001]"), "{stderr}");
    let (status, _, stderr) = tsuzuri(&["api-diff".as_ref(), empty.as_os_str(), good.as_os_str()]);
    assert_eq!(status, Some(1));
    assert!(
        stderr.contains("error[E2000]") && stderr.contains("has no .tz, .tt, or .tc sources"),
        "{stderr}"
    );
    for path in [good, bad, empty] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn the_command_applies_the_limits_of_the_other_commands() {
    let good = root("limits-good", &[("Main.tz", "def f :: i64 = 1\n")]);
    let large = root("limits-large", &[]);
    std::fs::write(large.join("Main.tz"), "/".repeat(4 * 1024 * 1024 + 1)).unwrap();
    let (status, stdout, stderr) =
        tsuzuri(&["api-diff".as_ref(), good.as_os_str(), large.as_os_str()]);
    assert_eq!(status, Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("error[E0003]"), "{stderr}");
    let many = root("limits-many", &[]);
    for index in 0..=4096 {
        std::fs::write(many.join(format!("M{index:04}.tz")), "def f :: i64 = 1\n").unwrap();
    }
    let (status, stdout, stderr) =
        tsuzuri(&["api-diff".as_ref(), many.as_os_str(), good.as_os_str()]);
    assert_eq!(status, Some(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("error[E1017]"), "{stderr}");
    for path in [good, large, many] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn the_command_refuses_symbolic_links() {
    let old = root("cli-link", &[("Main.tz", "def f :: i64 = 1\n")]);
    let target = root("cli-target", &[("Main.tz", "def f :: i64 = 1\n")]);
    let link = old.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let (status, _, stderr) = tsuzuri(&["api-diff".as_ref(), link.as_os_str(), target.as_os_str()]);
    assert_eq!(status, Some(1));
    assert!(stderr.contains("error[E1011]"), "{stderr}");
    // A symbolic link among the sources is refused, too.
    let linked = root("cli-linked", &[("Main.tz", "def f :: i64 = 1\n")]);
    std::os::unix::fs::symlink(target.join("Main.tz"), linked.join("Other.tz")).unwrap();
    let (status, _, stderr) =
        tsuzuri(&["api-diff".as_ref(), linked.as_os_str(), target.as_os_str()]);
    assert_eq!(status, Some(1));
    assert!(stderr.contains("error[E1011]"), "{stderr}");
    for path in [old, target, linked] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn a_long_report_lists_breaking_changes_first_and_counts_the_rest() {
    let declarations = |count: usize, type_name: &str| -> String {
        (0..count)
            .map(|index| format!("def f{index:04} :: {type_name} = 1\n"))
            .collect()
    };
    // 700 changed (breaking), 700 added (not): 1400 changes, 1000 of them listed.
    let old = root("long-old", &[("Main.tz", &declarations(700, "i64"))]);
    let new = root(
        "long-new",
        &[(
            "Main.tz",
            &format!(
                "{}{}",
                declarations(700, "i32"),
                (700..1400)
                    .map(|index| format!("def f{index:04} :: i64 = 1\n"))
                    .collect::<String>()
            ),
        )],
    );
    let report = report(&old, &new);
    assert_eq!((report.breaking(), report.non_breaking()), (700, 700));
    let text = report.render_text();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1000 + 2, "{}", lines.len());
    assert!(
        lines[..700]
            .iter()
            .all(|line| line.starts_with("breaking: "))
    );
    assert!(
        lines[700..1000]
            .iter()
            .all(|line| line.starts_with("non-breaking: "))
    );
    assert_eq!(lines[1000], "note: 400 more changes not shown");
    assert_eq!(lines[1001], "summary: 700 breaking, 700 non-breaking");
    let json = report.render_json();
    assert_eq!(json.lines().count(), 1001);
    assert!(json.contains("\"breaking\":700,\"non_breaking\":700,\"omitted\":400"));
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
}
