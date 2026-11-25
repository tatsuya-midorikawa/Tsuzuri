use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::check::{CheckedModule, TypedExpr, TypedExprKind};
use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::driver::Project;
use tsuzuri::{analyze, llvm};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/reference-operands")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn check(edition: &str, source: &str) -> Result<CheckedModule, Diagnostic> {
    let root = scratch_dir("check");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        format!("[package]\nname = \"refops\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n"),
    )
    .unwrap();
    let result = Project::load(&root).unwrap().analyze();
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn check_all(edition: &str, source: &str) -> Vec<Diagnostic> {
    let root = scratch_dir("check-all");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        format!("[package]\nname = \"refops\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n"),
    )
    .unwrap();
    let result = Project::load(&root).unwrap().analyze_all();
    let _ = std::fs::remove_dir_all(&root);
    result.expect_err(source).diagnostics
}

fn emits(source: &str) -> [String; 2] {
    let module = check("2027", source).unwrap_or_else(|error| {
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

fn rejects(source: &str, code: &str) -> String {
    let error = check("2027", source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.into_data().message
}

fn run_exit_code(source: &str, compact_strings: bool) -> i32 {
    let root = scratch_dir("run");
    std::fs::create_dir_all(&root).unwrap();
    let wrapped = format!(
        "{source}\n\
         test \"ref_ops_check\" =\n\
         \x20   assert (main () == 0i32)\n"
    );
    std::fs::write(root.join("Main.tz"), &wrapped).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"refops\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    cmd.arg("test").arg(&root);
    if compact_strings {
        cmd.env("TSUZURI_STRINGS", "compact");
    }
    let out = cmd.output().unwrap();
    let _ = std::fs::remove_dir_all(&root);
    if !out.status.success() {
        panic!(
            "test failed for:\n{source}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    0
}

fn build_llvm(source: &str, opt: &str, wasm: bool, compact_strings: bool) -> String {
    let root = scratch_dir("llvm");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"refops\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let ll = root.join("out.ll");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    cmd.arg("build")
        .arg(&root)
        .arg("--emit")
        .arg("llvm")
        .arg(opt)
        .arg("-o")
        .arg(&ll);
    if wasm {
        cmd.arg("--target").arg("wasm32-unknown-unknown");
    }
    if compact_strings {
        cmd.env("TSUZURI_STRINGS", "compact");
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&ll).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    text
}

fn extract_fn_body<'a>(ir: &'a str, name_fragment: &str) -> &'a str {
    let start = ir
        .find(&format!("@{name_fragment}("))
        .unwrap_or_else(|| panic!("missing @{name_fragment} in IR"));
    let line_start = ir[..start].rfind('\n').map_or(0, |i| i + 1);
    let rest = &ir[line_start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |i| i + 3);
    &rest[..end]
}

#[test]
fn string_operands_borrow_their_references() {
    let src = "\
def greet :: string -> string -> string
fn greet a b = a + b

def same :: string -> string -> bool
fn same a b = a == b

def greet_u8 :: utf8string -> utf8string -> utf8string
fn greet_u8 a b = a + b

def same_u8 :: utf8string -> utf8string -> bool
fn same_u8 a b = a == b

def wrap :: string -> string
fn wrap a = \"<\" + (a + \"!\")

def main :: unit -> i32 = \\() ->
    let ok1 = greet \"ab\" \"cd\" == \"abcd\"
    let ok2 = same \"xy\" \"xy\" && !(same \"xy\" \"xz\")
    let ok3 = greet_u8 u8\"ab\" u8\"cd\" == u8\"abcd\"
    let ok4 = same_u8 u8\"xy\" u8\"xy\" && !(same_u8 u8\"xy\" u8\"xz\")
    let ok5 = wrap \"hi\" == \"<hi!\"
    if ok1 && ok2 && ok3 && ok4 && ok5 then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn string_concatenation_does_not_consume_references() {
    let greet_src = "\
export def greet :: string -> string -> string
fn greet a b = a + b

export def framed :: string -> string -> i64
fn framed a b =
    let s = a + b
    let len_only = (a + b).length
    s.length + len_only + a.length + b.length

def owned_left_ref_right :: own string -> string -> string
fn owned_left_ref_right a b = a + b

def main :: unit -> i32 = \\() ->
    let s1 = greet \"ab\" \"cd\"
    let n = framed \"hello\" \"world\"
    let base = \"ab\"
    let r = ref base
    let joined = r + \"c\"
    let mix = owned_left_ref_right \"pq\" base
    let after_base = base + \"d\"
    if s1 == \"abcd\" && n == 30 && joined == \"abc\" && after_base == \"abd\" && mix == \"pqab\" then 0i32 else 1i32
";
    for compact in [false, true] {
        // Use wasm target so `staying_locals` / `concat_into` runs on Windows hosts too
        // (`native_windows` disables `staying` stack buffers for SEH stack-probe bounds).
        let ir = build_llvm(greet_src, "-O0", true, compact);
        let greet_fn = extract_fn_body(&ir, "tz.fn.Main.greet");
        assert!(
            !greet_fn.contains("append("),
            "greet should not call append (compact={compact}):\n{greet_fn}"
        );
        assert!(
            !greet_fn.contains("@tz.free") && !greet_fn.contains("drop"),
            "greet should not free borrowed operands (compact={compact}):\n{greet_fn}"
        );
        assert_eq!(
            greet_fn.matches("concat(").count(),
            1,
            "greet should call concat once (compact={compact}):\n{greet_fn}"
        );
        let framed_fn = extract_fn_body(&ir, "tz.fn.Main.framed");
        assert!(
            !framed_fn.contains("append("),
            "framed should not call append (compact={compact}):\n{framed_fn}"
        );
        assert!(
            framed_fn.contains("concat_into"),
            "framed should use concat_into for non-escaping frame concat (compact={compact}):\n{framed_fn}"
        );
        let mix_fn = extract_fn_body(&ir, "tz.fn.Main.owned_left_ref_right");
        assert_eq!(
            mix_fn.matches("@tz.free(").count(),
            1,
            "owned_left_ref_right must only have the zeroed own-param scope-exit free, never freeing borrowed right (compact={compact}):\n{mix_fn}"
        );
        assert_eq!(run_exit_code(greet_src, compact), 0);
    }

    // P9: owned `s` is still consumed by `s + "c"`.
    rejects(
        "def main :: unit -> i32 = \\() ->\n\
         \x20   let s = \"ab\"\n\
         \x20   let _ = s + \"c\"\n\
         \x20   let _ = s\n\
         \x20   0i32\n",
        "E1012",
    );
}

#[test]
fn bigint_operands_borrow_their_references() {
    let src = "\
def calc :: BigInt -> BigInt -> BigInt
fn calc a b = if a < b then (a + b) + (-a) + (a + 1) else b + 0I

def main :: unit -> i32 = \\() ->
    let x = 10I
    let y = 25I
    let r = calc x y
    if r == 36I && x == 10I && y == 25I then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);

    let ir_ref = build_llvm(
        "def add_big :: BigInt -> BigInt -> BigInt\nfn add_big a b = a + b\ndef main :: unit -> i32 = \\() -> if add_big 1I 2I == 3I then 0i32 else 1i32\n",
        "-O0",
        false,
        false,
    );
    let ir_deref = build_llvm(
        "def add_big :: BigInt -> BigInt -> BigInt\nfn add_big a b = deref a + deref b\ndef main :: unit -> i32 = \\() -> if add_big 1I 2I == 3I then 0i32 else 1i32\n",
        "-O0",
        false,
        false,
    );
    assert_eq!(ir_ref, ir_deref);
}

#[test]
fn record_operands_stay_usable() {
    let src = "\
record Meter { label: string, n: i64 }
instance Add<Meter> {
    fn add l r = Meter { label: ref l.label + ref r.label, n: l.n + r.n }
}

def combine :: Meter -> Meter -> i64
fn combine a b =
    let c = a + b
    c.n + a.n + b.n + c.label.length

def main :: unit -> i32 = \\() ->
    let x = Meter { label: \"a\", n: 3 }
    let y = Meter { label: \"bc\", n: 4 }
    let total = combine x y
    if total == 17 && x.n == 3 && y.n == 4 then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn array_and_mutable_reference_operands() {
    let src = "\
def cmp_arr :: ref mut [i64] -> ref [i64] -> bool
fn cmp_arr a b = a < b

def cmp_shared :: [i64] -> [i64] -> bool
fn cmp_shared a b = a < b

def add_mut :: ref mut i64 -> ref mut i64 -> i64
fn add_mut a b = a + b

def main :: unit -> i32 = \\() ->
    let mut xs = [1, 2]
    let ys = [1, 3]
    let mut p = 7
    let mut q = 8
    let ok1 = cmp_arr (ref mut xs) (ref ys)
    let ok2 = cmp_shared xs ys
    let ok3 = add_mut (ref mut p) (ref mut q) == 15
    if ok1 && ok2 && ok3 then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn mixed_reference_and_value_operands() {
    let src = "\
def mix_i64 :: ref i64 -> i64 -> i64
fn mix_i64 a n =
    let z = a + n
    let w = (a + 1) + (1 + a)
    let m = match Some (a + a) with | Some v -> v | None -> 0
    let neg_m = match Some (-a) with | Some v -> v | None -> 0
    let nested = a + (a + a)
    if a == n && a < n + 1 && a == -(-a) && a == n + 0 then z + w + m + neg_m + nested else 0

def mix_tree :: ref i64 -> ref i64 -> ref i64 -> ref i64 -> i64 -> bool
fn mix_tree a b c d n =
    (a < b + 1)
        && (a < n + 1)
        && (a == n + b)
        && (a == b + c + d)
        && (a == 1 + (b + c))
        && (a == -(b + c))

def mix_str :: string -> string -> bool
fn mix_str a b =
    let owned = \"ab\"
    (a == owned) && (owned == a) && (a == \"ab\") && (a == b + \"b\") && (a == owned + \"\")

def mix_big :: ref BigInt -> BigInt -> BigInt -> bool
fn mix_big a b c =
    let sum = a + b
    sum == b + c

def mix_bool :: ref bool -> ref bool -> bool
fn mix_bool a b =
    let opt = match Some (!b) with | Some v -> v | None -> false
    (a && !b) && opt

def main :: unit -> i32 = \\() ->
    let x: i64 = 3
    let r = ref x
    let reborrow_add = (ref r) + 1
    let v1 = mix_i64 r 3
    let p: i64 = 5
    let q: i64 = 2
    let s: i64 = 2
    let t: i64 = 1
    let _neg5: i64 = -5
    let ok_tree1 = mix_tree (ref p) (ref q) (ref s) (ref t) 3
    let qn: i64 = -3
    let sn: i64 = -2
    let ok_tree2 = (ref p) == -((ref qn) + (ref sn))
    let _ = ok_tree1
    let ok_str = mix_str \"ab\" \"a\"
    let ok_big = mix_big (ref 3I) 4I 3I
    let bt = true
    let bf = false
    let ok_bool = mix_bool (ref bt) (ref bf)
    if reborrow_add == 4 && v1 == 26 && ok_tree2 && ok_str && ok_big && ok_bool then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn scalar_reference_operands_match_deref() {
    for wasm in [false, true] {
        for opt in ["-O0", "-O3"] {
            let ir_ref = build_llvm(
                "def f :: ref i64 -> i64\nfn f a = a + 1\nexport def entry :: i64 -> i64\nfn entry x = f (ref x)\ndef main :: unit -> i32 = \\() -> 0i32\n",
                opt,
                wasm,
                false,
            );
            let ir_deref = build_llvm(
                "def f :: ref i64 -> i64\nfn f a = deref a + 1\nexport def entry :: i64 -> i64\nfn entry x = f (ref x)\ndef main :: unit -> i32 = \\() -> 0i32\n",
                opt,
                wasm,
                false,
            );
            assert_eq!(ir_ref, ir_deref, "mismatch for wasm={wasm}, opt={opt}");
        }
    }

    // Typed IR check: both operands of `a && b` and `a || b` on `ref bool` are unwrapped `bool`.
    let module = analyze(
        "def both_and :: ref bool -> ref bool -> bool\n\
         fn both_and a b = a && b\n\
         def both_or :: ref bool -> ref bool -> bool\n\
         fn both_or a b = a || b\n",
    )
    .unwrap();
    fn assert_bool_logic(expr: &TypedExpr) {
        if let TypedExprKind::Binary(_, left, right) = &expr.kind {
            assert_eq!(format!("{:?}", left.ty), "Bool");
            assert_eq!(format!("{:?}", right.ty), "Bool");
        }
    }
    for function in &module.functions {
        if function.name.ends_with("both_and") || function.name.ends_with("both_or") {
            assert_bool_logic(&function.body);
        }
    }

    let src = "\
def bits_ops :: ref i64 -> ref i64 -> i64
fn bits_ops a b = (a &&& b) + (a <<< 1) + (~~~a) + (-b)

def not_annotated :: ref bool -> bool
fn not_annotated a = !a

def bump_cnt :: ref mut i64 -> bool
fn bump_cnt counter =
    deref counter = deref counter + 1
    true

def short_circuit :: ref bool -> ref mut i64 -> bool
fn short_circuit flag counter =
    let c = !flag
    let r1 = flag && bump_cnt counter
    let r2 = (!flag) || bump_cnt counter
    let t = true
    let r3 = (flag || t) && (t && c)
    (!r1) && r2 && r3

def main :: unit -> i32 = \\() ->
    let x: i64 = 6
    let y: i64 = 3
    let b = bits_ops (ref x) (ref y)
    let f = false
    let mut hits: i64 = 0
    let ok = short_circuit (ref f) (ref mut hits) && not_annotated (ref f)
    if b == 4 && ok && hits == 0 then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn tensor_operands() {
    let src = "\
using std

def same_tensor :: Tensor<i64> -> Tensor<i64> -> bool
fn same_tensor a b = a == b

def main :: unit -> i32 = \\() ->
    let t1 = Tensor.of_array [2] [10, 20]
    let t2 = Tensor.of_array [2] [10, 20]
    let t3 = Tensor.of_array [2] [10, 21]
    if same_tensor t1 t2 && !(same_tensor t1 t3) then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn explicit_borrow_operands() {
    let src = "\
def main :: unit -> i32 = \\() ->
    let a = 12345678901234567890I
    let b = 98765432109876543210I
    if (ref a + ref b) == (a + b) then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn user_reference_instances_keep_today_path() {
    let src = "\
record Foo { n: i64, tag: i64 }
instance Eq<Foo> {
    fn eq l r = l.n == r.n
    fn ne l r = !(l.n == r.n)
}
instance Eq<ref Foo> {
    fn eq l r = (deref l).tag == (deref r).tag
    fn ne l r = !((deref l).tag == (deref r).tag)
}

def eq_refs :: ref Foo -> ref Foo -> bool
fn eq_refs a b = a == b

def main :: unit -> i32 = \\() ->
    let x = Foo { n: 1, tag: 99 }
    let y = Foo { n: 2, tag: 99 }
    if eq_refs (ref x) (ref y) then 0i32 else 1i32
";
    emits(src);
    assert_eq!(run_exit_code(src, false), 0);
}

#[test]
fn generic_reference_operands_keep_today_path() {
    let ok_src = "\
record Foo { tag: i64 }
instance Eq<ref Foo> {
    fn eq l r = (deref l).tag == (deref r).tag
    fn ne l r = !((deref l).tag == (deref r).tag)
}

def gen :: Eq<ref 'a> => ref 'a -> ref 'a -> bool
fn gen a b = a == b

def earlier :: Ord<'s> => ref 's -> ref 's -> bool
fn earlier left right = deref left < deref right

def main :: unit -> i32 = \\() ->
    let x = Foo { tag: 7 }
    let y = Foo { tag: 7 }
    let s1 = \"abc\"
    let s2 = \"abd\"
    if gen (ref x) (ref y) && earlier (ref s1) (ref s2) then 0i32 else 1i32
";
    emits(ok_src);
    assert_eq!(run_exit_code(ok_src, false), 0);

    let bad_src = "\
def gen :: Eq<ref 'a> => ref 'a -> ref 'a -> bool
fn gen a b = a == b

def main :: unit -> i32 = \\() ->
    let x: i32 = 1i32
    let y: i32 = 1i32
    if gen (ref x) (ref y) then 0i32 else 1i32
";
    let err = analyze(bad_src).expect_err(bad_src);
    assert_eq!(err.code, "E1005");
}

#[test]
fn reference_operands_keep_borrow_conflicts() {
    let bump_conflict = "\
def bump :: ref mut i64 -> i64
fn bump p =
    deref p = deref p + 1
    deref p

def main :: unit -> i32 = \\() ->
    let mut x: i64 = 1
    let r = ref x
    let _ = r + bump (ref mut x)
    0i32
";
    rejects(bump_conflict, "E1014");

    let string_move_conflict = "\
def take :: own string -> string
fn take s = s

def main :: unit -> i32 = \\() ->
    let s = \"hello\"
    let r = ref s
    let _ = r == take s
    0i32
";
    rejects(string_move_conflict, "E1014");
}

#[test]
fn non_copy_bits_operands_match_deref() {
    let non_copy = "\
record Mask { bits: i64, label: string }
instance Bits<Mask> {
    fn bit_and l r = Mask { bits: l.bits &&& r.bits, label: \"m\" }
    fn bit_or l _ = Mask { bits: l.bits, label: \"m\" }
    fn bit_xor l _ = Mask { bits: l.bits, label: \"m\" }
    fn bit_not v = Mask { bits: ~~~v.bits, label: \"m\" }
    fn shl l _ = Mask { bits: l.bits, label: \"m\" }
    fn shr l _ = Mask { bits: l.bits, label: \"m\" }
    fn ushr l _ = Mask { bits: l.bits, label: \"m\" }
}

def both :: Mask -> Mask -> Mask
fn both a b = a &&& b
";
    let err_ref = check("2027", non_copy).expect_err(non_copy);
    let err_deref =
        check("2027", &non_copy.replace("a &&& b", "deref a &&& deref b")).expect_err("deref");
    assert_eq!(err_ref.code, "E1012");
    assert_eq!(err_ref.code, err_deref.code);

    let copy_mask = "\
record CopyMask { bits: i64 }
instance Bits<CopyMask> {
    fn bit_and l r = CopyMask { bits: l.bits &&& r.bits }
    fn bit_or l r = CopyMask { bits: l.bits ||| r.bits }
    fn bit_xor l r = CopyMask { bits: l.bits ^^^ r.bits }
    fn bit_not v = CopyMask { bits: ~~~v.bits }
    fn shl l n = CopyMask { bits: l.bits <<< n.bits }
    fn shr l n = CopyMask { bits: l.bits >>> n.bits }
    fn ushr l _ = l
}

def both :: CopyMask -> CopyMask -> CopyMask
fn both a b = a &&& b

def main :: unit -> i32 = \\() ->
    let m = both (CopyMask { bits: 6 }) (CopyMask { bits: 3 })
    if m.bits == 2 then 0i32 else 1i32
";
    emits(copy_mask);
    assert_eq!(run_exit_code(copy_mask, false), 0);
}

#[test]
fn nested_reference_strips_one_level() {
    rejects("def f :: ref (ref i64) -> i64\nfn f rr = rr + 1\n", "E1003");

    let e2026_add = check(
        "2026",
        "def main :: unit -> i32 = \\() ->\n\
         \x20   let x: i64 = 3\n\
         \x20   let r = ref x\n\
         \x20   let rr = &r\n\
         \x20   let _ = rr + 1\n\
         \x20   0i32\n",
    )
    .expect_err("rr + 1");
    assert_eq!(e2026_add.code, "E1003");
    assert!(
        e2026_add
            .message
            .contains("expected ref ref i64, found i32"),
        "{}",
        e2026_add.message
    );

    let e2026_and = check(
        "2026",
        "def main :: unit -> i32 = \\() ->\n\
         \x20   let x = true\n\
         \x20   let r = &x\n\
         \x20   let rr = &r\n\
         \x20   let _ = r && rr\n\
         \x20   0i32\n",
    )
    .expect_err("r && rr");
    assert_eq!(e2026_and.code, "E1003");

    let e2026_eq = check(
        "2026",
        "def main :: unit -> i32 = \\() ->\n\
         \x20   let x: i64 = 1\n\
         \x20   let r = &x\n\
         \x20   let rr = &r\n\
         \x20   let _ = r == rr\n\
         \x20   0i32\n",
    )
    .expect_err("r == rr");
    assert_eq!(e2026_eq.code, "E1003");
}

#[test]
fn failed_strip_keeps_today_diagnostic() {
    // P17: non-Copy `record Foo` without `Add` emits 2 E1003s on `a` and `b`.
    let p17 =
        "record Foo { n: i64, tag: string }\ndef plus :: Foo -> Foo -> Foo\nfn plus a b = a + b\n";
    let errs = check_all("2027", p17);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert_eq!(errs[0].code, "E1003");
    assert_eq!(errs[1].code, "E1003");
    assert!(errs[0].span.start < errs[1].span.start);

    let p17_mixed =
        "record Foo { n: i64, tag: string }\ndef plus :: Foo -> i64 -> Foo\nfn plus a b = a + b\n";
    let errs = check_all("2027", p17_mixed);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert_eq!(errs[0].code, "E1003");
    assert_eq!(errs[1].code, "E1003");
    assert!(
        errs[1].message.contains("expected Main.Foo, found i64"),
        "{}",
        errs[1].message
    );

    // No `Eq`: `a == b` emits E1005.
    let no_eq = "record Foo { n: i64, tag: string }\ndef eq_foo :: Foo -> Foo -> bool\nfn eq_foo a b = a == b\n";
    let errs = check_all("2027", no_eq);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E1005");

    // Reference expected type in array literals.
    let arr_add = "def f :: ref i64 -> ref i64 -> i64\nfn f x a =\n    let xs = [x, a + a]\n    deref xs[0]\n";
    let err = check("2027", arr_add).expect_err(arr_add);
    assert_eq!(err.code, "E1005");
    assert!(err.message.contains("Add<ref i64>"), "{}", err.message);

    let arr_neg =
        "def f :: ref i64 -> ref i64 -> i64\nfn f x a =\n    let xs = [x, (-a)]\n    deref xs[0]\n";
    let err = check("2027", arr_neg).expect_err(arr_neg);
    assert_eq!(err.code, "E1005");
    assert!(err.message.contains("Neg<ref i64>"), "{}", err.message);

    let arr_not = "def f :: ref bool -> ref bool -> bool\nfn f x a =\n    let xs = [x, !a]\n    deref xs[0]\n";
    let err = check("2027", arr_not).expect_err(arr_not);
    assert_eq!(err.code, "E1003");
    assert!(
        err.message.contains("expected bool, found ref bool"),
        "{}",
        err.message
    );

    // Expected inference var appearing in local `g`'s type.
    let g_base = "\
record Foo { n: i64, tag: string }
def first :: own 'a -> own 'b -> 'a
fn first x _ = x
def f :: ref Foo -> ref Foo -> Foo -> i64
fn f a b c =
    let g = \\y -> y
    (g (a &&& first (g c) b)).n
";
    let err = check("2027", g_base).expect_err(g_base);
    assert_eq!(err.code, "E1005");
    assert!(
        err.message.contains("no instance for Bits<ref Main.Foo>"),
        "{}",
        err.message
    );

    let g_with_bits = format!(
        "instance Bits<ref Foo> {{\n\
         \x20   fn bit_and l _ = l\n\
         \x20   fn bit_or l _ = l\n\
         \x20   fn bit_xor l _ = l\n\
         \x20   fn bit_not v = v\n\
         \x20   fn shl l _ = l\n\
         \x20   fn shr l _ = l\n\
         \x20   fn ushr l _ = l\n\
         }}\n\
         {g_base}\n\
         def main :: unit -> i32 = \\() ->\n\
         \x20   let x = Foo {{ n: 4, tag: \"x\" }}\n\
         \x20   let y = Foo {{ n: 9, tag: \"y\" }}\n\
         \x20   let z = Foo {{ n: 2, tag: \"z\" }}\n\
         \x20   if f (ref x) (ref y) z == 4 then 0i32 else 1i32\n"
    );
    emits(&g_with_bits);
    assert_eq!(run_exit_code(&g_with_bits, false), 0);

    // Nested rollback cases where non-Copy `Foo` has `Add` (`Add<Foo>`) but neither `Eq` nor `Mul`.
    let foo_add = "\
record Foo { n: i64, tag: string }
instance Add<Foo> {
    fn add l r = Foo { n: l.n + r.n, tag: ref l.tag + ref r.tag }
}
";
    let eq_right_add =
        format!("{foo_add}def f :: Foo -> Foo -> Foo -> bool\nfn f a b c = a == b + c\n");
    let errs = check_all("2027", &eq_right_add);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E1005");

    let eq_left_add =
        format!("{foo_add}def f :: Foo -> Foo -> Foo -> bool\nfn f a b c = b + c == a\n");
    let errs = check_all("2027", &eq_left_add);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E1005");

    for expr in ["a * (b + c)", "(b + c) * a"] {
        let mul_nested =
            format!("{foo_add}def f :: Foo -> Foo -> Foo -> Foo\nfn f a b c = {expr}\n");
        let errs = check_all("2027", &mul_nested);
        assert_eq!(errs.len(), 3, "{expr}: {errs:?}");
        assert!(errs.iter().all(|e| e.code == "E1003"), "{expr}: {errs:?}");
    }

    let pick_case = format!(
        "{foo_add}\
         def pick :: own 'a -> 'a\n\
         fn pick x = x\n\
         def f :: Foo -> Foo -> bool\n\
         fn f b c = b + c == pick 1\n"
    );
    let errs = check_all("2027", &pick_case);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E1005");

    let owned_plus_ref =
        format!("{foo_add}def f :: Foo -> own Foo -> Foo -> bool\nfn f a n b = a == n + b\n");
    let errs = check_all("2027", &owned_plus_ref);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E1003");

    let wrong_ret = "def f :: ref i64 -> i64 -> string\nfn f a n = a < n + 1\n";
    let errs = check_all("2027", wrong_ret);
    assert!(
        errs.iter().all(|e| e.code == "E1003") && errs.len() >= 2,
        "{errs:?}"
    );
}

#[test]
fn editions_2026_and_2027_support_reference_operands() {
    check(
        "2026",
        "def same :: ref string -> ref string -> bool\n\
         fn same a b = a == b\n\
         pure def join :: string -> string -> string\n\
         fn join a b = a + b\n",
    )
    .unwrap();

    check(
        "2027",
        "def greet :: string -> string -> string\n\
         fn greet a b = a + b\n",
    )
    .unwrap();

    let errs = check_all(
        "2027",
        "record Foo { n: i64, tag: string }\ndef plus :: Foo -> Foo -> Foo\nfn plus a b = a + b\n",
    );
    assert_eq!(errs.len(), 2);
}
