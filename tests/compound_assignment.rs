use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::syntax::Edition;
use tsuzuri::{analyze_modules_with_edition, analyze_with_edition, llvm};

fn rejects_in(source: &str, edition: Edition, code: &str) -> Diagnostic {
    let error = analyze_with_edition(source, edition).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{error:?}");
    error
}

fn emits_in(source: &str, edition: Edition) -> [String; 2] {
    let module = analyze_with_edition(source, edition).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
        assert!(
            llvm::duplicate_declarations(&ir).is_empty(),
            "{source}: declarations"
        );
        ir
    })
}

fn strip_debug_and_trap_spans(ir: &str) -> String {
    ir.lines()
        .filter(|line| !line.starts_with('!') && !line.contains("@tz.str."))
        .map(|line| match line.find(", !dbg ") {
            Some(pos) => &line[..pos],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn builtin_types_accept_all_operators_and_editions() {
    for edition in [Edition::E2026, Edition::E2027, Edition::E2028] {
        emits_in(
            "\
const def c :: unit -> i64 = \\() -> { let mut total = 1; total += 4; total *= 3; total }
let mut a8: i8 = 10i8
a8 += 3i8
a8 -= 2i8
a8 *= 2i8
a8 /= 3i8
a8 %= 5i8
let mut a16: i16 = 100i16
a16 += 5i16
let mut a32: i32 = 50i32
a32 -= 7i32
let mut a64: i64 = 1
a64 += 2
a64 *= 5
let mut a128: i128 = 40i128
a128 /= 4i128
let mut u8v: i8u = 10i8u
u8v += 2i8u
let mut u16v: i16u = 20i16u
u16v *= 3i16u
let mut u32v: i32u = 30i32u
u32v -= 5i32u
let mut u64v: i64u = 99i64u
u64v %= 10i64u
let mut u128v: i128u = 50i128u
u128v /= 5i128u
let mut f32v: f32 = 1.5f32
f32v += 0.5f32
f32v -= 0.25f32
f32v *= 2.0f32
f32v /= 0.5f32
let mut f64v: f64 = 2.0
f64v += 3.0
f64v -= 1.0
f64v *= 1.5
f64v /= 2.0
let mut vi: i32x4 = Simd.splat 2i32
vi += Simd.splat 3i32
vi -= Simd.splat 1i32
vi *= Simd.splat 4i32
let mut vf: f64x2 = Simd.splat 6.0
vf += Simd.splat 2.0
vf -= Simd.splat 1.0
vf *= Simd.splat 3.0
vf /= Simd.splat 2.0
let mut s = \"ab\"
s += \"c\"
let mut u = u8\"xy\"
u += u8\"z\"
let mut r_target = 10
let r = ref mut r_target
deref r += 5
if true then a64 += 1 else ()
a64 + r_target + c () + s.length + u.length
",
            edition,
        );
    }

    // Edition 2027+ allows record fields and array/Vec/FixedArray elements.
    emits_in(
        "\
record Pair { x: i64, y: f64 }
def main :: unit -> i32 = \\() ->
    let mut p = Pair { x: 1, y: 1.5 }
    p.x += 4
    p.y += 2.5
    let mut xs = [10, 20, 30]
    xs[1] += 5
    if p.x == 5 && p.y == 4.0 && xs[1] == 25 then 0i32 else 1i32
",
        Edition::E2027,
    );

    // Edition 2026 rejects field compound assignment with E1012 + edition 2027 help,
    // while leaving `=` diagnostic unchanged.
    let err_compound = rejects_in(
        "record B { n: i64 }\ndef main :: unit -> i32 = \\() ->\n    let mut b = B { n: 1 }\n    b.n += 1\n    0i32\n",
        Edition::E2026,
        "E1012",
    )
    .into_data();
    assert_eq!(err_compound.kind, Some("assign_to_immutable_place"));
    assert_eq!(
        err_compound.help.as_deref(),
        Some("record fields can be updated with '+=' from edition 2027")
    );

    let err_plain = rejects_in(
        "record B { n: i64 }\ndef main :: unit -> i32 = \\() ->\n    let mut b = B { n: 1 }\n    b.n = b.n + 1\n    0i32\n",
        Edition::E2026,
        "E1012",
    )
    .into_data();
    assert_eq!(err_plain.help, None);
}

#[test]
fn simple_builtin_places_emit_identical_ir_to_expanded_assignment() {
    let pairs = [
        (
            "export def f :: own i64 -> i64 = \\n -> { let mut x = n; x += 7; x *= 3; x }\n",
            "export def f :: own i64 -> i64 = \\n -> { let mut x = n; x = x + 7; x = x * 3; x }\n",
        ),
        (
            "def bump :: ref mut i64 -> unit = \\r -> deref r += 9\nexport def f :: own i64 -> i64 = \\n -> { let mut x = n; bump (ref mut x); x }\n",
            "def bump :: ref mut i64 -> unit = \\r -> deref r = deref r + 9\nexport def f :: own i64 -> i64 = \\n -> { let mut x = n; bump (ref mut x); x }\n",
        ),
        (
            "export def f :: own i64 -> i64 = \\i -> { let mut xs = [1, 2, 3]; xs[i] += 5; xs[i] }\n",
            "export def f :: own i64 -> i64 = \\i -> { let mut xs = [1, 2, 3]; xs[i] = xs[i] + 5; xs[i] }\n",
        ),
        (
            "record Boxed { v: i64 }\nexport def f :: own i64 -> i64 = \\n -> { let mut b = Boxed { v: n }; b.v -= 2; b.v }\n",
            "record Boxed { v: i64 }\nexport def f :: own i64 -> i64 = \\n -> { let mut b = Boxed { v: n }; b.v = b.v - 2; b.v }\n",
        ),
    ];
    for (compound, expanded) in pairs {
        let ir_c = emits_in(compound, Edition::E2027);
        let ir_e = emits_in(expanded, Edition::E2027);
        for side in [0, 1] {
            assert_eq!(
                strip_debug_and_trap_spans(&ir_c[side]),
                strip_debug_and_trap_spans(&ir_e[side]),
                "IR mismatch (side={side}):\n{compound}"
            );
        }
    }
}

#[test]
fn evaluation_order_checked_overflow_and_rhs_guards_hold() {
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc20-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(
        &main_tz,
        r#"def say :: i64 -> i64 = \n ->
    do! IO.writeln (to_string n)
    n

def pick { r } :: ref mut { r } i64 -> i64 -> ref mut { r } i64 = \r n ->
    do! IO.writeln ("place " + to_string n)
    r

def next :: ref mut i64 -> i64 = \k ->
    let cur = deref k
    deref k += 1
    cur

def checked_bump :: ref mut i8 -> Result<unit, Exception> = \r ->
    try
        @checked (deref r += 10i8)
    with
    | e is OverflowException -> e

def main :: unit -> i32 = \() ->
    let mut x = 1
    deref (pick (ref mut x) 1) += say 2
    do! IO.writeln (to_string x)
    let mut k = 1
    let mut xs = [10, 20, 30]
    xs[next (ref mut k)] += 5
    let mut self_dbl = 7
    self_dbl += self_dbl
    let mut s = "hi"
    s += $"({s})"
    let mut ov: i8 = 125i8
    let caught = match checked_bump (ref mut ov) with
        | Result.Error _ -> ov == 125i8
        | Result.Ok () -> false
    do! IO.writeln ($"{k}:{xs[1]}:{self_dbl}:{s}:{caught}")
    0i32
"#,
    )
    .unwrap();
    for opt in ["-O0", "-O3"] {
        let exe = dir.join(if cfg!(windows) {
            format!("out{opt}.exe")
        } else {
            format!("out{opt}")
        });
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["build", opt, "-o"])
            .arg(&exe)
            .arg(&main_tz)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{opt} build stderr:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let out = std::process::Command::new(&exe).output().unwrap();
        assert!(
            out.status.success(),
            "{opt} stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(stdout, "place 1\n2\n3\n2:25:14:hi(hi):true\n", "{opt}");
    }
    let _ = std::fs::remove_dir_all(&dir);

    // RHS guard rejections (`E1014` `borrow_conflict` + custom help + related span).
    for (src, expected_help) in [
        (
            "def bump :: ref mut i64 -> i64 = \\r -> { deref r = 10; 0 }\nlet mut x = 1\nx += bump (ref mut x)\nx",
            "the right side of '+=' cannot change 'x'; compute it into a 'let' first",
        ),
        (
            "let mut x = 1\nx += { x = 5; 1 }\nx",
            "the right side of '+=' cannot change 'x'; compute it into a 'let' first",
        ),
        (
            "let mut s = \"ab\"\ns += s\ns.length",
            "the right side of '+=' cannot change 's'; compute it into a 'let' first",
        ),
    ] {
        let diag = rejects_in(src, Edition::E2027, "E1014").into_data();
        assert_eq!(diag.kind, Some("borrow_conflict"), "{src}\n{diag:?}");
        assert_eq!(diag.help.as_deref(), Some(expected_help), "{src}");
        assert!(
            !diag.related.is_empty(),
            "{src}: expected BorrowedHere span"
        );
    }
    let arr_guard = rejects_in(
        "def main :: unit -> i32 = \\() ->\n    let mut xs = [1, 2, 3]\n    xs[2] += { xs = [9]; 5 }\n    0i32\n",
        Edition::E2027,
        "E1014",
    )
    .into_data();
    assert_eq!(arr_guard.kind, Some("borrow_conflict"));
    assert_eq!(
        arr_guard.help.as_deref(),
        Some("the right side of '+=' cannot change 'xs'; compute it into a 'let' first")
    );

    // Closures and computation continuations reject compound assignment to outer `let mut`.
    rejects_in(
        "let mut x = 0\nlet f = \\() -> x += 1\nf ()\nx",
        Edition::E2027,
        "E1014",
    );
    rejects_in(
        "let mut x = 0\nlet m = Maybe { let! a = Maybe.Some 1; x += a; return a }\n0",
        Edition::E2027,
        "E1014",
    );
}

#[test]
fn builtin_type_errors_and_target_restrictions_match_spec() {
    // Missing builtin operator matches expanded `p = p op e`.
    let rem_f64 = rejects_in("let mut x = 1.5\nx %= 0.5\n0", Edition::E2027, "E1005").into_data();
    assert_eq!(rem_f64.kind, Some("no_instance"));
    assert!(rem_f64.message.contains("Rem<f64>"), "{}", rem_f64.message);

    let sub_str =
        rejects_in("let mut s = \"ab\"\ns -= \"b\"\n0", Edition::E2027, "E1005").into_data();
    assert_eq!(sub_str.kind, Some("no_instance"));

    let bool_add =
        rejects_in("let mut b = false\nb += true\n0", Edition::E2027, "E1005").into_data();
    assert_eq!(bool_add.kind, Some("no_instance"));
    assert!(
        bool_add.message.contains("Add<bool>"),
        "{}",
        bool_add.message
    );

    let late_str = rejects_in(
        "let mut s = Default.default ()\ns -= \"x\"\n0",
        Edition::E2027,
        "E1005",
    )
    .into_data();
    assert_eq!(late_str.kind, Some("no_instance"));
    assert!(
        late_str.message.contains("Sub<string>"),
        "{}",
        late_str.message
    );

    // Reference target -> E1005 compound_assign_unsupported_target + deref help.
    let ref_target = rejects_in(
        "def f :: ref mut i64 -> unit = \\m -> m += 1\n0",
        Edition::E2027,
        "E1005",
    )
    .into_data();
    assert_eq!(ref_target.kind, Some("compound_assign_unsupported_target"));
    assert_eq!(
        ref_target.help.as_deref(),
        Some("write 'deref m += ...' to update what 'm' refers to")
    );

    // Generic type variable target -> E1005 compound_assign_unsupported_target.
    let gen_target = rejects_in(
        "def f :: Add<'a> => 'a -> 'a -> 'a = \\x y -> { let mut acc = x; acc += y; acc }\n0",
        Edition::E2026,
        "E1005",
    )
    .into_data();
    assert_eq!(gen_target.kind, Some("compound_assign_unsupported_target"));
    assert_eq!(
        gen_target.message,
        "'+=' needs a concrete target type; write 'x = x + y' for a generic 'a"
    );

    // Undecided target inferred later to non-builtin type -> E1005 compound_assign_unsupported_target.
    let late_non_builtin = rejects_in(
        "record Money { cents: i64 }\ninstance Default<Money> { fn default = Money { cents: 0 } }\ninstance AddAssign<Money, Money> { fn add_assign rhs target = target.cents += rhs.cents }\ndef main :: unit -> i32 = \\() ->\n    let mut x = Default.default ()\n    x += Money { cents: 5 }\n    0i32\n",
        Edition::E2027,
        "E1005",
    )
    .into_data();
    assert_eq!(
        late_non_builtin.kind,
        Some("compound_assign_unsupported_target")
    );
    assert_eq!(
        late_non_builtin.message,
        "'+=' must know the type of its target where it is checked; annotate 'x'"
    );

    // Negative literal index on built-in array emits W1014.
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc20-w1014-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(
        &main_tz,
        "def main :: unit -> i32 = \\() ->\n    let mut xs = [1, 2, 3]\n    xs[-1] += 1\n    0i32\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Tsuzuri.toml"),
        "[package]\nname = \"w\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("check")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("W1014"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_assign_user_instances_orphan_overlap_and_place_rules() {
    // User record `AddAssign` across modules, field targets in 2027, and A40 `x += ref y`.
    let module = analyze_modules_with_edition(
        &[
            (
                "Types",
                "record Money { cents: i64 }\nextern type Counter\n",
            ),
            (
                "Ops",
                "\
instance AddAssign<Types.Money, Types.Money> {
    fn add_assign rhs target = target.cents += rhs.cents
}
instance AddAssign<Types.Money, Tensor<i64>> {
    fn add_assign rhs target = target.cents += Tensor.sum (Tensor.view rhs)
}
instance AddAssign<Types.Counter, i64> {
    fn add_assign _rhs _target = ()
}
",
            ),
            (
                "Main",
                "\
record Account { balance: Types.Money }
def main :: unit -> i32 = \\() ->
    let mut acct = Account { balance: Types.Money { cents: 100 } }
    let d = Types.Money { cents: 25 }
    acct.balance += d
    acct.balance += ref d
    acct.balance += Tensor.of_array [2] [3, 7]
    if acct.balance.cents == 160 then 0i32 else 1i32
",
            ),
        ],
        Edition::E2027,
    )
    .expect("cross-module AddAssign and field target in 2027");
    let _ = llvm::emit_target(&module, llvm::Entry::Console, false).unwrap();

    // Same field target in edition 2026 -> E1012 with edition 2027 help.
    let e2026_field = analyze_modules_with_edition(
        &[(
            "Main",
            "\
record Money { cents: i64 }
record Account { balance: Money }
instance AddAssign<Money, Money> {
    fn add_assign rhs target = deref target = Money { cents: target.cents + rhs.cents }
}
def main :: unit -> i32 = \\() ->
    let mut acct = Account { balance: Money { cents: 10 } }
    acct.balance += Money { cents: 5 }
    0i32
",
        )],
        Edition::E2026,
    )
    .unwrap_err()
    .into_data();
    assert_eq!(e2026_field.code, "E1012");
    assert_eq!(e2026_field.kind, Some("assign_to_immutable_place"));
    assert_eq!(
        e2026_field.help.as_deref(),
        Some("record fields can be updated with '+=' from edition 2027")
    );

    // Registration rules (D4):
    // 1. builtin_instance_override
    for bad in [
        "instance AddAssign<i64, i64> { fn add_assign _r _t = () }\n0",
        "record Money { cents: i64 }\ninstance AddAssign<(i64 * i64), Money> { fn add_assign _r _t = () }\n0",
        "instance AddAssign<'a, 'a> { fn add_assign _r _t = () }\n0",
        "record Money { cents: i64 }\ninstance IndexBorrowMut<Money, i64, Money> { fn index_borrow_mut _k s = Money { cents: s.cents } }\n0",
    ] {
        let d = rejects_in(bad, Edition::E2027, "E1016").into_data();
        assert_eq!(d.kind, Some("builtin_instance_override"), "{bad}\n{d:?}");
    }

    // 2. orphan_instance
    for (bad, expected_msg) in [
        (
            "instance AddAssign<Tensor<f64>, f64> { fn add_assign _r _t = () }\n0",
            "AddAssign<Tensor<f64>, f64> needs a type declared in this program as its target or right-hand type",
        ),
        (
            "instance AddAssign<Tensor.Mut<f64>, Tensor.View<f64>> { fn add_assign _r _t = () }\n0",
            "AddAssign<Tensor.Mut<f64>, Tensor.View<f64>> needs a type declared in this program as its target or right-hand type",
        ),
    ] {
        let d = rejects_in(bad, Edition::E2027, "E1016").into_data();
        assert_eq!(d.kind, Some("orphan_instance"), "{bad}\n{d:?}");
        assert_eq!(d.message, expected_msg);
    }

    // 3. overlapping_instance against std's `AddAssign<Tensor<'a>, 'a>`, reported at the user instance!
    let overlap_src = "record Money { cents: i64 }\ninstance AddAssign<Tensor<Money>, Money> {\n    fn add_assign _r _t = ()\n}\n0";
    let ov = rejects_in(overlap_src, Edition::E2027, "E1016").into_data();
    assert_eq!(ov.kind, Some("overlapping_instance"));
    assert_eq!(ov.span.source, Some(0), "must point at user module: {ov:?}");

    // 4. Reserved class name collision, constraint rejection, and method value rejection.
    rejects_in("record AddAssign { x: i64 }\n0", Edition::E2027, "E1001");
    rejects_in(
        "record IndexBorrowMut { x: i64 }\n0",
        Edition::E2027,
        "E1001",
    );
    rejects_in(
        "def f :: AddAssign<'a, 'b> => 'a -> 'b -> unit = \\_ _ -> ()\n0",
        Edition::E2027,
        "E0002",
    );
    rejects_in(
        "def f :: AddAssign<'a> => 'a -> unit = \\_ -> ()\n0",
        Edition::E2027,
        "E0002",
    );
    rejects_in("let g = AddAssign.add_assign\n0", Edition::E2027, "E1002");
    rejects_in(
        "let g = IndexBorrowMut.index_borrow_mut\n0",
        Edition::E2027,
        "E1002",
    );

    // 5. Missing AddAssign instance (BigInt has Add -> suggests `write 'x = x + y'`; View has no AddAssign -> E1005).
    let no_bigint = rejects_in(
        "let mut b = BigInt.of_i64 10\nb += BigInt.of_i64 5\n0",
        Edition::E2027,
        "E1005",
    )
    .into_data();
    assert_eq!(no_bigint.kind, Some("no_instance"));
    assert_eq!(no_bigint.help.as_deref(), Some("write 'x = x + y'"));

    let no_view = rejects_in(
        "let t = Tensor.zeros [3]\nlet mut v = Tensor.view (ref t)\nv += 1.0\n0",
        Edition::E2027,
        "E1005",
    )
    .into_data();
    assert_eq!(no_view.kind, Some("no_instance"));

    // 6. Ambiguous literal candidates -> E1043.
    let ambig = rejects_in(
        "\
record Bag { v: i64 }
instance AddAssign<Bag, i64> { fn add_assign r t = t.v += deref r }
instance AddAssign<Bag, i32> { fn add_assign r t = t.v += (deref r as i64) }
def main :: unit -> i32 = \\() ->
    let mut b = Bag { v: 0 }
    b += 1
    0i32
",
        Edition::E2027,
        "E1043",
    )
    .into_data();
    assert_eq!(ambig.kind, Some("ambiguous_instance_candidates"));

    // 7. Element projection on non-builtin target (`ts[i] += y`) -> E1014 with custom help.
    let elem_proj = rejects_in(
        "\
def main :: unit -> i32 = \\() ->
    let mut ts: [Tensor<f64>] = [Tensor.zeros [2], Tensor.ones [2]]
    let y: Tensor<f64> = Tensor.ones [2]
    ts[0] += y
    0i32
",
        Edition::E2027,
        "E1014",
    )
    .into_data();
    assert_eq!(elem_proj.kind, Some("mutate_immutable_value"));
    assert_eq!(
        elem_proj.help.as_deref(),
        Some(
            "'+=' on Tensor<f64> borrows its target exclusively, which an element cannot be; keep the value in a 'let mut' local or a record field"
        )
    );

    // 8. Self-overlap (`x += x` and `model.w += model.w`) -> E1014 with snapshot help,
    // while disjoint fields (`model.w += model.g`) succeed!
    let self_overlap = rejects_in(
        "let mut x: Tensor<f64> = Tensor.zeros [3]\nx += x\n0",
        Edition::E2027,
        "E1014",
    )
    .into_data();
    assert_eq!(self_overlap.kind, Some("borrow_conflict"));
    assert_eq!(
        self_overlap.help.as_deref(),
        Some("copy the right side first with 'Tensor.clone', bound with 'let'")
    );

    let field_overlap = rejects_in(
        "\
record Model { w: Tensor<f64>, g: Tensor<f64> }
def main :: unit -> i32 = \\() ->
    let mut model = Model { w: Tensor.zeros [3], g: Tensor.ones [3] }
    model.w += model.w
    0i32
",
        Edition::E2027,
        "E1014",
    )
    .into_data();
    assert_eq!(field_overlap.kind, Some("borrow_conflict"));
    assert_eq!(
        field_overlap.help.as_deref(),
        Some("copy the right side first with 'Tensor.clone', bound with 'let'")
    );
}

#[test]
fn tensor_compound_assign_zero_allocations_and_move_on_return() {
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc20-alloc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Tsuzuri.toml"),
        "[package]\nname = \"nc20alloc\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(
        &main_tz,
        r#"record Model { w: Tensor<f64>, g: Tensor<f64> }

def bump_mut { r } :: ref mut Tensor.Mut<f64> { r } -> unit = \m ->
    deref m += 1.0

def bump_tensor :: ref mut Tensor<f64> -> unit = \m ->
    deref m += 2.0

def return_plain :: unit -> Tensor<f64> = \() ->
    let t = Tensor.zeros [256]
    t

def return_after_add :: ref Tensor<f64> -> Tensor<f64> = \u ->
    let mut t = Tensor.zeros [256]
    t += u
    t

def return_model_plain :: unit -> Model = \() ->
    Model { w: Tensor.zeros [256], g: Tensor.ones [256] }

def return_model_after_add :: ref Tensor<f64> -> Model = \u ->
    let mut m = Model { w: Tensor.zeros [256], g: Tensor.ones [256] }
    m.w += u
    m

def main :: unit -> i32 = \() ->
    let mut x = Tensor.init [500] (\i -> i as f64)
    let y = Tensor.ones [500]
    let s1 = Alloc.snapshot ()
    x += y
    let u1 = Alloc.since s1

    let s2 = Alloc.snapshot ()
    x += 2.5
    let u2 = Alloc.since s2

    let mut model = Model { w: Tensor.zeros [500], g: Tensor.init [500] (\i -> (i + 1) as f64) }
    let s3 = Alloc.snapshot ()
    model.w += model.g
    let u3 = Alloc.since s3

    let mut raw = new [f64](500, \i -> i as f64)
    let mut mw = Tensor.borrow_mut [500] (ref mut raw)
    let s4 = Alloc.snapshot ()
    mw += 1.0
    bump_mut (ref mut mw)
    let u4 = Alloc.since s4

    let s5 = Alloc.snapshot ()
    bump_tensor (ref mut x)
    let u5 = Alloc.since s5

    let mut mat_s: Tensor<f64> = Tensor.zeros [10, 50]
    let mut mat_l: Tensor<f64> = Tensor.zeros [200, 50]
    let row: Tensor<f64> = Tensor.ones [50]
    let sb1 = Alloc.snapshot ()
    mat_s += row
    let ub1 = Alloc.since sb1
    let sb2 = Alloc.snapshot ()
    mat_l += row
    let ub2 = Alloc.since sb2

    let u_ret = Tensor.ones [256]
    let sp0 = Alloc.snapshot ()
    let _tp = return_plain ()
    let up0 = Alloc.since sp0
    let sp1 = Alloc.snapshot ()
    let _ta = return_after_add (ref u_ret)
    let up1 = Alloc.since sp1

    let sm0 = Alloc.snapshot ()
    let _mp = return_model_plain ()
    let um0 = Alloc.since sm0
    let sm1 = Alloc.snapshot ()
    let _ma = return_model_after_add (ref u_ret)
    let um1 = Alloc.since sm1

    do! IO.writeln ($"{u1.allocs}/{u2.allocs}/{u3.allocs}/{u4.allocs}/{u5.allocs} bcast={ub1.allocs == ub2.allocs} ret={up0.allocs == up1.allocs && up0.bytes == up1.bytes} mret={um0.allocs == um1.allocs && um0.bytes == um1.bytes}")
    0i32
"#,
    )
    .unwrap();
    for opt in ["-O0", "-O3"] {
        let exe = dir.join(if cfg!(windows) {
            format!("alloc{opt}.exe")
        } else {
            format!("alloc{opt}")
        });
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["build", "--allocator", "counting", opt, "-o"])
            .arg(&exe)
            .arg(&dir)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{opt} build stderr:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let out = std::process::Command::new(&exe).output().unwrap();
        assert!(
            out.status.success(),
            "{opt} stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(stdout, "0/0/0/0/0 bcast=true ret=true mret=true\n", "{opt}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deferred_slice_borrow_and_array_slice_target_diagnostics() {
    // `xs[1..3] += 1` on a built-in array still reports `E0002` `slice_must_be_borrowed`.
    let arr_slice = rejects_in(
        "def main :: unit -> i32 = \\() ->\n    let mut xs = [1, 2, 3, 4]\n    xs[1..3] += 1\n    0i32\n",
        Edition::E2028,
        "E0002",
    )
    .into_data();
    assert_eq!(arr_slice.kind, Some("slice_must_be_borrowed"));

    // Deferred `SliceMustBeBorrowed` fires when the bracket is NOT the direct LHS of `op=`.
    for bad in [
        "def f :: Tensor.View<f64> -> i64 = \\_ -> 0\ndef main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [4]\n    let mut x = 0\n    f t[1..3] += 1\n    0i32\n",
        "def main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [4]\n    let a = Tensor.zeros [2]\n    a + t[1..3] += a\n    0i32\n",
        "def main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [4]\n    t[1..3] = t[1..3]\n    0i32\n",
    ] {
        let d = rejects_in(bad, Edition::E2028, "E0002").into_data();
        assert_eq!(d.kind, Some("slice_must_be_borrowed"), "{bad}\n{d:?}");
    }

    // Explicit `ref` / `ref mut` on LHS -> E1012 with `write the target without 'ref'`.
    for bad in [
        "def main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [3, 3]\n    ref t[1..2, ..] += 1.0\n    0i32\n",
        "def main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [3, 3]\n    ref mut t[1..2, ..] += 1.0\n    0i32\n",
    ] {
        let d = rejects_in(bad, Edition::E2028, "E1012").into_data();
        assert_eq!(d.kind, Some("assign_to_immutable_place"), "{bad}\n{d:?}");
        assert_eq!(
            d.help.as_deref(),
            Some("write the target without 'ref': 't[...] += ...'")
        );
    }

    // Formatter prints `t[1..3] += u` idempotently.
    let src = "def main :: unit -> i32 = \\() ->\n    let mut t = Tensor.zeros [4]\n    let u = Tensor.ones [3]\n    t[1 .. 3] += u\n    0i32\n";
    let formatted = tsuzuri::formatter::format_source_with_edition(
        "Main.tz",
        src,
        tsuzuri::syntax::SourceKind::from_extension("tz").unwrap(),
        Edition::E2028,
    )
    .unwrap();
    assert!(!formatted.changed);
    assert_eq!(formatted.formatted, src);
}
