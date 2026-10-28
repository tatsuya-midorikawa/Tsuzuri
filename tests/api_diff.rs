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
    report_mode(old, new, false)
}

fn report_mode(old: &Path, new: &Path, effective: bool) -> Report {
    let api = |path: &Path| {
        let project = if effective {
            Project::load_for_docs(path)
        } else {
            Project::load_for_api(path)
        }
        .unwrap();
        let api = if effective {
            api_diff::effective_api(&project)
        } else {
            api_diff::api(&project)
        };
        api.unwrap_or_else(|error| panic!("{path:?}: {error:?}"))
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
    compare_mode(old, new, others, false)
}

fn compare_mode(
    old: &str,
    new: &str,
    others: &[(&str, &str, &str)],
    effective: bool,
) -> Vec<String> {
    let mut old_files = vec![("Main.tz", old)];
    let mut new_files = vec![("Main.tz", new)];
    for &(file, before, after) in others {
        old_files.push((file, before));
        new_files.push((file, after));
    }
    let (old, new) = (root("old", &old_files), root("new", &new_files));
    let lines = summary(&report_mode(&old, &new, effective));
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
    lines
}

#[test]
fn effective_pure_additions_preserve_generic_specialization_obligations() {
    let old = "def inspect :: own 'a -> i64 = \\value -> 0\n";
    let new = format!("pure {old}");
    tsuzuri::analyze(&format!("{old}inspect (Rc.new 1)\n")).unwrap();
    assert_eq!(
        tsuzuri::analyze(&format!("{new}inspect (Rc.new 1)\n"))
            .unwrap_err()
            .code,
        "E1029"
    );
    tsuzuri::analyze(&format!("{new}inspect 1\n")).unwrap();
    let borrowed = old.replace("own 'a", "ref 'a");
    let callback = "let callback: i64 -> i64 = \\x -> x\ninspect (ref callback)\n";
    tsuzuri::analyze(&format!("{borrowed}{callback}")).unwrap();
    assert_eq!(
        tsuzuri::analyze(&format!("pure {borrowed}{callback}"))
            .unwrap_err()
            .code,
        "E1029"
    );
    assert_eq!(
        compare_mode(old, &new, &[], true),
        ["breaking changed def Main.inspect"]
    );
    assert_eq!(
        compare(old, &new, &[]),
        ["non-breaking changed def Main.inspect"]
    );
    assert!(compare_mode(&new, &new.replace("'a", "'element"), &[], true).is_empty());
    for (prefix, parameter, breaking) in [
        ("", "own ['a]", true),
        ("record Box<'a> { value: 'a }\n", "own Box<'a>", true),
        ("type Payload<'a> = ['a]\n", "own Payload<'a>", true),
        ("", "ref 'a", true),
        ("record Box<'a> { value: 'a }\n", "ref Box<'a>", true),
        ("", "ref [i64]", false),
        ("", "pure ('a -> i64)", true),
        ("", "own [i64]", false),
        ("record Box { value: i64 }\n", "own Box", false),
    ] {
        let before = format!("{prefix}def inspect :: {parameter} -> i64 = \\value -> 0\n");
        let after = before.replace("def inspect", "pure def inspect");
        assert_eq!(
            compare_mode(&before, &after, &[], true),
            [format!(
                "{} changed def Main.inspect",
                if breaking { "breaking" } else { "non-breaking" }
            )],
            "{parameter}",
        );
    }
    let result = "def inspect :: unit -> Maybe<'a> = \\() -> None\n";
    assert_eq!(
        compare_mode(result, &format!("pure {result}"), &[], true),
        ["breaking changed def Main.inspect"]
    );
    let prefix = "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\n";
    let caller = "let result: Maybe<Ticket> = inspect ()\n0\n";
    tsuzuri::analyze(&format!("{prefix}{result}{caller}")).unwrap();
    let error = tsuzuri::analyze(&format!("{prefix}pure {result}{caller}"))
        .expect_err("generic result specialization must retain the purity proof obligation");
    assert_eq!(error.code, "E1029");
}

#[test]
fn effective_pure_callback_obligations_reach_ordinary_generic_functions() {
    for (before, caller) in [
        (
            "def f :: own 'a -> i64 = \\x -> { let k: 'a -> i64 = \\_ -> 0; k x }\n",
            "f (Rc.new 1)\n",
        ),
        (
            "private def helper :: own 'a -> i64 = \\x -> { let k: 'a -> i64 = \\_ -> 0; k x }\ndef f :: own 'a -> i64 = \\x -> helper x\n",
            "f (Rc.new 1)\n",
        ),
        (
            "def f :: ref 'a -> i64 = \\x -> { let k: unit -> i64 = \\() -> { x; 0 }; k () }\n",
            "let callback: i64 -> i64 = \\x -> x\nf (ref callback)\n",
        ),
        (
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\ndef f :: own 'a -> i64 = \\_ -> { let k: unit -> i64 = \\() -> { let value: Maybe<'a> = None; 0 }; k () }\n",
            "f (Ticket { id: 1 })\n",
        ),
        (
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\ndef f :: own 'a -> i64 = \\_ -> { let k: unit -> i64 = \\() -> { let t = task { let value: Maybe<'a> = None; 0 }; Task.run t }; k () }\n",
            "f (Ticket { id: 1 })\n",
        ),
        (
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\ndef f :: own 'a -> i64 = \\_ -> { let k: unit -> i64 = \\() -> { let values = Parallel.map (\\x -> { let value: Maybe<'a> = None; x }) [1]; values.length }; k () }\n",
            "f (Ticket { id: 1 })\n",
        ),
        (
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\ndef f :: own 'a -> i64 = \\_ -> { let k: unit -> i64 = \\() -> { let run = \\() -> { let value: Maybe<'a> = None; 0 }; run () }; k () }\n",
            "f (Ticket { id: 1 })\n",
        ),
    ] {
        let after = before
            .replace("let k: 'a -> i64", "let k: pure ('a -> i64)")
            .replace("let k: unit -> i64", "let k: pure (unit -> i64)");
        tsuzuri::analyze(&format!("{before}{caller}")).unwrap();
        let Err(error) = tsuzuri::analyze(&format!("{after}{caller}")) else {
            panic!("expected a new purity restriction: {after}");
        };
        assert_eq!(error.code, "E1029", "{after}");
        let changes = compare_mode(before, &after, &[], true);
        assert!(
            changes.contains(&"breaking changed def Main.f".to_owned()),
            "{changes:?}: {after}"
        );
        assert!(compare(before, &after, &[]).is_empty());
    }
    for call in ["helper ()", "alias ()"] {
        let before = format!(
            "record Ticket {{ id: i64 }}\ninstance Drop<Ticket> {{ fn drop value = () }}\ndef f :: own 'a -> i64 = \\_ -> {{ let helper: unit -> i64 = \\() -> {{ let value: Maybe<'a> = None; 0 }}; let middle = helper; let alias = middle; let k: unit -> i64 = \\() -> {call}; k () }}\n"
        );
        let after = before.replace("let k: unit -> i64", "let k: pure (unit -> i64)");
        let changes = compare_mode(&before, &after, &[], true);
        assert!(
            changes.contains(&"breaking changed def Main.f".to_owned()),
            "{changes:?}: {after}"
        );
    }
    for before in [
        "def f :: own 'a -> i64 = \\_ -> { let k: i64 -> i64 = \\x -> x; k 1 }\n",
        "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\ndef f :: own 'a -> i64 = \\_ -> { let k: unit -> (unit -> i64) = \\() -> \\() -> { let value: Maybe<'a> = None; 0 }; let ordinary = k (); ordinary () }\n",
    ] {
        let after = before
            .replace("let k: i64 -> i64", "let k: pure (i64 -> i64)")
            .replace(
                "let k: unit -> (unit -> i64)",
                "let k: pure (unit -> (unit -> i64))",
            );
        assert!(compare_mode(before, &after, &[], true).is_empty());
        if before.starts_with("record") {
            tsuzuri::analyze(&format!("{after}f (Ticket {{ id: 1 }})\n")).unwrap();
        }
    }
    let first =
        "def f :: own 'a -> own 'b -> i64 = \\x y -> { let k: pure ('a -> i64) = \\_ -> 0; k x }\n";
    let second = first.replace("pure ('a", "pure ('b").replace("k x", "k y");
    assert_eq!(
        compare_mode(first, &second, &[], true),
        ["breaking changed def Main.f"]
    );
    assert!(compare_mode(first, &first.replace("'a", "'z"), &[], true).is_empty());
}

