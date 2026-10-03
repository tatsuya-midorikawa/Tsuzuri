use tsuzuri::check::{CheckedModule, ModuleOrigin};
use tsuzuri::copies::{CopyCost, CopyKind, CopySite};
use tsuzuri::diagnostic::{Diagnostic, Severity, Span};
use tsuzuri::llvm;

const ARRAY: &str = "implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit";
const LIST: &str = "implicit copy of a list allocates a new node for every element; borrow it with 'ref', or call 'List.copy' to make the copy explicit";
const AGGREGATE: &str = "implicit copy of a value that contains arrays or lists copies all of them; borrow it with 'ref', or use the value only once so that it moves";

const REUSED: &str = "let a = [1, 2, 3]\nlet b = a\nArray.length (ref a) + Array.length (ref b)";
const MOVED: &str = "let a = [1, 2, 3]\nlet b = a\nArray.length (ref b)";
const MOVED_TWICE: &str = "let a = [1, 2, 3]\nlet b = a\nlet c = b\nArray.length (ref c)";
const SCALAR: &str = "let n = 5\nlet m = n\nn + m";
const SCALAR_RECORD: &str =
    "record Point { value: i64 }\nlet p = Point { value: 1 }\nlet q = p\np.value + q.value";
const FIELD: &str = "record Bag { items: [i64] }\nlet bag = Bag { items: [1, 2] }\nlet xs = bag.items\nlet ys = bag.items\nArray.length (ref xs) + Array.length (ref ys)";
const LIST_REUSED: &str =
    "let a = [|1, 2, 3|]\nlet b = a\nList.length (ref a) + List.length (ref b)";
const ARGUMENT_KEPT: &str = "def total :: [i64] -> i64\nfn total values = Array.length (ref values)\nlet a = [1, 2]\ntotal a + Array.length (ref a)";
const ARGUMENT_MOVED: &str = "def total :: [i64] -> i64\nfn total values = Array.length (ref values)\nlet a = [1, 2]\ntotal a";
const AGGREGATE_COPY: &str = "record Bag { items: [i64] }\nlet bag = Bag { items: [1] }\nlet other = bag\nArray.length (ref bag.items) + Array.length (ref other.items)";
const DEREFERENCE: &str = "def copy_array :: ref [i64] -> [i64]\nfn copy_array values = *values\nlet a = [1, 2, 3]\nlet b = copy_array (ref a)\nArray.length (ref a) + Array.length (ref b)";
const CLOSURE: &str = "let offset = 1\nlet add = x -> x + offset\nlet f = add\nf 1 + add 2";
const SPECIALIZED: &str = "def dup :: Copy<'a> => 'a -> ('a * 'a)\nfn dup value = (value, value)\nlet a = dup [1, 2]\nlet b = dup [1.5]\n0";
const EXPLICIT: &str =
    "let a = [1, 2, 3]\nlet b = Array.copy (ref a)\nArray.length (ref a) + Array.length (ref b)";
const EXPLICIT_LIST: &str =
    "let a = [|1, 2, 3|]\nlet b = List.copy (ref a)\nList.length (ref a) + List.length (ref b)";
const BINDING: &str =
    "let pair = ([1, 2], 3)\nmatch pair with\n| (items, n) -> Array.length (ref items) + n";
const ELEMENT: &str = "let rows = [[1, 2], [3]]\nlet mut total = 0\nfor row in rows do\n    let kept = row\n    total = total + Array.length (ref kept)\ntotal";
const DERIVED: &str = "record Bag { items: [i64], tags: [|i64|] } deriving (Eq, Ord, Default, Hash, Display)\nlet a = Bag { items: [1, 2], tags: [|3|] }\nlet b: Bag = Default.default()\nlet same = if a == b then 1 else 0\nlet order = if a < b then 1 else 0\nlet shown = Display.display a\nsame + order + shown.length + (Hash.hash (ref a) as i64)";
const OWNED: &str = "let items = [1, 2, 3]\nlet count = Owned.function (\\extra -> { let kept = items; Array.length (ref kept) + extra })\nOwned.call (ref count) 1";

fn module(source: &str) -> CheckedModule {
    tsuzuri::analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"))
}

