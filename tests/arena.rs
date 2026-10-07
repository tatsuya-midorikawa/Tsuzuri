use tsuzuri::{analyze, llvm};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.message
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
        ir
    })
}

const NODE: &str = "record Node { value: i64, edges: Vec<Arena.Handle<Node>> }\n\
def link :: Arena.Handle<Node> -> Node -> Node\n\
fn link target node =\n    match node with\n    | Node { value = value, edges = edges } -> Node { value: value, edges: Vec.push edges target }\n";

#[test]
fn arena_id_primitive_is_private() {
    for source in [
        "Arena.__next_id()",
        "def f :: i64\nfn f = Arena.__next_id()\nf()",
        "let next = Arena.__next_id\nnext()",
    ] {
        let message = rejects(source, "E1022");
        assert!(
            message.contains("private to the standard Arena module"),
            "{message}"
        );
    }
}

#[test]
fn phantom_parameters_are_std_opaque_only() {
    let message = rejects("record Tag<'a> { id: i64 }\n0", "E1024");
    assert!(message.contains("is not used by any field"), "{message}");
    rejects(
        "record Tag<'a> { id: i64 }\nrecord Handle<'b> { tag: Tag<'b> }\n0",
        "E1024",
    );
    emits(&format!(
        "{NODE}let graph: Arena<Node> = Arena.empty()\nmatch Arena.insert graph (Node {{ value: 1, edges: Vec.empty() }}) with\n| (next, handle) -> Arena.length (ref next) + (Arena.at (ref next) handle).value"
    ));
    rejects("record Bad { value: i64, edges: Vec<Bad> }\n0", "E1010");
}

#[test]
fn arena_types_are_opaque_noncopy_owned_values() {
    for source in [
        "let arena: Arena<i64> = Arena { id: 0, values: Vec.empty(), owners: Vec.empty(), slots: Vec.empty(), free: -1 }\n0",
        "let arena: Arena<i64> = Arena.empty()\narena.values.length",
        "let arena: Arena<i64> = Arena.empty()\nlet other = { arena with free = 0 }\n0",
        "let arena: Arena<i64> = Arena.empty()\nmatch arena with\n| Arena { id = id, values = values, owners = owners, slots = slots, free = free } -> free",
        "let handle: Arena.Handle<i64> = Arena.Handle { arena: 0, index: 0, generation: 0 }\n0",
        "let arena: Arena<i64> = Arena.empty()\nmatch Arena.insert arena 5 with\n| (next, handle) -> handle.index",
        "let arena: Arena<i64> = Arena.empty()\nmatch Arena.insert arena 5 with\n| (next, Arena.Handle { arena = id, index = index, generation = generation }) -> index",
    ] {
        let message = rejects(source, "E1022");
        assert!(message.contains("is opaque"), "{source}\n{message}");
    }
    rejects(
        "let arena: Arena<i64> = Arena.empty()\nlet other = arena\nArena.length (ref arena) + Arena.length (ref other)",
        "E1012",
    );
    rejects(
        "export def bad :: Arena<i64>\nfn bad = Arena.empty()",
        "E1008",
    );
    rejects("const bad: Arena<i64> = Arena.empty()\n0", "E1026");
    let error = tsuzuri::analyze_modules(&[("Arena", "")]).unwrap_err();
    assert_eq!(error.code, "E1011", "{}", error.message);
    let source = "let first: Arena<i64> = Arena.empty()\nlet second: Arena<string> = Arena.with_capacity 2\nmatch Arena.insert first 1 with\n| (left, handle) ->\n    match Arena.insert second \"x\" with\n    | (right, other) -> Arena.length (ref left) + Arena.length (ref right) + (if Arena.contains (ref right) other then 1 else 0)";
    emits(source);
}