#[test]
fn effective_private_helper_purity_obligations_reach_public_wrappers() {
    let helper = "private def helper :: own 'a -> i64 = \\value -> 0\n";
    for wrapper in [
        "def f :: own 'a -> i64 = \\value -> helper value\n",
        "def f :: own 'a -> i64 = \\value -> helper (Some value)\n",
        "def f :: own 'a -> i64 = \\value -> { let run = \\item -> helper item; run value }\n",
        "private def middle :: own 'a -> i64 = \\value -> helper value\ndef f :: own 'a -> i64 = \\value -> middle value\n",
        "private def rec left :: i64 -> own 'a -> i64 = \\n value -> if n == 0 then helper value else right (n - 1) value\nprivate def rec right :: i64 -> own 'a -> i64 = \\n value -> left n value\ndef f :: own 'a -> i64 = \\value -> right 0 value\n",
    ] {
        let before = format!("{helper}{wrapper}");
        let after = before.replace("private def helper", "private pure def helper");
        tsuzuri::analyze(&format!("{before}f (Rc.new 1)\n")).unwrap();
        assert_eq!(
            tsuzuri::analyze(&format!("{after}f (Rc.new 1)\n"))
                .unwrap_err()
                .code,
            "E1029"
        );
        assert_eq!(
            compare_mode(&before, &after, &[], true),
            ["breaking changed def Main.f"],
            "{wrapper}"
        );
        assert!(compare(&before, &after, &[]).is_empty());
        assert!(compare_mode(&after, &after.replace("'a", "'element"), &[], true).is_empty());
        let reordered = format!(
            "{wrapper}{}",
            helper.replace("private def", "private pure def")
        );
        assert!(compare_mode(&after, &reordered, &[], true).is_empty());
        tsuzuri::analyze(&format!("{after}f 1\n")).unwrap();
    }
}

