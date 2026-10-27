//! G30 (D-60): the diagrams from the rows of G30 appendix C. The expected outputs are those of
//! the reference implementation (appendix E), as appendix D lists them.

use std::collections::BTreeMap;
use std::path::Path;

use tsuzuri::diagram::{
    DiagramRequest, Format, Input, Kind, mermaid_blocks, render, validate_mermaid, views,
};

fn repository() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repository().join(path)).unwrap_or_default()
}

fn golden(name: &str) -> String {
    let text = read(&format!("tests/golden/diagram/{name}"));
    assert!(!text.is_empty(), "{name}");
    text
}

/// The input of a fixture of appendix C.
fn fixture(name: &str) -> Input {
    let file = |file: &str| read(&format!("tests/fixtures/diagram/kg/{name}/{file}.jsonl"));
    let (facts, layout) = (file("facts"), file("layout"));
    Input::from_texts(&file("nodes"), &file("edges"), &[&facts], &[&layout], "").unwrap()
}

fn no_sources() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn request(kind: Kind, id: Option<&str>) -> DiagramRequest {
    DiagramRequest {
        id: id.map(str::to_owned),
        ..DiagramRequest::new(kind)
    }
}

fn draw(input: &Input, request: &DiagramRequest) -> String {
    render(input, &no_sources(), request).unwrap().text
}

fn dot(mut request: DiagramRequest) -> DiagramRequest {
    request.format = Format::Dot;
    request
}

fn folded(mut request: DiagramRequest, max_nodes: usize) -> DiagramRequest {
    request.max_nodes = max_nodes;
    request
}

#[test]
fn module_diagram_of_point_matches_the_golden() {
    let point = fixture("point");
    let output = render(&point, &no_sources(), &request(Kind::Module, None)).unwrap();
    assert_eq!(output.text, golden("point.module.mmd"));
    assert_eq!((output.nodes, output.folded), (2, false));
    assert_eq!(output.format, Format::Mermaid);
}

#[test]
fn module_diagram_dot_matches_the_golden() {
    let request = dot(request(Kind::Module, None));
    assert_eq!(
        draw(&fixture("point"), &request),
        golden("point.module.dot")
    );
    assert_eq!(
        draw(&fixture("cycle"), &request),
        golden("cycle.module.dot")
    );
}

#[test]
fn module_diagram_marks_cycles() {
    assert_eq!(
        draw(&fixture("cycle"), &request(Kind::Module, None)),
        golden("cycle.module.mmd")
    );
}

/// Rows of `count` modules `M00`, `M01`, ..., each calling the next.
fn chain(count: usize) -> Input {
    let mut nodes = String::new();
    let mut edges = String::new();
    for index in 0..count {
        let module = format!("M{index:02}");
        nodes.push_str(&format!("{{\"id\":\"{module}\",\"kind\":\"module\",\"module\":\"{module}\",\"name\":\"{module}\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{{\"path\":\"{module}.tz\",\"line\":1,\"column\":1}}}}\n"));
        nodes.push_str(&format!("{{\"id\":\"{module}.f\",\"kind\":\"def\",\"module\":\"{module}\",\"name\":\"f\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{{\"path\":\"{module}.tz\",\"line\":1,\"column\":5}}}}\n"));
        if index + 1 < count {
            edges.push_str(&format!("{{\"rel\":\"calls\",\"from\":\"{module}.f\",\"to\":\"M{:02}.f\",\"args\":[],\"span\":{{\"line\":1,\"column\":9}}}}\n", index + 1));
        }
    }
    Input::from_texts(&nodes, &edges, &[], &[], "").unwrap()
}

#[test]
fn module_diagram_folds_at_max_nodes() {
    let output = render(
        &chain(8),
        &no_sources(),
        &folded(request(Kind::Module, None), 5),
    )
    .unwrap();
    assert!(output.folded);
    // M01 to M04 have 2 edges each (then by name); M00, M05, M06 and M07 become one node. The
    // edges M04 -> M05 and M00 -> M01 go to and from it; M05 -> M06 and M06 -> M07 vanish in it.
    assert_eq!(
        output.text,
        "flowchart TD\n    n1[\"M01\"]\n    n2[\"M02\"]\n    n3[\"M03\"]\n    n4[\"M04\"]\n    n5[\"\u{2026}4 more modules\"]\n    n1 --> n2\n    n2 --> n3\n    n3 --> n4\n    n4 --> n5\n    n5 --> n1\n    classDef dep fill:#f6f8fa,stroke:#8c959f\n    class n5 dep\n"
    );
    validate_mermaid(&output.text).unwrap();
}