fn user_sites(source: &str) -> Vec<CopySite> {
    let module = module(source);
    tsuzuri::copies::sites(&module)
        .into_iter()
        .filter(|site| module.functions[site.function].origin.module == ModuleOrigin::User)
        .collect()
}

fn copy_warnings(source: &str) -> Vec<Diagnostic> {
    tsuzuri::copies::warnings(&module(source))
}

fn text(source: &str, span: Span) -> &str {
    &source[span.start..span.end]
}

fn assert_warnings(source: &str, expected: &[(&str, &str)]) -> Vec<Diagnostic> {
    let warnings = copy_warnings(source);
    assert_eq!(warnings.len(), expected.len(), "{source}\n{warnings:?}");
    for (warning, (spelled, message)) in warnings.iter().zip(expected) {
        assert_eq!(warning.code, "W1006");
        assert_eq!(warning.severity, Severity::Warning);
        assert_eq!(warning.message, *message);
        assert_eq!(text(source, warning.span), *spelled, "{source}");
    }
    warnings
}

#[test]
fn reports_array_copy_when_the_source_is_used_again() {
    let sites = user_sites(REUSED);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].kind, CopyKind::Local);
    assert_eq!(sites[0].cost, CopyCost::Length);
    assert_eq!(text(REUSED, sites[0].span), "a");
    let warnings = assert_warnings(REUSED, &[("a", ARRAY)]);
    assert_eq!(warnings[0].span.start, REUSED.find("= a").unwrap() + 2);
}

#[test]
fn does_not_report_after_a_move() {
    for source in [MOVED, MOVED_TWICE] {
        assert!(user_sites(source).is_empty(), "{source}");
        assert_warnings(source, &[]);
    }
}

#[test]
fn never_reports_copy_scalars_or_scalar_records() {
    for source in [SCALAR, SCALAR_RECORD] {
        assert!(user_sites(source).is_empty(), "{source}");
        assert_warnings(source, &[]);
    }
}

#[test]
fn reports_each_take_of_an_array_field() {
    let sites = user_sites(FIELD);
    assert_eq!(sites.len(), 2, "{sites:?}");
    assert!(sites.iter().all(|site| site.kind == CopyKind::Field));
    assert_warnings(FIELD, &[("bag.items", ARRAY), ("bag.items", ARRAY)]);
}

#[test]
fn reports_list_copy_with_the_list_message() {
    assert_warnings(LIST_REUSED, &[("a", LIST)]);
}

#[test]
fn reports_argument_copy_only_when_the_caller_keeps_the_value() {
    let warnings = assert_warnings(ARGUMENT_KEPT, &[("a", ARRAY)]);
    assert_eq!(
        warnings[0].span.start,
        ARGUMENT_KEPT.find("total a").unwrap() + 6
    );
    assert_warnings(ARGUMENT_MOVED, &[]);
}

#[test]
fn reports_aggregate_copy_with_the_aggregate_message() {
    let warnings = assert_warnings(AGGREGATE_COPY, &[("bag", AGGREGATE)]);
    assert_eq!(
        warnings[0].span.start,
        AGGREGATE_COPY.find("= bag").unwrap() + 2
    );
}

#[test]
fn reports_dereference_copy() {
    let sites = user_sites(DEREFERENCE);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].kind, CopyKind::Dereference);
    assert_eq!(text(DEREFERENCE, sites[0].span), "*values");
    assert_warnings(DEREFERENCE, &[("*values", ARRAY)]);
}

#[test]
fn closure_copies_are_listed_but_not_reported() {
    let sites = user_sites(CLOSURE);
    let start = CLOSURE.find("= add").unwrap() + 2;
    assert!(
        sites.iter().any(|site| site.cost == CopyCost::Environment
            && site.span.start == start
            && text(CLOSURE, site.span) == "add"),
        "{sites:?}"
    );
    assert!(sites.iter().all(|site| site.cost == CopyCost::Environment));
    assert_warnings(CLOSURE, &[]);
}

#[test]
fn specializations_report_one_warning_per_source_position() {
    let sites = user_sites(SPECIALIZED);
    assert_eq!(sites.len(), 4, "{sites:?}");
    assert!(
        sites
            .iter()
            .all(|site| text(SPECIALIZED, site.span) == "value")
    );
    let first = SPECIALIZED.find("(value").unwrap() + 1;
    let second = SPECIALIZED.find(", value)").unwrap() + 2;
    let warnings = assert_warnings(SPECIALIZED, &[("value", ARRAY), ("value", ARRAY)]);
    assert_eq!(warnings[0].span.start, first);
    assert_eq!(warnings[1].span.start, second);
}

