use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        assert!(llvm::duplicate_declarations(&ir).is_empty(), "{source}");
    }
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn hash_containers_are_opaque_noncopy_owned_values() {
    accepts(
        "let map: HashMap<i64, string> = HashMap.empty()\nlet set = HashSet.insert \"key\" (HashSet.empty())\nHashMap.length (&map) + HashSet.length (&set)",
    );
    for source in [
        "let map: HashMap<i64, i64> = HashMap { entries: Vec.empty(), slots: Vec.empty() }\n0",
        "let map = HashMap.insert 1 2 (HashMap.empty())\nmap.entries.length",
        "let map = HashMap.insert 1 2 (HashMap.empty())\nlet other = { map with slots = Vec.empty() }\nHashMap.length (&other)",
        "let map = HashMap.insert 1 2 (HashMap.empty())\nmatch map with | HashMap { entries = storage } -> Vec.length (&storage)",
        "let set = HashSet.insert 1 (HashSet.empty())\nmatch set with | { map = inner } -> HashMap.length (&inner)",
        "let entry = HashMap.Entry { hash: 0, key: 1, value: 2 }\n0",
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nmap.key0",
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nmap.keyed",
        "let map: HashMap<i64, i64> = HashMap { entries: Vec.empty(), slots: Vec.empty(), key0: 0i64u, key1: 0i64u, keyed: false }\n0",
    ] {
        rejects(source, "E1022");
    }
    rejects(
        "let map = HashMap.insert 1 2 (HashMap.empty())\nlet other = map\nHashMap.length (&map) + HashMap.length (&other)",
        "E1012",
    );
    rejects(
        "export def bad :: HashMap<i64, i64>\nfn bad = HashMap.empty()",
        "E1008",
    );
    rejects(
        "export def bad :: HashSet<i64>\nfn bad = HashSet.empty()",
        "E1008",
    );
}

#[test]
fn hash_module_names_are_reserved() {
    for name in ["HashMap", "HashSet"] {
        let error = analyze_modules(&[(name, "")]).unwrap_err();
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
        assert!(error.message.contains("reserved"), "{}", error.message);
    }
}

