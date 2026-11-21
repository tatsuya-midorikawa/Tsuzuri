use tsuzuri::check::semantic::{Role, SemanticIndex, SymbolKind};
use tsuzuri::{analyze_modules, analyze_modules_with_semantics, llvm};

const P1_MAIN: &str = "def read :: i64 -> i64\nfn read number = { let text = \"\u{65e5}\u{1f600}\"; let result = number + text.length; result }\ndef point :: Shapes.Point -> i64\nfn point value = value.x\ndef twice :: i64 -> i64\nfn twice n = read (read n)\n";
const P1_SHAPES: &str = "record Point { x: i64 }\n";
const P1_WARN: &str = "fn warn() -> i64 { let spare = 1; 2 }\n";
const P2_MAIN: &str = "record Point { x: i64 }\nunion Shape = Circle of i64 | Empty\nconst LIMIT: i64 = 3\ndef make :: i64 -> Point\nfn make n = Point { x: n }\ndef get :: Point -> i64\nfn get p = match p with\n    | Point { x = v } -> v + LIMIT\ndef area :: Shape -> i64\nfn area s = match s with\n    | Circle r -> r\n    | Empty -> 0\ndef moved :: Point -> Point\nfn moved p = { p with x = 2 }\n";

fn definition(index: &SemanticIndex, source: usize, name: &str, kind: SymbolKind) -> usize {
    index
        .definitions
        .iter()
        .position(|item| item.span.source == Some(source) && item.name == name && item.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} {name}"))
}

fn starts(index: &SemanticIndex, definition: usize) -> Vec<(usize, usize)> {
    index
        .occurrences(definition)
        .iter()
        .map(|occurrence| (occurrence.span.source.unwrap(), occurrence.span.start))
        .collect()
}

fn at(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("missing {needle:?}"))
}

/// The LSP position of the first `needle` in `text`.
fn position(text: &str, needle: &str, encoding: &str) -> serde_json::Value {
    let prefix = &text[..at(text, needle)];
    let line = prefix.matches('\n').count();
    let column = &prefix[prefix.rfind('\n').map_or(0, |at| at + 1)..];
    let character = if encoding == "utf-8" {
        column.len()
    } else {
        column.encode_utf16().count()
    };
    serde_json::json!({"line": line, "character": character})
}

fn range(line: usize, start: usize, end: usize) -> serde_json::Value {
    serde_json::json!({"start": {"line": line, "character": start}, "end": {"line": line, "character": end}})
}

/// Runs one server session over `files` in a fresh root. `steps` receives a
/// file-name-to-URI function and returns messages; those with an `id` are
/// requests, whose responses are returned in order.
fn scripted(
    files: &[(&str, &str)],
    encoding: &str,
    steps: impl FnOnce(&dyn Fn(&str) -> String) -> Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    let (responses, ids) = session(files, encoding, steps);
    ids.iter()
        .map(|id| {
            responses
                .iter()
                .find(|response| response.get("id") == Some(id))
                .cloned()
                .unwrap_or_else(|| panic!("no response {id}"))
        })
        .collect()
}

/// Runs one server session as `scripted` does and returns every message the server wrote,
/// notifications included, with the ids of the requests that `steps` sent.
fn session(
    files: &[(&str, &str)],
    encoding: &str,
    steps: impl FnOnce(&dyn Fn(&str) -> String) -> Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    use serde_json::json;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tsuzuri::lsp::{file_uri, read_message, serve, write_message};
    // Parallel tests can read the same clock value.
    static SESSIONS: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-lsp-scripted-{}-{}-{}",
        std::process::id(),
        SESSIONS.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let uri = |name: &str| file_uri(&root.join(name)).unwrap();
    let capabilities = if encoding == "utf-8" {
        json!({"general": {"positionEncodings": ["utf-8"]}})
    } else {
        json!({})
    };
    let mut messages = vec![
        json!({"id": 0, "method": "initialize", "params": {"rootUri": file_uri(&root), "capabilities": capabilities}}),
    ];
    for (name, text) in files {
        messages.push(json!({"method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri(name), "languageId": "tsuzuri", "version": 1, "text": text}}}));
    }
    let steps = steps(&uri);
    let ids: Vec<_> = steps
        .iter()
        .filter_map(|step| step.get("id").cloned())
        .collect();
    messages.extend(steps);
    messages.push(json!({"id": "shutdown", "method": "shutdown"}));
    messages.push(json!({"method": "exit"}));
    let mut input = Vec::new();
    for mut message in messages {
        message["jsonrpc"] = json!("2.0");
        write_message(&mut input, &message).unwrap();
    }
    let mut output = Vec::new();
    assert_eq!(serve(Cursor::new(input), &mut output, Vec::new()), 0);
    let mut output = Cursor::new(output);
    let mut responses = Vec::new();
    while let Some(message) = read_message(&mut output).unwrap() {
        responses.push(message.unwrap());
    }
    std::fs::remove_dir_all(&root).unwrap();
    (responses, ids)
}

const P1: [(&str, &str); 3] = [
    ("Main.tz", P1_MAIN),
    ("Shapes.tz", P1_SHAPES),
    ("Warn.tz", P1_WARN),
];

#[test]
fn a_shebang_line_keeps_positions() {
    use serde_json::json;
    const SOURCE: &str = "#!/usr/bin/env tsuzuri script\nlet x = 41\nx + 1\n";
    let responses = scripted(&[("Main.tz", SOURCE)], "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id": 1, "method": "textDocument/documentHighlight", "params": {"textDocument": {"uri": main}, "position": position(SOURCE, "x + 1", "utf-16")}}),
            json!({"id": 2, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": position(SOURCE, "x + 1", "utf-16")}}),
        ]
    });
    assert_eq!(
        responses[0]["result"],
        json!([{"range": range(1, 4, 5), "kind": 3}, {"range": range(2, 0, 1), "kind": 2}])
    );
    assert!(
        responses[1]["result"].to_string().contains("x: i32"),
        "{}",
        responses[1]
    );
}

#[test]
fn references_and_highlights_cover_def_and_fn_heads() {
    use serde_json::json;
    let responses = scripted(&P1, "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id": 1, "method": "textDocument/references", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "read (read", "utf-16"), "context": {"includeDeclaration": true}}}),
            json!({"id": 2, "method": "textDocument/references", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "read n", "utf-16"), "context": {"includeDeclaration": false}}}),
            json!({"id": 3, "method": "textDocument/documentHighlight", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "result =", "utf-16")}}),
            json!({"id": 4, "method": "textDocument/references", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "{ let", "utf-16"), "context": {"includeDeclaration": true}}}),
        ]
    });
    let starts: Vec<_> = responses[0]["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|location| location["range"]["start"].clone())
        .collect();
    assert_eq!(
        starts,
        [
            json!({"line": 0, "character": 4}),
            json!({"line": 1, "character": 3}),
            json!({"line": 5, "character": 13}),
            json!({"line": 5, "character": 19}),
        ]
    );
    assert_eq!(responses[1]["result"].as_array().unwrap().len(), 2);
    assert_eq!(
        responses[2]["result"],
        json!([{"range": range(1, 41, 47), "kind": 3}, {"range": range(1, 72, 78), "kind": 2}])
    );
    assert!(responses[3]["result"].is_null());
}

#[test]
fn rename_covers_function_names_in_parentheses() {
    let main = "def twice :: i64 -> i64 = \\n -> n * 2\n\ndef apply :: (i64 -> i64) -> i64 = \\f -> f 1\n\nlet a = apply (twice)\nlet b = ( (twice) ) 3\na + b + twice 4\n";
    let mut uri = String::new();
    let responses = scripted(&[("Main.tz", main)], "utf-16", |path| {
        uri = path("Main.tz");
        vec![rename_request(1, &uri, main, "twice ::", "double")]
    });
    let edits = responses[0]["result"]["changes"][uri.as_str()]
        .as_array()
        .unwrap_or_else(|| panic!("{:?}", responses[0]));
    assert_eq!(
        apply_text_edits(main, edits),
        main.replace("twice", "double")
    );
}

fn rename_request(id: u64, uri: &str, text: &str, needle: &str, name: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "method": "textDocument/rename", "params": {"textDocument": {"uri": uri}, "position": position(text, needle, "utf-16"), "newName": name}})
}

#[test]
fn rename_rewrites_fields_cases_and_records() {
    use serde_json::json;
    let mut main_uri = String::new();
    let responses = scripted(&[("Main.tz", P2_MAIN)], "utf-16", |uri| {
        main_uri = uri("Main.tz");
        let main = main_uri.as_str();
        vec![
            rename_request(1, main, P2_MAIN, "x: i64", "y"),
            rename_request(2, main, P2_MAIN, "Circle r", "Round"),
            rename_request(3, main, P2_MAIN, "Point {", "Pt"),
            rename_request(4, main, P2_MAIN, "LIMIT", "MAX"),
            json!({"id": 5, "method": "textDocument/prepareRename", "params": {"textDocument": {"uri": main}, "position": position(P2_MAIN, "Point {", "utf-16")}}),
        ]
    });
    for (response, (name, count)) in
        responses
            .iter()
            .zip([("y", 4), ("Round", 2), ("Pt", 7), ("MAX", 2)])
    {
        let edits = response["result"]["changes"][main_uri.as_str()]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"));
        assert_eq!(edits.len(), count, "{response}");
        assert!(edits.iter().all(|edit| edit["newText"] == name));
    }
    assert_eq!(responses[4]["result"]["placeholder"], "Point");
    assert_eq!(responses[4]["result"]["range"], range(0, 7, 12));
}

/// A24 (review round 1): a rename rewrites the type of a type test `:? T`, and references and
/// go-to-definition see it.
#[test]
fn rename_and_references_reach_type_tests() {
    use serde_json::json;
    let main = "record Point { x: i64 }\n\ndef size :: own (i64 | Point) -> i64 = \\v ->\n    match v with\n    | :? Point as p -> p.x\n    | :? i64 as n -> n\n\nlet annotated: Point = Point { x: 1 }\nsize annotated\n";
    let mut uri = String::new();
    let responses = scripted(&[("Main.tz", main)], "utf-16", |path| {
        uri = path("Main.tz");
        vec![
            rename_request(1, &uri, main, "Point {", "Spot"),
            json!({"id": 2, "method": "textDocument/references", "params": {"textDocument": {"uri": uri}, "position": position(main, "Point {", "utf-16"), "context": {"includeDeclaration": true}}}),
            json!({"id": 3, "method": "textDocument/definition", "params": {"textDocument": {"uri": uri}, "position": position(main, "Point as p", "utf-16")}}),
        ]
    });
    let edits = responses[0]["result"]["changes"][uri.as_str()]
        .as_array()
        .unwrap_or_else(|| panic!("{:?}", responses[0]));
    assert_eq!(apply_text_edits(main, edits), main.replace("Point", "Spot"));
    let references = responses[1]["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{:?}", responses[1]));
    assert_eq!(references.len(), main.matches("Point").count());
    assert!(
        references
            .iter()
            .any(|reference| reference["range"] == range(4, 9, 14)),
        "{references:?}"
    );
    assert_eq!(responses[2]["result"]["range"], range(0, 7, 12));
}

/// A field or case renamed for JSON (`@json "name"`) keeps its attribute; the rename changes only
/// the Tsuzuri name (D08).
#[test]
fn rename_keeps_json_attributes() {
    const SOURCE: &str = "record Point { @json \"px\" x: i64 } deriving (Encode, Decode)\nunion Shape = @json \"round\" Circle of i64 | Empty deriving (Encode)\ndef get :: Point -> i64\nfn get p = p.x\ndef make :: i64 -> Shape\nfn make n = Circle n\n";
    let mut main_uri = String::new();
    let responses = scripted(&[("Main.tz", SOURCE)], "utf-16", |uri| {
        main_uri = uri("Main.tz");
        let main = main_uri.as_str();
        vec![
            rename_request(1, main, SOURCE, "x: i64", "y"),
            rename_request(2, main, SOURCE, "Circle of", "Ball"),
        ]
    });
    for (response, (name, count)) in responses.iter().zip([("y", 2), ("Ball", 2)]) {
        let edits = response["result"]["changes"][main_uri.as_str()]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"));
        assert_eq!(edits.len(), count, "{response}");
        assert!(edits.iter().all(|edit| edit["newText"] == name));
        assert!(
            edits
                .iter()
                .all(|edit| edit["range"]["start"]["character"] != 15),
            "{response}"
        );
    }
}

#[test]
fn rejects_documents_over_the_source_limit() {
    let limit = tsuzuri::syntax::MAX_SOURCE_BYTES;
    // Sent as a request, the notification's error comes back as a response.
    let response = scripted(&[], "utf-16", |uri| {
        vec![serde_json::json!({"id": 1, "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri("Big.tz"), "languageId": "tsuzuri", "version": 1, "text": " ".repeat(limit + 1)}}})]
    })
    .remove(0);
    assert_eq!(response["error"]["code"], -32602, "{response}");
    assert_eq!(
        response["error"]["message"],
        format!("source exceeds the {limit}-byte limit")
    );
}

#[test]
fn rename_rejects_std_export_conflicts_and_errors() {
    let reject = |files: &[(&str, &str)], needle: &str, name: &str| {
        let text = files[0].1;
        let response = scripted(files, "utf-16", |uri| {
            vec![rename_request(1, &uri(files[0].0), text, needle, name)]
        })
        .remove(0);
        let error = &response["error"];
        (
            error["code"]
                .as_i64()
                .unwrap_or_else(|| panic!("{response}")),
            error["message"].as_str().unwrap().to_owned(),
        )
    };
    assert_eq!(
        reject(&[("Main.tz", "export def answer :: i64 = 42\n")], "answer", "reply"),
        (-32803, "cannot rename exported function 'answer' because its export name is part of the ABI; change the export manually".into())
    );
    for name in ["match", "a b", "1x", "_"] {
        assert_eq!(
            reject(&P1, "result =", name),
            (
                -32602,
                format!(
                    "'{name}' is not a valid identifier; use letters, digits and '_' and avoid keywords"
                )
            )
        );
    }
    assert_eq!(
        reject(&P1, "result =", "number"),
        (
            -32803,
            "renaming 'result' to 'number' conflicts with parameter 'number'; choose another name"
                .into()
        )
    );
    assert_eq!(
        reject(&P1, "{ let", "other"),
        (
            -32803,
            "no renamable name at this position; place the cursor on a name".into()
        )
    );
    assert_eq!(
        reject(&[("Main.tz", "fn bad() -> i64 { true }\n")], "bad", "good"),
        (
            -32803,
            "cannot rename while the project has errors; fix the errors first".into()
        )
    );
    assert_eq!(
        reject(
            &[("Main.tz", "fn z() -> f64 { Math.zero() }\n")],
            "zero",
            "one"
        ),
        (
            -32803,
            "cannot rename 'zero' because it is defined in the standard library".into()
        )
    );
    assert_eq!(
        reject(
            &[("Main.tz", "type Id = i64\ndef f :: Id -> i64\nfn f x = x\n")],
            "Id",
            "Key"
        ),
        (
            -32803,
            "renaming type alias names is not supported yet; rename 'Id' manually".into()
        )
    );
}

#[test]
fn rename_is_verified_by_reanalysis() {
    let main = "def helper :: i64 -> i64\nfn helper n = n + 1\ndef f :: i64 -> i64\nfn f value = helper value\n";
    let mut main_uri = String::new();
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        main_uri = uri("Main.tz");
        vec![
            rename_request(1, &main_uri, main, "helper value", "value"),
            rename_request(2, &main_uri, main, "helper value", "assist"),
        ]
    });
    assert_eq!(responses[0]["error"]["code"], -32803);
    let message = responses[0]["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with("rename would change the meaning of the program ("),
        "{message}"
    );
    let edits = responses[1]["result"]["changes"][main_uri.as_str()]
        .as_array()
        .unwrap();
    assert_eq!(edits.len(), 3);
}

#[test]
fn workspace_symbols_filter_by_query_across_files() {
    use serde_json::json;
    let responses = scripted(&P1, "utf-16", |_| {
        vec![
            json!({"id": 1, "method": "workspace/symbol", "params": {"query": "PO"}}),
            json!({"id": 2, "method": "workspace/symbol", "params": {"query": ""}}),
        ]
    });
    let symbols = responses[0]["result"].as_array().unwrap();
    let found: Vec<_> = symbols
        .iter()
        .map(|symbol| (symbol["name"].clone(), symbol["containerName"].clone()))
        .collect();
    assert_eq!(
        found,
        [
            (json!("point"), json!("Main")),
            (json!("Point"), json!("Shapes"))
        ]
    );
    assert_eq!(symbols[1]["kind"], 23);
    assert_eq!(responses[1]["result"].as_array().unwrap().len(), 5);
}

#[test]
fn completion_offers_fields_members_locals_and_keywords() {
    use serde_json::json;
    let member = P1_MAIN.replace("= value.x", "= value.");
    let module = P1_MAIN.replace("= value.x", "= Shapes.");
    let labels = |response: &serde_json::Value| -> Vec<String> {
        response["result"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect()
    };
    let responses = stepwise(&P1, "utf-16", |uri| {
        let main = uri("Main.tz");
        let change = |version: u64, text: &str| json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": version}, "contentChanges": [{"text": text}]}});
        vec![
            json!({"id": 1, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": main}}}),
            change(2, &member),
            json!({"id": 2, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": {"line": 3, "character": 23}}}),
            change(3, &module),
            json!({"id": 3, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": {"line": 3, "character": 24}}}),
            change(4, P1_MAIN),
            json!({"id": 4, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "result }", "utf-16")}}),
            json!({"id": 5, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "\u{65e5}", "utf-16")}}),
        ]
    });
    assert_eq!(
        responses[1]["result"],
        json!({"isIncomplete": false, "items": [{"label": "x", "kind": 5, "detail": "x: i64", "sortText": "2_x"}]})
    );
    assert_eq!(labels(&responses[2]), ["Point"]);
    let names = labels(&responses[3]);
    for expected in [
        "number", "text", "result", "read", "Shapes", "Math", "match",
    ] {
        assert!(
            names.contains(&expected.to_owned()),
            "{expected}: {names:?}"
        );
    }
    assert!(!names.contains(&"value".to_owned()));
    assert_eq!(
        responses[3]["result"]["items"][0]["sortText"],
        json!("0_number")
    );
    assert_eq!(responses[4]["result"]["items"], json!([]));
}

#[test]
fn signature_help_counts_curried_arguments() {
    use serde_json::json;
    let main = "def add :: i64 -> i64 -> i64\nfn add a b = a + b\ndef use_add :: i64 -> i64\nfn use_add n = add n 1\n";
    let responses = scripted(&P1, "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id": 1, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main}, "position": position(P1_MAIN, "n)", "utf-16")}}),
            json!({"id": 2, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main}, "position": {"line": 5, "character": 19}}}),
            json!({"id": 3, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main}, "position": {"line": 0, "character": 2}}}),
        ]
    });
    assert_eq!(
        responses[0]["result"],
        json!({"signatures": [{"label": "read (number: i64) -> i64", "parameters": [{"label": "number: i64"}]}], "activeSignature": 0, "activeParameter": 0})
    );
    assert_eq!(responses[1]["result"]["activeParameter"], 0);
    assert_eq!(
        responses[1]["result"]["signatures"][0]["label"],
        "read (number: i64) -> i64"
    );
    assert!(responses[2]["result"].is_null());
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        let main_uri = uri("Main.tz");
        let at = |character| json!({"id": character, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main_uri}, "position": {"line": 3, "character": character}}});
        vec![at(18), at(20), at(21), at(22)]
    });
    assert_eq!(
        responses[0]["result"]["signatures"][0]["label"],
        "add (a: i64) (b: i64) -> i64"
    );
    let active: Vec<_> = responses
        .iter()
        .map(|response| response["result"]["activeParameter"].clone())
        .collect();
    assert_eq!(active, [json!(0), json!(0), json!(1), json!(1)]);
}

