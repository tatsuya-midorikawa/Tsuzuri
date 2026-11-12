use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::check::CheckedModule;
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::driver::Project;
use tsuzuri::syntax::SourceKind;
use tsuzuri::{formatter, llvm};

fn check(edition: &str, source: &str) -> Result<CheckedModule, Diagnostic> {
    check_with_files(edition, source, &[])
}

fn check_with_files(
    edition: &str,
    source: &str,
    extra: &[(&str, &str)],
) -> Result<CheckedModule, Diagnostic> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/parameter-conventions")
        .join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    for (name, text) in extra {
        std::fs::write(root.join(name), text).unwrap();
    }
    std::fs::write(
        root.join("Tsuzuri.toml"),
        format!(
            "[package]\nname = \"conventions\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n"
        ),
    )
    .unwrap();
    let result = Project::load(&root).unwrap().analyze();
    std::fs::remove_dir_all(root).unwrap();
    result
}

fn ir(edition: &str, source: &str) -> String {
    let module = check(edition, source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

#[test]
fn only_2027_borrows_unmarked_owned_data() {
    let source = "def size :: string -> i64 = \\s -> s.length\n\
        export def answer :: i64 = { let text = \"hello\"; size text + text.length }\n";
    assert_eq!(check("2026", source).unwrap_err().code, "E1012");
    assert_eq!(tsuzuri::analyze(source).unwrap_err().code, "E1012");
    tsuzuri::analyze_with_edition(source, tsuzuri::syntax::Edition::E2027).unwrap();
    assert_eq!(
        ir("2027", source),
        ir("2027", &source.replace(":: string", ":: ref string"))
    );
}

#[test]
fn borrowed_arrays_match_explicit_references_without_copying() {
    let source = "def first :: [i64] -> i64 = \\xs -> xs[0]\n\
        export def answer :: i64 = { let xs = [10, 20]; first xs + xs[1] }\n";
    let borrowed = ir("2027", source);
    assert_eq!(
        borrowed,
        ir("2027", &source.replace(":: [i64]", ":: ref [i64]"))
    );
    let answer = borrowed
        .split("\n\n")
        .find(|block| block.starts_with("define ") && block.contains("@tz.fn.Main.answer("))
        .unwrap();
    assert!(!answer.contains("call ptr @tz.alloc"), "{answer}");
    assert_ne!(borrowed, ir("2026", source));
}

#[test]
fn entry_and_active_result_inference_use_the_module_conventions() {
    let entry = "let texts = [\"a\", \"bbb\"]\n\
        let lengths = Array.map (\\text -> text.length) texts\n\
        Array.sum lengths + texts.length\n";
    check("2027", entry).unwrap();
    assert_eq!(check("2026", entry).unwrap_err().code, "E1005");
    check(
        "2027",
        "let mut values = Vec.empty()\nVec.push 2 values\nvalues.length\n",
    )
    .unwrap();
    let active = "def (|Short|Long|) :: [string] -> 'T = \\texts -> \
        if Array.sum (Array.map (\\text -> text.length) texts) < 5 then Short else Long\n";
    check("2027", active).unwrap();
    assert!(check("2026", active).is_err());
}

#[test]
fn own_is_additive_and_keeps_the_2026_value_convention() {
    let source = "def take :: own string -> string = \\s -> s\n\
        export def answer :: i64 = (take \"hello\").length\n";
    assert_eq!(ir("2027", source), ir("2026", source));
    assert_eq!(ir("2026", source), ir("2026", &source.replace("own ", "")));
    let moved = "def take :: own string -> i64 = \\s -> s.length\n\
        def bad :: unit -> i64 = \\() -> { let s = \"x\"; take s + s.length }\n";
    assert_eq!(check("2027", moved).unwrap_err().code, "E1012");
}

#[test]
fn own_is_only_a_whole_def_parameter_and_is_not_reserved() {
    for source in [
        "def bad :: i64 -> own string = \\n -> \"x\"\n",
        "def bad :: unit -> i64 = \\() -> { let s: own string = \"x\"; s.length }\n",
        "record Bad { value: own string }\n",
        "def bad :: (own string -> i64) -> i64 = \\f -> 0\n",
        "def bad :: own own string -> i64 = \\s -> s.length\n",
    ] {
        for edition in ["2026", "2027"] {
            assert_eq!(
                check(edition, source).unwrap_err().code,
                "E1005",
                "{source}"
            );
        }
    }
    let source = "def own :: i64 -> i64 = \\own -> own + 1\n";
    assert_eq!(ir("2026", source), ir("2027", source));
}

#[test]
fn own_type_names_do_not_consume_the_next_function_implementation() {
    let source = "record own { n: i64 }\n\
        def id :: own -> own\nfn id x = x\n";
    let explicit = source.replace(":: own ->", ":: own own ->");
    assert_eq!(ir("2026", source), ir("2026", &explicit));
    assert_eq!(ir("2026", source), ir("2027", &explicit));
    for edition in ["2026", "2027"] {
        check(
            edition,
            "fn apply(f: own fn(i64) -> i64, x: i64) -> i64 { f(x) }\n",
        )
        .unwrap();
    }
}

#[test]
fn own_type_names_remain_nominal_at_alias_and_union_boundaries() {
    for (declaration, entry) in [
        ("type Alias = own", "(own { n: 42 }).n"),
        (
            "union Choice = Item of own",
            "(match Item (own { n: 42 }) with | Item value -> value.n)",
        ),
    ] {
        let delimited = format!("record own {{ n: i64 }}\n{declaration};\n{entry}\n");
        let expected = ir("2026", &delimited);
        for separator in ["\n", "\r\n", " "] {
            let source = format!("record own {{ n: i64 }}\n{declaration}{separator}{entry}\n");
            for edition in ["2026", "2027"] {
                assert_eq!(expected, ir(edition, &source));
            }
        }
        let prefix = "record own { n: i64 }\ndef left :: i64 = 40\ndef right :: i64 = 2\n";
        let expected =
            check("2026", &format!("{prefix}{declaration};\nleft | right\n")).unwrap_err();
        assert_eq!(expected.code, "E1005");
        for separator in ["\n", "\r\n", " "] {
            let source = format!("{prefix}{declaration}{separator}left | right\n");
            for edition in ["2026", "2027"] {
                let actual = check(edition, &source).unwrap_err();
                assert_eq!(actual.code, expected.code);
                assert_eq!(actual.message, expected.message);
            }
        }
    }
}

#[test]
fn own_type_names_do_not_capture_extern_entry_expressions() {
    for (declaration, entry) in [
        ("extern def get :: own", "get().n"),
        ("extern def get :: i64 -> own", "(get 7).n"),
    ] {
        let explicit = format!(
            "record own {{ n: i64 }}\ntype Return = own\n{}\n{entry}\n",
            declaration.replace("own", "Return")
        );
        let expected = ir("2026", &explicit);
        for separator in ["\n", "\r\n", " "] {
            let source = format!("record own {{ n: i64 }}\n{declaration}{separator}{entry}\n");
            for edition in ["2026", "2027"] {
                assert_eq!(expected, ir(edition, &source));
            }
        }
    }
    let source = "extern def host_len :: own ref string -> i64\n";
    assert_eq!(ir("2026", source), ir("2027", source));
    assert_eq!(ir("2026", source), ir("2026", &source.replace("own ", "")));
}

#[test]
fn explicit_ownership_preserves_generic_and_callable_value_conventions() {
    let source = "record Point { x: i64, y: i64 }\n\
        def get :: Point -> i64 = \\p -> p.x\n\
        def identity :: own 'a -> 'a = \\x -> x\n\
        def apply :: own ('a -> 'b) -> own 'a -> 'b = \\f x -> f x\n\
        export def answer :: i64 -> i64 = \\n -> apply identity (get (Point { x: n, y: 2 }))\n";
    assert_eq!(ir("2026", source), ir("2027", source));
    check("2027", "def length :: ['a] -> i64 = \\xs -> xs.length\n").unwrap();
}

#[test]
fn generic_reads_and_borrowed_callables_preserve_the_callers_values() {
    let source = "def inspect :: 'a -> i64 = \\value -> 3\n\
        def twice :: (ref 'a -> i64) -> 'a -> i64 = \\f value -> f value + f value\n\
        def size :: string -> i64 = \\text -> text.length\n\
        export def answer :: i64 = { let text = \"hello\"; twice size text + inspect text + text.length }\n";
    check("2027", source).unwrap();
    assert_eq!(check("2026", source).unwrap_err().code, "E1003");
    check(
        "2027",
        "def identity :: 'a -> 'a = \\x -> x\n\
         export def answer :: i64 = identity 42\n",
    )
    .unwrap();
    let error = check(
        "2027",
        "def identity :: 'a -> 'a = \\x -> x\n\
         export def answer :: i64 = (identity \"moved\").length\n",
    )
    .unwrap_err();
    assert_eq!(error.code, "E1005");
    assert!(error.message.contains("Copy<string>"), "{error:?}");
}

#[test]
fn map_reads_noncopy_elements_and_keeps_first_class_callback_conventions() {
    for callback in ["size", "(\\text -> text.length)"] {
        check(
            "2027",
            &format!(
                "def size :: string -> i64 = \\text -> text.length\n\
                 export def answer :: i64 = {{ let texts = [\"a\", \"bc\"]; \
                 let mapper = Array.map; \
                 Array.sum (mapper {callback} texts) + texts.length }}\n"
            ),
        )
        .unwrap();
    }
    check(
        "2027",
        "export def answer :: i64 = Array.sum (Array.map (\\n -> n + n) [1, 2, 3])\n",
    )
    .unwrap();
    check(
        "2027",
        "def value :: own i64 -> i64 = \\n -> n + 1\n\
         export def answer :: i64 = Array.sum (Array.map value [1, 2, 3])\n",
    )
    .unwrap();
    check(
        "2027",
        "export def answer :: i64 = Array.sum (Array.mapr (\\n -> deref n + 1) [1, 2, 3])\n",
    )
    .unwrap();
}

#[test]
fn all_read_callback_entry_points_accept_noncopy_elements() {
    for (module, values) in [("Array", "[\"a\", \"bc\"]"), ("List", "[|\"a\", \"bc\"|]")] {
        for (name, body) in [
            (
                "map",
                "(callback (\\text -> text.length) texts).length + texts.length",
            ),
            (
                "mapi",
                "(callback (\\i text -> i + text.length) texts).length + texts.length",
            ),
            ("iter", "callback (\\text -> ()) texts; texts.length"),
            ("iteri", "callback (\\i text -> ()) texts; texts.length"),
            (
                "fold",
                "callback (\\sum text -> sum + text.length) 0 texts + texts.length",
            ),
            (
                "fold_back",
                "callback (\\text sum -> sum + text.length) 0 texts + texts.length",
            ),
        ] {
            let source = format!(
                "export def answer :: i64 = {{ let texts = {values}; \
                 let callback = {module}.{name}; {body} }}\n"
            );
            check("2027", &source).unwrap_or_else(|error| panic!("{module}.{name}: {error:?}"));
            assert_eq!(check("2026", &source).unwrap_err().code, "E1005");
        }
    }
}

#[test]
fn owning_callback_aliases_keep_complete_editor_reference_spans() {
    let source = "export def answer :: i64 = { let values = [1, 2]; \
        let mapper = Array.map_owned; Array.sum (mapper (\\n -> n + 1) values) }\n";
    let (_, index) = tsuzuri::analyze_modules_with_semantics(&[("Main", source)]).unwrap();
    let offset = source.find("map_owned").unwrap();
    let occurrence = index.occurrence_at(0, offset + 4).unwrap();
    assert_eq!(
        &source[occurrence.span.start..occurrence.span.end],
        "map_owned"
    );
    assert_eq!(index.definitions[occurrence.definition].name, "map");
    assert_eq!(index.definitions[occurrence.definition].module, "Array");
}

#[test]
fn contextual_callbacks_keep_array_types_in_blocks_and_computation_continuations() {
    check(
        "2027",
        "export def answer :: i64 = { let texts = [\"a\", \"bc\"]; \
         Array.sum (Array.map ({ let ignored = 0; \\text -> text.length }) texts) }\n",
    )
    .unwrap();
    let source = "def action :: unit -> Maybe<i64> = \\() -> Maybe { \
        let! values = Maybe.Some [Maybe.default_value 0 (Maybe { return 1 }), 2]; \
        let! more = Maybe { let! inner = Maybe.Some [values[1], 3]; return inner }; \
        return values[0] + more[1] }\n\
        export def answer :: i64 = 0\n";
    assert_eq!(ir("2027", source), ir("2026", source));
}

#[test]
fn owning_generic_callbacks_constrain_implicit_borrows_before_value_arguments() {
    let source = "def apply :: own ('a -> 'b) -> own 'a -> 'b = \\f x -> f x\n\
        export def answer :: i64 = { let text = \"abc\"; \
        let size = apply String.length text; \
        let copied = clone_string text; assert (copied == text); \
        size + text.length }\n";
    assert_eq!(ir("2027", source), ir("2026", source));
    let borrowed = source
        .replace("own ('a -> 'b)", "ref ('a -> 'b)")
        .replace("\\f x -> f x", "\\f x -> (deref f) x");
    assert_eq!(ir("2027", &borrowed), ir("2026", &borrowed));
}

#[test]
fn projected_updates_preserve_destructor_whole_value_restrictions() {
    let source = "record Value { text: string }\n\
        instance Drop<Value> { fn drop value = value.text = \"done\" }\n\
        export def answer :: i64 = { let value = Value { text: \"old\" }; value.text.length }\n";
    assert_eq!(check("2027", source).unwrap_err().code, "E1012");
    assert_eq!(check("2026", source).unwrap_err().code, "E1012");
    let whole = source.replace(
        "value.text = \"done\"",
        "deref value = Value { text: \"done\" }",
    );
    assert_eq!(check("2027", &whole).unwrap_err().code, "E1012");
}

#[test]
fn read_callbacks_cannot_escape_local_elements() {
    let source = "def borrow_value :: ref i64 -> ref i64 = \\value -> value\n\
        def leak :: unit -> [ref i64] = \\() -> { let values = [42]; Array.map borrow_value values }\n\
        export def answer :: i64 = deref (leak ())[0]\n";
    let error = check("2027", source).unwrap_err();
    assert!(matches!(error.code, "E1013" | "E1014"), "{error:?}");
}

#[test]
fn generic_read_views_accept_borrowed_scalar_and_callable_temporaries() {
    for value in ["42", "-42", "true", "'x'", "()", "\"hello\""] {
        check(
            "2027",
            &format!(
                "def inspect :: 'a -> i64 = \\value -> 1\n\
                 export def answer :: i64 = inspect ({value})\n"
            ),
        )
        .unwrap();
    }
    check(
        "2027",
        "def twice :: (ref 'a -> i64) -> 'a -> i64 = \\f value -> f value + f value\n\
         export def answer :: i64 = twice (\\text -> text.length) \"hello\"\n",
    )
    .unwrap();
}

#[test]
fn value_expressions_are_lent_like_operator_temporaries() {
    // A cast, `@checked`, a unary operator, `if`, `match` and a block are lent to a `ref T`
    // parameter, and in 2027 to a type parameter, as `m + 1` is; a branch that gives a
    // reference is reborrowed instead.
    let forms = [
        "k (n as i64)",
        "k (@checked (m + 1))",
        "k (-m)",
        "k (if c then 1 else 2)",
        "k (if c then ref m else ref m)",
        "k (match n with | 1 -> 2 | _ -> 3)",
        "k ({ let x = m; x })",
    ];
    let headers = [
        ("2026", "def k :: ref i64 -> i64 = \\a -> deref a\n"),
        ("2027", "def k :: ref i64 -> i64 = \\a -> deref a\n"),
        ("2027", "def k :: 'n -> i64 = \\a -> 1\n"),
    ];
    for (edition, header) in headers {
        for form in forms {
            let source = format!(
                "{header}export def answer :: i32 -> i64 = \\n ->\n    let m = n as i64\n    let c = n > 0\n    {form}\n"
            );
            ir(edition, &source);
        }
    }
    // A cast argument followed by a symbol borrow (`at (n as i16) &xs`) is two arguments.
    ir(
        "2027",
        "def at :: Integer<'n> => 'n -> ref [i64] -> i64 = \\i xs -> xs[i as i64]\n\
         export def answer :: i32 -> i64 = \\n ->\n    let xs = [10i64, 20]\n    at (n as i16) (ref xs) + at (n as i64) (ref xs)\n",
    );
    // A value of another type is still a mismatch.
    let source = "def k :: ref i64 -> i64 = \\a -> deref a\nexport def answer :: i32 -> i64 = \\n -> k (n as i16)\n";
    assert_eq!(check("2027", source).unwrap_err().code, "E1003");
}

/// A27: a call whose result type is still open when it is passed to a `ref T` parameter (in 2027,
/// a type parameter such as `id`'s) is lent like a value, as the explicit `ref (...)` and a `let`
/// binding are: the result does not pass through the call's arguments.
#[test]
fn call_results_without_a_known_type_are_lent_to_read_parameters() {
    let program = |body: &str| {
        format!(
            "def id :: 'a -> 'a = \\v -> v\n\
             export def answer :: i64 -> i64\nfn answer input =\n{body}"
        )
    };
    let next = "    let next = Owned.function (\\value -> value + 1)\n";
    let forms = [
        (next, "id (Owned.call (ref next) input)"),
        (
            "    let next = Owned.function (\\(value: i64) -> value + 1)\n",
            "id (Owned.call (ref next) input)",
        ),
        (
            "    let next: Owned.Function<i64, i64> = Owned.function (\\value -> value + 1)\n",
            "id (Owned.call (ref next) input)",
        ),
        (next, "id (Owned.call next input)"),
        ("", "id ((\\value -> value + 1) input)"),
        ("    let g = \\value -> value + 1\n", "id (g input)"),
        ("    let xs = [input, 2]\n", "id (Array.get 0 xs)"),
        (
            "    let mk = Owned.function (\\(n: i64) -> to_string n)\n",
            "String.length (Owned.call (ref mk) input)",
        ),
    ];
    for (prelude, call) in forms {
        let (callee, argument) = call.split_once(' ').unwrap();
        let explicit = format!("{callee} (ref {argument})");
        assert_eq!(
            ir("2027", &program(&format!("{prelude}    {call}\n"))),
            ir("2027", &program(&format!("{prelude}    {explicit}\n"))),
            "{call}"
        );
    }
}

/// A27: a call whose result comes from its arguments keeps the type the expected `ref T` gives it,
/// so `id` lends its argument instead of the call lending a temporary.
#[test]
fn pass_through_calls_keep_their_types() {
    // `id r` is `ref string` and lends `r` as `ref (ref string)`: `id` returns a reference.
    let size = "def id :: 'a -> 'a = \\v -> v\n\
                def size :: ref string -> i64 = \\s -> s.length\n\
                export def answer :: i64\nfn answer =\n    let s = \"hello\"\n    let r = ref s\n";
    let direct = ir("2027", &format!("{size}    size (id r)\n"));
    let id = direct
        .lines()
        .find(|line| line.starts_with("define ") && line.contains("@tz.fn.Main.id."))
        .expect("id is specialized");
    assert!(id.starts_with("define internal ptr "), "{id}");
    // A `let` takes `id r` by value, which needs `Copy<string>`.
    let split = format!("{size}    let t = id r\n    size t\n");
    assert_eq!(check("2027", &split).unwrap_err().code, "E1005");
    // The constructor argument of the call is lent too.
    let maybe = "def id :: 'a -> 'a = \\v -> v\n\
                 export def answer :: i64 -> i64\nfn answer input =\n    let x = input\n";
    assert_eq!(
        ir(
            "2027",
            &format!("{maybe}    id (Maybe.get (Maybe.Some x))\n")
        ),
        ir(
            "2027",
            &format!("{maybe}    id (Maybe.get (Maybe.Some (ref x)))\n")
        )
    );
    // 2026: `id input` passes `input` through to `peek`, which borrows it.
    let peek = "def id :: 'a -> 'a = \\v -> v\n\
                def peek :: Copy<'a> => ref 'a -> 'a = \\v -> deref v\n\
                export def answer :: i64 -> i64\nfn answer input =\n";
    assert_eq!(
        ir("2026", &format!("{peek}    peek (id input)\n")),
        ir("2026", &format!("{peek}    peek (id (ref input))\n"))
    );
    // The pass-through forms that a later rule would need stay rejected.
    for call in ["id (id input)", "id (id 41)"] {
        let source = format!(
            "def id :: 'a -> 'a = \\v -> v\n\
             export def answer :: i64 -> i64\nfn answer input =\n    {call}\n"
        );
        assert_eq!(check("2027", &source).unwrap_err().code, "E1003", "{call}");
    }
}

/// A27: a call that stays rejected reports the error of its `let` form. `Array.get` copies the
/// element it returns, so lending the result of `Array.get 1 xs` to `String.length` needs
/// `Copy<string>` (`E1005` at the inner call), as `let t = Array.get 1 xs` does. The guess before
/// the arguments reported `E1003 expected ref string, found string` at `xs` instead.
#[test]
fn rejected_call_results_report_the_error_of_their_let_form() {
    let program = |body: &str| {
        format!("export def answer :: i64\nfn answer =\n    let xs = [\"ab\", \"cde\"]\n{body}")
    };
    let direct = program("    String.length (Array.get 1 xs)\n");
    let split = program("    let t = Array.get 1 xs\n    String.length t\n");
    for source in [&direct, &split] {
        let error = check("2027", source).unwrap_err();
        assert_eq!(error.code, "E1005", "{source}\n{}", error.message);
        assert!(error.message.contains("Copy<string>"), "{}", error.message);
        assert!(
            source[error.span.start..].starts_with("Array.get 1 xs"),
            "{source}\n{:?}",
            error.span
        );
    }
}

#[test]
fn value_expression_temporaries_keep_their_boundaries() {
    for edition in ["2026", "2027"] {
        // `try` lends its `Result`.
        ir(
            edition,
            "def ok :: ref Result<i64, Exception> -> i64 = \\r -> 1\n\
             export def answer :: i64 -> i64 = \\m -> ok (try @checked m + 1 with | e -> e)\n",
        );
        // A literal in a branch takes the referenced type and keeps its range check.
        let narrow = "def k :: ref i8 -> i64 = \\a -> 1\n\
                      export def answer :: bool -> i64 = \\c -> k (if c then 100 else -100)\n";
        ir(edition, narrow);
        let wide = narrow.replace("100 else", "300 else");
        assert_eq!(check(edition, &wide).unwrap_err().code, "E1009");
        // A branch that gives a non-Copy local moves it into the temporary; `ref` keeps it.
        let moved = "def size :: ref string -> i64 = \\t -> t.length\n\
                     export def answer :: bool -> i64 = \\c ->\n    let s = \"abc\"\n    size (if c then s else \"x\") + s.length\n";
        assert_eq!(check(edition, moved).unwrap_err().code, "E1012");
        ir(
            edition,
            &moved.replace("then s else \"x\"", "then ref s else ref s"),
        );
        // The borrow of a temporary ends with the call: a partial application and a reference
        // to a block's local are rejected as they are for `m + 1`.
        for form in [
            "{ let f = add (n as i64); f 1 }",
            "{ let f = add (if c then 1 else 2); f 1 }",
            "add ({ let x = 5; ref x }) 1",
        ] {
            let source = format!(
                "def add :: ref i64 -> i64 -> i64 = \\a b -> deref a + b\n\
                 export def answer :: i32 -> i64 = \\n ->\n    let c = n > 0\n    {form}\n"
            );
            assert_eq!(
                check(edition, &source).unwrap_err().code,
                "E1013",
                "{edition}: {form}"
            );
        }
    }
    // A branch is typed without the parameter's type, so a lambda in it names its parameter's
    // type, or the branch is bound with a type first.
    ir(
        "2027",
        "record Point { x: i64, y: i64 }\n\
         def apply :: (Point -> i64) -> Point -> i64 = \\f p -> f p\n\
         export def answer :: bool -> i64 = \\c ->\n    let pt = Point { x: 3, y: 4 }\n    \
         let pick: Point -> i64 = if c then \\p -> p.x else \\p -> p.y\n    \
         apply pick pt + apply (if c then \\(p: Point) -> p.x else \\(p: Point) -> p.y) pt\n",
    );
}

#[test]
fn shared_callable_temporaries_keep_the_no_escape_rule() {
    let source = "def apply :: (i64 -> i64) -> i64 -> i64 = \\f n -> f n\n\
         def increment :: i64 -> i64 = \\n -> n + 1\n\
         def add :: i64 -> i64 -> i64 = \\left right -> left + right\n\
         def pass :: own (i64 -> i64) -> (i64 -> i64) = \\f -> f\n\
         export def answer :: i64 = \
           apply increment 41 + apply (add 1) 41 + apply (pass increment) 41\n";
    check("2027", source).unwrap();
    let unsafe_escape = source.replace("own (i64 -> i64)", "(i64 -> i64)");
    assert_eq!(check("2027", &unsafe_escape).unwrap_err().code, "E1013");
}

#[test]
fn mutable_conventions_update_scalar_fields_and_noncopy_elements() {
    check(
        "2027",
        "record Named { text: string, n: i64 }\n\
         def bump :: mut i64 -> unit = \\n -> n = n + 1\n\
         def change :: mut Named -> unit = \\item -> { item.text = \"next\"; item.n = item.n + 1 }\n\
         def overwrite :: own 'a -> mut ['a] -> unit = \\value values -> values[0] = value\n\
         export def answer :: i64 = {\n\
           let mut n = 1; bump n;\n\
           let mut item = Named { text: \"old\", n: 2 }; change item;\n\
           let mut texts = [\"first\", \"second\"]; overwrite \"changed\" texts;\n\
           n + item.n + item.text.length + texts[0].length\n\
         }\n",
    )
    .unwrap();
    assert_eq!(
        check("2026", "def bump :: mut i64 -> unit = \\n -> n = n + 1\n")
            .unwrap_err()
            .code,
        "E1005"
    );
    for source in [
        "def bad :: unit -> mut i64 = \\() -> 0\n",
        "record Bad { value: mut i64 }\n",
        "def bad :: own mut i64 -> unit = \\value -> ()\n",
    ] {
        assert_eq!(check("2027", source).unwrap_err().code, "E1005", "{source}");
    }
}

#[test]
fn projected_writes_preserve_sibling_loans_and_require_unique_mutable_owners() {
    for source in [
        "export def answer :: i64 = { let values = [1]; values[0] = 2; values[0] }\n",
        "def change :: ref [i64] -> unit = \\values -> values[0] = 2\n",
        "record Holder { borrowed: ref i64, n: i64 }\n\
         export def answer :: i64 = { let mut value = 1; \
         let mut holder = Holder { borrowed: ref value, n: 0 }; \
         holder.n = 1; value = 2; deref holder.borrowed }\n",
    ] {
        assert_eq!(check("2027", source).unwrap_err().code, "E1014", "{source}");
    }
    let source = "record Holder { text: string, borrowed: ref string }\n\
        export def answer :: i64 = { let outside = \"outside\"; \
        let mut holder = Holder { text: \"inside\", borrowed: ref outside }; \
        holder.borrowed = ref holder.text; holder.borrowed.length }\n";
    assert_eq!(check("2027", source).unwrap_err().code, "E1013");
}

#[test]
fn vector_push_borrows_mutably_only_in_2027_and_tracks_stored_borrows() {
    check(
        "2027",
        "export def answer :: i64 = { let text = \"owned\"; \
         let mut values: Vec<ref string> = Vec.empty(); \
         Vec.push (ref text) values; values[0].length }\n",
    )
    .unwrap();
    check(
        "2027",
        "export def answer :: i64 = { let mut values: Vec<i64> = Vec.empty(); \
         Vec.push 1 values; Vec.push (values[0] + 1) values; \
         let push = Vec.push; push 3 values; values[0] + values[1] + values[2] }\n",
    )
    .unwrap();
    for edition in ["2026", "2027"] {
        check(
            edition,
            "export def answer :: i64 = { \
             let values = Vec.push_owned 42 (Vec.empty()); values[0] }\n",
        )
        .unwrap();
    }
    let source = "def leak :: unit -> Vec<ref string> = \\() -> { \
        let text = \"local\"; let mut values: Vec<ref string> = Vec.empty(); \
        Vec.push (ref text) values; values }\n\
        export def answer :: i64 = (leak ())[0].length\n";
    assert_eq!(check("2027", source).unwrap_err().code, "E1013");
    let source = "export def answer :: i64 = { let text = \"owned\"; \
        let mut values: Vec<ref string> = Vec.empty(); \
        let push = Vec.push; push (ref text) values; values[0].length }\n";
    let error = check("2027", source).unwrap_err();
    assert_eq!(error.code, "E1013");
    assert!(error.message.contains("directly"), "{error:?}");
    for source in [
        "export def answer :: i64 = { let mut values = Vec.of_array [1]; \
         values |> Vec.push (values[0] + 1); values[0] }\n",
        "export def answer :: i64 = { let mut values = Vec.of_array [1]; \
         let element = Vec.at 0 values; Vec.push 2 values; deref element }\n",
    ] {
        assert_eq!(check("2027", source).unwrap_err().code, "E1014", "{source}");
    }
}

#[test]
fn fixed_class_methods_keep_explicit_mutable_conventions() {
    check_with_files(
        "2027",
        "instance Traits.Change<i64> { fn change value = value = value + 1 }\n\
         export def answer :: i64 = { let mut value = 41; Traits.Change.change value; value }\n",
        &[(
            "Traits.tt",
            "class Change<'a> { def change :: mut 'a -> unit }\n",
        )],
    )
    .unwrap();
}

#[test]
fn mixed_editions_keep_both_vector_push_abis_distinct() {
    let old = tsuzuri::parser::parse_with_source(
        "def make :: unit -> Vec<i64> = \\() -> Vec.push 1 (Vec.empty())\n",
        0,
        tsuzuri::syntax::Edition::E2026,
    )
    .unwrap();
    let new = tsuzuri::parser::parse_with_source(
        "export def answer :: i64 = { let mut values = Old.make (); \
         Vec.push 2 values; values[0] + values[1] }\n",
        1,
        tsuzuri::syntax::Edition::E2027,
    )
    .unwrap();
    let inputs = [
        ("Old", &old, tsuzuri::syntax::Edition::E2026),
        ("Main", &new, tsuzuri::syntax::Edition::E2027),
    ]
    .map(|(name, program, edition)| tsuzuri::check::ModuleInput {
        name,
        program,
        origin: tsuzuri::check::ModuleOrigin::User,
        namespace: "",
        entry: name == "Main",
        edition,
        package: None,
    });
    let module = tsuzuri::check::check_modules(&inputs).unwrap();
    let output = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(
        output.contains("Vec.push_mut"),
        "missing mutable push helper"
    );
    assert!(output.contains("Vec.push."), "missing legacy push helper");
    assert_eq!(output, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

#[test]
fn fixed_signature_contracts_are_not_complemented() {
    check(
        "2027",
        "const def size :: string -> i64 = \\s -> s.length\n\
         const Eight: i64 = size \"abcdefgh\"\n\
         def main :: [string] -> i32 = \\args -> args.length as i32\n",
    )
    .unwrap();
    assert_eq!(
        check(
            "2027",
            "const def size :: ref string -> i64 = \\s -> s.length\n"
        )
        .unwrap_err()
        .code,
        "E1026"
    );
    assert_eq!(
        check("2027", "extern def host_len :: string -> i64\n")
            .unwrap_err()
            .code,
        "E1008"
    );
    check("2027", "extern def host_len :: ref string -> i64\n").unwrap();
}

#[test]
fn default_read_diagnostics_explain_ownership_but_explicit_borrows_do_not() {
    for (source, code) in [
        ("def bad :: string -> string = \\s -> s\n", "E1003"),
        (
            "record Named { text: string, n: i64 }\ndef bad :: Named -> string = \\p -> p.text\n",
            "E1012",
        ),
        (
            "record Named { text: string, n: i64 }\ndef bad :: Named -> Named = \\p -> { p with n = 1 }\n",
            "E1012",
        ),
    ] {
        let error = check("2027", source).unwrap_err();
        assert_eq!(error.code, code, "{error:?}");
        assert!(error.message.contains("read-only by default"), "{error:?}");
        assert!(error.message.contains("write 'own "), "{error:?}");
        let explicit = source
            .replace(":: string", ":: ref string")
            .replace(":: Named", ":: ref Named");
        let error = check("2027", &explicit).unwrap_err();
        assert_eq!(error.code, code);
        assert!(!error.message.contains("read-only by default"), "{error:?}");
        check("2026", source).unwrap();
    }
}

#[test]
fn own_survives_formatter_and_documentation_round_trips() {
    let source = "def take::own [|string|]->i64=\\xs->xs.length\n";
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(formatted.contains("own [|string|]"), "{formatted}");
    let second = formatter::format_source("Main.tz", &formatted, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(formatted, second);
    let program = tsuzuri::parser::parse(&formatted).unwrap();
    assert!(tsuzuri::docgen::render_module("Main", &program).contains("own [|string|]"));
    check("2027", &formatted).unwrap();
}

#[test]
fn mut_survives_formatter_and_documentation_round_trips() {
    let source = "def bump::mut i64->unit=\\n->n=n+1\n";
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(formatted.contains("mut i64"), "{formatted}");
    assert_eq!(
        formatted,
        formatter::format_source("Main.tz", &formatted, SourceKind::Code)
            .unwrap()
            .formatted
    );
    let program = tsuzuri::parser::parse(&formatted).unwrap();
    assert!(tsuzuri::docgen::render_module("Main", &program).contains("mut i64"));
    check("2027", &formatted).unwrap();
}

#[test]
fn own_is_accepted_on_fixed_class_method_parameters() {
    let source = "instance Traits.Take<string> { fn take text = text.length }\n\
        export def answer :: i64 = Traits.Take.take \"abc\"\n";
    let declaration = "class Take<'a> { def take :: own 'a -> i64 }\n";
    let emit = |edition, declaration: &str| {
        let module = check_with_files(edition, source, &[("Traits.tt", declaration)])
            .unwrap_or_else(|error| panic!("{error:?}"));
        llvm::emit(&module, llvm::Entry::Library).unwrap()
    };
    assert_eq!(emit("2026", declaration), emit("2027", declaration));
    assert_eq!(
        emit("2026", declaration),
        emit("2026", &declaration.replace("own ", ""))
    );
    for signature in ["'a -> own i64", "(own 'a -> i64) -> i64"] {
        let source = format!("class Bad<'a> {{ def bad :: {signature} }}\n");
        assert_eq!(
            check_with_files("2027", "", &[("Traits.tt", &source)])
                .unwrap_err()
                .code,
            "E1005"
        );
    }
}

#[test]
fn explicit_record_regions_keep_their_existing_parameter_contract() {
    let source = "record Named {r s} { left: ref {r} string, right: ref {s} string, name: string }\n\
        def size {r s} :: Named {r s} -> i64 = \\p -> p.name.length\n";
    assert_eq!(ir("2026", source), ir("2027", source));
}

#[test]
fn unrelated_type_errors_do_not_suggest_own() {
    let error = check("2027", "def bad :: string -> i64 = \\text -> text\n").unwrap_err();
    assert_eq!(error.code, "E1003");
    assert!(!error.message.contains("read-only by default"), "{error:?}");
    assert!(!error.message.contains("write 'own"), "{error:?}");
}

#[test]
fn own_accepts_every_type_primary_starter() {
    for ty in [
        "own dyn Traits.Shape",
        "own (dyn Traits.Shape)",
        "(own dyn Traits.Shape)",
        "(own fn(i64) -> i64)",
        "(own (fn(i64) -> i64))",
        "own &&string",
        "own (&&string)",
        "(own &&string)",
        "own ref ref string",
        "own (ref ref string)",
        "(own ref ref string)",
        "own Vec<Vec<i64>>",
        "own Vec<Vec<Vec<i64>>>",
        "own (Vec<Vec<i64>>)",
    ] {
        let source = format!("def accept :: {ty} -> i64 = \\_value -> 0\n");
        // A22: edition 2027 spells `&&T` as `ref ref T`.
        let editions: &[&str] = if ty.contains('&') {
            &["2026"]
        } else {
            &["2026", "2027"]
        };
        for &edition in editions {
            check_with_files(
                edition,
                &source,
                &[(
                    "Traits.tt",
                    "class Shape<'a> { def area :: ref 'a -> i64 }\n",
                )],
            )
            .unwrap_or_else(|error| panic!("{edition}, {ty}: {error:?}"));
        }
    }
}

/// The lines of a source, each ended by a newline.
fn program(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// A32: `pick n` chooses between a declared function and a lambda written where the declared
/// function's type, with its conventions, is expected.
fn chooser(declarations: &[&str], body: &[&str]) -> String {
    let mut lines = declarations.to_vec();
    lines.push("export def pick :: i64 -> i64 = \\n ->");
    lines.extend(body);
    program(&lines)
}

const TEXT_LENGTH: &str = "def text_length :: string -> i64 = \\text -> text.length";
const PURE_TEXT_LENGTH: &str = "pure def text_length :: string -> i64 = \\text -> text.length";
const BOTH: &str = "def both :: string -> string -> i64 = \\a b -> a.length + b.length";
const COUNTER: &str = "record Counter { count: i64 }";

#[test]
fn annotated_reference_lambdas_meet_convention_function_types() {
    let accepted = [
        // `if`, with the type of the bound form and a `ref string` local in the body.
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\(text: ref string) -> { let view: ref string = text; view.length * 10 }",
                "    let probe: ref string -> i64 = chosen",
                "    let text = \"hello\"",
                "    probe (ref text)",
            ],
        ),
        // `match`
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let chosen =",
                "        match n with",
                "        | 0 -> text_length",
                "        | _ -> \\(text: ref string) -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        // a generic record
        chooser(
            &["record Box<'a> { value: 'a }", TEXT_LENGTH],
            &[
                "    let first = Box { value: text_length }",
                "    let chosen = if n > 0 then first else Box { value: \\(text: ref string) -> text.length * 10 }",
                "    let text = \"hello\"",
                "    let f = chosen.value",
                "    f (ref text)",
            ],
        ),
        // `Vec`
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let mut readers = Vec.empty()",
                "    Vec.push text_length readers",
                "    Vec.push (\\(text: ref string) -> text.length * 10 + n) readers",
                "    let text = \"hello\"",
                "    let second = readers[1]",
                "    second (ref text)",
            ],
        ),
        // [Own, Mut]
        chooser(
            &[
                COUNTER,
                "def bump :: i64 -> mut Counter -> unit = \\step counter -> counter.count = counter.count + step",
            ],
            &[
                "    let change = if n > 0 then bump else \\(step: i64) (counter: ref mut Counter) -> counter.count = counter.count + step * 100",
                "    let mut counter = Counter { count: 1 }",
                "    change 3 (ref mut counter)",
                "    counter.count",
            ],
        ),
        // [Mut] alone
        chooser(
            &[
                COUNTER,
                "def touch :: mut Counter -> unit = \\counter -> counter.count = counter.count + 1",
            ],
            &[
                "    let change = if n > 0 then touch else \\(counter: ref mut Counter) -> counter.count = counter.count + 100",
                "    let mut counter = Counter { count: 1 }",
                "    change (ref mut counter)",
                "    counter.count",
            ],
        ),
        // a guard
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\(text: ref string) when text.length > 0 -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        // two annotated parameters, a name and an annotation, a value annotation and a reference
        chooser(
            &[BOTH],
            &[
                "    let two = if n > 0 then both else \\(a: ref string) (b: ref string) -> a.length * 10 + b.length * 100",
                "    let mixed = if n > 0 then both else \\a (b: ref string) -> a.length * 10 + b.length * 100",
                "    let valued = if n > 0 then both else \\(a: string) (b: ref string) -> a.length * 10 + b.length * 100",
                "    let left = \"hello\"",
                "    let right = \"hi\"",
                "    two (ref left) (ref right) + mixed (ref left) (ref right) + valued (ref left) (ref right)",
            ],
        ),
        // a destructuring pattern under a reference annotation
        chooser(
            &[
                "def pair_size :: (string * string) -> i64 = \\pair -> match pair with | (a, b) -> a.length + b.length",
            ],
            &[
                "    let chosen = if n > 0 then pair_size else \\((a, b): ref (string * string)) -> a.length * 10 + b.length * 100",
                "    let pair = (\"hello\", \"hi\")",
                "    chosen (ref pair)",
            ],
        ),
        // a wildcard
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\(_: ref string) -> 7",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        // an array against a generic read, and an exclusive array
        chooser(
            &[
                "def first_size :: ['a] -> i64 = \\xs -> xs.length",
                "def put :: mut [i64] -> unit = \\values -> values[0] = 7",
            ],
            &[
                "    let counted = if n > 0 then first_size else \\(xs: ref [i64]) -> xs.length * 10",
                "    let change = if n > 0 then put else \\(values: ref mut [i64]) -> values[0] = 9",
                "    let mut values = [1, 2, 3, 4, 5]",
                "    let size = counted (ref values)",
                "    change (ref mut values)",
                "    size + values[0]",
            ],
        ),
        // a type alias of a reference
        chooser(
            &["type Borrowed<'a> = ref 'a", TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\(text: Borrowed<string>) -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
    ];
    for source in &accepted {
        check("2027", source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    let pure = [
        chooser(
            &[PURE_TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\(text: ref string) -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        chooser(
            &[PURE_TEXT_LENGTH],
            &[
                "    let chosen =",
                "        match n with",
                "        | 0 -> text_length",
                "        | _ -> \\(text: ref string) -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        chooser(
            &["record Box<'a> { value: 'a }", PURE_TEXT_LENGTH],
            &[
                "    let first = Box { value: text_length }",
                "    let chosen = if n > 0 then first else Box { value: \\(text: ref string) -> text.length * 10 }",
                "    let text = \"hello\"",
                "    let f = chosen.value",
                "    f (ref text)",
            ],
        ),
        chooser(
            &[PURE_TEXT_LENGTH],
            &[
                "    let mut readers = Vec.empty()",
                "    readers = Vec.push text_length readers",
                "    readers = Vec.push (\\(text: ref string) -> text.length * 10 + n) readers",
                "    let text = \"hello\"",
                "    let second = readers[1]",
                "    second (ref text)",
            ],
        ),
    ];
    for source in &pure {
        check("2026", source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}

/// The functions that the lambdas of `ir` became, and their apply thunks.
fn lambda_functions(ir: &str) -> Vec<&str> {
    ir.split("\n\n")
        .filter(|block| {
            block.starts_with("define ") && block.lines().next().unwrap().contains("$lambda.")
        })
        .collect()
}

#[test]
fn annotated_reference_lambdas_compile_like_their_bound_form() {
    for (declarations, direct, bound, rest) in [
        (
            vec![TEXT_LENGTH],
            "    let chosen = if n > 0 then text_length else \\(text: ref string) -> text.length * 10",
            [
                "    let scaled = \\(text: ref string) -> text.length * 10",
                "    let chosen = if n > 0 then text_length else scaled",
            ],
            vec!["    let text = \"hello\"", "    chosen (ref text)"],
        ),
        (
            vec![
                COUNTER,
                "def bump :: i64 -> mut Counter -> unit = \\step counter -> counter.count = counter.count + step",
            ],
            "    let chosen = if n > 0 then bump else \\(step: i64) (counter: ref mut Counter) -> counter.count = counter.count + step * 100",
            [
                "    let scaled = \\(step: i64) (counter: ref mut Counter) -> counter.count = counter.count + step * 100",
                "    let chosen = if n > 0 then bump else scaled",
            ],
            vec![
                "    let mut counter = Counter { count: 1 }",
                "    chosen 3 (ref mut counter)",
                "    counter.count",
            ],
        ),
    ] {
        let mut direct_body = vec![direct];
        direct_body.extend(&rest);
        let mut bound_body = bound.to_vec();
        bound_body.extend(&rest);
        let direct = ir("2027", &chooser(&declarations, &direct_body));
        let bound = ir("2027", &chooser(&declarations, &bound_body));
        let lambdas = lambda_functions(&direct);
        assert!(!lambdas.is_empty(), "{direct}");
        assert_eq!(lambdas, lambda_functions(&bound));
    }
}

#[test]
fn annotated_reference_def_parameters_meet_their_conventions() {
    for (edition, source) in [
        (
            "2027",
            program(&[
                COUNTER,
                "def bump :: i64 -> mut Counter -> unit = \\(step: i64) (counter: ref mut Counter) -> counter.count = counter.count + step",
                "export def answer :: i64 =",
                "    let mut counter = Counter { count: 1 }",
                "    bump 3 counter",
                "    counter.count",
            ]),
        ),
        (
            "2027",
            program(&[
                "def size :: 'a -> i64 = \\(value: ref 'a) -> 3",
                "export def answer :: i64 =",
                "    let text = \"hello\"",
                "    size text + text.length",
            ]),
        ),
        (
            "2026",
            program(&[
                "pure def size :: 'a -> i64 = \\(value: ref 'a) -> 3",
                "export def answer :: i64 =",
                "    let text = \"hello\"",
                "    size text + text.length",
            ]),
        ),
        (
            "2027",
            program(&[
                "def both :: string -> mut [i64] -> unit = \\(a: ref string) (b: ref mut [i64]) -> b[0] = a.length",
                "export def answer :: i64 =",
                "    let left = \"hello\"",
                "    let mut values = [0]",
                "    both left values",
                "    values[0]",
            ]),
        ),
    ] {
        check(edition, &source).unwrap_or_else(|error| panic!("{edition}\n{source}\n{error:?}"));
    }
    // A function annotated `ref (A -> B)` binds a reference, which cannot be called, as in a
    // lambda bound by `let`.
    for (edition, source) in [
        (
            "2027",
            "def apply :: (i64 -> i64) -> i64 -> i64 = \\(f: ref (i64 -> i64)) n -> f n\n",
        ),
        (
            "2026",
            "pure def apply :: pure (i64 -> i64) -> i64 -> i64 = \\(f: ref (pure (i64 -> i64))) n -> f n\n",
        ),
        (
            "2027",
            "export def answer :: i64 = { let g = \\(f: ref (i64 -> i64)) (n: i64) -> f n; 0 }\n",
        ),
    ] {
        assert_eq!(
            check(edition, source).unwrap_err().code,
            "E1005",
            "{source}"
        );
    }
}

#[test]
fn lambda_parameters_without_reference_annotations_keep_reading_values() {
    // A value annotation, no annotation, and a destructuring pattern still read the value: the
    // lambda has the type of its expected type, and the body sees a `string`.
    for lambda in [
        "\\(text: string) -> text.length * 10",
        "\\text -> text.length * 10",
    ] {
        let reads = chooser(
            &[TEXT_LENGTH],
            &[
                &format!("    let chosen = if n > 0 then text_length else {lambda}"),
                "    let probe: ref string -> i64 = chosen",
                "    let text = \"hello\"",
                "    probe (ref text)",
            ],
        );
        check("2027", &reads).unwrap_or_else(|error| panic!("{reads}\n{error:?}"));
        let body = reads.replace(
            "text.length * 10",
            "{ let view: ref string = text; view.length * 10 }",
        );
        assert_eq!(check("2027", &body).unwrap_err().code, "E1003", "{body}");
    }
    let destructures = chooser(
        &[
            "def pair_size :: (string * string) -> i64 = \\pair -> match pair with | (a, b) -> a.length + b.length",
        ],
        &[
            "    let chosen = if n > 0 then pair_size else \\(a, b) -> a.length * 10 + b.length * 100",
            "    let probe: ref (string * string) -> i64 = chosen",
            "    let pair = (\"hello\", \"hi\")",
            "    probe (ref pair)",
        ],
    );
    check("2027", &destructures).unwrap_or_else(|error| panic!("{destructures}\n{error:?}"));
    // An explicit `ref string` parameter, and a concrete read parameter of a `def`.
    for source in [
        chooser(
            &["def text_length :: ref string -> i64 = \\text -> text.length"],
            &[
                "    let chosen = if n > 0 then text_length else \\(text: ref string) -> text.length * 10",
                "    let text = \"hello\"",
                "    chosen (ref text)",
            ],
        ),
        program(&[
            "def text_length :: string -> i64 = \\(text: ref string) -> text.length",
            "export def answer :: i64 =",
            "    let text = \"hello\"",
            "    text_length text + text.length",
        ]),
    ] {
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    // Where the parameter's type may itself be a reference, the annotation is still compared with
    // the value read through the parameter: a generic read gives `ref ref string`, and a `Vec`
    // whose element type was fixed first takes the `ref string` lambda for its `ref ref string`.
    let generic = chooser(
        &["def inspect :: 'a -> i64 = \\_value -> 3"],
        &[
            "    let chosen = if n > 0 then inspect else \\(text: ref string) -> text.length * 10",
            "    let probe: ref (ref string) -> i64 = chosen",
            "    let text = \"hello\"",
            "    let r = ref text",
            "    probe r",
        ],
    );
    check("2027", &generic).unwrap_or_else(|error| panic!("{generic}\n{error:?}"));
    let single = generic.replace("ref (ref string) -> i64", "ref string -> i64");
    assert_eq!(
        check("2027", &single).unwrap_err().code,
        "E1003",
        "{single}"
    );
    let fixed_first = chooser(
        &["def inspect :: 'a -> i64 = \\_value -> 3"],
        &[
            "    let mut readers = Vec.empty()",
            "    Vec.push inspect readers",
            "    let scaled = \\(text: ref (ref string)) -> text.length * 10",
            "    Vec.push scaled readers",
            "    Vec.push (\\(text: ref string) -> text.length * 100 + n) readers",
            "    let text = \"hello\"",
            "    let r = ref text",
            "    let third = readers[2]",
            "    third r",
        ],
    );
    check("2027", &fixed_first).unwrap_or_else(|error| panic!("{fixed_first}\n{error:?}"));
    // Mismatches stay errors: an exclusive reference where the convention reads, a value where
    // it writes, and a reference where the function type has no convention (out of scope): the
    // argument of `apply`, and the second stage of a `def` that binds only its first parameter,
    // whose type is `ref string -> string -> i64`.
    for (source, code) in [
        (
            chooser(
                &[TEXT_LENGTH],
                &[
                    "    let chosen = if n > 0 then text_length else \\(text: ref mut string) -> text.length * 10",
                    "    let text = \"hello\"",
                    "    chosen (ref text)",
                ],
            ),
            "E1003",
        ),
        (
            chooser(
                &[
                    COUNTER,
                    "def touch :: mut Counter -> unit = \\counter -> counter.count = counter.count + 1",
                ],
                &[
                    "    let change = if n > 0 then touch else \\(counter: Counter) -> counter.count = counter.count + 100",
                    "    let mut counter = Counter { count: 1 }",
                    "    change (ref mut counter)",
                    "    counter.count",
                ],
            ),
            "E1014",
        ),
        (
            program(&[
                "def apply :: (string -> i64) -> i64 = \\f -> f \"hello\"",
                "export def answer :: i64 = apply (\\(text: ref string) -> text.length * 10)",
            ]),
            "E1003",
        ),
        (
            program(&[
                "def both :: string -> string -> i64 = \\(a: ref string) -> \\(b: ref string) -> a.length + b.length * 10",
                "export def answer :: i64 =",
                "    let left = \"hello\"",
                "    let right = \"hi\"",
                "    both left right",
            ]),
            "E1003",
        ),
    ] {
        assert_eq!(check("2027", &source).unwrap_err().code, code, "{source}");
    }
}

#[test]
fn annotated_reference_parameters_keep_the_borrow_rules() {
    // The reference that an annotated parameter binds is checked like that of the bound lambda:
    // it may be returned while the argument lives, and not past it.
    let returned = chooser(
        &["def same_text :: string -> ref string = \\text -> text"],
        &[
            "    let chosen = if n > 0 then same_text else \\(text: ref string) -> text",
            "    let text = \"hello\"",
            "    let view = chosen (ref text)",
            "    view.length",
        ],
    );
    check("2027", &returned).unwrap_or_else(|error| panic!("{returned}\n{error:?}"));
    let escaping = returned.replace(
        "    let text = \"hello\"\n    let view = chosen (ref text)\n",
        "    let view = { let text = \"hello\"; chosen (ref text) }\n",
    );
    assert_ne!(escaping, returned);
    assert_eq!(
        check("2027", &escaping).unwrap_err().code,
        "E1013",
        "{escaping}"
    );
    let generic = program(&[
        "def keep :: 'a -> ref 'a = \\(value: ref 'a) -> value",
        "export def answer :: i64 =",
        "    let text = \"hello\"",
        "    let view = keep text",
        "    view.length",
    ]);
    check("2027", &generic).unwrap_or_else(|error| panic!("{generic}\n{error:?}"));
    let generic_escaping = program(&[
        "def keep :: 'a -> ref 'a = \\(value: ref 'a) -> value",
        "export def answer :: i64 =",
        "    let view = { let text = \"hello\"; keep text }",
        "    view.length",
    ]);
    assert_eq!(
        check("2027", &generic_escaping).unwrap_err().code,
        "E1013",
        "{generic_escaping}"
    );
    let exclusive = program(&[
        COUNTER,
        "def grab :: mut Counter -> ref mut Counter = \\(counter: ref mut Counter) -> counter",
        "export def answer :: i64 =",
        "    let mut a = Counter { count: 1 }",
        "    let r = grab a",
        "    a.count = 5",
        "    r.count",
    ]);
    assert_eq!(
        check("2027", &exclusive).unwrap_err().code,
        "E1014",
        "{exclusive}"
    );
}

#[test]
fn patterns_that_bind_a_moved_value_twice_name_no_hidden_parameter() {
    // Review 3 of A32: an annotated `ref mut T` pattern may now be an `as` pattern in a [Mut]
    // position, which binds the exclusive reference twice. The `E1012` names the pattern, not the
    // hidden parameter `$lambdaN` that the lambda gets for it (nobody can write `ref $lambdaN`).
    // The bound lambda, the explicit `ref mut` signature and an owned value had the same wording
    // before A32.
    let message = "use of moved or partially moved value: this parameter's pattern binds the value, \
                   or a part of it, to more than one name";
    let help = "a value that is not Copy, such as an owned value or a 'ref mut' reference, moves \
                into the first name that binds it, and an 'as' or '&' pattern cannot bind it again; \
                keep only one of the names";
    let bump = "def bump :: i64 -> mut Counter -> unit = \\step c -> c.count = c.count + step";
    for (source, marker) in [
        (
            chooser(
                &[COUNTER, bump],
                &[
                    "    let chosen = if n > 0 then bump else \\(step: i64) ((c as d): ref mut Counter) -> c.count = c.count + step * 100",
                    "    let mut counter = Counter { count: 1 }",
                    "    chosen 3 counter",
                    "    counter.count",
                ],
            ),
            "(step: i64) ",
        ),
        (
            program(&[
                COUNTER,
                "def bump :: i64 -> mut Counter -> unit = \\(step: i64) ((c as d): ref mut Counter) -> c.count = c.count + step",
            ]),
            "(step: i64) ",
        ),
        (
            program(&[
                "def put :: mut [(unit -> i64)] -> [i64] -> unit = \\((s as slot): ref mut [(unit -> i64)]) (xs: ref [i64]) -> Array.write 0 (\\() -> xs[0] * 1000 + xs.length) slot",
            ]),
            "= \\",
        ),
        (
            chooser(
                &[COUNTER],
                &[
                    "    let chosen = \\(step: i64) ((c as d): ref mut Counter) -> c.count = c.count + step * 100",
                    "    let mut counter = Counter { count: 1 }",
                    "    chosen 3 (ref mut counter)",
                    "    counter.count",
                ],
            ),
            "(step: i64) ",
        ),
        (
            program(&[
                COUNTER,
                "def bump :: i64 -> ref mut Counter -> unit = \\(step: i64) ((c as d): ref mut Counter) -> c.count = c.count + step",
            ]),
            "(step: i64) ",
        ),
        (
            program(&[
                "export def answer :: i64 = { let f = \\((s as t): string) -> s.length + t.length; f \"abc\" }",
            ]),
            "let f = \\",
        ),
        (
            program(&[
                "export def answer :: i64 = { let f = \\(((a, b) as whole): (string * string)) -> a.length + b.length; f (\"ab\", \"c\") }",
            ]),
            "let f = \\",
        ),
        (
            program(&[
                "export def answer :: i64 = { let f = \\((s & t): string) -> s.length + t.length; f \"abc\" }",
            ]),
            "let f = \\",
        ),
    ] {
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str(), error.help.as_deref()),
            ("E1012", message, Some(help)),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, marker),
            "{source}"
        );
    }
    // A name that the program wrote is still named, with the `ref` that it can write.
    let named = program(&[
        "export def answer :: i64 =",
        "    let text = \"abc\"",
        "    match text with",
        "        | (s as t) -> s.length + t.length",
    ]);
    let error = check("2027", &named).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str(), error.help.as_deref()),
        (
            "E1012",
            "use of moved or partially moved value 'text'",
            Some("borrow the value with 'ref text' or move it last")
        ),
        "{named}"
    );
}

#[test]
fn function_clauses_after_a_moving_pattern_are_named_as_the_second_use() {
    // Review 4 of A32: function clauses after the arrow (`\(s: string) -> | t -> …`) match the
    // parameter itself again, after its pattern moved a value that is not Copy. The `E1012` points
    // at the clauses and says so; it does not blame an `as` or `&` pattern that the program does
    // not have. The annotated `ref mut` spellings are new with A32 (`E1003` before); the others
    // named the hidden parameter `$lambdaN` before.
    let message = "use of moved or partially moved value: these clauses match the parameter again \
                   after its pattern moved the value";
    let help = "a value that is not Copy, such as an owned value or a 'ref mut' reference, moves \
                into the names of the parameter's pattern, so the clauses after '->' cannot match \
                the parameter again; match those names with 'match' in the body instead, or write \
                clauses that only test conditions on them, as in '| x > 0 -> …'";
    let bump = "def bump :: mut Counter -> i64 = \\c -> c.count";
    // Each program, the marker that ends right before its pattern, the marker that ends right
    // before its first clause, and whether the pattern moved only parts of the value (the note
    // then points at the part that moved last).
    for (source, pattern, clauses, parts) in [
        (
            program(&[
                "def classify :: own string -> i64 = \\(s: string) ->",
                "    | t when t.length > 3 -> 1",
                "    | _ -> 0",
            ]),
            "= \\",
            "->\n    ",
            false,
        ),
        (
            program(&[
                COUNTER,
                "def bump :: mut Counter -> i64 = \\(c: ref mut Counter) ->",
                "    | d -> { d.count = d.count + 1; d.count }",
            ]),
            "= \\",
            "->\n    ",
            false,
        ),
        (
            chooser(
                &[COUNTER, bump],
                &[
                    "    let chosen = if n > 100 then bump else \\(c: ref mut Counter) ->",
                    "        | d -> { d.count = d.count + 1; d.count }",
                    "    let mut c = Counter { count: n }",
                    "    chosen c",
                ],
            ),
            "else \\",
            "->\n        ",
            false,
        ),
        (
            program(&[
                COUNTER,
                "def bump :: ref mut Counter -> i64 = \\(c: ref mut Counter) ->",
                "    | d -> { d.count = d.count + 1; d.count }",
            ]),
            "= \\",
            "->\n    ",
            false,
        ),
        (
            program(&[
                "export def answer :: i64 =",
                "    let f = \\(s: string) ->",
                "        | t when t.length > 3 -> 1",
                "        | _ -> 0",
                "    f (to_string 12345)",
            ]),
            "let f = \\",
            "->\n        ",
            false,
        ),
        (
            program(&[
                "def classify :: i64 -> own string -> i64 = \\n (s: string) ->",
                "    | (0, t) when t.length > 3 -> 1",
                "    | _ -> n",
            ]),
            "\\n ",
            "->\n    ",
            false,
        ),
        (
            program(&[
                "def classify :: own (string * string) -> i64 = \\((a, b): (string * string)) ->",
                "    | (x, y) -> x.length + y.length + a.length",
            ]),
            "\\((a, ",
            "->\n    ",
            true,
        ),
        (
            program(&[
                "def classify :: own string -> i64 = \\mut (s: string) ->",
                "    | t when t.length > 3 -> 1",
                "    | _ -> 0",
            ]),
            "= \\mut ",
            "->\n    ",
            false,
        ),
    ] {
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str(), error.help.as_deref()),
            ("E1012", message, Some(help)),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, clauses),
            "{source}"
        );
        assert_eq!(
            error
                .related
                .iter()
                .map(|related| (related.message.as_str(), related.span.start))
                .collect::<Vec<_>>(),
            [(
                if parts {
                    "part of the value moved here"
                } else {
                    "value moved here"
                },
                pattern_offset(&source, pattern)
            )],
            "{source}"
        );
    }
    // A pattern that binds the value twice is still reported as such, before the clauses.
    let twice = program(&[
        "def classify :: own string -> i64 = \\((s as t): string) ->",
        "    | u when u.length > 3 -> 1",
        "    | _ -> 0",
    ]);
    let error = check("2027", &twice).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str(), error.span.start),
        (
            "E1012",
            "use of moved or partially moved value: this parameter's pattern binds the value, or a \
             part of it, to more than one name",
            pattern_offset(&twice, "= \\")
        ),
        "{twice}"
    );
    // The ways out that the help names are accepted: a `match` on the pattern's names in the body,
    // and clauses that only test conditions (with the annotated `ref mut` of A32 too).
    for source in [
        program(&[
            "def classify :: own string -> i64 = \\(s: string) ->",
            "    match s with",
            "    | t when t.length > 3 -> 1",
            "    | _ -> 0",
        ]),
        program(&[
            "def classify :: own string -> i64 = \\(s: string) ->",
            "    | s.length > 3 -> 1",
            "    | otherwise -> 0",
        ]),
        program(&[
            "def classify :: own string -> i64 = \\(s: string) ->",
            "    | _ when s.length > 3 -> 1",
            "    | _ -> 0",
        ]),
        program(&[
            COUNTER,
            "def bump :: mut Counter -> i64 = \\(c: ref mut Counter) ->",
            "    match c with",
            "    | d -> { d.count = d.count + 1; d.count }",
        ]),
        program(&[
            COUNTER,
            "def bump :: mut Counter -> i64 = \\(c: ref mut Counter) ->",
            "    | c.count > 3 -> { c.count = c.count + 1; c.count }",
            "    | otherwise -> 0",
        ]),
    ] {
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}

/// The offset of the pattern that follows `marker` in `source` (the `(` after the `\` or space
/// that `marker` ends with).
fn pattern_offset(source: &str, marker: &str) -> usize {
    source
        .find(marker)
        .unwrap_or_else(|| panic!("{marker}\n{source}"))
        + marker.len()
}

#[test]
fn mistyped_reference_annotations_keep_their_diagnostics() {
    // A reference annotation that does not match the parameter's reference type keeps reading the
    // value (review 1 of A32): the pattern reports what it reported before A32, with the `ref`
    // that the annotation has, at the annotated pattern.
    let records =
        "record Big { items: [i64], label: string }\nrecord Other { items: [i64], label: string }";
    let touch = "def touch :: mut Counter -> unit = \\counter -> counter.count = counter.count + 1";
    for (source, marker, message) in [
        (
            chooser(
                &[TEXT_LENGTH],
                &[
                    "    let chosen = if n > 0 then text_length else \\(text: ref [string]) -> 1",
                    "    let text = \"hello\"",
                    "    chosen (ref text)",
                ],
            ),
            "else \\",
            "expected ref [string], found string",
        ),
        (
            chooser(
                &[records, "def size :: Big -> i64 = \\b -> b.items.length"],
                &[
                    "    let chosen = if n > 0 then size else \\(b: ref Other) -> 1",
                    "    let b = Big { items: [1], label: \"x\" }",
                    "    chosen (ref b)",
                ],
            ),
            "else \\",
            "expected ref Main.Other, found Main.Big",
        ),
        (
            chooser(
                &[TEXT_LENGTH],
                &[
                    "    let chosen = if n > 0 then text_length else \\(text: ref mut string) -> text.length * 10",
                    "    let text = \"hello\"",
                    "    chosen (ref text)",
                ],
            ),
            "else \\",
            "expected ref mut string, found string",
        ),
        (
            chooser(
                &[COUNTER, touch],
                &[
                    "    let change = if n > 0 then touch else \\(counter: ref Counter) -> ()",
                    "    let mut counter = Counter { count: 1 }",
                    "    change (ref mut counter)",
                    "    counter.count",
                ],
            ),
            "else \\",
            "expected ref Main.Counter, found Main.Counter",
        ),
        (
            program(&["def size :: 'a -> i64 = \\(value: ref string) -> value.length"]),
            "= \\",
            "expected ref string, found 'a",
        ),
        (
            program(&["def size :: 'a -> i64 = \\(value: ref mut 'a) -> 3"]),
            "= \\",
            "expected ref mut 'a, found 'a",
        ),
        (
            program(&[
                COUNTER,
                "def bump :: i64 -> mut Counter -> unit = \\(step: i64) (counter: ref [Counter]) -> ()",
            ]),
            "(step: i64) ",
            "expected ref [Main.Counter], found Main.Counter",
        ),
    ] {
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1003", message),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, marker),
            "{source}"
        );
    }
    // A function parameter annotated `ref (i64 -> i64)` binds a reference: calling it is `E1005`,
    // whose help says to dereference it, and `(deref f) n` calls the function. The help names no
    // callee or argument (review 2 of A32), so it fits a pipeline into a reference as well.
    let call = "def apply :: (i64 -> i64) -> i64 -> i64 = \\(f: ref (i64 -> i64)) n -> f n\n";
    let error = check("2027", call).unwrap_err();
    assert_eq!(error.code, "E1005", "{call}");
    assert_eq!(
        error.help.as_deref(),
        Some(
            "a value of type ref (i64 -> i64) is a reference to a function, not a function; \
             dereference it with `deref` before calling it"
        )
    );
    let explicit = "def apply :: ref (i64 -> i64) -> i64 -> i64 = \\f n -> f n\n";
    let piped =
        "export def answer :: i64 = { let g = \\(x: i64) -> x + 1; let r = ref g; 3 |> r }\n";
    for source in [explicit, piped] {
        let other = check("2027", source).unwrap_err();
        assert_eq!(
            (other.code, other.help.as_deref()),
            ("E1005", error.help.as_deref()),
            "{source}"
        );
    }
    let dereferenced = call.replace("-> f n", "-> (deref f) n");
    check("2027", &dereferenced).unwrap_or_else(|error| panic!("{dereferenced}\n{error:?}"));
}

#[test]
fn annotated_reference_read_callbacks() {
    // The callbacks of the Array and List read entry points have the [Read] convention, so a lambda
    // passed to them may annotate its element parameter `ref string`, and its body sees a reference.
    for (module, values) in [("Array", "[\"a\", \"bc\"]"), ("List", "[|\"a\", \"bc\"|]")] {
        for call in [
            format!(
                "({module}.map (\\(text: ref string) -> {{ let view: ref string = text; view.length }}) texts).length"
            ),
            format!(
                "(texts |> {module}.map (\\(text: ref string) -> {{ let view: ref string = text; view.length }})).length"
            ),
            format!(
                "({module}.mapi (\\i (text: ref string) -> {{ let view: ref string = text; i + view.length }}) texts).length"
            ),
            format!(
                "{module}.iter (\\(text: ref string) -> {{ let _view: ref string = text; () }}) texts; 0"
            ),
            format!(
                "{module}.iteri (\\_ (text: ref string) -> {{ let _view: ref string = text; () }}) texts; 0"
            ),
            format!(
                "{module}.fold (\\sum (text: ref string) -> {{ let view: ref string = text; sum + view.length }}) 0 texts"
            ),
            format!(
                "(texts |> {module}.fold (\\sum (text: ref string) -> {{ let view: ref string = text; sum + view.length }}) 0)"
            ),
            format!(
                "{module}.fold_back (\\(text: ref string) sum -> {{ let view: ref string = text; sum + view.length }}) 0 texts"
            ),
        ] {
            let source = format!(
                "export def answer :: i64 = {{ let texts = {values}; {call} + texts.length }}\n"
            );
            check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        }
    }
    // The reference may be returned while the array lives, and not past it, as with the lambda
    // bound by `let`.
    let inside = "export def answer :: i64 = { let xs = [\"a\", \"bb\"]; let ys = xs |> Array.map (\\(x: ref string) -> x); ys[1].length }\n";
    check("2027", inside).unwrap_or_else(|error| panic!("{inside}\n{error:?}"));
    for source in [
        "export def answer :: i64 = { let ys = { let xs = [\"a\", \"bb\"]; xs |> Array.map (\\(x: ref string) -> x) }; ys.length }\n",
        "export def answer :: i64 = { let ys = { let xs = [\"a\", \"bb\"]; let f = \\(x: ref string) -> x; xs |> Array.map f }; ys.length }\n",
    ] {
        assert_eq!(check("2027", source).unwrap_err().code, "E1013", "{source}");
    }
}

#[test]
fn annotated_reference_callbacks_over_number_literals() {
    // Review 3 of A32: when a read callback is checked, the element type of a number-literal
    // array is still the open type of the literals. A `ref` annotation whose referent is a number
    // type that the literals can take binds the reference and decides the element type, as the
    // lambda bound by `let` does. Before, the literal failed (`expected an integer, found ref
    // i64`), since the parameter read the value and the pattern made the literal a reference.
    for body in [
        "let ys = [1, 2, 3] |> Array.map (\\(x: ref i64) -> { let view: ref i64 = x; deref view * 2 }); ys[2]",
        "let xs = [1, 2, 3]; xs |> Array.fold (\\acc (x: ref i64) -> acc + deref x) 0",
        "let ys = [|1, 2, 3|] |> List.map (\\(x: ref i64) -> deref x * 2); List.length ys",
        "let ys = Array.map (\\(x: ref i64) -> deref x * 2) [1, 2, 3]; ys[2]",
        "let xs = [1, 2, 3]; let first = xs[0] + 10; (xs |> Array.fold (\\acc (x: ref i64) -> acc + deref x) 0) + first",
        "let xs = [1, 2, 3]; let g = \\() -> xs.length; (xs |> Array.fold (\\acc (x: ref i64) -> acc + deref x) 0) + g ()",
        "let ys = [-1, 2] |> Array.map (\\(x: ref i64) -> deref x * 10); ys[0]",
        "let ys = [1, 2, 3] |> Array.mapi (\\i (x: ref i64) -> i * 100 + deref x); ys[2]",
        "let ys = [250, 3] |> Array.map (\\(x: ref i8u) -> deref x + 1); ys.length",
    ] {
        let source = format!("export def answer :: i64 = {{ {body} }}\n");
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    // The function that the lambda becomes is the one of the bound lambda.
    let direct = ir(
        "2027",
        "export def answer :: i64 = { let ys = [1, 2, 3] |> Array.map (\\(x: ref i64) -> deref x * 2); ys[2] }\n",
    );
    let bound = ir(
        "2027",
        "export def answer :: i64 = { let f = \\(x: ref i64) -> deref x * 2; let ys = [1, 2, 3] |> Array.map f; ys[2] }\n",
    );
    let lambdas = lambda_functions(&direct);
    assert!(!lambdas.is_empty(), "{direct}");
    assert_eq!(lambdas, lambda_functions(&bound));
    // The literals are checked against the type that the annotation decides, as for the bound
    // lambda: 300 is no `i8`.
    for source in [
        "export def answer :: i64 = { let ys = [300, 1] |> Array.map (\\(x: ref i8) -> 1); ys.length }\n",
        "export def answer :: i64 = { let f = \\(x: ref i8) -> 1; let ys = [300, 1] |> Array.map f; ys.length }\n",
    ] {
        let error = check("2027", source).unwrap_err();
        assert_eq!(error.code, "E1009", "{source}\n{}", error.message);
        assert_eq!(error.span.start, source.find("300").unwrap(), "{source}");
    }
    // A referent that the literals cannot take keeps reading the value, so the literal reports
    // what it reported before.
    for (call, message) in [
        (
            "[1, 2, 3] |> Array.map (\\(x: ref string) -> x.length)",
            "expected an integer, found ref string",
        ),
        (
            "[1, 2, 3] |> Array.map (\\(x: ref f64) -> 1)",
            "expected an integer, found ref f64",
        ),
        (
            "[1, 2, 3] |> Array.map (\\(x: ref mut i64) -> 1)",
            "expected an integer, found ref mut i64",
        ),
        (
            "[-1, 2] |> Array.map (\\(x: ref i8u) -> 1)",
            "expected an integer, found ref i8u",
        ),
    ] {
        let source = format!("export def answer :: i64 = {{ let ys = {call}; ys.length }}\n");
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1003", message),
            "{source}"
        );
        assert_eq!(error.span.start, source.find('[').unwrap() + 1, "{source}");
    }
    // So does a referent that another constraint on the literals rejects: `Traits.Show` has an
    // instance for `i32` and none for `i64`.
    let traits = "class Show<'t> {\n    def show :: ref 't -> i64\n}\n";
    let shown = |element: &str| {
        program(&[
            "instance Traits.Show<i32> {",
            "    fn show x = 7",
            "}",
            "export def answer :: i64 =",
            "    let xs = [1, 2, 3]",
            "    let s = Traits.Show.show (ref (xs[0]))",
            &format!(
                "    let total = xs |> Array.fold (\\acc (x: ref {element}) -> acc + (if deref x > 1 then 10 else 1)) 0"
            ),
            "    total * 100 + s",
        ])
    };
    check_with_files("2027", &shown("i32"), &[("Traits.tt", traits)])
        .unwrap_or_else(|error| panic!("{}\n{error:?}", shown("i32")));
    let rejected = shown("i64");
    let error = check_with_files("2027", &rejected, &[("Traits.tt", traits)]).unwrap_err();
    assert_eq!(error.code, "E1015", "{rejected}\n{}", error.message);
}

#[test]
fn annotated_reference_callbacks_over_nested_number_literals() {
    // Review 4 of A32: the open types of number literals may sit inside the element type
    // (`[[1, 2], [3]]` has elements `[?]`). The annotation's match with `ref [?]` gives them a
    // type; when a constraint on them (the literal's own `Integer`) does not hold with it, the
    // parameter keeps reading the value, and the annotated pattern reports what it reported
    // before A32, where the mistake is, not at a literal.
    for (lines, message, marker) in [
        (
            vec![
                "export def answer :: i64 =",
                "    let xs = [[1, 2], [3]]",
                "    let ys = xs |> Array.map (\\(x: ref [string]) -> x.length)",
                "    ys[0]",
            ],
            "expected ref [string], found [an undetermined type]",
            "map (\\",
        ),
        (
            vec![
                "export def answer :: i64 =",
                "    let ys = [[1, 2], [3]] |> Array.map (\\(x: ref [string]) -> x.length)",
                "    ys[0]",
            ],
            "expected ref [string], found [an undetermined type]",
            "map (\\",
        ),
        (
            vec![
                "export def answer :: i64 =",
                "    let ys = [[1, 2], [3]] |> Array.map (\\(x: ref [f64]) -> x.length)",
                "    ys[0]",
            ],
            "expected ref [f64], found [an undetermined type]",
            "map (\\",
        ),
        (
            vec![
                "export def answer :: i64 =",
                "    let ys = [(1, [2, 3]), (4, [5])] |> Array.map (\\(p: ref (i64 * [string])) -> 1)",
                "    ys[0]",
            ],
            "expected ref (i64 * [string]), found (an undetermined type * [an undetermined type])",
            "map (\\",
        ),
    ] {
        let source = program(&lines);
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1003", message),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, marker),
            "{source}"
        );
    }
    // A type that the literals take binds the reference, as at the top level.
    for body in [
        "let ys = [[1, 2], [3]] |> Array.map (\\(x: ref [i64]) -> { let view: ref [i64] = x; view.length * 10 + view[0] }); ys[0] + ys[1] * 100",
        "let ys = [(1, [2, 3]), (4, [5])] |> Array.map (\\(p: ref (i64 * [i64])) -> match p with | (a, b) -> a * 10 + b.length); ys[0] + ys[1] * 100",
    ] {
        let source = format!("export def answer :: i64 = {{ {body} }}\n");
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}

#[test]
fn builtins_that_store_through_exclusive_references_are_named_as_written() {
    // Review 4 of A32: a function value of `Vec.push_mut` over borrowed elements may store an
    // input through an exclusive reference, so it runs only in direct calls. The error names the
    // builtin as the program writes it, not the numbered instance (`Vec.push_mut.3`) that the
    // compiler makes of it. The annotated `else` branch is new with A32 (`E1003` before); the
    // other spellings had the numbered name before.
    let message = "'Vec.push_mut' may store an input through an exclusive reference, so call it \
                   directly with all of its arguments";
    // Each `push` and the marker that ends right before the function value of `Vec.push_mut`.
    for (push, marker) in [
        (
            "    let push = if n > 1000000 then Vec.push_mut else \\(x: ref string) (w: ref mut Vec<ref string>) -> Vec.push_mut x w",
            "then ",
        ),
        (
            "    let push = if n > 1000000 then Vec.push_mut else \\x w -> Vec.push_mut x w",
            "then ",
        ),
        ("    let push = Vec.push_mut", "let push = "),
    ] {
        let source = program(&[
            "export def run :: i64 -> i64 = \\n ->",
            "    let zz = \"zz\"",
            "    let mut v: Vec<ref string> = Vec.empty()",
            push,
            "    push (ref zz) v",
            "    v[0].length",
        ]);
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.kind, error.message.as_str()),
            (
                "E1013",
                Some("region_callback_stores_exclusive_input"),
                message
            ),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, marker),
            "{source}"
        );
    }
}