#[test]
fn hash_lookup_and_updates_use_only_required_constraints() {
    for source in [
        "let map = HashMap.insert \"a\" 1 (HashMap.insert \"b\" 2 (HashMap.empty()))\nlet value = HashMap.at \"b\" (&map)\nassert (deref value == 2)\nHashMap.fold (\\total key value -> total + key.length + deref value) 0 (&map)",
        "let map = HashMap.insert 2 \"new\" (HashMap.insert 2 \"old\" (HashMap.empty()))\nlet value = HashMap.at 2 (&map)\nvalue.length",
        "let map = HashMap.insert 1 10 (HashMap.insert 2 20 (HashMap.empty()))\nlet keys = HashMap.keys (&map)\nlet values = HashMap.values (&map)\nlet pairs = HashMap.to_array (&map)\nlet map = HashMap.remove 2 map\nassert (Maybe.get (HashMap.get 1 (&map)) == 10)\nkeys[0] + values[0] + pairs.length",
        "record Key { name: string } deriving (Eq, Hash)\nlet map = HashMap.insert (Key { name: \"x\" }) 42 (HashMap.empty())\nHashMap.get (Key { name: \"x\" }) (&map)",
        "let map = HashMap.remove 1 (HashMap.insert 1 2 (HashMap.empty()))\nHashMap.contains_key 1 (&map)",
        "let empty: HashMap<i64, string> = HashMap.with_capacity 0\nlet sized: HashMap<i64, string> = HashMap.with_capacity 10\nHashMap.length (&empty) + HashMap.length (&sized)",
    ] {
        accepts(source);
    }
    for source in [
        "let set = HashSet.remove \"a\" (HashSet.insert \"b\" (HashSet.insert \"a\" (HashSet.empty())))\nHashSet.fold (\\total key -> total + key.length) 0 (&set)",
        "let set = HashSet.insert 7 (HashSet.empty())\nlet values = HashSet.to_array (&set)\nlet mut total = 0\nfor key in HashSet.iter (&set) do total = total + deref key\ntotal + values[0] + (if HashSet.contains 7 (&set) then 1 else 0) + (if HashSet.is_empty (&set) then 1 else 0)",
        "let text = \"borrowed\"\nlet map = HashMap.insert 1 (ref text) (HashMap.empty())\nlet value = HashMap.at 1 (&map)\nvalue.length",
        "let map = HashMap.insert 1 (task { return 42 }) (HashMap.empty())\nlet map = HashMap.remove 1 map\nHashMap.length (&map)",
        "let map = HashMap.insert 1 2 (HashMap.empty())\nlet total = HashMap.fold (\\sum key value -> sum + deref key + deref value) 0 (&map)\nlet mut count = 0\nfor pair in HashMap.iter (&map) do count = count + 1\ntotal + count",
    ] {
        accepts(source);
    }
    for (source, code) in [
        (
            "record Key { name: string } deriving (Eq)\nlet map = HashMap.insert (Key { name: \"x\" }) 1 (HashMap.empty())\nHashMap.length (&map)",
            "E1005",
        ),
        (
            "record Key { name: string } deriving (Hash)\nlet map = HashMap.insert (Key { name: \"x\" }) 1 (HashMap.empty())\nHashMap.length (&map)",
            "E1005",
        ),
        (
            "let map = HashMap.insert 1 \"x\" (HashMap.empty())\nHashMap.get 1 (&map)",
            "E1005",
        ),
        (
            "let map = HashMap.insert \"x\" 1 (HashMap.empty())\nHashMap.keys (&map)",
            "E1005",
        ),
        (
            "let set = HashSet.insert \"x\" (HashSet.empty())\nHashSet.to_array (&set)",
            "E1005",
        ),
        (
            "let map = HashMap.insert 1 \"x\" (HashMap.empty())\nlet value = HashMap.at 1 (&map)\nlet next = HashMap.remove 1 map\nvalue.length",
            "E1014",
        ),
        (
            "fn bad() -> &string { let map = HashMap.insert 1 \"x\" (HashMap.empty()); HashMap.at 1 (&map) }",
            "E1013",
        ),
        (
            "let mut value = 1\nlet map = HashMap.insert 0 (ref mut value) (HashMap.empty())\nHashMap.length (&map)",
            "E1013",
        ),
        (
            "record Wrapped { map: HashMap<i64, ref mut string> }",
            "E1013",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn seeded_hash_containers_and_borrowed_keys_type_check() {
    for source in [
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nlet map = HashMap.insert 1 2 map\nHashMap.length (&map) + HashMap.longest_probe (&map)",
        "let map: HashMap<i64, i64> = HashMap.with_capacity_and_seed 10 3i64u\nlet map = HashMap.insert 1 2 map\nHashMap.length (&map)",
        "let map: HashMap<string, i64> = HashMap.with_seed 1i64u\nlet map = HashMap.insert \"k\" 1 map\nlet key = \"k\"\nlet found = HashMap.contains_key_ref (&key) (&map)\nlet value = HashMap.get_ref (&key) (&map)\nlet stored = deref (HashMap.at_ref (&key) (&map))\nlet map = HashMap.remove_ref (&key) map\nHashMap.length (&map) + stored + Maybe.get value + (if found then 1 else 0) + key.length",
        "let set: HashSet<string> = HashSet.with_seed 1i64u\nlet set = HashSet.insert \"a\" set\nlet key = \"a\"\nlet present = HashSet.contains_ref (&key) (&set)\nlet set = HashSet.remove_ref (&key) set\nHashSet.longest_probe (&set) + HashSet.length (&set) + (if present then 1 else 0)",
        "let set: HashSet<i64> = HashSet.with_capacity_and_seed 4 2i64u\nHashSet.length (&set)",
        "let word: i64u = HashMap.sip13 1i64u 2i64u 3i64u\nword",
        "def lookup {r} :: ref {r} HashMap<string, string> -> string -> ref {r} string\nfn lookup map key = HashMap.at key map\nlet map = HashMap.insert \"k\" \"v\" (HashMap.empty())\n(lookup (&map) \"k\").length",
    ] {
        accepts(source);
    }
    for (source, code) in [
        (
            "let map = HashMap.insert \"k\" \"v\" (HashMap.empty())\nlet key = \"k\"\nlet value = HashMap.at_ref (&key) (&map)\nlet next = HashMap.remove_ref (&key) map\nvalue.length",
            "E1014",
        ),
        (
            "let seed: i64 = 7\nlet map: HashMap<i64, i64> = HashMap.with_seed seed\nHashMap.length (&map)",
            "E1003",
        ),
        (
            "record Key { name: string } deriving (Eq)\nlet set: HashSet<Key> = HashSet.with_seed 1i64u\nlet key = Key { name: \"x\" }\nHashSet.contains_ref (&key) (&set)",
            "E1005",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn unused_hash_containers_emit_no_code() {
    let baseline = "let mut map: Map<i64, i64> = Map.empty()\nmap = Map.insert 2 20 map\nmap = Map.insert 1 10 map\nMap.fold (\\total key value -> total * 100 + deref key + deref value) 0 (&map)";
    let module = analyze(baseline).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(!ir.contains("HashMap") && !ir.contains("HashSet"), "{ir}");
    }
    let used =
        analyze("let map = HashMap.insert 1 2 (HashMap.empty())\nHashMap.length (&map)").unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&used, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("@tz.fn.HashMap.insert"), "{ir}");
        assert!(!ir.contains("@tz.fn.HashSet."), "{ir}");
    }
}
