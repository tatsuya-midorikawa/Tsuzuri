use tsuzuri::{analyze_modules, analyze_modules_with_semantics, llvm};

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