/// The first error of `source` has `code` and `kind` and starts at the 1-based `line:column`.
fn rejects_at(source: &str, code: &str, kind: &str, at: (usize, usize)) {
    let error = check("2027", source)
        .err()
        .unwrap_or_else(|| panic!("accepted:\n{source}"));
    assert_eq!(
        (error.code, error.kind),
        (code, Some(kind)),
        "{source}\n{}",
        error.message
    );
    let before = &source[..error.span.start];
    let position = (
        before.matches('\n').count() + 1,
        before.len() - before.rfind('\n').map_or(0, |at| at + 1) + 1,
    );
    assert_eq!(position, at, "{source}\n{}", error.message);
}

#[test]
fn annotated_reference_parameters_keep_their_in_place_writes_tracked() {
    // A32 stacked on A31: an in-place write (`Array.write`, the std functions that call it, such
    // as `Array.set` and `Array.fill`) that stores a function holding a loan of an annotated
    // reference parameter adds that loan to the target's owner (A31's store facts), as it does for
    // the spellings that A32 did not change. Each program stores, through the annotated spelling,
    // a function that borrows `local` (or `k`) in `arr`, which outlives the block that owns
    // `local`; its twin is the same program without the annotations, or with explicit `ref`
    // signatures. Both fail where A31's rule for an owner that a block drops reports: at that
    // block, `E1013 assignment_extends_borrow`. The seeds of the two A32 reviews are kept verbatim
    // (`h19`..`h22`, `r2_ag02`..`r2_ag04`, `alias`, `src_side`, `src_side_bound`); until A31 they
    // kept the pre-A32 `E1003` through a guard (review 2 of A32), and a base program with the
    // twin's spelling was accepted.
    let store = "Array.write 0 (\\() -> xs[0] * 1000 + xs.length) slot";
    let put_plain =
        format!("def put :: mut [(unit -> i64)] -> [i64] -> unit = \\slot xs -> {store}");
    let put_annotated = format!(
        "def put :: mut [(unit -> i64)] -> [i64] -> unit = \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
    );
    let put_source_first =
        format!("def put :: [i64] -> mut [(unit -> i64)] -> unit = \\xs slot -> {store}");
    let caller = |declaration: &str, chosen: Option<&str>, call: &str, n: &str| {
        let mut lines = vec![
            declaration.to_string(),
            String::new(),
            "def run :: i64 -> i64 = \\n ->".to_string(),
            "    let mut arr = [\\() -> 0]".to_string(),
        ];
        lines.extend(chosen.map(|line| format!("    let chosen = {line}")));
        lines.push(format!(
            "    {{ let local = [n, n + 1, n + 2, n + 3]; {call} }}"
        ));
        lines.push("    let other = [777, 778, 779, 780, 781, 782]".to_string());
        lines.push("    let g = arr[0]".to_string());
        lines.push("    g () + other[0] * 0".to_string());
        lines.push(String::new());
        lines.push(format!("run {n}"));
        program(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    };
    // h19 and h22: a `def` whose right-hand side lambda annotates its parameters.
    let h19 = caller(&put_plain, None, "put arr local", "5");
    let h22 = caller(&put_annotated, None, "put arr local", "5");
    // h20: both branches of an `if` bound by `let` are annotated lambdas; no type with
    // conventions is expected there, so base accepted it as it is. The twin chooses `put` twice.
    let h20 = program(&[
        "def run :: i64 -> i64 = \\n ->",
        "    let mut arr = [\\() -> 0]",
        &format!(
            "    let chosen = if n > 0 then (\\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}) else (\\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> ())"
        ),
        "    { let local = [n, n + 1, n + 2, n + 3]; chosen (ref mut arr) (ref local) }",
        "    let other = [777, 778, 779, 780, 781, 782]",
        "    let g = arr[0]",
        "    g () + other[0] * 0",
        "",
        "run 5",
    ]);
    let h20_twin = caller(
        &put_plain,
        Some("if n > 0 then put else put"),
        "chosen (ref mut arr) (ref local)",
        "5",
    );
    // h21: an annotated lambda where `put`'s type, with its conventions, is expected.
    let h21 = caller(
        &put_plain,
        Some(&format!(
            "if n > 0 then put else \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
        )),
        "chosen (ref mut arr) (ref local)",
        "(-5)",
    );
    let h21_twin = caller(
        &put_plain,
        Some("put"),
        "chosen (ref mut arr) (ref local)",
        "(-5)",
    );
    // review 1, alias: an explicit `ref mut` signature whose lambda annotates its parameters.
    let alias = caller(
        &format!(
            "def put :: ref mut [(unit -> i64)] -> ref [i64] -> unit = \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
        ),
        None,
        "put (ref mut arr) (ref local)",
        "5",
    );
    let alias_twin = caller(
        &format!("def put :: ref mut [(unit -> i64)] -> ref [i64] -> unit = \\slot xs -> {store}"),
        None,
        "put (ref mut arr) (ref local)",
        "5",
    );
    // review 1, src_side and src_side_bound: the annotated parameter is the source of the store,
    // in an annotated lambda in the `else` branch and in a lambda bound by `let`.
    let src_side = caller(
        &put_source_first,
        Some(&format!(
            "if n > 0 then put else \\(xs: ref [i64]) slot -> {store}"
        )),
        "chosen local arr",
        "(-5)",
    );
    let src_side_bound = {
        let mut lines: Vec<String> = caller(
            &put_source_first,
            Some("if n > 0 then put else other_put"),
            "chosen local arr",
            "(-5)",
        )
        .lines()
        .map(str::to_string)
        .collect();
        lines.insert(
            4,
            format!(
                "    let other_put = \\(xs: ref [i64]) (slot: ref mut [(unit -> i64)]) -> {store}"
            ),
        );
        program(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    };
    let src_side_twin = caller(&put_source_first, Some("put"), "chosen local arr", "(-5)");
    // r2_ag02 (annotated), r2_ag03 (unannotated) and r2_ag04 (explicit `ref` signature): the
    // stored function calls a function parameter that captures `k`.
    let function_store = |declaration: &str, call: &str| {
        program(&[
            declaration,
            "",
            "export def run :: i64 -> i64 = \\n ->",
            "    let mut arr = [\\() -> 0]",
            &format!(
                "    {{ let k = [n, n + 1, n + 2, n + 3]; let g = \\x -> x + k[0] + k.length; {call} }}"
            ),
            "    let other = [777, 778, 779, 780, 781, 782]",
            "    let h = arr[0]",
            "    h () + other[0] * 0",
            "",
            "run 5",
        ])
    };
    let r2_ag02 = function_store(
        "def put :: mut [(unit -> i64)] -> (i64 -> i64) -> unit = \\(slot: ref mut [(unit -> i64)]) (f: ref (i64 -> i64)) -> Array.write 0 (\\() -> (deref f) 1000) slot",
        "put arr g",
    );
    let r2_ag03 = function_store(
        "def put :: mut [(unit -> i64)] -> (i64 -> i64) -> unit = \\slot f -> Array.write 0 (\\() -> f 1000) slot",
        "put arr g",
    );
    let r2_ag04 = function_store(
        "def put :: ref mut [(unit -> i64)] -> ref (i64 -> i64) -> unit = \\slot f -> Array.write 0 (\\() -> (deref f) 1000) slot",
        "put (ref mut arr) (ref g)",
    );
    // Further spellings of A32: a std function that calls `Array.write` (`Array.set`,
    // `Array.fill`), a `match` arm, a generic record, a vector of functions, a generic `def`, and
    // a read callback that stores through a lock.
    let set_store = "Array.set 0 (\\() -> xs[0] * 1000 + xs.length) slot";
    let set = caller(
        &format!("def put :: mut [(unit -> i64)] -> [i64] -> unit = \\slot xs -> {set_store}"),
        Some(&format!(
            "if n > 0 then put else \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {set_store}"
        )),
        "chosen (ref mut arr) (ref local)",
        "(-5)",
    );
    let set_twin = caller(
        &format!("def put :: mut [(unit -> i64)] -> [i64] -> unit = \\slot xs -> {set_store}"),
        Some("put"),
        "chosen (ref mut arr) (ref local)",
        "(-5)",
    );
    let fill_store = "Array.fill 0 1 (\\() -> xs[0] * 1000 + xs.length) slot";
    let fill = caller(
        &format!(
            "def put :: mut [(unit -> i64)] -> [i64] -> unit = \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {fill_store}"
        ),
        None,
        "put arr local",
        "5",
    );
    let fill_twin = caller(
        &format!("def put :: mut [(unit -> i64)] -> [i64] -> unit = \\slot xs -> {fill_store}"),
        None,
        "put arr local",
        "5",
    );
    let arm = caller(
        &put_plain,
        Some(&format!(
            "match n with | 0 -> put | _ -> \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
        )),
        "chosen (ref mut arr) (ref local)",
        "5",
    );
    let arm_twin = caller(
        &put_plain,
        Some("match n with | 0 -> put | _ -> put"),
        "chosen (ref mut arr) (ref local)",
        "5",
    );
    let holder = |value: &str| {
        caller(
            &format!("record Holder<'a> {{ value: 'a }}\n{put_plain}"),
            Some(&format!(
                "if n > 0 then Holder {{ value: put }} else Holder {{ value: {value} }}"
            )),
            "chosen.value (ref mut arr) (ref local)",
            "(-5)",
        )
    };
    let in_holder = holder(&format!(
        "\\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
    ));
    let in_holder_twin = holder("put");
    let vector = |second: &str| {
        caller(
            &put_plain,
            Some(&format!(
                "{{ let mut writers = Vec.empty(); Vec.push put writers; Vec.push ({second}) writers; writers[1] }}"
            )),
            "chosen (ref mut arr) (ref local)",
            "5",
        )
    };
    let in_vector = vector(&format!(
        "\\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
    ));
    let in_vector_twin = vector("put");
    let generic = |put: &str| {
        program(&[
            put,
            "export def run :: i64 -> i64 = \\n ->",
            "    let mut arr = [\\() -> 0]",
            "    { let local = to_string n; put arr local (\\(s: ref string) -> s.length) }",
            "    let other = to_string 987654321",
            "    arr[0] () + other.length * 0",
        ])
    };
    let in_generic = generic(
        "def put :: mut [(unit -> i64)] -> 'a -> (ref 'a -> i64) -> unit = \\(slot: ref mut [(unit -> i64)]) (v: ref 'a) (measure: ref (ref 'a -> i64)) -> Array.write 0 (\\() -> (deref measure) v) slot",
    );
    let in_generic_twin = generic(
        "def put :: mut [(unit -> i64)] -> 'a -> (ref 'a -> i64) -> unit = \\slot v measure -> Array.write 0 (\\() -> measure (ref v)) slot",
    );
    let locked = |parameter: &str| {
        program(&[
            "export def run :: i64 -> i64 = \\n ->",
            "    let m = Mutex.create [\\() -> 0]",
            "    let mr = ref m",
            &format!(
                "    {{ let texts = [to_string n, \"bb\"]; let ks = Array.map (\\{parameter} -> {{ Mutex.with_lock mr (\\slot -> Array.write 0 (\\() -> t.length) slot); 1 }}) texts; () }}"
            ),
            "    Mutex.with_lock mr (\\slot -> slot[0] ())",
        ])
    };
    let read_callback = locked("(t: ref string)");
    let read_callback_twin = locked("t");
    // Each annotated program, then its twin, with the line of the block that owns `local` (or
    // `k`, `texts`), which starts at column 5.
    for (source, line) in [
        (&h22, 5),
        (&h19, 5),
        (&h20, 4),
        (&h20_twin, 6),
        (&h21, 6),
        (&h21_twin, 6),
        (&alias, 5),
        (&alias_twin, 5),
        (&src_side, 6),
        (&src_side_bound, 7),
        (&src_side_twin, 6),
        (&r2_ag02, 5),
        (&r2_ag03, 5),
        (&r2_ag04, 5),
        (&set, 6),
        (&set_twin, 6),
        (&fill, 5),
        (&fill_twin, 5),
        (&arm, 6),
        (&arm_twin, 6),
        (&in_holder, 7),
        (&in_holder_twin, 7),
        (&in_vector, 6),
        (&in_vector_twin, 6),
        (&in_generic, 4),
        (&in_generic_twin, 4),
        (&read_callback, 4),
        (&read_callback_twin, 4),
    ] {
        rejects_at(source, "E1013", "assignment_extends_borrow", (line, 5));
    }
    // Moving the owner while the target holds the stored loan conflicts with it (`E1014`), at the
    // move, with the annotations and without.
    let moved = |put: &str| {
        program(&[
            put,
            "def consume :: own string -> i64 = \\text -> text.length",
            "export def run :: i64 -> i64 = \\n ->",
            "    let local = to_string n",
            "    let mut arr = [\\() -> 0]",
            "    put arr local",
            "    let c = consume local",
            "    arr[0] () + c",
        ])
    };
    for source in [
        moved(
            "def put :: mut [(unit -> i64)] -> string -> unit = \\(slot: ref mut [(unit -> i64)]) (xs: ref string) -> Array.write 0 (\\() -> xs.length * 1000) slot",
        ),
        moved(
            "def put :: mut [(unit -> i64)] -> string -> unit = \\slot xs -> Array.write 0 (\\() -> xs.length * 1000) slot",
        ),
    ] {
        rejects_at(&source, "E1014", "borrow_conflict", (7, 21));
    }
    // The annotated spellings whose stores hold no loan that outlives its owner are accepted:
    // r2_ag01 (the stored function captures nothing), a store of an `i64`, a store of a function
    // that captures only `n`, and owners declared before the target. The conventions suite runs
    // them (`annotated_reference_stores`).
    for source in [
        program(&[
            "def put :: mut [(unit -> i64)] -> 'a -> ('a -> i64) -> unit = \\(slot: ref mut [(unit -> i64)]) (v: ref 'a) (measure: ref ('a -> i64)) -> Array.write 0 (\\() -> 1) slot",
            "",
            "export def run :: i64 -> i64 = \\n ->",
            "    let mut arr = [\\() -> 0]",
            "    { let local = [n, n + 1, n + 2, n + 3]; put arr local (\\xs -> xs.length) }",
            "    let g = arr[0]",
            "    g ()",
            "",
            "run 5",
        ]),
        program(&[
            "def put :: mut [i64] -> [i64] -> unit = \\(slot: ref mut [i64]) (xs: ref [i64]) -> Array.write 0 (xs[0] * 1000 + xs.length) slot",
            "export def run :: i64 -> i64 = \\n ->",
            "    let mut arr = [0]",
            "    { let local = [n, n + 1, n + 2, n + 3]; put arr local }",
            "    arr[0]",
        ]),
        program(&[
            TEXT_LENGTH,
            "def keep :: mut [(unit -> i64)] -> (unit -> i64) -> unit = \\slots f -> Array.set 0 f slots",
            "export def pick :: i64 -> i64 = \\n ->",
            "    let chosen = if n > 0 then text_length else \\(text: ref string) -> text.length * 10",
            "    let mut slots = [\\() -> 0]",
            "    keep slots (\\() -> n)",
            "    let text = \"hello\"",
            "    chosen (ref text) + slots[0] ()",
        ]),
        program(&[
            &put_annotated,
            "export def run :: i64 -> i64 = \\n ->",
            "    let local = [n, n + 1, n + 2, n + 3]",
            "    let mut arr = [\\() -> 0]",
            "    put arr local",
            "    arr[0] ()",
        ]),
        program(&[
            &put_plain,
            "export def run :: i64 -> i64 = \\n ->",
            "    let local = [n, n + 1, n + 2]",
            "    let mut arr = [\\() -> 0]",
            &format!(
                "    let chosen = if n > 0 then put else \\(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> {store}"
            ),
            "    chosen (ref mut arr) (ref local)",
            "    arr[0] ()",
        ]),
        program(&[
            "def put :: mut [(unit -> i64)] -> (i64 -> i64) -> unit = \\(slot: ref mut [(unit -> i64)]) (f: ref (i64 -> i64)) -> Array.write 0 (\\() -> (deref f) 1000) slot",
            "export def run :: i64 -> i64 = \\n ->",
            "    let k = [n, n + 1, n + 2, n + 3]",
            "    let g = \\x -> x + k[0] + k.length",
            "    let mut arr = [\\() -> 0]",
            "    put arr g",
            "    arr[0] ()",
        ]),
    ] {
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}

/// A32 on A31 (second merge): one program of
/// `annotated_reference_parameters_follow_the_later_rules_of_in_place_writes`. `explicit` and
/// `conventions` are the type of `target` with `ref` parameter types and with conventions,
/// `plain` and `annotated` its parameters without and with reference annotations, `body` its
/// body (`annotated_body` where an annotated pattern destructures the parameter), and `CALL`
/// in `run` stands for the function that the program calls.
struct LaterRule {
    prelude: &'static [&'static str],
    explicit: String,
    conventions: String,
    plain: &'static str,
    annotated: String,
    body: &'static str,
    annotated_body: Option<&'static str>,
    run: &'static [&'static str],
}

impl LaterRule {
    fn source(&self, declaration: &str, before: Option<&str>, call: &str) -> String {
        let mut lines: Vec<String> = self.prelude.iter().map(|line| line.to_string()).collect();
        lines.push(declaration.to_string());
        lines.push("def run :: i64 -> i64".to_string());
        lines.push("fn run n =".to_string());
        lines.extend(before.map(|line| format!("    {line}")));
        lines.extend(
            self.run
                .iter()
                .map(|line| format!("    {}", line.replace("CALL", call))),
        );
        lines.push("run 123456789".to_string());
        program(&lines.iter().map(String::as_str).collect::<Vec<_>>())
    }

    fn annotated_body(&self) -> &str {
        self.annotated_body.unwrap_or(self.body)
    }

    /// A31's own spelling: a named function with `ref` parameter types.
    fn twin(&self) -> String {
        self.source(
            &format!(
                "def target :: {}\nfn target {} = {{ {} }}",
                self.explicit, self.plain, self.body
            ),
            None,
            "target",
        )
    }

    /// The same conventions in a named function: the twin of a `def` whose parameters are read
    /// parameters lent temporaries, which a named function with `ref` types cannot take.
    fn convention_twin(&self) -> String {
        self.source(
            &format!(
                "def target :: {}\nfn target {} = {{ {} }}",
                self.conventions, self.plain, self.body
            ),
            None,
            "target",
        )
    }

    /// A32's spelling: the right-hand side lambda of a `def` with conventions annotates its
    /// parameters with reference types.
    fn annotated_def(&self) -> String {
        self.source(
            &format!(
                "def target :: {} = \\{} -> {{ {} }}",
                self.conventions,
                self.annotated,
                self.annotated_body()
            ),
            None,
            "target",
        )
    }

    /// A32's spelling: the annotated lambda in the `else` branch of an `if` whose `then` branch is
    /// the same function written without annotations.
    fn annotated_branch(&self) -> String {
        self.source(
            &format!(
                "def target :: {} = \\{} -> {{ {} }}",
                self.conventions, self.plain, self.body
            ),
            Some(&format!(
                "let chosen = if n < 0 then target else \\{} -> {{ {} }}",
                self.annotated,
                self.annotated_body()
            )),
            "chosen",
        )
    }
}

#[test]
fn annotated_reference_parameters_follow_the_later_rules_of_in_place_writes() {
    // A32 stacked on A31, second merge (A31 reviews 4 to 7): the rules that A31 added later treat
    // a parameter annotated with a reference type as A31's own spelling, a named function with
    // `ref` parameter types (`twin`), treats it. Each unsafe program fails with the twin's code and
    // kind at the same expression; each safe one is accepted, and the conventions suite runs four
    // of them (`annotated_reference_later_rules`). No program here changed between the two merges
    // except through A31's rules, which change the twin in the same way.
    const SLOT: &str = "ref mut [(unit -> i64)]";
    const OTHER: &str = "let other = [to_string 777777, to_string 88888888, to_string 12345678]";
    const RECORD: &[&str] = &["record Rec { name: string, count: i64 }"];
    const SINK: [&str; 4] = [
        "let m = Mutex.create [\\() -> 0]",
        "let mr = ref m",
        "let put = \\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)",
        "put (\\() -> n)",
    ];
    const SINK_THEN_SAFE_PAIR: &[&str] = &[
        SINK[0],
        SINK[1],
        SINK[2],
        SINK[3],
        "let g = ((\\f -> { let _x = f (); () }), 2)",
        "let k = CALL (ref g)",
        "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())",
    ];
    const SINK_THEN_UNSAFE_PAIR: &[&str] = &[
        SINK[0],
        SINK[1],
        SINK[2],
        SINK[3],
        "let g = (put, 2)",
        "let k = CALL (ref g)",
        OTHER,
        "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0",
    ];
    let record = |body: &'static str, ret: &str, run: &'static [&'static str]| LaterRule {
        prelude: RECORD,
        explicit: format!("ref mut Rec -> {SLOT} -> {ret}"),
        conventions: format!("mut Rec -> mut [(unit -> i64)] -> {ret}"),
        plain: "r slot",
        annotated: format!("(r: ref mut Rec) (slot: {SLOT})"),
        body,
        annotated_body: None,
        run,
    };
    let element = |body: &'static str, run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: format!("ref mut Vec<string> -> {SLOT} -> unit"),
        conventions: "mut Vec<string> -> mut [(unit -> i64)] -> unit".to_string(),
        plain: "v slot",
        annotated: format!("(v: ref mut Vec<string>) (slot: {SLOT})"),
        body,
        annotated_body: None,
        run,
    };
    let handed = |run: &'static [&'static str]| LaterRule {
        prelude: &[
            "def push_all :: ref mut Vec<string> -> unit\nfn push_all v = { let mut i = 0; while i < 100 do { Vec.push_mut (to_string i) v; i = i + 1 } }",
        ],
        explicit: "ref mut Vec<string> -> ((unit -> i64) -> unit) -> unit".to_string(),
        conventions: "mut Vec<string> -> ((unit -> i64) -> unit) -> unit".to_string(),
        plain: "v use",
        annotated: "(v: ref mut Vec<string>) use".to_string(),
        body: "let sr = ref v[0]; use (\\() -> String.length sr); push_all v",
        annotated_body: None,
        run,
    };
    let view = |run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: format!("ref Maybe<string> -> {SLOT} -> unit"),
        conventions: "Maybe<string> -> mut [(unit -> i64)] -> unit".to_string(),
        plain: "m slot",
        annotated: format!("(m: ref Maybe<string>) (slot: {SLOT})"),
        body: "match m with | Maybe.Some s -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | Maybe.None -> ()",
        annotated_body: None,
        run,
    };
    let nested_view = |run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: format!("ref (Maybe<string> * i64) -> {SLOT} -> unit"),
        conventions: "(Maybe<string> * i64) -> mut [(unit -> i64)] -> unit".to_string(),
        plain: "p slot",
        annotated: format!("(p: ref (Maybe<string> * i64)) (slot: {SLOT})"),
        body: "match deref p with | (Maybe.Some s, _) -> { let sr = ref s; Array.write 0 (\\() -> String.length sr) slot } | _ -> ()",
        annotated_body: None,
        run,
    };
    let view_callback = |run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: "ref Maybe<string> -> ((unit -> i64) -> unit) -> i64".to_string(),
        conventions: "Maybe<string> -> ((unit -> i64) -> unit) -> i64".to_string(),
        plain: "m use",
        annotated: "(m: ref Maybe<string>) use".to_string(),
        body: "match m with | Maybe.Some s -> { let sr = ref s; use (\\() -> String.length sr); 0 } | Maybe.None -> 0",
        annotated_body: None,
        run,
    };
    let pair = |run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: "ref (((unit -> i64) -> unit) * i64) -> i64".to_string(),
        conventions: "(((unit -> i64) -> unit) * i64) -> i64".to_string(),
        plain: "p",
        annotated: "((use, z): ref (((unit -> i64) -> unit) * i64))".to_string(),
        body: "match p with | (use, z) -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z }",
        annotated_body: Some(
            "let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); z",
        ),
        run,
    };
    let maybe = |run: &'static [&'static str]| LaterRule {
        prelude: &[],
        explicit: "ref Maybe<(unit -> i64) -> unit> -> i64".to_string(),
        conventions: "Maybe<(unit -> i64) -> unit> -> i64".to_string(),
        plain: "p",
        annotated: "(p: ref Maybe<(unit -> i64) -> unit>)".to_string(),
        body: "match p with | Maybe.Some use -> { let s = to_string 123456789; let b = ref s; use (\\() -> String.length b); 3 } | Maybe.None -> 0",
        annotated_body: None,
        run,
    };
    let field = |run: &'static [&'static str]| LaterRule {
        prelude: &["record Box { use: (unit -> i64) -> unit, z: i64 }"],
        explicit: "ref Box -> i64".to_string(),
        conventions: "Box -> i64".to_string(),
        plain: "p",
        annotated: "(p: ref Box)".to_string(),
        body: "let s = to_string 123456789; let b = ref s; p.use (\\() -> String.length b); p.z",
        annotated_body: None,
        run,
    };
    let unsafe_cases = [
        // Review 6, a kept borrow of a writable parameter: the field that the stored borrow points
        // at is replaced afterwards (`m01`).
        (
            record(
                "let sr = ref r.name; Array.write 0 (\\() -> String.length sr) slot; r.name = to_string 5",
                "unit",
                &[
                    "let mut v0 = Rec { name: to_string n, count: 0 }",
                    "let mut arr = [\\() -> 0]",
                    "CALL (ref mut v0) (ref mut arr)",
                    OTHER,
                    "arr[0] () + other.length * 0",
                ],
            ),
            "E1014",
            "borrow_conflict",
        ),
        // The same with a pushed vector whose element the stored borrow points at (`m03`).
        (
            element(
                "let sr = ref v[0]; Array.write 0 (\\() -> String.length sr) slot; let mut i = 0; while i < 100 do { Vec.push_mut (to_string i) v; i = i + 1 }",
                &[
                    "let mut v0: Vec<string> = Vec.empty()",
                    "Vec.push_mut (to_string n) (ref mut v0)",
                    "let mut arr = [\\() -> 0]",
                    "CALL (ref mut v0) (ref mut arr)",
                    OTHER,
                    "arr[0] () + other.length * 0",
                ],
            ),
            "E1014",
            "borrow_conflict",
        ),
        // Review 7, a borrow handed to a function parameter and written over afterwards: the
        // caller may not pass a function that keeps what it receives (`y02`).
        (
            handed(&[
                "let m = Mutex.create [\\() -> 0]",
                "let mr = ref m",
                "let mut v0: Vec<string> = Vec.empty()",
                "Vec.push_mut (to_string n) (ref mut v0)",
                "CALL (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))",
                OTHER,
                "Mutex.with_lock mr (\\slot -> slot[0] ()) + other.length * 0",
            ]),
            "E1013",
            "store_local_borrow_through_reference",
        ),
        // Review 6, a view of the referent of a read parameter is a borrow of the caller's data,
        // which dies with the inner block (`vw03`, and `vw04` through `deref p`).
        (
            view(&[
                "let mut arr = [\\() -> 0]",
                "{ let v0 = Maybe.Some (to_string n); CALL (ref v0) (ref mut arr) }",
                OTHER,
                "arr[0] () + other.length * 0",
            ]),
            "E1013",
            "assignment_extends_borrow",
        ),
        (
            nested_view(&[
                "let mut arr = [\\() -> 0]",
                "{ let pp = (Maybe.Some (to_string n), 1); CALL (ref pp) (ref mut arr) }",
                OTHER,
                "arr[0] () + other.length * 0",
            ]),
            "E1013",
            "assignment_extends_borrow",
        ),
        (
            view_callback(&[
                "let mx = Mutex.create [\\() -> 0]",
                "let mr = ref mx",
                "let k = { let v0 = Maybe.Some (to_string n); CALL (ref v0) (\\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot)) }",
                OTHER,
                "k + Mutex.with_lock (ref mx) (\\slot -> slot[0] ()) + other.length * 0",
            ]),
            "E1013",
            "assignment_extends_borrow",
        ),
        // Review 6, a function reached through a pattern of the parameter (a tuple, a `Maybe`)
        // or a field is called as the parameter: the caller that passes a function keeping what
        // it receives fails at the call (`n21`, `n19`, `n23`).
        (
            pair(SINK_THEN_UNSAFE_PAIR),
            "E1013",
            "store_local_borrow_through_reference",
        ),
        (
            maybe(&[
                SINK[0],
                SINK[1],
                SINK[2],
                SINK[3],
                "let g = Maybe.Some put",
                "let k = CALL (ref g)",
                OTHER,
                "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0",
            ]),
            "E1013",
            "store_local_borrow_through_reference",
        ),
        (
            field(&[
                SINK[0],
                SINK[1],
                SINK[2],
                SINK[3],
                "let g = Box { use: put, z: 2 }",
                "let k = CALL (ref g)",
                OTHER,
                "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ()) + other.length * 0",
            ]),
            "E1013",
            "store_local_borrow_through_reference",
        ),
    ];
    for (case, code, kind) in &unsafe_cases {
        let twin = case.twin();
        let expected = check("2027", &twin)
            .err()
            .unwrap_or_else(|| panic!("accepted:\n{twin}"));
        assert_eq!(
            (expected.code, expected.kind),
            (*code, Some(*kind)),
            "{twin}\n{}",
            expected.message
        );
        let at = &twin[expected.span.start..expected.span.end];
        let annotated = case.annotated_def();
        let error = check("2027", &annotated)
            .err()
            .unwrap_or_else(|| panic!("accepted:\n{annotated}"));
        assert_eq!(
            (
                error.code,
                error.kind,
                &annotated[error.span.start..error.span.end]
            ),
            (*code, Some(*kind), at),
            "{annotated}\n{}",
            error.message
        );
    }
    // The safe forms, accepted in the three spellings: a write to another field of the writable
    // parameter, a read of it, and a read of the owner after the call (`wp01`, `wp02`, `wq01`); a
    // kept borrow of an element without a later push (`m04`); a function parameter that only
    // calls what it receives (`y01`, `y02c`); views of the caller's data that outlives the target
    // (`vw02`, `vw05`, `v07`); and functions reached through a tuple, a `Maybe` or a field that
    // keep nothing (`n20`, `n18`, `n22`).
    let safe_cases = [
        (
            record(
                "let sr = ref r.name; Array.write 0 (\\() -> String.length sr) slot; r.count = 5",
                "unit",
                &[
                    "let mut v0 = Rec { name: to_string n, count: 0 }",
                    "let mut arr = [\\() -> 0]",
                    "CALL (ref mut v0) (ref mut arr)",
                    "arr[0] () + v0.count",
                ],
            ),
            true,
        ),
        (
            record(
                "let sr = ref r.name; Array.write 0 (\\() -> String.length sr) slot; String.length (ref r.name) + r.count",
                "i64",
                &[
                    "let mut v0 = Rec { name: to_string n, count: 1 }",
                    "let mut arr = [\\() -> 0]",
                    "let k = CALL (ref mut v0) (ref mut arr)",
                    "arr[0] () + k",
                ],
            ),
            true,
        ),
        (
            record(
                "let sr = ref r.name; Array.write 0 (\\() -> String.length sr) slot",
                "unit",
                &[
                    "let mut v0 = Rec { name: to_string n, count: 3 }",
                    "let mut arr = [\\() -> 0]",
                    "CALL (ref mut v0) (ref mut arr)",
                    "arr[0] () + v0.count",
                ],
            ),
            true,
        ),
        (
            element(
                "let sr = ref v[0]; Array.write 0 (\\() -> String.length sr) slot",
                &[
                    "let mut v0: Vec<string> = Vec.empty()",
                    "Vec.push_mut (to_string n) (ref mut v0)",
                    "let mut arr = [\\() -> 0]",
                    "CALL (ref mut v0) (ref mut arr)",
                    "arr[0] () + v0.length",
                ],
            ),
            true,
        ),
        // Calling the chosen function value with a lambda for its read function parameter is
        // `E1013 borrow_of_temporary` with or without the annotation, so these two have no branch
        // spelling.
        (
            LaterRule {
                prelude: &[],
                explicit: "ref mut Vec<string> -> ((unit -> i64) -> i64) -> i64".to_string(),
                conventions: "mut Vec<string> -> ((unit -> i64) -> i64) -> i64".to_string(),
                plain: "v use",
                annotated: "(v: ref mut Vec<string>) use".to_string(),
                body: "let sr = ref v[0]; let k = use (\\() -> String.length sr); let mut i = 0; while i < 100 do { Vec.push_mut (to_string i) v; i = i + 1 }; k",
                annotated_body: None,
                run: &[
                    "let mut v0: Vec<string> = Vec.empty()",
                    "Vec.push_mut (to_string n) (ref mut v0)",
                    "CALL (ref mut v0) (\\g -> g ())",
                ],
            },
            false,
        ),
        (
            handed(&[
                "let mut v0: Vec<string> = Vec.empty()",
                "Vec.push_mut (to_string n) (ref mut v0)",
                "CALL (ref mut v0) (\\g -> { let _x = g (); () })",
                "v0.length",
            ]),
            false,
        ),
        (
            view(&[
                "let v0 = Maybe.Some (to_string n)",
                "let mut arr = [\\() -> 0]",
                "CALL (ref v0) (ref mut arr)",
                "arr[0] ()",
            ]),
            true,
        ),
        (
            nested_view(&[
                "let pp = (Maybe.Some (to_string n), 1)",
                "let mut arr = [\\() -> 0]",
                "CALL (ref pp) (ref mut arr)",
                "arr[0] ()",
            ]),
            true,
        ),
        (
            view_callback(&[
                "let v0 = Maybe.Some (to_string n)",
                "let mx = Mutex.create [\\() -> 0]",
                "let mr = ref mx",
                "let k = CALL (ref v0) (\\f -> Mutex.with_lock mr (\\slot -> Array.write 0 f slot))",
                "k + Mutex.with_lock (ref mx) (\\slot -> slot[0] ())",
            ]),
            false,
        ),
        (pair(SINK_THEN_SAFE_PAIR), true),
        (
            maybe(&[
                SINK[0],
                SINK[1],
                SINK[2],
                SINK[3],
                "let g: Maybe<(unit -> i64) -> unit> = Maybe.Some (\\f -> { let _x = f (); () })",
                "let k = CALL (ref g)",
                "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())",
            ]),
            true,
        ),
        (
            field(&[
                SINK[0],
                SINK[1],
                SINK[2],
                SINK[3],
                "let g = Box { use: \\f -> { let _x = f (); () }, z: 2 }",
                "let k = CALL (ref g)",
                "k + Mutex.with_lock (ref m) (\\slot -> slot[0] ())",
            ]),
            true,
        ),
    ];
    for (case, branch) in &safe_cases {
        let mut sources = vec![case.twin(), case.annotated_def()];
        if *branch {
            sources.push(case.annotated_branch());
        }
        for source in sources {
            check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        }
    }
}

