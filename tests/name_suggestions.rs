//! G25 Phase 4: an unresolved name (`E1002`, `E1004`, `E1007`, `E1020`) gets the names near it as
//! candidates, a `help` that names them, and a `maybe` fix when one is best. The code, the message
//! and the primary span do not change, and a diagnostic without a near name stays as it was.

use tsuzuri::analyze_all;
use tsuzuri::diagnostic::{Applicability, Diagnostic};

fn first(source: &str) -> Diagnostic {
    analyze_all(source).expect_err(source).diagnostics.remove(0)
}

fn spanned<'a>(source: &'a str, diagnostic: &Diagnostic) -> &'a str {
    &source[diagnostic.span.start..diagnostic.span.end]
}

fn candidates(diagnostic: &Diagnostic) -> Vec<(&str, &str, &str)> {
    diagnostic
        .detail
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.name.as_str(),
                candidate.how.as_str(),
                candidate.signature.as_str(),
            )
        })
        .collect()
}

/// The one `maybe` fix of `diagnostic`: its title and the text it writes over the primary span.
fn maybe_fix(diagnostic: &Diagnostic) -> (&str, &str) {
    assert_eq!(diagnostic.fixes.len(), 1, "{:?}", diagnostic.fixes);
    let fix = &diagnostic.fixes[0];
    assert_eq!(fix.applicability, Applicability::Maybe);
    assert_eq!(fix.edits.len(), 1);
    assert_eq!(fix.edits[0].span, diagnostic.span);
    (fix.title.as_str(), fix.edits[0].text.as_str())
}

const NEAR_LOCAL: &str = "let total = 1\nlet y = totl + 1\ny\n";
const BARE_MEMBER: &str = "let xs = [1, 2]\nlength (ref xs)\n";
const MEMBER_TYPO: &str = "let xs = [1, 2]\nArray.lenght (ref xs)\n";
const MODULE_TYPO: &str = "let xs = [1, 2]\nAray.length (ref xs)\n";
const TYPE_TYPO: &str = "record Point { x: i64 }\nlet p: Pointt = Point { x: 1 }\np\n";
const FIELD_ACCESS: &str = "record P { count: i64 }\nlet p = P { count: 1 }\np.cont\n";
const FIELD_LITERAL: &str = "record P { count: i64 }\nlet p = P { cont: 1 }\np\n";
const CASE_TYPO: &str = "union Shape = Circle of f64 | Empty\nlet s = Empty\nmatch s with\n| Circl r -> r\n| Empty -> 0.0\n";

#[test]
fn a_near_local_gets_a_help_and_a_maybe_fix() {
    let diagnostic = first(NEAR_LOCAL);
    assert_eq!(diagnostic.code, "E1002");
    assert_eq!(diagnostic.message, "unknown value 'totl'");
    // An unsuffixed integer literal's type is open; its default is `i32`.
    assert_eq!(candidates(&diagnostic)[0], ("total", "local", "total: i32"));
    assert_eq!(diagnostic.help.as_deref(), Some("did you mean 'total'?"));
    assert_eq!(
        maybe_fix(&diagnostic),
        ("Replace 'totl' with 'total'", "total")
    );
}

#[test]
fn no_near_name_leaves_the_diagnostic_unchanged() {
    let source = "let count = 1\nlet y = cnt + 1\ny\n";
    let diagnostic = first(source);
    assert_eq!(diagnostic.message, "unknown value 'cnt'");
    assert!(diagnostic.detail.candidates.is_empty());
    assert!(diagnostic.fixes.is_empty());
    let json = diagnostic.json("Main.tz", source);
    assert!(!json.contains("\"candidates\""), "{json}");
    assert!(!json.contains("\"fixes\""), "{json}");
    // G22's help stays.
    assert!(
        diagnostic
            .help
            .as_deref()
            .is_some_and(|help| help.starts_with("check the spelling"))
    );
}

#[test]
fn a_bare_member_name_gets_qualified_candidates() {
    let diagnostic = first(BARE_MEMBER);
    assert_eq!(diagnostic.code, "E1002");
    assert_eq!(diagnostic.message, "unknown value 'length'");
    let found = candidates(&diagnostic);
    assert!(
        found.contains(&("Array.length", "qualify", "function")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(name, how, _)| *name == "String.length" && *how == "qualify")
    );
    assert!(diagnostic.fixes.is_empty());
    assert!(
        diagnostic
            .help
            .as_deref()
            .is_some_and(|help| help.starts_with("did you mean one of: 'Array.length'")),
        "{:?}",
        diagnostic.help
    );
}

