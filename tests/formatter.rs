use tsuzuri::lexer::{TriviaKind, lex, lex_with_trivia};
use tsuzuri::{
    formatter::{ast_fingerprint, format_source},
    parser,
    syntax::SourceKind,
};

#[test]
fn documentation_text_survives_formatting_and_span_changes() {
    let source = "/// First.  \r\ndef answer::i64\r\nfn answer=42\r\n/// Point.\r\nrecord Point{x:i64}\r\n/// Class.\r\nclass Measure<'a>{\r\n  /// ```tsuzuri\r\n  /// measure value\r\n  /// ```\r\n  def measure::ref 'a->i64\r\n}\r\n";
    let first = format_source("Main.tz", source, SourceKind::Code).unwrap();
    let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
    assert_eq!(first.formatted, second.formatted);
    assert!(first.formatted.contains("/// First.  \r\n"));
    let before = parser::parse(source).unwrap();
    let after = parser::parse(&first.formatted).unwrap();
    assert_eq!(
        before.classes[0].methods[0].doc.as_ref().unwrap().text,
        after.classes[0].methods[0].doc.as_ref().unwrap().text
    );
    assert_eq!(ast_fingerprint(before), ast_fingerprint(after));
}

#[test]
fn keeps_a_shebang_line() {
    for (source, expected) in [
        (
            "#!/usr/bin/env tsuzuri script\nlet   x =  1\nx\n",
            "#!/usr/bin/env tsuzuri script\nlet x = 1\nx\n",
        ),
        (
            "#!/usr/bin/env -S tsuzuri script -O0\r\nlet x=1\r\n// keep\r\nx\r\n",
            "#!/usr/bin/env -S tsuzuri script -O0\r\nlet x = 1\r\n// keep\r\nx\r\n",
        ),
        (
            "#!/usr/bin/env tsuzuri script",
            "#!/usr/bin/env tsuzuri script\n",
        ),
    ] {
        let result = format_source("Main.tz", source, SourceKind::Code).unwrap();
        assert_eq!(result.formatted, expected);
        assert_eq!(
            format_source("Main.tz", expected, SourceKind::Code)
                .unwrap()
                .formatted,
            expected
        );
    }
}

#[test]
fn trims_only_external_whitespace_and_preserves_newline_style() {
    for (source, expected) in [
        (
            "\u{feff}let value=1  \r\n// keep   \r\nvalue\r\n\n",
            "\u{feff}let value = 1\r\n// keep   \r\nvalue\r\n",
        ),
        (
            "let value=1  \n/* keep  \r\n  here   */  \nvalue\t \n\n",
            "let value = 1\n/* keep  \r\n  here   */\nvalue\n",
        ),
        ("// keep   ", "// keep   \n"),
        ("", "\n"),
    ] {
        let result = format_source("Main.tz", source, SourceKind::Code).unwrap();
        assert_eq!(result.formatted, expected);
        assert_eq!(
            format_source("Main.tz", expected, SourceKind::Code)
                .unwrap()
                .formatted,
            expected
        );
    }
}

