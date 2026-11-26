use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/range-family")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

#[test]
fn inclusive_slice_end_of_i64u_max_traps() {
    let root = scratch_dir("u64-end");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"u64-end\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "test \"valid_u64_inclusive_end\" =\n\
         \x20   let xs = [10, 20, 30, 40, 50]\n\
         \x20   let all = ref xs[0 ..= 4i64u]\n\
         \x20   assert (all.length == 5 && all[4] == 50)\n",
    )
    .unwrap();
    let ok = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // `u64::MAX` (`18446744073709551615i64u`) must trap rather than wrapping `-1 + 1` to `0`.
    std::fs::write(
        root.join("Main.tz"),
        "export def bad :: [i64] -> i64 = \\xs -> (ref xs[0 ..= 18446744073709551615i64u]).length\n\
         test \"u64_max_traps\" =\n\
         \x20   let xs = [10, 20, 30, 40, 50]\n\
         \x20   let _ = bad xs\n\
         \x20   assert true\n",
    )
    .unwrap();
    let trapped = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !trapped.status.success(),
        "expected u64::MAX inclusive slice end to trap"
    );
    let stdout = String::from_utf8_lossy(&trapped.stdout);
    assert!(stdout.contains("not ok 1"), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn plain_index_of_i64u_keeps_its_ir() {
    let m_u64 = tsuzuri::analyze("def get :: [i64] -> i64u -> i64\nfn get xs i = xs[i]\n").unwrap();
    let m_cast =
        tsuzuri::analyze("def get :: [i64] -> i64u -> i64\nfn get xs i = xs[i as i64]\n").unwrap();
    let ir_u64 = tsuzuri::llvm::emit(&m_u64, tsuzuri::llvm::Entry::Library).unwrap();
    let ir_cast = tsuzuri::llvm::emit(&m_cast, tsuzuri::llvm::Entry::Library).unwrap();
    assert_eq!(ir_u64, ir_cast);
}

#[test]
fn lexer_longest_match_for_new_range_operators() {
    use tsuzuri::lexer::lex;
    use tsuzuri::syntax::{Edition, TokenKind::*};
    for edition in [Edition::E2026, Edition::E2027, Edition::E2028] {
        let kinds = |src: &str| -> Vec<tsuzuri::syntax::TokenKind> {
            lex(src, edition)
                .unwrap()
                .into_iter()
                .map(|t| t.kind)
                .filter(|k| !matches!(k, End))
                .collect()
        };
        assert_eq!(
            kinds("a<..b"),
            vec![Ident("a".into()), LessDotDot, Ident("b".into())]
        );
        assert_eq!(
            kinds("a<..<b"),
            vec![Ident("a".into()), LessDotDotLess, Ident("b".into())]
        );
        assert_eq!(
            kinds("a < ..b"),
            vec![Ident("a".into()), Less, DotDot, Ident("b".into())]
        );
        assert_eq!(
            kinds("a << b <= c ..< d"),
            vec![
                Ident("a".into()),
                DoubleLess,
                Ident("b".into()),
                LessEqual,
                Ident("c".into()),
                DotDotLess,
                Ident("d".into()),
            ]
        );
    }
}

#[test]
fn new_forms_need_edition_2028() {
    use tsuzuri::parser::parse_with_source_all;
    use tsuzuri::syntax::Edition;
    let cases = [
        (
            "let xs = [1, 2, 3]\nlet s = ref xs[0<..2]\n0\n",
            "'<..' requires edition 2028",
        ),
        (
            "let xs = [1, 2, 3]\nlet s = ref xs[0<..<2]\n0\n",
            "'<..<' requires edition 2028",
        ),
        (
            "let xs = [1, 2, 3]\nlet x = xs[^0]\n0\n",
            "'^' (a position from the end) requires edition 2028",
        ),
        (
            "let xs = [1, 2, 3]\nlet s = ref xs[0..^1]\n0\n",
            "'^' (a position from the end) requires edition 2028",
        ),
        (
            "let xs = [1, 2, 3, 4]\nlet s = ref xs[0 .. 2 .. 3]\n0\n",
            "a step inside brackets requires edition 2028",
        ),
        ("for i in 0 <.. 3 do ()\n0\n", "'<..' requires edition 2028"),
        (
            "for i in 0 <..< 3 do ()\n0\n",
            "'<..<' requires edition 2028",
        ),
    ];
    for edition in [Edition::E2026, Edition::E2027] {
        for (src, expected_msg) in cases {
            let errs = parse_with_source_all(src, 0, edition).unwrap_err();
            assert_eq!(errs[0].code, "E0002", "{edition} {src}");
            assert_eq!(
                errs[0].kind,
                Some("range_form_needs_edition_2028"),
                "{edition} {src}"
            );
            assert_eq!(errs[0].message, expected_msg, "{edition} {src}");
            assert!(
                errs[0]
                    .help
                    .as_deref()
                    .is_some_and(|h| h.contains("tsuzuri fix --edition 2028")),
                "{edition} {src}"
            );
        }
    }
}

#[test]
fn from_end_is_only_a_component_start() {
    use tsuzuri::parser::parse_with_source_all;
    use tsuzuri::syntax::Edition;

    // Infix XOR `k ^ 1` / `k ^1` inside brackets remains bitwise XOR in edition 2028.
    let xor_mod = tsuzuri::analyze_with_edition(
        "def pick :: [i64] -> i64 -> i64 = \\xs k -> xs[k ^1]\n",
        Edition::E2028,
    )
    .unwrap();
    let ir = tsuzuri::llvm::emit(&xor_mod, tsuzuri::llvm::Entry::Library).unwrap();
    assert!(ir.contains("xor i64"), "{ir}");

    // Valid term operands for `^`: `^ys[0]`, `^(i + 1)`, `^r.k`.
    tsuzuri::analyze_with_edition(
        "record R { k: i64 }\n\
         def ok :: [i64] -> [i64] -> R -> i64 -> i64 = \\xs ys r i ->\n\
         \x20   xs[^ys[0]] + xs[^(i + 1)] + xs[^r.k] + (ref xs[^ys[0] .. ^(i + 1)]).length\n",
        Edition::E2028,
    )
    .unwrap();

    // `^` followed by binary operator without parentheses -> `from_end_in_expression`.
    let errs =
        parse_with_source_all("let xs = [1, 2, 3]\nxs[^1 ^ 2]\n", 0, Edition::E2028).unwrap_err();
    assert_eq!(errs[0].kind, Some("from_end_in_expression"));
    let errs =
        parse_with_source_all("let xs = [1, 2, 3]\nxs[^1 + 1]\n", 0, Edition::E2028).unwrap_err();
    assert_eq!(errs[0].kind, Some("from_end_in_expression"));

    // `^` outside index/slice bound -> `from_end_outside_index` in every edition.
    for src in [
        "let xs = [^1]\n0\n",
        "let xs = [1, 2]\nxs[(^0)]\n",
        "for i in 0 ..< ^1 do ()\n0\n",
        "let x = ^0\nx\n",
    ] {
        for edition in [Edition::E2026, Edition::E2027, Edition::E2028] {
            let errs = parse_with_source_all(src, 0, edition).unwrap_err();
            assert_eq!(
                errs[0].kind,
                Some("from_end_outside_index"),
                "{edition}: {src}"
            );
        }
    }

    // `^` at start of step -> `from_end_step`.
    let errs = parse_with_source_all(
        "let xs = [1, 2, 3]\nref xs[0 .. ^1 .. 2]\n",
        0,
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(errs[0].kind, Some("from_end_step"));
}

#[test]
fn malformed_range_family_forms() {
    use tsuzuri::parser::parse_with_source_all;
    use tsuzuri::syntax::Edition;

    for (src, kind, snippet) in [
        (
            "let xs = [1, 2, 3]\nref xs[<..2]\n",
            "start_open_range_needs_start",
            "'<..' excludes its start",
        ),
        (
            "let xs = [1, 2, 3]\nref xs[<..<2]\n",
            "start_open_range_needs_start",
            "'<..<' excludes its start",
        ),
        (
            "let xs = [1, 2, 3]\nref xs[0 <.. 1 .. 2]\n",
            "start_open_range_with_step",
            "'<..' takes no step",
        ),
        (
            "let xs = [1, 2, 3]\nref xs[0 .. 1 <.. 2]\n",
            "start_open_range_with_step",
            "'<..' takes no step",
        ),
        (
            "for i in 0 <.. 1 .. 4 do ()\n0\n",
            "start_open_range_with_step",
            "'<..' takes no step",
        ),
        (
            "for i in 0 .. 1 <..< 4 do ()\n0\n",
            "start_open_range_with_step",
            "'<..<' takes no step",
        ),
        (
            "let xs = [1, 2, 3]\nref xs[2 .. -1 ..< 0]\n",
            "slice_step_down_excludes_end",
            "cannot exclude its end",
        ),
        (
            "let xs = [1, 2, 3]\nref xs[1<..<]\n",
            "slice_range_needs_end",
            "a slice with '<..<' needs an end bound",
        ),
    ] {
        let errs = parse_with_source_all(src, 0, Edition::E2028).unwrap_err();
        assert_eq!(errs[0].code, "E0002", "{src}");
        assert_eq!(errs[0].kind, Some(kind), "{src}: {:?}", errs[0]);
        assert!(
            errs[0].message.contains(snippet),
            "{src}: {}",
            errs[0].message
        );
    }
}

#[test]
fn bare_slice_in_2028_has_the_ir_of_a_closed_slice() {
    use tsuzuri::syntax::Edition;
    let bare_2028 = tsuzuri::analyze_with_edition(
        "export def sub :: [i64] -> i64 -> i64 -> i64 = \\xs a b -> (ref xs[a..b]).length\n",
        Edition::E2028,
    )
    .unwrap();
    let closed_2027 = tsuzuri::analyze_with_edition(
        "export def sub :: [i64] -> i64 -> i64 -> i64 = \\xs a b -> (ref xs[a..=b]).length\n",
        Edition::E2027,
    )
    .unwrap();
    assert_eq!(
        tsuzuri::llvm::emit(&bare_2028, tsuzuri::llvm::Entry::Library).unwrap(),
        tsuzuri::llvm::emit(&closed_2027, tsuzuri::llvm::Entry::Library).unwrap(),
    );

    let open_ir = |ed| {
        let m = tsuzuri::analyze_with_edition(
            "export def sub :: ref [i64] -> i64 -> i64 -> i64 = \\xs a b -> (ref xs[a..<b]).length\n",
            ed,
        )
        .unwrap();
        tsuzuri::llvm::emit(&m, tsuzuri::llvm::Entry::Library).unwrap()
    };
    let ir26 = open_ir(Edition::E2026);
    assert_eq!(ir26, open_ir(Edition::E2027));
    assert_eq!(ir26, open_ir(Edition::E2028));
}

#[test]
fn start_open_for_loops() {
    let root = scratch_dir("start-open-for");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"start-open-for\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "const def c_open :: i64 -> i64 = \\n ->\n\
         \x20   let mut s: i64 = 0\n\
         \x20   for i in 0 <..< n do\n\
         \x20       s = s * 10 + i\n\
         \x20   s\n\
         const def c_edge :: unit -> i64 = \\() ->\n\
         \x20   let mut count: i64 = 0\n\
         \x20   for _i in 127i8 <.. 127i8 do\n\
         \x20       count = count + 1\n\
         \x20   count\n\
         test \"start_open_runtime_and_const\" =\n\
         \x20   let mut a = 0\n\
         \x20   for i in 0 <..< 3 do\n\
         \x20       a = a * 10 + i\n\
         \x20   let mut b = 0\n\
         \x20   for i in 0 <.. 3 do\n\
         \x20       b = b * 10 + i\n\
         \x20   let mut c = 0\n\
         \x20   for _i in 127i8 <.. 127i8 do\n\
         \x20       c = c + 1\n\
         \x20   for _i in 255i8u <.. 255i8u do\n\
         \x20       c = c + 1\n\
         \x20   assert (a == 12 && b == 123 && c == 0 && c_open 3 == 12 && c_edge () == 0)\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stepped_ranges_in_array_brackets_are_e1005() {
    use tsuzuri::syntax::Edition;
    for src in [
        "let xs = [1, 2, 3, 4]\nlet s = ref xs[0 .. 2 .. 3]\ns.length\n",
        "let xs: [i64; 4] = [1, 2, 3, 4]\nlet s = ref xs[.. 2 ..]\ns.length\n",
    ] {
        let err = tsuzuri::analyze_with_edition(src, Edition::E2028).unwrap_err();
        assert_eq!(err.code, "E1005");
        assert_eq!(err.kind, Some("array_slice_with_step"));
        assert!(
            err.help
                .as_deref()
                .is_some_and(|h| h.contains("first .. step .. last"))
        );
    }
}

#[test]
fn from_end_needs_a_builtin_length_and_rejects_const() {
    use tsuzuri::syntax::Edition;
    let err = tsuzuri::analyze_with_edition(
        "let m = Map.insert 0 10 (Map.empty())\nm[^0]\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err.code, "E1005");
    assert_eq!(err.kind, Some("from_end_needs_length"));
    assert!(err.message.contains("Map<i32, i32>") || err.message.contains("Map"));

    let err_const = tsuzuri::analyze_with_edition(
        "const def last_elem :: i64 = { let xs = [10i64, 20]; xs[^0] }\n0\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err_const.code, "E1026");
    assert_eq!(err_const.kind, Some("const_operation_not_allowed"));
}

#[test]
fn checked_slice_bounds_do_not_raise_overflow_exception() {
    let root = scratch_dir("checked-slice");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"checked-slice\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    // Valid `@checked` slice with `^k` and `<..` succeeds; `b = i64.max` traps with BoundsCheck
    // instead of being caught as OverflowException by `try ... with`.
    std::fs::write(
        root.join("Main.tz"),
        "def ok :: [i64] -> Result<i64, Exception> = \\xs ->\n\
         \x20   try\n\
         \x20       @checked (ref xs[0 <.. ^0]).length\n\
         \x20   with\n\
         \x20   | e is OverflowException -> e\n\
         test \"checked_slice_ok\" =\n\
         \x20   let r = match ok [10, 20, 30] with | Ok n -> n | Error _ -> -1\n\
         \x20   assert (r == 2)\n",
    )
    .unwrap();
    let ok = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    std::fs::write(
        root.join("Main.tz"),
        "def trap_not_overflow :: [i64] -> i64 -> Result<i64, Exception> = \\xs b ->\n\
         \x20   try\n\
         \x20       @checked (ref xs[0 .. b]).length\n\
         \x20   with\n\
         \x20   | e is OverflowException -> e\n\
         test \"traps_on_i64_max\" =\n\
         \x20   let _ = trap_not_overflow [10, 20, 30] 9223372036854775807i64\n\
         \x20   assert true\n",
    )
    .unwrap();
    let trapped = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!trapped.status.success());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cross_edition_packages_preserve_each_modules_slice_meaning() {
    let root = scratch_dir("cross-edition");
    std::fs::create_dir_all(root.join("dep")).unwrap();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(
        root.join("dep/Tsuzuri.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2027\"\nnamespace = \"Dep\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("dep/Slice27.tz"),
        "namespace Dep\n\
         def concrete_len :: [i64] -> i64 = \\xs -> (ref xs[0..2]).length\n\
         def generic_len :: ['a] -> i64 = \\xs -> (ref xs[0..2]).length\n",
    )
    .unwrap();
    std::fs::write(
        root.join("app/Tsuzuri.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2028\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("app/Main.tz"),
        "test \"dep_2027_stays_half_open_while_root_2028_is_closed\" =\n\
         \x20   let xs = [10, 20, 30, 40]\n\
         \x20   let dep_c = Dep::Slice27.concrete_len xs\n\
         \x20   let dep_g = Dep::Slice27.generic_len xs\n\
         \x20   let own_s = (ref xs[0..2]).length\n\
         \x20   assert (dep_c == 2 && dep_g == 2 && own_s == 3)\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "test",
            root.join("app").to_str().unwrap(),
            "--deny-warnings",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn legacy_bare_slice_range_warns_with_a_certain_fix() {
    use tsuzuri::syntax::Edition;
    for edition in [Edition::E2026, Edition::E2027] {
        let m = tsuzuri::analyze_with_edition(
            "let xs = [1, 2, 3, 4]\nlet a = ref xs[1..3]\nlet b = ref xs[1..]\na.length + b.length\n",
            edition,
        )
        .unwrap();
        assert_eq!(m.warnings.len(), 1, "{edition}");
        assert_eq!(m.warnings[0].code, "W1013");
        assert_eq!(m.warnings[0].kind, Some("legacy_bare_slice_range"));
        assert!(
            m.warnings[0]
                .message
                .contains(&format!("in edition {edition}"))
        );
        assert_eq!(m.warnings[0].fixes.len(), 1);
    }
    let m28 = tsuzuri::analyze_with_edition(
        "let xs = [1, 2, 3, 4]\nlet a = ref xs[1..3]\na.length\n",
        Edition::E2028,
    )
    .unwrap();
    assert!(m28.warnings.is_empty());
}

#[test]
fn negative_literal_bounds_warn() {
    use tsuzuri::syntax::Edition;
    for edition in [Edition::E2026, Edition::E2027, Edition::E2028] {
        let src = if edition.range_family() {
            "let xs = [1, 2, 3]\nlet a = ref xs[..-1]\nlet b = ref xs[-1<..2]\nlet c = ref xs[..^-1]\n\
             let ok1 = ref xs[-0..1]\nlet ok2 = ref xs[-0x0..<1]\nlet ok3 = ref xs[-00..<1]\n\
             a.length + b.length + c.length + ok1.length + ok2.length + ok3.length\n"
        } else {
            "let xs = [1, 2, 3]\nlet a = ref xs[..=-1]\nlet b = ref xs[-1..<2]\n\
             let ok1 = ref xs[-0..<1]\nlet ok2 = ref xs[-0x0..<1]\nlet ok3 = ref xs[-00..<1]\n\
             a.length + b.length + ok1.length + ok2.length + ok3.length\n"
        };
        let m = tsuzuri::analyze_with_edition(src, edition).unwrap();
        assert_eq!(
            m.warnings.len(),
            if edition.range_family() { 3 } else { 2 },
            "{edition}: {:?}",
            m.warnings
        );
        for w in &m.warnings {
            assert_eq!(w.code, "W1014");
            assert_eq!(w.kind, Some("negative_literal_range_bound"));
            assert!(w.fixes.is_empty());
            if edition.range_family() {
                assert!(w.help.as_deref().unwrap().contains("ref xs[..^1]"));
            } else {
                assert!(w.help.as_deref().unwrap().contains("xs.length"));
            }
        }
    }
}

#[test]
fn negative_literal_index_warns() {
    use tsuzuri::syntax::Edition;

    // 6 built-in types + `ref xs[-1]` + `xs[(-1)]` + `xs[-1] = v` + hex & i64::MIN magnitudes.
    let src_27 = "\
        def check_all :: unit -> i64 = \\() ->\n\
        \x20   let mut arr = [10i64, 20, 30]\n\
        \x20   let fixed: [i64; 3] = [10, 20, 30]\n\
        \x20   let lst = [|10i64, 20, 30|]\n\
        \x20   let mut vec: Vec<i64> = Vec.empty()\n\
        \x20   Vec.push 10 vec\n\
        \x20   let s = \"abc\"\n\
        \x20   let u = u8\"abc\"\n\
        \x20   arr[-1] = 99\n\
        \x20   let r = ref arr[-3]\n\
        \x20   let p = arr[(-1)]\n\
        \x20   let hx = arr[-0x10]\n\
        \x20   let mn = arr[-9223372036854775808i64]\n\
        \x20   arr[-1] + fixed[-1] + lst[-1] + vec[-1] + (s[-1] as i64) + (u[-1] as i64) + deref r + p + hx + mn\n";
    let m27 = tsuzuri::analyze_with_edition(src_27, Edition::E2027).unwrap();
    assert_eq!(m27.warnings.len(), 11, "{:?}", m27.warnings);
    for w in &m27.warnings {
        assert_eq!(w.code, "W1014");
        assert_eq!(w.kind, Some("negative_literal_index"));
        assert!(w.fixes.is_empty());
    }
    assert!(
        m27.warnings[1]
            .help
            .as_deref()
            .unwrap()
            .contains("xs[xs.length - 3]")
    );
    assert!(m27.warnings[1].help.as_deref().unwrap().contains("xs[^2]"));
    assert!(
        m27.warnings[3]
            .help
            .as_deref()
            .unwrap()
            .contains("xs[xs.length - 16]")
    );
    assert!(m27.warnings[3].help.as_deref().unwrap().contains("xs[^15]"));
    assert!(
        m27.warnings[4]
            .help
            .as_deref()
            .unwrap()
            .contains("xs[xs.length - 9223372036854775808]")
    );

    let m28 = tsuzuri::analyze_with_edition(src_27, Edition::E2028).unwrap();
    assert_eq!(m28.warnings.len(), 11);
    assert_eq!(
        m28.warnings[1].help.as_deref(),
        Some("'^0' is the last element: write 'xs[^2]' for NumPy 'a[-3]'")
    );

    // Non-warning cases: `-0`/`-0x0`/`-00`, `0 - 1`, negative variable, const name, `Map`, `Tensor`,
    // custom `Index`, and `new [i64](-1, ...)`.
    let clean = "\
        const NEG: i64 = -1\n\
        record Bag { v: i64 }\n\
        instance Index<Bag, i64, i64> {\n\
        \x20   fn index k b = b.v + k\n\
        }\n\
        def no_warnings :: i64 -> i64 = \\neg ->\n\
        \x20   let xs = [10i64, 20]\n\
        \x20   let m: Map<i64, i64> = Map.insert (-1i64) 7i64 (Map.empty())\n\
        \x20   let t: Tensor<f64> = Tensor.zeros [2, 2]\n\
        \x20   let bag = Bag { v: 5 }\n\
        \x20   let _bad_alloc = \\() -> new [i64](-1, \\i -> i)\n\
        \x20   xs[-0] + xs[-0x0] + xs[-00] + xs[0 - 1] + xs[neg] + xs[NEG] + m[-1i64] + (t[[-1, 0]] as i64) + bag[-1]\n";
    let m_clean = tsuzuri::analyze_with_edition(clean, Edition::E2028).unwrap();
    assert!(m_clean.warnings.is_empty(), "{:?}", m_clean.warnings);

    // Slice bound `ref xs[..-1]` emits ONLY `negative_literal_range_bound`, never `negative_literal_index`.
    let m_slice = tsuzuri::analyze_with_edition(
        "let xs = [1, 2, 3]\n(ref xs[..-1]).length\n",
        Edition::E2028,
    )
    .unwrap();
    assert_eq!(m_slice.warnings.len(), 1);
    assert_eq!(
        m_slice.warnings[0].kind,
        Some("negative_literal_range_bound")
    );
}

#[test]
fn fmt_spacing_of_the_range_family() {
    use tsuzuri::formatter::format_source_with_edition;
    use tsuzuri::syntax::{Edition, SourceKind};
    let input = "def f :: [i64] -> i64 = \\xs ->\n    let a = ref xs[1<..3]\n    let b = ref xs[1<..<3]\n    let c = ref xs[1..^1]\n    let d = xs[^ 0]\n    for i in 0<..3 do ()\n    a.length + b.length + c.length + d\n";
    let formatted = format_source_with_edition("Main.tz", input, SourceKind::Code, Edition::E2028)
        .unwrap()
        .formatted;
    assert!(formatted.contains("ref xs[1 <.. 3]"), "{formatted}");
    assert!(formatted.contains("ref xs[1 <..< 3]"), "{formatted}");
    assert!(formatted.contains("ref xs[1 .. ^1]"), "{formatted}");
    assert!(formatted.contains("xs[^0]"), "{formatted}");
    assert!(formatted.contains("for i in 0 <.. 3 do"), "{formatted}");
    let again = format_source_with_edition("Main.tz", &formatted, SourceKind::Code, Edition::E2028)
        .unwrap()
        .formatted;
    assert_eq!(formatted, again);
}
