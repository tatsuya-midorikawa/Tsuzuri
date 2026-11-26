use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::diagnostic::DiagnosticData;
use tsuzuri::syntax::Edition;
use tsuzuri::{analyze, analyze_modules, analyze_with_edition, llvm};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/operator-extension")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn check_err(source: &str, code: &str, kind: &str) -> DiagnosticData {
    let err = analyze_with_edition(source, Edition::E2028)
        .expect_err(source)
        .into_data();
    assert_eq!(err.code, code, "{source}\n{err:?}");
    assert_eq!(err.kind, Some(kind), "{source}\n{err:?}");
    err
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze_with_edition(source, Edition::E2028)
        .unwrap_or_else(|error| panic!("{source}\n{:?}", error.into_data()));
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR must be deterministic"
        );
        assert!(
            llvm::duplicate_declarations(&ir).is_empty(),
            "{source}: duplicate declarations"
        );
        ir
    })
}

#[test]
fn declaration_headers_head_rule_coverage_rule_and_overlaps() {
    // 3-arg Add..Pow and 2-arg Neg accepted
    emits(
        "record Money { cents: i64 }\n\
         record Debt { cents: i64 }\n\
         instance Add<Money, i64, Money> { fn add m d = Money { cents: m.cents + deref d } }\n\
         instance Sub<Money, i64, Money> { fn sub m d = Money { cents: m.cents - deref d } }\n\
         instance Mul<Money, i64, Money> { fn mul m k = Money { cents: m.cents * deref k } }\n\
         instance Div<Money, i64, Money> { fn div m k = Money { cents: m.cents / deref k } }\n\
         instance Rem<Money, i64, Money> { fn rem m k = Money { cents: m.cents % deref k } }\n\
         instance Pow<Money, i64, Money> { fn pow m k = Money { cents: m.cents ** deref k } }\n\
         instance Neg<Money, Debt> { fn neg m = Debt { cents: -m.cents } }\n\
         let p = Money { cents: 100 }\n\
         let q = ((p + 20 - 5) * 2 / 3 % 50) ** 2\n\
         let d = -q\n\
         d.cents\n",
    );

    // Wrong arities and non-arithmetic multi-arg headers -> E0002
    for bad in [
        "record M { v: i64 }\ninstance Add<M, i64> { fn add l _ = deref l }\n0\n",
        "record M { v: i64 }\ninstance Add<M, i64, M, i64> { fn add l _ = deref l }\n0\n",
        "record M { v: i64 }\ninstance Neg<M, M, M> { fn neg v = deref v }\n0\n",
        "record M { v: i64 }\ninstance Eq<M, M, bool> { fn eq _ _ = true\n fn ne _ _ = false }\n0\n",
    ] {
        check_err(bad, "E0002", "expected_single_type_argument");
    }

    // Both built-in sides & L == R == O -> E1016 operator_instance_form (before orphan_instance!)
    let d = check_err(
        "instance Mul<i64, f64, f64> { fn mul l r = (deref l as f64) * deref r }\n0\n",
        "E1016",
        "operator_instance_form",
    );
    assert!(d.message.contains("record, union or extern type"), "{d:?}");

    let d = check_err(
        "record Money { cents: i64 }\n\
         instance Add<Money, Money, Money> { fn add l r = Money { cents: l.cents + r.cents } }\n0\n",
        "E1016",
        "operator_instance_form",
    );
    assert!(
        d.message
            .contains("is a same-type instance; write instance Add<Main.Money>"),
        "{d:?}"
    );

    // Coverage rule: uncovered bare type variable on right or left -> E1016 operator_instance_form
    let d_right = check_err(
        "record Money { cents: i64 }\n\
         instance Mul<Money, 'b, Money> { fn mul l _ = deref l }\n0\n",
        "E1016",
        "operator_instance_form",
    );
    assert!(
        d_right
            .message
            .contains("the type variable 'b on the right must also appear in the left type"),
        "{d_right:?}"
    );
    let d_left = check_err(
        "record Money { cents: i64 }\n\
         instance Mul<'a, Money, Money> { fn mul _ r = deref r }\n0\n",
        "E1016",
        "operator_instance_form",
    );
    assert!(
        d_left
            .message
            .contains("the type variable 'a on the left must also appear in the right type"),
        "{d_left:?}"
    );

    // Covered generic `Vec3<'a>` and `extern type copy Buf` -> accepted!
    emits(
        "record Vec3<'a> { x: 'a, y: 'a, z: 'a }\n\
         instance (Mul<'a>, Copy<'a>) => Mul<Vec3<'a>, 'a, Vec3<'a>> {\n\
             fn mul v s = Vec3 { x: v.x * deref s, y: v.y * deref s, z: v.z * deref s }\n\
         }\n\
         extern type copy Buf\n\
         extern def scale_buf :: Buf -> f64 -> Buf\n\
         instance Mul<Buf, f64, Buf> {\n\
             fn mul b k = scale_buf (deref b) (deref k)\n\
         }\n\
         let v = Vec3 { x: 2.0, y: 3.0, z: 4.0 }\n\
         (v * 2.0).z\n",
    );

    // Separate user modules with `Mul<'a, A<'a>, A<'a>>` and `Mul<B<'b>, 'b, B<'b>>` do NOT overlap
    analyze_modules(&[
        (
            "ModA",
            "record A<'a> { x: 'a }\n\
             instance (Mul<'a>, Copy<'a>) => Mul<'a, A<'a>, A<'a>> {\n\
                 fn mul s a = A { x: deref s * a.x }\n\
             }\n",
        ),
        (
            "ModB",
            "record B<'b> { y: 'b }\n\
             instance (Mul<'b>, Copy<'b>) => Mul<B<'b>, 'b, B<'b>> {\n\
                 fn mul b s = B { y: b.y * deref s }\n\
             }\n",
        ),
        (
            "Main",
            "let a = ModA.A { x: 3 }\n\
             let b = ModB.B { y: 5 }\n\
             (2 * a).x + (b * 4).y\n",
        ),
    ])
    .unwrap();

    // Unbound type variable in Output or context -> E1027 instance_context_variable
    check_err(
        "record Money { cents: i64 }\n\
         instance Sub<Money, i64, 'out> { fn sub _ _ = loop () }\n\
         def rec loop :: unit -> 'a\nfn rec loop _ = loop ()\n0\n",
        "E1027",
        "instance_context_variable",
    );
    check_err(
        "record Money { cents: i64 }\n\
         instance Copy<'extra> => Sub<Money, i64, Money> { fn sub l _ = deref l }\n0\n",
        "E1027",
        "instance_context_variable",
    );

    // OEI vs OEI and OEI vs SameType overlap in BOTH declaration orders -> E1016 overlapping_instance
    let same = "instance Add<Boxed<'a>> { fn add l _ = Boxed { v: l.v } }";
    let oei = "instance Add<Boxed<i64>, Boxed<i64>, i64> { fn add l r = l.v + r.v }";
    for (first, second) in [(same, oei), (oei, same)] {
        let src = format!("record Boxed<'a> {{ v: 'a }}\n{first}\n{second}\n0\n");
        check_err(&src, "E1016", "overlapping_instance");
    }
}

