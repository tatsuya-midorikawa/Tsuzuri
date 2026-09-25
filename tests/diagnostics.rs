use tsuzuri::check::{Type, TypeContext, TypedExprKind};
use tsuzuri::diagnostic::{
    Diagnostic, DiagnosticSet, MAX_REPORTED_ERRORS, MAX_UNIQUE_DIAGNOSTICS, Span,
};
use tsuzuri::{
    analyze, analyze_all, analyze_modules, analyze_modules_all, analyze_modules_with_std_all,
    lexer, llvm, parser,
};

fn errors(source: &str) -> DiagnosticSet {
    let first = analyze_all(source).expect_err(source);
    assert_eq!(first, analyze_all(source).unwrap_err(), "{source}");
    assert!(!first.is_empty());
    assert_eq!(analyze(source).unwrap_err(), first.diagnostics[0]);
    first
}

#[test]
fn collects_function_errors_in_source_order_without_changing_legacy_api() {
    let source = "def z :: i64\nfn z = true\ndef a :: i64\nfn a = \"text\"\n";
    let set = errors(source);
    assert_eq!(set.diagnostics.len(), 2);
    assert!(
        set.diagnostics
            .iter()
            .all(|d| d.code == "E1003" && d.span.source == Some(0))
    );
    assert!(set.diagnostics[0].span.start < set.diagnostics[1].span.start);
    assert_eq!(set.omitted, 0);
    assert!(!set.omitted_is_lower_bound);
    let set = errors("fn a() -> i64 { true }\nfn b() -> i64 { false }");
    assert_eq!(set.diagnostics.len(), 2);
}

#[test]
fn recovers_lexer_errors_without_splitting_utf8_or_skipping_next_line() {
    let source = "\"unterminated\n?\n1bad ? /*";
    let (tokens, diagnostics) = lexer::lex_all(source);
    assert_eq!(diagnostics.len(), 5, "{diagnostics:?}");
    assert!(diagnostics.iter().all(|d| d.code == "E0001"));
    assert_eq!(lexer::lex(source).unwrap_err(), diagnostics[0]);
    assert_eq!(tokens.last().unwrap().kind, tsuzuri::syntax::TokenKind::End);
    let (_, diagnostics) = lexer::lex_all("日 ? 1__2 \"bad\\q\"\n?");
    assert_eq!(diagnostics.len(), 5);
    for diagnostic in diagnostics {
        assert!("日 ? 1__2 \"bad\\q\"\n?".is_char_boundary(diagnostic.span.start));
        assert!("日 ? 1__2 \"bad\\q\"\n?".is_char_boundary(diagnostic.span.end));
    }
    assert_eq!(errors("? ?").diagnostics.len(), 2);
}

#[test]
fn parser_recovers_only_at_balanced_top_level_declarations() {
    for source in [
        "fn first =\nrecord R { x: i64 }\nfn second =\n",
        "fn first() -> i64 {\ntrue + ;\nlet inside = )\n}\nfn second() -> i64 { false + ; }\n",
        "fn first =\nprivate union Choice = Chosen of i64\nfn second =\n",
        "fn rec first =\nand second x =\nfn third =\n",
        "let first =\nlet second =\n",
        "record Bad<'a 'b> { value: 'a }\nrecord Good<'a> { value: 'a }\nfn second =\n",
        "def first :: Task<Option<i64>>=>\nfn second =\n",
    ] {
        let set = parser::parse_with_source_all(source, 7).unwrap_err();
        assert_eq!(set.len(), 2, "{source}\n{set:?}");
        assert!(
            set.iter()
                .all(|d| d.code == "E0002" && d.span.source == Some(7))
        );
    }
    // An unclosed explicit block never makes its inner declarations top-level.
    let source = "fn first() -> i64 {\ntrue + ;\nlet inside = )\nfn nested =\n";
    assert_eq!(
        parser::parse_with_source_all(source, 0).unwrap_err().len(),
        1
    );
}