#[test]
fn module_using_edges_are_dashed() {
    let nodes = [
        ("App::Main", "project"),
        ("App::Util::Text", "project"),
        ("App::Util::Deep::More", "project"),
    ]
    .iter()
    .map(|(id, origin)| format!("{{\"id\":\"{id}\",\"kind\":\"module\",\"module\":\"{id}\",\"name\":\"{id}\",\"params\":[],\"result\":null,\"origin\":\"{origin}\",\"span\":{{\"path\":\"x.tz\",\"line\":1,\"column\":1}}}}\n"))
    .collect::<String>();
    let edges = "{\"rel\":\"imports\",\"from\":\"App::Main\",\"to\":\"App::Util\",\"span\":{\"line\":2,\"column\":1}}\n";
    let input = Input::from_texts(&nodes, edges, &[], &[], "").unwrap();
    let text = draw(&input, &request(Kind::Module, None));
    // `App::Util::Deep::More` is not directly in the namespace.
    assert_eq!(
        text,
        "flowchart TD\n    n1[\"App::Main\"]\n    n2[\"App::Util::Deep::More\"]\n    n3[\"App::Util::Text\"]\n    n1 -.->|\"using\"| n3\n"
    );
}

#[test]
fn types_diagram_matches_the_golden() {
    let shapes = fixture("shapes");
    let output = render(&shapes, &no_sources(), &request(Kind::Types, None)).unwrap();
    assert_eq!(output.text, golden("shapes.types.mmd"));
    assert_eq!((output.nodes, output.folded), (6, false));
}

#[test]
fn types_diagram_dot_matches_the_golden() {
    assert_eq!(
        draw(&fixture("shapes"), &dot(request(Kind::Types, None))),
        golden("shapes.types.dot")
    );
}

#[test]
fn types_diagram_folds_by_module() {
    let output = render(
        &fixture("shapes"),
        &no_sources(),
        &folded(request(Kind::Types, None), 5),
    )
    .unwrap();
    assert_eq!(output.text, golden("shapes.types.folded.mmd"));
    assert_eq!((output.nodes, output.folded), (3, true));
}

#[test]
fn types_member_limit_is_twelve() {
    let mut nodes = String::from(
        "{\"id\":\"M\",\"kind\":\"module\",\"module\":\"M\",\"name\":\"M\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n{\"id\":\"M.R\",\"kind\":\"record\",\"module\":\"M\",\"name\":\"R\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":8}}\n",
    );
    for index in 0..14 {
        nodes.push_str(&format!("{{\"id\":\"M.R.f{index:02}\",\"kind\":\"field\",\"module\":\"M\",\"name\":\"f{index:02}\",\"params\":[],\"result\":\"Maybe<i64>\",\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":{},\"column\":3}}}}\n", index + 2));
    }
    let input = Input::from_texts(&nodes, "", &[], &[], "").unwrap();
    let text = draw(&input, &request(Kind::Types, None));
    assert!(
        text.contains("        +f11 Maybe#lt;i64#gt;\n        +\u{2026} 2 more\n    }\n"),
        "{text}"
    );
    assert!(!text.contains("+f12"));
    validate_mermaid(&text).unwrap();
}

#[test]
fn types_derive_and_instance_relations() {
    let text = draw(&fixture("shapes"), &request(Kind::Types, None));
    // `deriving (Eq)` on Point, and the instance `Sized<Point>` that points at Point.
    assert!(
        text.contains(" ..|> n1 : derive\n") || text.contains(" : derive\n"),
        "{text}"
    );
    assert!(
        text.contains(" ..> ") && text.contains(" : for\n"),
        "{text}"
    );
    assert!(text.contains("        <<builtin>>\n"), "{text}");
}

#[test]
fn calls_diagram_of_hypotenuse_matches_the_golden() {
    let point = fixture("point");
    let calls = request(Kind::Calls, Some("Point.hypotenuse"));
    assert_eq!(draw(&point, &calls), golden("point.calls.mmd"));
    assert_eq!(draw(&point, &dot(calls)), golden("point.calls.dot"));
}

#[test]
fn calls_flow_matches_the_golden() {
    let flow = fixture("flow");
    assert_eq!(
        draw(&flow, &request(Kind::Calls, Some("Main.$entry"))),
        golden("flow.calls.mmd")
    );
    let output = render(
        &flow,
        &no_sources(),
        &folded(request(Kind::Calls, Some("Main.$entry")), 5),
    )
    .unwrap();
    assert!(output.folded);
    assert_eq!(output.text, golden("flow.calls.folded.mmd"));
}

#[test]
fn calls_depth_marks_truncated_nodes() {
    let mut calls = request(Kind::Calls, Some("Main.$entry"));
    calls.depth = 1;
    let text = draw(&fixture("point"), &calls);
    assert!(text.contains("[\"Point.distance \u{2026}\"]"), "{text}");
    calls.depth = 2;
    assert!(!draw(&fixture("point"), &calls).contains('\u{2026}'));
}

