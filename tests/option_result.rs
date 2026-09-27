use tsuzuri::check::CheckedModule;
use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn standard_types_constructors_and_functions_need_no_user_modules() {
    for source in [
        "let x: Option<i64> = Some 1\nOption.get x",
        "let x: Option<i64> = None\nOption.default_value 42 x",
        "let x: Result<i64, string> = Ok 42\nResult.get x",
        "let x: Result<i64, string> = Error \"bad\"\nResult.default_value 42 x",
        "Option.get (Option.map (n -> n + 1) (Some 41))",
        "Option.get (Option.filter (n -> *n > 0) (Some 42))",
        "let r: Result<i64, string> = Result.map_error (s -> s + \"!\") (Error \"bad\")\nResult.get_error r",
        "Result.get (Option.to_result \"missing\" (Some 42))",
        "let r: Result<i64, string> = Ok 42\nOption.get (Result.to_option r)",
        "let r: Result<Option<i64>, string> = Ok (Some 42)\nOption.get (Result.get r)",
        "let xs: [Option<i64>] = [None, Some 42]\nOption.default_value 0 xs[1]",
        "let constructor: i64 -> Option<i64> = Some\nOption.get (constructor 42)",
        "let constructor: string -> Result<i64, string> = Error\nResult.get_error (constructor \"bad\")",
    ] {
        accepts(source);
    }
    let module = accepts("42");
    assert!(module.warnings.is_empty(), "{:?}", module.warnings);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!ir.contains("tz.union.Option"));
    assert!(!ir.contains("tz.union.Result"));
}