#[test]
fn effective_private_helper_purity_includes_shared_inputs_and_generic_results() {
    for (before, caller) in [
        (
            "private def helper :: ref 'a -> i64 = \\value -> 0\ndef f :: ref 'a -> i64 = \\value -> helper value\n",
            "let callback: i64 -> i64 = \\x -> x\nf (ref callback)\n",
        ),
        (
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\nprivate def helper :: unit -> Maybe<'a> = \\() -> None\ndef f :: unit -> Maybe<'a> = \\() -> helper ()\n",
            "let value: Maybe<Ticket> = f ()\n0\n",
        ),
    ] {
        let after = before.replace("private def helper", "private pure def helper");
        tsuzuri::analyze(&format!("{before}{caller}")).unwrap();
        assert_eq!(
            tsuzuri::analyze(&format!("{after}{caller}"))
                .unwrap_err()
                .code,
            "E1029"
        );
        assert_eq!(
            compare_mode(before, &after, &[], true),
            ["breaking changed def Main.f"]
        );
    }
}

#[test]
fn effective_helper_purity_tracks_substitutions_not_unrelated_parameters() {
    for before in [
        "private def helper :: own 'a -> i64 = \\value -> 0\nprivate def middle :: own 'a -> i64 = \\value -> helper 1\ndef f :: own 'a -> i64 = \\value -> middle value\n",
        "private def helper :: own 'a -> ref 'b -> i64 = \\x y -> 0\nprivate def middle :: own 'a -> ref 'b -> i64 = \\x y -> helper 1 y\ndef f :: own 'a -> i64 = \\value -> { let y = 1; middle value (ref y) }\n",
    ] {
        let after = before.replace("private def helper", "private pure def helper");
        tsuzuri::analyze(&format!("{after}f (Rc.new 1)\n")).unwrap();
        assert!(compare_mode(before, &after, &[], true).is_empty());
    }
    let before = "private pure def helper :: own 'a -> i64 = \\value -> 0\ndef f :: own 'a -> own 'b -> i64 = \\first second -> helper first\n";
    let after = before.replace("helper first", "helper second");
    assert_eq!(
        compare_mode(before, &after, &[], true),
        ["breaking changed def Main.f"]
    );
    let renamed = before.replace("'a", "'z").replace("'b", "'m");
    assert!(compare_mode(before, &renamed, &[], true).is_empty());
    assert!(compare(before, &after, &[]).is_empty());
}

#[test]
fn effective_concrete_pure_helper_calls_are_proved_before_discharge() {
    let class = "class C<'a> { def value :: ref 'a -> i64 }\nclass D<'a> { def consume :: own 'a -> i64 }\n";
    for (helper, body) in [
        (
            "private pure def enforce :: own 'a -> i64 = \\value -> 0\nprivate def helper :: own 'a -> i64 = \\value -> enforce value\n",
            "helper (Rc.new 1)",
        ),
        (
            "record Box<'a> { value: 'a }\nprivate pure def enforce :: own 'a -> i64 = \\value -> 0\ninstance Traits.D<Box<'a>> { fn consume box = enforce box.value }\n",
            "Traits.D.consume (Box { value: Rc.new 1 })",
        ),
        (
            "private pure def helper :: own 'a -> i64 = \\value -> 0\n",
            "helper (Rc.new 1)",
        ),
        (
            "private pure def helper :: ref 'a -> i64 = \\value -> 0\n",
            "{ let callback: i64 -> i64 = \\x -> x; helper (ref callback) }",
        ),
        (
            "instance Traits.C<i64> { fn value _item = { Debug.print (ref 1); 0 } }\nprivate pure def helper :: ref 'a -> i64\n  @'a : Traits.C\nfn helper value = Traits.C.value value\n",
            "{ let value = 1i64; helper (ref value) }",
        ),
    ] {
        let source = format!("{helper}def f :: own 'a -> i64 = \\_ -> {body}\n");
        // Ordinary compilation still defers an unused generic wrapper, unlike explicit API inspection.
        tsuzuri::analyze(&format!("{class}{}", source.replace("Traits.", ""))).unwrap();
        let directory = root(
            "concrete-pure-proof",
            &[("Main.tz", &source), ("Traits.tt", class)],
        );
        let project = Project::load_for_docs(&directory).unwrap();
        let Err(error) = api_diff::effective_api(&project) else {
            panic!("concrete helper specialization must be proved: {source}");
        };
        assert_eq!(error.code, "E1029", "{error:?}\n{source}");
        std::fs::remove_dir_all(directory).unwrap();
    }
    let before = "instance Traits.C<i64> { fn value _item = 0 }\nprivate def helper :: ref 'a -> i64\n  @'a : Traits.C\nfn helper value = Traits.C.value value\ndef f :: own 'a -> i64 = \\_ -> { let value = 1i64; helper (ref value) }\n";
    let after = before.replace("private def helper", "private pure def helper");
    assert!(compare_mode(before, &after, &[("Traits.tt", class, class)], true).is_empty());
}