#[test]
fn explicit_copy_api_is_listed_in_std_but_not_reported() {
    for source in [EXPLICIT, EXPLICIT_LIST] {
        assert!(user_sites(source).is_empty(), "{source}");
        assert_warnings(source, &[]);
        let module = module(source);
        assert!(
            tsuzuri::copies::sites(&module).iter().any(|site| {
                module.functions[site.function].origin.module == ModuleOrigin::Std
                    && site.kind == CopyKind::Dereference
                    && site.cost == CopyCost::Length
            }),
            "{source}"
        );
    }
}

#[test]
fn pattern_bindings_and_loop_elements_copy_at_their_names() {
    let sites = user_sites(BINDING);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].kind, CopyKind::Field);
    assert_eq!(sites[0].span.start, BINDING.find("items,").unwrap());
    assert_warnings(BINDING, &[("items", ARRAY)]);
    let sites = user_sites(ELEMENT);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].span.start, ELEMENT.find("= row").unwrap() + 2);
    assert_warnings(ELEMENT, &[("row", ARRAY)]);
}

#[test]
fn derived_instances_and_owned_functions_follow_their_bodies() {
    assert!(user_sites(DERIVED).is_empty());
    assert_warnings(DERIVED, &[]);
    let warnings = assert_warnings(OWNED, &[("items", ARRAY)]);
    assert_eq!(warnings[0].span.start, OWNED.find("= items").unwrap() + 2);
}

#[test]
fn inventory_covers_every_emitted_clone() {
    for source in [
        REUSED,
        MOVED,
        MOVED_TWICE,
        SCALAR,
        SCALAR_RECORD,
        FIELD,
        LIST_REUSED,
        ARGUMENT_KEPT,
        ARGUMENT_MOVED,
        AGGREGATE_COPY,
        DEREFERENCE,
        CLOSURE,
        SPECIALIZED,
        EXPLICIT,
        EXPLICIT_LIST,
        BINDING,
        ELEMENT,
        DERIVED,
        OWNED,
    ] {
        let module = module(source);
        // Debug builds check every emitted clone against `copies::sites` while emitting.
        for wasm in [false, true] {
            let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
            assert_eq!(
                ir,
                llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
            );
        }
    }
}

#[test]
fn cli_warn_implicit_copy_is_opt_in_deniable_and_ir_neutral() {
    use std::{
        fs,
        process::{Command, Output},
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-copy-cost-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let input = root.join("Main.tz");
    fs::write(&input, REUSED).unwrap();
    let run = |action: &str, options: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg(action)
            .arg(&input)
            .args(options)
            .output()
            .unwrap()
    };
    let output = run("check", &["--json"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("W1006"));
    let output = run("check", &["--json", "--warn", "implicit-copy"]);
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.matches("\"code\":\"W1006\"").count(), 1, "{stderr}");
    let output = run(
        "check",
        &["--json", "--warn", "implicit-copy", "--deny-warnings"],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    for (action, options) in [
        ("check", vec!["--warn"]),
        ("check", vec!["--warn", "shadowing"]),
        (
            "check",
            vec!["--warn", "implicit-copy", "--warn", "implicit-copy"],
        ),
        ("fmt", vec!["--warn", "implicit-copy"]),
    ] {
        let output = run(action, &options);
        assert_eq!(output.status.code(), Some(2), "{options:?}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E2000"),
            "{output:?}"
        );
    }
    let plain = root.join("plain.ll");
    let warned = root.join("warned.ll");
    for (path, warn) in [(&plain, false), (&warned, true)] {
        let mut options = vec!["--emit", "llvm", "--no-cache", "-o", path.to_str().unwrap()];
        if warn {
            options.extend(["--warn", "implicit-copy"]);
        }
        let output = run("build", &options);
        assert!(output.status.success(), "{output:?}");
    }
    assert_eq!(fs::read(&plain).unwrap(), fs::read(&warned).unwrap());
    fs::remove_dir_all(root).unwrap();
}
