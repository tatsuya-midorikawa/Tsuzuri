use tsuzuri::{analyze, analyze_modules, llvm};

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn alias_names_share_the_type_namespace() {
    for source in [
        "type i64 = i32",
        "type Task = i64",
        "type _ = i64",
        "type Add = i64",
        "type Name = i64\ntype Name = i32",
        "record Name {}\ntype Name = i64",
        "type Name = i64\nunion Name = Empty",
        "type Name = i64\nunion Kind = Name",
        "type Name = i64\nclass Name<'a> { def name :: 'a -> i64 }",
    ] {
        rejects(source, "E1001");
    }
    rejects("type lower = i64", "E1024");
}

#[test]
fn aliases_follow_source_kind_rules() {
    let error = analyze_modules(&[("Types.tt", "type Meters = f64")]).unwrap_err();
    assert_eq!(error.code, "E1018");
    analyze_modules(&[(
        "Identity.tc",
        "type Meters = f64\ndef Return :: i64 -> i64\nfn Return value = value",
    )])
    .unwrap();
}

#[test]
fn non_generic_aliases_are_transparent() {
    let alias = analyze(
        "type Distance = Meters\ntype Meters = f64\n\
         def distance :: Distance -> Meters\nfn distance value = value + 1.0",
    )
    .unwrap();
    let direct = analyze("def distance :: f64 -> f64\nfn distance value = value + 1.0").unwrap();
    for wasm in [false, true] {
        assert_eq!(
            llvm::emit_target(&alias, llvm::Entry::Library, wasm).unwrap(),
            llvm::emit_target(&direct, llvm::Entry::Library, wasm).unwrap(),
        );
    }
    rejects("type First = Second\ntype Second = First", "E1024");
    rejects("type Recursive = [Recursive]", "E1024");
    rejects("type Missing = Unknown", "E1004");
}

#[test]
fn aliases_resolve_in_the_declaring_module_and_preserve_privacy() {
    analyze_modules(&[
        ("Units.tz", "type Value = i64\ntype Distance = Value"),
        (
            "Main.tz",
            "type Value = string\ndef distance :: Units.Distance -> i64\nfn distance value = value",
        ),
    ])
    .unwrap();
    for name in ["Secret.Hidden", "Hidden"] {
        let source = format!("def use_value :: {name} -> i64\nfn use_value value = value");
        let error = analyze_modules(&[
            ("Secret.tz", "private type Hidden = i64"),
            ("Main.tz", &source),
        ])
        .unwrap_err();
        assert_eq!(error.code, "E1022");
        assert_eq!(error.span.source, Some(1));
    }
    rejects("private record Hidden {}\ntype Leaked = Hidden", "E1022");
    rejects(
        "private type Hidden = i64\ndef leak :: Hidden\nfn leak = 1",
        "E1022",
    );
}

#[test]
fn generic_aliases_substitute_types_without_capturing_names() {
    let alias = analyze(
        "record Pair<'a, 'b> { left: 'a, right: 'b }\n\
         type Pair2<'a> = Pair<'a, 'a>\n\
         type Identity<'a> = 'a\n\
         def sum :: Pair2<Identity<Identity<i64>>> -> i64\n\
         fn sum pair = pair.left + pair.right",
    )
    .unwrap();
    let direct = analyze(
        "record Pair<'a, 'b> { left: 'a, right: 'b }\n\
         def sum :: Pair<i64, i64> -> i64\nfn sum pair = pair.left + pair.right",
    )
    .unwrap();
    assert_eq!(
        llvm::emit(&alias, llvm::Entry::Library).unwrap(),
        llvm::emit(&direct, llvm::Entry::Library).unwrap(),
    );
    analyze_modules(&[
        ("Aliases.tz", "type Identity<'a> = 'a\ntype Nested<'a> = Identity<Option.Option<'a>>"),
        ("Main.tz", "private record Local { value: i64 }\nprivate def own :: Aliases.Identity<Local> -> i64\nfn own value = value.value"),
    ])
    .unwrap();
    analyze("type Flip<'a, 'b> = 'b * 'a\ntype Swapped<'a, 'b> = Flip<'b, 'a>\ndef first :: Swapped<i64, string> -> i64\nfn first pair = match pair with | (value, _) -> value").unwrap();
}

#[test]
fn rejects_invalid_alias_parameters_and_bounds_expansion() {
    for source in [
        "type Bad = 'a",
        "type Unused<'a> = i64",
        "type Duplicate<'a, 'a> = 'a",
        "type Identity<'a> = 'a\ntype Bad = Identity",
        "type Identity<'a> = 'a\ntype Bad = Identity<i64, string>",
        "type Recursive<'a> = Recursive<['a]>",
        "type First<'a> = Second<'a>\ntype Second<'a> = First<'a>",
    ] {
        rejects(source, "E1024");
    }
    let mut source = String::from("type End = i64\n");
    for index in 0..140 {
        source.push_str(&format!("type Alias{index} = Alias{}\n", index + 1));
    }
    source.push_str("type Alias140 = End");
    rejects(&source, "E1017");
    let mut growing = String::from("type Twice<'a> = 'a * 'a\ntype Huge = ");
    growing.push_str(&"Twice<".repeat(16));
    growing.push_str("i64");
    growing.push_str(&">".repeat(16));
    rejects(&growing, "E1017");
}

#[test]
fn aliases_preserve_constraints_and_instance_coherence() {
    let source = "type Addable<'a> = Add<'a>\n\
                  def twice :: Addable<'a> -> 'a\nfn twice value = value + value\n";
    analyze(&format!("{source}twice 21")).unwrap();
    rejects(&format!("{source}twice true"), "E1005");
    rejects(
        "type Meters = f64\ninstance Add<Meters> { fn add left right = left }",
        "E1016",
    );
    rejects(
        "type Addable<'a> = Add<'a>\nrecord Bad<'a> { value: Addable<'a> }",
        "E1016",
    );
    rejects(
        "class Number<'a> { def number :: 'a -> i64 }\n\
         type Meters = i64\n\
         instance Number<Meters> { fn number value = value }\n\
         instance Number<i64> { fn number value = value }",
        "E1016",
    );
}

#[test]
fn aliases_preserve_ownership_and_borrow_rules() {
    rejects(
        "type Text = string\nfn moved(value: Text) -> Text { let other = value; value }",
        "E1012",
    );
    rejects(
        "type Borrowed = &i64\nfn dangling() -> Borrowed { let value = 1; &value }",
        "E1013",
    );
    rejects(
        "type Borrowed<'a> = &mut 'a\nrecord Bad { value: Borrowed<i64> }",
        "E1013",
    );
    analyze("type Borrowed<'a> = &'a\nrecord View { value: Borrowed<i64> }\nlet value = 42\nlet view = View { value: &value }\nderef view.value").unwrap();
    analyze("type Borrowed = ref i64\nfn read(value: Borrowed) -> i64 { deref value }").unwrap();
}