#[test]
fn effective_class_defaults_and_instances_retain_helper_obligations() {
    let class = "class C<'a> { def value :: own 'a -> i64 }\n";
    let before = "record Box<'a> { value: 'a }\nprivate def helper :: own 'a -> i64 = \\value -> 0\ninstance Traits.C<Box<'a>> { fn value box = helper box.value }\ndef f :: own Box<'a> -> i64 = \\box -> Traits.C.value box\n";
    let after = before.replace("private def helper", "private pure def helper");
    let caller = "f (Box { value: Rc.new 1 })\n";
    tsuzuri::analyze(&format!("{class}{}{caller}", before.replace("Traits.", ""))).unwrap();
    assert_eq!(
        tsuzuri::analyze(&format!("{class}{}{caller}", after.replace("Traits.", "")))
            .unwrap_err()
            .code,
        "E1029"
    );
    let changes = compare_mode(before, &after, &[("Traits.tt", class, class)], true);
    assert!(
        changes
            .iter()
            .any(|line| line.starts_with("breaking changed instance ")),
        "{changes:?}"
    );
    assert!(compare(before, &after, &[("Traits.tt", class, class)]).is_empty());
    assert!(
        compare_mode(
            &after,
            &after.replace("'a", "'element"),
            &[("Traits.tt", class, class)],
            true
        )
        .is_empty()
    );

    let two_methods = "class C<'a> { def value :: own 'a -> i64; def stable :: own 'a -> i64 }\n";
    let reordered = "class C<'a> { def stable :: own 'a -> i64; def value :: own 'a -> i64 }\n";
    let with_stable = after.replace(
        "helper box.value }",
        "helper box.value; fn stable box = 0 }",
    );
    assert!(
        compare_mode(
            &with_stable,
            &with_stable,
            &[("Traits.tt", two_methods, reordered)],
            true
        )
        .is_empty()
    );

    let helper = "def helper :: own 'a -> i64 = \\value -> 0\n";
    let pure_helper = format!("pure {helper}");
    let default =
        "class C<'a> { def value :: own 'a -> i64; fn value value = Main.helper value }\n";
    let changes = compare_mode(
        helper,
        &pure_helper,
        &[("Traits.tt", default, default)],
        true,
    );
    assert!(
        changes.contains(&"breaking changed method Traits.C.value".to_owned()),
        "{changes:?}"
    );
    assert_eq!(
        compare_mode(
            &pure_helper,
            &pure_helper,
            &[("Traits.tt", class, default)],
            true
        ),
        ["non-breaking changed method Traits.C.value"]
    );
    let instances = "instance Traits.C<i64> {}\ninstance Traits.C<'a> => Traits.C<Maybe<'a>> { fn value item = 0 }\n";
    let with_instances = format!("{helper}{instances}");
    let changes = compare_mode(
        &with_instances,
        &with_instances.replace("def helper", "pure def helper"),
        &[("Traits.tt", default, default)],
        true,
    );
    assert!(
        !changes
            .iter()
            .any(|line| line.starts_with("breaking changed instance ")),
        "{changes:?}"
    );
}

#[test]
fn effective_inline_and_alias_constraints_are_part_of_the_callers_contract() {
    let old = "def f :: 'a -> bool = \\x -> true\n";
    let new = "def f :: Ord<'a> -> bool = \\x -> true\n";
    for (before, after) in [(old, new), (new, old)] {
        assert_eq!(
            compare_mode(before, after, &[], true),
            ["breaking changed def Main.f"]
        );
    }
    let alias = "type Argument<'a> = 'a\ndef f :: Argument<'a> -> bool = \\x -> true\n";
    let constrained_alias = alias.replace("= 'a", "= Ord<'a>");
    assert!(
        compare_mode(alias, &constrained_alias, &[], true)
            .contains(&"breaking changed def Main.f".to_owned())
    );
}