#[test]
fn annotated_reference_parameters_follow_the_drop_order_and_lock_rules_of_in_place_writes() {
    // A32 stacked on A31, third merge (A31 reviews 8 and 9): borrows of a lock's contents kept
    // beyond the callback, shared borrows handed to a function parameter, temporaries lent to
    // read parameters, cells of the caller's data and parts of one value that the target ties
    // together are checked for a parameter annotated with a reference type as for a named function
    // with the same conventions (`convention_twin`). Each unsafe program fails with the twin's code
    // and kind at the same expression (and, where the `if` spelling calls the chosen function the
    // same way, with the same code and kind there); each safe one is accepted. Before this merge
    // the annotated spellings of `lk01`, `tp01`, `cp03` and `sp02` were accepted with wrong values
    // and `sh01` was rejected, as their twins were. Before the fourth merge (A31 reviews 10 and
    // 11), every spelling of the `ref mut` forms of `sp02` (`sm02`: directly, through a `ref mut`
    // variable, relayed to a named or an annotated `inner`), A31's own `ref mut` signature too, was
    // accepted and printed `drop -1` instead of being rejected.
    const OTHER: &str = "let other = [to_string 777777, to_string 88888888, to_string 12345678]";
    const HOLDER: &str = "record Holder { cell: Mutex<[unit -> i64]> }\ninstance Drop<Holder> {\n    fn drop h = do! IO.write_line (\"drop \" + to_string (Mutex.with_lock (ref h.cell) (\\b -> b[0] ())))\n}";
    const CALLER: &str = "record Caller { f: unit -> i64 }\ninstance Drop<Caller> {\n    fn drop c = do! IO.write_line (\"drop \" + to_string (c.f ()))\n}";
    const LOOK: &str = "def look :: ref string -> i64\nfn look t =\n    match String.find (ref \"9\") t with\n    | Maybe.Some i -> i\n    | Maybe.None -> -1";
    const LOCK_RUN: &[&str] = &[
        "let m = Mutex.create [to_string n]",
        "let m2 = Mutex.create [\\() -> 0]",
        "let rm2 = ref m2",
        "Mutex.with_lock (ref m) (\\a -> CALL a rm2)",
        "Mutex.with_lock (ref m) (\\a -> Array.set 0 (to_string 5) a)",
        OTHER,
        "Mutex.with_lock rm2 (\\b -> b[0] ()) + other.length * 0",
    ];
    let lock = |body: &'static str| LaterRule {
        prelude: &[],
        explicit: "ref mut [string] -> ref Mutex<[unit -> i64]> -> unit".to_string(),
        conventions: "mut [string] -> Mutex<[unit -> i64]> -> unit".to_string(),
        plain: "a m2",
        annotated: "(a: ref mut [string]) (m2: ref Mutex<[unit -> i64]>)".to_string(),
        body,
        annotated_body: None,
        run: LOCK_RUN,
    };
    let shared = |run: &'static [&'static str]| LaterRule {
        prelude: &["record Rec { name: string, count: i64 }"],
        explicit: "ref mut Rec -> ((unit -> i64) -> unit) -> unit".to_string(),
        conventions: "mut Rec -> ((unit -> i64) -> unit) -> unit".to_string(),
        plain: "r use",
        annotated: "(r: ref mut Rec) use".to_string(),
        body: "let sr = ref r.name; use (\\() -> String.length sr)",
        annotated_body: None,
        run,
    };
    const TEMP_BODY: &str = "let t = ref s; Mutex.with_lock (ref c.cell) (\\b -> Array.write 0 (\\() -> String.length t) b); 0";
    let temps = |holder_first: bool, run: &'static [&'static str]| LaterRule {
        prelude: &[HOLDER],
        explicit: String::new(),
        conventions: if holder_first {
            "Holder -> string -> i64"
        } else {
            "string -> Holder -> i64"
        }
        .to_string(),
        plain: if holder_first { "c s" } else { "s c" },
        annotated: if holder_first {
            "(c: ref Holder) (s: ref string)"
        } else {
            "(s: ref string) (c: ref Holder)"
        }
        .to_string(),
        body: TEMP_BODY,
        annotated_body: None,
        run,
    };
    let cell = |run: &'static [&'static str]| LaterRule {
        prelude: &[CALLER],
        explicit: "ref string -> ref Mutex<[Caller]> -> i64".to_string(),
        conventions: "string -> Mutex<[Caller]> -> i64".to_string(),
        plain: "s m",
        annotated: "(s: ref string) (m: ref Mutex<[Caller]>)".to_string(),
        body: "let t = ref s; Mutex.with_lock m (\\b -> Array.write 0 (Caller { f: \\() -> String.length t }) b); 0",
        annotated_body: None,
        run,
    };
    const PART_BODY: &str = "let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0";
    let part = |pair: &'static str, run: &'static [&'static str]| LaterRule {
        prelude: match pair {
            "safe" => &[HOLDER, LOOK, "record Pair { h: Holder, s: string }"],
            _ => &[HOLDER, LOOK, "record Pair { s: string, h: Holder }"],
        },
        explicit: "ref Pair -> i64".to_string(),
        conventions: "Pair -> i64".to_string(),
        plain: "p",
        annotated: "(p: ref Pair)".to_string(),
        body: PART_BODY,
        annotated_body: None,
        run,
    };
    // The same parts through an exclusive reference (fourth merge, A31 reviews 10 and 11): the
    // target ties the parts together itself, or relays the reference to a named function
    // (`inner`) or to an annotated lambda of a `def` with conventions that does.
    const SAFE_PAIR: &str = "record Pair { h: Holder, s: string }";
    const UNSAFE_PAIR: &str = "record Pair { s: string, h: Holder }";
    const INNER_NAMED: &str = "def inner :: ref mut Pair -> i64\nfn inner p = { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }";
    const INNER_ANNOTATED: &str = "def inner :: mut Pair -> i64 = \\(p: ref mut Pair) -> { let t = ref p.s; Mutex.with_lock (ref p.h.cell) (\\b -> Array.write 0 (\\() -> look t) b); 0 }";
    let exclusive_part = |prelude: &'static [&'static str],
                          body: &'static str,
                          run: &'static [&'static str]| LaterRule {
        prelude,
        explicit: "ref mut Pair -> i64".to_string(),
        conventions: "mut Pair -> i64".to_string(),
        plain: "p",
        annotated: "(p: ref mut Pair)".to_string(),
        body,
        annotated_body: None,
        run,
    };
    const UNSAFE_INIT: &str = "let mut p0 = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }";
    const SAFE_INIT: &str = "let mut p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }";
    const SHARED_RUN: [&str; 4] = [
        "let m = Mutex.create [\\() -> 0]",
        "let mr = ref m",
        "let mut v0 = Rec { name: to_string n, count: 3 }",
        "CALL (ref mut v0) (\\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot))",
    ];
    // (case, code, kind, whether the `if` spelling reports the same code and kind)
    let unsafe_cases = [
        (
            lock(
                "let sr = ref a[0]; Mutex.with_lock m2 (\\b -> Array.write 0 (\\() -> String.length sr) b)",
            ),
            "E1013",
            "mutex_result_retains_borrow",
            true,
        ),
        (
            shared(&[
                SHARED_RUN[0],
                SHARED_RUN[1],
                SHARED_RUN[2],
                SHARED_RUN[3],
                "v0.name = to_string 1",
                "Mutex.with_lock mr (\\slot -> slot[0] ()) + v0.count",
            ]),
            "E1014",
            "borrow_conflict",
            false,
        ),
        (
            temps(
                false,
                &["CALL (to_string n) (Holder { cell: Mutex.create [\\() -> 0] })"],
            ),
            "E1013",
            "assignment_extends_borrow",
            false,
        ),
        (
            cell(&[
                "let m0 = Mutex.create [Caller { f: \\() -> 0 }]",
                "let s0 = to_string n",
                "let k = CALL (ref s0) (ref m0)",
                "k",
            ]),
            "E1013",
            "assignment_extends_borrow",
            true,
        ),
        (
            cell(&["CALL (to_string n) (Mutex.create [Caller { f: \\() -> 0 }])"]),
            "E1013",
            "assignment_extends_borrow",
            false,
        ),
        (
            part(
                "unsafe",
                &[
                    "let p0 = Pair { s: String.repeat 4 (ref \"123456789\"), h: Holder { cell: Mutex.create [\\() -> 0] } }",
                    "CALL (ref p0)",
                ],
            ),
            "E1013",
            "self_referential_store",
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, UNSAFE_PAIR],
                PART_BODY,
                &[UNSAFE_INIT, "CALL (ref mut p0)"],
            ),
            "E1013",
            "self_referential_store",
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, UNSAFE_PAIR],
                PART_BODY,
                &[UNSAFE_INIT, "let r = ref mut p0", "CALL r"],
            ),
            "E1013",
            "self_referential_store",
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, UNSAFE_PAIR, INNER_NAMED],
                "inner p",
                &[UNSAFE_INIT, "CALL (ref mut p0)"],
            ),
            "E1013",
            "self_referential_store",
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, UNSAFE_PAIR, INNER_ANNOTATED],
                "inner p",
                &[UNSAFE_INIT, "CALL (ref mut p0)"],
            ),
            "E1013",
            "self_referential_store",
            true,
        ),
    ];
    for (case, code, kind, branch) in &unsafe_cases {
        let twin = case.convention_twin();
        let expected = check("2027", &twin)
            .err()
            .unwrap_or_else(|| panic!("accepted:\n{twin}"));
        assert_eq!(
            (expected.code, expected.kind),
            (*code, Some(*kind)),
            "{twin}\n{}",
            expected.message
        );
        let at = &twin[expected.span.start..expected.span.end];
        // A31's own spelling, where it can be written: a named function with `ref` types.
        if !case.explicit.is_empty() {
            let explicit = case.twin();
            let error = check("2027", &explicit)
                .err()
                .unwrap_or_else(|| panic!("accepted:\n{explicit}"));
            assert_eq!(
                (
                    error.code,
                    error.kind,
                    &explicit[error.span.start..error.span.end]
                ),
                (*code, Some(*kind), at),
                "{explicit}\n{}",
                error.message
            );
        }
        let annotated = case.annotated_def();
        let error = check("2027", &annotated)
            .err()
            .unwrap_or_else(|| panic!("accepted:\n{annotated}"));
        assert_eq!(
            (
                error.code,
                error.kind,
                &annotated[error.span.start..error.span.end]
            ),
            (*code, Some(*kind), at),
            "{annotated}\n{}",
            error.message
        );
        if *branch {
            let chosen = case.annotated_branch();
            let error = check("2027", &chosen)
                .err()
                .unwrap_or_else(|| panic!("accepted:\n{chosen}"));
            assert_eq!(
                (error.code, error.kind),
                (*code, Some(*kind)),
                "{chosen}\n{}",
                error.message
            );
        }
    }
    // The safe forms: a copy of the locked text's length (`lk01c`); the caller reads the owner
    // after handing a shared borrow (`sh01`); the holder lent before the text (`tp01c`); the
    // caller's cell declared after the text (`cp01`); the part dropped after its reader (`sp01`,
    // and its `ref mut` forms `sm01`).
    // (case, whether the `if` spelling is accepted too: a function value lent temporaries or a
    // lambda argument is `borrow_of_temporary` in every spelling, as before A31)
    let safe_cases = [
        (
            lock(
                "let k = String.length (ref a[0]); Mutex.with_lock m2 (\\b -> Array.write 0 (\\() -> k) b)",
            ),
            true,
        ),
        (
            shared(&[
                SHARED_RUN[0],
                SHARED_RUN[1],
                SHARED_RUN[2],
                SHARED_RUN[3],
                "Mutex.with_lock mr (\\slot -> slot[0] ()) + v0.count",
            ]),
            false,
        ),
        (
            temps(
                true,
                &["CALL (Holder { cell: Mutex.create [\\() -> 0] }) (to_string n)"],
            ),
            false,
        ),
        (
            cell(&[
                "let s0 = to_string n",
                "let m0 = Mutex.create [Caller { f: \\() -> 0 }]",
                "let k = CALL (ref s0) (ref m0)",
                "k",
            ]),
            true,
        ),
        (
            part(
                "safe",
                &[
                    "let p0 = Pair { h: Holder { cell: Mutex.create [\\() -> 0] }, s: String.repeat 4 (ref \"123456789\") }",
                    "CALL (ref p0)",
                ],
            ),
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, SAFE_PAIR],
                PART_BODY,
                &[SAFE_INIT, "CALL (ref mut p0)"],
            ),
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, SAFE_PAIR],
                PART_BODY,
                &[SAFE_INIT, "let r = ref mut p0", "CALL r"],
            ),
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, SAFE_PAIR, INNER_NAMED],
                "inner p",
                &[SAFE_INIT, "CALL (ref mut p0)"],
            ),
            true,
        ),
        (
            exclusive_part(
                &[HOLDER, LOOK, SAFE_PAIR, INNER_ANNOTATED],
                "inner p",
                &[SAFE_INIT, "CALL (ref mut p0)"],
            ),
            true,
        ),
    ];
    for (case, branch) in &safe_cases {
        let mut sources = vec![case.convention_twin(), case.annotated_def()];
        if !case.explicit.is_empty() {
            sources.push(case.twin());
        }
        if *branch {
            sources.push(case.annotated_branch());
        }
        for source in sources {
            check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
        }
    }
}