#[test]
fn keyword_member_yield_has_completion_hover_and_signature_help() {
    use serde_json::json;
    // `yield` after a dot is a name: a function `yield` of a module of the program has hover, signature help and
    // completion, and the builtin `Routine.yield`, which has no definition to offer, has hover.
    let steps = "/// Gives way to the other work.\ndef yield :: unit -> i64\nfn yield _unit = 0\n";
    let source =
        "def work :: i64 -> i64\nfn work n =\n    Routine.yield ()\n    Steps.yield () + n\n";
    let responses = scripted(
        &[("Main.tz", source), ("Steps.tz", steps)],
        "utf-16",
        |uri| {
            let main = uri("Main.tz");
            vec![
                json!({"id":1,"method":"textDocument/hover","params":{"textDocument":{"uri":main},"position":position(source,"yield () +", "utf-16")}}),
                json!({"id":2,"method":"textDocument/signatureHelp","params":{"textDocument":{"uri":main},"position":position(source,") +", "utf-16")}}),
                json!({"id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":main},"position":position(source," () +", "utf-16")}}),
                json!({"id":4,"method":"textDocument/hover","params":{"textDocument":{"uri":main},"position":position(source,"yield ()\n", "utf-16")}}),
            ]
        },
    );
    assert_eq!(
        responses[3]["result"]["contents"]["value"],
        json!("```tsuzuri\nunit -> unit\n```"),
        "{responses:?}"
    );
    assert!(
        responses[0]["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("unit -> i64"),
        "{responses:?}"
    );
    let label = responses[1]["result"]["signatures"][0]["label"]
        .as_str()
        .unwrap();
    assert!(
        label.starts_with("yield ") && label.contains("i64"),
        "{label}"
    );
    assert_eq!(responses[1]["result"]["activeParameter"], json!(0));
    assert!(
        responses[2]["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "yield")
    );
}

#[test]
fn signature_help_after_a_line_that_ends_with_a_closer() {
    use serde_json::json;
    // The brackets that end the line before a call are not its arguments, whichever bracket closes that line,
    // and a CRLF document reads the same. A line that has no call of its own before the cursor, inside a `name(`
    // that an earlier line opened, continues that call.
    let steps = "/// Gives way.\ndef yield :: unit -> i64\nfn yield _unit = 0\n";
    let expected = json!({"signatures": [{"label": "yield (_unit: unit) -> i64", "parameters": [{"label": "_unit: unit"}]}], "activeSignature": 0, "activeParameter": 0});
    let body = |previous: &str| {
        format!("def work :: i64 -> i64\nfn work n =\n    {previous}\n    Steps.yield () + n\n")
    };
    let mut cases: Vec<(String, &str)> = [
        "Routine.yield ()",
        "let xs = [1, 2]",
        "let s = $\"v{n}\"",
        "let t = (n, n)",
        "let ys = [|1, 2|]",
        "let m = 1",
    ]
    .into_iter()
    .map(|previous| (body(previous), ") + n"))
    .collect();
    cases.push((
        body("let r = Pair { a: n, b: n }") + "record Pair { a: i64, b: i64 }\n",
        ") + n",
    ));
    cases.push((body("Routine.yield ()").replace('\n', "\r\n"), ") + n"));
    cases.push((
        "def work :: i64 -> i64\nfn work n =\n    Routine.yield ()\n    Steps.yield(\n        ()) + n\n"
            .to_owned(),
        "()) + n",
    ));
    cases.push((
        "def work :: i64 -> i64\nfn work n =\n    Routine.yield ()\n    Steps.yield(\n        ()\n    ) + n\n"
            .to_owned(),
        ") + n",
    ));
    let wrong: Vec<_> = cases
        .iter()
        .filter_map(|(source, needle)| {
            let responses = scripted(
                &[("Main.tz", source), ("Steps.tz", steps)],
                "utf-16",
                |uri| {
                    vec![json!({"id": 1, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": position(source, needle, "utf-16")}})]
                },
            );
            let result = &responses[0]["result"];
            (*result != expected).then(|| format!("{source:?}: {result}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn signature_help_inside_brackets_that_an_earlier_line_opened() {
    use serde_json::json;
    // Each text is an edit in progress after a good analysis, at the end of its last line. Below a line that
    // ends with a closer, a call of the cursor's line is the innermost one, whether an earlier line opened a
    // tuple, a group after a space, a `name(` call, or a `name(` call that a comma continues. A line whose own
    // head names no function, or that starts with a group, stays inside the `name(` call. A stray `)` does not
    // make the first name of the document the call.
    let steps = "def yield :: unit -> i64\nfn yield _unit = 0\ndef add :: i64 -> i64 -> i64\nfn add a b = a + b\ndef one :: i64 -> i64\nfn one a = a\n";
    let primer = "def work :: i64 -> i64\nfn work n =\n    n\n";
    let one = json!({"signatures": [{"label": "one (a: i64) -> i64", "parameters": [{"label": "a: i64"}]}], "activeSignature": 0, "activeParameter": 0});
    let add = json!({"signatures": [{"label": "add (a: i64) (b: i64) -> i64", "parameters": [{"label": "a: i64"}, {"label": "b: i64"}]}], "activeSignature": 0, "activeParameter": 0});
    let body = |lines: &[&str]| {
        let lines: Vec<_> = lines.iter().map(|line| format!("    {line}\n")).collect();
        format!("def work :: i64 -> i64\nfn work n =\n{}", lines.concat())
    };
    let cases = [
        (body(&["let t = (", "    (n)", "    Steps.one ("]), &one),
        (body(&["Steps.add (", "    (n)", "    Steps.one ("]), &one),
        (body(&["Steps.add(", "    (n)", "    Steps.one ("]), &one),
        (body(&["Steps.add(", "    ()", "    Steps.one n"]), &one),
        (body(&["Steps.add(n,", "    (n)", "    Steps.one ("]), &one),
        (body(&["let t = ((n),", "    (n)", "    Steps.one ("]), &one),
        (body(&["Steps.add(", "    ()", "    n"]), &add),
        (body(&["Steps.add(", "    ()", "    (n) "]), &add),
        ("Steps.add n)\n    Steps.one (\n".to_owned(), &one),
    ];
    let responses = stepwise(
        &[("Main.tz", primer), ("Steps.tz", steps)],
        "utf-16",
        |uri| {
            let main = uri("Main.tz");
            let mut steps = vec![
                json!({"id": 0, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main}, "position": {"line": 2, "character": 5}}}),
            ];
            for (version, (text, _)) in cases.iter().enumerate() {
                let last = text.trim_end();
                let line = last.matches('\n').count();
                let character = last.len() - last.rfind('\n').map_or(0, |at| at + 1);
                steps.push(json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": version + 2}, "contentChanges": [{"text": text}]}}));
                steps.push(json!({"id": version + 1, "method": "textDocument/signatureHelp", "params": {"textDocument": {"uri": main}, "position": {"line": line, "character": character}}}));
            }
            steps
        },
    );
    let wrong: Vec<_> = cases
        .iter()
        .zip(&responses[1..])
        .filter(|((_, expected), response)| response["result"] != **expected)
        .map(|((text, _), response)| format!("{text:?}: {}", response["result"]))
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn interpolated_holes_are_indexed_and_their_text_offers_no_completions() {
    use serde_json::json;
    let main = "def read :: i64 -> i64\nfn read number = number\ndef greet :: i64 -> string\nfn greet n = $\"n={read n:>4} done {{ok}}\"\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let read = definition(&index, 0, "read", SymbolKind::Function);
    assert!(starts(&index, read).contains(&(0, at(main, "read n:"))));
    let n = index
        .definitions
        .iter()
        .position(|item| item.name == "n" && item.kind == SymbolKind::Parameter)
        .unwrap();
    assert!(starts(&index, n).contains(&(0, at(main, "n:>4"))));
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        let main_uri = uri("Main.tz");
        let complete = |id, needle: &str| json!({"id": id, "method": "textDocument/completion", "params": {"textDocument": {"uri": main_uri}, "position": position(main, needle, "utf-16")}});
        vec![
            json!({"id": 0, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": main_uri}}}),
            complete(1, "={read"),
            complete(2, "4} done"),
            complete(3, "{ok}}"),
            complete(4, "read n:"),
        ]
    });
    for quiet in &responses[1..4] {
        assert_eq!(quiet["result"]["items"], json!([]), "{quiet}");
    }
    let labels: Vec<_> = responses[4]["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", responses[4]))
        .iter()
        .map(|item| item["label"].as_str().unwrap().to_owned())
        .collect();
    assert!(labels.contains(&"read".to_owned()), "{labels:?}");
}

#[test]
fn stale_fallbacks_hide_private_declarations_of_other_modules() {
    use serde_json::json;
    let main = "def run :: i64 -> i64\nfn run n = n\n";
    let secret = "private union Hidden = Gone | Here\nunion Shown = Up | Down\nprivate def helper :: i64 -> i64\nfn helper n = n + 1\ndef open :: i64 -> i64\nfn open n = helper n\n";
    let stale = [
        "Secret.Hidden.",
        "Secret.Shown.",
        "Secret.helper ",
        "Secret.open ",
    ]
    .map(|tail| main.replace("= n\n", &format!("= {tail}\n")));
    let responses = stepwise(
        &[("Main.tz", main), ("Secret.tz", secret)],
        "utf-16",
        |uri| {
            let main = uri("Main.tz");
            let mut steps = vec![
                json!({"id": 0, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": main}}}),
            ];
            for (version, (text, method)) in stale
                .iter()
                .zip(["completion", "completion", "signatureHelp", "signatureHelp"])
                .enumerate()
            {
                let line = text.lines().nth(1).unwrap();
                steps.push(json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": version + 2}, "contentChanges": [{"text": text}]}}));
                steps.push(json!({"id": version + 1, "method": format!("textDocument/{method}"), "params": {"textDocument": {"uri": main}, "position": {"line": 1, "character": line.len()}}}));
            }
            steps
        },
    );
    let labels = |response: &serde_json::Value| -> Vec<String> {
        response["result"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .map(|item| item["label"].as_str().unwrap().to_owned())
            .collect()
    };
    assert!(labels(&responses[1]).is_empty(), "{}", responses[1]);
    assert_eq!(labels(&responses[2]), ["Down", "Up"]);
    assert!(responses[3]["result"].is_null(), "{}", responses[3]);
    assert_eq!(
        responses[4]["result"]["signatures"][0]["label"],
        "open (n: i64) -> i64"
    );
}

/// Decodes relative semantic tokens into absolute `[line, start, length, type, modifiers]`.
fn decode(data: &serde_json::Value) -> Vec<[u64; 5]> {
    let data: Vec<_> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().unwrap())
        .collect();
    let (mut line, mut start, mut tokens) = (0, 0, Vec::new());
    for chunk in data.chunks(5) {
        line += chunk[0];
        start = if chunk[0] == 0 {
            start + chunk[1]
        } else {
            chunk[1]
        };
        tokens.push([line, start, chunk[2], chunk[3], chunk[4]]);
    }
    tokens
}

#[test]
fn semantic_tokens_encode_declarations_and_references() {
    use serde_json::json;
    for encoding in ["utf-16", "utf-8"] {
        let responses = scripted(&P1, encoding, |uri| {
            vec![
                json!({"id": 1, "method": "textDocument/semanticTokens/full", "params": {"textDocument": {"uri": uri("Shapes.tz")}}}),
                json!({"id": 2, "method": "textDocument/semanticTokens/full", "params": {"textDocument": {"uri": uri("Main.tz")}}}),
            ]
        });
        assert_eq!(
            responses[0]["result"],
            json!({"data": [0, 7, 5, 1, 1, 0, 8, 1, 4, 1]})
        );
        let shift = if encoding == "utf-8" { 4 } else { 0 };
        let line: Vec<_> = decode(&responses[1]["result"]["data"])
            .into_iter()
            .filter(|token| token[0] == 1)
            .collect();
        assert_eq!(
            line,
            [
                [1, 3, 4, 8, 1],
                [1, 8, 6, 10, 1],
                [1, 23, 4, 9, 1],
                [1, 41 + shift, 6, 9, 1],
                [1, 50 + shift, 6, 10, 0],
                [1, 59 + shift, 4, 9, 0],
                [1, 72 + shift, 6, 9, 0],
            ]
        );
        let qualified: Vec<_> = decode(&responses[1]["result"]["data"])
            .into_iter()
            .filter(|token| token[0] == 2)
            .collect();
        assert_eq!(
            qualified,
            [[2, 4, 5, 8, 1], [2, 13, 6, 0, 0], [2, 20, 5, 1, 0]]
        );
    }
}

/// `text` with the LSP text edits applied (ASCII text: a character is a byte).
fn apply_text_edits(text: &str, edits: &[serde_json::Value]) -> String {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let offset = |position: &serde_json::Value| {
        starts[position["line"].as_u64().unwrap() as usize]
            + position["character"].as_u64().unwrap() as usize
    };
    let mut edits: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|edit| {
            (
                offset(&edit["range"]["start"]),
                offset(&edit["range"]["end"]),
                edit["newText"].as_str().unwrap(),
            )
        })
        .collect();
    edits.sort_unstable();
    let mut result = text.to_owned();
    for (start, end, new_text) in edits.into_iter().rev() {
        result.replace_range(start..end, new_text);
    }
    result
}

#[test]
fn code_action_fix_all_prefixes_every_unused_local_of_the_document() {
    use serde_json::json;
    let warn = "fn warn() -> i64 { let spare = 1; let other = 3; 2 }\n";
    let mut uri = String::new();
    let responses = scripted(
        &[
            ("Main.tz", P1_MAIN),
            ("Shapes.tz", P1_SHAPES),
            ("Warn.tz", warn),
        ],
        "utf-16",
        |path| {
            uri = path("Warn.tz");
            vec![
                json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri}, "range": range(0, 0, 0), "context": {"diagnostics": [], "only": ["source.fixAll"]}}}),
            ]
        },
    );
    let actions = responses[0]["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0]["kind"], "source.fixAll.tsuzuri");
    assert_eq!(actions[0]["title"], "Fix all auto-fixable problems");
    assert!(actions[0].get("isPreferred").is_none());
    let edits = actions[0]["edit"]["changes"][uri.as_str()]
        .as_array()
        .unwrap();
    assert_eq!(
        apply_text_edits(warn, edits),
        "fn warn() -> i64 { let _spare = 1; let _other = 3; 2 }\n"
    );
}

#[test]
fn code_action_fix_all_is_offered_only_when_asked_and_when_there_is_something_to_fix() {
    use serde_json::json;
    let responses = scripted(&P1, "utf-16", |uri| {
        let warn = uri("Warn.tz");
        let shapes = uri("Shapes.tz");
        let request = |id: u64, uri: &str, end: usize, context: serde_json::Value| json!({"id": id, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri}, "range": range(0, 0, end), "context": context}});
        vec![
            request(
                1,
                &warn,
                37,
                json!({"diagnostics": [], "only": ["quickfix"]}),
            ),
            request(
                2,
                &warn,
                37,
                json!({"diagnostics": [], "only": ["refactor"]}),
            ),
            request(3, &shapes, 10, json!({"diagnostics": []})),
            request(4, &warn, 37, json!({"diagnostics": []})),
        ]
    });
    let kinds = |index: usize| -> Vec<String> {
        responses[index]["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{:?}", responses[index]))
            .iter()
            .map(|action| action["kind"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(kinds(0), ["quickfix"]);
    assert!(kinds(1).is_empty());
    assert!(kinds(2).is_empty());
    assert_eq!(kinds(3), ["quickfix", "source.fixAll.tsuzuri"]);
}

#[test]
fn code_action_prefixes_unused_locals() {
    use serde_json::json;
    let mut warn = String::new();
    let responses = scripted(&P1, "utf-16", |uri| {
        warn = uri("Warn.tz");
        let request = |id: u64, only: &str| json!({"id": id, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": warn}, "range": range(0, 0, 37), "context": {"diagnostics": [], "only": [only]}}});
        vec![request(1, "quickfix"), request(2, "refactor")]
    });
    let actions = responses[0]["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0]["title"], "Prefix 'spare' with '_'");
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(actions[0]["isPreferred"], true);
    assert_eq!(
        actions[0]["diagnostics"],
        json!([{"range": range(0, 23, 28), "severity": 2, "code": "W1001", "source": "tsuzuri", "message": "unused local 'spare'; prefix it with '_' to silence this warning", "data": {"kind": "unused_local", "fixes": [{"title": "Prefix 'spare' with '_'", "applicability": "certain"}]}}])
    );
    assert_eq!(
        actions[0]["edit"],
        json!({"changes": {warn.as_str(): [{"range": range(0, 23, 28), "newText": "_spare"}]}})
    );
    assert_eq!(responses[1]["result"], json!([]));
}

#[test]
fn formatting_matches_the_library_formatter() {
    use serde_json::json;
    let text = "fn warn() -> i64 {  let _spare = 1;  2 }";
    let expected =
        tsuzuri::formatter::format_source("Warn.tz", text, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    assert!(expected.changed);
    let responses = scripted(&[("Warn.tz", text), ("Bad.tz", "fn (")], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/formatting", "params": {"textDocument": {"uri": uri("Warn.tz")}, "options": {"tabSize": 8, "insertSpaces": true}}}),
            json!({"id": 2, "method": "textDocument/formatting", "params": {"textDocument": {"uri": uri("Bad.tz")}, "options": {"tabSize": 4, "insertSpaces": true}}}),
        ]
    });
    assert_eq!(
        responses[0]["result"],
        json!([{"range": range(0, 0, text.len()), "newText": expected.formatted}])
    );
    assert!(responses[1]["result"].is_null());
}

#[test]
fn inlay_hints_show_implicit_copies() {
    use serde_json::json;
    let text = "record Bag { items: [i64] }\ndef total :: own Bag -> i64\nfn total bag =\n    let xs = bag.items\n    let ys = bag.items\n    Array.length (ref xs) + Array.length (ref ys)\ndef reuse :: own [i64] -> i64\nfn reuse a =\n    let b = a\n    let moved = [1]\n    let n = 2\n    let m = n\n    Array.length (ref a) + Array.length (ref b) + Array.length (ref moved) + m\n";
    let broken = format!("{text}fn (");
    let responses = stepwise(&[("Main.tz", text)], "utf-16", |uri| {
        let request = |id: u64, range: serde_json::Value| json!({"id": id, "method": "textDocument/inlayHint", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range}});
        let whole =
            json!({"start": {"line": 0, "character": 0}, "end": {"line": 13, "character": 0}});
        vec![
            request(1, whole.clone()),
            request(2, range(8, 0, 13)),
            json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": uri("Main.tz"), "version": 2}, "contentChanges": [{"text": broken}]}}),
            request(3, whole),
        ]
    });
    let array = "implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit";
    let hint = |line: usize, character: usize, kind: &str| json!({"position": {"line": line, "character": character}, "label": format!("copy ({kind})"), "tooltip": array, "paddingLeft": true});
    let all = json!([
        hint(3, 22, "field"),
        hint(4, 22, "field"),
        hint(8, 13, "local")
    ]);
    assert_eq!(responses[0]["result"], all);
    assert_eq!(responses[1]["result"], json!([hint(8, 13, "local")]));
    assert_eq!(responses[2]["result"], all);
}

#[test]
fn capabilities_are_advertised_exactly() {
    use serde_json::json;
    use std::io::Cursor;
    use tsuzuri::lsp::{read_message, serve, write_message};
    let mut input = Vec::new();
    write_message(
        &mut input,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    )
    .unwrap();
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, Vec::new());
    let response = read_message(&mut Cursor::new(output))
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        response["result"]["capabilities"],
        json!({
            "positionEncoding": "utf-16",
            "textDocumentSync": {"openClose": true, "change": 1},
            "hoverProvider": true,
            "definitionProvider": true,
            "documentSymbolProvider": true,
            "referencesProvider": true,
            "documentHighlightProvider": true,
            "renameProvider": {"prepareProvider": true},
            "workspaceSymbolProvider": true,
            "completionProvider": {"triggerCharacters": [".", ":"], "resolveProvider": false},
            "signatureHelpProvider": {"triggerCharacters": [" ", "("], "retriggerCharacters": [","]},
            "semanticTokensProvider": {
                "legend": {
                    "tokenTypes": ["namespace", "struct", "enum", "enumMember", "property", "type", "interface", "method", "function", "variable", "parameter"],
                    "tokenModifiers": ["declaration", "readonly", "defaultLibrary"],
                },
                "full": true,
                "range": false,
            },
            "codeActionProvider": {"codeActionKinds": ["quickfix", "source.fixAll.tsuzuri"]},
            "documentFormattingProvider": true,
            "inlayHintProvider": true,
        })
    );
}

#[test]
fn index_links_locals_and_parameters() {
    let (_, index) =
        analyze_modules_with_semantics(&[("Main.tz", P1_MAIN), ("Shapes.tz", P1_SHAPES)]).unwrap();
    let number = definition(&index, 0, "number", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, number),
        [(0, at(P1_MAIN, "number =")), (0, at(P1_MAIN, "number +"))]
    );
    let result = definition(&index, 0, "result", SymbolKind::Local);
    assert_eq!(
        starts(&index, result),
        [(0, at(P1_MAIN, "result =")), (0, at(P1_MAIN, "result }"))]
    );
    assert_eq!(index.occurrences(result)[0].role, Role::Declaration);
    let text = definition(&index, 0, "text", SymbolKind::Local);
    let scope = index
        .scopes
        .iter()
        .find(|scope| scope.definition == text)
        .unwrap();
    assert_eq!(scope.visible.start, at(P1_MAIN, "; let result"));
    assert_eq!(
        scope.visible.end,
        at(P1_MAIN, "result }") + "result }".len()
    );
}

/// Review 5 of #64: the result name of each `@ensures` line is a definition of its own, with the
/// written name and span: hover shows it, its definition is the binder, and its references are
/// the reads in that line. No generated name of the woven body shows.
#[test]
fn index_links_the_result_names_of_ensures_lines() {
    let main = "def grow :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > x\n    @ensures \\res -> res == @old (x) + 1\n    x + 1\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let r = definition(&index, 0, "r", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, r),
        [(0, at(main, "r -> r")), (0, at(main, "r > x"))]
    );
    let res = definition(&index, 0, "res", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, res),
        [(0, at(main, "res ->")), (0, at(main, "res =="))]
    );
    let hover = index.at(0, at(main, "r > x")).unwrap();
    assert_eq!(hover.detail, "r: i64");
    assert_eq!(
        hover.target.map(|span| span.start),
        Some(at(main, "r -> r"))
    );
    assert!(
        index
            .entries
            .iter()
            .all(|entry| !entry.detail.contains('$')),
        "{:?}",
        index
            .entries
            .iter()
            .map(|entry| &entry.detail)
            .collect::<Vec<_>>()
    );
}

/// Review 6 of #64: a parenthesized name in a contract line is a reference spelled by the name
/// alone, as in the body: a function named in `@requires` and the result named in `@ensures`.
#[test]
fn index_links_parenthesized_names_in_contract_lines() {
    let main = "def positive :: i64 -> bool = \\x -> x > 0\n\
                def grow :: i64 -> i64 = \\x ->\n    @requires (positive) x\n    @ensures \\r -> (r) > (x)\n    if (positive) x then x + 1 else 1\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let positive = definition(&index, 0, "positive", SymbolKind::Function);
    assert_eq!(
        starts(&index, positive),
        [
            (0, at(main, "positive ::")),
            (0, at(main, "positive) x\n")),
            (0, at(main, "positive) x then")),
        ]
    );
    let r = definition(&index, 0, "r", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, r),
        [(0, at(main, "r -> (r)")), (0, at(main, "r) >"))]
    );
    for occurrence in index.occurrences(r) {
        assert_eq!(&main[occurrence.span.start..occurrence.span.end], "r");
    }
    // A parenthesized local, in a contract line and in the body.
    let grow = at(main, "def grow");
    let x = index
        .definitions
        .iter()
        .position(|item| item.name == "x" && item.span.start > grow)
        .unwrap();
    assert_eq!(
        starts(&index, x),
        [
            (0, at(main, "x ->\n    @requires")),
            (0, at(main, "x\n    @ensures")),
            (0, at(main, "x)\n")),
            (0, at(main, "x then")),
            (0, at(main, "x + 1")),
        ]
    );
}

#[test]
fn index_links_records_fields_cases_and_constants() {
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", P2_MAIN)]).unwrap();
    let count = |name, kind| index.occurrences(definition(&index, 0, name, kind)).len();
    assert_eq!(count("x", SymbolKind::Field), 4);
    assert_eq!(count("Point", SymbolKind::Record), 7);
    assert_eq!(count("Circle", SymbolKind::Case), 2);
    assert_eq!(count("LIMIT", SymbolKind::Const), 2);
    assert_eq!(count("Shape", SymbolKind::Union), 2);
    let x = definition(&index, 0, "x", SymbolKind::Field);
    assert_eq!(index.definitions[x].detail, "x: i64");
    for occurrence in index.occurrences(x) {
        assert_eq!(&P2_MAIN[occurrence.span.start..occurrence.span.end], "x");
    }
}

#[test]
fn index_records_receivers_for_field_completion() {
    let (_, index) =
        analyze_modules_with_semantics(&[("Main.tz", P1_MAIN), ("Shapes.tz", P1_SHAPES)]).unwrap();
    let point = definition(&index, 1, "Point", SymbolKind::Record);
    let receiver = at(P1_MAIN, "value.x");
    assert!(index.receivers.iter().any(|(span, record)| {
        span.source == Some(0)
            && span.start == receiver
            && span.end == receiver + "value".len()
            && *record == point
    }));
}

#[test]
fn index_follows_lambda_captures() {
    let main = "def adder :: i64 -> i64 -> i64\nfn adder n = \\x -> x + n\n";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let n = definition(&index, 0, "n", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, n),
        [(0, at(main, "n =")), (0, main.len() - 2)]
    );
    let x = definition(&index, 0, "x", SymbolKind::Parameter);
    assert_eq!(
        starts(&index, x),
        [(0, at(main, "x ->")), (0, at(main, "x +"))]
    );
}