#[test]
fn orphan_rules_and_std_overlap_locations() {
    for src in [
        "instance Mul<Tensor<f64>, f64, Tensor<f64>> { fn mul l _ = Tensor.clone l }\n0\n",
        "instance Neg<Tensor.View<f64>, Tensor<f64>> { fn neg v = Tensor.to_tensor v }\n0\n",
        "instance Mul<i64, Tensor<f64>, Tensor<f64>> { fn mul _ r = Tensor.clone r }\n0\n",
    ] {
        check_err(src, "E1016", "orphan_instance");
    }

    // Program-wide orphan rule: Money in same module OR another user module of the program is accepted!
    emits(
        "record Money { cents: f64 }\n\
         instance Mul<f64, Money, Money> { fn mul k m = Money { cents: deref k * m.cents } }\n\
         instance Mul<Money, Tensor<f64>, Tensor<f64>> { fn mul m t = m.cents * deref t }\n\
         let m = Money { cents: 2.0 }\n\
         let t = Tensor.of_array [2] [3.0, 4.0]\n\
         let r = (3.0 * m) * t\n\
         r[0]\n",
    );
    analyze_modules(&[
        ("Types", "record Money { cents: f64 }\n"),
        (
            "Ops",
            "instance Mul<Types.Money, Tensor<f64>, Tensor<f64>> {\n\
                 fn mul m t = m.cents * deref t\n\
             }\n",
        ),
        (
            "Main",
            "let m = Types.Money { cents: 2.0 }\n\
             let t = Tensor.of_array [2] [3.0, 4.0]\n\
             (m * t)[0]\n",
        ),
    ])
    .unwrap();

    // P18: user `Eq<Maybe<'a>>` style non-orphan-checked class remains accepted
    analyze("record Tag { x: i64 }\ninstance Eq<Maybe<Tag>> { fn eq _ _ = true\n fn ne _ _ = false }\n0\n")
        .unwrap();

    // Overlap with std's open scalar OEI (`Mul<'a, Tensor<'a>, Tensor<'a>>` and `Mul<Tensor<'a>, 'a, Tensor<'a>>`)
    // reports E1016 `overlapping_instance` at the user's class name with std in `related`!
    for head in [
        "Mul<Money, Tensor<Money>, Tensor<Money>>",
        "Mul<Tensor<Money>, Money, Tensor<Money>>",
    ] {
        let src = format!(
            "record Money {{ cents: i64 }}\n\
             instance {head} {{ fn mul _ _ = Tensor.of_array [1] [Money {{ cents: 0 }}] }}\n\
             let _load_tensor = Tensor.of_array [1] [0]\n0\n"
        );
        let d = check_err(&src, "E1016", "overlapping_instance");
        assert!(
            src[d.span.start..].starts_with("Mul"),
            "primary span must be user class name, got {:?}",
            &src[d.span.start..]
        );
        assert!(!d.related.is_empty(), "must reference std instance: {d:?}");
    }
}

