use tsuzuri::{analyze, llvm};

fn accepts(source: &str) {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    llvm::emit(&module, llvm::Entry::Library).unwrap();
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn checks_every_integer_width_signedness_and_boundary() {
    for bits in [8, 16, 32, 64, 128] {
        let signed_max = (1u128 << (bits - 1)) - 1;
        let unsigned_max = u128::MAX >> (128 - bits);
        for (ty, maximum) in [
            (format!("i{bits}"), signed_max),
            (format!("i{bits}u"), unsigned_max),
        ] {
            accepts(&format!(
                "fn f(x: {ty}) -> {ty} {{ let y: {ty} = {maximum}; (x + y) * 2 / 1 % 3 }}"
            ));
            accepts(&format!("fn f() -> {ty} {{ {maximum}{ty} }}"));
            if maximum != u128::MAX {
                rejects(&format!("fn f() -> {ty} {{ {} }}", maximum + 1), "E1009");
            }
            if ty.ends_with('u') {
                rejects(&format!("fn f() -> {ty} {{ -1 }}"), "E1009");
            } else {
                accepts(&format!("fn f() -> {ty} {{ -{} }}", signed_max + 1));
                rejects(
                    &format!("fn f() -> {ty} {{ -{} }}", signed_max + 2),
                    "E1009",
                );
            }
        }
    }
    accepts("fn f(x: byte) -> i8 { x } fn g(x: ubyte) -> i8u { x }");
    accepts("fn f(x: &i8) -> i128u { let y = *x as i128u; y + (-128i8 as i128u) }");
    rejects("fn f(x: i8) -> i16 { x }", "E1003");
    rejects("fn f(x: i8) -> i8u { x }", "E1003");
    rejects("fn f() -> Int { 1 }", "E1004");
    rejects("record f128 {}", "E1001");
}

#[test]
fn checks_all_float_formats_and_contextual_literals() {
    for ty in ["f16", "f32", "f64", "f128", "d32", "d64", "d128"] {
        accepts(&format!(
            "fn f(x: {ty}) -> {ty} {{ let a: {ty} = 0.1; (x + a) / 2.0 }}"
        ));
        accepts(&format!("fn f() -> {ty} {{ 1{ty} + -0.25{ty} }}"));
        accepts(&format!("fn f(x: {ty}) -> bool {{ 1.0 < x }}"));
        accepts(&format!("fn f(x: {ty}) -> d128 {{ x as d128 }}"));
    }
    accepts("fn f() -> f128 { 1.189731495357231765085759326628007e4932 }");
    accepts("fn f() -> d128 { 9.999999999999999999999999999999999e6144 }");
    rejects("fn f() -> f128 { 1e4933 }", "E1009");
    rejects("fn f() -> d128 { 1e6145 }", "E1009");
    rejects("fn f() -> d32 { 1.0f64 }", "E1003");
    rejects("fn f() -> f16 { 65520.0 }", "E1009");
}

#[test]
fn lowers_common_numeric_casts_to_native_instructions() {
    for bits in [8, 16, 32, 64] {
        for (suffix, instruction, sign) in [("", "sitofp", "s"), ("u", "uitofp", "u")] {
            for (float, llvm_float) in [(32, "float"), (64, "double")] {
                let source = format!(
                    "def to_float_value :: i{bits}{suffix} -> f{float}\n\
                     fn to_float_value x = x as f{float}\n\
                     def to_integer_value :: f{float} -> i{bits}{suffix}\n\
                     fn to_integer_value x = x as i{bits}{suffix}"
                );
                let module = analyze(&source).unwrap();
                let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
                assert!(ir.contains(&format!("{instruction} i{bits}")));
                let declaration =
                    format!("declare i{bits} @llvm.fpto{sign}i.sat.i{bits}.f{float}({llvm_float})");
                assert_eq!(ir.matches(&declaration).count(), 1);
                assert!(!ir.contains("@tz_soft_"));
            }
        }
    }
    let module = analyze(
        "def narrow :: f64 -> f32\nfn narrow x = x as f32\n\
         def widen :: f32 -> f64\nfn widen x = x as f64\n\
         def integer :: f64 -> i64\nfn integer x = (x as i64) + to_int x",
    )
    .unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("fptrunc double"));
    assert!(ir.contains("fpext float"));
    assert_eq!(
        ir.matches("declare i64 @llvm.fptosi.sat.i64.f64(double)")
            .count(),
        1
    );
    assert!(!ir.contains("@tz_soft_"));
}

#[test]
fn retains_exact_software_conversions_for_wide_and_decimal_types() {
    for (from, to) in [
        ("i128", "f32"),
        ("i128u", "f64"),
        ("f64", "i128"),
        ("f32", "i128u"),
        ("f16", "f32"),
        ("f64", "f16"),
        ("f128", "f64"),
        ("i64", "f128"),
        ("d32", "f64"),
        ("f32", "d64"),
        ("d128", "i64"),
    ] {
        let module = analyze(&format!(
            "def convert :: {from} -> {to}\nfn convert x = x as {to}"
        ))
        .unwrap();
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert!(ir.contains("call void @tz_soft_cast"), "{from} -> {to}");
    }
}