const LET_REC: &str = "def f :: i64 -> bool = \\n ->\n    let rec even = \\m -> if m == 0 then true else odd (m - 1)\n    and odd = \\m -> if m == 0 then false else even (m - 1)\n    even n\n";

/// B09: a `let rec` name is a local that its own function and the other functions of its group
/// refer to, from the end of the group's first name.
#[test]
fn index_links_local_let_rec_functions() {
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", LET_REC)]).unwrap();
    let even = definition(&index, 0, "even", SymbolKind::Local);
    assert_eq!(
        starts(&index, even),
        [
            (0, at(LET_REC, "even =")),
            (0, at(LET_REC, "even (m")),
            (0, at(LET_REC, "even n"))
        ]
    );
    let odd = definition(&index, 0, "odd", SymbolKind::Local);
    // The call in the body of `even` comes before the declaration of `odd`, which is the later
    // name of the group.
    assert_eq!(
        starts(&index, odd),
        [(0, at(LET_REC, "odd (m")), (0, at(LET_REC, "odd ="))]
    );
    assert_eq!(index.occurrences(even)[0].role, Role::Declaration);
    for definition in [even, odd] {
        let scope = index
            .scopes
            .iter()
            .find(|scope| scope.definition == definition)
            .unwrap();
        assert_eq!(scope.visible.start, at(LET_REC, "even =") + "even".len());
        assert_eq!(scope.visible.end, LET_REC.len() - 1);
    }
}

/// B09: renaming a `let rec` function rewrites its own calls, the calls of its group, and the uses
/// after it, and nothing else.
#[test]
fn rename_covers_every_use_of_a_local_let_rec_function() {
    let mut main_uri = String::new();
    let responses = scripted(&[("Main.tz", LET_REC)], "utf-16", |uri| {
        main_uri = uri("Main.tz");
        let main = main_uri.as_str();
        vec![
            rename_request(1, main, LET_REC, "even =", "evens"),
            rename_request(2, main, LET_REC, "odd (m", "odds"),
            serde_json::json!({"id": 3, "method": "textDocument/references", "params": {"textDocument": {"uri": main}, "position": position(LET_REC, "even n", "utf-16"), "context": {"includeDeclaration": false}}}),
        ]
    });
    for (response, (name, count)) in responses.iter().zip([("evens", 3), ("odds", 2)]) {
        let edits = response["result"]["changes"][main_uri.as_str()]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"));
        assert_eq!(edits.len(), count, "{response}");
        assert!(edits.iter().all(|edit| edit["newText"] == name));
    }
    assert_eq!(responses[2]["result"].as_array().unwrap().len(), 2);
}

#[cfg(windows)]
#[test]
fn windows_file_uris_round_trip_drive_letters_and_canonical_paths() {
    use std::path::{Path, PathBuf};
    use tsuzuri::lsp::{file_uri, uri_path};
    let uri = "file:///C:/space%20%23/Main.tz";
    assert_eq!(
        file_uri(Path::new(r"C:\space #\Main.tz")).as_deref(),
        Some(uri)
    );
    assert_eq!(
        file_uri(Path::new(r"\\?\C:\space #\Main.tz")).as_deref(),
        Some(uri)
    );
    assert_eq!(uri_path(uri).unwrap(), PathBuf::from(r"C:/space #/Main.tz"));
}

#[test]
fn semantic_docs_follow_function_and_type_definition_targets() {
    let main = "/// Keeps the value.\ndef identity :: 'a -> 'a\nfn identity value = value\ndef read :: i64 -> i64\nfn read value = identity value\ndef point :: Shapes.Point -> i64\nfn point value = value.x";
    let shapes = "/// Coordinates.\nrecord Point { x: i64 }";
    let (_, index) =
        analyze_modules_with_semantics(&[("Main.tz", main), ("Shapes.tz", shapes)]).unwrap();
    let function = index.at(0, main.find("identity value").unwrap()).unwrap();
    assert_eq!(
        index.doc_for(function.target.unwrap()),
        Some("Keeps the value.")
    );
    let record = index.at(0, main.find("Shapes.Point").unwrap()).unwrap();
    assert_eq!(index.doc_for(record.target.unwrap()), Some("Coordinates."));
}

#[test]
fn semantic_docs_hide_the_setup_lines_of_examples() {
    let main = "/// Adds.\n///\n/// ```tsuzuri run=11\n/// # let base = 10\n/// Main.add base 1\n/// ```\ndef add :: i64 -> i64 -> i64\nfn add a b = a + b\ndef use_it :: i64 -> i64\nfn use_it value = add value 1";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let function = index.at(0, main.find("add value").unwrap()).unwrap();
    let doc = index.doc_for(function.target.unwrap()).unwrap();
    assert!(!doc.contains("# let base"), "{doc}");
    assert!(doc.contains("Output:\n\n```text\n11\n```"), "{doc}");
}

#[test]
fn an_index_shows_its_own_type_not_the_i64_it_converts_to() {
    let main = "def read :: ref [i64] -> i32 -> i64\nfn read values n = values[n - 1]\n\
        def wide :: ref [i64] -> i128u -> i64\nfn wide values n = values[n * 2]\n\
        def generic :: Integer<'n> => ref [i64] -> 'n -> i64\nfn generic values n = values[n + n]";
    let (_, index) = analyze_modules_with_semantics(&[("Main.tz", main)]).unwrap();
    let narrow = index.at(0, main.find("- 1").unwrap()).unwrap();
    assert_eq!(narrow.detail, "i32");
    let wide = index.at(0, main.find("* 2").unwrap()).unwrap();
    assert_eq!(wide.detail, "i128u");
    let generic = index.at(0, main.find("+ n]").unwrap()).unwrap();
    assert_eq!(generic.detail, "'n");
    let element = index.at(0, main.find("values[n - 1]").unwrap()).unwrap();
    assert!(element.detail.contains("values"), "{}", element.detail);
}

#[test]
fn semantic_index_preserves_source_types_and_definitions() {
    let main = "def identity :: 'a -> 'a\nfn identity value = value\n\
        def use_value :: i64 -> i64\nfn use_value number = identity number\n\
        def point :: Shapes.Point -> i64\nfn point value = value.x";
    let shapes = "record Point { x: i64 }";
    let sources = [("Main.tz", main), ("Shapes.tz", shapes)];
    let (module, index) = analyze_modules_with_semantics(&sources).unwrap();
    let local_use = main.find("= value").unwrap() + 2;
    let entry = index.at(0, local_use).unwrap();
    assert!(entry.detail.contains("'a"), "{}", entry.detail);
    assert_eq!(entry.target.unwrap().start, main.find("value =").unwrap());
    let reference = index.at(0, main.find("identity number").unwrap()).unwrap();
    assert_eq!(
        reference.target.unwrap().start,
        main.find("fn identity").unwrap() + 3
    );
    let record = index.at(0, main.find("Shapes.Point").unwrap()).unwrap();
    assert_eq!(record.target.unwrap().source, Some(1));
    assert_eq!(record.target.unwrap().start, shapes.find("Point").unwrap());
    let symbols: Vec<_> = index
        .symbols
        .iter()
        .filter(|symbol| symbol.selection.source == Some(0))
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(symbols, ["identity", "use_value", "point"]);
    assert!(
        !index
            .entries
            .iter()
            .any(|entry| entry.detail.contains("$mono") || entry.detail.contains("$lambda"))
    );
    let ordinary = analyze_modules(&sources).unwrap();
    assert_eq!(
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        llvm::emit(&ordinary, llvm::Entry::Library).unwrap()
    );
}

#[test]
fn invalid_semantic_snapshots_are_not_returned() {
    let errors = analyze_modules_with_semantics(&[(
        "Main.tz",
        "fn bad() -> i64 { true }\nfn other() -> bool { 1 }",
    )])
    .unwrap_err();
    assert_eq!(errors.diagnostics.len(), 2);
    assert!(errors.diagnostics.iter().all(|error| error.code == "E1003"));
}