#[test]
fn user_cases_take_precedence_without_changing_standard_library_types() {
    accepts(
        "union Local = None | Some of bool
         let x: Local = Some true
         let y: Option<i64> = Option.Some 42
         Option.get y",
    );
    let error = analyze_modules(&[
        ("Main.tz", "let value = Some 42\n0"),
        ("First.tz", "union First = Some of i64"),
        ("Second.tz", "union Second = Some of i64"),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1004");
}

#[test]
fn rejects_unresolved_types_invalid_errors_noncopy_iteration_and_abi() {
    for (source, code) in [
        ("let x = None\n0", "E1015"),
        ("let x = Error \"bad\"\n0", "E1015"),
        (
            "let r: Result<i64, string> = Result { let! n = Ok 1; return! Error 2 }\n0",
            "E1003",
        ),
        (
            "Option.get (Option { for s in [\"a\"] do do! Some (); return 1 })",
            "E1005",
        ),
        ("export def bad :: Option<i64>\nfn bad = Some 42", "E1008"),
        (
            "let result: Result<i64, string> = Ok 1\nlet mapped = Result.map_ref (r -> *r) (&result)\n0",
            "E1005",
        ),
        ("def unreachable :: i64\nfn unreachable = 0", "E1001"),
    ] {
        rejects(source, code);
    }
    accepts("def fail :: string\nfn fail = unreachable ()");
}

#[test]
fn shared_union_payloads_preserve_owners_and_input_lifetimes() {
    for source in [
        "let owner = 42\nlet borrowed: Option<&i64> = Some (ref owner)\n*(Option.get borrowed)",
        "let owner = 42\nlet borrowed: Result<&i64, i64> = Ok (ref owner)\n*(Result.get borrowed)",
        "def first :: ref [string] -> Option<ref string>\nfn first values = if values.length == 0 then None else Some (ref values[0])\nlet values = [\"owned\"]\n(Option.get (first (ref values))).length",
        "let number = 42\nlet value = Some (Some (ref number))\nlet copy = value\nmatch copy with | Some (Some borrowed) -> deref borrowed | _ -> 0",
        "def choose :: ref i64 -> ref i64 -> Option<ref i64>\nfn choose left right = Some left\nlet left = 20\nlet right = 22\nderef (Option.get (choose (ref left) (ref right)))",
        "record Stored { value: Option<ref i64> }\nlet owner = 42\nlet stored = Stored { value: Some (ref owner) }\nderef (Option.get stored.value)",
    ] {
        let module = accepts(source);
        for wasm in [false, true] {
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        }
    }
    for source in [
        "def nested :: ref (Option<ref i64>) -> ref i64\nfn nested value = Option.get (deref value)\nlet mut owner = 42\nlet container = Some (ref owner)\nlet borrowed = nested (ref container)\nowner = 1\nderef borrowed",
        "def nested :: ref (Option<ref i64>) -> ref i64\nfn nested value = Option.get (deref value)\nlet owner = 42\nlet borrowed = { let container = Some (ref owner); nested (ref container) }\nderef borrowed",
        "let option = Some 42\nlet borrowed: Option<&i64> = Option.map_ref (value -> value) (&option)\n*(Option.get borrowed)",
        "let result: Result<i64, i64> = Ok 42\nlet borrowed: Result<&i64, i64> = Result.bind_ref (&result) (value -> Ok value)\n*(Result.get borrowed)",
        "let borrowed = { let value = 42; Some (ref value) }\n*(Option.get borrowed)",
        "let mut value = 42\nlet borrowed = Some (ref value)\nvalue = 1\n*(Option.get borrowed)",
        "let mut value = 42\nlet borrowed = Some (ref mut value)\n()",
        "record Stored { value: Option<ref mut i64> }",
        "let value = 42\nlet borrowed = Some (ref value)\nlet work = task { return borrowed }\n()",
        "def choose :: ref i64 -> ref i64 -> Option<ref i64>\nfn choose left right = Some left\nlet left = 20\nlet borrowed = { let right = 22; choose (ref left) (ref right) }\n*(Option.get borrowed)",
    ] {
        let error = analyze(source).expect_err(source);
        assert!(
            matches!(error.code, "E1013" | "E1014"),
            "{source}\n{}: {}",
            error.code,
            error.message
        );
    }
}

#[test]
fn maps_borrowed_noncopy_payloads_without_inferred_copy_constraints() {
    for source in [
        "let option = Some \"hello\"\nOption.get (Option.map_ref size (&option))",
        "let option = Some \"hello\"\nOption.get (Option.bind_ref (&option) (s -> Some s.length))",
        "let result: Result<string, i64> = Ok \"hello\"\nResult.get (Result.map_ref size (&result))",
        "let result: Result<string, i64> = Ok \"hello\"\nResult.get (Result.bind_ref (&result) (s -> Ok s.length))",
        "let option = Some \"hello\"\nOption.get (Option.filter (s -> size s > 0) option)",
        "def length :: &(Option<'a>) -> (&'a -> i64) -> i64
         fn length option size =
             match option with
             | Some value -> size (&value)
             | None -> 0
         let value = Some \"hello\"
         length (&value) (s -> s.length)",
    ] {
        accepts(&format!(
            "def size :: &string -> i64\nfn size text = text.length\n{source}"
        ));
    }
}

#[test]
fn views_cover_nested_patterns_collections_strings_and_iteration() {
    for source in [
        "let value = Some (Some \"hello\")
         match &value with
         | Some inner -> match inner with | Some text -> text.length | None -> 0
         | None -> 0",
        "let words = [\"a\", \"bb\"]
         match &words with | [a; b] -> a.length + b.length | _ -> 0",
        "let words = [|\"a\", \"bb\", \"ccc\"|]
         match &words with
         | head :: tail ->
             let mut count = head.length
             for word in tail do count = count + word.length
             count
         | [||] -> 0",
        "let pairs = [(\"a\", 1), (\"bb\", 2)]
         let mut count = 0
         for (word, n) in pairs do count = count + word.length + n
         count",
        "def length :: &string -> i64
         fn length text = match *text with | value -> value.length
         let text = \"hello\"
         length (&text)",
        "let pair = ([\"a\"], \"bb\")
         match pair with | ([text], _) | (_, text) -> text.length",
        "let value = Some \"hello\"
         match &value with
         | Some text ->
             let borrowed = &text
             let length = u -> borrowed.length
             length ()
         | None -> 0",
    ] {
        accepts(source);
    }
}

#[test]
fn views_cannot_move_mutate_escape_or_outlive_the_matched_storage() {
    for (source, code) in [
        (
            "def bad :: &(Option<string>) -> string
             fn bad value = match value with | Some text -> text | None -> \"\"",
            "E1012",
        ),
        (
            "def bad :: &mut (Option<string>) -> i64
             fn bad value =
                 match *value with
                 | Some text ->
                     *value = None
                     text.length
                 | None -> 0",
            "E1014",
        ),
        (
            "def bad :: &mut (Option<string>) -> i64
             fn bad value =
                 match *value with
                 | Some text ->
                     let borrowed = &text
                     *value = None
                     borrowed.length
                 | None -> 0",
            "E1014",
        ),
        (
            "def bad :: &(Option<string>) -> &string
             fn bad value =
                 match value with | Some text -> &text | None -> unreachable ()",
            "E1013",
        ),
        (
            "def bad :: &(Option<string>) -> (unit -> i64)
             fn bad value =
                 match value with
                 | Some text ->
                     let borrowed = &text
                     u -> borrowed.length
                 | None -> u -> 0",
            "E1013",
        ),
        (
            "let pair = ([\"a\"], \"bb\")
             match pair with | ([text], _) | (_, text) -> text",
            "E1012",
        ),
        (
            "let value = Some \"hello\"
             match &value with | Some text -> { let r = &mut text; 0 } | None -> 0",
            "E1014",
        ),
    ] {
        rejects(source, code);
    }
}

#[test]
fn view_last_use_and_copy_snapshot_semantics_are_preserved() {
    accepts(
        "def size_and_clear :: &mut (Option<string>) -> i64
         fn size_and_clear value =
             match *value with
             | Some text ->
                 let size = text.length
                 *value = None
                 size
             | None -> 0",
    );
    const TAKE: &str = "def take :: &mut (Option<'a>) -> Option<'a>
        fn take value =
            match *value with
            | Some item ->
                *value = None
                Some item
            | None -> None\n";
    accepts(&format!(
        "{TAKE}let mut value = Some 42\nOption.get (take (&mut value))"
    ));
    accepts(&format!(
        "{TAKE}let mut value = Some [20, 22]\n(Option.get (take (&mut value))).length"
    ));
    rejects(
        &format!("{TAKE}let mut value = Some \"hello\"\nOption.get (take (&mut value))"),
        "E1005",
    );
}

#[test]
fn runtime_fixture_has_deterministic_import_free_lowering() {
    let module =
        analyze_modules(&[("Main.tz", include_str!("fixtures/option_result/Main.tz"))]).unwrap();
    assert!(module.warnings.is_empty(), "{:?}", module.warnings);
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        assert!(ir.contains("@tz.specialized."));
        assert!(ir.contains("switch i32"));
        for function in ir.split("\n\n").filter(|function| {
            function.starts_with("define internal")
                && ["@tz.fn.Option.For.", "@tz.fn.Result.For."]
                    .iter()
                    .any(|name| function.lines().next().unwrap().contains(name))
        }) {
            assert!(!function.contains("call ptr @tz.alloc"), "{function}");
        }
    }
}