#[test]
fn effective_class_constraints_are_a_canonical_set_without_losing_members() {
    let inline = "def f :: Ord<'a> -> bool = \\x -> true\n";
    let explicit = "def f :: 'element -> bool\n  @'element : Ord\nfn f x = true\n";
    let duplicate = "def f :: Ord<'value> -> bool\n  @'value : Ord, Ord\nfn f x = true\n";
    assert!(compare_mode(inline, explicit, &[], true).is_empty());
    assert!(compare_mode(explicit, duplicate, &[], true).is_empty());
    let repeated = explicit.replace(": Ord", ": Ord, Ord");
    assert!(compare_mode(explicit, &repeated, &[], true).is_empty());
    assert_eq!(
        compare(explicit, &repeated, &[]),
        ["breaking changed def Main.f"]
    );
    assert!(compare_mode(inline, &inline.replace("true", "x == x"), &[], true).is_empty());
    let two = "def f :: Ord<'a> -> Eq<'b> -> bool = \\x y -> true\n";
    let reordered = "def f :: 'first -> 'second -> bool\n  @'second : Eq\n  @'first : Ord, Eq\nfn f x y = true\n";
    assert!(compare_mode(two, reordered, &[], true).is_empty());
    let class = "class Label<'a> { def label :: 'a -> bool }\n";
    let local = "def f :: Traits.Label<'a> -> bool = \\x -> true\n";
    let local_explicit = "def f :: 'item -> bool\n  @'item : Traits.Label\nfn f x = true\n";
    assert!(compare_mode(local, local_explicit, &[("Traits.tt", class, class)], true).is_empty());
    let member = "def f :: Ord<'a> -> bool\n  @'a : #distance\nfn f x = true\n";
    assert_eq!(
        compare_mode(inline, member, &[], true),
        ["breaking changed def Main.f"]
    );
}

#[test]
fn effective_member_requirements_include_inferred_and_inherited_signatures() {
    let old = "def f :: 'T -> i64\n  @'T : #distance\nfn f value = 'T.distance value\n";
    let new = old.replace("'T.distance value", "'T.distance value 0i64");
    assert_eq!(
        compare_mode(old, &new, &[], true),
        ["breaking changed def Main.f"]
    );
    let annotated = old.replace("#distance", "(#distance: 'T -> i64)");
    assert!(compare_mode(old, &annotated, &[], true).is_empty());
    let annotated_two = new.replace("#distance", "(#distance: 'T -> i64 -> i64)");
    assert!(compare_mode(&new, &annotated_two, &[], true).is_empty());
    assert!(compare_mode(old, &old.replace("'T", "'element"), &[], true).is_empty());
    let inherited = old
        .replace("def f", "private def helper")
        .replace("fn f", "fn helper")
        + "def f :: 'T -> i64 = \\value -> helper value\n";
    assert_eq!(
        compare_mode(
            &inherited,
            &inherited.replace("'T.distance value", "'T.distance value 0i64"),
            &[],
            true,
        ),
        ["breaking changed def Main.f"]
    );
}

#[test]
fn effective_concrete_member_requirements_do_not_escape_into_the_api() {
    let old = "record Item { value: i64 }\ndef distance :: Item -> i64 = \\value -> value.value\n\
        private def helper :: 'T -> i64\n  @'T : #distance\nfn helper value = 'T.distance value\n\
        def f :: Item -> i64 = \\value -> 0\n";
    let new = old.replace("\\value -> 0", "\\value -> helper value");
    assert!(compare_mode(old, &new, &[], true).is_empty());
}

#[test]
fn effective_unreferenced_public_aliases_preserve_their_constraints() {
    let old = "type Argument<'a> = 'a\n";
    let new = "type Argument<'a> = Add<'a>\n";
    for (before, after) in [(old, new), (new, old)] {
        assert_eq!(
            compare_mode(before, after, &[], true),
            ["breaking changed type Main.Argument"]
        );
    }
    assert!(compare_mode(new, &new.replace("'a", "'element"), &[], true).is_empty());
    let ordered = "type Argument<'a> = Ord<'a>\n";
    let repeated = "type Argument<'a> = Ord<Ord<'a>>\n";
    assert!(compare_mode(ordered, repeated, &[], true).is_empty());
    let old = "type Inner<'a> = 'a\ntype Argument<'a> = Inner<'a>\n";
    let new = old.replace("= 'a", "= Add<'a>");
    assert_eq!(
        compare_mode(old, &new, &[], true),
        [
            "breaking changed type Main.Argument",
            "breaking changed type Main.Inner",
        ]
    );
}

#[test]
fn effective_inferred_copy_requirements_are_part_of_the_callers_contract() {
    let old = "def f :: own 'a -> 'a = \\value -> value\n";
    let new = "def f :: own 'a -> 'a = \\value -> { let copy = value; value }\n";
    assert_eq!(
        compare_mode(old, new, &[], true),
        ["breaking changed def Main.f"]
    );
    let inherited = |source: &str| {
        source.replace("def f", "private def helper")
            + "def f :: own 'a -> 'a = \\value -> helper value\n"
    };
    assert_eq!(
        compare_mode(&inherited(old), &inherited(new), &[], true),
        ["breaking changed def Main.f"]
    );
}

#[test]
fn effective_satisfied_body_constraints_do_not_change_the_api() {
    assert!(
        compare_mode(
            "def f :: i64 -> i64 = \\x -> x\n",
            "def f :: i64 -> i64 = \\x -> x + 0\n",
            &[],
            true,
        )
        .is_empty()
    );
}

#[test]
fn effective_aliases_and_copy_records_do_not_turn_pure_additions_into_breaks() {
    let old = "record Point { x: i64 }\ntype Alias = Point\ndef size :: Alias -> i64 = \\point -> point.x\n";
    let new = old.replace("def size :: Alias", "pure def size :: Point");
    assert_eq!(
        compare_mode(old, &new, &[], true),
        ["non-breaking changed def Main.size"]
    );
    assert_eq!(compare(old, &new, &[]), ["breaking changed def Main.size"]);
    assert!(compare_mode(old, &old.replace(":: Alias", ":: Point"), &[], true).is_empty());
}