#[test]
fn parses_every_source_but_does_not_type_check_broken_fragments() {
    let sources = [
        ("Main.tz", "fn first =\n"),
        ("Other.tz", "fn second =\n"),
        ("Types.tz", "fn third() -> i64 { true }"),
    ];
    let set = analyze_modules_all(&sources).unwrap_err();
    assert_eq!(set.diagnostics.len(), 2);
    assert_eq!(set.diagnostics[0].span.source, Some(0));
    assert_eq!(set.diagnostics[1].span.source, Some(1));
    assert!(set.diagnostics.iter().all(|d| d.code == "E0002"));
    assert_eq!(analyze_modules(&sources).unwrap_err(), set.diagnostics[0]);
}

#[test]
fn poisoned_signatures_suppress_dependent_errors_on_every_expression_surface() {
    for body in [
        "bad 1",
        "bad []",
        "(bad 1).field",
        "(bad 1)[0]",
        "bad 1 as i64",
        "bad 1 + 1",
        "1 + bad 1",
        "! (bad 1)",
        "bad 1 |> identity",
        "identity (bad 1)",
        "(to_string (bad 1)).length",
        "let x: bool = bad 1; x",
        "[bad 1]",
        "[|bad 1|]",
        "(bad 1, true)",
        "Some (bad 1)",
        "Pair { first: bad 1, second: true }",
        "* (bad 1)",
        "&(bad 1)",
        "&mut (bad 1)",
        "*(bad 1) = 42",
        "if bad 1 then 0 else 1",
        "match bad 1 with | Some x -> x | None -> 0",
        "for x in bad 1 do ()",
        "let f = x -> bad x; f 1",
        "Task.run (task { return bad 1 })",
    ] {
        let source = format!(
            "record Pair<'a, 'b> {{ first: 'a, second: 'b }}
             def identity :: 'a -> 'a\nfn identity x = x
             def bad :: Missing -> i64\nfn bad x = x
             def caller :: i64\nfn caller = {{ let unused = {{ {body} }}; 0 }}
             def independent :: i64\nfn independent = true"
        );
        let set = errors(&source);
        assert_eq!(set.diagnostics.len(), 2, "{body}\n{set:?}");
        assert_eq!(set.diagnostics[0].code, "E1004", "{body}");
        assert_eq!(set.diagnostics[1].code, "E1003", "{body}");
        assert!(
            !set.diagnostics
                .iter()
                .any(|d| d.message.contains("erroneous"))
        );
    }
}

#[test]
fn poisoned_active_recognizers_do_not_panic_or_cascade() {
    let set = errors(
        "def (|Bad|_|) :: Missing -> bool
         fn (|Bad|_|) x = true
         def use :: i64 -> i64
         fn use x = match x with | Bad -> 1 | _ -> 0
         def other :: i64
         fn other = true",
    );
    assert_eq!(set.diagnostics.len(), 2, "{set:?}");
    assert_eq!(set.diagnostics[0].code, "E1004");
    assert_eq!(set.diagnostics[1].code, "E1003");
}