#[test]
fn handles_are_typed_copy_and_send() {
    emits(
        "let arena: Arena<string> = Arena.empty()\nmatch Arena.insert arena \"x\" with\n| (filled, handle) ->\n    let copy = handle\n    let checked = Task.run (task { return copy == handle })\n    assert (checked && Arena.contains (ref filled) copy && Arena.contains (ref filled) handle)\n    Arena.length (ref filled)",
    );
    emits(
        "let arena: Arena<ref string> = Arena.empty()\nlet text = \"borrowed\"\nmatch Arena.insert arena (ref text) with\n| (filled, handle) -> Task.run (task { return handle == handle })",
    );
    rejects(
        "let numbers: Arena<i64> = Arena.empty()\nlet words: Arena<string> = Arena.empty()\nmatch Arena.insert words \"x\" with\n| (filled, handle) -> Arena.get (ref numbers) handle",
        "E1003",
    );
}

#[test]
fn arena_borrows_follow_ownership_rules() {
    let message = rejects(
        "let empty: Arena<string> = Arena.empty()\nmatch Arena.insert empty \"x\" with\n| (filled, handle) ->\n    let text = Arena.at (ref filled) handle\n    let more = Arena.insert filled \"y\"\n    text.length",
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );
    let message = rejects(
        "let arena: Arena<string> = Arena.empty()\nlet view = ref arena\nTask.run (task { return Arena.length view })",
        "E1013",
    );
    assert!(
        message.starts_with("tasks require owned values"),
        "{message}"
    );
    let message = rejects(
        "let arena: Arena<i64> = Arena.empty()\nlet moved = Arena.insert arena 5\nArena.length (ref arena)",
        "E1012",
    );
    assert!(
        message.starts_with("use of moved or partially moved value 'arena'"),
        "{message}"
    );
}

#[test]
fn arena_ir_has_one_atomic_counter() {
    let source = "let first: Arena<i64> = Arena.empty()\nlet second: Arena<string> = Arena.with_capacity 4\nArena.length (ref first) + Arena.length (ref second)";
    for ir in emits(source) {
        assert_eq!(
            ir.matches("@tz.arena.next_id = internal global i64 0")
                .count(),
            1
        );
        assert_eq!(
            ir.matches("define internal i64 @tz.builtin.Arena.__next_id()")
                .count(),
            1
        );
        assert!(ir.contains("atomicrmw add ptr @tz.arena.next_id, i64 1 monotonic"));
        assert_eq!(ir.matches("atomicrmw").count(), 1);
        assert!(ir.matches("declare void @llvm.trap").count() <= 1);
    }
    for ir in emits("let map = Map.singleton 1 2\nMap.length (ref map)") {
        assert!(!ir.contains("tz.arena"));
        assert!(!ir.contains("atomicrmw"));
    }
}

