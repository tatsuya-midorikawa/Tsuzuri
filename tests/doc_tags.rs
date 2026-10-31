//! The structured tags of documentation comments (G35): `@complexity`, `@since` and `@see` follow the
//! line rule of `@deprecated` (G19), and `deprecation()` keeps its behaviour.

use tsuzuri::diagnostic::Span;
use tsuzuri::syntax::Documentation;

fn documented(text: &str) -> Documentation {
    Documentation {
        text: text.to_owned(),
        span: Span::default(),
    }
}

/// The implementation of `deprecation()` before G35, kept as an independent reference.
fn old_deprecation(text: &str) -> Option<String> {
    if !text.contains("@deprecated") {
        return None;
    }
    text.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("@deprecated")?;
        (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| rest.trim().to_owned())
    })
}

const DEPRECATED: [(&str, Option<&str>); 10] = [
    ("Old.\n@deprecated use new", Some("use new")),
    ("@deprecated", Some("")),
    ("  @deprecated  spaced  ", Some("spaced")),
    ("@deprecated\ttabbed", Some("tabbed")),
    ("@deprecated a\n@deprecated b", Some("a")),
    ("@deprecatedX", None),
    ("@deprecatedX\n@deprecated real", Some("real")),
    ("text", None),
    ("see @deprecated here", None),
    ("", None),
];

#[test]
fn tag_notes_follow_the_deprecated_rule() {
    for (text, expected) in DEPRECATED {
        assert_eq!(documented(text).deprecation(), expected, "{text:?}");
    }
    let complexity = [
        ("@complexity O(n)", Some("O(n)")),
        (
            "Text.\n\n@complexity amortized O(1), worst O(n)",
            Some("amortized O(1), worst O(n)"),
        ),
        ("@complexity", Some("")),
        ("  @complexity\tO(1)  ", Some("O(1)")),
        ("@complexity O(1)\n@complexity O(n)", Some("O(1)")),
        ("@complexityX O(n)", None),
        ("see @complexity O(n)", None),
    ];
    for (text, expected) in complexity {
        assert_eq!(documented(text).complexity(), expected, "{text:?}");
    }
}

#[test]
fn complexity_since_and_see_read_their_lines() {
    let doc = documented("Text.\n\n@complexity O(n)\n@since 0.2.0\n@see Array.sum\n@see Map");
    assert_eq!(doc.complexity(), Some("O(n)"));
    assert_eq!(doc.since(), Some("0.2.0"));
    assert_eq!(doc.see(), ["Array.sum", "Map"]);
    assert_eq!(
        documented("@since 0.2.0\n@since 0.3.0").since(),
        Some("0.2.0")
    );
    assert_eq!(documented("@see").see(), [""]);
    assert!(documented("No tags.").see().is_empty());
}

#[test]
fn deprecation_is_unchanged() {
    let texts = DEPRECATED
        .iter()
        .map(|(text, _)| *text)
        .chain(["@deprecated x\r\n", "a\n  @deprecated\tz"]);
    for text in texts {
        assert_eq!(
            documented(text).deprecation().map(str::to_owned),
            old_deprecation(text),
            "{text:?}"
        );
    }
}