#[test]
fn overlays_replace_disk_and_include_unsaved_files() {
    use std::{collections::BTreeMap, fs};
    use tsuzuri::driver::Project;
    let directory = std::env::temp_dir().join(format!(
        "tsuzuri-lsp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let main = directory.join("Main.tz");
    let extra = directory.join("Extra.tz");
    fs::write(&main, "fn value() -> i64 { true }").unwrap();
    let overlays = BTreeMap::from([
        (main.clone(), "Extra.value()".to_owned()),
        (extra, "fn value() -> i64 { 42 }".to_owned()),
    ]);
    let project = Project::load_with_overlays(&main, &overlays).unwrap();
    project.analyze().unwrap();
    assert!(project.sources.iter().any(|source| source.name == "Extra"));
    let closed = Project::load_with_overlays(&main, &BTreeMap::new()).unwrap();
    assert!(!closed.sources.iter().any(|source| source.name == "Extra"));
    assert_eq!(closed.analyze().unwrap_err().code, "E1003");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn server_lifecycle_and_protocol_errors_are_framed() {
    use serde_json::json;
    use std::io::Cursor;
    use tsuzuri::lsp::{read_message, serve, write_message};
    let mut input = b"Content-Length: 1\r\n\r\n{".to_vec();
    for message in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {"general": {"positionEncodings": ["utf-8"]}}}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "unknown"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"}),
        json!({"jsonrpc": "2.0", "method": "exit"}),
    ] {
        write_message(&mut input, &message).unwrap();
    }
    let mut output = Vec::new();
    assert_eq!(serve(Cursor::new(input), &mut output, Vec::new()), 0);
    let mut output = Cursor::new(output);
    assert_eq!(
        read_message(&mut output).unwrap().unwrap().unwrap()["error"]["code"],
        -32700
    );
    assert_eq!(
        read_message(&mut output).unwrap().unwrap().unwrap()["result"]["capabilities"]["positionEncoding"],
        "utf-8"
    );
    assert_eq!(
        read_message(&mut output).unwrap().unwrap().unwrap()["error"]["code"],
        -32601
    );
    assert!(read_message(&mut output).unwrap().unwrap().unwrap()["result"].is_null());
}

const N_CIRCLE: &str = "namespace Demo::Shapes\n\ndef radius :: i64 -> i64 = \\x -> x\n";
const N_REPORT: &str = "namespace Demo\n\nusing Demo::Shapes\n\ndef total :: i64 -> i64 = \\x -> Circle.radius x + Shapes::Circle.radius x\n";

#[test]
fn own_and_inferred_read_signatures_keep_the_source_convention() {
    use serde_json::json;
    let source = "def take :: own string -> string = \\text -> text\n\
        def size :: string -> i64 = \\text -> text.length\n\
        def value :: i64 = { let text = take \"abc\"; size text }\n";
    let manifest = "[package]\nname = \"conventions\"\nversion = \"0.1.0\"\nedition = \"2027\"\n";
    let responses = scripted(
        &[("Main.tz", source), ("Tsuzuri.toml", manifest)],
        "utf-16",
        |uri| {
            [
                ("textDocument/hover", "take ::"),
                ("textDocument/hover", "size ::"),
                ("textDocument/signatureHelp", "\"abc\""),
                ("textDocument/signatureHelp", "text }\n"),
            ]
            .iter()
            .enumerate()
            .map(|(id, (method, needle))| {
                json!({"id": id + 1, "method": method, "params": {"textDocument": {"uri": uri("Main.tz")}, "position": position(source, needle, "utf-16")}})
            })
            .chain(std::iter::once(json!({"id": 5, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": uri("Main.tz")}}})))
            .collect()
        },
    );
    for (response, signature) in responses
        .iter()
        .zip(["take :: own string -> string", "size :: string -> i64"])
    {
        let hover = response["result"]["contents"]["value"].as_str().unwrap();
        assert!(hover.contains(signature), "{hover}");
    }
    for (response, parameter) in responses[2..4]
        .iter()
        .zip(["text: own string", "text: string"])
    {
        let label = &response["result"]["signatures"][0]["parameters"][0]["label"];
        assert_eq!(label, parameter, "{response}");
    }
    let symbols = responses[4]["result"].as_array().unwrap();
    assert!(
        symbols[0]["detail"]
            .as_str()
            .unwrap()
            .contains("own string")
    );
    assert!(
        !symbols[1]["detail"]
            .as_str()
            .unwrap()
            .contains("ref string")
    );
}

#[test]
fn const_def_hovers_show_the_qualifier() {
    use serde_json::json;
    let main = "const def twice :: i64 -> i64\nfn twice x = x * 2\ndef plain :: i64 -> i64\nfn plain x = x\nconst Answer: i64 = twice 21\nAnswer\n";
    let hovers = [
        ("twice x", "const def Main.twice :: i64 -> i64"),
        ("plain x", "def Main.plain :: i64 -> i64"),
    ];
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        hovers
            .iter()
            .enumerate()
            .map(|(id, (needle, _))| {
                json!({"id": id + 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": position(main, needle, "utf-16")}})
            })
            .collect()
    });
    for (response, (needle, detail)) in responses.iter().zip(&hovers) {
        let hover = response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{needle}: {response}"));
        assert!(
            hover.starts_with(&format!("```tsuzuri\n{detail}\n```")),
            "{needle}: {hover}"
        );
    }
}

#[test]
fn declaration_hovers_name_modules_with_double_colons() {
    use serde_json::json;
    let shapes = "namespace Demo::Shapes\n\nrecord Circle { r: i64 }\nunion Kind = Round | Flat\ntype Radius = i64\nconst Unit: i64 = 1\nextern type Handle\nextern def tick :: i64 -> i64\ndef radius :: Circle -> i64 = \\c -> c.r\ndef keep :: Handle -> Handle = \\h -> h\n";
    let measures =
        "namespace Demo::Shapes\n\nclass Measure<'a> {\n    def size :: ref 'a -> i64\n}\n";
    let probe = "namespace Demo::Shapes\n\ndef probe :: i64 = Circle.tick 1\n";
    let tone =
        "namespace Demo::Shapes\n\nunion Tone = Light | Dark\n\ndef paint :: Tone = Tone.Dark\n";
    let files = [
        ("Circle.tz", shapes),
        ("Measures.tt", measures),
        ("Probe.tz", probe),
        ("Tone.tz", tone),
    ];
    let declarations = [
        (
            "Circle.tz",
            shapes,
            "Circle {",
            "record Demo::Shapes::Circle",
        ),
        (
            "Circle.tz",
            shapes,
            "Kind =",
            "union Demo::Shapes::Circle.Kind",
        ),
        ("Tone.tz", tone, "Tone =", "union Demo::Shapes::Tone"),
        (
            "Circle.tz",
            shapes,
            "Radius =",
            "type Demo::Shapes::Circle.Radius",
        ),
        (
            "Circle.tz",
            shapes,
            "Unit:",
            "const Demo::Shapes::Circle.Unit: i64",
        ),
        (
            "Circle.tz",
            shapes,
            "Handle",
            "extern type Demo::Shapes::Circle.Handle",
        ),
        (
            "Circle.tz",
            shapes,
            "radius ::",
            "def Demo::Shapes::Circle.radius :: Demo::Shapes::Circle -> i64",
        ),
        (
            "Circle.tz",
            shapes,
            "keep ::",
            "def Demo::Shapes::Circle.keep :: Demo::Shapes::Circle.Handle -> Demo::Shapes::Circle.Handle",
        ),
        (
            "Measures.tt",
            measures,
            "Measure<",
            "class Demo::Shapes::Measures.Measure",
        ),
        (
            "Measures.tt",
            measures,
            "size",
            "def Demo::Shapes::Measures.Measure.size",
        ),
    ];
    let responses = scripted(&files, "utf-16", |uri| {
        let mut steps: Vec<_> = declarations
            .iter()
            .enumerate()
            .map(|(id, (file, text, needle, _))| {
                json!({"id": id + 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri(file)}, "position": position(text, needle, "utf-16")}})
            })
            .collect();
        // An extern's name hovers its host call, so its detail shows in completion.
        let mut at = position(probe, "Circle.", "utf-16");
        at["character"] = json!(at["character"].as_u64().unwrap() + 7);
        steps.push(json!({"id": 100, "method": "textDocument/completion", "params": {"textDocument": {"uri": uri("Probe.tz")}, "position": at}}));
        let mut at = position(tone, "Tone.", "utf-16");
        at["character"] = json!(at["character"].as_u64().unwrap() + 5);
        steps.push(json!({"id": 101, "method": "textDocument/completion", "params": {"textDocument": {"uri": uri("Tone.tz")}, "position": at}}));
        steps
    });
    for (response, (_, _, needle, detail)) in responses.iter().zip(&declarations) {
        let hover = response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{needle}: {response}"));
        assert!(
            hover.starts_with(&format!("```tsuzuri\n{detail}\n```")),
            "{needle}: {hover}"
        );
    }
    let completed = &responses[declarations.len()]["result"]["items"];
    let items = completed
        .as_array()
        .unwrap_or_else(|| panic!("{completed}"));
    let tick = items
        .iter()
        .find(|item| item["label"] == "tick")
        .unwrap_or_else(|| panic!("{completed}"));
    assert_eq!(tick["detail"], "extern def Demo::Shapes::Circle.tick");
    // The type named after the module is the module path, not one of its
    // members, and its cases follow the module path directly.
    let detail = |items: &[serde_json::Value], label: &str| {
        items
            .iter()
            .find(|item| item["label"] == label)
            .map(|item| item["detail"].clone())
    };
    assert!(
        items.iter().all(|item| item["label"] != "Circle"),
        "{completed}"
    );
    assert_eq!(
        detail(items, "Round"),
        Some(json!("Demo::Shapes::Circle.Kind.Round")),
        "{completed}"
    );
    let toned = &responses[declarations.len() + 1]["result"]["items"];
    let tones = toned.as_array().unwrap_or_else(|| panic!("{toned}"));
    assert!(tones.iter().all(|item| item["label"] != "Tone"), "{toned}");
    assert_eq!(
        detail(tones, "Dark"),
        Some(json!("Demo::Shapes::Tone.Dark")),
        "{toned}"
    );
}

#[test]
fn namespaces_and_using_resolve_definitions_completions_and_tokens() {
    use serde_json::json;
    let files = [("Circle.tz", N_CIRCLE), ("Report.tz", N_REPORT)];
    let labels = |response: &serde_json::Value| -> Vec<(String, String)> {
        response["result"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .map(|item| {
                (
                    item["label"].as_str().unwrap().to_owned(),
                    item["detail"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    };
    let edited = |tail: &str| N_REPORT.replace("Circle.radius x + Shapes::Circle.radius x", tail);
    let mut circle = String::new();
    let responses = stepwise(&files, "utf-16", |uri| {
        circle = uri("Circle.tz");
        let report = uri("Report.tz");
        let change = |version: u64, text: &str| json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": report, "version": version}, "contentChanges": [{"text": text}]}});
        let complete = |id: u64, text: &str, needle: &str| {
            let mut at = position(text, needle, "utf-16");
            at["character"] = json!(at["character"].as_u64().unwrap() + needle.len() as u64);
            json!({"id": id, "method": "textDocument/completion", "params": {"textDocument": {"uri": report}, "position": at}})
        };
        let definition = |id: u64, needle: &str| json!({"id": id, "method": "textDocument/definition", "params": {"textDocument": {"uri": report}, "position": position(N_REPORT, needle, "utf-16")}});
        let (shapes, demo, roots) = (edited("Shapes::"), edited("Demo::"), edited("C"));
        let (member, colon) = (edited("Shapes::Circle."), edited("Shapes:"));
        // A compact `def name::Type` annotation does not start the path.
        let compact = format!("{N_REPORT}def probe::Demo::Shapes::");
        let compact_member = format!("{N_REPORT}def probe::Demo::Shapes::Circle.");
        let (std_path, std_member) = (edited("std::"), edited("std::Maybe."));
        vec![
            definition(1, "Circle.radius x +"),
            definition(2, "Shapes::Circle.radius"),
            json!({"id": 3, "method": "textDocument/semanticTokens/full", "params": {"textDocument": {"uri": report}}}),
            change(2, &shapes),
            complete(4, &shapes, "Shapes::"),
            change(3, &demo),
            complete(5, &demo, "-> Demo::"),
            change(4, &roots),
            complete(6, &roots, "\\x -> C"),
            change(5, &member),
            complete(7, &member, "Shapes::Circle."),
            change(6, &colon),
            complete(8, &colon, "Shapes:"),
            change(7, &compact),
            complete(9, &compact, "probe::Demo::Shapes::"),
            change(8, &compact_member),
            complete(10, &compact_member, "probe::Demo::Shapes::Circle."),
            change(9, &std_path),
            complete(11, &std_path, "-> std::"),
            change(10, &std_member),
            complete(12, &std_member, "std::Maybe."),
        ]
    });
    for response in &responses[..2] {
        assert_eq!(response["result"]["uri"], json!(circle), "{response}");
    }
    let header: Vec<_> = decode(&responses[2]["result"]["data"])
        .into_iter()
        .filter(|token| token[0] < 3)
        .collect();
    assert_eq!(
        header,
        [[0, 10, 4, 0, 0], [2, 6, 4, 0, 0], [2, 12, 6, 0, 0]]
    );
    assert_eq!(
        labels(&responses[3]),
        [(
            "Circle".to_owned(),
            "module Demo::Shapes::Circle".to_owned()
        )]
    );
    let demo = labels(&responses[4]);
    for expected in [
        ("Report", "module Demo::Report"),
        ("Shapes", "namespace Demo::Shapes"),
    ] {
        assert!(
            demo.contains(&(expected.0.to_owned(), expected.1.to_owned())),
            "{expected:?}: {demo:?}"
        );
    }
    let roots = labels(&responses[5]);
    for expected in [
        ("Circle", "module Demo::Shapes::Circle"),
        ("Demo", "namespace Demo"),
        ("Shapes", "namespace Demo::Shapes"),
        ("std", "namespace std"),
        ("Maybe", "module std::Maybe"),
        ("namespace", "keyword"),
        ("using", "keyword"),
    ] {
        assert!(
            roots.contains(&(expected.0.to_owned(), expected.1.to_owned())),
            "{expected:?}: {roots:?}"
        );
    }
    // A module's members follow `.`; a typed `:` alone offers nothing.
    assert_eq!(
        labels(&responses[6]),
        [(
            "radius".to_owned(),
            "def Demo::Shapes::Circle.radius :: i64 -> i64".to_owned()
        )]
    );
    let colon = labels(&responses[7]);
    assert!(colon.is_empty(), "{colon:?}");
    assert_eq!(labels(&responses[8]), labels(&responses[3]));
    assert_eq!(labels(&responses[9]), labels(&responses[6]));
    // The standard library is the namespace `std`.
    let std_modules = labels(&responses[10]);
    for expected in [
        ("Maybe", "module std::Maybe"),
        ("Result", "module std::Result"),
    ] {
        assert!(
            std_modules.contains(&(expected.0.to_owned(), expected.1.to_owned())),
            "{expected:?}: {std_modules:?}"
        );
    }
    let std_members = labels(&responses[11]);
    assert!(
        std_members
            .iter()
            .any(|(label, detail)| label == "map" && detail.starts_with("def std::Maybe.map ::")),
        "{std_members:?}"
    );

    // The current namespace's `Demo::Circle` wins over the imported one, and
    // `Square`, which both imports hold, is ambiguous.
    let report = "namespace Demo\n\nusing Demo::Shapes\nusing Demo::Extra\n\ndef total :: i64 -> i64 = \\x -> Circle.radius x\n";
    let side = "def side :: i64 -> i64 = \\x -> x\n";
    let files = [
        ("Circle.tz", N_CIRCLE.replace("Demo::Shapes", "Demo")),
        ("Shapes/Circle.tz", N_CIRCLE.to_owned()),
        (
            "Shapes/Square.tz",
            format!("namespace Demo::Shapes\n\n{side}"),
        ),
        (
            "Extra/Square.tz",
            format!("namespace Demo::Extra\n\n{side}"),
        ),
        ("Report.tz", report.to_owned()),
    ];
    let files = files.each_ref().map(|(name, text)| (*name, text.as_str()));
    let roots = report.replace("Circle.radius x", "C");
    let responses = stepwise(&files, "utf-16", |uri| {
        circle = uri("Circle.tz");
        let report_uri = uri("Report.tz");
        let mut at = position(&roots, "\\x -> C", "utf-16");
        at["character"] = json!(at["character"].as_u64().unwrap() + 7);
        vec![
            json!({"id": 1, "method": "textDocument/definition", "params": {"textDocument": {"uri": report_uri}, "position": position(report, "Circle.radius", "utf-16")}}),
            json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": report_uri, "version": 2}, "contentChanges": [{"text": roots}]}}),
            json!({"id": 2, "method": "textDocument/completion", "params": {"textDocument": {"uri": report_uri}, "position": at}}),
        ]
    });
    assert_eq!(
        responses[0]["result"]["uri"],
        json!(circle),
        "{}",
        responses[0]
    );
    let roots = labels(&responses[1]);
    for expected in [
        ("Circle", "module Demo::Circle"),
        ("Extra", "namespace Demo::Extra"),
        ("Shapes", "namespace Demo::Shapes"),
    ] {
        assert!(
            roots.contains(&(expected.0.to_owned(), expected.1.to_owned())),
            "{expected:?}: {roots:?}"
        );
    }
    assert!(
        roots.iter().all(|(label, _)| label != "Square"),
        "{roots:?}"
    );
}

#[test]
fn opt_in_matrix_module_completes_and_hovers_when_named() {
    use serde_json::json;
    let valid = "let m = Matrix.init 2 2 (\\i j -> i)\nlet n = Matrix.rows (ref m)\n";
    let incomplete = "let m = Matrix.init 2 2 (\\i j -> i)\nlet n = Matrix.";
    let responses = stepwise(&[("Main.tz", valid)], "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": position(valid, "rows", "utf-16")}}),
            json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": 2}, "contentChanges": [{"text": incomplete}]}}),
            json!({"id": 2, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": {"line": 1, "character": 15}}}),
        ]
    });
    assert_eq!(
        responses[0]["result"]["contents"]["value"],
        json!(
            "```tsuzuri\nref Matrix<i64> -> i64\n```\n\nThe number of rows.\n\n```tsuzuri\nlet m = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]\nMatrix.rows m\n```\n\nOutput:\n\n```text\n2\n```"
        ),
        "{}",
        responses[0]
    );
    let labels: Vec<&str> = responses[1]["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", responses[1]))
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    for member in [
        "of_array",
        "init",
        "rows",
        "cols",
        "at",
        "get",
        "row",
        "as_array",
        "to_array",
        "map",
        "fold",
        "transpose",
        "add",
        "mul",
        "set",
        "mul_fma",
        "mul_parallel",
        "mul_fma_parallel",
    ] {
        assert!(labels.contains(&member), "{member}: {labels:?}");
    }
}

#[test]
fn opt_in_view_and_tensor_modules_complete_when_named() {
    use serde_json::json;
    let cases: [(&str, &str, &str, &[&str]); 2] = [
        (
            "let a = Matrix.init 2 2 (\\i j -> i)\nlet v = MatrixView.of_matrix (ref a)\nlet n = MatrixView.rows v\n",
            "let a = Matrix.init 2 2 (\\i j -> i)\nlet v = MatrixView.of_matrix (ref a)\nlet n = MatrixView.",
            "let n = MatrixView.",
            &[
                "of_matrix",
                "of_array",
                "strided",
                "transpose",
                "sub",
                "row",
                "col",
                "to_matrix",
                "mul",
                "of_array_mut",
                "write",
                "row_mut",
                "sub_mut",
                "transpose_mut",
                "fill",
                "copy_from",
                "map_in_place",
                "freeze",
            ],
        ),
        (
            "let t = Tensor.init [2, 2] (\\i -> i)\nlet v = Tensor.view (ref t)\nlet n = Tensor.rank (ref v)\n",
            "let t = Tensor.init [2, 2] (\\i -> i)\nlet v = Tensor.view (ref t)\nlet n = Tensor.",
            "let n = Tensor.",
            &[
                "of_array",
                "init",
                "of_matrix",
                "to_matrix",
                "reshape",
                "view",
                "borrow",
                "permute",
                "index_axis",
                "narrow",
                "reshape_view",
                "to_tensor",
                "as_matrix_view",
                "of_matrix_view",
            ],
        ),
    ];
    for (valid, incomplete, last_line, members) in cases {
        let responses = stepwise(&[("Main.tz", valid)], "utf-16", |uri| {
            let main = uri("Main.tz");
            vec![
                json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": {"line": 0, "character": 4}}}),
                json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": 2}, "contentChanges": [{"text": incomplete}]}}),
                json!({"id": 2, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": {"line": 2, "character": last_line.len()}}}),
            ]
        });
        let labels: Vec<&str> = responses[1]["result"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{}", responses[1]))
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect();
        for member in members {
            assert!(labels.contains(member), "{member}: {labels:?}");
        }
    }
}

/// PB08 O6: the language server loads only the opt-in std modules that the sources name. Runs
/// `steps` on `main` alone, which names none, and with a second file whose comment names `Json` and
/// `Matrix`, so that every analysis loads them as every analysis did before; returns the results of
/// the requests and the last diagnostics of `Main.tz` of each, with the root written `ROOT/`. The
/// steps go one at a time, each request after the response to the one before, as in `stepwise`
/// (G39): a request followed by an edit of its document answers for the text it was sent for.
fn opt_in_sessions(
    main: &str,
    steps: impl Fn(&dyn Fn(&str) -> String) -> Vec<serde_json::Value>,
) -> [(Vec<serde_json::Value>, serde_json::Value); 2] {
    let run = |files: &[(&str, &str)]| {
        let fixture = live::Fixture::new();
        for (name, text) in files {
            fixture.write(name, text);
        }
        let mut server = live::Server::start(&fixture.uri(""), "utf-16");
        for (name, text) in files {
            server.open(&fixture.uri(name), text);
        }
        let root = fixture
            .uri("Main.tz")
            .strip_suffix("Main.tz")
            .unwrap()
            .to_owned();
        let rooted = |value: &serde_json::Value| -> serde_json::Value {
            serde_json::from_str(&value.to_string().replace(&root, "ROOT/")).unwrap()
        };
        let uri = |name: &str| fixture.uri(name);
        let mut results = Vec::new();
        for step in steps(&uri) {
            match step.get("id").cloned() {
                Some(id) => {
                    server.send(step);
                    let response = server.wait(
                        |message| message.get("id") == Some(&id) && message.get("method").is_none(),
                        &format!("the response {id}"),
                    );
                    results.push(rooted(&response["result"]));
                }
                None => server.send(step),
            }
        }
        let diagnostics = server
            .seen
            .iter()
            .rfind(|message| {
                message["method"] == "textDocument/publishDiagnostics"
                    && message["params"]["uri"]
                        .as_str()
                        .is_some_and(|uri| uri.ends_with("/Main.tz"))
            })
            .map(|message| rooted(&message["params"]["diagnostics"]))
            .unwrap_or_default();
        server.finish();
        (results, diagnostics)
    };
    [
        run(&[("Main.tz", main)]),
        run(&[
            ("Main.tz", main),
            ("Names.tz", "// Json Matrix\ndef names_marker :: i64 = 0\n"),
        ]),
    ]
}

fn labels(result: &serde_json::Value) -> Vec<&str> {
    result["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"))
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect()
}

#[test]
fn opt_in_module_names_complete_in_a_project_that_names_none() {
    use serde_json::json;
    let main = "let answer = 42\nanswer\n";
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/completion", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": {"line": 2, "character": 0}}}),
        ]
    });
    let items = responses[0]["result"]["items"].as_array().unwrap();
    for (path, _) in tsuzuri::stdlib::SOURCES {
        let name = tsuzuri::stdlib::module_name(path).unwrap();
        let item = items
            .iter()
            .find(|item| item["label"] == name)
            .unwrap_or_else(|| panic!("{name}: {items:?}"));
        assert_eq!(item["kind"], 9, "{item}");
        assert_eq!(item["detail"], format!("module std::{name}"), "{item}");
    }
}

