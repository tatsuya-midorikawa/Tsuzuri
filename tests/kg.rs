//! G26 (D-60): the knowledge graph of a program, its nodes, edges and property tables.

use std::path::Path;

use tsuzuri::kg::{self, KgModel};

fn sources_of(directory: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_owned()];
    while let Some(at) = pending.pop() {
        for entry in std::fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| matches!(extension.to_str(), Some("tz" | "tt" | "tc")))
            {
                let relative = path
                    .strip_prefix(directory)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                found.push((relative, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> KgModel {
    let sources = sources_of(&root().join("examples").join(name));
    let borrowed: Vec<(&str, &str)> = sources
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    kg::collect_sources(&borrowed).unwrap_or_else(|errors| panic!("{name}: {errors:?}"))
}

fn kg_basic() -> KgModel {
    let project = tsuzuri::driver::Project::load(&root().join("tests/fixtures/kg_basic")).unwrap();
    kg::collect(&project).unwrap()
}

fn text(write: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>) -> String {
    let mut out = Vec::new();
    write(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn golden(name: &str, file: &str) -> String {
    std::fs::read_to_string(root().join("tests/golden/kg").join(name).join(file)).unwrap()
}

fn rows(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn sources(model: Result<KgModel, tsuzuri::diagnostic::DiagnosticSet>) -> KgModel {
    // A model, or the diagnostics of the failed analysis.
    model.unwrap_or_else(|errors| panic!("{errors:?}"))
}

fn one(main: &str) -> KgModel {
    sources(kg::collect_sources(&[("Main.tz", main)]))
}

fn facts_of(model: &KgModel, module: &str, id: &str) -> serde_json::Value {
    rows(&text(|o| model.write_facts(module, o)))
        .into_iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no facts row for {id}"))
}

fn kinds_at(sites: &serde_json::Value) -> Vec<(String, usize, usize)> {
    sites
        .as_array()
        .unwrap()
        .iter()
        .map(|site| {
            (
                site["kind"]
                    .as_str()
                    .or(site["event"].as_str())
                    .unwrap()
                    .to_owned(),
                site["span"]["line"].as_u64().unwrap() as usize,
                site["span"]["column"].as_u64().unwrap() as usize,
            )
        })
        .collect()
}

// Phase 1: nodes.

#[test]
fn nodes_of_examples_point_match_the_golden_file() {
    let model = example("point");
    assert_eq!(
        text(|o| model.write_nodes(o)),
        golden("point", "nodes.jsonl")
    );
}

#[test]
fn nodes_of_examples_polymorphism_match_the_golden_file() {
    let model = example("polymorphism");
    assert_eq!(
        text(|o| model.write_nodes(o)),
        golden("polymorphism", "nodes.jsonl")
    );
}

#[test]
fn kg_basic_nodes_cover_every_kind() {
    let nodes = rows(&text(|o| kg_basic().write_nodes(o)));
    let mut kinds = std::collections::BTreeMap::new();
    for node in &nodes {
        *kinds
            .entry(node["kind"].as_str().unwrap().to_owned())
            .or_insert(0) += 1;
    }
    let expected = [
        ("alias", 1),
        ("bench", 1),
        ("case", 2),
        ("class", 1),
        ("const", 1),
        ("def", 23),
        ("extern", 1),
        ("field", 5),
        ("handle", 1),
        ("instance", 1),
        ("method", 2),
        ("module", 4),
        ("record", 2),
        ("test", 1),
        ("union", 1),
    ];
    let expected: std::collections::BTreeMap<String, i32> = expected
        .iter()
        .map(|(kind, count)| ((*kind).to_owned(), *count))
        .collect();
    assert_eq!(kinds, expected);
    assert_eq!(nodes.len(), 47);
    let defs = |module: &str| {
        nodes
            .iter()
            .filter(|node| node["kind"] == "def" && node["module"] == module)
            .count()
    };
    assert_eq!(defs("Acme::Kg::Geometry::Metrics"), 1);
    assert_eq!(defs("Acme::Kg::Main"), 18);
    assert_eq!(defs("Acme::Kg::Shapes"), 4);
    let metrics = nodes
        .iter()
        .find(|node| node["id"] == "Acme::Kg::Geometry::Metrics.norm1")
        .unwrap();
    assert_eq!(metrics["doc"], "The sum of the absolute coordinates.");
    assert_eq!(metrics["export"], true);
}

#[test]
fn nodes_are_sorted_unique_and_deterministic() {
    let files = sources_of(&root().join("examples/polymorphism"));
    let forward: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let backward: Vec<(&str, &str)> = forward.iter().rev().copied().collect();
    let first = sources(kg::collect_sources(&forward));
    let second = sources(kg::collect_sources(&forward));
    let reversed = sources(kg::collect_sources(&backward));
    let write = |model: &KgModel| {
        (
            text(|o| model.write_nodes(o)),
            text(|o| model.write_edges(o)),
            model
                .fact_modules()
                .iter()
                .map(|module| text(|o| model.write_facts(module, o)))
                .collect::<Vec<_>>(),
            model
                .layout_modules()
                .iter()
                .map(|module| text(|o| model.write_layout(module, o)))
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(write(&first), write(&second));
    let ids: Vec<String> = rows(&text(|o| first.write_nodes(o)))
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(ids, sorted);
    // The sources' order changes their indices, not the rows.
    assert_eq!(write(&first), write(&reversed));
}

#[test]
fn duplicate_test_names_get_suffixes() {
    let model = one(
        "def one :: unit -> i64 = \\_ -> 1\n\ntest \"same\" = assert (one () == 1)\n\ntest \"same\" = assert (one () == 1)\n\nbench \"same\" = \\n -> n\n",
    );
    let ids: Vec<String> = rows(&text(|o| model.write_tests(o)))
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        ids,
        ["bench:Main.same", "test:Main.same", "test:Main.same#2"]
    );
    let edges = text(|o| model.write_edges(o));
    assert!(edges.contains("\"from\":\"test:Main.same#2\""), "{edges}");
}

/// With `def main`, the entry is `main` itself: no `$entry` node, and `main` keeps its calls.
#[test]
fn def_main_is_not_an_entry_node() {
    let model = one(
        "def helper :: i64 -> i64 = \\n -> n + 1\n\ndef main :: unit -> i32 = \\() ->\n    let value = helper 1\n    0\n",
    );
    let nodes = text(|o| model.write_nodes(o));
    assert!(!nodes.contains("$entry"), "{nodes}");
    let calls: Vec<String> = rows(&text(|o| model.write_edges(o)))
        .into_iter()
        .filter(|edge| edge["rel"] == "calls")
        .map(|edge| {
            format!(
                "{} -> {}",
                edge["from"].as_str().unwrap(),
                edge["to"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(calls, ["Main.main -> Main.helper"]);
}

#[test]
fn collect_fails_on_a_type_error() {
    let errors = kg::collect_sources(&[("Main.tz", "let x: i64 = \"a\"\nx\n")]).unwrap_err();
    assert_eq!(errors.first().unwrap().code, "E1003");
}

// Phase 2: edges and tests.

#[test]
fn edges_of_examples_point_match_the_golden_file() {
    let model = example("point");
    assert_eq!(
        text(|o| model.write_edges(o)),
        golden("point", "edges.jsonl")
    );
}

#[test]
fn kg_basic_edges_have_every_rel() {
    let edges = rows(&text(|o| kg_basic().write_edges(o)));
    let count = |rel: &str| edges.iter().filter(|edge| edge["rel"] == rel).count();
    assert_eq!(count("contains"), 43);
    assert_eq!(count("calls"), 24);
    assert_eq!(count("implements"), 2);
    assert_eq!(count("instance_of"), 1);
    assert_eq!(count("imports"), 1);
    assert_eq!(count("tested_by"), 1);
    assert!(count("uses_type") > 0);
    let from = |name: &str| {
        edges
            .iter()
            .filter(|edge| edge["rel"] == "calls" && edge["from"] == name)
            .count()
    };
    for (name, calls) in [
        ("Acme::Kg::Geometry::Metrics.norm1", 2),
        ("Acme::Kg::Main.sum_to", 1),
        ("Acme::Kg::Main.fib", 2),
        ("Acme::Kg::Main.describe", 2),
        ("Acme::Kg::Main.copies", 3),
        ("Acme::Kg::Main.ownership", 3),
        ("Acme::Kg::Main.traced", 1),
        ("test:Acme::Kg::Main.total adds", 2),
        ("bench:Acme::Kg::Main.total of three", 1),
    ] {
        assert_eq!(from(name), calls, "{name}");
    }
    let implements: Vec<_> = edges
        .iter()
        .filter(|edge| edge["rel"] == "implements")
        .map(|edge| (edge["from"].as_str().unwrap(), edge["to"].as_str().unwrap()))
        .collect();
    assert_eq!(
        implements,
        [
            ("Acme::Kg::Shapes.Point", "Acme::Kg::Traits.Sized"),
            ("Acme::Kg::Shapes.Point", "Eq")
        ]
    );
    let tested: Vec<_> = edges
        .iter()
        .filter(|edge| edge["rel"] == "tested_by")
        .map(|edge| (edge["from"].as_str().unwrap(), edge["to"].as_str().unwrap()))
        .collect();
    assert_eq!(
        tested,
        [("Acme::Kg::Main.total", "test:Acme::Kg::Main.total adds")]
    );
}

#[test]
fn contains_edges_form_a_forest_rooted_at_modules() {
    for model in [example("point"), example("polymorphism"), kg_basic()] {
        let nodes = rows(&text(|o| model.write_nodes(o)));
        let edges = rows(&text(|o| model.write_edges(o)));
        let ids: std::collections::BTreeSet<&str> = nodes
            .iter()
            .map(|node| node["id"].as_str().unwrap())
            .collect();
        let mut parents = std::collections::BTreeMap::new();
        for edge in edges.iter().filter(|edge| edge["rel"] == "contains") {
            let (from, to) = (edge["from"].as_str().unwrap(), edge["to"].as_str().unwrap());
            assert!(ids.contains(from) && ids.contains(to));
            assert!(parents.insert(to, from).is_none(), "{to} has two parents");
        }
        for node in &nodes {
            let id = node["id"].as_str().unwrap();
            assert_eq!(node["kind"] == "module", !parents.contains_key(id), "{id}");
            // Following parents ends at the node's module.
            let mut at = id;
            for _ in 0..8 {
                match parents.get(at) {
                    Some(parent) => at = parent,
                    None => break,
                }
            }
            assert_eq!(at, node["module"].as_str().unwrap(), "{id}");
        }
        for edge in &edges {
            if edge["rel"] != "implements" {
                assert!(ids.contains(edge["from"].as_str().unwrap()), "{edge}");
            }
        }
    }
}

#[test]
fn calls_args_conv_cases() {
    let model = one(concat!(
        "def add_to :: ref mut i64 -> i64 -> unit = \\target amount -> { deref target = deref target + amount; }\n",
        "def peek :: ref string -> i64 = \\text -> text.length\n",
        "def consume :: own string -> i64 = \\text -> text.length\n",
        "def apply :: (i64 -> i64) -> i64 = \\f -> f 1\n",
        "def inc :: i64 -> i64 = \\n -> n + 1\n",
        "def run :: i64 -> i64 = \\n ->\n",
        "    let mut total = n\n",
        "    add_to (ref mut total) 1\n",
        "    let label = \"a\" + \"b\"\n",
        "    let seen = peek (ref label)\n",
        "    let owned = consume label\n",
        "    total + seen + owned + apply inc + Int.max 1 2\n",
    ));
    let calls: Vec<(String, Vec<String>)> = rows(&text(|o| model.write_edges(o)))
        .into_iter()
        .filter(|edge| edge["rel"] == "calls" && edge["from"] == "Main.run")
        .map(|edge| {
            (
                edge["to"].as_str().unwrap().to_owned(),
                edge["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.as_str().unwrap().to_owned())
                    .collect(),
            )
        })
        .collect();
    let find = |to: &str| {
        calls
            .iter()
            .find(|(target, _)| target == to)
            .unwrap_or_else(|| panic!("no call of {to} in {calls:?}"))
            .1
            .clone()
    };
    assert_eq!(find("Main.add_to"), ["ref_mut", "copy"]);
    assert_eq!(find("Main.peek"), ["ref"]);
    assert_eq!(find("Main.consume"), ["own"]);
    // Edition 2027 reads a function-typed parameter by default, so the checker lends `inc`.
    assert_eq!(find("Main.apply"), ["ref"]);
    // A function used as a value is a call without arguments.
    assert_eq!(find("Main.inc"), Vec::<String>::new());
    assert_eq!(find("Int.max"), ["copy", "copy"]);
}

#[test]
fn tested_by_depth_and_cap() {
    let mut chain = String::new();
    for index in 1..=9 {
        if index == 9 {
            chain.push_str("def f9 :: i64 -> i64 = \\n -> n\n");
        } else {
            chain.push_str(&format!(
                "def f{index} :: i64 -> i64 = \\n -> f{} n\n",
                index + 1
            ));
        }
    }
    chain.push_str("test \"chain\" = assert (f1 1 == 1)\n");
    let model = one(&chain);
    let mut tested: Vec<String> = rows(&text(|o| model.write_edges(o)))
        .into_iter()
        .filter(|edge| edge["rel"] == "tested_by")
        .map(|edge| edge["from"].as_str().unwrap().to_owned())
        .collect();
    tested.sort();
    let expected: Vec<String> = (1..=8).map(|index| format!("Main.f{index}")).collect();
    assert_eq!(tested, expected);
    let mut many = String::new();
    let mut body = String::from("{ ");
    for index in 0..250 {
        many.push_str(&format!("def g{index} :: i64 -> i64 = \\n -> n\n"));
        body.push_str(&format!("let a{index} = g{index} 1; "));
    }
    many.push_str(&format!("test \"many\" = {body}assert (a249 == 1) }}\n"));
    let model = one(&many);
    let tested = rows(&text(|o| model.write_edges(o)))
        .into_iter()
        .filter(|edge| edge["rel"] == "tested_by")
        .count();
    assert_eq!(tested, 200);
}

#[test]
fn tests_jsonl_lists_tests_and_benches() {
    let tests = rows(&text(|o| kg_basic().write_tests(o)));
    assert_eq!(tests.len(), 2);
    assert_eq!(tests[0]["id"], "bench:Acme::Kg::Main.total of three");
    assert_eq!(tests[0]["kind"], "bench");
    assert_eq!(tests[0]["index"], 0);
    assert_eq!(tests[1]["id"], "test:Acme::Kg::Main.total adds");
    assert_eq!(tests[1]["kind"], "test");
    assert_eq!(tests[1]["module"], "Acme::Kg::Main");
    assert_eq!(tests[1]["name"], "total adds");
    assert_eq!(tests[1]["index"], 0);
    assert_eq!(text(|o| example("point").write_tests(o)), "");
}

// Phase 3: facts and effects.

#[test]
fn effects_debug_with_via() {
    let model = kg_basic();
    let traced = facts_of(&model, "Main", "Acme::Kg::Main.traced");
    assert_eq!(traced["effects"], serde_json::json!(["debug"]));
    let site = &traced["effect_sites"][0];
    assert_eq!(site["effect"], "debug");
    assert_eq!(site["via"], serde_json::json!(["Debug.print"]));
    assert_eq!(
        (&site["span"]["line"], &site["span"]["column"]),
        (&81.into(), &5.into())
    );
    assert_eq!(traced["effect_sites"].as_array().unwrap().len(), 1);
}

#[test]
fn effects_are_transitive_through_user_functions() {
    let model = one(concat!(
        "def b :: i64 -> unit = \\n -> Debug.print (ref n)\n",
        "def a :: i64 -> unit = \\n -> b n\n",
    ));
    let a = facts_of(&model, "Main", "Main.a");
    assert_eq!(a["effects"], serde_json::json!(["debug"]));
    assert_eq!(
        a["effect_sites"][0]["via"],
        serde_json::json!(["Main.b", "Debug.print"])
    );
}

#[test]
fn effects_complete_is_false_without_a_20_summary() {
    let model = kg_basic();
    for module in model.fact_modules() {
        for row in rows(&text(|o| model.write_facts(module, o))) {
            assert_eq!(row["effects_complete"], false);
            assert_eq!(row["alloc_complete"], false);
        }
    }
    assert_eq!(model.fact_keys(), kg::model::FACT_KEYS);
}

#[test]
fn facts_rows_exist_for_every_function_like_node() {
    let model = kg_basic();
    assert_eq!(model.fact_modules(), ["Geometry.Metrics", "Main", "Shapes"]);
    let count = |module: &str| rows(&text(|o| model.write_facts(module, o))).len();
    assert_eq!(count("Geometry.Metrics"), 1);
    assert_eq!(count("Main"), 20);
    assert_eq!(count("Shapes"), 6);
    assert_eq!(count("Traits"), 0);
    let row = rows(&text(|o| model.write_facts("Main", o))).remove(0);
    let keys: Vec<&str> = row
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = kg::model::FACT_KEYS.to_vec();
    expected.sort();
    let mut keys = keys;
    keys.sort();
    assert_eq!(keys, expected);
}

// Phase 4: sites, recursion and loops.

#[test]
fn copy_sites_match_copies_sites() {
    let project = tsuzuri::driver::Project::load(&root().join("tests/fixtures/kg_basic")).unwrap();
    let module = project.analyze_all().unwrap();
    let copies = tsuzuri::copies::sites(&module);
    let model = kg::collect(&project).unwrap();
    let row = facts_of(&model, "Main", "Acme::Kg::Main.copies");
    let sites = row["copy_sites"].as_array().unwrap();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites.len(), copies.len());
    assert_eq!(sites[0]["span"]["start"], copies[0].span.start);
    assert_eq!(
        (&sites[0]["span"]["line"], &sites[0]["span"]["column"]),
        (&54.into(), &17.into())
    );
    assert_eq!(sites[0]["type"], "[i64]");
    assert_eq!(sites[0]["kind"], "local");
    assert_eq!(sites[0]["cost"], "length");
}

#[test]
fn recursion_classes() {
    let model = kg_basic();
    let recursion = |name: &str| {
        facts_of(&model, "Main", &format!("Acme::Kg::Main.{name}"))["recursion"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(recursion("sum_to"), "self_tail");
    assert_eq!(recursion("fib"), "self");
    assert_eq!(recursion("is_even"), "mutual");
    assert_eq!(recursion("is_odd"), "mutual");
    assert_eq!(recursion("total"), "none");
}

#[test]
fn loops_kinds() {
    let model = kg_basic();
    let countdown = facts_of(&model, "Main", "Acme::Kg::Main.countdown");
    assert_eq!(kinds_at(&countdown["loops"]), [("while".to_owned(), 34, 5)]);
    let total = facts_of(&model, "Main", "Acme::Kg::Main.total");
    assert_eq!(kinds_at(&total["loops"]), [("for_range".to_owned(), 6, 5)]);
    let model = one(
        "def each :: [i64] -> i64 = \\values ->\n    let mut sum = 0\n    for value in values do\n        sum = sum + value\n    sum\n",
    );
    let each = facts_of(&model, "Main", "Main.each");
    assert_eq!(kinds_at(&each["loops"]), [("for_each".to_owned(), 3, 5)]);
}

#[test]
fn trap_sites_proven_and_unproven() {
    let model = kg_basic();
    let traps = |name: &str| {
        facts_of(&model, "Main", name)["trap_sites"]
            .as_array()
            .unwrap()
            .iter()
            .map(|site| {
                (
                    site["kind"].as_str().unwrap().to_owned(),
                    site["proven"].as_bool().unwrap(),
                    site["span"]["line"].as_u64().unwrap(),
                    site["span"]["column"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let total = traps("Acme::Kg::Main.total");
    assert!(
        total.contains(&("bounds_check".into(), true, 7, 21)),
        "{total:?}"
    );
    // The range's step is the literal 1, so it is never zero.
    assert!(
        total.contains(&("range_step_zero".into(), true, 6, 5)),
        "{total:?}"
    );
    assert_eq!(
        traps("Acme::Kg::Main.at"),
        [("bounds_check".into(), false, 11, 22)]
    );
    assert_eq!(
        traps("Acme::Kg::Main.ratio"),
        [
            ("integer_division_by_zero".into(), false, 14, 23),
            ("integer_division_overflow".into(), false, 14, 23)
        ]
    );
    assert_eq!(
        traps("test:Acme::Kg::Main.total adds"),
        [("assert".into(), false, 84, 21)]
    );
    assert_eq!(traps("Acme::Kg::Main.sum_to"), []);
    let model = one("def half :: i64 -> i64 = \\n -> n / 2\n");
    let half = facts_of(&model, "Main", "Main.half");
    let proven: Vec<bool> = half["trap_sites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|site| site["proven"].as_bool().unwrap())
        .collect();
    assert_eq!(proven, [true, true]);
}

#[test]
fn alloc_sites_new() {
    let model = kg_basic();
    let filled = facts_of(&model, "Main", "Acme::Kg::Main.filled");
    assert_eq!(
        kinds_at(&filled["alloc_sites"]),
        [("new_array".to_owned(), 47, 18)]
    );
    let copies = facts_of(&model, "Main", "Acme::Kg::Main.copies");
    assert_eq!(copies["alloc_sites"], serde_json::json!([]));
}

#[test]
fn arrays_truncate_at_100() {
    let mut body = String::new();
    for index in 0..150 {
        body.push_str(&format!("    assert (n != {index})\n"));
    }
    let model = one(&format!("def many :: i64 -> unit = \\n ->\n{body}    ()\n"));
    let many = facts_of(&model, "Main", "Main.many");
    assert_eq!(many["trap_sites"].as_array().unwrap().len(), 100);
    assert_eq!(many["truncated"], serde_json::json!({"trap_sites": 50}));
}

#[test]
fn deep_expressions_do_not_overflow_the_stack() {
    let mut expression = String::from("n");
    for _ in 0..120 {
        expression = format!("({expression} + 1)");
    }
    let model = one(&format!("def deep :: i64 -> i64 = \\n -> {expression}\n"));
    let deep = facts_of(&model, "Main", "Main.deep");
    assert_eq!(deep["recursion"], "none");
}

// Phase 5: ownership events.

#[test]
fn ownership_events_for_kg_basic() {
    let model = kg_basic();
    let events = |name: &str| {
        facts_of(&model, "Main", name)["ownership"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                (
                    event["var"].as_str().unwrap().to_owned(),
                    event["event"].as_str().unwrap().to_owned(),
                    event["to"].as_str().map(str::to_owned),
                    event["span"]["line"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let main = |name: &str| Some(format!("Acme::Kg::Main.{name}"));
    assert_eq!(
        events("Acme::Kg::Main.ownership"),
        [
            ("total".into(), "borrow_mut".into(), main("add_to"), 72),
            ("label".into(), "borrow".into(), main("peek_twice"), 74),
            ("text".into(), "move".into(), main("consume"), 76),
            ("label".into(), "drop".into(), None, 77),
        ]
    );
    assert_eq!(
        events("Acme::Kg::Main.peek_twice"),
        [
            ("text".into(), "reborrow".into(), main("peek"), 64),
            ("text".into(), "reborrow".into(), main("peek"), 64),
        ]
    );
    let row = facts_of(&model, "Main", "Acme::Kg::Main.ownership");
    let drop = row["ownership"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event"] == "drop")
        .unwrap();
    assert_eq!(drop["span"]["start"], drop["span"]["end"]);
    assert_eq!(drop["span"]["column"], 25);
    assert!(events("Acme::Kg::Main.copies").contains(&("values".into(), "copy".into(), None, 54)));
}

#[test]
fn generic_function_events_use_the_abstract_body() {
    let model = kg_basic();
    let twice = facts_of(&model, "Shapes", "Acme::Kg::Shapes.twice");
    assert_eq!(twice["ownership"], serde_json::json!([]));
}

// Phase 6: layouts and specializations.

#[test]
fn layout_of_packed_reorders() {
    let model = kg_basic();
    assert_eq!(model.layout_modules(), ["Shapes"]);
    let layouts = rows(&text(|o| model.write_layout("Shapes", o)));
    let packed = layouts
        .iter()
        .find(|row| row["id"] == "Acme::Kg::Shapes.Packed")
        .unwrap();
    assert_eq!(packed["size"], serde_json::json!([16, 16]));
    assert_eq!(packed["align"], serde_json::json!([8, 8]));
    assert_eq!(packed["reordered"], true);
    let slots: Vec<(&str, u64)> = packed["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| {
            (
                field["name"].as_str().unwrap(),
                field["slot"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(slots, [("a", 1), ("n", 0), ("b", 2)]);
}

#[test]
fn layout_union_kinds() {
    let model = kg_basic();
    let layouts = text(|o| model.write_layout("Shapes", o));
    let expected = concat!(
        "{\"id\":\"Acme::Kg::Shapes.Packed\",\"kind\":\"record\",\"type\":\"Shapes.Packed\",\"size\":[16,16],\"align\":[8,8],\"fields\":[{\"name\":\"a\",\"type\":\"i8\",\"slot\":1},{\"name\":\"n\",\"type\":\"i64\",\"slot\":0},{\"name\":\"b\",\"type\":\"i8\",\"slot\":2}],\"reordered\":true,\"drop_flag\":false}\n",
        "{\"id\":\"Acme::Kg::Shapes.Point\",\"kind\":\"record\",\"type\":\"Shapes.Point\",\"size\":[16,16],\"align\":[8,8],\"fields\":[{\"name\":\"x\",\"type\":\"f64\",\"slot\":0},{\"name\":\"y\",\"type\":\"f64\",\"slot\":1}],\"reordered\":false,\"drop_flag\":false}\n",
        "{\"id\":\"Acme::Kg::Shapes.Shape\",\"kind\":\"union\",\"type\":\"Shapes.Shape\",\"size\":[16,16],\"align\":[8,8],\"layout\":\"common\",\"tag_bytes\":1,\"niche\":null,\"cases\":[{\"name\":\"Circle\",\"payload\":\"f64\"},{\"name\":\"Empty\",\"payload\":null}],\"drop_flag\":false}\n",
    );
    assert_eq!(layouts, expected);
}

#[test]
fn specializations_count() {
    let model = kg_basic();
    let count = |module: &str, id: &str| facts_of(&model, module, id)["specializations"].clone();
    assert_eq!(count("Shapes", "Acme::Kg::Shapes.twice"), 2);
    assert_eq!(count("Shapes", "Acme::Kg::Shapes.area"), 0);
    assert_eq!(count("Main", "Acme::Kg::Main.total"), 0);
}

#[test]
#[ignore = "a measurement tool: writes the graphs to target/kg-dump"]
fn dump_kg_for_inspection() {
    let out = root().join("target/kg-dump");
    for (name, model) in [
        ("point", example("point")),
        ("polymorphism", example("polymorphism")),
        ("kg_basic", kg_basic()),
    ] {
        let directory = out.join(name);
        std::fs::create_dir_all(directory.join("facts")).unwrap();
        std::fs::create_dir_all(directory.join("layout")).unwrap();
        std::fs::write(
            directory.join("nodes.jsonl"),
            text(|o| model.write_nodes(o)),
        )
        .unwrap();
        std::fs::write(
            directory.join("edges.jsonl"),
            text(|o| model.write_edges(o)),
        )
        .unwrap();
        std::fs::write(
            directory.join("tests.jsonl"),
            text(|o| model.write_tests(o)),
        )
        .unwrap();
        for module in model.fact_modules() {
            std::fs::write(
                directory.join(format!("facts/{module}.jsonl")),
                text(|o| model.write_facts(module, o)),
            )
            .unwrap();
        }
        for module in model.layout_modules() {
            std::fs::write(
                directory.join(format!("layout/{module}.jsonl")),
                text(|o| model.write_layout(module, o)),
            )
            .unwrap();
        }
    }
}

/// Appendix J2: the cost of the knowledge graph over a plain analysis, for every project of
/// `tests/fixtures`, `examples` and `benchmarks` (medians of 5). A measurement, not a threshold:
/// `cargo test --locked --release --test kg -- --ignored --nocapture measure`.
#[test]
#[ignore = "a measurement tool"]
fn measure_kg_cost_for_shipped_projects() {
    let mut projects = Vec::new();
    for directory in ["tests/fixtures", "examples", "benchmarks"] {
        let Ok(entries) = std::fs::read_dir(root().join(directory)) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.join("Main.tz").is_file() || path.join("Tsuzuri.toml").is_file() {
                projects.push(path);
            }
        }
    }
    projects.sort();
    let median = |mut values: Vec<f64>| {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let mut rows = Vec::new();
    for path in projects {
        let Ok(project) = tsuzuri::driver::Project::load(&path) else {
            continue;
        };
        if project.analyze_all().is_err() {
            continue;
        }
        let time = |work: &dyn Fn()| {
            median(
                (0..5)
                    .map(|_| {
                        let start = std::time::Instant::now();
                        work();
                        start.elapsed().as_secs_f64() * 1000.0
                    })
                    .collect(),
            )
        };
        let check = time(&|| {
            project.analyze_all().unwrap();
        });
        let collect = time(&|| {
            kg::collect(&project).unwrap();
        });
        rows.push((
            path.strip_prefix(root()).unwrap().display().to_string(),
            check,
            collect,
        ));
    }
    let mut ratios: Vec<f64> = Vec::new();
    for (name, check, collect) in &rows {
        let ratio = (collect - check) / check;
        ratios.push(ratio);
        eprintln!("{name}\tcheck {check:.1} ms\tkg {collect:.1} ms\tratio {ratio:.2}");
    }
    let mut sorted = ratios.clone();
    sorted.sort_by(f64::total_cmp);
    eprintln!(
        "projects {}  ratio median {:.2}  max {:.2}",
        rows.len(),
        sorted[sorted.len() / 2],
        sorted[sorted.len() - 1]
    );
    let mut slowest = rows.clone();
    slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (name, check, collect) in slowest.iter().take(5) {
        eprintln!("slowest check: {name}\tcheck {check:.1} ms\tkg {collect:.1} ms");
    }
}