#[test]
fn calls_merges_edges_and_counts_sites() {
    let text = draw(&fixture("flow"), &request(Kind::Calls, Some("Main.$entry")));
    assert!(text.contains("|\"ref,copy,copy \u{d7}2\"|"), "{text}");
}

#[test]
fn calls_needs_a_function_id() {
    let point = fixture("point");
    let error = render(&point, &no_sources(), &request(Kind::Calls, None)).unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        ("diagram_needs_id", "diagram kind 'calls' needs an id")
    );
    let error = render(
        &point,
        &no_sources(),
        &request(Kind::Calls, Some("Point.Point")),
    )
    .unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        (
            "diagram_wrong_id_kind",
            "'Point.Point' is a record; diagram kind 'calls' needs a function id"
        )
    );
}

#[test]
fn effect_class_follows_the_table() {
    let nodes = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| format!("{{\"id\":\"M.{name}\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"{name}\",\"params\":[],\"result\":\"i64\",\"pure\":{},\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n", *name == "e"))
        .collect::<String>();
    let edges = ["b", "c", "d", "e", "f"]
        .iter()
        .map(|name| format!("{{\"rel\":\"calls\",\"from\":\"M.a\",\"to\":\"M.{name}\",\"args\":[],\"span\":{{\"line\":1,\"column\":1}}}}\n"))
        .collect::<String>()
        + "{\"rel\":\"calls\",\"from\":\"M.a\",\"to\":\"sqrt\",\"args\":[],\"span\":{\"line\":2,\"column\":1}}\n";
    let facts = "{\"id\":\"M.a\",\"effects\":[\"net\",\"alloc\"],\"effects_complete\":false}\n\
                 {\"id\":\"M.b\",\"effects\":[\"copy\",\"trap\"],\"effects_complete\":false}\n\
                 {\"id\":\"M.c\",\"effects\":[\"diverge\"],\"effects_complete\":false}\n\
                 {\"id\":\"M.d\",\"effects\":[],\"effects_complete\":true}\n\
                 {\"id\":\"M.e\",\"effects\":[],\"effects_complete\":false}\n";
    let input = Input::from_texts(&nodes, &edges, &[facts], &[], "").unwrap();
    let text = draw(&input, &request(Kind::Calls, Some("M.a")));
    for line in [
        "    class n7 pure\n".replace("n7", "n4,n5"),
        "    class n1 io\n".to_owned(),
        "    class n2 alloc\n".to_owned(),
        "    class n3 trap\n".to_owned(),
        "    class n7 ext\n".to_owned(),
    ] {
        assert!(text.contains(&line), "{line}{text}");
    }
    assert!(text.contains("    n7([\"sqrt\"])\n"), "{text}");
    // `M.f` has no facts row: no class.
    assert!(!text.contains("n6 "), "{text}");
}

#[test]
fn concurrency_matches_the_golden() {
    let flow = fixture("flow");
    let output = render(&flow, &no_sources(), &request(Kind::Concurrency, None)).unwrap();
    assert_eq!(output.text, golden("flow.concurrency.mmd"));
    assert!(!output.folded);
    let output = render(
        &flow,
        &no_sources(),
        &folded(request(Kind::Concurrency, None), 6),
    )
    .unwrap();
    assert_eq!(output.text, golden("flow.concurrency.folded.mmd"));
    assert!(output.folded);
}

#[test]
fn concurrency_dot_matches_the_golden() {
    assert_eq!(
        draw(&fixture("flow"), &dot(request(Kind::Concurrency, None))),
        golden("flow.concurrency.dot")
    );
}

#[test]
fn concurrency_unique_element_type_attaches_creators() {
    let symbol = |id: &str, param: &str| {
        format!(
            "{{\"id\":\"M.{id}\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"{id}\",\"params\":[{{\"name\":\"c\",\"type\":\"{param}\",\"conv\":\"ref\"}}],\"result\":\"i64\",\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n"
        )
    };
    let creator = "{\"id\":\"M.make\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"make\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n";
    let edges = "{\"rel\":\"calls\",\"from\":\"M.make\",\"to\":\"Channel.bounded\",\"args\":[],\"span\":{\"line\":3,\"column\":1}}\n";
    let one = Input::from_texts(
        &(symbol("send", "ref Channel.Sender<i64>") + creator),
        edges,
        &[],
        &[],
        "",
    )
    .unwrap();
    let text = draw(&one, &request(Kind::Concurrency, None));
    assert!(
        text.contains("(\"Channel#lt;i64#gt;\")") && !text.contains('?'),
        "{text}"
    );
    let two = Input::from_texts(
        &(symbol("send", "ref Channel.Sender<i64>")
            + &symbol("other", "ref Channel.Sender<string>")
            + creator),
        edges,
        &[],
        &[],
        "",
    )
    .unwrap();
    assert!(draw(&two, &request(Kind::Concurrency, None)).contains("(\"Channel#lt;?#gt;\")"));
}