#[test]
fn spacing_preserves_calls_indices_generics_and_prefix_operators() {
    for (source, expected) in [
        ("let value=1+2\nvalue", "let value = 1 + 2\nvalue\n"),
        ("f (1,2)", "f (1, 2)\n"),
        ("f(1,2)", "f(1, 2)\n"),
        ("f [1,2]", "f [1, 2]\n"),
        ("f[1]", "f[1]\n"),
        (
            "record Pair<'a,'b>{left:'a,right:'b}",
            "record Pair<'a, 'b> { left: 'a, right: 'b }\n",
        ),
        (
            "def get::&i64->i64\nfn get value=*value",
            "def get :: &i64 -> i64\nfn get value = *value\n",
        ),
        (
            "Maybe{let! value=Some 1;return value}",
            "Maybe { let! value = Some 1; return value }\n",
        ),
        (
            "Maybe{use ! value=Some 1;return value}",
            "Maybe { use! value = Some 1; return value }\n",
        ),
        ("use  held=make 1\nheld", "use held = make 1\nheld\n"),
        (
            "let f=\\ x->\\ y->\\ z->x+y+z\nf 2 3 4",
            "let f = \\x -> \\y -> \\z -> x + y + z\nf 2 3 4\n",
        ),
        (
            "let add=\\ left right->left+right\nadd 20 22",
            "let add = \\left right -> left + right\nadd 20 22\n",
        ),
        (
            "(\\ (value:i64)->value+1) 41",
            "(\\(value: i64) -> value + 1) 41\n",
        ),
        ("(\\ ()->42) ()", "(\\() -> 42) ()\n"),
        ("let value = - 1\nvalue", "let value = - 1\nvalue\n"),
    ] {
        let formatted = format_source("Main.tz", source, SourceKind::Code)
            .unwrap_or_else(|error| panic!("{source}: {}", error.message));
        assert_eq!(formatted.formatted, expected);
    }
}

#[test]
fn inline_definitions_and_implicit_unions_preserve_layout_and_fingerprints() {
    for source in [
        "def add::i64->i64->i64 =\n  \\left->\\right->left+right\nadd 20 22\n",
        "def rec even::i64->bool=\\value->if value==0 then true else odd (value-1)\nand odd::i64->bool=\\value->if value==0 then false else even (value-1)\n",
        "def (|Even|Odd|)::i64->'T =\n  \\value->if value%2==0 then Even else Odd\nmatch 42 with |Even->42|Odd->0|_-> -1\n",
        "def (|Small|Large|)::'a->'T=\\value->if true then Small value else Large value\n",
    ] {
        let first = format_source("Main.tz", source, SourceKind::Code).unwrap();
        let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
        assert_eq!(first.formatted, second.formatted);
        tsuzuri::analyze(&first.formatted).unwrap();
    }
}

#[test]
fn formats_exclusive_slices() {
    let source = "def fill::ref mut [i64..]->i64->unit=\\values value->Array.write values 0 value\nlet mut values:[i64]=[1,2,3]\nlet tail=ref mut values[1..]\nfill tail 2\nlet head=&mut values[..2]\nfill head 1\nlet whole:ref mut [i64..]=ref mut values[0..]\nwhole.length\n";
    let first = format_source("Main.tz", source, SourceKind::Code).unwrap();
    let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
    assert_eq!(first.formatted, second.formatted);
    assert!(first.formatted.contains("ref mut [i64..] -> i64 -> unit"));
    assert!(first.formatted.contains("ref mut values[1 ..]"));
    assert!(first.formatted.contains("values[.. 2]"));
    let before = parser::parse(source).unwrap();
    let after = parser::parse(&first.formatted).unwrap();
    assert_eq!(ast_fingerprint(before), ast_fingerprint(after));
    tsuzuri::analyze(&first.formatted).unwrap();
}

#[test]
fn formats_fixed_arrays_and_length_parameters() {
    let source = "record Grid<'a,const N:i64>{cells:['a;N]}\ndef total::[i64;N]->i64=\\v->v.length\ndef first::Grid<f64,3>->f64=\\g->g.cells[0]\nlet v:[i64;3]=[1,2,3]\nlet mut sum=0i64\nfor index in 0i64 .. 2 do sum=sum+v[index]\nsum+total v\n";
    let first = format_source("Main.tz", source, SourceKind::Code).unwrap();
    let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
    assert_eq!(first.formatted, second.formatted);
    assert!(
        first
            .formatted
            .contains("record Grid<'a, const N: i64> { cells: ['a; N] }"),
        "{}",
        first.formatted
    );
    assert!(
        first.formatted.contains("def total :: [i64; N] -> i64"),
        "{}",
        first.formatted
    );
    assert!(
        first.formatted.contains("Grid<f64, 3> -> f64"),
        "{}",
        first.formatted
    );
    assert!(
        first.formatted.contains("let v: [i64; 3] = [1, 2, 3]"),
        "{}",
        first.formatted
    );
    // A statement's range keeps its spaces (its implicit `unit` annotation is no type text).
    assert!(
        first.formatted.contains("for index in 0i64 .. 2 do"),
        "{}",
        first.formatted
    );
    let before = parser::parse(source).unwrap();
    let after = parser::parse(&first.formatted).unwrap();
    assert_eq!(ast_fingerprint(before), ast_fingerprint(after));
    tsuzuri::analyze(&first.formatted).unwrap();
}