#[test]
fn effective_parameter_modes_distinguish_real_edition_changes_from_copy_values() {
    let manifest = "[package]\nname = \"api\"\nversion = \"0.1.0\"\nedition = \"2026\"\n";
    let new_manifest = manifest.replace("2026", "2027").replace("0.1.0", "0.2.0");
    let source = "record Point { x: i64 }\ndef point :: Point -> i64 = \\p -> p.x\ndef size :: string -> i64 = \\text -> text.length\n";
    let old = root(
        "effective-edition-old",
        &[("Main.tz", source), ("Tsuzuri.toml", manifest)],
    );
    let new = root(
        "effective-edition-new",
        &[("Main.tz", source), ("Tsuzuri.toml", &new_manifest)],
    );
    let changes = report_mode(&old, &new, true);
    assert_eq!(changes.changes.len(), 1, "{:?}", changes.changes);
    assert!(changes.changes[0].breaking);
    assert!(changes.changes[0].name.ends_with(".size"));
    assert_eq!(report(&old, &new).breaking(), 2);
    std::fs::remove_dir_all(old).unwrap();
    std::fs::remove_dir_all(new).unwrap();
}

#[test]
fn effective_generic_names_constraints_and_length_schemas_keep_their_meaning() {
    let old = "record Grid<const N: i64> { cells: [i64; N] }\ntype Row<const N: i64> = [i64; N]\nunion Cell<const N: i64> = Full of [i64; N] | Empty\n\
        pure def identity :: own 'a -> 'a = \\value -> value\n\
        def ordered :: Ord<'a> => own 'a -> bool = \\value -> value == value\n";
    let new = old.replace("N", "Count").replace("'a", "'element");
    assert!(compare_mode(old, &new, &[], true).is_empty());
    assert_eq!(
        compare_mode(
            "record Pair<const N: i64, const M: i64> { first: [i64; N], second: [i64; M] }\n",
            "record Pair<const N: i64, const M: i64> { first: [i64; M], second: [i64; N] }\n",
            &[],
            true,
        ),
        [
            "breaking changed field Main.Pair.first",
            "breaking changed field Main.Pair.second"
        ]
    );
}

#[test]
fn effective_class_method_aliases_and_pure_stages_are_resolved() {
    let main = "type Word = i64\nrecord Point { x: i64 }\ninstance Traits.Measure<Point> { fn size point = point.x }\n";
    let old = "class Measure<'a> { pure def size :: 'a -> Main.Word }\n";
    let new = old.replace("Main.Word", "i64").replace("'a", "'element");
    assert!(compare_mode(main, main, &[("Traits.tt", old, &new)], true).is_empty());
    let main = "instance Traits.Factory<i64> { fn make offset = { \\value -> offset + value } }\n";
    let class = "class Factory<'a> { pure def make :: own 'a -> (i64 -> i64) }\n";
    assert!(compare_mode(main, main, &[("Traits.tt", class, class)], true).is_empty());
}

#[test]
fn effective_higher_kinded_constraints_and_nested_pure_callbacks_are_canonical() {
    let class = "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }\n";
    let main = "def same :: Traits.Functor<'f> => own 'f<'a> -> 'f<'a> = \\value -> value\n";
    let renamed_class = class
        .replace("'f", "'constructor")
        .replace("'a", "'input")
        .replace("'b", "'output");
    let renamed_main = main.replace("'f", "'constructor").replace("'a", "'item");
    assert!(
        compare_mode(
            main,
            &renamed_main,
            &[("Traits.tt", class, &renamed_class)],
            true
        )
        .is_empty()
    );
    let old = "type Transform<'a> = pure ('a -> 'a)\ndef apply :: Transform<'a> -> own 'a -> 'a = \\f value -> f value\n";
    let new = old
        .replace("Transform<'a> ->", "pure ('a -> 'a) ->")
        .replace("'a", "'element");
    assert!(compare_mode(old, &new, &[], true).is_empty());
    let old = "def apply :: pure (i64 -> (i64 -> i64)) -> i64 -> i64 = \\f value -> f value (value + 1)\n";
    let new = old.replace("(i64 -> (i64 -> i64))", "(i64 -> i64 -> i64)");
    assert_eq!(
        compare_mode(old, &new, &[], true),
        ["breaking changed def Main.apply"]
    );
}

