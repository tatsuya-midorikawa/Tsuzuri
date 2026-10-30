//! The hint database (G23 Phase 3): `help` (and a `likely` or `maybe` fix) for the diagnostics of
//! common mistakes, such as a tuple read as `pair.0` or a match arm written with `=>`. A hint
//! names the kind of the diagnostic it explains, fragments of its message, and the tokens around
//! the start of its span. Hints are attached once, to the diagnostics of a failed analysis of
//! user sources; the same table is the source of the guide's list of common mistakes (G21).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use crate::SourceInput;
use crate::check::ModuleOrigin;
use crate::diagnostic::{Applicability, Diagnostic, DiagnosticSet, Edit, Fix, MAX_FIXES};
use crate::lexer;
use crate::syntax::{Token, TokenKind};

/// One common mistake: the diagnostic it explains and how to write the program instead.
#[derive(Debug)]
pub struct Hint {
    /// Unique in the table: lower case and hyphens, as in `tuple-dot-index`.
    pub id: &'static str,
    /// The heading of the mistake in the guide.
    pub title: &'static str,
    /// The id of the registered kind of the diagnostic.
    pub kind: &'static str,
    /// Fragments that the diagnostic's message contains, all of them.
    pub message_has: &'static [&'static str],
    /// Alternative token patterns, one of which must match; empty for no token condition. Each
    /// element matches one token: its source text, or `<int>`, `<ident>`, `<upper>` (an
    /// identifier that starts with an ASCII capital), `<string>`, or `<any>`. The element with
    /// the prefix `@` is the main token, the token at which the diagnostic's span starts.
    pub patterns: &'static [&'static [&'static str]],
    /// An imperative sentence: lower case, without a final period.
    pub help: &'static str,
    /// The smallest program that reports the diagnostic.
    pub bad: &'static str,
    /// The program written the right way; it checks without errors.
    pub good: &'static str,
    pub fix: Option<HintFix>,
}

/// Replaces the main token with `replace`. Never `Certain`: a hint is a heuristic, so
/// `tsuzuri fix` does not apply it by default.
#[derive(Debug)]
pub struct HintFix {
    pub applicability: Applicability,
    pub replace: &'static str,
}

type Index = BTreeMap<&'static str, Vec<&'static Hint>>;

fn index(table: &'static [Hint]) -> Index {
    let mut index = Index::new();
    for hint in table {
        index.entry(hint.kind).or_default().push(hint);
    }
    index
}

static BY_KIND: LazyLock<Index> = LazyLock::new(|| index(TABLE));

