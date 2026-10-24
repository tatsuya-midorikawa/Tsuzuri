use tsuzuri::{analyze, llvm};

#[test]
fn ordered_containers_are_opaque_noncopy_owned_values() {
    let source = "let map: Map<i64, string> = Map.empty()\nlet set = Set.singleton \"key\"\nMap.length (&map) + Set.length (&set)";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
    for source in [
        "let map: Map<i64, i64> = Map { entries: Vec.empty() }\n0",
        "let map = Map.singleton 1 2\nmap.entries.length",
        "let map = Map.singleton 1 2\nlet other = { map with entries = Vec.empty() }\nMap.length (&other)",
        "let map = Map.singleton 1 2\nmatch map with | Map { entries = storage } -> Vec.length (&storage)",
        "let set = Set.singleton 1\nmatch set with | { entries = storage } -> Vec.length (&storage)",
        "let map: Map<i64, i64> = Map { head: Vec.empty(), rest: Vec.empty(), length: 0 }\n0",
        "let map = Map.singleton 1 2\nmap.head.length",
        "let map = Map.singleton 1 2\nlet other = { map with length = 5 }\nMap.length (&other)",
        "let map = Map.singleton 1 2\nmatch map with | Map { rest = storage } -> Vec.length (&storage)",
        "let set = Set.singleton 1\nmatch set with | { head = storage } -> Vec.length (&storage)",
        "let set = Set.singleton 1\nset.rest.length",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1022", "{source}");
    }
    assert_eq!(
        analyze(
            "let map = Map.singleton 1 2\nlet other = map\nMap.length (&map) + Map.length (&other)"
        )
        .unwrap_err()
        .code,
        "E1012"
    );
    assert_eq!(
        analyze("export def bad :: Map<i64, i64>\nfn bad = Map.singleton 1 2")
            .unwrap_err()
            .code,
        "E1008"
    );
}

#[test]
fn ordered_lookup_updates_and_snapshots_use_only_required_constraints() {
    for source in [
        "let map = Map.insert \"a\" 1 (Map.singleton \"b\" 2)\nlet value = Map.at \"b\" (&map)\nassert (deref value == 2)\nMap.fold (\\total key value -> total + key.length + deref value) 0 (&map)",
        "let map = Map.insert 2 \"new\" (Map.singleton 2 \"old\")\nlet value = Map.at 2 (&map)\nvalue.length",
        "let map = Map.insert 1 10 (Map.singleton 2 20)\nlet keys = Map.keys (&map)\nlet values = Map.values (&map)\nlet pairs = Map.to_array (&map)\nlet map = Map.remove 2 map\nassert (Maybe.get (Map.get 1 (&map)) == 10)\nkeys[0] + values[0] + pairs.length",
        "let set = Set.remove \"a\" (Set.insert \"b\" (Set.singleton \"a\"))\nSet.fold (\\total key -> total + key.length) 0 (&set)",
        "record Key { name: string } deriving (Eq, Ord)\nlet map = Map.insert (Key { name: \"x\" }) 42 (Map.empty())\nMap.get (Key { name: \"x\" }) (&map)",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for (source, code) in [
        ("let map = Map.singleton 1 \"x\"\nMap.get 1 (&map)", "E1005"),
        ("let map = Map.singleton \"x\" 1\nMap.keys (&map)", "E1005"),
        (
            "let map = Map.singleton 1 2\nlet next = Map.insert 2 3 map\nMap.length (&map)",
            "E1012",
        ),
        (
            "let map = Map.singleton 1 \"x\"\nlet value = Map.at 1 (&map)\nlet next = Map.remove 1 map\nvalue.length",
            "E1014",
        ),
        (
            "fn bad() -> &string { let map = Map.singleton 1 \"x\"; Map.at 1 (&map) }",
            "E1013",
        ),
        (
            "record Key { value: i64 }\nlet map = Map.insert (Key { value: 1 }) 2 (Map.empty())\nMap.length (&map)",
            "E1005",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn set_algebra_and_borrowed_map_values_preserve_ownership() {
    assert_eq!(analyze("let union = 1\nunion").unwrap_err().code, "E0002");
    for source in [
        "let left = Set.insert 2 (Set.singleton 1)\nlet right = Set.insert 3 (Set.singleton 2)\nlet common = Set.intersect (&left) (&right)\nlet different = Set.difference (&left) (&right)\nlet both = Set.union left right\nSet.length (&common) + Set.length (&different) + Set.length (&both)",
        "let set = Set.union (Set.singleton \"x\") (Set.insert \"y\" (Set.singleton \"x\"))\nSet.fold (\\total key -> total + key.length) 0 (&set)",
        "let text = \"borrowed\"\nlet map = Map.singleton 1 (ref text)\nlet value = Map.at 1 (&map)\nvalue.length",
        "let map = Map.singleton 1 (task { return 42 })\nlet map = Map.remove 1 map\nMap.length (&map)",
    ] {
        let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for (source, code) in [
        (
            "let left = Set.singleton \"x\"\nlet right = Set.singleton \"y\"\nSet.intersect (&left) (&right)",
            "E1005",
        ),
        (
            "fn bad() -> Map<i64, ref string> { let text = \"borrowed\"; Map.singleton 1 (ref text) }",
            "E1013",
        ),
        (
            "let mut value = 1\nlet map = Map.singleton 0 (ref mut value)\nMap.length (&map)",
            "E1013",
        ),
        ("record Wrapped { map: Map<i64, ref mut string> }", "E1013"),
        (
            "record Holder<'a> { value: 'a }\nfn bad(map: Map<i64, ref mut string>) -> Holder<Map<i64, ref mut string>> { Holder { value: map } }",
            "E1013",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}