#[test]
fn lambda_guards_and_where_preserve_layout() {
    for source in [
        "def choose::i64->i64=\\value->\n  |diff>10->diff\n  |otherwise->0\n  where\n    diff=Int.abs value\n",
        "def choose::i64->i64\nfn choose value\n  |diff>10->diff\n  |otherwise->0\n  where\n    diff=Int.abs value\n",
        "let choose=\\left->\\right->\n  |total>0->total\n  |otherwise->0\n  where\n    first:i64=left+1\n    total=first+right\nchoose 20 21\n",
        "def (|Even|Odd|)::i64->'T=\\value->\n  |value%2==0->Even\n  |otherwise->Odd\nmatch 42 with |Even->42|Odd->0|_-> -1\n",
        "def (|Even|Odd|)::i64->'T=\\value->\n  |remainder==0->Even value\n  |otherwise->Odd value\n  where\n    remainder=value%2\nmatch 42 with |Even number->number|Odd _->0|_-> -1\n",
        "def (|Even|Odd|)::'number->'T\n  @'number:Integer,Copy=\\value->\n  |remainder==0->Even value\n  |otherwise->Odd value\n  where\n    remainder=value%2\nmatch 42i32 with |Even number->number|Odd _->0|_-> -1\n",
    ] {
        let first = format_source("Main.tz", source, SourceKind::Code)
            .unwrap_or_else(|error| panic!("{source}: {}", error.message));
        let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
        assert_eq!(first.formatted, second.formatted);
        assert_eq!(
            ast_fingerprint(parser::parse(source).unwrap()),
            ast_fingerprint(parser::parse(&first.formatted).unwrap())
        );
        tsuzuri::analyze(&first.formatted).unwrap();
    }
}

#[test]
fn namespace_and_using_headers_keep_their_lines() {
    let source = "namespace   Sample::Features\nusing  Sample   // shared\nusing Other::Tools\n\ndef main::unit->i32=\\()->Tools::Math.add 40 2\n";
    let formatted = format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert_eq!(
        formatted,
        "namespace Sample::Features\nusing Sample   // shared\nusing Other::Tools\n\ndef main :: unit -> i32 = \\() -> Tools::Math.add 40 2\n"
    );
    let program = parser::parse(&formatted).unwrap();
    assert_eq!(
        program.namespace.as_ref().unwrap().path.text,
        "Sample::Features"
    );
    let usings: Vec<_> = program
        .usings
        .iter()
        .map(|using| using.path.text.as_str())
        .collect();
    assert_eq!(usings, ["Sample", "Other::Tools"]);
    assert_eq!(
        ast_fingerprint(parser::parse(source).unwrap()),
        ast_fingerprint(program)
    );
}

#[test]
fn layout_tracks_bodies_delimiters_and_else_owners() {
    for (source, expected) in [
        (
            "def choose :: bool -> i64\nfn choose flag =\n  if flag then\n    1\n  else\n    2\n",
            "def choose :: bool -> i64\nfn choose flag =\n    if flag then\n        1\n    else\n        2\n",
        ),
        (
            "record Point {\n  x:i64,\n  y:i64\n}\n",
            "record Point {\n    x: i64,\n    y: i64\n}\n",
        ),
        (
            "def nested :: bool -> bool -> i64\nfn nested first second =\n  if first then\n    if second then\n      1\n    else\n      2\n  else\n    3\n",
            "def nested :: bool -> bool -> i64\nfn nested first second =\n    if first then\n        if second then\n            1\n        else\n            2\n    else\n        3\n",
        ),
    ] {
        let formatted = format_source("Main.tz", source, SourceKind::Code)
            .unwrap_or_else(|error| panic!("{source}: {}", error.message));
        assert_eq!(formatted.formatted, expected);
        assert_eq!(
            format_source("Main.tz", expected, SourceKind::Code)
                .unwrap()
                .formatted,
            expected
        );
    }
}