#[test]
fn documented_exceptions_of_annotated_reference_parameters() {
    // The language reference lists these forms as exceptions (review 2 of A32); each keeps the
    // diagnostic that it had before A32, and the documented alternative is accepted.
    // A parameter whose expected type is a reference to a reference compares the annotation with
    // the value read through it.
    let elements = |callback: &str| {
        program(&[
            "export def run :: i64 -> i64 = \\n ->",
            "    let a = \"hello\"",
            "    let b = \"hi\"",
            "    let rs = [ref a, ref b]",
            callback,
            "    lens[0] + lens[1] * 10 + n",
        ])
    };
    let nested = elements("    let lens = Array.map (\\(r: ref (ref string)) -> r.length) rs");
    let error = check("2027", &nested).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str()),
        ("E1003", "expected ref string, found string"),
        "{nested}"
    );
    assert_eq!(
        error.span.start,
        pattern_offset(&nested, "Array.map (\\"),
        "{nested}"
    );
    for source in [
        elements("    let lens = Array.map (\\(r: ref string) -> r.length) rs"),
        elements(
            "    let measure = \\(r: ref (ref string)) -> r.length\n    let lens = Array.map measure rs",
        ),
    ] {
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    // A read parameter whose type applies a type variable to arguments keeps the kind error.
    let functor = "class Functor<'container: * -> *> {\n    def map :: ('input -> 'output) -> 'container<'input> -> 'container<'output>\n}\n";
    let probe = |lambda: &str| {
        program(&[
            "instance Traits.Functor<Maybe> {",
            "    fn map transform value = Maybe.map transform value",
            "}",
            &format!(
                "def probe :: Traits.Functor<'container> => 'container<i64> -> i64 = {lambda} -> 3"
            ),
            "export def answer :: i64 = { let m: Maybe<i64> = Maybe.Some 1; probe m }",
        ])
    };
    let applied = probe("\\(value: ref 'container<i64>)");
    let error = check_with_files("2027", &applied, &[("Traits.tt", functor)]).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str()),
        (
            "E1015",
            "type constructor application has an incompatible kind"
        ),
        "{applied}"
    );
    for lambda in ["\\(value: 'container<i64>)", "\\value"] {
        let source = probe(lambda);
        check_with_files("2027", &source, &[("Traits.tt", functor)])
            .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    // `\mut (pattern)` makes the parameter itself mutable, so a `def` takes a read parameter by
    // value and a reference annotation on the pattern does not match; where the type does not
    // depend on `mut` (an expected [Read] type, a `mut T` parameter), the annotation binds the
    // reference.
    for (source, message) in [
        (
            program(&["def size :: 'a -> i64 = \\mut (v: ref 'a) -> 3"]),
            "expected ref 'a, found 'a",
        ),
        (
            program(&["def size :: string -> i64 = \\mut (t: ref string) -> t.length"]),
            "expected ref string, found string",
        ),
    ] {
        let error = check("2027", &source).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            ("E1003", message),
            "{source}"
        );
        assert_eq!(
            error.span.start,
            pattern_offset(&source, "\\mut "),
            "{source}"
        );
    }
    for source in [
        chooser(
            &[TEXT_LENGTH],
            &[
                "    let chosen = if n > 0 then text_length else \\mut (t: ref string) -> t.length * 10",
                "    let text = \"abc\"",
                "    chosen (ref text)",
            ],
        ),
        program(&[
            COUNTER,
            "def touch :: mut Counter -> unit = \\mut (c: ref mut Counter) -> c.count = c.count + 1",
            "export def answer :: i64 = { let mut c = Counter { count: 1 }; touch c; c.count }",
        ]),
    ] {
        check("2027", &source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
}