#[test]
fn opt_in_members_complete_and_sign_while_the_first_edit_that_names_them_does_not_parse() {
    use serde_json::json;
    let main = "let answer = 42\nanswer\n";
    let edits = [
        "let answer = 42\nanswer\nlet probe = Json.",
        "let answer = 42\nanswer\nlet probe = Matrix.",
        "let answer = 42\nanswer\nlet probe = Json.Value.",
        "let answer = 42\nanswer\nlet probe = Json.parse (",
    ];
    let [unnamed, named] = opt_in_sessions(main, |uri| {
        let main = uri("Main.tz");
        // The analysis of the text that names neither module, which the requests fall back to.
        let mut steps = vec![
            json!({"id": 0, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": main}}}),
        ];
        for (version, text) in edits.iter().enumerate() {
            let line = text.lines().last().unwrap();
            let at = json!({"line": 2, "character": line.len()});
            let method = if text.ends_with('(') {
                "textDocument/signatureHelp"
            } else {
                "textDocument/completion"
            };
            steps.push(json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": version + 2}, "contentChanges": [{"text": text}]}}));
            steps.push(json!({"id": version + 1, "method": method, "params": {"textDocument": {"uri": main}, "position": at}}));
        }
        steps
    });
    assert_eq!(unnamed.0, named.0);
    let [_, json, matrix, value, signature] = unnamed.0.as_slice() else {
        panic!("{:?}", unnamed.0);
    };
    for member in ["parse", "serialize", "Value", "Error", "Null"] {
        assert!(labels(json).contains(&member), "{member}: {json}");
    }
    for member in ["init", "rows", "of_array", "transpose"] {
        assert!(labels(matrix).contains(&member), "{member}: {matrix}");
    }
    for case in ["Null", "Bool", "Number", "Items"] {
        assert!(labels(value).contains(&case), "{case}: {value}");
    }
    let label = signature["signatures"][0]["label"]
        .as_str()
        .unwrap_or_default();
    assert!(
        label.starts_with("parse (") && label.contains("ref utf8string"),
        "{signature}"
    );
    assert_eq!(signature["activeParameter"], 0, "{signature}");
}

#[test]
fn opt_in_module_names_hover_once_an_edit_names_them() {
    use serde_json::json;
    let main = "let answer = 42\nanswer\n";
    let edited = "let parsed = Json.parse (ref u8\"1\")\nlet m = Matrix.init 2 2 (\\i j -> i)\nlet answer = 42\nanswer\n";
    let [unnamed, named] = opt_in_sessions(main, |uri| {
        let main = uri("Main.tz");
        let hover = |id: u64, needle: &str| json!({"id": id, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": position(edited, needle, "utf-16")}});
        vec![
            json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": 2}, "contentChanges": [{"text": edited}]}}),
            hover(1, "Json"),
            hover(2, "parse ("),
            hover(3, "Matrix"),
            hover(4, "init"),
        ]
    });
    assert_eq!(unnamed.0, named.0);
    for (result, doc) in unnamed.0.iter().zip([
        "Parses one JSON text",
        "Parses one JSON text",
        "once per element",
        "once per element",
    ]) {
        let value = result["contents"]["value"].as_str().unwrap_or_default();
        assert!(value.contains(doc), "{doc}: {result}");
    }
}

#[test]
fn unknown_names_get_the_candidates_and_quickfixes_of_unloaded_opt_in_modules() {
    use serde_json::json;
    let main = "let m = Matrx.init 2 2 (\\i j -> i)\nlet a = parse\nlet b = Null\n0\n";
    let [unnamed, named] = opt_in_sessions(main, |uri| {
        let main = uri("Main.tz");
        let quickfix = |id: u64, line: usize, start: usize, end: usize| json!({"id": id, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": main}, "range": range(line, start, end), "context": {"diagnostics": [], "only": ["quickfix"]}}});
        vec![
            quickfix(1, 0, 8, 13),
            quickfix(2, 1, 8, 13),
            quickfix(3, 2, 8, 12),
        ]
    });
    assert_eq!(unnamed, named);
    let fixes = |message: &str| {
        let diagnostic = unnamed
            .1
            .as_array()
            .unwrap()
            .iter()
            .find(|diagnostic| diagnostic["message"] == message)
            .unwrap_or_else(|| panic!("{message}: {}", unnamed.1))
            .clone();
        diagnostic["data"]["fixes"].clone()
    };
    // `Matrix` is one edit away; `parse` is the last part of `Format.parse` and of `Json.parse`, so
    // no name is best; `Null` is the last part of `Json.Null` alone.
    assert_eq!(
        fixes("unknown value 'Matrx'"),
        json!([{"title": "Replace 'Matrx' with 'Matrix'", "applicability": "maybe"}])
    );
    assert_eq!(fixes("unknown value 'parse'"), serde_json::Value::Null);
    assert_eq!(
        fixes("unknown value 'Null'"),
        json!([{"title": "Replace 'Null' with 'Json.Null'", "applicability": "maybe"}])
    );
    let [matrix, parse, null] = unnamed.0.as_slice() else {
        panic!("{:?}", unnamed.0);
    };
    assert_eq!(
        matrix[0]["title"], "Replace 'Matrx' with 'Matrix'",
        "{matrix}"
    );
    assert_eq!(
        matrix[0]["edit"]["changes"]["ROOT/Main.tz"],
        json!([{"range": range(0, 8, 13), "newText": "Matrix"}])
    );
    assert_eq!(*parse, json!([]));
    assert_eq!(
        null[0]["title"], "Replace 'Null' with 'Json.Null'",
        "{null}"
    );
}

/// PB08 review of Phase 2: the language server loads the opt-in std modules that `check` loads, so
/// a check over every loaded std instance (here the overlap of instances) follows `check` for an
/// opt-in module that no source names. `Main.tz` gives `Monoid` a blanket instance, which overlapped
/// the instances of `std/Monoids.tz` while the server loaded every std module (9 `E1016` in that
/// file, none in `Main.tz`). The editor publishes what `check --json` reports, the `W1001` of `b`
/// with its `certain` fix, and `source.fixAll.tsuzuri` applies what `tsuzuri fix` applies.
#[test]
fn diagnostics_and_fix_all_follow_check_and_fix_for_an_opt_in_module_that_no_source_names() {
    use serde_json::{Value, json};
    const CLASSES: &str = "class Zero<'a> { def zero :: unit -> 'a }\n";
    const MAIN: &str = "instance Classes.Zero<'a> => Monoid<'a> {\n    fn empty _ = Classes.Zero.zero ()\n    fn combine a b = a\n}\ninstance Classes.Zero<i64> { fn zero _ = 0 }\ndef main :: own unit -> i32 = \\() -> 0\n";
    let files = [("Classes.tt", CLASSES), ("Main.tz", MAIN)];
    let mut main = String::new();
    let (messages, _) = session(&files, "utf-16", |uri| {
        main = uri("Main.tz");
        // A request makes the server analyze the project and publish its diagnostics.
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": {"line": 2, "character": 7}}}),
            json!({"id": 2, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": main}, "range": range(0, 0, 0), "context": {"diagnostics": [], "only": ["source.fixAll"]}}}),
        ]
    });
    let mut published = std::collections::BTreeMap::new();
    for message in &messages {
        if message["method"] == "textDocument/publishDiagnostics" {
            published.insert(
                message["params"]["uri"].as_str().unwrap().to_owned(),
                message["params"]["diagnostics"].clone(),
            );
        }
    }
    for (uri, diagnostics) in &published {
        if *uri != main {
            assert_eq!(*diagnostics, json!([]), "{uri}");
        }
    }
    // `check --json` of the same files, as the server writes a diagnostic: zero-based positions
    // (the text is ASCII, so columns are UTF-16 code units), the kind and the fix titles in `data`.
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-lsp-follows-check-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (name, text) in files {
        std::fs::write(root.join(name), text).unwrap();
    }
    let tsuzuri = |arguments: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", "")
            .output()
            .unwrap()
    };
    let output = tsuzuri(&["check", root.to_str().unwrap(), "--json"]);
    // And what `tsuzuri fix` writes.
    let fixed = tsuzuri(&["fix", root.to_str().unwrap()]);
    assert!(fixed.status.success(), "{fixed:?}");
    let fixed = std::fs::read_to_string(root.join("Main.tz")).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    let position = |line: &Value, column: &Value| json!({"line": line.as_u64().unwrap() - 1, "character": column.as_u64().unwrap() - 1});
    // `check --json` writes its diagnostics to stderr, one JSON object per line.
    let checked: Vec<Value> = String::from_utf8(output.stderr)
        .unwrap()
        .lines()
        .map(|line| {
            let diagnostic: Value = serde_json::from_str(line).unwrap();
            assert!(diagnostic["path"].as_str().unwrap().ends_with("Main.tz"), "{line}");
            let span = &diagnostic["span"];
            let mut data = serde_json::Map::new();
            if let Some(kind) = diagnostic.get("kind") {
                data.insert("kind".into(), kind.clone());
            }
            if let Some(fixes) = diagnostic["fixes"].as_array() {
                let titles: Vec<Value> = fixes
                    .iter()
                    .map(|fix| json!({"title": fix["title"], "applicability": fix["applicability"]}))
                    .collect();
                data.insert("fixes".into(), json!(titles));
            }
            json!({
                "range": {"start": position(&span["line"], &span["column"]), "end": position(&span["end_line"], &span["end_column"])},
                "severity": if diagnostic["severity"] == "error" { 1 } else { 2 },
                "code": diagnostic["code"],
                "source": "tsuzuri",
                "message": diagnostic["message"],
                "data": data,
            })
        })
        .collect();
    assert_eq!(published[&main], json!(checked));
    // The parameter `b` of `combine` is unused, and nothing else is wrong.
    assert_eq!(
        published[&main],
        json!([{
            "range": range(2, 17, 18),
            "severity": 2,
            "code": "W1001",
            "source": "tsuzuri",
            "message": "unused local 'b'; prefix it with '_' to silence this warning",
            "data": {"kind": "unused_local", "fixes": [{"title": "Prefix 'b' with '_'", "applicability": "certain"}]},
        }])
    );
    let response = messages
        .iter()
        .find(|message| message.get("id") == Some(&json!(2)))
        .unwrap();
    let actions = response["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0]["kind"], "source.fixAll.tsuzuri");
    let edits = actions[0]["edit"]["changes"][main.as_str()]
        .as_array()
        .unwrap();
    assert_eq!(apply_text_edits(MAIN, edits), fixed);
    assert_eq!(fixed, MAIN.replace("fn combine a b", "fn combine a _b"));
}

#[test]
fn deprecated_uses_from_other_packages_are_published_as_warnings() {
    use serde_json::json;
    let manifest = |name: &str, dependencies: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{dependencies}")
    };
    let app = "let _light = LegacyLib::Old.Red\nLegacyLib::Old.old 1\n";
    let old = "/// @deprecated use next\ndef old :: i64 -> i64 = \\x -> x\n\n/// @deprecated\nunion Light = Red | Green\n\ndef inside :: i64 -> i64 = \\x -> old x\n";
    let (messages, _) = session(
        &[
            (
                "Tsuzuri.toml",
                &manifest(
                    "app",
                    "[dependencies]\nlegacy-lib = { path = \"legacy-lib\" }\n",
                ),
            ),
            ("Main.tz", app),
            ("legacy-lib/Tsuzuri.toml", &manifest("legacy-lib", "")),
            ("legacy-lib/Old.tz", old),
        ],
        "utf-16",
        |uri| {
            // A request makes the server analyze the project and publish its diagnostics.
            vec![
                json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": {"line": 0, "character": 4}}}),
            ]
        },
    );
    // The last notification of a file is the state after the whole project was analyzed.
    let published = |file: &str| {
        messages
            .iter()
            .filter(|message| message["method"] == "textDocument/publishDiagnostics")
            .rfind(|message| message["params"]["uri"].as_str().unwrap().ends_with(file))
            .map(|message| message["params"]["diagnostics"].clone())
            .unwrap_or_else(|| panic!("nothing published for {file}"))
    };
    assert_eq!(
        published("/Main.tz"),
        json!([
            {"range": range(0, 28, 31), "severity": 2, "code": "W1005", "source": "tsuzuri",
             "message": "'LegacyLib::Old.Light' is deprecated; see its documentation for a replacement",
             "data": {"kind": "deprecated_use"}},
            {"range": range(1, 15, 18), "severity": 2, "code": "W1005", "source": "tsuzuri",
             "message": "'LegacyLib::Old.old' is deprecated: use next", "data": {"kind": "deprecated_use"}},
        ])
    );
    // A use inside the package that declares it is not reported.
    assert_eq!(published("/Old.tz"), json!([]));
}

const MOVE_TWICE: &str = "def consume :: own string -> i64 = \\s -> s.length\n\ndef main_value :: unit -> i64 = \\_ ->\n    let text = \"hello\"\n    let a = consume text\n    let b = consume text\n    a + b\n";
const MISSING_ARM: &str = "union Shape = Circle of f64 | Square of f64 | Triangle of f64\n\ndef area :: Shape -> f64 = \\s ->\n    match s with\n    | Circle r -> r\n    | Square w -> w\n";
const NESTED_MISSING_ARM: &str = "union Shape = Circle of f64 | Square of f64 | Triangle of f64\n\ndef area :: Shape -> Shape -> f64 = \\s t ->\n    match s with\n    | Circle r -> r\n    | Square w -> match t with | Circle q -> q | Square z -> z | Triangle _ -> unreachable ()\n";

#[test]
fn publishes_related_information_and_data() {
    use serde_json::json;
    let (messages, _) = session(&[("Main.tz", MOVE_TWICE)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": {"line": 0, "character": 4}}}),
        ]
    });
    let published = messages
        .iter()
        .rfind(|message| message["method"] == "textDocument/publishDiagnostics")
        .unwrap();
    let diagnostics = published["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["code"], "E1012");
    assert_eq!(diagnostic["range"], range(5, 20, 24));
    let related = diagnostic["relatedInformation"].as_array().unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["message"], "value moved here");
    assert_eq!(related[0]["location"]["uri"], published["params"]["uri"]);
    assert_eq!(related[0]["location"]["range"], range(4, 20, 24));
    // Every diagnostic has a kind (G23); this one has no fixes.
    assert_eq!(
        diagnostic["data"],
        json!({"kind": "use_after_move"}),
        "{diagnostic}"
    );
}

#[test]
fn code_action_offers_a_verified_fix() {
    use serde_json::json;
    let responses = scripted(&[("Main.tz", MISSING_ARM)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range(3, 4, 5), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
        ]
    });
    let actions = responses[0]["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(
        actions[0]["title"],
        "Add the arm '| Triangle _ -> unreachable ()'"
    );
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(actions[0]["isPreferred"], false);
    let changes = actions[0]["edit"]["changes"].as_object().unwrap();
    assert_eq!(changes.len(), 1);
    let edits = changes.values().next().unwrap().as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0]["newText"], " | Triangle _ -> unreachable ()");
    assert_eq!(
        edits[0]["range"],
        json!({"start": {"line": 5, "character": 19}, "end": {"line": 5, "character": 19}})
    );
    assert_eq!(
        actions[0]["diagnostics"][0]["data"]["fixes"][0]["applicability"],
        "maybe"
    );
}

#[test]
fn code_action_omits_a_fix_that_does_not_verify() {
    use serde_json::json;
    let responses = scripted(&[("Main.tz", NESTED_MISSING_ARM)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range(3, 4, 5), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
        ]
    });
    assert_eq!(responses[0]["result"], json!([]));
}

#[test]
fn code_action_verifies_a_fix_past_the_display_limit() {
    use serde_json::json;
    // 51 non-exhaustive matches: only 50 are published, but a fix removes one of all 51.
    let source: String = (0..51)
        .map(|index| {
            format!("def f{index} :: bool -> i64 = \\b ->\n    match b with\n    | true -> 1\n\n")
        })
        .collect();
    let responses = scripted(&[("Main.tz", source.as_str())], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": uri("Main.tz")}, "range": range(1, 4, 5), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
        ]
    });
    let actions = responses[0]["result"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(
        actions[0]["title"],
        "Add the arm '| false -> unreachable ()'"
    );
}

#[test]
fn hover_shows_attributes_and_inferred_effects() {
    use serde_json::json;
    let main = "@no_copy\ndef total :: ref [i64] -> i64\nfn total xs = Array.sum xs\n\
                @no_trap\ndef first :: ref [i64] -> i64\nfn first xs = xs.length\n\
                @no_copy\ndef pick :: ref [i64] -> i64 -> i64\nfn pick xs i = xs[i]\n\
                def plain :: i64 -> i64\nfn plain n = n\n\
                @no_alloc\ndef apply :: (i64 -> i64) -> i64 -> i64\nfn apply f x = f x\n\
                def main :: unit -> i32 = \\() -> (total [1, 2] + first [3] + pick [4] 0 + plain 1 + apply plain 2) as i32\n";
    let hovers = [
        (
            "total xs",
            "@no_copy def Main.total :: ref [i64] -> i64",
            Some("inferred effects: none"),
        ),
        (
            "first xs",
            "@no_trap def Main.first :: ref [i64] -> i64",
            Some("inferred effects: none"),
        ),
        (
            "pick xs",
            "@no_copy def Main.pick :: ref [i64] -> i64 -> i64",
            Some("inferred effects: trap"),
        ),
        ("plain n", "def Main.plain :: i64 -> i64", None),
        // Review 3 of #63: the effects of the callbacks are not in the summary.
        (
            "apply f x",
            "@no_alloc def Main.apply :: (i64 -> i64) -> i64 -> i64",
            Some("inferred effects: none (and the effects of the functions it is given)"),
        ),
    ];
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        hovers
            .iter()
            .enumerate()
            .map(|(id, (needle, _, _))| {
                json!({"id": id + 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": position(main, needle, "utf-16")}})
            })
            .collect()
    });
    for (response, (needle, detail, effects)) in responses.iter().zip(&hovers) {
        let hover = response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{needle}: {response}"));
        assert!(
            hover.starts_with(&format!("```tsuzuri\n{detail}\n```")),
            "{needle}: {hover}"
        );
        match effects {
            Some(line) => assert!(hover.contains(line), "{needle}: {hover}"),
            None => assert!(!hover.contains("inferred effects"), "{needle}: {hover}"),
        }
    }
}

/// Review of #63: the hover's summary is complete (a lone `@no_copy` still shows `trap`), and a
/// function in a namespace finds its summary.
#[test]
fn hover_shows_complete_effects_in_namespaces() {
    use serde_json::json;
    let main = "namespace Demo::Shapes\n@no_copy\ndef pick :: ref [i64] -> i64 -> i64\nfn pick xs i = xs[i]\n\
                def main :: unit -> i32 = \\() -> (pick [4] 0) as i32\n";
    let responses = scripted(&[("Main.tz", main)], "utf-16", |uri| {
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri("Main.tz")}, "position": position(main, "pick xs", "utf-16")}}),
        ]
    });
    let hover = responses[0]["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{}", responses[0]));
    assert!(hover.contains("Demo::Shapes"), "{hover}");
    assert!(hover.contains("inferred effects: trap"), "{hover}");
}