#[test]
fn inline_control_alignment_follows_formatted_keyword_positions() {
    for source in [
        "def choose::bool->i64\nfn choose flag=(if flag then\n                 1\n                else\n                 2)\n",
        "def choose::bool->i64\nfn choose flag=match flag with\n               |true->1\n               |false->0\n",
    ] {
        let first = format_source("Main.tz", source, SourceKind::Code).unwrap();
        let second = format_source("Main.tz", &first.formatted, SourceKind::Code).unwrap();
        assert_eq!(first.formatted, second.formatted);
        assert_eq!(
            ast_fingerprint(parser::parse(source).unwrap()),
            ast_fingerprint(parser::parse(&first.formatted).unwrap())
        );
    }
    assert_eq!(
        format_source("Main.tz", "let value = (", SourceKind::Code)
            .unwrap_err()
            .code,
        "E0002"
    );
}

#[test]
fn fingerprints_ignore_positions_but_preserve_program_structure() {
    for (before, after) in [
        ("let value=1\nvalue", "let value = 1\nvalue\n"),
        (
            "(\\(left, right) -> left+right) (1,2)",
            "(\\(left, right) -> left + right) (1, 2)\n",
        ),
        (
            "def (|Whole|) :: i64 -> i64\nfn (|Whole|) value=value",
            "def (|Whole|) :: i64 -> i64\nfn (|Whole|) value = value\n",
        ),
    ] {
        assert_eq!(
            ast_fingerprint(parser::parse(before).unwrap()),
            ast_fingerprint(parser::parse(after).unwrap())
        );
    }
    assert_ne!(
        ast_fingerprint(parser::parse("f (1, 2)").unwrap()),
        ast_fingerprint(parser::parse("f(1, 2)").unwrap())
    );
    assert_ne!(
        ast_fingerprint(parser::parse("f [1]").unwrap()),
        ast_fingerprint(parser::parse("f[1]").unwrap())
    );
}

#[test]
fn corpus_is_idempotent_and_semantically_unchanged() {
    use std::{fs, path::Path};
    fn visit(path: &Path, count: &mut usize) {
        let mut entries: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(&path, count);
            } else if let Some(kind) = path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(SourceKind::from_extension)
            {
                let source = fs::read_to_string(&path).unwrap();
                let first = format_source(&path.to_string_lossy(), &source, kind)
                    .unwrap_or_else(|error| panic!("{}: {}", path.display(), error.message));
                let second =
                    format_source(&path.to_string_lossy(), &first.formatted, kind).unwrap();
                assert_eq!(first.formatted, second.formatted, "{}", path.display());
                assert_eq!(
                    ast_fingerprint(parser::parse(&source).unwrap()),
                    ast_fingerprint(parser::parse(&first.formatted).unwrap()),
                    "{}",
                    path.display()
                );
                *count += 1;
            }
        }
    }
    let mut count = 0;
    for directory in ["std", "tests/fixtures", "examples", "benchmarks"] {
        visit(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join(directory),
            &mut count,
        );
    }
    assert!(count > 50, "{count} source files");
}