#[test]
fn ownership_markdown_matches_the_golden() {
    let own = fixture("own");
    let output = render(
        &own,
        &no_sources(),
        &request(Kind::Ownership, Some("Main.demo")),
    )
    .unwrap();
    assert_eq!(output.text, golden("own.ownership.md"));
    assert_eq!(output.format, Format::Markdown);
    for block in mermaid_blocks(&output.text) {
        validate_mermaid(&block).unwrap();
    }
}

/// A function `M.f` with the given ownership events (`var`, `event`, line).
fn owning(events: &[(&str, &str, usize)]) -> Input {
    let nodes = "{\"id\":\"M.f\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"f\",\"params\":[{\"name\":\"p\",\"type\":\"string\",\"conv\":\"own\"}],\"result\":\"i64\",\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n";
    let events: Vec<String> = events
        .iter()
        .map(|(var, event, line)| format!("{{\"var\":\"{var}\",\"event\":\"{event}\",\"span\":{{\"path\":\"M.tz\",\"line\":{line},\"column\":5}}}}"))
        .collect();
    let facts = format!(
        "{{\"id\":\"M.f\",\"effects\":[],\"effects_complete\":false,\"ownership\":[{}]}}\n",
        events.join(",")
    );
    Input::from_texts(nodes, "", &[&facts], &[], "").unwrap()
}

#[test]
fn ownership_release_precedes_the_next_event() {
    let input = owning(&[("p", "borrow", 2), ("p", "move", 3)]);
    let text = draw(&input, &request(Kind::Ownership, Some("M.f")));
    assert!(text.contains("## p (parameter: string)\n"), "{text}");
    assert!(
        text.contains(
            "    s1 --> s2 : release\n    state \"Moved\" as s3\n    s2 --> s3 : L3 move\n"
        ),
        "{text}"
    );
}

#[test]
fn ownership_reborrow_keeps_the_borrow() {
    let input = owning(&[("p", "borrow_mut", 2), ("p", "reborrow", 3)]);
    let text = draw(&input, &request(Kind::Ownership, Some("M.f")));
    assert!(
        text.contains("    state \"BorrowedMut\" as s2\n    s1 --> s2 : L3 reborrow\n"),
        "{text}"
    );
}

#[test]
fn ownership_limits_variables() {
    let names: Vec<String> = (0..13).map(|index| format!("v{index:02}")).collect();
    let events: Vec<(&str, &str, usize)> = names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), "borrow", index + 2))
        .collect();
    let output = render(
        &owning(&events),
        &no_sources(),
        &request(Kind::Ownership, Some("M.f")),
    )
    .unwrap();
    assert_eq!((output.nodes, output.folded), (12, true));
    assert!(output.text.contains("## v11\n") && !output.text.contains("## v12\n"));
    assert!(
        output
            .text
            .contains("1 variables are not drawn; the table lists every event.\n")
    );
    assert!(
        output
            .text
            .contains("| 13 | 14 | v12 | borrow | - | borrowed_here |\n")
    );
}

#[test]
fn ownership_without_facts_is_unavailable() {
    let error = render(
        &fixture("point"),
        &no_sources(),
        &request(Kind::Ownership, Some("Point.distance")),
    );
    // point's facts rows have an empty `ownership`: a diagram with no events.
    assert!(error.unwrap().text.contains("(no ownership events)\n"));
    let input = Input::from_texts(
        "{\"id\":\"M.f\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"f\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n",
        "",
        &["{\"id\":\"M.f\",\"effects\":[],\"effects_complete\":false}\n"],
        &[],
        "",
    )
    .unwrap();
    let error = render(
        &input,
        &no_sources(),
        &request(Kind::Ownership, Some("M.f")),
    )
    .unwrap_err();
    assert_eq!(error.kind, "diagram_unavailable");
    assert_eq!(
        error.message,
        "the KG has no ownership facts for 'M.f'; build the KG with a compiler that records them"
    );
}

#[test]
fn ownership_ledger_roles() {
    let input = owning(&[("p", "borrow", 2), ("p", "drop", 4), ("q", "move", 3)]);
    let text = draw(&input, &request(Kind::Ownership, Some("M.f")));
    assert!(
        text.contains("| 1 | 2 | p | borrow | - | borrowed_here |\n"),
        "{text}"
    );
    assert!(
        text.contains("| 2 | 3 | q | move | - | moved_here |\n"),
        "{text}"
    );
    assert!(text.contains("| 3 | 4 | p | drop | - | - |\n"), "{text}");
}