/// Attaches the first matching hint of `TABLE` to each diagnostic of a user source.
pub(crate) fn annotate(set: &mut DiagnosticSet, inputs: &[SourceInput<'_>]) {
    annotate_with(set, inputs, &BY_KIND);
}

/// A hint with token patterns replaces the help of the emission site, which does not look at
/// the tokens around the diagnostic; a hint without patterns only fills an empty help.
fn annotate_with(set: &mut DiagnosticSet, inputs: &[SourceInput<'_>], index: &Index) {
    let mut lexed: BTreeMap<usize, Vec<Token>> = BTreeMap::new();
    for diagnostic in &mut set.diagnostics {
        let Some(candidates) = diagnostic.kind.and_then(|kind| index.get(kind)) else {
            continue;
        };
        let source = diagnostic.span.source.unwrap_or(0);
        let Some(input) = inputs
            .get(source)
            .filter(|input| input.origin == ModuleOrigin::User)
        else {
            continue;
        };
        let tokens = lexed
            .entry(source)
            .or_insert_with(|| lexer::lex_all(input.text, input.edition).0);
        let chosen = candidates.iter().find(|hint| {
            hint.message_has
                .iter()
                .all(|fragment| diagnostic.message.contains(fragment))
                && if hint.patterns.is_empty() {
                    diagnostic.help.is_none()
                } else {
                    hint.patterns
                        .iter()
                        .any(|pattern| matches(pattern, tokens, input.text, diagnostic))
                }
        });
        if let Some(hint) = chosen {
            apply(hint, tokens, diagnostic);
        }
    }
}

/// The index of the main token: the token that starts where the span starts.
fn main_token(tokens: &[Token], diagnostic: &Diagnostic) -> Option<usize> {
    let index = tokens.partition_point(|token| token.span.start < diagnostic.span.start);
    tokens
        .get(index)
        .is_some_and(|token| token.span.start == diagnostic.span.start)
        .then_some(index)
}

fn element_matches(element: &str, kind: Option<&TokenKind>, text: &str) -> bool {
    match element {
        "<any>" => true,
        "<int>" => matches!(kind, Some(TokenKind::Integer(_))),
        "<ident>" => matches!(kind, Some(TokenKind::Ident(_))),
        "<upper>" => {
            matches!(kind, Some(TokenKind::Ident(name)) if name.starts_with(|c: char| c.is_ascii_uppercase()))
        }
        "<string>" => matches!(kind, Some(TokenKind::String(_))),
        literal => literal == text,
    }
}

/// Whether `pattern` matches the tokens around the main token. Without a main token (a character
/// that is no token), the text of the span stands for it, and only a one-element pattern can match.
fn matches(pattern: &[&str], tokens: &[Token], text: &str, diagnostic: &Diagnostic) -> bool {
    let Some(anchor) = pattern.iter().position(|element| element.starts_with('@')) else {
        return false;
    };
    let element = |offset: usize| pattern[offset].trim_start_matches('@');
    let Some(main) = main_token(tokens, diagnostic) else {
        let span = diagnostic.span;
        return pattern.len() == 1
            && text
                .get(span.start..span.end.max(span.start))
                .is_some_and(|spelled| element_matches(element(0), None, spelled));
    };
    (0..pattern.len()).all(|offset| {
        (main + offset)
            .checked_sub(anchor)
            .and_then(|index| tokens.get(index))
            .is_some_and(|token| {
                let spelled = text.get(token.span.start..token.span.end).unwrap_or("");
                element_matches(element(offset), Some(&token.kind), spelled)
            })
    })
}

fn apply(hint: &Hint, tokens: &[Token], diagnostic: &mut Diagnostic) {
    diagnostic.help = Some(hint.help.to_string());
    let Some(fix) = &hint.fix else {
        return;
    };
    let Some(main) = main_token(tokens, diagnostic) else {
        return;
    };
    if diagnostic.fixes.len() < MAX_FIXES {
        let mut span = tokens[main].span;
        span.source = diagnostic.span.source;
        diagnostic.fixes.push(Fix {
            title: format!("replace with '{}'", fix.replace),
            applicability: fix.applicability,
            edits: vec![Edit {
                span,
                text: fix.replace.to_string(),
            }],
        });
    }
}

pub static TABLE: &[Hint] = &[
    Hint {
        id: "tuple-dot-index",
        title: "Tuple elements are read by pattern matching",
        kind: "expected_identifier",
        message_has: &[],
        patterns: &[&[".", "@<int>"]],
        help: "read a tuple element by pattern matching, as in 'match pair with | (first, _) -> first'",
        bad: r#"def first :: (i64 * i64) -> i64 = \pair -> pair.0
"#,
        good: r#"def first :: (i64 * i64) -> i64 = \pair ->
    match pair with
    | (left, _) -> left
"#,
        fix: None,
    },
    Hint {
        id: "type-ascription",
        title: "A value is annotated on its binding",
        kind: "expected_token",
        message_has: &["closing delimiter"],
        patterns: &[&["(", "<any>", "@:"]],
        help: "annotate the binding instead, as in 'let x: i64 = 5'",
        bad: r#"let x = (5: i64)
"#,
        good: r#"let x: i64 = 5
"#,
        fix: None,
    },
    Hint {
        id: "display-to-string",
        title: "to_string and Display.display",
        kind: "class_has_no_method",
        message_has: &["no method 'to_string'"],
        patterns: &[&["Display", ".", "@to_string"]],
        help: "use 'to_string value' to consume a value or 'Display.display value' to borrow it",
        bad: r#"let s = Display.to_string 5
"#,
        good: r#"let s = to_string 5
"#,
        fix: None,
    },
    Hint {
        id: "new-is-reserved",
        title: "'new' is a reserved word",
        kind: "expected_identifier",
        message_has: &[],
        patterns: &[&["let", "@new"]],
        help: "'new' is a reserved word; choose another name such as 'create'",
        bad: r#"let new = 1
"#,
        good: r#"let create = 1
"#,
        fix: None,
    },
    Hint {
        id: "abs-on-integer",
        title: "'abs' is for f64; integers use Int.abs",
        kind: "type_mismatch",
        message_has: &["found f64"],
        patterns: &[&["@abs", "<ident>"]],
        help: "'abs' takes an f64; use 'Int.abs' for a signed integer",
        bad: r#"def f :: i64 -> i64 = \n -> abs n
"#,
        good: r#"def f :: i64 -> i64 = \n -> Int.abs n
"#,
        fix: None,
    },
    Hint {
        id: "return-after-then",
        title: "A task choice uses braces",
        kind: "expected_expression",
        message_has: &[],
        patterns: &[&["then", "@return"]],
        help: "write a task choice as 'if c { return a } else { return b }'",
        bad: r#"def f :: bool -> Task<i64> = \c ->
    task {
        if c then return 1
        return 2
    }
"#,
        good: r#"def f :: bool -> Task<i64> = \c ->
    task {
        if c { return 1 } else { return 2 }
    }
"#,
        fix: None,
    },
    Hint {
        id: "fn-unit-parameter",
        title: "An implementation takes '_unit', not '()'",
        kind: "expected_token",
        message_has: &["explicit return type"],
        patterns: &[&["(", ")", "@="]],
        help: "write the implementation as 'fn name _unit = ...'; '()' is not a parameter pattern there",
        bad: r#"def f :: unit -> i64
fn f () = 1
"#,
        good: r#"def f :: unit -> i64
fn f _unit = 1
"#,
        fix: None,
    },
    Hint {
        id: "union-braces",
        title: "A union is written with = and |",
        kind: "expected_token",
        message_has: &["after the union name"],
        patterns: &[&["<upper>", "@{"]],
        help: "declare a union with '=' and '|', as in 'union Reading = Missing | Value of i64'",
        bad: r#"union Reading { Missing, Value(i64) }
"#,
        good: r#"union Reading = Missing | Value of i64
"#,
        fix: None,
    },
    Hint {
        id: "for-braces",
        title: "A for loop uses 'do' and an indented body",
        kind: "expected_token",
        message_has: &["after the enumerable expression"],
        patterns: &[&["@{"]],
        help: "write 'for i in a .. b do' and indent the body; the range includes both ends",
        bad: r#"def f :: i64 -> i64 = \n ->
    let mut t = 0
    for i in 0 .. n { t = t + i }
    t
"#,
        good: r#"def f :: i64 -> i64 = \n ->
    let mut t = 0
    for i in 0 .. n do
        t = t + i
    t
"#,
        fix: None,
    },
    Hint {
        id: "while-braces",
        title: "A while loop uses 'do' and an indented body",
        kind: "expected_token",
        message_has: &["after the while condition"],
        patterns: &[&["@{"]],
        help: "write 'while condition do' and indent the body",
        bad: r#"def f :: i64 -> i64 = \n ->
    let mut i = 0
    while i < n {
        i = i + 1
    }
    i
"#,
        good: r#"def f :: i64 -> i64 = \n ->
    let mut i = 0
    while i < n do
        i = i + 1
    i
"#,
        fix: None,
    },
    Hint {
        id: "print-macro",
        title: "Printing is IO.write_line",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@println", "!"]],
        help: "write a line with 'IO.write_line' in an IO block, or use 'Debug.print' while debugging",
        bad: r#"def f :: unit -> unit = \_ -> println!("x")
"#,
        good: r#"IO.write_line "x"
"#,
        fix: None,
    },
    Hint {
        id: "compound-assignment",
        title: "No compound assignment",
        kind: "expected_expression",
        message_has: &[],
        patterns: &[&["+", "@="], &["-", "@="], &["*", "@="], &["/", "@="]],
        help: "there is no compound assignment; write 'x = x + 1'",
        bad: r#"def f :: i64 -> i64 = \n ->
    let mut x = n
    x += 1
    x
"#,
        good: r#"def f :: i64 -> i64 = \n ->
    let mut x = n
    x = x + 1
    x
"#,
        fix: None,
    },
    Hint {
        id: "module-path-call",
        title: "A module member is reached with '.'",
        kind: "expected_binding_end",
        message_has: &[],
        patterns: &[&["<upper>", "@::", "<ident>"]],
        help: "reach a module member with '.', as in 'String.length s'; '::' only joins namespace parts",
        bad: r#"def f :: unit -> i64 = \_ ->
    let n = String::length "abc"
    n
"#,
        good: r#"def f :: unit -> i64 = \_ ->
    let n = String.length "abc"
    n
"#,
        fix: None,
    },
    Hint {
        id: "method-call",
        title: "No methods; call the module function",
        kind: "field_access_non_record",
        message_has: &[],
        patterns: &[&[".", "@<ident>", "("]],
        help: "there are no methods; call the module function, as in 'Array.length xs' or 'Array.map f xs'",
        bad: r#"def f :: [i64] -> [i64] = \xs -> xs.map(\x -> x + 1)
"#,
        good: r#"def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
"#,
        fix: None,
    },
    Hint {
        id: "struct-keyword",
        title: "A struct is a record",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@struct"]],
        help: "declare a record with 'record Point { x: i64, y: i64 }'",
        bad: r#"struct Point { x: i64 }
"#,
        good: r#"record Point { x: i64 }
"#,
        fix: None,
    },
    Hint {
        id: "null-value",
        title: "No null; use Maybe",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@null"]],
        help: "there is no null; use 'Maybe<T>' with 'Maybe.None' and 'Maybe.Some value'",
        bad: r#"def f :: unit -> i64 = \_ -> null
"#,
        good: r#"def f :: unit -> Maybe<i64> = \_ -> Maybe.None
"#,
        fix: None,
    },
    Hint {
        id: "question-mark",
        title: "'?' is not an operator",
        kind: "expected_expression",
        message_has: &["expected an expression"],
        patterns: &[&["?", "<any>", "@:"]],
        help: "'?' is not an operator but a typed hole; write 'if c then a else b', or 'match' on a Result or Maybe",
        bad: r#"def f :: bool -> i64 = \c -> c ? 1 : 2
"#,
        good: r#"def f :: bool -> i64 = \c -> if c then 1 else 2
"#,
        fix: None,
    },
    Hint {
        id: "let-in",
        title: "No 'let ... in'",
        kind: "expected_expression",
        message_has: &[],
        patterns: &[&["->", "@let"]],
        help: "start an indented body for the bindings: 'let x = 1' on its own line, then the result below",
        bad: r#"def f :: unit -> i64 = \_ -> let x = 1 in x
"#,
        good: r#"def f :: unit -> i64 = \_ ->
    let x = 1
    x
"#,
        fix: None,
    },
    Hint {
        id: "and-as-boolean",
        title: "Boolean and is '&&'",
        kind: "and_without_recursive_group",
        message_has: &[],
        patterns: &[&["<ident>", "and", "@<ident>"]],
        help: "'and' continues recursive groups; write '&&' for boolean and, '||' for or",
        bad: r#"def f :: bool -> bool -> bool = \a b -> a and b
"#,
        good: r#"def f :: bool -> bool -> bool = \a b -> a && b
"#,
        fix: None,
    },
    Hint {
        id: "pub-keyword",
        title: "No 'pub'; declarations are public by default",
        kind: "expected_token",
        message_has: &["end of the file"],
        patterns: &[&["pub", "@def"]],
        help: "declarations are public by default; write 'private def' to hide one and 'export def' for a C export",
        bad: r#"pub def f :: i64 -> i64 = \x -> x
"#,
        good: r#"def f :: i64 -> i64 = \x -> x
"#,
        fix: None,
    },
    Hint {
        id: "use-import",
        title: "Imports are 'using Namespace'",
        kind: "expected_token",
        message_has: &["end of the file"],
        patterns: &[&["use", "<ident>", "@::"]],
        help: "import a namespace with 'using Namespace' at the top of the file",
        bad: r#"use std::io;
"#,
        good: r#"using std
"#,
        fix: None,
    },
    Hint {
        id: "generic-angle-def",
        title: "Generics are type variables like 'a",
        kind: "expected_token",
        message_has: &["after the declaration name"],
        patterns: &[&["def", "<ident>", "@<"]],
        help: "write generics as type variables in the signature, as in \"def id :: 'a -> 'a = \\\\x -> x\"",
        bad: r#"def id<T> :: T -> T = \x -> x
"#,
        good: r#"def id :: 'a -> 'a = \x -> x
"#,
        fix: None,
    },
    Hint {
        id: "def-parameter-list",
        title: "A def has a signature and a lambda",
        kind: "expected_token",
        message_has: &["after the declaration name"],
        patterns: &[&["def", "<ident>", "@("]],
        help: "declare a function as 'def name :: ParamType -> ResultType = \\arg -> body'",
        bad: r#"def f(x: i64): i64 = x
"#,
        good: r#"def f :: i64 -> i64 = \x -> x
"#,
        fix: None,
    },
    Hint {
        id: "record-call",
        title: "A record is built with field names",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@<upper>", "("]],
        help: "build a record with field names, as in 'Point { x: 1, y: 2 }'",
        bad: r#"record Point { x: i64, y: i64 }
let p = Point(1, 2)
"#,
        good: r#"record Point { x: i64, y: i64 }
let p = Point { x: 1, y: 2 }
"#,
        fix: None,
    },
    Hint {
        id: "module-len",
        title: "Length is .length or Array.length",
        kind: "unknown_module_member",
        message_has: &[],
        patterns: &[&["<upper>", ".", "@len"]],
        help: "the length of an array, list, or string is 'xs.length' or 'Array.length xs'",
        bad: r#"def f :: string -> i64 = \s -> String.len s
"#,
        good: r#"def f :: string -> i64 = \s -> String.length s
"#,
        fix: None,
    },
    Hint {
        id: "len-function",
        title: "Length is .length or Array.length",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@len", "<ident>"]],
        help: "the length of an array, list, or string is 'xs.length' or 'Array.length xs'",
        bad: r#"def f :: [i64] -> i64 = \xs -> len xs
"#,
        good: r#"def f :: [i64] -> i64 = \xs -> Array.length xs
"#,
        fix: None,
    },
    Hint {
        id: "option-type",
        title: "Option is Maybe",
        kind: "unknown_type_name",
        message_has: &[],
        patterns: &[&["@Option", "<"]],
        help: "the optional type is 'Maybe<T>', with the cases 'Maybe.None' and 'Maybe.Some'",
        bad: r#"def f :: Option<i64> -> i64 = \o -> 0
"#,
        good: r#"def f :: Maybe<i64> -> i64 = \o -> 0
"#,
        fix: Some(HintFix {
            applicability: Applicability::Likely,
            replace: "Maybe",
        }),
    },
    Hint {
        id: "integer-type-name",
        title: "Integer types are sized",
        kind: "unknown_type_name",
        message_has: &[],
        patterns: &[&["@Int"], &["@int"], &["@integer"]],
        help: "write a sized integer type such as 'i32' or 'i64'",
        bad: r#"def f :: int -> int = \x -> x
"#,
        good: r#"def f :: i32 -> i32 = \x -> x
"#,
        fix: None,
    },
    Hint {
        id: "float-type-name",
        title: "Float types are f32 and f64",
        kind: "unknown_type_name",
        message_has: &[],
        patterns: &[&["@float"], &["@Float"], &["@double"]],
        help: "write a sized floating-point type: 'f64' or 'f32'",
        bad: r#"def f :: float -> float = \x -> x
"#,
        good: r#"def f :: f64 -> f64 = \x -> x
"#,
        fix: Some(HintFix {
            applicability: Applicability::Maybe,
            replace: "f64",
        }),
    },
    Hint {
        id: "string-type-name",
        title: "The string type is 'string'",
        kind: "unknown_type_name",
        message_has: &[],
        patterns: &[&["@str"], &["@Str"]],
        help: "the string type is written 'string'",
        bad: r#"def f :: str -> str = \x -> x
"#,
        good: r#"def f :: string -> string = \x -> x
"#,
        fix: Some(HintFix {
            applicability: Applicability::Likely,
            replace: "string",
        }),
    },
    Hint {
        id: "match-without-with",
        title: "A match needs 'with'",
        kind: "expected_token",
        message_has: &["after the matched expression"],
        patterns: &[],
        help: "write 'match value with' and then one '| pattern -> result' arm per line",
        bad: r#"def f :: i64 -> i64 = \x -> match x | 1 -> 2 | _ -> 3
"#,
        good: r#"def f :: i64 -> i64 = \x -> match x with | 1 -> 2 | _ -> 3
"#,
        fix: None,
    },
    Hint {
        id: "match-fat-arrow",
        title: "A match arm uses '->'",
        kind: "expected_token",
        message_has: &["after the pattern"],
        patterns: &[&["@=>"]],
        help: "separate a match pattern from its result with '->', not '=>'",
        bad: r#"def f :: i64 -> i64 = \x -> match x with | 1 => 2 | _ => 3
"#,
        good: r#"def f :: i64 -> i64 = \x -> match x with | 1 -> 2 | _ -> 3
"#,
        fix: Some(HintFix {
            applicability: Applicability::Likely,
            replace: "->",
        }),
    },
    Hint {
        id: "match-arm-bar",
        title: "Every match arm starts with '|'",
        kind: "match_without_arms",
        message_has: &[],
        patterns: &[],
        help: "start every arm with '|', as in 'match x with | 1 -> a | _ -> b'",
        bad: r#"def f :: i64 -> i64 = \x ->
    match x with
    1 -> 2
    _ -> 3
"#,
        good: r#"def f :: i64 -> i64 = \x ->
    match x with
    | 1 -> 2
    | _ -> 3
"#,
        fix: None,
    },
    Hint {
        id: "true-false-case",
        title: "Booleans are 'true' and 'false'",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@True"]],
        help: "the boolean literals are 'true' and 'false'",
        bad: r#"def f :: unit -> bool = \_ -> True
"#,
        good: r#"def f :: unit -> bool = \_ -> true
"#,
        fix: Some(HintFix {
            applicability: Applicability::Likely,
            replace: "true",
        }),
    },
    Hint {
        id: "false-case",
        title: "Booleans are 'true' and 'false'",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@False"]],
        help: "the boolean literals are 'true' and 'false'",
        bad: r#"def f :: unit -> bool = \_ -> False
"#,
        good: r#"def f :: unit -> bool = \_ -> false
"#,
        fix: Some(HintFix {
            applicability: Applicability::Likely,
            replace: "false",
        }),
    },
    Hint {
        id: "void-value",
        title: "The unit value is '()'",
        kind: "unknown_value",
        message_has: &[],
        patterns: &[&["@void"]],
        help: "the unit value is written '()'",
        bad: r#"def f :: unit -> unit = \_ -> void
"#,
        good: r#"def f :: unit -> unit = \_ -> ()
"#,
        fix: Some(HintFix {
            applicability: Applicability::Maybe,
            replace: "()",
        }),
    },
    Hint {
        id: "char-double-quotes",
        title: "A char uses single quotes",
        kind: "type_mismatch",
        message_has: &["expected char, found string"],
        patterns: &[&["@<string>"]],
        help: "a char literal uses single quotes, as in 'a'",
        bad: r#"def f :: unit -> char = \_ -> "a"
"#,
        good: r#"def f :: unit -> char = \_ -> 'a'
"#,
        fix: None,
    },
    Hint {
        id: "string-single-quotes",
        title: "A string uses double quotes",
        kind: "char_literal_not_one_unit",
        message_has: &["char literal"],
        patterns: &[&["@<any>"]],
        help: "a string literal uses double quotes, as in \"abc\"",
        bad: r#"def f :: unit -> string = \_ -> 'abc'
"#,
        good: r#"def f :: unit -> string = \_ -> "abc"
"#,
        fix: None,
    },
    Hint {
        id: "lambda-pipes",
        title: "A lambda is '\\x -> body'",
        kind: "expected_expression",
        message_has: &[],
        patterns: &[&["(", "@|"]],
        help: "write a lambda as '\\\\x -> x + 1'",
        bad: r#"def f :: [i64] -> [i64] = \xs -> Array.map (|x| x + 1) xs
"#,
        good: r#"def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
"#,
        fix: None,
    },
    Hint {
        id: "lambda-fat-arrow",
        title: "A lambda is '\\x -> body'",
        kind: "expected_token",
        message_has: &["closing delimiter"],
        patterns: &[&["<ident>", "@=>"]],
        help: "write a lambda as '\\\\x -> x + 1'",
        bad: r#"def f :: [i64] -> [i64] = \xs -> Array.map (x => x + 1) xs
"#,
        good: r#"def f :: [i64] -> [i64] = \xs -> Array.map (\x -> x + 1) xs
"#,
        fix: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{DiagnosticKind, Span};
    use crate::syntax::Edition;

    fn hint(
        id: &'static str,
        message_has: &'static [&'static str],
        patterns: &'static [&'static [&'static str]],
    ) -> Hint {
        Hint {
            id,
            title: id,
            kind: "unknown_value",
            message_has,
            patterns,
            help: id,
            bad: "",
            good: "",
            fix: None,
        }
    }

    fn input(text: &str, origin: ModuleOrigin) -> SourceInput<'_> {
        SourceInput {
            path: "Main.tz",
            text,
            origin,
            namespace: "",
            edition: Edition::E2026,
            package: None,
        }
    }

    /// The help that `table` attaches to an `unknown_value` diagnostic at `start..end` of `text`.
    fn help_with(
        table: &'static [Hint],
        text: &str,
        start: usize,
        end: usize,
        origin: ModuleOrigin,
        help: Option<&str>,
    ) -> Option<String> {
        let mut diagnostic = Diagnostic::of(
            DiagnosticKind::UnknownValue,
            "unknown value 'x'",
            Span::new(start, end),
        );
        diagnostic.help = help.map(str::to_string);
        let mut set = DiagnosticSet::from(diagnostic);
        annotate_with(&mut set, &[input(text, origin)], &index(table));
        set.diagnostics[0].help.clone()
    }

    fn help(table: &'static [Hint], text: &str, start: usize, end: usize) -> Option<String> {
        help_with(table, text, start, end, ModuleOrigin::User, None)
    }

    #[test]
    fn matches_a_pattern_around_the_main_token() {
        static TABLE: LazyLock<[Hint; 1]> =
            LazyLock::new(|| [hint("dot-int", &[], &[&[".", "@<int>"]])]);
        let table: &'static [Hint] = &*TABLE;
        assert_eq!(help(table, "pair.0", 5, 6).as_deref(), Some("dot-int"));
        assert_eq!(help(table, "pair.x", 5, 6), None);
        // The span must start at a token: inside `pair` there is none, and the pattern has 2 elements.
        assert_eq!(help(table, "pair.0", 1, 2), None);
    }

    #[test]
    fn a_pseudo_token_needs_a_single_element_pattern() {
        static ONE: LazyLock<[Hint; 1]> = LazyLock::new(|| [hint("one", &[], &[&["@`"]])]);
        static TWO: LazyLock<[Hint; 1]> =
            LazyLock::new(|| [hint("two", &[], &[&["<ident>", "@`"]])]);
        assert_eq!(help(&*ONE, "a ` b", 2, 3).as_deref(), Some("one"));
        assert_eq!(help(&*TWO, "a ` b", 2, 3), None);
    }

    #[test]
    fn every_message_fragment_must_be_present() {
        static BOTH: LazyLock<[Hint; 1]> =
            LazyLock::new(|| [hint("both", &["unknown", "'x'"], &[])]);
        static MISSING: LazyLock<[Hint; 1]> =
            LazyLock::new(|| [hint("missing", &["unknown", "'y'"], &[])]);
        assert_eq!(help(&*BOTH, "x", 0, 1).as_deref(), Some("both"));
        assert_eq!(help(&*MISSING, "x", 0, 1), None);
    }

    #[test]
    fn an_existing_help_is_kept() {
        static PLAIN: LazyLock<[Hint; 1]> = LazyLock::new(|| [hint("plain", &[], &[])]);
        static TOKENS: LazyLock<[Hint; 1]> = LazyLock::new(|| [hint("tokens", &[], &[&["@x"]])]);
        let user = ModuleOrigin::User;
        assert_eq!(
            help_with(&*PLAIN, "x", 0, 1, user, Some("site")).as_deref(),
            Some("site")
        );
        // A hint that matched the tokens is more specific than the help of the emission site.
        assert_eq!(
            help_with(&*TOKENS, "x", 0, 1, user, Some("site")).as_deref(),
            Some("tokens")
        );
    }

    #[test]
    fn std_sources_get_no_hints() {
        static TABLE: LazyLock<[Hint; 1]> = LazyLock::new(|| [hint("any", &[], &[])]);
        assert_eq!(help_with(&*TABLE, "x", 0, 1, ModuleOrigin::Std, None), None);
    }

    #[test]
    fn the_first_matching_hint_wins() {
        static TABLE: LazyLock<[Hint; 3]> = LazyLock::new(|| {
            [
                hint("first", &["no such text"], &[]),
                hint("second", &[], &[&["@x"]]),
                hint("third", &[], &[]),
            ]
        });
        assert_eq!(help(&*TABLE, "x", 0, 1).as_deref(), Some("second"));
    }
}
