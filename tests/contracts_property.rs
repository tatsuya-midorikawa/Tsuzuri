//! A21 Phase 3: property tests from contracts (`tsuzuri test --contracts test`), `Gen.for_all_where`,
//! the built-in class `Arbitrary` and `deriving (Arbitrary)`.

use std::path::{Path, PathBuf};
use std::process::Command;

use tsuzuri::analyze;

fn project(name: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-property-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

fn test(arguments: &[&str], root: &Path) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("test")
        .args(arguments)
        .arg(root)
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn for_all_where_discards_and_shrinks() {
    let root = project(
        "where",
        "def ordered :: (i64 * i64) -> Maybe<bool> = \\p ->\n    match p with\n    | (a, b) -> if a <= b then Maybe.Some (a == b) else Maybe.None\n\
         def gen_pair :: Gen<(i64 * i64)> = Arbitrary.arbitrary()\n\
         test \"broken\" = Gen.for_all_where (gen_pair()) ordered\n\
         test \"gives up\" = Gen.for_all_where (gen_pair()) (\\_p -> Maybe.None)\n",
    );
    let (code, stdout, _) = test(&[], &root);
    assert_eq!(code, 1);
    // The smallest pair with a < b: the shrinker lowers both values toward 0.
    assert!(stdout.contains("counterexample: (-1, 0)"), "{stdout}");
    assert!(
        stdout.contains("property gave up: 0 of 100 values were valid after 1000 tries"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn arbitrary_instances_and_derivations_generate_values() {
    let root = project(
        "arbitrary",
        "record Pt { x: i64, y: i64 } deriving (Arbitrary, Display, Eq)\n\
         union Shape =\n    | Circle of i64\n    | Rect of (i64 * i64)\n    | Empty\n    deriving (Arbitrary, Display)\n\
         def area :: Shape -> i64 = \\s ->\n    match s with\n    | Circle r -> r\n    | Rect (w, h) -> w + h\n    | Empty -> 0\n\
         test \"points\" = Gen.for_all (Arbitrary.arbitrary()) (\\(p: Pt) -> p == p)\n\
         test \"shapes\" = Gen.for_all (Arbitrary.arbitrary()) (\\(s: Shape) -> area s == area s)\n\
         test \"tuples\" = Gen.for_all (Arbitrary.arbitrary()) (\\(t: (i8u * bool * [i64])) -> match t with | (_, b, _) -> b || !b)\n",
    );
    let (code, stdout, stderr) = test(&[], &root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let _ = std::fs::remove_dir_all(root);
    let error =
        analyze("union T = Leaf | Node of (T * T) deriving (Arbitrary)\n").expect_err("recursive");
    assert_eq!(error.code, "E1025");
    assert!(
        error.message.contains("cannot build a recursive type"),
        "{}",
        error.message
    );
}

const CONTRACTED: &str = "def clamp :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r <= hi\n    if x < lo then lo elif x > hi then hi else x\n\
def broken :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r < hi\n    if x < lo then lo elif x > hi then hi else x\n\
def apply :: (i64 -> i64) -> i64 -> i64 = \\f x ->\n    @requires x > 0\n    f x\n\
def first :: ref ['a] -> i64 = \\xs ->\n    @requires xs.length > 0\n    xs.length\n\
test \"plain\" = assert (clamp 0 3 5 == 3)\n";

#[test]
fn contracts_test_generates_property_tests() {
    let root = project("generated", CONTRACTED);
    let (code, stdout, stderr) = test(&["--contracts", "test"], &root);
    assert_eq!(code, 1);
    assert!(stdout.contains("ok 2 - Main contract clamp"), "{stdout}");
    assert!(
        stdout.contains("not ok 3 - Main contract broken"),
        "{stdout}"
    );
    assert!(stdout.contains("counterexample: (0, (0, 0))"), "{stdout}");
    assert!(
        stderr.contains("no property test for 'Main.apply': parameter 'f' has type"),
        "{stderr}"
    );
    assert!(
        stderr.contains("no property test for 'Main.first': it is generic"),
        "{stderr}"
    );
    // The seed changes the values but not the verdicts.
    let (code, stdout, _) = test(&["--contracts", "test", "--seed", "7"], &root);
    assert_eq!(code, 1);
    assert!(stdout.contains("(seed 7)"), "{stdout}");
    // Without `--contracts test` nothing is generated.
    let (_, stdout, _) = test(&[], &root);
    assert!(!stdout.contains("contract clamp"), "{stdout}");
    let _ = std::fs::remove_dir_all(root);
}

/// Review of #64: `@old` in an `@ensures` line becomes a value taken before the generated call,
/// and the library API loads `Gen` for `test` mode without the sources naming it.
#[test]
fn generated_properties_take_old_values_before_the_call() {
    let source = "def grow :: i64 -> i64 = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\r -> r == @old (x) + 1\n    x + 1\n\
                  def shrink :: i64 -> i64 = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\r -> r == @old (x * 2) - x\n    x - 1\n";
    let root = project("old", source);
    let (code, stdout, stderr) = test(&["--contracts", "test"], &root);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(
        stdout.contains("ok 1 - Main contract grow"),
        "{stdout}{stderr}"
    );
    assert!(
        stdout.contains("not ok 2 - Main contract shrink"),
        "{stdout}{stderr}"
    );
    let _ = std::fs::remove_dir_all(root);
    let module = tsuzuri::analyze_modules_with_contracts(
        &[("Main", "def clamp :: i64 -> i64 = \\x ->\n    @ensures \\r -> r >= 0\n    if x < 0 then 0 else x\n")],
        tsuzuri::check::contracts::ContractMode::Test,
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(
        module
            .tests
            .iter()
            .any(|test| test.name == "contract clamp")
    );
}

/// Review of #64, round 2: the generated property runs the body where the `@ensures` lines still
/// read an owned argument that it only borrows; each line that names the result sees it, and a
/// line with `_` sees the parameter of the same name; `@old` inside a computation is resolved.
#[test]
fn generated_properties_keep_arguments_and_result_names() {
    let source = "def size :: own string -> i64 = \\s ->\n    @ensures \\r -> r == s.length\n    s.length\n\
                  def scoped :: i64 -> i64 = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\x -> x == 1000\n    @ensures \\_ -> x < 1000\n    @ensures \\x -> x > 999\n    1000\n\
                  def once :: i64 -> string = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\_ -> x < 1000\n    @ensures \\x -> x.length > 0 && x.length < 4\n    $\"{x}\"\n\
                  def label :: i64 -> string = \\x ->\n    @requires x >= 0\n    @ensures \\x -> x.length > 0\n    @ensures \\_ -> x >= 0\n    @ensures \\x -> x.length < 30\n    $\"{x}\"\n\
                  def boxed :: i64 -> i64 = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\r -> { let m = Maybe { return @old (x) + 1 }; Maybe.default_value 0 m == r }\n    x + 1\n\
                  def narrow :: i8 -> i8 = \\x ->\n    @requires x > 0i8\n    @ensures \\r -> r == 1i8\n    1i8\n";
    let root = project("scopes", source);
    let (code, stdout, stderr) = test(&["--contracts", "test"], &root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    for name in ["size", "scoped", "once", "boxed", "narrow"] {
        assert!(
            stdout.contains(&format!(" - Main contract {name}\n")),
            "{name}\n{stdout}{stderr}"
        );
    }
    // Two runs of lines that name a string result would each move it.
    assert!(
        stderr.contains("no property test for 'Main.label': its '@ensures' lines name the result 'x' on both sides of a line that reads another 'x'"),
        "{stderr}"
    );
    assert!(stdout.contains("5 passed; 0 failed"), "{stdout}");
    let _ = std::fs::remove_dir_all(root);
}

/// Review 4 of #64: the generated property passes the generated arguments to the parameters as a
/// call does, so a parameter that the function borrows by default (`pure def`, edition 2027) is a
/// reference in the body and the lines, as in `check`.
#[test]
fn generated_properties_borrow_the_parameters_that_the_function_borrows() {
    let source = "pure def size :: string -> i64 = \\s ->\n    @ensures \\r -> r == (deref s).length\n    (deref s).length\n\
                  def count :: [i64] -> i64 = \\xs ->\n    @requires (deref xs).length < 100\n    @ensures \\r -> r >= 0\n    (deref xs).length\n\
                  def plain :: i64 -> string -> i64 = \\n s ->\n    @requires n >= 0 && n < 1000\n    @ensures \\r -> r >= n\n    n + s.length\n";
    let root = project("borrowed", source);
    let (code, stdout, stderr) = test(&["--contracts", "test"], &root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    for name in ["size", "count", "plain"] {
        assert!(
            stdout.contains(&format!(" - Main contract {name}\n")),
            "{name}\n{stdout}{stderr}"
        );
    }
    assert!(stdout.contains("3 passed; 0 failed"), "{stdout}");
    let _ = std::fs::remove_dir_all(root);
}

/// Review 5 of #64: the Project API loads what the generated property tests use (`Gen`) in
/// `test` mode, into the project's own sources, so the spans of the module and the diagnostics
/// index them; the command line does not add it separately.
#[test]
fn the_project_api_loads_gen_for_test_mode() {
    let root = project(
        "api",
        "def grow :: i64 -> i64 = \\x ->\n    @requires x >= 0 && x < 1000\n    @ensures \\r -> r > x\n    x + 1\n",
    );
    let mut loaded = tsuzuri::driver::Project::load_for_tests(&root).unwrap();
    let module = loaded
        .analyze_cached_with_contracts(
            false,
            tsuzuri::driver::AnalysisKind::Tests,
            tsuzuri::check::contracts::ContractMode::Test,
        )
        .unwrap_or_else(|errors| panic!("{:?}", errors.diagnostics));
    assert!(module.tests.iter().any(|test| test.name == "contract grow"));
    let for_all = module
        .functions
        .iter()
        .find(|function| function.module == "Gen")
        .expect("Gen is loaded");
    let source = loaded.source_for_span(for_all.span);
    assert!(source.path.ends_with("Gen.tz"), "{}", source.path.display());
    let _ = std::fs::remove_dir_all(root);
}

/// Review 5 of #64: an ignored parameter (`fn f _ =`) is bound and passed under a generated name.
#[test]
fn generated_properties_take_ignored_parameters() {
    let source = "def zero :: i64 -> i64\nfn zero _ =\n    @ensures \\r -> r == 0\n    0\n\
                  def second :: i64 -> i64 -> i64\nfn second _ y =\n    @requires y >= 0\n    @ensures \\r -> r == y\n    y\n";
    let root = project("ignored", source);
    let (code, stdout, stderr) = test(&["--contracts", "test"], &root);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("2 passed; 0 failed"), "{stdout}{stderr}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn contract_warnings_name_useless_lines() {
    let module = analyze(
        "def f :: i64 -> i64 = \\x ->\n    @requires true\n    @ensures \\r -> x > 0\n    @ensures \\_ -> true\n    x\n",
    )
    .unwrap();
    let kinds: Vec<_> = module
        .warnings
        .iter()
        .filter(|warning| warning.code == "W1009")
        .map(|warning| warning.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            Some("contract_always_true"),
            Some("contract_result_unused"),
            Some("contract_always_true")
        ]
    );
}

#[test]
fn doc_shows_the_contract_lines() {
    let program = tsuzuri::parser::parse(
        "def clamp :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r <= hi\n    if x < lo then lo elif x > hi then hi else x\n",
    )
    .unwrap();
    let page = tsuzuri::docgen::render_module("Main", &program);
    assert!(
        page.contains("```tsuzuri\ndef clamp :: i64 -> i64 -> i64 -> i64\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r <= hi\n```"),
        "{page}"
    );
}