#[test]
fn resolution_literal_hints_soft_hints_and_ambiguity() {
    // (a) Ambiguous literal candidate on Money * 3 -> E1043 independent of declaration order
    let i32_inst = "instance Mul<Money, i32, Money> { fn mul m k = Money { cents: m.cents * (deref k as i64) } }";
    let i64_inst =
        "instance Mul<Money, i64, Money> { fn mul m k = Money { cents: m.cents * deref k } }";
    for (a, b) in [(i32_inst, i64_inst), (i64_inst, i32_inst)] {
        let src = format!(
            "record Money {{ cents: i64 }}\n{a}\n{b}\nlet price = Money {{ cents: 250 }}\n(price * 3).cents\n"
        );
        let d = check_err(&src, "E1043", "ambiguous_instance_candidates");
        assert!(d.message.contains("give the literal a suffix"), "{d:?}");
    }

    // (a) `2.0 * x` where `x: Tensor<i32>` -> E1003 (no implicit element widening)
    let d = check_err(
        "let x = Tensor.of_array [2] [1i32, 2i32]\n2.0 * x\n",
        "E1003",
        "type_mismatch",
    );
    assert!(
        d.help
            .as_deref()
            .unwrap_or("")
            .contains("do not implicitly convert element types"),
        "{d:?}"
    );

    // (c) `Tensor<f32> + Tensor<f64>` -> E1003; `let k = 2.0; x * k` (`x: Tensor<i32>`) -> E1003 with help
    check_err(
        "let a = Tensor.of_array [2] [1.0f32, 2.0f32]\n\
         let b = Tensor.of_array [2] [1.0f64, 2.0f64]\n\
         a + b\n",
        "E1003",
        "type_mismatch",
    );
    let d = check_err(
        "let x = Tensor.of_array [2] [1i32, 2i32]\n\
         let k = 2.0\n\
         x * k\n",
        "E1003",
        "type_mismatch",
    );
    assert!(
        d.help
            .as_deref()
            .unwrap_or("")
            .contains("available Mul operator instances"),
        "{d:?}"
    );

    // (d) `Sub.sub v w` and `Sub<'a> =>` with `View` remain E1005; `\u -> 2.0 * u` infers `f64 -> f64`
    check_err(
        "let t = Tensor.of_array [2] [1.0, 2.0]\n\
         let v = Tensor.view (ref t)\n\
         Sub.sub v v\n",
        "E1005",
        "no_instance",
    );
    emits("let f = \\u -> 2.0 * u\nf 3.5\n");

    // Soft hints (P6, P13, P22, P23, P24, P25, and nested call `t + f (g 2)` non-leakage!)
    emits(
        "record V<'a> { x: 'a }\n\
         instance Mul<V<f64>> { fn mul l r = V { x: l.x * r.x } }\n\
         instance Mul<f64, V<f64>, V<f64>> { fn mul k v = V { x: deref k * v.x } }\n\
         def ident :: own 'a -> 'a = \\z -> z\n\
         def g_int :: i32 -> i64 = \\n -> (n as i64) + 1\n\
         def mk_t :: i64 -> Tensor<f64> = \\n -> Tensor.full 1.0 [n]\n\
         let b = true\n\
         let t: Tensor<f64> = Tensor.of_array [2] [1.0, 2.0]\n\
         let u: Tensor<f64> = Tensor.of_array [2] [3.0, 4.0]\n\
         let wv = Tensor.view (ref u)\n\
         let s1 = t + Tensor.view (ref t)\n\
         let s2: Tensor<f64> = Tensor.view (ref t) - wv\n\
         let s3 = t + Tensor.full 2 [2]\n\
         let k: f64 = 2.0\n\
         let s4: Tensor<f64> = k * t\n\
         let ki: i64 = 2\n\
         let ti = Tensor.of_array [2] [10i64, 20i64]\n\
         let s5 = ki * Tensor.view (ref ti)\n\
         let s6 = t + (if b then Tensor.full 2 [2] else Tensor.full 3 [2])\n\
         let s7 = t + { Tensor.of_array [2] [1.0, 2.0] }\n\
         let s8 = t * (match 3 with | 3 -> Tensor.full 2 [2] | _ -> Tensor.full 3 [2])\n\
         let s9 = t + (if b then Tensor.view (ref t) else Tensor.view (ref u))\n\
         let nested = t + mk_t (g_int 2)\n\
         let vv = V { x: 1.5 }\n\
         let v1 = vv * (if b then V { x: 2 } else V { x: 3 })\n\
         let v2 = vv * V { x: 2 }\n\
         let v3 = k * vv\n\
         let p13 = k * ident 2\n\
         let p25 = k * (if b then 1 else 2)\n\
         let big = 10I\n\
         let p22 = (big * 2) + (3 * big)\n\
         s1[0] + s2[0] + s3[0] + s4[0] + (s5[0] as f64) + s6[0] + s7[0] + s8[0] + s9[0] + nested[0] + v1.x + v2.x + v3.x + p13 + p25 + (if p22 == 50I then 1.0 else 0.0)\n",
    );
}