/// A server on its own thread that the test feeds one message at a time and whose output it reads
/// as it comes, so that a test can change the disk between messages (G39). Pipelined input (messages
/// sent without waiting for their answers) is for the tests of the server's queue only.
mod live {
    use serde_json::{Value, json};
    use std::io::{self, Read, Write};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;
    use tsuzuri::lsp::{file_uri, serve, write_message};

    /// The bytes that the test sends, read by the server as they arrive.
    struct Input {
        receiver: mpsc::Receiver<Vec<u8>>,
        chunk: Vec<u8>,
        at: usize,
    }

    impl Read for Input {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            while self.at == self.chunk.len() {
                match self.receiver.recv() {
                    Ok(chunk) => (self.chunk, self.at) = (chunk, 0),
                    Err(_) => return Ok(0),
                }
            }
            let count = buffer.len().min(self.chunk.len() - self.at);
            buffer[..count].copy_from_slice(&self.chunk[self.at..self.at + count]);
            self.at += count;
            Ok(count)
        }
    }

    /// The server's output, handed to the test a message at a time.
    struct Output {
        buffer: Vec<u8>,
        sender: mpsc::Sender<Value>,
    }

    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            loop {
                let Some(end) = self
                    .buffer
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                else {
                    return Ok(());
                };
                let header = std::str::from_utf8(&self.buffer[..end]).unwrap();
                let length: usize = header
                    .strip_prefix("Content-Length: ")
                    .unwrap()
                    .parse()
                    .unwrap();
                if self.buffer.len() < end + 4 + length {
                    return Ok(());
                }
                let body: Vec<u8> = self
                    .buffer
                    .drain(..end + 4 + length)
                    .skip(end + 4)
                    .collect();
                let _ = self.sender.send(serde_json::from_slice(&body).unwrap());
            }
        }
    }

    pub struct Server {
        input: Option<mpsc::Sender<Vec<u8>>>,
        output: mpsc::Receiver<Value>,
        thread: Option<std::thread::JoinHandle<i32>>,
        next: u64,
        /// Every message that the server wrote so far, in order.
        pub seen: Vec<Value>,
    }

    impl Server {
        /// Starts a server and initializes it with `root_uri` as the root.
        pub fn start(root_uri: &str, encoding: &str) -> Self {
            let capabilities = if encoding == "utf-8" {
                json!({"general": {"positionEncodings": ["utf-8"]}})
            } else {
                json!({})
            };
            Self::start_with(json!({"rootUri": root_uri, "capabilities": capabilities}))
        }

        /// Starts a server and initializes it with the parameters `initialize`.
        pub fn start_with(initialize: Value) -> Self {
            let (input, receiver) = mpsc::channel();
            let (sender, output) = mpsc::channel();
            let thread = std::thread::spawn(move || {
                serve(
                    Input {
                        receiver,
                        chunk: Vec::new(),
                        at: 0,
                    },
                    Output {
                        buffer: Vec::new(),
                        sender,
                    },
                    Vec::new(),
                )
            });
            let mut server = Self {
                input: Some(input),
                output,
                thread: Some(thread),
                // Above the small ids that the tests' own steps use.
                next: 1000,
                seen: Vec::new(),
            };
            let response = server.request("initialize", initialize);
            assert!(response.get("result").is_some(), "{response}");
            server
        }

        /// Sends one message without waiting for anything.
        pub fn send(&mut self, mut message: Value) {
            message["jsonrpc"] = json!("2.0");
            let mut bytes = Vec::new();
            write_message(&mut bytes, &message).unwrap();
            self.input.as_ref().unwrap().send(bytes).unwrap();
        }

        pub fn notify(&mut self, method: &str, params: Value) {
            self.send(json!({"method": method, "params": params}));
        }

        pub fn open(&mut self, uri: &str, text: &str) {
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument": {"uri": uri, "languageId": "tsuzuri", "version": 1, "text": text}}),
            );
        }

        pub fn change(&mut self, uri: &str, version: i64, text: &str) {
            self.notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
            );
        }

        /// A fresh request id.
        pub fn id(&mut self) -> u64 {
            self.next += 1;
            self.next
        }

        /// Sends a request and waits for its response.
        pub fn request(&mut self, method: &str, params: Value) -> Value {
            let id = self.id();
            self.send(json!({"id": id, "method": method, "params": params}));
            self.response(id)
        }

        /// Waits for the response to the request `id`.
        pub fn response(&mut self, id: u64) -> Value {
            self.wait(
                |message| message.get("id") == Some(&json!(id)) && message.get("method").is_none(),
                &format!("the response {id}"),
            )
        }

        /// The first message from `self.seen` on that satisfies `predicate`, waiting for it.
        pub fn wait(&mut self, predicate: impl Fn(&Value) -> bool, what: &str) -> Value {
            if let Some(found) = self.seen.iter().find(|message| predicate(message)) {
                return found.clone();
            }
            loop {
                let message = self
                    .output
                    .recv_timeout(Duration::from_secs(120))
                    .unwrap_or_else(|_| panic!("no {what}; received {:#?}", self.seen));
                self.seen.push(message.clone());
                if predicate(&message) {
                    return message;
                }
            }
        }

        /// The diagnostics of every `publishDiagnostics` for `uri` so far, in order.
        pub fn published(&self, uri: &str) -> Vec<Value> {
            self.seen
                .iter()
                .filter(|message| {
                    message["method"] == "textDocument/publishDiagnostics"
                        && message["params"]["uri"] == uri
                })
                .map(|message| message["params"]["diagnostics"].clone())
                .collect()
        }

        /// The codes of the last diagnostics published for `uri`.
        pub fn codes(&self, uri: &str) -> Vec<String> {
            let published = self.published(uri);
            let last = published
                .last()
                .unwrap_or_else(|| panic!("nothing published for {uri}: {:#?}", self.seen));
            last.as_array()
                .unwrap()
                .iter()
                .map(|diagnostic| diagnostic["code"].as_str().unwrap().to_owned())
                .collect()
        }

        /// Makes the server analyze every root it has scheduled and waits until it has
        /// (`workspace/symbol` refreshes the pending roots first).
        pub fn settle(&mut self) {
            let response = self.request("workspace/symbol", json!({"query": ""}));
            assert!(response.get("result").is_some(), "{response}");
        }

        /// Shuts the server down and checks that it exits with 0.
        pub fn finish(mut self) {
            let response = self.request("shutdown", Value::Null);
            assert!(response["result"].is_null(), "{response}");
            self.notify("exit", Value::Null);
            self.input.take();
            assert_eq!(self.thread.take().unwrap().join().unwrap(), 0);
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            // A failed test must not leave the server's thread reading forever.
            self.input.take();
        }
    }

    /// A temporary directory, canonicalized, that holds a test's roots and dependencies.
    pub struct Fixture {
        pub base: PathBuf,
    }

    impl Fixture {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static FIXTURES: AtomicUsize = AtomicUsize::new(0);
            let base = std::env::temp_dir().join(format!(
                "tsuzuri-lsp-live-{}-{}-{}",
                std::process::id(),
                FIXTURES.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&base).unwrap();
            Self {
                base: base.canonicalize().unwrap(),
            }
        }

        pub fn path(&self, name: &str) -> PathBuf {
            if name.is_empty() {
                self.base.clone()
            } else {
                self.base.join(name)
            }
        }

        pub fn write(&self, name: &str, text: &str) {
            let path = self.path(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        pub fn remove(&self, name: &str) {
            let path = self.path(name);
            if path.is_dir() {
                std::fs::remove_dir_all(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }

        pub fn rename(&self, from: &str, to: &str) {
            std::fs::rename(self.path(from), self.path(to)).unwrap();
        }

        /// The URI of `name` under the canonical base.
        pub fn uri(&self, name: &str) -> String {
            file_uri(&self.path(name)).unwrap()
        }

        /// The URI of `name` under another spelling of the base (a symbolic link to it). Only the tests of symbolic
        /// links use it, and they run on Unix.
        #[cfg(unix)]
        pub fn uri_under(&self, base: &std::path::Path, name: &str) -> String {
            file_uri(&if name.is_empty() {
                base.to_owned()
            } else {
                base.join(name)
            })
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }
}

/// Runs `steps` as `scripted` does, but sends one message at a time and waits for each request's
/// response before the next message (G39): a request followed by an edit of its document answers
/// for the text it was sent for.
fn stepwise(
    files: &[(&str, &str)],
    encoding: &str,
    steps: impl FnOnce(&dyn Fn(&str) -> String) -> Vec<serde_json::Value>,
) -> Vec<serde_json::Value> {
    let fixture = live::Fixture::new();
    for (name, text) in files {
        fixture.write(name, text);
    }
    let mut server = live::Server::start(&fixture.uri(""), encoding);
    for (name, text) in files {
        server.open(&fixture.uri(name), text);
    }
    let uri = |name: &str| fixture.uri(name);
    let mut responses = Vec::new();
    for step in steps(&uri) {
        match step.get("id").cloned() {
            Some(id) => {
                server.send(step);
                responses.push(server.wait(
                    |message| message.get("id") == Some(&id) && message.get("method").is_none(),
                    &format!("the response {id}"),
                ));
            }
            None => server.send(step),
        }
    }
    server.finish();
    responses
}

/// G39 段 6: a file that leaves the project (removed, renamed, or in a removed folder) gets an empty
/// list at the URI that its diagnostics were sent to.
#[test]
fn files_that_leave_the_project_get_an_empty_list() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("Main.tz", main);
    fixture.write("Helper.tz", "fn offset() -> i64 { true }\n");
    fixture.write("Old.tz", "fn old() -> i64 { true }\n");
    fixture.write("Sub/Inner.tz", "fn inner() -> i64 { true }\n");
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    server.open(&fixture.uri("Main.tz"), main);
    server.settle();
    for name in ["Helper.tz", "Old.tz", "Sub/Inner.tz"] {
        assert_eq!(server.codes(&fixture.uri(name)), ["E1003"], "{name}");
    }
    fixture.remove("Helper.tz");
    fixture.rename("Old.tz", "New.tz");
    fixture.remove("Sub");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [
            {"uri": fixture.uri("Helper.tz"), "type": 3},
            {"uri": fixture.uri("Old.tz"), "type": 3},
            {"uri": fixture.uri("New.tz"), "type": 1},
            {"uri": fixture.uri("Sub"), "type": 3},
        ]}),
    );
    server.settle();
    for name in ["Helper.tz", "Old.tz", "Sub/Inner.tz"] {
        assert!(server.codes(&fixture.uri(name)).is_empty(), "{name}");
    }
    assert_eq!(server.codes(&fixture.uri("New.tz")), ["E1003"]);
    server.finish();
}

const APP_MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";

/// G39 段 6: a fixed manifest gets an empty list, and so do the files of a path dependency inside the
/// root that the manifest no longer names.
#[test]
fn a_fixed_manifest_and_a_dropped_dependency_get_an_empty_list() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    let with_lib = format!("{APP_MANIFEST}\n[dependencies]\nlib = {{ path = \"lib\" }}\n");
    fixture.write("Tsuzuri.toml", &with_lib);
    fixture.write("Main.tz", main);
    fixture.write(
        "lib/Tsuzuri.toml",
        "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n",
    );
    fixture.write("lib/Calc.tz", "fn calc() -> i64 { true }\n");
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    server.open(&fixture.uri("Main.tz"), main);
    server.settle();
    assert_eq!(server.codes(&fixture.uri("lib/Calc.tz")), ["E1003"]);
    let changed = |server: &mut live::Server| {
        server.notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": fixture.uri("Tsuzuri.toml"), "type": 2}]}),
        );
        server.settle();
    };
    fixture.write("Tsuzuri.toml", &format!("{with_lib}bogus\n"));
    changed(&mut server);
    assert_eq!(server.codes(&fixture.uri("Tsuzuri.toml")), ["E0002"]);
    fixture.write("Tsuzuri.toml", &with_lib);
    changed(&mut server);
    assert!(server.codes(&fixture.uri("Tsuzuri.toml")).is_empty());
    assert_eq!(server.codes(&fixture.uri("lib/Calc.tz")), ["E1003"]);
    fixture.write("Tsuzuri.toml", APP_MANIFEST);
    fixture.remove("lib");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": fixture.uri("Tsuzuri.toml"), "type": 2}, {"uri": fixture.uri("lib"), "type": 3}]}),
    );
    server.settle();
    assert!(server.codes(&fixture.uri("lib/Calc.tz")).is_empty());
    server.finish();
}