#[test]
fn effective_regions_follow_checked_provenance_after_alias_resolution() {
    let old = "type Text = string\ndef pick {r s} :: ref {r} Text -> ref {s} Text -> ref {r} Text = \\left right -> left\n";
    let renamed = old
        .replace("Text ->", "string ->")
        .replace("{r", "{first")
        .replace(" s}", " second}")
        .replace("{s}", "{second}");
    assert!(compare_mode(old, &renamed, &[], true).is_empty());
    let new = old
        .replace("-> ref {r} Text =", "-> ref {s} Text =")
        .replace("-> left\n", "-> right\n");
    assert_eq!(
        compare_mode(old, &new, &[], true),
        ["breaking changed def Main.pick"]
    );
    let old = "record View {r s} { first: ref {r} string, second: ref {s} string }\n";
    let new = "record View {r s} { first: ref {s} string, second: ref {r} string }\n";
    assert_eq!(
        compare_mode(old, new, &[], true),
        [
            "breaking changed field Main.View.first",
            "breaking changed field Main.View.second"
        ]
    );
}

#[test]
fn effective_standard_library_root_is_not_mistaken_for_dependency_sources() {
    let standard =
        Project::load_for_docs(&Path::new(env!("CARGO_MANIFEST_DIR")).join("std")).unwrap();
    assert!(standard.sources.iter().all(|source| {
        let path = source.relative_path.to_str().unwrap();
        path.starts_with("std/") && !path.contains('\\')
    }));
    let standard_api = api_diff::effective_api(&standard).unwrap();
    let empty = root("effective-empty", &[("Main.tz", "0\n")]);
    let empty_api = api_diff::api(&Project::load_for_api(&empty).unwrap()).unwrap();
    let additions = api_diff::compare(&empty_api, &standard_api);
    assert!(
        additions.non_breaking() > 100,
        "{}",
        additions.render_text()
    );
    assert_eq!(additions.breaking(), 0);
    std::fs::remove_dir_all(empty).unwrap();
}