#[test]
fn layout_tables_match_the_goldens() {
    let layout = fixture("layout");
    assert_eq!(
        draw(
            &layout,
            &request(Kind::Layout, Some("Acme::Kb::Shapes.Packed"))
        ),
        golden("layout.packed.md")
    );
    assert_eq!(
        draw(
            &layout,
            &request(Kind::Layout, Some("Acme::Kb::Shapes.Shape"))
        ),
        golden("layout.shape.md")
    );
}

#[test]
fn layout_offsets_stop_at_the_first_unknown_type() {
    let row = "{\"id\":\"M.R\",\"kind\":\"record\",\"type\":\"M.R\",\"size\":[24,24],\"align\":[8,8],\"fields\":[{\"name\":\"a\",\"type\":\"i32\",\"slot\":0},{\"name\":\"s\",\"type\":\"string\",\"slot\":1},{\"name\":\"b\",\"type\":\"i8\",\"slot\":2}],\"reordered\":false,\"drop_flag\":false}\n";
    let input = Input::from_texts("", "", &[], &[row], "").unwrap();
    let text = draw(&input, &request(Kind::Layout, Some("M.R")));
    assert!(text.contains("| 0 | a | i32 | 4 | 4 | 0 |\n| 1 | s | string | ? | ? | ? |\n| 2 | b | i8 | ? | ? | ? |\n"), "{text}");
    assert!(!text.contains("tail padding"));
}

#[test]
fn layout_reports_unknown_id() {
    let layout = fixture("layout");
    let error = render(
        &layout,
        &no_sources(),
        &request(Kind::Layout, Some("Packd")),
    )
    .unwrap_err();
    assert_eq!(error.kind, "diagram_unknown_id");
    let error = render(
        &fixture("point"),
        &no_sources(),
        &request(Kind::Layout, Some("Point.Point")),
    )
    .unwrap_err();
    assert_eq!(error.kind, "diagram_unavailable");
    // A unique suffix names the row, as the queries of `kg` do.
    assert!(
        render(
            &layout,
            &no_sources(),
            &request(Kind::Layout, Some("Packed"))
        )
        .is_ok()
    );
}

/// The light fixture with its source, as a graph built from it would have them.
fn light() -> (Input, BTreeMap<String, String>) {
    let source = read("tests/fixtures/diagram/src/light/Light.tz");
    let manifest = format!(
        "{{\"edition\":\"2026\",\"modules\":[{{\"name\":\"Light\",\"key\":\"Light\",\"origin\":\"project\",\"path\":\"Light.tz\",\"source\":\"{}\"}}]}}",
        sha256(&source)
    );
    let nodes = read("tests/fixtures/diagram/kg/light/nodes.jsonl");
    let input = Input::from_texts(&nodes, "", &[], &[], &manifest).unwrap();
    (input, BTreeMap::from([("Light.tz".to_owned(), source)]))
}

fn sha256(text: &str) -> String {
    let output = std::process::Command::new("shasum")
        .args(["-a", "256"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(text.as_bytes())?;
            child.wait_with_output()
        });
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned(),
        _ => tsuzuri::kg::input::digest(text.as_bytes()),
    }
}