/// G39 段 7: while the project does not load, the other files' diagnostics of the last analysis are
/// cleared, and the load error reaches a document that was opened and closed. A manifest error is at
/// the span that the CLI reports.
#[test]
fn a_load_error_clears_the_other_files_and_reaches_closed_documents() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let broken = "fn value() -> i64 { true }\n";
    let fixed = "fn value() -> i64 { 1 }\n";
    fixture.write("Tsuzuri.toml", APP_MANIFEST);
    fixture.write("Main.tz", broken);
    let main = fixture.uri("Main.tz");
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    server.open(&main, broken);
    server.settle();
    assert_eq!(server.codes(&main), ["E1003"]);
    let manifest = format!("{APP_MANIFEST}edition = \"2027\"\nbogus\n");
    fixture.write("Tsuzuri.toml", &manifest);
    server.change(&main, 2, fixed);
    server.settle();
    assert!(server.codes(&main).is_empty());
    assert_eq!(server.codes(&fixture.uri("Tsuzuri.toml")), ["E0002"]);
    // The range is the span of the CLI's `--json`, in the file's text.
    let error = tsuzuri::driver::Project::load(&fixture.base).unwrap_err();
    let span = error.diagnostic.span;
    assert!(span.start > 0, "{span:?}");
    let place = |offset: usize| {
        let prefix = &manifest[..offset];
        json!({"line": prefix.matches('\n').count(), "character": prefix.len() - prefix.rfind('\n').map_or(0, |at| at + 1)})
    };
    let published = server.published(&fixture.uri("Tsuzuri.toml"));
    assert_eq!(
        published.last().unwrap()[0]["range"],
        json!({"start": place(span.start), "end": place(span.end)})
    );
    fixture.write("Tsuzuri.toml", APP_MANIFEST);
    // `Main.old.tz` does not load (its stem has a '.'); opened and closed, it still gets the error.
    let old = fixture.uri("Main.old.tz");
    fixture.write("Main.old.tz", fixed);
    server.open(&old, fixed);
    server.settle();
    assert_eq!(server.codes(&old), ["E1011"]);
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": old}}),
    );
    server.change(&main, 3, broken);
    server.settle();
    assert_eq!(server.codes(&old), ["E1011"]);
    assert!(server.codes(&main).is_empty());
    server.finish();
}

/// G39 段 8: with every document of the root closed, a change on the disk is analyzed.
#[test]
fn disk_changes_are_analyzed_without_open_documents() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("Main.tz", main);
    fixture.write("Helper.tz", "fn offset() -> i64 { true }\n");
    let helper = fixture.uri("Helper.tz");
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    server.open(&fixture.uri("Main.tz"), main);
    server.settle();
    assert_eq!(server.codes(&helper), ["E1003"]);
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": fixture.uri("Main.tz")}}),
    );
    // The close analyzes the root once more; the disk changes after that.
    server.settle();
    fixture.write("Helper.tz", "fn offset() -> i64 { 2 }\n");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": helper, "type": 2}]}),
    );
    server.settle();
    assert!(server.codes(&helper).is_empty());
    server.finish();
}

/// G39 段 8・9: through a root that the client names by a symbolic link, the diagnostics of a file
/// that is not open go to the link's spelling, and a change in a subdirectory analyzes the root, not
/// the subdirectory.
#[cfg(unix)]
#[test]
fn a_linked_root_keeps_the_clients_spelling() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("real/Main.tz", main);
    fixture.write("real/Helper.tz", "fn offset() -> i64 { true }\n");
    // Analyzed with `Sub` as its root, `Helper` would be unknown (E1002).
    fixture.write("real/Sub/Util.tz", "fn util() -> i64 { Helper.offset() }\n");
    let link = fixture.base.join("link");
    std::os::unix::fs::symlink(fixture.path("real"), &link).unwrap();
    let mut server = live::Server::start(&fixture.uri_under(&link, ""), "utf-16");
    server.open(&fixture.uri_under(&link, "Main.tz"), main);
    server.settle();
    assert_eq!(
        server.codes(&fixture.uri_under(&link, "Helper.tz")),
        ["E1003"]
    );
    assert!(server.published(&fixture.uri("real/Helper.tz")).is_empty());
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": fixture.uri_under(&link, "Main.tz")}}),
    );
    server.settle();
    fixture.write("real/Helper.tz", "fn offset() -> i64 { 2 }\n");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": fixture.uri_under(&link, "Helper.tz"), "type": 2}, {"uri": fixture.uri_under(&link, "Sub/Util.tz"), "type": 2}]}),
    );
    // `workspace/symbol` lists the symbols of every analyzed root: D9 hides the diagnostics of a
    // root analyzed by mistake (`link` itself, or `link/Sub`), but not its symbols.
    let symbols = server.request("workspace/symbol", json!({"query": "util"}));
    let found: Vec<_> = symbols["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{symbols}"))
        .iter()
        .map(|symbol| {
            (
                symbol["name"].as_str().unwrap().to_owned(),
                symbol["location"]["uri"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [("util".to_owned(), fixture.uri_under(&link, "Sub/Util.tz"))]
    );
    assert!(
        server
            .codes(&fixture.uri_under(&link, "Helper.tz"))
            .is_empty()
    );
    for uri in [
        fixture.uri_under(&link, "Sub/Util.tz"),
        fixture.uri("real/Sub/Util.tz"),
    ] {
        assert!(
            server
                .published(&uri)
                .iter()
                .all(|diagnostics| diagnostics == &json!([])),
            "{uri}: {:?}",
            server.published(&uri)
        );
    }
    server.finish();
}

/// G39 段 8 (spec 9): a changed path is placed by its nearest existing ancestor directory, as
/// `document_path` places a document, not through the file itself: a source link created in the
/// root and pointing outside it analyzes the root, whose module discovery rejects it (E1011).
#[cfg(unix)]
#[test]
fn a_link_created_in_the_root_is_placed_by_its_directory() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("app/Main.tz", main);
    fixture.write("other/Ext.tz", "fn ext() -> i64 { 2 }\n");
    let mut server = live::Server::start(&fixture.uri("app"), "utf-16");
    server.open(&fixture.uri("app/Main.tz"), main);
    server.settle();
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": fixture.uri("app/Main.tz")}}),
    );
    // No open document and nothing published: only the changed path can name the root.
    server.settle();
    std::os::unix::fs::symlink(fixture.path("other/Ext.tz"), fixture.path("app/Ext.tz")).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": fixture.uri("app/Ext.tz"), "type": 1}]}),
    );
    server.settle();
    assert_eq!(server.codes(&fixture.uri("app/Ext.tz")), ["E1011"]);
    assert!(server.published(&fixture.uri("other/Ext.tz")).is_empty());
    server.finish();
}

/// G39 段 9: the files of a path dependency outside the root get no analysis diagnostics; the load
/// error of its manifest does reach the client.
#[test]
fn dependencies_outside_the_root_get_only_load_errors() {
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write(
        "app/Tsuzuri.toml",
        &format!("{APP_MANIFEST}\n[dependencies]\nlib = {{ path = \"../lib\" }}\n"),
    );
    fixture.write("app/Main.tz", main);
    let lib_manifest = "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n";
    fixture.write("lib/Tsuzuri.toml", lib_manifest);
    fixture.write("lib/Calc.tz", "fn calc() -> i64 { true }\n");
    let app = fixture.uri("app/Main.tz");
    let mut server = live::Server::start(&fixture.uri("app"), "utf-16");
    server.open(&app, main);
    server.settle();
    assert!(server.codes(&app).is_empty());
    assert!(
        server.published(&fixture.uri("lib/Calc.tz")).is_empty(),
        "{:?}",
        server.published(&fixture.uri("lib/Calc.tz"))
    );
    fixture.write("lib/Tsuzuri.toml", &format!("{lib_manifest}bogus\n"));
    server.change(&app, 2, main);
    server.settle();
    assert_eq!(server.codes(&fixture.uri("lib/Tsuzuri.toml")), ["E0002"]);
    server.finish();
}

/// Copilot review of PR #119: a client root that is removed is not analyzed through its parent
/// directory (which `Project::load_with_overlays` loads for a path that is not a directory): its
/// diagnostics are cleared, and nothing of the directories beside it is indexed or published.
#[test]
fn a_removed_root_is_not_loaded_through_its_parent() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("app/Main.tz", main);
    fixture.write("app/Helper.tz", "fn offset() -> i64 { true }\n");
    fixture.write("other/Other.tz", "fn beside_the_root() -> i64 { 2 }\n");
    let app = fixture.uri("app");
    let helper = fixture.uri("app/Helper.tz");
    let mut server = live::Server::start(&app, "utf-16");
    server.open(&fixture.uri("app/Main.tz"), main);
    server.settle();
    assert_eq!(server.codes(&helper), ["E1003"]);
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": fixture.uri("app/Main.tz")}}),
    );
    server.settle();
    let removed = server.seen.len();
    fixture.remove("app");
    // The root has diagnostics on the client, so the change analyzes it again.
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": app, "type": 3}]}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "beside"}));
    assert_eq!(symbols["result"], json!([]), "{symbols}");
    assert!(server.codes(&helper).is_empty());
    // A load error beside the root (a source whose stem has a '.') is not published either.
    fixture.write("Stray.old.tz", main);
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": fixture.uri("app/Main.tz"), "type": 3}]}),
    );
    server.settle();
    for message in &server.seen[removed..] {
        if message["method"] == "textDocument/publishDiagnostics" {
            let uri = message["params"]["uri"].as_str().unwrap();
            assert!(
                uri.starts_with(&format!("{app}/"))
                    && message["params"]["diagnostics"] == json!([]),
                "{message}"
            );
        }
    }
    server.finish();
}

/// Copilot review 2 of PR #119: a client root named through a symbolic link is resolved again when a
/// root is analyzed. Once the link is removed, the diagnostics sent at its spelling are cleared, not
/// sent there again for the directory it pointed to; once it points elsewhere, the files at its
/// spelling are those of the new directory.
#[cfg(unix)]
#[test]
fn a_removed_or_repointed_root_link_is_resolved_again() {
    use serde_json::json;
    let main = "fn value() -> i64 { 1 }\n";
    // A server whose root is `link` (to `real`), with the documents closed and an error in
    // `link/Helper.tz` on the client.
    let start = || {
        let fixture = live::Fixture::new();
        fixture.write("real/Main.tz", main);
        fixture.write("real/Helper.tz", "fn offset() -> i64 { true }\n");
        fixture.write("moved/Main.tz", main);
        fixture.write("moved/Helper.tz", "fn offset() -> i64 { missing }\n");
        let link = fixture.base.join("link");
        std::os::unix::fs::symlink(fixture.path("real"), &link).unwrap();
        let mut server = live::Server::start(&fixture.uri_under(&link, ""), "utf-16");
        server.open(&fixture.uri_under(&link, "Main.tz"), main);
        server.settle();
        assert_eq!(
            server.codes(&fixture.uri_under(&link, "Helper.tz")),
            ["E1003"]
        );
        server.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": fixture.uri_under(&link, "Main.tz")}}),
        );
        server.settle();
        (fixture, link, server)
    };
    // Removed.
    let (fixture, link, mut server) = start();
    let helper = fixture.uri_under(&link, "Helper.tz");
    std::fs::remove_file(&link).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": fixture.uri_under(&link, ""), "type": 3}]}),
    );
    server.settle();
    assert!(server.codes(&helper).is_empty());
    assert!(server.published(&fixture.uri("real/Helper.tz")).is_empty());
    server.finish();
    // Pointed elsewhere. The change notification resolves the root again and schedules the new
    // directory.
    let (fixture, link, mut server) = start();
    let helper = fixture.uri_under(&link, "Helper.tz");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(fixture.path("moved"), &link).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": helper, "type": 2}]}),
    );
    server.settle();
    assert_eq!(server.codes(&helper), ["E1002"]);
    for real in ["real/Helper.tz", "moved/Helper.tz"] {
        assert!(server.published(&fixture.uri(real)).is_empty(), "{real}");
    }
    server.finish();
}

/// Copilot review 3 of PR #119: two roots are due together, and the analysis of the first finds that
/// the second one's link is gone and drops the directory it pointed to. That directory is not then
/// analyzed from the list of due roots taken before: it would be loaded again, its symbols indexed
/// and its diagnostics sent outside every root that remains. (`workspace/symbol` and the debounce
/// timer take their lists alike and refresh them through the same function.)
#[cfg(unix)]
#[test]
fn a_root_dropped_by_an_earlier_analysis_is_not_analyzed() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    for (name, function) in [("a_real", "in_a"), ("b_real", "only_in_b")] {
        let helper = format!("fn {function}() -> i64 {{ let spare = 1; 2 }}\n");
        fixture.write(&format!("{name}/Main.tz"), main);
        fixture.write(&format!("{name}/Helper.tz"), &helper);
    }
    let a_link = fixture.base.join("a_link");
    let b_link = fixture.base.join("b_link");
    std::os::unix::fs::symlink(fixture.path("a_real"), &a_link).unwrap();
    std::os::unix::fs::symlink(fixture.path("b_real"), &b_link).unwrap();
    let folders = json!([
        {"uri": fixture.uri_under(&a_link, ""), "name": "a"},
        {"uri": fixture.uri_under(&b_link, ""), "name": "b"},
    ]);
    let mut server = live::Server::start_with(
        json!({"rootUri": null, "workspaceFolders": folders, "capabilities": {}}),
    );
    // `b`'s document is open at its directory's own spelling, so that it can still change once the
    // link is gone.
    let a_main = fixture.uri_under(&a_link, "Main.tz");
    let b_main = fixture.uri("b_real/Main.tz");
    server.open(&a_main, main);
    server.open(&b_main, main);
    server.settle();
    let b_helper = fixture.uri_under(&b_link, "Helper.tz");
    assert_eq!(server.codes(&b_helper), ["W1001"]);
    // `b`'s link goes, and a change of a document in each root schedules both again, with no change
    // notification (which resolves the roots before it schedules any).
    std::fs::remove_file(&b_link).unwrap();
    server.change(&a_main, 2, main);
    server.change(&b_main, 2, main);
    // `a_real` sorts first, and its analysis drops `b_real`, which was due as well.
    let symbols = server.request("workspace/symbol", json!({"query": "only_in_b"}));
    assert_eq!(symbols["result"], json!([]), "{symbols}");
    assert!(server.codes(&b_helper).is_empty());
    let unlinked = fixture.uri("b_real/Helper.tz");
    assert!(server.published(&unlinked).is_empty());
    server.finish();
}

/// Copilot review 3 of PR #119: a clean project named through a symbolic link, with its documents
/// closed, has no diagnostics on the client, so a change notification finds a root only by the
/// changed path. Once the link points elsewhere, that path resolves under the new directory: the
/// notification resolves the client roots again before it places the path, so the new directory is
/// analyzed and the old one dropped.
#[cfg(unix)]
#[test]
fn a_change_under_a_repointed_clean_root_link_analyzes_the_new_directory() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("real/Main.tz", main);
    fixture.write("real/Old.tz", "fn only_in_real() -> i64 { 2 }\n");
    let link = fixture.base.join("link");
    std::os::unix::fs::symlink(fixture.path("real"), &link).unwrap();
    let mut server = live::Server::start(&fixture.uri_under(&link, ""), "utf-16");
    let link_main = fixture.uri_under(&link, "Main.tz");
    server.open(&link_main, main);
    server.settle();
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": link_main}}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "only_in_real"}));
    assert_eq!(symbols["result"].as_array().unwrap().len(), 1, "{symbols}");
    let repointed = server.seen.len();
    // The link points to `moved`, where a new file has an error.
    fixture.write("moved/Main.tz", main);
    fixture.write("moved/Helper.tz", "fn offset() -> i64 { true }\n");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(fixture.path("moved"), &link).unwrap();
    let helper = fixture.uri_under(&link, "Helper.tz");
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": helper, "type": 1}]}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "only_in_real"}));
    assert_eq!(symbols["result"], json!([]), "{symbols}");
    assert_eq!(server.codes(&helper), ["E1003"]);
    let link_root = fixture.uri_under(&link, "");
    for message in &server.seen[repointed..] {
        if message["method"] == "textDocument/publishDiagnostics" {
            let uri = message["params"]["uri"].as_str().unwrap();
            assert!(uri.starts_with(&format!("{link_root}/")), "{message}");
        }
    }
    server.finish();
}

/// Copilot review 5 of PR #119: a root link that is removed and then made again resolves again with
/// no directory before it. With the documents closed, the change notification of the link alone
/// places the link beside its directory (spec 9 starts from the parent directory and does not follow
/// the link), and nothing is recorded or open: only the resolution of the root can schedule it, so
/// its diagnostics and its symbols come back. (The file not open has a warning: a project with an
/// error has no symbols.)
#[cfg(unix)]
#[test]
fn a_root_link_made_again_is_analyzed_again() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("real/Main.tz", main);
    fixture.write(
        "real/Helper.tz",
        "fn offset() -> i64 { let spare = 1; 2 }\n",
    );
    let link = fixture.base.join("link");
    std::os::unix::fs::symlink(fixture.path("real"), &link).unwrap();
    let root = fixture.uri_under(&link, "");
    let link_main = fixture.uri_under(&link, "Main.tz");
    let helper = fixture.uri_under(&link, "Helper.tz");
    let mut server = live::Server::start(&root, "utf-16");
    server.open(&link_main, main);
    server.settle();
    assert_eq!(server.codes(&helper), ["W1001"]);
    server.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": link_main}}),
    );
    server.settle();
    // Removed: the diagnostics at the link's spelling are cleared.
    std::fs::remove_file(&link).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": root, "type": 3}]}),
    );
    let symbols = server.request("workspace/symbol", json!({"query": "offset"}));
    assert_eq!(symbols["result"], json!([]), "{symbols}");
    assert!(server.codes(&helper).is_empty());
    // Made again, and only the link itself is notified.
    std::os::unix::fs::symlink(fixture.path("real"), &link).unwrap();
    server.notify(
        "workspace/didChangeWatchedFiles",
        json!({"changes": [{"uri": root, "type": 1}]}),
    );
    server.settle();
    assert_eq!(server.codes(&helper), ["W1001"]);
    let symbols = server.request("workspace/symbol", json!({"query": "offset"}));
    let found = symbols["result"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{symbols}");
    assert_eq!(found[0]["location"]["uri"], json!(helper), "{symbols}");
    assert!(server.published(&fixture.uri("real/Helper.tz")).is_empty());
    server.finish();
}