#[test]
fn arena_api_programs_emit_for_both_targets() {
    for source in [
        "let arena: Arena<string> = Arena.with_capacity 2\nmatch Arena.insert arena \"a\" with\n| (filled, a) ->\n    match Arena.insert filled \"bc\" with\n    | (more, b) ->\n        let first = Arena.get (ref more) a\n        let total = (match first with | Maybe.Some text -> text.length | Maybe.None -> 0) + (Arena.at (ref more) b).length\n        match Arena.remove more a with\n        | (removed, value) ->\n            match Arena.insert removed \"def\" with\n            | (reused, c) -> total + Arena.length (ref reused) + (if Arena.contains (ref reused) a then 100 else 0) + (if c == a then 1000 else 0) + (match value with | Maybe.Some text -> text.length | Maybe.None -> 0)",
        "let arena: Arena<i64> = Arena.empty()\nmatch Arena.insert arena 1 with\n| (filled, handle) ->\n    let updated = Arena.update filled handle (\\value -> value + 41)\n    let mut total = 0\n    for (key, value) in Arena.iter (ref updated) do\n        if key == handle then total = total + deref value\n    total",
        &format!(
            "{NODE}def cycle :: i64\nfn cycle =\n    let empty: Arena<Node> = Arena.empty()\n    match Arena.insert empty (Node {{ value: 1, edges: Vec.empty() }}) with\n    | (with_a, a) ->\n        match Arena.insert with_a (Node {{ value: 20, edges: Vec.push (Vec.empty()) a }}) with\n        | (with_b, b) ->\n            let graph = Arena.update with_b a (link b)\n            let first = Arena.at (ref graph) a\n            let second = Arena.at (ref graph) first.edges[0]\n            let back = Arena.at (ref graph) second.edges[0]\n            let mut total = 0\n            for (handle, node) in Arena.iter (ref graph) do\n                total = total + node.value\n                if handle == a then total = total + 100\n            first.value + second.value + back.value + total\ncycle()"
        ),
        "let arena: Arena<i64> = Arena.empty()\nmatch Arena.insert arena 1 with\n| (filled, handle) ->\n    match Arena.remove filled handle with\n    | (removed, _value) ->\n        let updated = Arena.update removed handle (\\value -> value + 1)\n        Arena.length (ref updated)",
        "let keys: Map<Arena.Handle<i64>, i64> = Map.empty()\nlet arena: Arena<i64> = Arena.empty()\nmatch Arena.insert arena 7 with\n| (filled, handle) ->\n    let map = Map.insert keys handle 3\n    let table = HashMap.insert (HashMap.empty()) handle 4\n    Maybe.get (Map.get (ref map) handle) + Maybe.get (HashMap.get (ref table) handle) + Arena.length (ref filled)",
    ] {
        emits(source);
    }
}

/// Two copies of one arena, made by copying a function value that captured it, keep its id.
const SNAPSHOTS: &str = "def without :: Arena<i64> -> Arena.Handle<i64> -> Arena<i64>
fn without arena handle =
    match Arena.remove arena handle with
    | (next, _value) -> next

def snapshots :: i64
fn snapshots =
    let mut base: Arena<i64> = Arena.empty()
    let mut handles: Vec<Arena.Handle<i64>> = Vec.empty()
    let mut index = 0
    while index < 5 do
        match Arena.insert base ((index + 1) * 10) with
        | (next, handle) ->
            base = next
            handles = Vec.push handles handle
        index = index + 1
    let frozen = base
    let snapshot = \\() -> frozen
    let first = without (without (snapshot ()) handles[1]) handles[0]
    match Arena.insert (without (snapshot ()) handles[0]) 99 with
    | (second, key) ->
        let mut flags = 0
        if Arena.contains (ref first) key then flags = flags + 1000
        match Arena.get (ref first) key with
        | Maybe.Some _value -> flags = flags + 200
        | Maybe.None -> flags = flags + 100
        match Arena.remove first key with
        | (kept, removed) ->
            match removed with
            | Maybe.Some _value -> flags = flags + 20
            | Maybe.None -> flags = flags + 10
            if !(Arena.contains (ref second) handles[0]) && deref (Arena.at (ref second) key) == 99 then flags = flags + 1
            let mut refilled = kept
            let mut value = 7
            while value < 10 do
                match Arena.insert refilled value with
                | (next, _handle) -> refilled = next
                value = value + 1
            let mut digits = 0
            for (_handle, item) in Arena.iter (ref refilled) do digits = digits * 100 + deref item
            digits * 10000 + flags

snapshots()
";

#[test]
fn handles_never_match_free_slots_of_another_copy() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    emits(SNAPSHOTS);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-arena-snapshots-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Main.tz"), SNAPSHOTS).unwrap();
    for optimization in ["-O0", "-O3"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("run")
            .arg(&root)
            .arg(optimization)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // The second copy's handle names slot 0, which is free in the first copy: not found
        // there (100), not removed (10), and the first copy stays consistent: it keeps 40, 50, 30
        // and refills with 7, 8, 9. Matching the free slot gave 40300708091221.
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "4050300708090111"
        );
    }
    fs::remove_dir_all(&root).unwrap();
}
