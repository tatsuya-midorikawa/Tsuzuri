use tsuzuri::{
    parser,
    syntax::{Kind, TypeExprKind},
};

#[test]
fn parses_explicit_kinds_and_variable_application_heads() {
    let program =
        parser::parse("class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }")
            .unwrap();
    let class = &program.classes[0];
    assert_eq!(
        class.kind,
        Kind::Arrow(Box::new(Kind::Type), Box::new(Kind::Type))
    );
    assert!(
        matches!(&class.methods[0].parameters[1].kind, TypeExprKind::Apply(head, arguments) if head.text == "'f" && arguments.len() == 1)
    );
    let higher = parser::parse("class Higher<'f: (* -> *) -> *> { def value :: i64 }").unwrap();
    assert!(matches!(higher.classes[0].kind, Kind::Arrow(..)));
    assert_eq!(
        parser::parse("class Plain<'a> { def value :: 'a -> i64 }")
            .unwrap()
            .classes[0]
            .kind,
        Kind::Type
    );
    for constructor in ["Option", "Result<string>", "Array", "List", "Vec", "Task"] {
        parser::parse(&format!(
            "instance Functor<{constructor}> {{ fn map transform value = value }}"
        ))
        .unwrap();
    }
}

#[test]
fn resolves_functor_instances_without_runtime_dictionaries() {
    let traits = "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b>\ndef mapped :: ('a -> 'b) -> 'f<'a> -> 'f<'b>\nfn mapped transform value = Functor.map transform value }";
    let implementation = "instance Functor<Option> { fn map transform value = match value with | Option.None -> Option.None | Option.Some inner -> Option.Some (transform inner) }\ninstance Functor<Result<'e>> { fn map transform value = match value with | Result.Ok inner -> Result.Ok (transform inner) | Result.Error error -> Result.Error error }";
    let main = "def fmap :: Functor<'f> => ('a -> 'b) -> 'f<'a> -> 'f<'b>\nfn fmap transform value = Functor.mapped transform value\nexport def result :: i64\nfn result = { let option = fmap (\\value -> value + 1) (Option.Some 41); let result: Result<i64, string> = Functor.map (\\value -> value + 2) (Result.Ok 40); Option.get option + Result.get result }";
    let module = tsuzuri::analyze_modules(&[
        ("Traits.tt", traits),
        ("Instances.tz", implementation),
        ("Main.tz", main),
    ])
    .unwrap();
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap()
        );
        assert!(!ir.contains("%tz.hkt"));
    }
}

#[test]
fn rejects_kind_mismatches_unsaturated_values_and_overlap() {
    for source in [
        "def bad :: Option -> i64\nfn bad value = 0",
        "def bad :: 'f<'a> -> 'f<'a>\nfn bad value = value",
        "class Bad<'f: * -> *> { def bad :: 'f -> i64 }",
        "class Bad<'f: * -> *> { def bad :: 'f<'a, 'b> -> i64 }",
    ] {
        assert_eq!(
            tsuzuri::analyze(source).unwrap_err().code,
            "E1015",
            "{source}"
        );
    }
    let class = "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }\n";
    for (instances, code) in [
        (
            "instance Functor<Result> { fn map transform value = value }",
            "E1015",
        ),
        (
            "instance Functor<i64> { fn map transform value = value }",
            "E1015",
        ),
        (
            "instance Functor<Result<'e>> { fn map transform value = value }\ninstance Functor<Result<string>> { fn map transform value = value }",
            "E1016",
        ),
    ] {
        assert_eq!(
            tsuzuri::analyze(&format!("{class}{instances}"))
                .unwrap_err()
                .code,
            code,
            "{instances}"
        );
    }
}

#[test]
fn instance_head_variables_do_not_capture_method_variables() {
    let source = "class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }\ninstance Functor<Result<'a>> { fn map transform value = match value with | Result.Ok inner -> Result.Ok (transform inner) | Result.Error error -> Result.Error error }\ndef mapped :: Result<i64, bool> -> Result<string, bool>\nfn mapped input = Functor.map (\\_value -> \"mapped\") input";
    let module = tsuzuri::analyze(source).unwrap();
    tsuzuri::llvm::emit(&module, tsuzuri::llvm::Entry::Library).unwrap();
}

#[test]
fn runtime_fixture_covers_constructor_shapes_and_local_annotations() {
    let module = tsuzuri::analyze_modules(&[
        ("Traits.tt", include_str!("fixtures/higher_kinds/Traits.tt")),
        (
            "Instances.tz",
            include_str!("fixtures/higher_kinds/Instances.tz"),
        ),
        ("Main.tz", include_str!("fixtures/higher_kinds/Main.tz")),
    ])
    .unwrap();
    for wasm in [false, true] {
        tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
    }
    let error = tsuzuri::analyze("class Functor<'f: * -> *> { def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b> }\ninstance Functor<Result<'e>> { fn map transform value = match value with | Result.Ok inner -> Result.Ok (transform inner) | Result.Error error -> Result.Error error }\nFunctor.map (\\value -> value + 1) (Result.Ok 1)").unwrap_err();
    assert_eq!(error.code, "E1015");
}
