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
    let responses = scripted(&P1, "utf-16", |uri| {
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
    let source =
        "def work :: i64 -> Async<i64>\nfn work n = Async { do! Async.yield (); return n }\n";
    let responses = scripted(&[("Main.tz", source)], "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id":1,"method":"textDocument/hover","params":{"textDocument":{"uri":main},"position":position(source,"yield", "utf-16")}}),
            json!({"id":2,"method":"textDocument/signatureHelp","params":{"textDocument":{"uri":main},"position":position(source,");", "utf-16")}}),
            json!({"id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":main},"position":position(source," ();", "utf-16")}}),
        ]
    });
    assert!(
        responses[0]["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Async<unit>"),
        "{responses:?}"
    );
    let label = responses[1]["result"]["signatures"][0]["label"]
        .as_str()
        .unwrap();
    assert!(
        label.starts_with("yield ") && label.contains("Async<unit>"),
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
    let responses = scripted(
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
        json!([{"range": range(0, 23, 28), "severity": 2, "code": "W1001", "source": "tsuzuri", "message": "unused local 'spare'; prefix it with '_' to silence this warning"}])
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
    let text = "record Bag { items: [i64] }\ndef total :: Bag -> i64\nfn total bag =\n    let xs = bag.items\n    let ys = bag.items\n    Array.length (ref xs) + Array.length (ref ys)\ndef reuse :: [i64] -> i64\nfn reuse a =\n    let b = a\n    let moved = [1]\n    let n = 2\n    let m = n\n    Array.length (ref a) + Array.length (ref b) + Array.length (ref moved) + m\n";
    let broken = format!("{text}fn (");
    let responses = scripted(&[("Main.tz", text)], "utf-16", |uri| {
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
            "codeActionProvider": {"codeActionKinds": ["quickfix"]},
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
    let responses = scripted(&files, "utf-16", |uri| {
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
    let responses = scripted(&files, "utf-16", |uri| {
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
    let responses = scripted(&[("Main.tz", valid)], "utf-16", |uri| {
        let main = uri("Main.tz");
        vec![
            json!({"id": 1, "method": "textDocument/hover", "params": {"textDocument": {"uri": main}, "position": position(valid, "rows", "utf-16")}}),
            json!({"method": "textDocument/didChange", "params": {"textDocument": {"uri": main, "version": 2}, "contentChanges": [{"text": incomplete}]}}),
            json!({"id": 2, "method": "textDocument/completion", "params": {"textDocument": {"uri": main}, "position": {"line": 1, "character": 15}}}),
        ]
    });
    assert_eq!(
        responses[0]["result"]["contents"]["value"],
        json!("```tsuzuri\nref Matrix<i64> -> i64\n```"),
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
    ] {
        assert!(labels.contains(&member), "{member}: {labels:?}");
    }
}