#[test]
fn transitions_match_the_golden() {
    let (input, sources) = light();
    let output = render(
        &input,
        &sources,
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap();
    assert_eq!(output.text, golden("light.transitions.mmd"));
    assert_eq!(output.nodes, 3);
}

#[test]
fn transitions_dot_matches_the_golden() {
    let (input, sources) = light();
    let output = render(
        &input,
        &sources,
        &dot(request(Kind::Transitions, Some("Light.Light"))),
    )
    .unwrap();
    assert_eq!(output.text, golden("light.transitions.dot"));
}

#[test]
fn transitions_wildcard_means_the_remaining_cases() {
    let (input, mut sources) = light();
    // Only `reset`: `Red -> Red` and `_ -> Red` for Green and Yellow.
    let text = sources["Light.tz"].clone();
    let without_next = text.replace("    | Red -> Green\n", "    | Red -> Red\n");
    sources.insert("Light.tz".into(), without_next.clone());
    let manifest_input = {
        let nodes = read("tests/fixtures/diagram/kg/light/nodes.jsonl");
        let manifest = format!(
            "{{\"edition\":\"2026\",\"modules\":[{{\"name\":\"Light\",\"key\":\"Light\",\"origin\":\"project\",\"path\":\"Light.tz\",\"source\":\"{}\"}}]}}",
            sha256(&without_next)
        );
        Input::from_texts(&nodes, "", &[], &[], &manifest).unwrap()
    };
    let output = render(
        &manifest_input,
        &sources,
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap();
    assert!(
        output.text.contains("    s1 --> s1 : next, reset\n"),
        "{}",
        output.text
    );
    assert!(
        output.text.contains("    s2 --> s1 : reset\n"),
        "{}",
        output.text
    );
    drop(input);
}

#[test]
fn transitions_reject_a_changed_source() {
    let (input, mut sources) = light();
    let changed = format!("{}\n", sources["Light.tz"]);
    sources.insert("Light.tz".into(), changed);
    let error = render(
        &input,
        &sources,
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap_err();
    assert_eq!(error.kind, "kg_stale");
    assert_eq!(
        error.message,
        "source 'Light.tz' changed after the KG was built; run tsuzuri kg build"
    );
    let error = render(
        &input,
        &no_sources(),
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        ("kg_broken", "cannot read source 'Light.tz'")
    );
}

#[test]
fn transitions_without_candidates_is_unavailable() {
    let (_, sources) = light();
    let nodes: String = read("tests/fixtures/diagram/kg/light/nodes.jsonl")
        .lines()
        .filter(|line| !line.contains("\"kind\":\"def\""))
        .map(|line| format!("{line}\n"))
        .collect();
    let input = Input::from_texts(&nodes, "", &[], &[], "").unwrap();
    let error = render(
        &input,
        &sources,
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap_err();
    assert_eq!(error.kind, "diagram_unavailable");
    assert_eq!(
        error.message,
        "no function of type Light.Light -> \u{2026} -> Light.Light with a 'match' over its first parameter was found"
    );
}

#[test]
fn transitions_ignore_guards() {
    let (_, mut sources) = light();
    let text = sources["Light.tz"].replace("    | Red -> Red\n", "    | Red when true -> Red\n");
    sources.insert("Light.tz".into(), text.clone());
    let nodes = read("tests/fixtures/diagram/kg/light/nodes.jsonl");
    let manifest = format!(
        "{{\"edition\":\"2026\",\"modules\":[{{\"name\":\"Light\",\"key\":\"Light\",\"origin\":\"project\",\"path\":\"Light.tz\",\"source\":\"{}\"}}]}}",
        sha256(&text)
    );
    let input = Input::from_texts(&nodes, "", &[], &[], &manifest).unwrap();
    let output = render(
        &input,
        &sources,
        &request(Kind::Transitions, Some("Light.Light")),
    )
    .unwrap();
    assert_eq!(output.text, golden("light.transitions.mmd"));
}

#[test]
fn views_for_shapes_and_flow() {
    let shapes: Vec<String> = views(&fixture("shapes"))
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(shapes, ["views/modules.mmd", "views/types.mmd"]);
    for (path, text) in views(&fixture("shapes")) {
        let name = format!("shapes.{}", path.replace('/', "."));
        let expected = match name.as_str() {
            "shapes.views.types.mmd" => golden("shapes.types.mmd"),
            other => golden(other),
        };
        assert_eq!(text, expected, "{name}");
    }
    let flow = views(&fixture("flow"));
    let paths: Vec<&str> = flow.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, ["views/concurrency.mmd", "views/modules.mmd"]);
    assert_eq!(flow[0].1, golden("flow.concurrency.mmd"));
    assert_eq!(flow[1].1, golden("flow.views.modules.mmd"));
}

/// Every file of the golden directories.
fn golden_files() -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for directory in ["tests/golden/diagram", "tests/golden/diagram/cli"] {
        for entry in std::fs::read_dir(repository().join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn validator_accepts_every_golden() {
    let mut checked = 0;
    for path in golden_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let texts = match path.extension().and_then(|e| e.to_str()) {
            Some("mmd") => vec![text],
            Some("md") => mermaid_blocks(&text),
            _ => continue,
        };
        for text in texts {
            validate_mermaid(&text)
                .unwrap_or_else(|errors| panic!("{}: {errors:?}", path.display()));
            checked += 1;
        }
    }
    // Appendix D: 12 Mermaid files and the 2 blocks of `own.ownership.md`; the CLI's: 11 and 2.
    assert_eq!(checked, 27);
}

#[test]
fn validator_rejects_each_negative_case() {
    for (name, expected) in [
        (
            "end-id.mmd",
            vec![
                "line 3: unknown form: '    end[\"b\"]'",
                "line 4: unknown form: '    n1 --> end'",
            ],
        ),
        (
            "raw-quote.mmd",
            vec!["line 2: raw special character in label"],
        ),
        (
            "unquoted-parens.mmd",
            vec![
                "line 2: unknown form: '    n1[f (x) -> y]'",
                "line 4: n1 is not declared",
            ],
        ),
        (
            "bad-arrow.mmd",
            vec!["line 6: unknown form: '    s2 -> s1 : reset'"],
        ),
        (
            "raw-angle.mmd",
            vec!["line 2: raw special character in label"],
        ),
        ("undeclared-node.mmd", vec!["line 3: n2 is not declared"]),
        ("class-undeclared.mmd", vec!["line 7: n2 is not declared"]),
    ] {
        let text = read(&format!("tests/fixtures/diagram/neg/{name}"));
        assert_eq!(validate_mermaid(&text).unwrap_err(), expected, "{name}");
    }
}

#[test]
fn every_golden_has_one_trailing_newline() {
    for path in golden_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.ends_with('\n') && !text.ends_with("\n\n"),
            "{}",
            path.display()
        );
    }
}

#[test]
fn render_twice_gives_the_same_bytes() {
    for (name, request) in [
        ("shapes", request(Kind::Types, None)),
        ("flow", request(Kind::Concurrency, None)),
        ("flow", request(Kind::Calls, Some("Main.$entry"))),
        ("cycle", request(Kind::Module, None)),
    ] {
        assert_eq!(
            draw(&fixture(name), &request),
            draw(&fixture(name), &request)
        );
    }
}

#[test]
fn flowchart_truncates_edges() {
    let mut nodes = String::new();
    let mut edges = String::new();
    for index in 0..450 {
        nodes.push_str(&format!("{{\"id\":\"M.f{index:03}\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"f{index:03}\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n"));
        edges.push_str(&format!("{{\"rel\":\"calls\",\"from\":\"M.root\",\"to\":\"M.f{index:03}\",\"args\":[],\"span\":{{\"line\":{},\"column\":1}}}}\n", index + 1));
        edges.push_str(&format!("{{\"rel\":\"calls\",\"from\":\"M.f{index:03}\",\"to\":\"M.root\",\"args\":[],\"span\":{{\"line\":1,\"column\":1}}}}\n"));
    }
    nodes.push_str("{\"id\":\"M.root\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"root\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n");
    let input = Input::from_texts(&nodes, &edges, &[], &[], "").unwrap();
    // 451 calls nodes fold to 200; the edges of the folded graph are still drawn in full.
    let mut calls = request(Kind::Calls, Some("M.root"));
    calls.max_nodes = 200;
    calls.depth = 1;
    let output = render(&input, &no_sources(), &calls).unwrap();
    assert!(output.folded);
    validate_mermaid(&output.text).unwrap();
    // 30 modules that all call each other: 870 edges, of which 400 are drawn.
    let module_nodes: String = (0..30)
        .map(|index| format!("{{\"id\":\"M{index:02}\",\"kind\":\"module\",\"module\":\"M{index:02}\",\"name\":\"M{index:02}\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n{{\"id\":\"M{index:02}.f\",\"kind\":\"def\",\"module\":\"M{index:02}\",\"name\":\"f\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n"))
        .collect();
    let mut module_edges = String::new();
    for a in 0..30 {
        for b in 0..30 {
            if a != b {
                module_edges.push_str(&format!("{{\"rel\":\"calls\",\"from\":\"M{a:02}.f\",\"to\":\"M{b:02}.f\",\"args\":[],\"span\":{{\"line\":1,\"column\":1}}}}\n"));
            }
        }
    }
    let input = Input::from_texts(&module_nodes, &module_edges, &[], &[], "").unwrap();
    let output = render(&input, &no_sources(), &request(Kind::Module, None)).unwrap();
    assert!(output.folded);
    assert!(
        output.text.ends_with("    %% omitted: 470 edges\n"),
        "{}",
        &output.text[output.text.len() - 200..]
    );
    assert_eq!(output.text.matches(" --> ").count(), 400);
    validate_mermaid(&output.text).unwrap();
}

/// A calls graph of `M.r` and 199 callees, both ways, with labels of `#` (4 characters each once
/// escaped).
fn hashes(label: usize, args: usize) -> Input {
    let id = |index: usize| format!("M.{}{index:03}", "#".repeat(label));
    let node = |index: usize| {
        format!(
            "{{\"id\":\"{}\",\"kind\":\"def\",\"module\":\"M\",\"name\":\"f\",\"params\":[],\"result\":\"i64\",\"origin\":\"project\",\"span\":{{\"path\":\"M.tz\",\"line\":1,\"column\":1}}}}\n",
            id(index)
        )
    };
    let edge = |from: usize, to: usize, line: usize| {
        format!(
            "{{\"rel\":\"calls\",\"from\":\"{}\",\"to\":\"{}\",\"args\":[\"{}\"],\"span\":{{\"line\":{line},\"column\":1}}}}\n",
            id(from),
            id(to),
            "#".repeat(args)
        )
    };
    let nodes: String = (0..200).map(node).collect();
    let mut edges = String::new();
    for index in 1..200 {
        edges.push_str(&edge(0, index, index));
        if args > 0 {
            edges.push_str(&edge(index, 0, 1));
        }
    }
    Input::from_texts(&nodes, &edges, &[], &[], "").unwrap()
}

#[test]
fn too_large_text_is_reported() {
    let calls = |input: &Input| {
        let mut request = request(Kind::Calls, Some(&input.symbols[0].id));
        request.max_nodes = 200;
        request.depth = 1;
        render(input, &no_sources(), &request)
    };
    // 200 labels of 59 `#` are about 50,000 characters; with the limits halved, 29 fit.
    let output = calls(&hashes(100, 0)).unwrap();
    let length = output.text.chars().count();
    assert!((25_000..=45_000).contains(&length), "{length}");
    assert!(
        output
            .text
            .contains(&format!("[\"M.{}\u{2026}\"]", "#35;".repeat(27)))
    );
    // With 398 labeled edges the halved text is still over the limit.
    let error = calls(&hashes(100, 100)).unwrap_err();
    assert_eq!(error.kind, "diagram_too_large");
    assert!(
        error.message.starts_with("the diagram is too large (")
            && error.message.ends_with(" characters); lower --max-nodes"),
        "{}",
        error.message
    );
}

#[test]
fn input_ignores_unknown_keys() {
    let nodes = "{\"id\":\"M\",\"kind\":\"module\",\"module\":\"M\",\"name\":\"M\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1},\"extra\":{\"x\":1},\"signature\":\"module M\"}\n";
    assert!(Input::from_texts(nodes, "", &[], &[], "").is_ok());
}

#[test]
fn input_reports_the_missing_key() {
    let nodes = "{\"id\":\"M\",\"kind\":\"module\",\"module\":\"M\",\"params\":[],\"result\":null,\"origin\":\"project\",\"span\":{\"path\":\"M.tz\",\"line\":1,\"column\":1}}\n";
    let error = Input::from_texts(nodes, "", &[], &[], "").unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        ("kg_broken", "bad row in 'symbols' line 1: missing 'name'")
    );
    let edges = "\n{\"rel\":\"calls\",\"from\":\"a\",\"span\":{\"line\":1}}\n";
    let error = Input::from_texts("", edges, &[], &[], "").unwrap_err();
    assert_eq!(error.message, "bad row in 'graph' line 2: missing 'to'");
}

#[test]
fn resolve_suggests_by_last_segment() {
    let point = fixture("point");
    let error = render(
        &point,
        &no_sources(),
        &request(Kind::Calls, Some("Other.distance")),
    )
    .unwrap_err();
    assert_eq!(
        (error.kind, error.message.as_str()),
        (
            "diagram_unknown_id",
            "unknown id 'Other.distance' for diagram kind 'calls'; did you mean: Point.distance"
        )
    );
    let error = render(&point, &no_sources(), &request(Kind::Calls, Some("zzz"))).unwrap_err();
    assert_eq!(error.message, "unknown id 'zzz' for diagram kind 'calls'");
    // A unique suffix resolves, as in the queries of `kg`.
    assert!(
        render(
            &point,
            &no_sources(),
            &request(Kind::Calls, Some("hypotenuse"))
        )
        .is_ok()
    );
}

#[test]
fn request_options_are_checked() {
    let point = fixture("point");
    for (request, kind, message) in [
        (
            folded(request(Kind::Module, None), 4),
            "diagram_bad_option",
            "--max-nodes must be an integer from 5 to 200",
        ),
        (
            DiagramRequest {
                depth: 7,
                ..request(Kind::Calls, Some("Point.hypotenuse"))
            },
            "diagram_bad_option",
            "--depth must be an integer from 1 to 6",
        ),
        (
            dot(request(Kind::Layout, Some("x"))),
            "diagram_format_unsupported",
            "format 'dot' is not available for kind 'layout'",
        ),
        (
            dot(request(Kind::Ownership, Some("Point.distance"))),
            "diagram_format_unsupported",
            "format 'dot' is not available for kind 'ownership'",
        ),
    ] {
        let error = render(&point, &no_sources(), &request).unwrap_err();
        assert_eq!((error.kind, error.message.as_str()), (kind, message));
    }
}

#[test]
fn input_from_kg_matches_input_from_texts() {
    use tsuzuri::kg::manifest::Manifest;
    use tsuzuri::kg::store::KgData;
    use tsuzuri::kg::write::Files;
    let file = |name: &str| read(&format!("tests/fixtures/diagram/kg/point/{name}.jsonl"));
    let mut files = Files::default();
    files.put("nodes.jsonl", file("nodes").into_bytes());
    files.put("edges.jsonl", file("edges").into_bytes());
    files.put("facts/Point.jsonl", file("facts").into_bytes());
    let manifest = Manifest {
        schema: "tz-kg/1".into(),
        compiler: "tsuzuri 0.1.0".into(),
        edition: "2026".into(),
        project: "point".into(),
        version: None,
        complete: true,
        source_hash: String::new(),
        counts: Vec::new(),
        modules: Vec::new(),
        files: files
            .0
            .keys()
            .map(|path| (path.clone(), String::new()))
            .collect(),
    };
    let data = KgData::from_parts(&manifest, &files).unwrap();
    assert_eq!(Input::from_kg(&data).unwrap(), fixture("point"));
}