#[test]
fn accepts_moves_borrows_partial_moves_and_local_mutation() {
    for source in [
        "fn f() -> string { let a = \"hello\"; let b = a; b }",
        "fn f(s: &string) -> i64 { s.length } fn g() -> i64 { let s = \"日本語\"; f(&s) + f(&s) }",
        "fn f() -> string { let s = \"x\"; let r = &s; let _ = r.length; s }",
        "fn id(s: &string) -> &string { s } fn f() -> i64 { let s = \"x\"; id(&s).length }",
        "fn first(s: &[string]) -> &string { &s[0] } fn f() -> string { let a = [\"a\", \"b\"]; clone_string(first(&a)) }",
        "fn f(x: &mut i64) -> unit { *x = *x + 1; } fn g() -> i64 { let mut x = 1; f(&mut x); x }",
        "fn f() -> string { let mut x = \"a\"; let r = &mut x; *r = \"b\"; x }",
        "fn f() -> i64 { let mut x = 1; let a = &x; let b = &x; let n = *a + *b; x = n; x }",
        "fn f() -> i64 { let mut x = 1; let r = &mut x; let s = &mut *r; *s = 2; *r = 3; x }",
        "record R { x: string, y: string } fn f() -> string { let r = R { x: \"a\", y: \"b\" }; let x = r.x; x + r.y }",
        "record R { x: string, y: i64 } fn f() -> i64 { let r = R { x: \"a\", y: 1 }; let _ = r.x; r.y }",
        "fn f() -> bool { let s = \"x\"; s == s && { let t = \"x\"; t } == s }",
        "fn f() -> string { let mut s = \"a\"; let t = s; s = \"b\"; t + s }",
        "fn f(flag: bool) -> string { let s = \"a\"; if flag { s } else { s } }",
        "fn f() -> string { let s = \"\\u{1f600}\\n\\0\"; clone_string(&s) }",
        "fn f(x: i32) -> i32 { let a: [i32] = [x, 2, 3, 4]; let b = a; a[0] + b[1] }",
        "fn f() -> unit { (); {} }",
    ] {
        accepts(source);
    }
}

#[test]
fn rejects_use_after_move_borrow_conflicts_and_escaping_references() {
    for (source, code) in [
        ("fn f() -> string { let s = \"x\"; let t = s; s }", "E1012"),
        ("fn f(s: &string) -> string { *s }", "E1012"),
        ("fn f() -> string { let a = [\"a\", \"b\"]; a[0] }", "E1012"),
        (
            "record R { x: string } fn f() -> R { let r = R { x: \"a\" }; let _ = r.x; r }",
            "E1012",
        ),
        (
            "fn f(flag: bool) -> string { let s = \"x\"; let _ = if flag { s } else { \"y\" }; s }",
            "E1012",
        ),
        (
            "fn f(flag: bool) -> string { let s = \"x\"; let _ = flag && { let _ = s; true }; s }",
            "E1012",
        ),
        (
            "fn f() -> i64 { let s = \"x\"; let r = &s; let t = s; r.length }",
            "E1014",
        ),
        (
            "fn f() -> i64 { let mut x = 1; let a = &x; let b = &mut x; *a + *b }",
            "E1014",
        ),
        (
            "fn f() -> i64 { let mut x = 1; let a = &mut x; let b = &mut x; *a + *b }",
            "E1014",
        ),
        (
            "fn f() -> i64 { let mut x = 1; let a = &mut x; x + *a }",
            "E1014",
        ),
        ("fn f() -> i64 { let x = 1; let a = &mut x; *a }", "E1014"),
        ("fn f(x: &i64) -> unit { *x = 2; }", "E1014"),
        (
            "fn f() -> i64 { let mut x = 1; let a = &mut x; let b = &mut *a; *a = 2; *b }",
            "E1014",
        ),
        (
            "record R { x: i64 } fn f() -> unit { let mut r = R { x: 1 }; r.x = 2; }",
            "E1012",
        ),
        ("fn f() -> &string { let s = \"x\"; &s }", "E1013"),
        ("fn f(s: &string) -> &string { let t = \"x\"; &t }", "E1013"),
        ("fn f(s: &string, t: &string) -> &string { s }", "E1013"),
        (
            "fn f() -> i64 { let r = { let s = \"x\"; &s }; r.length }",
            "E1013",
        ),
        (
            "fn f() -> i64 { let s = \"outer\"; let mut r = &s; { let t = \"inner\"; r = &t; }; r.length }",
            "E1013",
        ),
        (
            "fn f() -> i64 { let r = &\"temporary\"; r.length }",
            "E1013",
        ),
        ("record R { s: &string }", "E1013"),
        ("fn f() -> string { \"\\u{d800}\" }", "E0001"),
        ("fn f() -> string { \"\\q\" }", "E0001"),
        ("export fn f(s: string) -> i64 { s.length }", "E1008"),
        ("export fn f(x: f128) -> f128 { x }", "E1008"),
    ] {
        rejects(source, code);
    }
}

#[test]
fn guards_reads_while_other_operands_are_evaluated() {
    rejects(
        "record R { s: string } fn f() -> string { let a = [R { s: \"x\" }]; a[0].s }",
        "E1012",
    );
    accepts("fn f(value: &[[i128]]) -> unit {}");
    rejects(
        "fn replace(s: &mut string) -> unit { *s = \"changed\"; } fn f() -> i64 { let mut s = \"original\"; let r = &mut s; let shared = &*r; replace(r); shared.length }",
        "E1014",
    );
    rejects(
        "fn replace(s: [&mut string]) -> unit { *s[0] = \"changed\"; } fn f() -> i64 { let mut s = \"original\"; let refs = [&mut s]; let shared = &*refs[0]; replace(refs); shared.length }",
        "E1005",
    );
    rejects(
        "fn f() -> bool { let s = \"x\"; s == { let t = s; t } }",
        "E1014",
    );
    rejects(
        "fn f() -> ubyte { let s = \"x\"; s[{ let t = s; 0 }] }",
        "E1014",
    );
    rejects(
        "fn use(a: &string, b: string) -> unit {} fn f() -> unit { let s = \"x\"; use(&s, s) }",
        "E1014",
    );
}
