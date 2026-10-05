use std::fmt::Write;

use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        let declarations: Vec<_> = ir
            .lines()
            .filter(|line| line.starts_with("declare "))
            .collect();
        let unique: std::collections::BTreeSet<_> = declarations.iter().collect();
        assert_eq!(unique.len(), declarations.len(), "{source}");
    }
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn hash_containers_are_opaque_noncopy_owned_values() {
    accepts(
        "let map: HashMap<i64, string> = HashMap.empty()\nlet set = HashSet.insert (HashSet.empty()) \"key\"\nHashMap.length (&map) + HashSet.length (&set)",
    );
    for source in [
        "let map: HashMap<i64, i64> = HashMap { entries: Vec.empty(), slots: Vec.empty() }\n0",
        "let map = HashMap.insert (HashMap.empty()) 1 2\nmap.entries.length",
        "let map = HashMap.insert (HashMap.empty()) 1 2\nlet other = { map with slots = Vec.empty() }\nHashMap.length (&other)",
        "let map = HashMap.insert (HashMap.empty()) 1 2\nmatch map with | HashMap { entries = storage } -> Vec.length (&storage)",
        "let set = HashSet.insert (HashSet.empty()) 1\nmatch set with | { map = inner } -> HashMap.length (&inner)",
        "let entry = HashMap.Entry { hash: 0, key: 1, value: 2 }\n0",
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nmap.key0",
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nmap.keyed",
        "let map: HashMap<i64, i64> = HashMap { entries: Vec.empty(), slots: Vec.empty(), key0: 0i64u, key1: 0i64u, keyed: false }\n0",
    ] {
        rejects(source, "E1022");
    }
    rejects(
        "let map = HashMap.insert (HashMap.empty()) 1 2\nlet other = map\nHashMap.length (&map) + HashMap.length (&other)",
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
        "let map = HashMap.insert (HashMap.insert (HashMap.empty()) \"b\" 2) \"a\" 1\nlet value = HashMap.at (&map) \"b\"\nassert (deref value == 2)\nHashMap.fold (\\total key value -> total + key.length + deref value) 0 (&map)",
        "let map = HashMap.insert (HashMap.insert (HashMap.empty()) 2 \"old\") 2 \"new\"\nlet value = HashMap.at (&map) 2\nvalue.length",
        "let map = HashMap.insert (HashMap.insert (HashMap.empty()) 2 20) 1 10\nlet keys = HashMap.keys (&map)\nlet values = HashMap.values (&map)\nlet pairs = HashMap.to_array (&map)\nlet map = HashMap.remove map 2\nassert (Maybe.get (HashMap.get (&map) 1) == 10)\nkeys[0] + values[0] + pairs.length",
        "record Key { name: string } deriving (Eq, Hash)\nlet map = HashMap.insert (HashMap.empty()) (Key { name: \"x\" }) 42\nHashMap.get (&map) (Key { name: \"x\" })",
        "let map = HashMap.remove (HashMap.insert (HashMap.empty()) 1 2) 1\nHashMap.contains_key (&map) 1",
        "let empty: HashMap<i64, string> = HashMap.with_capacity 0\nlet sized: HashMap<i64, string> = HashMap.with_capacity 10\nHashMap.length (&empty) + HashMap.length (&sized)",
    ] {
        accepts(source);
    }
    for source in [
        "let set = HashSet.remove (HashSet.insert (HashSet.insert (HashSet.empty()) \"a\") \"b\") \"a\"\nHashSet.fold (\\total key -> total + key.length) 0 (&set)",
        "let set = HashSet.insert (HashSet.empty()) 7\nlet values = HashSet.to_array (&set)\nlet mut total = 0\nfor key in HashSet.iter (&set) do total = total + deref key\ntotal + values[0] + (if HashSet.contains (&set) 7 then 1 else 0) + (if HashSet.is_empty (&set) then 1 else 0)",
        "let text = \"borrowed\"\nlet map = HashMap.insert (HashMap.empty()) 1 (ref text)\nlet value = HashMap.at (&map) 1\nvalue.length",
        "let map = HashMap.insert (HashMap.empty()) 1 (task { return 42 })\nlet map = HashMap.remove map 1\nHashMap.length (&map)",
        "let map = HashMap.insert (HashMap.empty()) 1 2\nlet total = HashMap.fold (\\sum key value -> sum + deref key + deref value) 0 (&map)\nlet mut count = 0\nfor pair in HashMap.iter (&map) do count = count + 1\ntotal + count",
    ] {
        accepts(source);
    }
    for (source, code) in [
        (
            "record Key { name: string } deriving (Eq)\nlet map = HashMap.insert (HashMap.empty()) (Key { name: \"x\" }) 1\nHashMap.length (&map)",
            "E1005",
        ),
        (
            "record Key { name: string } deriving (Hash)\nlet map = HashMap.insert (HashMap.empty()) (Key { name: \"x\" }) 1\nHashMap.length (&map)",
            "E1005",
        ),
        (
            "let map = HashMap.insert (HashMap.empty()) 1 \"x\"\nHashMap.get (&map) 1",
            "E1005",
        ),
        (
            "let map = HashMap.insert (HashMap.empty()) \"x\" 1\nHashMap.keys (&map)",
            "E1005",
        ),
        (
            "let set = HashSet.insert (HashSet.empty()) \"x\"\nHashSet.to_array (&set)",
            "E1005",
        ),
        (
            "let map = HashMap.insert (HashMap.empty()) 1 \"x\"\nlet value = HashMap.at (&map) 1\nlet next = HashMap.remove map 1\nvalue.length",
            "E1014",
        ),
        (
            "fn bad() -> &string { let map = HashMap.insert (HashMap.empty()) 1 \"x\"; HashMap.at (&map) 1 }",
            "E1013",
        ),
        (
            "let mut value = 1\nlet map = HashMap.insert (HashMap.empty()) 0 (ref mut value)\nHashMap.length (&map)",
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
        "let map: HashMap<i64, i64> = HashMap.with_seed 7i64u\nlet map = HashMap.insert map 1 2\nHashMap.length (&map) + HashMap.longest_probe (&map)",
        "let map: HashMap<i64, i64> = HashMap.with_capacity_and_seed 10 3i64u\nlet map = HashMap.insert map 1 2\nHashMap.length (&map)",
        "let map: HashMap<string, i64> = HashMap.with_seed 1i64u\nlet map = HashMap.insert map \"k\" 1\nlet key = \"k\"\nlet found = HashMap.contains_key_ref (&map) (&key)\nlet value = HashMap.get_ref (&map) (&key)\nlet stored = deref (HashMap.at_ref (&map) (&key))\nlet map = HashMap.remove_ref map (&key)\nHashMap.length (&map) + stored + Maybe.get value + (if found then 1 else 0) + key.length",
        "let set: HashSet<string> = HashSet.with_seed 1i64u\nlet set = HashSet.insert set \"a\"\nlet key = \"a\"\nlet present = HashSet.contains_ref (&set) (&key)\nlet set = HashSet.remove_ref set (&key)\nHashSet.longest_probe (&set) + HashSet.length (&set) + (if present then 1 else 0)",
        "let set: HashSet<i64> = HashSet.with_capacity_and_seed 4 2i64u\nHashSet.length (&set)",
        "let word: i64u = HashMap.sip13 1i64u 2i64u 3i64u\nword",
        "def lookup {r} :: ref {r} HashMap<string, string> -> string -> ref {r} string\nfn lookup map key = HashMap.at map key\nlet map = HashMap.insert (HashMap.empty()) \"k\" \"v\"\n(lookup (&map) \"k\").length",
    ] {
        accepts(source);
    }
    for (source, code) in [
        (
            "let map = HashMap.insert (HashMap.empty()) \"k\" \"v\"\nlet key = \"k\"\nlet value = HashMap.at_ref (&map) (&key)\nlet next = HashMap.remove_ref map (&key)\nvalue.length",
            "E1014",
        ),
        (
            "let seed: i64 = 7\nlet map: HashMap<i64, i64> = HashMap.with_seed seed\nHashMap.length (&map)",
            "E1003",
        ),
        (
            "record Key { name: string } deriving (Eq)\nlet set: HashSet<Key> = HashSet.with_seed 1i64u\nlet key = Key { name: \"x\" }\nHashSet.contains_ref (&set) (&key)",
            "E1005",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn unused_hash_containers_emit_no_code() {
    let baseline = "let mut map: Map<i64, i64> = Map.empty()\nmap = Map.insert map 2 20\nmap = Map.insert map 1 10\nMap.fold (\\total key value -> total * 100 + deref key + deref value) 0 (&map)";
    let module = analyze(baseline).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(!ir.contains("HashMap") && !ir.contains("HashSet"), "{ir}");
    }
    let used =
        analyze("let map = HashMap.insert (HashMap.empty()) 1 2\nHashMap.length (&map)").unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&used, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("@tz.fn.HashMap.insert"), "{ir}");
        assert!(!ir.contains("@tz.fn.HashSet."), "{ir}");
    }
}

#[test]
fn hash_containers_leave_the_specialization_budget_to_users() {
    let mut source = String::from("def id :: 'a -> 'a\nfn id x = x\n");
    for index in 0..1024 {
        writeln!(
            source,
            "record R{index} {{ value: i8 }}\nfn f{index}(x: R{index}) -> R{index} {{ id x }}"
        )
        .unwrap();
    }
    analyze(&source).unwrap();
    source.push_str("fn extra(x: [i8]) -> [i8] { id x }");
    rejects(&source, "E1017");
}