/// Copilot review of PR #119: two roots of one server that depend on the same broken manifest both
/// report its load error. When one of them drops the dependency, the error stays: the client
/// replaces a URI's whole list, and the other root still reports it there.
#[test]
fn a_load_error_that_another_root_reports_stays() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    let manifest = |name: &str, dependency: bool| {
        let mut text = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
        if dependency {
            text.push_str("\n[dependencies]\nlib = { path = \"../lib\" }\n");
        }
        text
    };
    for name in ["one", "two"] {
        fixture.write(&format!("{name}/Tsuzuri.toml"), &manifest(name, true));
        fixture.write(&format!("{name}/Main.tz"), main);
    }
    fixture.write(
        "lib/Tsuzuri.toml",
        &format!("{}bogus\n", manifest("lib", false)),
    );
    fixture.write("lib/Calc.tz", "fn calc() -> i64 { 2 }\n");
    let lib = fixture.uri("lib/Tsuzuri.toml");
    let folders: Vec<_> = ["one", "two"]
        .iter()
        .map(|name| json!({"uri": fixture.uri(name), "name": name}))
        .collect();
    let mut server = live::Server::start_with(
        json!({"rootUri": null, "workspaceFolders": folders, "capabilities": {}}),
    );
    for name in ["one", "two"] {
        server.open(&fixture.uri(&format!("{name}/Main.tz")), main);
    }
    server.settle();
    assert_eq!(server.codes(&lib), ["E0002"]);
    // `two` drops the dependency. Both roots have diagnostics on the client, so both are analyzed
    // again, `one` first: `two` would then clear the error that `one` has just reported.
    let drop_dependency = |server: &mut live::Server, name: &str| {
        fixture.write(&format!("{name}/Tsuzuri.toml"), &manifest(name, false));
        server.notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": fixture.uri(&format!("{name}/Tsuzuri.toml")), "type": 2}]}),
        );
        server.settle();
    };
    drop_dependency(&mut server, "two");
    assert_eq!(server.codes(&lib), ["E0002"]);
    // Once no root reports it, it is cleared.
    drop_dependency(&mut server, "one");
    assert!(server.codes(&lib).is_empty());
    server.finish();
}

/// Opens the files of `files` in a fresh root, one message at a time.
fn live_session(files: &[(&str, &str)]) -> (live::Fixture, live::Server) {
    let fixture = live::Fixture::new();
    for (name, text) in files {
        fixture.write(name, text);
    }
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    for (name, text) in files {
        server.open(&fixture.uri(name), text);
    }
    (fixture, server)
}

/// Sends a request without waiting for its response and returns its id.
fn ask(server: &mut live::Server, method: &str, params: serde_json::Value) -> u64 {
    let id = server.id();
    server.send(serde_json::json!({"id": id, "method": method, "params": params}));
    id
}

/// G39 段 11: a request that the client cancels before the server starts it gets -32800, however many
/// messages wait before the cancellation (the reader no longer stops at eight).
#[test]
fn a_request_cancelled_behind_many_messages_is_not_computed() {
    use serde_json::json;
    let (fixture, mut server) = live_session(&P1);
    // The hover's refresh keeps the server busy while the rest arrives.
    let busy = ask(
        &mut server,
        "textDocument/hover",
        json!({"textDocument": {"uri": fixture.uri("Main.tz")}, "position": position(P1_MAIN, "read (read", "utf-16")}),
    );
    let action = ask(
        &mut server,
        "textDocument/codeAction",
        json!({"textDocument": {"uri": fixture.uri("Warn.tz")}, "range": range(0, 0, 37), "context": {"diagnostics": []}}),
    );
    for _ in 0..10 {
        server.notify("initialized", json!({}));
    }
    server.notify("$/cancelRequest", json!({"id": action}));
    let hovered = server.response(busy);
    assert!(hovered.get("result").is_some(), "{hovered}");
    let response = server.response(action);
    assert_eq!(response["error"]["code"], -32800, "{response}");
    server.finish();
}

/// G39 段 11: cancellations of ids that no request has do not pile up and stop later cancellations
/// from taking effect.
#[test]
fn cancellations_of_unknown_ids_leave_room_for_later_ones() {
    use serde_json::json;
    let (fixture, mut server) = live_session(&P1);
    let main = fixture.uri("Main.tz");
    for ghost in 0..1100 {
        server.notify("$/cancelRequest", json!({"id": format!("ghost-{ghost}")}));
    }
    server.change(&main, 2, P1_MAIN);
    // The change makes the hover refresh first, so that it does not end before its cancellation.
    let hover = ask(
        &mut server,
        "textDocument/hover",
        json!({"textDocument": {"uri": main}, "position": position(P1_MAIN, "read (read", "utf-16")}),
    );
    server.notify("$/cancelRequest", json!({"id": hover}));
    let response = server.response(hover);
    assert_eq!(response["error"]["code"], -32800, "{response}");
    server.finish();
}

/// G39 段 11: JSON-RPC ids need be unique only among outstanding requests. A client that reuses the
/// id of an answered request can cancel the new request: the server forgets the answered id when a
/// request with it arrives, so the cancellation is not taken for a late one of the answered request.
/// A late cancellation of the answered request (one sent while its response was on the way) that
/// arrives just before the new request cancels neither the new request nor drops its cancellation.
#[test]
fn a_reused_request_id_can_be_cancelled() {
    use serde_json::json;
    let (fixture, mut server) = live_session(&P1);
    let main = fixture.uri("Main.tz");
    let hover = json!({"textDocument": {"uri": main}, "position": position(P1_MAIN, "read (read", "utf-16")});
    for id in [7, 8, 9] {
        server.send(json!({"id": id, "method": "textDocument/hover", "params": hover}));
        let first = server.response(id);
        assert!(first["result"]["contents"].is_object(), "{first}");
    }
    // The change makes the next hover refresh, so that the rest arrives while it is computed and
    // joins the queue at once.
    server.change(&main, 2, P1_MAIN);
    let busy = ask(&mut server, "textDocument/hover", hover);
    let symbols = |id: u64| json!({"id": id, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": main}}});
    // 7: reused and cancelled.
    server.send(symbols(7));
    server.notify("$/cancelRequest", json!({"id": 7}));
    // 8: a late cancellation of the answered 8, then 8 reused and cancelled.
    server.notify("$/cancelRequest", json!({"id": 8}));
    server.send(symbols(8));
    server.notify("$/cancelRequest", json!({"id": 8}));
    // 9: a late cancellation of the answered 9, then 9 reused and not cancelled.
    server.notify("$/cancelRequest", json!({"id": 9}));
    server.send(symbols(9));
    assert!(server.response(busy).get("result").is_some());
    let second = |server: &mut live::Server, id: u64| {
        server.wait(
            |message| {
                message.get("id") == Some(&json!(id))
                    && message.get("method").is_none()
                    && (message.get("error").is_some() || message["result"].is_array())
            },
            &format!("the response to the second request {id}"),
        )
    };
    for id in [7, 8] {
        let response = second(&mut server, id);
        assert_eq!(response["error"]["code"], -32800, "{response}");
    }
    let response = second(&mut server, 9);
    assert!(response["result"].is_array(), "{response}");
    server.finish();
}

/// G39 段 11: a request sees every text sync that arrived before it, of any document, and none that
/// arrived after it: the changes of a document are not merged across a request that is computed.
#[test]
fn a_request_sees_the_changes_that_arrived_before_it() {
    use serde_json::json;
    let shapes = |ty: &str| format!("def value :: {ty} = 1\n");
    let main = "def probe :: unit -> unit = \\_ -> { let _v = Shapes.value; () }\n";
    let first = shapes("i64");
    let (fixture, mut server) = live_session(&[("Main.tz", main), ("Shapes.tz", &first)]);
    let hover = json!({"textDocument": {"uri": fixture.uri("Main.tz")}, "position": position(main, "value;", "utf-16")});
    let busy = ask(&mut server, "textDocument/hover", hover.clone());
    server.change(&fixture.uri("Shapes.tz"), 2, &shapes("i32"));
    let between = ask(&mut server, "textDocument/hover", hover.clone());
    server.change(&fixture.uri("Shapes.tz"), 3, &shapes("i16"));
    let last = ask(&mut server, "textDocument/hover", hover);
    let detail = |server: &mut live::Server, id: u64| {
        let response = server.response(id);
        response["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{response}"))
            .to_owned()
    };
    assert!(detail(&mut server, busy).contains("i64"));
    let between = detail(&mut server, between);
    assert!(
        between.contains("i32") && !between.contains("i16"),
        "{between}"
    );
    assert!(detail(&mut server, last).contains("i16"));
    server.finish();
}

/// G39 段 11: a request about a document with a later change in the queue gets -32801, and the text
/// between is never analyzed: its diagnostics are not sent, those of the last text once.
#[test]
fn a_request_for_a_replaced_text_gets_content_modified() {
    use serde_json::json;
    let valid = "fn value() -> i64 { 1 }\n";
    let other = "fn other() -> i64 { 2 }\n";
    let (fixture, mut server) = live_session(&[("Main.tz", valid), ("Other.tz", other)]);
    let main = fixture.uri("Main.tz");
    let at = |text: &str| json!({"textDocument": {"uri": main}, "position": position(text, "value", "utf-16")});
    // A hover of another document of the root: computed, it analyzes the root while the rest arrives.
    let busy = ask(
        &mut server,
        "textDocument/hover",
        json!({"textDocument": {"uri": fixture.uri("Other.tz")}, "position": position(other, "other", "utf-16")}),
    );
    let mismatched = "fn value() -> i64 { true }\n";
    server.change(&main, 2, mismatched);
    let stale = ask(&mut server, "textDocument/hover", at(mismatched));
    server.change(&main, 3, "fn value() -> i64 { missing }\n");
    let symbols = ask(
        &mut server,
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": main}}),
    );
    assert!(server.response(busy).get("result").is_some());
    let response = server.response(stale);
    assert_eq!(response["error"]["code"], -32801, "{response}");
    let response = server.response(symbols);
    assert!(response["result"].is_array(), "{response}");
    let versions: Vec<_> = server
        .seen
        .iter()
        .filter(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == main.as_str()
        })
        .map(|message| message["params"]["version"].clone())
        .collect();
    assert!(!versions.contains(&json!(2)), "{versions:?}");
    assert_eq!(
        versions.iter().filter(|version| **version == 3).count(),
        1,
        "{versions:?}"
    );
    assert_eq!(server.codes(&main), ["E1002"]);
    server.finish();
}

/// Copilot review 2 of PR #119: a change that a later change of the same document replaces is still
/// applied, so that it stays when the later one is rejected (here for its size).
#[test]
fn a_change_before_a_rejected_one_is_kept() {
    use serde_json::json;
    let valid = "fn value() -> i64 { 1 }\n";
    let other = "fn other() -> i64 { 2 }\n";
    let (fixture, mut server) = live_session(&[("Main.tz", valid), ("Other.tz", other)]);
    let main = fixture.uri("Main.tz");
    // A hover of another document of the root: computed, it analyzes the root while the rest arrives.
    let busy = ask(
        &mut server,
        "textDocument/hover",
        json!({"textDocument": {"uri": fixture.uri("Other.tz")}, "position": position(other, "other", "utf-16")}),
    );
    server.change(&main, 2, "fn renamed() -> i64 { 1 }\n");
    let limit = tsuzuri::syntax::MAX_SOURCE_BYTES;
    server.change(&main, 3, &" ".repeat(limit + 1));
    let symbols = ask(
        &mut server,
        "textDocument/documentSymbol",
        json!({"textDocument": {"uri": main}}),
    );
    assert!(server.response(busy).get("result").is_some());
    let rejected = server.wait(
        |message| message["method"] == "window/logMessage",
        "the rejection of the third version",
    );
    assert_eq!(
        rejected["params"]["message"],
        format!("source exceeds the {limit}-byte limit")
    );
    let response = server.response(symbols);
    let names: Vec<_> = response["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["renamed"]);
    server.finish();
}

/// Copilot review 2 of PR #119: the changes of several documents are checked against the shared
/// limit of the open buffers in the order they arrive. A shrinks, B grows into the room that A left,
/// and A shrinks again: B's change fits, although it would not beside A's first text.
#[test]
fn changes_of_two_documents_meet_the_shared_limit_in_order() {
    use serde_json::json;
    let fixture = live::Fixture::new();
    let main = "fn value() -> i64 { 1 }\n";
    fixture.write("Main.tz", main);
    // The project does not load the sources under a directory whose name starts with '.', so the
    // large buffers cost the analysis nothing.
    std::fs::create_dir(fixture.path(".big")).unwrap();
    let mut server = live::Server::start(&fixture.uri(""), "utf-16");
    server.open(&fixture.uri("Main.tz"), main);
    // Eight buffers of the largest size would fill the 32 MiB: six, A, and Main.tz leave less room
    // than one more.
    let full = " ".repeat(tsuzuri::syntax::MAX_SOURCE_BYTES);
    for n in 0..6 {
        server.open(&fixture.uri(&format!(".big/Fill{n}.tz")), &full);
    }
    let (a, b) = (fixture.uri(".big/A.tz"), fixture.uri(".big/B.tz"));
    server.open(&a, &full);
    server.open(&b, "");
    // The first request analyzes the root while the rest arrives.
    let busy = ask(
        &mut server,
        "textDocument/hover",
        json!({"textDocument": {"uri": fixture.uri("Main.tz")}, "position": position(main, "value", "utf-16")}),
    );
    server.change(&a, 2, "  ");
    // Sent as a request, the notification's error would come back as a response.
    let grow = server.id();
    server.send(json!({"id": grow, "method": "textDocument/didChange", "params": {"textDocument": {"uri": b, "version": 2}, "contentChanges": [{"text": full}]}}));
    server.change(&a, 3, " ");
    assert!(server.response(busy).get("result").is_some());
    let response = server.response(grow);
    assert!(
        response.get("error").is_none() && response["result"].is_null(),
        "{response}"
    );
    server.finish();
}

/// G39 段 12: the light bulb's automatic request (`triggerKind` 2) without `only` gets no fix all,
/// which VS Code would drop; an invoked one and one that asks for source actions do.
#[test]
fn automatic_code_action_requests_skip_fix_all() {
    use serde_json::json;
    let responses = scripted(&P1, "utf-16", |uri| {
        let warn = uri("Warn.tz");
        let request = |id: u64, context: serde_json::Value| json!({"id": id, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": warn}, "range": range(0, 0, 37), "context": context}});
        vec![
            request(1, json!({"diagnostics": [], "triggerKind": 2})),
            request(2, json!({"diagnostics": [], "triggerKind": 1})),
            request(
                3,
                json!({"diagnostics": [], "triggerKind": 2, "only": ["source.fixAll"]}),
            ),
        ]
    });
    let kinds = |index: usize| -> Vec<String> {
        responses[index]["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{:?}", responses[index]))
            .iter()
            .map(|action| action["kind"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(kinds(0), ["quickfix"]);
    assert_eq!(kinds(1), ["quickfix", "source.fixAll.tsuzuri"]);
    assert_eq!(kinds(2), ["source.fixAll.tsuzuri"]);
}

/// E19: the quick fix of the `E1040` that rewrites an old `Channel.recv` given an end of a
/// DataChannel is offered (the counts of the codes follow the rewrite), with the spelling that
/// names the std `DataChannel` where the call is.
#[test]
fn the_channel_call_rewrite_is_a_quick_fix() {
    use serde_json::json;
    let body = "def f :: own std::DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match Channel.recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n";
    let own = "namespace App\n\ndef recv :: ref i64 -> i64\nfn recv value = deref value\n";
    for (files, expected) in [
        (vec![("Main.tz", body.to_owned())], "DataChannel.recv"),
        (
            vec![
                ("Main.tz", format!("namespace App\n\n{body}")),
                ("App/DataChannel.tz", own.to_owned()),
            ],
            "std::DataChannel.recv",
        ),
    ] {
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, text)| (*name, text.as_str()))
            .collect();
        let line = files[0]
            .1
            .lines()
            .position(|line| line.contains("Channel.recv"))
            .unwrap();
        let (messages, _) = session(&files, "utf-16", |uri| {
            let main = uri("Main.tz");
            vec![
                json!({"id": 1, "method": "textDocument/codeAction", "params": {"textDocument": {"uri": main}, "range": range(line, 0, 40), "context": {"diagnostics": [], "only": ["quickfix"]}}}),
            ]
        });
        let actions = messages
            .iter()
            .find(|message| message.get("id") == Some(&json!(1)))
            .expect("the code actions")["result"]
            .clone();
        let titles: Vec<&str> = actions
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|action| action["title"].as_str())
            .collect();
        assert!(
            titles.contains(&format!("write '{expected}'").as_str()),
            "{expected}: {actions}"
        );
        let edit = actions
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["title"] == format!("write '{expected}'"))
            .unwrap();
        let changes = edit["edit"]["changes"].as_object().unwrap();
        let new_text: Vec<&str> = changes
            .values()
            .flat_map(|edits| edits.as_array().unwrap())
            .filter_map(|edit| edit["newText"].as_str())
            .collect();
        assert_eq!(new_text, [expected], "{actions}");
    }
}