#[test]
fn generic_functions_borrows_drop_order_and_checked_semantics() {
    // Generic `scale_all` works for f64 and BigInt (both Copy + Mul), and rejects non-Copy or non-Mul types at call site (`E1005`)
    let scale_header = "\
def scale_all :: own 'a -> own Tensor<'a> -> Tensor<'a>
fn scale_all s t = s * t
def mul_two :: (Mul<'a>, Copy<'a>, Integer<'a>) => own Tensor<'a> -> Tensor<'a>
fn mul_two t = t * 2
";
    emits(&format!(
        "{scale_header}let x = Tensor.of_array [2] [1.5, 2.5]\nlet y = scale_all 2.0 x\nlet z = mul_two (Tensor.of_array [2] [3i64, 4i64])\nlet b = Tensor.of_array [1] [5I]\nlet big_scaled = scale_all 2I b\ny[0] + (z[0] as f64) + (if big_scaled[0] == 10I then 1.0 else 0.0)\n"
    ));
    let d_nc = check_err(
        &format!(
            "record NonCopy {{ tag: string, n: i64 }}\n\
             instance Mul<NonCopy> {{ fn mul l r = NonCopy {{ tag: \"prod\", n: l.n * r.n }} }}\n\
             {scale_header}\
             let nc = NonCopy {{ tag: \"x\", n: 3 }}\n\
             let t = Tensor.of_array [1] [NonCopy {{ tag: \"y\", n: 4 }}]\n\
             scale_all nc t\n"
        ),
        "E1005",
        "no_instance",
    );
    assert!(d_nc.message.contains("Copy<Main.NonCopy>"), "{d_nc:?}");
    let d_nm = check_err(
        &format!("{scale_header}let b = Tensor.of_array [1] [true]\nscale_all true b\n"),
        "E1005",
        "no_instance",
    );
    assert!(d_nm.message.contains("Mul<bool>"), "{d_nm:?}");

    // Instance method using an OEI without declaring required context -> E1027 undeclared_instance_constraint
    check_err(
        "record Wrap<'a> { t: Tensor<'a> }\n\
         class Scale2<'a> { def dbl :: 'a -> 'a }\n\
         instance Scale2<Wrap<'a>> {\n\
             fn dbl w = Wrap { t: Tensor.clone (ref w.t) + Tensor.clone (ref w.t) }\n\
         }\n0\n",
        "E1027",
        "undeclared_instance_constraint",
    );

    // P11: loan-carrying element type through scalar * Tensor keeps loan of non-Copy owner (`E1014` on move)
    let err = analyze_with_edition(
        "record Borrowed { r } { data: ref { r } Vec<i64> }\n\
         instance Mul<Borrowed> {\n\
             fn mul l _ = Borrowed { data: l.data }\n\
         }\n\
         let base = Vec.of_array [10, 20]\n\
         let item = Borrowed { data: ref base }\n\
         let t = Tensor.of_array [1] [item]\n\
         let scaled = item * t\n\
         let moved = base\n\
         Vec.length scaled[0].data + Vec.length (ref moved)\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err.code, "E1014", "{err:?}");

    // P12: OEI body returning a borrow of its parameter place -> E1013 on temporary operand, and E1014 when mutating a local operand while live
    let err_tmp = analyze_with_edition(
        "record RefOut { r } { p: ref { r } i64 }\n\
         record BoxI { v: i64 }\n\
         instance Mul<BoxI, i64, RefOut> {\n\
             fn mul b _ = RefOut { p: ref b.v }\n\
         }\n\
         def run :: own i64 -> i64 = \\n ->\n\
             let z = (BoxI { v: n }) * 2\n\
             deref z.p\n\
         run 7\n"
            .replace(
                "def run :: own i64 -> i64 = \\n ->\nlet",
                "def run :: own i64 -> i64 = \\n ->\n    let",
            )
            .replace("\nderef z.p", "\n    deref z.p")
            .as_str(),
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err_tmp.code, "E1013", "{err_tmp:?}");

    let err_mut = analyze_with_edition(
        "record RefOut { r } { p: ref { r } i64 }\n\
         record BoxI { v: i64 }\n\
         instance Mul<BoxI, i64, RefOut> {\n\
             fn mul b _ = RefOut { p: ref b.v }\n\
         }\n\
         let mut b = BoxI { v: 7 }\n\
         let k: i64 = 2\n\
         let z = b * k\n\
         b = BoxI { v: 99 }\n\
         deref z.p\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err_mut.code, "E1014", "{err_mut:?}");

    // Drop order (left temporary, then right temporary) and `@checked (t * 1000)` elementwise wrapping
    let root = scratch_dir("drop-and-checked");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"drop-and-checked\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "\
record LTag { id: i64, log: Arc<Mutex<i64>> }
instance Drop<LTag> {
    fn drop self =
        let id = self.id
        Mutex.with_lock (Arc.get (ref self.log)) (\\acc -> deref acc = (deref acc) * 10 + id)
}
record RTag { id: i64, log: Arc<Mutex<i64>> }
instance Drop<RTag> {
    fn drop self =
        let id = self.id
        Mutex.with_lock (Arc.get (ref self.log)) (\\acc -> deref acc = (deref acc) * 10 + id)
}
instance Mul<LTag, RTag, i64> {
    fn mul l r =
        Mutex.with_lock (Arc.get (ref l.log)) (\\acc -> deref acc = (deref acc) * 10 + 9)
        l.id + r.id
}

test \"temporaries_drop_left_then_right_after_call\" =
    let log = Arc.new (Mutex.create 0i64)
    let v = (LTag { id: 1, log: Arc.share (ref log) }) * (RTag { id: 2, log: Arc.share (ref log) })
    let recorded = Mutex.with_lock (Arc.get (ref log)) (\\acc -> deref acc)
    Test.equal 3 v
    Test.equal 912 recorded

test \"checked_does_not_wrap_elementwise_oei_call\" =
    let t = Tensor.of_array [2] [2000000000i32, -2000000000i32]
    let prod = @checked (t * 2)
    Test.equal (-294967296i32) prod[0]
",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn std_tensor_has_43_generated_oei_instances_matching_spec_table() {
    let (_, tensor_src) = tsuzuri::stdlib::SOURCES
        .iter()
        .find(|(path, _)| *path == "std/Tensor.tz")
        .unwrap();
    let program = tsuzuri::parser::parse(tensor_src).unwrap();
    let oei: Vec<_> = program
        .instances
        .iter()
        .filter(|inst| {
            matches!(
                inst.class.text.as_str(),
                "Add" | "Sub" | "Mul" | "Div" | "Rem" | "Pow" | "Neg"
            ) && !inst.extra_args.is_empty()
        })
        .collect();
    assert_eq!(oei.len(), 43, "std/Tensor.tz must define exactly 43 OEIs");
    for inst in &oei {
        assert_eq!(inst.methods.len(), 1);
        assert!(
            matches!(
                inst.methods[0].body.kind,
                tsuzuri::syntax::ExprKind::Call(..)
            ),
            "each OEI body must be a single kernel call: {:?}",
            inst.methods[0].name.text
        );
    }

    let check_gen = Command::new("node")
        .args(["scripts/gen-tensor-operators.mjs", "--check"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        check_gen.status.success(),
        "{}",
        String::from_utf8_lossy(&check_gen.stderr)
    );

    // Allocation parity (`v - w` == `Tensor.sub (ref v) (ref w)`, and `t + v` == `v + w` + 2 for `view`)
    let root = scratch_dir("alloc-parity");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"alloc-parity\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "\
test \"view_sub_allocates_same_as_tensor_sub_and_tensor_plus_view_adds_two\" =
    let a = Tensor.of_array [4] [10.0, 20.0, 30.0, 40.0]
    let b = Tensor.of_array [4] [1.0, 2.0, 3.0, 4.0]
    let va = Tensor.view (ref a)
    let vb = Tensor.view (ref b)

    let m1 = Alloc.snapshot ()
    let r_op = va - vb
    let d_op = Alloc.since m1

    let m2 = Alloc.snapshot ()
    let r_fn = match Tensor.sub (ref va) (ref vb) with
        | Result.Ok t -> t
        | Result.Error _ -> Tensor.zeros [0]
    let d_fn = Alloc.since m2

    let m3 = Alloc.snapshot ()
    let r_tv = a + vb
    let d_tv = Alloc.since m3

    Test.is_true (r_op == r_fn)
    Test.equal d_fn.allocs d_op.allocs
    Test.equal (d_op.allocs + 2) d_tv.allocs
    Test.equal 11.0 r_tv[0]
",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}