#[test]
fn a_module_member_typo_gets_the_member_name() {
    let diagnostic = first(MEMBER_TYPO);
    assert_eq!(diagnostic.code, "E1002");
    assert_eq!(
        diagnostic.message,
        "module 'Array' has no function or union case 'lenght'"
    );
    assert_eq!(spanned(MEMBER_TYPO, &diagnostic), "lenght");
    assert_eq!(
        candidates(&diagnostic)[0],
        ("length", "similar", "function")
    );
    assert_eq!(
        maybe_fix(&diagnostic),
        ("Replace 'lenght' with 'length'", "length")
    );
}

#[test]
fn a_module_name_typo_gets_the_module() {
    let diagnostic = first(MODULE_TYPO);
    assert_eq!(diagnostic.code, "E1002");
    assert_eq!(diagnostic.message, "unknown value 'Aray'");
    assert_eq!(candidates(&diagnostic)[0], ("Array", "similar", "module"));
    assert_eq!(
        maybe_fix(&diagnostic),
        ("Replace 'Aray' with 'Array'", "Array")
    );
}

#[test]
fn an_unknown_type_gets_a_near_type() {
    let diagnostic = first(TYPE_TYPO);
    assert_eq!(diagnostic.code, "E1004");
    assert_eq!(
        diagnostic.message,
        "unknown record, union, or type alias 'Pointt'"
    );
    assert_eq!(candidates(&diagnostic)[0], ("Point", "similar", "type"));
    assert_eq!(
        maybe_fix(&diagnostic),
        ("Replace 'Pointt' with 'Point'", "Point")
    );
    // A primitive type is a name too.
    let diagnostic = first("let x: Strng = \"a\"\nx\n");
    assert_eq!(candidates(&diagnostic)[0], ("string", "similar", "type"));
}

#[test]
fn an_unknown_field_gets_a_near_field() {
    for (source, message) in [
        (FIELD_ACCESS, "unknown field 'cont'"),
        (FIELD_LITERAL, "record 'Main.P' has no field 'cont'"),
    ] {
        let diagnostic = first(source);
        assert_eq!(diagnostic.code, "E1007");
        assert_eq!(diagnostic.message, message);
        assert_eq!(candidates(&diagnostic)[0], ("count", "similar", "field"));
        assert_eq!(
            maybe_fix(&diagnostic),
            ("Replace 'cont' with 'count'", "count")
        );
    }
}

#[test]
fn an_unknown_union_case_gets_a_near_case() {
    let diagnostic = first(CASE_TYPO);
    assert_eq!(diagnostic.code, "E1020");
    assert_eq!(
        diagnostic.message,
        "unknown union case or active pattern 'Circl'"
    );
    assert_eq!(
        candidates(&diagnostic)[0],
        ("Circle", "similar", "union case")
    );
    assert_eq!(
        maybe_fix(&diagnostic),
        ("Replace 'Circl' with 'Circle'", "Circle")
    );
}

#[test]
fn an_ambiguous_best_gets_no_fix() {
    let diagnostic = first("let aa = 1\nlet ab = 2\nlet y = ac + 1\ny\n");
    let names: Vec<&str> = candidates(&diagnostic)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(names, ["aa", "ab"]);
    assert!(diagnostic.fixes.is_empty());
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("did you mean one of: 'aa', 'ab'?")
    );
}

#[test]
fn messages_codes_and_spans_do_not_change() {
    for (source, code, message, spelled) in [
        (NEAR_LOCAL, "E1002", "unknown value 'totl'", "totl"),
        (BARE_MEMBER, "E1002", "unknown value 'length'", "length"),
        (
            MEMBER_TYPO,
            "E1002",
            "module 'Array' has no function or union case 'lenght'",
            "lenght",
        ),
        (MODULE_TYPO, "E1002", "unknown value 'Aray'", "Aray"),
        (
            TYPE_TYPO,
            "E1004",
            "unknown record, union, or type alias 'Pointt'",
            "Pointt",
        ),
        (FIELD_ACCESS, "E1007", "unknown field 'cont'", "cont"),
        (
            FIELD_LITERAL,
            "E1007",
            "record 'Main.P' has no field 'cont'",
            "cont",
        ),
        (
            CASE_TYPO,
            "E1020",
            "unknown union case or active pattern 'Circl'",
            "Circl",
        ),
    ] {
        let diagnostic = first(source);
        assert_eq!(
            (diagnostic.code, diagnostic.message.as_str()),
            (code, message)
        );
        let at = source.rfind(spelled).unwrap();
        assert_eq!(
            (diagnostic.span.start, diagnostic.span.end),
            (at, at + spelled.len()),
            "{source}"
        );
    }
}
