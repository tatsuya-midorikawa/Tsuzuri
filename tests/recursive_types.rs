use tsuzuri::{analyze, check::Type};

#[test]
fn recursive_runtime_fixture_typechecks() {
    tsuzuri::analyze_modules(&[("Main.tz", include_str!("fixtures/recursive_types/Main.tz"))])
        .unwrap();
}

#[test]
fn recursive_patterns_and_loans_keep_existing_safety_rules() {
    let prefix = "union Chain<'a> = End | Link of 'a * Chain<'a>\n";
    for source in [
        "def read :: ref Chain<i64> -> i64\nfn read tree = match tree with | End -> 0 | Link (value, _) -> value",
        "let number = 42\nlet tree = Link (ref number, End)\nmatch ref tree with | End -> 0 | Link (value, _) -> deref value",
    ] {
        analyze(&format!("{prefix}{source}")).unwrap();
    }
    for (source, code) in [
        (
            "def escape :: i64 -> Chain<ref i64>\nfn escape number = Link (ref number, End)",
            "E1013",
        ),
        (
            "def send :: ref i64 -> Task<Chain<ref i64>>\nfn send number = task { return Link (number, End) }",
            "E1013",
        ),
        (
            "def send :: ref i64 -> Task<Chain<i64 -> i64>>\nfn send number = { let callback = value -> value + deref number; let tree = Link (callback, End); task { return tree } }",
            "E1013",
        ),
        ("record Holder { tree: Chain<ref mut i64> }", "E1013"),
        (
            "let tree = Link (1, End)\nmatch tree with | Link (_, tail) as whole -> { let moved = tail; match whole with | End -> 0 | Link _ -> 1 } | End -> 0",
            "E1012",
        ),
        (
            "def read :: Chain<i64> -> i64\nfn read tree = match tree with | Link (_, End) -> 0",
            "E1021",
        ),
        (
            "export def expose :: Chain<i64> -> i64\nfn expose _tree = 0",
            "E1008",
        ),
    ] {
        let error = analyze(&format!("{prefix}{source}")).unwrap_err();
        assert_eq!(error.code, code, "{source}\n{error:?}");
    }
    let module = analyze(&format!("{prefix}def read :: Chain<i64> -> i64\nfn read tree = match tree with | End -> 0 | Link _ -> 1 | End -> 2")).unwrap();
    assert!(
        module
            .warnings
            .iter()
            .any(|warning| warning.code == "W1003")
    );
}

#[test]
fn recursive_layout_and_workers_are_instance_sensitive() {
    let source = "record Link { next: Maybe<Link> }\ndef make :: Link\nfn make = Link { next: Some (Link { next: None }) }\ndef ordinary :: Maybe<i64>\nfn ordinary = Some 42\ndef captured :: i64 -> i64\nfn captured number = { let tree = make(); let callback = value -> match ref tree.next with | Some child -> number + value + (if Maybe.is_none (ref child.next) then 1 else 0) | None -> 0; let second = callback; second 1 + callback 2 }";
    let module = analyze(source).unwrap();
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap()
        );
        assert!(ir.contains("tz.rec.node.Maybe.Maybe[Main.Link]"));
        assert!(ir.contains("%\"tz.union.Maybe.Maybe[i64]\" = type { i32, i64 }"));
        assert!(ir.contains("@tz.rec.drop") && ir.contains("@tz.rec.clone"));
        assert!(!ir.contains("boxed["));
        for body in ir.split("define internal void @\"tz.drop.rec.").skip(1) {
            assert!(!body.split("\n}").next().unwrap().contains("@tz.alloc"));
        }
    }
}

#[test]
fn recursive_unions_have_finite_values_without_implicit_copy() {
    for source in [
        "union Tree<'a> = Leaf | Node of Tree<'a> * 'a * Tree<'a>\nlet tree = Node (Leaf, 1, Leaf)\nmatch tree with | Leaf -> 0 | Node (_, value, _) -> value",
        "record Branch<'a> { left: Tree<'a>, value: 'a, right: Tree<'a> }\nunion Tree<'a> = Leaf | Node of Branch<'a>\nlet tree = Node (Branch { left: Leaf, value: 1, right: Leaf })\nmatch tree with | Leaf -> 0 | Node branch -> branch.value",
        "union Rose = Node of [Rose]\nlet tree = Node []\n()",
        "record Link { next: Maybe<Link> }\nlet last = Link { next: None }\nlet first = Link { next: Some last }\n()",
    ] {
        analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    let module = analyze("union Chain = End | Link of Chain\nlet value = Link End\n()").unwrap();
    let id = module
        .unions
        .iter()
        .position(|union| union.name == "Main.Chain")
        .unwrap();
    assert!(!Type::Union(id, Box::default()).is_copy(&module.types()));
    for (source, code) in [
        ("record Bad { next: Bad }", "E1010"),
        ("record Bad { next: [Bad] }", "E1010"),
        ("union Bad = Loop of Bad", "E1010"),
        ("union Bad<'a> = End | Loop of Bad<['a]>", "E1017"),
        ("union Swap<'a, 'b> = End | Loop of Swap<'b, 'a>", "E1017"),
        (
            "union Chain = End | Link of Chain\nlet value = Link End\nlet moved = value\nmatch value with | End -> 0 | Link _ -> 1",
            "E1012",
        ),
    ] {
        assert_eq!(analyze(source).unwrap_err().code, code, "{source}");
    }
}