#[test]
fn cli_formats_flat_directories_without_a_compiler_or_entry_point() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-fmt-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let source = "let value=1+2\nvalue  ";
    let input = root.join("Example.tz");
    fs::write(&input, source).unwrap();
    fs::write(
        root.join("Traits.tt"),
        "class Score<'a>{def score::'a->i64}",
    )
    .unwrap();
    fs::write(
        root.join("Builder.tc"),
        "def Return::'a->'a\nfn Return value=value",
    )
    .unwrap();
    fs::write(root.join("notes.txt"), "  unchanged  ").unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/Other.tz"), source).unwrap();
    let cli = |check: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .arg("fmt")
            .arg(&root)
            .arg("--json")
            .env("TSUZURI_CLANG", root.join("missing-clang"));
        if check {
            command.arg("--check");
        }
        command.output().unwrap()
    };
    let checked = cli(true);
    assert_eq!(checked.status.code(), Some(1));
    let stderr = String::from_utf8(checked.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 3, "{stderr}");
    assert!(stderr.contains("file is not formatted"));
    assert_eq!(fs::read_to_string(&input).unwrap(), source);
    let formatted = cli(false);
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    assert!(formatted.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(&input).unwrap(),
        "let value = 1 + 2\nvalue\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("notes.txt")).unwrap(),
        "  unchanged  "
    );
    assert_eq!(
        fs::read_to_string(root.join("nested/Other.tz")).unwrap(),
        source
    );
    let modified = fs::metadata(&input).unwrap().modified().unwrap();
    assert!(cli(false).status.success());
    assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), modified);
    assert!(cli(true).status.success());
    fs::create_dir(root.join("bad")).unwrap();
    fs::write(root.join("bad/A.tz"), source).unwrap();
    fs::write(root.join("bad/Z.tz"), "let broken = (").unwrap();
    assert!(tsuzuri::driver::format_sources(&root.join("bad"), false).is_err());
    assert_eq!(fs::read_to_string(root.join("bad/A.tz")).unwrap(), source);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn atomic_source_updates_preserve_modes_and_reject_symlinks() {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-fmt-mode-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let input = root.join("Source.tz");
    let linked = root.join("original.txt");
    let source = "let value=1\nvalue";
    fs::write(&input, source).unwrap();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o640)).unwrap();
    fs::hard_link(&input, &linked).unwrap();
    tsuzuri::driver::format_sources(&input, false).unwrap();
    assert_eq!(
        fs::metadata(&input).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(fs::read_to_string(linked).unwrap(), source);
    let link = root.join("Link.tz");
    symlink(&input, &link).unwrap();
    assert_eq!(
        tsuzuri::driver::format_sources(&link, false)
            .unwrap_err()
            .diagnostic
            .code,
        "E2003"
    );
    let directory_link = root.join("directory-link");
    symlink(&root, &directory_link).unwrap();
    assert_eq!(
        tsuzuri::driver::format_sources(&directory_link, false)
            .unwrap_err()
            .diagnostic
            .code,
        "E2003"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trivia_reconstructs_source_without_changing_tokens() {
    let source = "let value=1 // keep  \r\n/* outer\n  /* nested */ keep  \r\n*/\nvalue\n\n";
    let plain = lex(source).unwrap();
    let retained = lex_with_trivia(source).unwrap();
    assert_eq!(plain.len(), retained.len());
    let mut restored = String::new();
    for (original, retained) in plain.iter().zip(&retained) {
        assert_eq!(original.kind, retained.token.kind);
        assert_eq!(original.span, retained.token.span);
        for trivia in &retained.leading {
            assert_eq!(trivia.text, source[trivia.span.start..trivia.span.end]);
            restored.push_str(&trivia.text);
        }
        restored.push_str(&source[retained.token.span.start..retained.token.span.end]);
    }
    assert_eq!(restored, source);
    let comments: Vec<_> = retained
        .iter()
        .flat_map(|token| &token.leading)
        .filter(|trivia| {
            matches!(
                trivia.kind,
                TriviaKind::LineComment | TriviaKind::BlockComment
            )
        })
        .collect();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].text, "// keep  ");
    assert_eq!(comments[1].text, "/* outer\n  /* nested */ keep  \r\n*/");
}

#[test]
fn bom_is_metadata_not_comment_or_token_text() {
    let source = "\u{feff}let value = '\\uD800'\r\nvalue\r\n";
    let tokens = lex_with_trivia(source).unwrap();
    assert_eq!(tokens[0].token.span.start, 3);
    assert!(tokens[0].leading.is_empty());
}
