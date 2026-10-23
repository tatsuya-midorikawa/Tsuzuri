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
    }
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
        "export def answer :: i64 = Array.sum (Array.mapr (\\n -> *n + 1) [1, 2, 3])\n",
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
    let source = "def action :: unit -> IO<i64> = \\() -> Async.block_on (Async { \
        let! values = Async.all [Async { return 1 }, Async { return 2 }]; \
        return values[0] + values[1] })\n\
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
    let whole = source.replace("value.text = \"done\"", "*value = Value { text: \"done\" }");
    assert_eq!(check("2027", &whole).unwrap_err().code, "E1012");
}

#[test]
fn read_callbacks_cannot_escape_local_elements() {
    let source = "def borrow_value :: ref i64 -> ref i64 = \\value -> value\n\
        def leak :: unit -> [ref i64] = \\() -> { let values = [42]; Array.map borrow_value values }\n\
        export def answer :: i64 = *(leak ())[0]\n";
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
         holder.n = 1; value = 2; *holder.borrowed }\n",
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
         let element = Vec.at 0 values; Vec.push 2 values; *element }\n",
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
    ] {
        let source = format!("def accept :: {ty} -> i64 = \\_value -> 0\n");
        for edition in ["2026", "2027"] {
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