#[test]
fn effective_analysis_errors_never_fall_back_to_syntax_only_comparison() {
    let directory = root(
        "effective-error",
        &[("Main.tz", "def broken :: i64 = missing\n")],
    );
    let syntax = Project::load_for_api(&directory).unwrap();
    assert!(api_diff::api(&syntax).is_ok());
    let checked = Project::load_for_docs(&directory).unwrap();
    let error = api_diff::effective_api(&checked)
        .err()
        .expect("analysis must fail");
    assert_eq!(error.code, "E1002");
    assert!(error.message.contains("missing"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn effective_path_dependencies_resolve_without_becoming_root_api() {
    let directory = root(
        "effective-path-dependencies",
        &[
            (
                "old/Tsuzuri.toml",
                &manifest(
                    "app",
                    "0.1.0",
                    "[dependencies]\nshapes = { path = \"../shapes-old\" }\n",
                ),
            ),
            (
                "new/Tsuzuri.toml",
                &manifest(
                    "app",
                    "0.2.0",
                    "[dependencies]\nshapes = { path = \"../shapes-new\" }\n",
                ),
            ),
            ("shapes-old/Tsuzuri.toml", &manifest("shapes", "0.1.0", "")),
            ("shapes-new/Tsuzuri.toml", &manifest("shapes", "0.2.0", "")),
            ("shapes-old/Point.tz", "record Point { x: i64 }\n"),
            (
                "shapes-new/Point.tz",
                "record Point { x: i64 }\ndef extra :: i64 = 7\n",
            ),
            (
                "old/Main.tz",
                "type Local = Shapes::Point\ndef read :: Local -> i64 = \\point -> point.x\n",
            ),
            (
                "new/Main.tz",
                "type Local = Shapes::Point\npure def read :: Shapes::Point -> i64 = \\point -> point.x\n",
            ),
        ],
    );
    let checked = report_mode(&directory.join("old"), &directory.join("new"), true);
    assert_eq!(checked.breaking(), 0, "{}", checked.render_text());
    assert_eq!(
        summary(&checked),
        ["non-breaking changed def App::Main.read"]
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn effective_command_is_opt_in_validates_flags_and_reports_analysis_errors() {
    let old_source = "type Word = i64\ndef f :: Word -> Word = \\n -> n\n";
    let new_source = "type Word = i64\ndef f :: i64 -> i64 = \\n -> n\n";
    let directory = root(
        "effective-cli",
        &[("old/Main.tz", old_source), ("new/Main.tz", new_source)],
    );
    let old = directory.join("old");
    let new = directory.join("new");
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command.arg("api-diff");
        command
    };
    let ordinary = command().arg(&old).arg(&new).output().unwrap();
    assert_eq!(ordinary.status.code(), Some(1));
    let checked = command()
        .arg("--effective")
        .arg("--json")
        .arg(&old)
        .arg(&new)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("\"breaking\":0"));
    let duplicate = command()
        .args(["--effective", "--effective", "--json"])
        .arg(&old)
        .arg(&new)
        .output()
        .unwrap();
    assert_eq!(duplicate.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("E2000"));
    let ignored_thread_setting = command()
        .arg(&old)
        .arg(&old)
        .env("TSUZURI_THREADS", "invalid")
        .output()
        .unwrap();
    assert!(ignored_thread_setting.status.success());
    let invalid_thread_setting = command()
        .args(["--effective", "missing-old", "missing-new"])
        .env("TSUZURI_THREADS", "invalid")
        .output()
        .unwrap();
    assert_eq!(invalid_thread_setting.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_thread_setting.stderr).contains("E2000"));
    std::fs::write(old.join("Main.tz"), "def f :: i64 = missing\n").unwrap();
    let invalid = command()
        .args(["--effective", "--json"])
        .arg(&old)
        .arg(&new)
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(1));
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("E1002"));
    std::fs::rename(&new, directory.join("--effective")).unwrap();
    let literal = command()
        .args(["--effective", "--", "--effective", "--effective"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(
        literal.status.success(),
        "{}",
        String::from_utf8_lossy(&literal.stderr)
    );
    std::fs::remove_dir_all(directory).unwrap();
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
fn effective_extern_signatures_accept_whole_parameter_ownership() {
    let source = "extern def host_len :: own ref string -> i64\n";
    assert!(compare_mode(source, source, &[], true).is_empty());
    for source in [
        "extern def host_len :: own ref own string -> i64\n",
        "extern def host_len :: ref string -> own i64\n",
    ] {
        let directory = root("invalid-extern-own", &[("Main.tz", source)]);
        let project = Project::load_for_docs(&directory).unwrap();
        assert!(api_diff::effective_api(&project).is_err(), "{source}");
        std::fs::remove_dir_all(directory).unwrap();
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
fn pure_call_stage_boundaries_are_not_flattened_away() {
    let curried = "pure def f :: i64 -> i64 -> i64 = \\x y -> x + y\n";
    let factory = "pure def f :: i64 -> (i64 -> i64) = \\x -> { \\y -> x + y }\n";
    check(curried, factory, &["breaking changed def Main.f"]);
    check(factory, curried, &["breaking changed def Main.f"]);
    assert_eq!(
        compare_mode(curried, factory, &[], true),
        ["breaking changed def Main.f"]
    );
    assert_eq!(
        compare_mode(factory, curried, &[], true),
        ["breaking changed def Main.f"]
    );
    assert_eq!(
        compare_mode(&factory.replacen("pure ", "", 1), factory, &[], true),
        ["non-breaking changed def Main.f"]
    );
    check(
        &factory.replacen("pure ", "", 1),
        factory,
        &["non-breaking changed def Main.f"],
    );
}

#[test]
fn pure_class_method_contracts_and_call_stages_are_compared() {
    let ordinary = "class Measure<'a> { def size :: ref 'a -> i64 }\n";
    let pure = ordinary.replace("def size", "pure def size");
    check(
        ordinary,
        &pure,
        &["breaking changed method Main.Measure.size"],
    );
    check(
        &pure,
        ordinary,
        &["breaking changed method Main.Measure.size"],
    );
    let curried = "class Combine<'a> { pure def combine :: ref 'a -> i64 -> i64 }\n";
    let factory = "class Combine<'a> { pure def combine :: ref 'a -> (i64 -> i64) }\n";
    check(
        curried,
        factory,
        &["breaking changed method Main.Combine.combine"],
    );
    check(
        factory,
        curried,
        &["breaking changed method Main.Combine.combine"],
    );
    check(&pure, &pure.replace("'a", "'element"), &[]);
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
        "def build :: fn() -> (i64 -> i64)\nfn build = { let k = 1; \\n -> n + k }\n",
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
    // Effective comparison requires the real dependency graph, never the syntax-only fallback.
    assert!(Project::load_for_docs(&app).is_err());
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
    let usage = "usage: tsuzuri api-diff old new [--effective] [--json]";
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

#[test]
fn api_diff_treats_attributes_as_promises() {
    let plain = "def f :: i64 -> i64 = \\n -> n + 1\n";
    let attributed = format!("@no_alloc\n@no_trap\n{plain}");
    assert_eq!(
        compare(plain, &attributed, &[]),
        ["non-breaking changed def Main.f"]
    );
    assert_eq!(
        compare(&attributed, plain, &[]),
        ["breaking changed def Main.f"]
    );
    assert_eq!(
        compare(&attributed, &attributed.replace("@no_trap\n", ""), &[]),
        ["breaking changed def Main.f"]
    );
    assert!(compare(&attributed, &attributed, &[]).is_empty());
}

#[test]
fn api_diff_classifies_contract_lines() {
    let plain = "def f :: i64 -> i64 = \\x ->\n    x + 1\n";
    let requires = "def f :: i64 -> i64 = \\x ->\n    @requires x > 0\n    x + 1\n";
    let ensures = "def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > x\n    x + 1\n";
    assert_eq!(
        compare(plain, requires, &[]),
        ["breaking added contract Main.f.@requires x > 0"]
    );
    assert_eq!(
        compare(requires, plain, &[]),
        ["non-breaking removed contract Main.f.@requires x > 0"]
    );
    assert_eq!(
        compare(plain, ensures, &[]),
        ["non-breaking added contract Main.f.@ensures \\r -> r > x"]
    );
    assert_eq!(
        compare(ensures, plain, &[]),
        ["breaking removed contract Main.f.@ensures \\r -> r > x"]
    );
    assert!(compare(requires, requires, &[]).is_empty());
}