#[test]
fn collects_ownership_errors_during_inference_instead_of_stopping_at_first_function() {
    let set = errors(
        "def consume :: string -> unit\nfn consume s = ()
         def a :: unit\nfn a = { let s = \"a\"; consume s; consume s }
         def b :: unit\nfn b = { let s = \"b\"; consume s; consume s }",
    );
    assert_eq!(set.diagnostics.len(), 2, "{set:?}");
    assert!(set.diagnostics.iter().all(|d| d.code == "E1012"));
    let module = analyze("42").unwrap();
    assert!(tsuzuri::ownership::check_all(&module).is_empty());
    tsuzuri::ownership::check(&module).unwrap();
}

#[test]
fn collects_independent_declaration_and_module_errors_at_safe_phase_boundaries() {
    for source in [
        "record A { x: Missing }\nrecord B { y: AlsoMissing }",
        "union A = First of Missing\nunion B = Second of AlsoMissing",
        "record A { x: i64 }\nrecord A { x: i64 }\nrecord B { x: i64 }\nrecord B { x: i64 }",
        "union A = First | First\nunion B = Second | Second",
        "record A { a: A }\nrecord B { b: B }",
        "class A<'a> { def a :: Missing -> 'a }\nclass B<'a> { def b :: AlsoMissing -> 'a }",
        "instance Display<i64> { fn display x = \"\" }\ninstance Display<bool> { fn display x = \"\" }",
    ] {
        assert_eq!(errors(source).diagnostics.len(), 2, "{source}");
    }
    let set = analyze_modules_all(&[("Bad-name.tz", ""), ("Other-name.tz", "")]).unwrap_err();
    assert_eq!(set.diagnostics.len(), 2);
    assert!(set.diagnostics.iter().all(|d| d.code == "E1011"));
    let set = analyze_modules_all(&[
        ("Bad.tt", "record A { x: i64 }"),
        ("Other.tt", "record B { y: i64 }"),
    ])
    .unwrap_err();
    assert_eq!(set.diagnostics.len(), 2);
    assert!(set.diagnostics.iter().all(|d| d.code == "E1018"));
}

#[test]
fn sorts_sources_and_preserves_std_origins() {
    let set = analyze_modules_with_std_all(
        &[("Main", "def f :: i64\nfn f = true")],
        &[("std/Extra.tz", "def g :: i64\nfn g = false")],
    )
    .unwrap_err();
    assert_eq!(set.diagnostics.len(), 2);
    assert_eq!(set.diagnostics[0].span.source, Some(0));
    assert_eq!(set.diagnostics[1].span.source, Some(1));
    let a = Diagnostic::new("E1003", "a", Span::new(5, 8).in_source(1));
    let b = Diagnostic::new("E1002", "b", Span::new(5, 8).in_source(1));
    let c = Diagnostic::new("E1003", "c", Span::new(0, 0));
    let set = DiagnosticSet::from_diagnostics([a.clone(), c.clone(), b.clone(), a.clone()], 2);
    assert_eq!(set.diagnostics, [b, a, c]);
}

fn many_errors(count: usize) -> String {
    (0..count)
        .map(|index| format!("def f{index} :: i64\nfn f{index} = true\n"))
        .collect()
}

#[test]
fn display_cap_counts_all_unique_errors_until_the_separate_hard_limit() {
    let set = errors(&many_errors(62));
    assert_eq!(set.diagnostics.len(), MAX_REPORTED_ERRORS);
    assert_eq!(set.omitted, 12);
    assert!(!set.omitted_is_lower_bound);
    assert_eq!(
        set.omission_note().as_deref(),
        Some("12 more errors not shown")
    );
    let set = errors(&many_errors(MAX_UNIQUE_DIAGNOSTICS + 3));
    assert_eq!(set.diagnostics.len(), MAX_REPORTED_ERRORS);
    assert_eq!(set.omitted, MAX_UNIQUE_DIAGNOSTICS - MAX_REPORTED_ERRORS);
    assert!(set.omitted_is_lower_bound);
    assert_eq!(
        set.omission_note().as_deref(),
        Some("at least 950 more errors not shown")
    );
    let set = errors(&"?\n".repeat(MAX_UNIQUE_DIAGNOSTICS + 3));
    assert!(set.omitted_is_lower_bound);
    let duplicate = Diagnostic::new("E1003", "same", Span::default());
    let set = DiagnosticSet::from_diagnostics(
        std::iter::repeat_n(duplicate, MAX_UNIQUE_DIAGNOSTICS + 3),
        0,
    );
    assert_eq!(set.diagnostics.len(), 1);
    assert_eq!(set.omitted, 0);
    assert!(!set.omitted_is_lower_bound);
}

#[test]
fn error_type_properties_and_codegen_barrier_are_explicit() {
    let types = TypeContext {
        records: &[],
        unions: &[],
    };
    assert_eq!(Type::Error.display(&types), "an erroneous type");
    assert!(Type::Error.is_copy(&types));
    assert!(!Type::Error.needs_drop(&types));
    assert!(!Type::Error.contains_reference());
    assert!(!Type::Error.exportable());
    for ty in [
        Type::Array(Box::new(Type::Error)),
        Type::List(Box::new(Type::Error)),
        Type::Tuple(vec![Type::Error]),
        Type::Function(vec![Type::Error], Box::new(Type::Unit)),
        Type::Reference(Box::new(Type::Error), false),
        Type::Record(0, vec![Type::Error].into()),
        Type::Union(0, vec![Type::Error].into()),
    ] {
        assert!(ty.contains_error());
    }
    let mut module = analyze("42").unwrap();
    let entry = module.entry.unwrap();
    module.functions[entry].body.kind = TypedExprKind::Error;
    assert!(
        llvm::emit(&module, llvm::Entry::Library)
            .unwrap_err()
            .message
            .contains("erroneous")
    );
}
