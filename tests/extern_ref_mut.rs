use std::fs;
use std::process::Command;
use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::formatter::format_source;
use tsuzuri::syntax::SourceKind;
use tsuzuri::{analyze, llvm};

fn emit_ir(source: &str) -> String {
    let module = analyze(source).expect("source should analyze");
    let a = llvm::emit_target(&module, llvm::Entry::Console, false).expect("IR should emit");
    let b = llvm::emit_target(&module, llvm::Entry::Console, false).expect("IR deterministic");
    assert_eq!(a, b);
    a
}

#[test]
fn ref_mut_record_is_copied_in_and_out() {
    let source = r#"
record Rect { left: i32, top: i32, right: i32, bottom: i32 }

@trusted
extern "c_inflate" def inflate :: ref mut Rect -> i32

@trusted
extern "c_with_cb" def with_cb :: (i64 -> i64) -> ref mut Rect -> i32

def step :: i64 -> i64 = \x -> x + 1

def main :: unit -> i32 = \() ->
    let mut rc = Rect { left: 1i32, top: 2i32, right: 3i32, bottom: 4i32 }
    let _a = inflate (ref mut rc)
    let _b = with_cb step (ref mut rc)
    rc.right - rc.left
"#;
    let ir = emit_ir(source);
    assert!(ir.contains("declare i32 @c_inflate(ptr)"), "{ir}");
    assert!(ir.contains("declare i32 @c_with_cb(ptr, ptr)"), "{ir}");

    let fn_start = ir.find("define internal i32 @tz.fn.Main.inflate(").unwrap();
    let fn_end = ir[fn_start..].find("\n}\n").unwrap() + fn_start;
    let body = &ir[fn_start..fn_end];
    let slot_alloca = body.find("alloca %tz_record_4Main_4Rect").unwrap();
    let call_pos = body.find("call i32 @c_inflate(ptr ").unwrap();
    let store_back = body[call_pos..]
        .find("store %tz.record.Main.Rect ")
        .unwrap()
        + call_pos;
    assert!(slot_alloca < call_pos && call_pos < store_back, "{body}");
}

#[test]
fn ref_mut_scalar_widths_and_copy_handles() {
    let source = r#"
extern type copy Hdc

@trusted
extern "c_scalars" def scalars :: ref mut i16u -> ref mut bool -> ref mut f64 -> ref mut Hdc -> unit

@trusted
extern "c_make_hdc" def make_hdc :: unit -> Hdc

def main :: unit -> i32 = \() ->
    let mut w: i16u = 7i16u
    let mut b = false
    let mut d = 1.5
    let mut h = make_hdc ()
    scalars (ref mut w) (ref mut b) (ref mut d) (ref mut h)
    if b then w as i32 else 0i32
"#;
    let ir = emit_ir(source);
    assert!(
        ir.contains("declare void @c_scalars(ptr, ptr, ptr, ptr)"),
        "{ir}"
    );
    assert!(ir.contains("alloca i16, align 2"), "{ir}");
    assert!(ir.contains("alloca i32, align 4"), "{ir}");
    assert!(ir.contains("alloca double, align 8"), "{ir}");
    assert!(ir.contains("alloca ptr, align 8"), "{ir}");
    assert!(ir.contains("zext i1 "), "{ir}");
    assert!(ir.contains("icmp ne i32 "), "{ir}");
}

#[test]
fn mutable_buffer_passes_storage_without_copy() {
    let source = r#"
@trusted
extern "c_fill_view" def fill_view :: ref mut [i16u..] -> i32

@trusted
extern "c_fill_arr" def fill_arr :: ref mut [i32] -> i32

def use_view :: ref mut [i16u..] -> i32 = \buf -> fill_view buf

def use_arr :: ref mut [i32] -> i32 = \buf -> fill_arr buf

def main :: unit -> i32 = \() ->
    let mut u: [i16u] = [1i16u, 2i16u, 3i16u]
    let mut a: [i32] = [4i32, 5i32]
    use_view (ref mut u[0..2]) + use_arr (ref mut a)
"#;
    let ir = emit_ir(source);
    assert!(ir.contains("declare i32 @c_fill_view(ptr, i64)"), "{ir}");
    assert!(ir.contains("declare i32 @c_fill_arr(ptr, i64)"), "{ir}");
    for fn_name in [
        "define internal i32 @tz.fn.Main.fill_view(",
        "define internal i32 @tz.fn.Main.fill_arr(",
    ] {
        let s = ir.find(fn_name).unwrap();
        let e = ir[s..].find("\n}\n").unwrap() + s;
        let body = &ir[s..e];
        assert!(!body.contains("@tz.alloc"), "{body}");
    }
}

#[test]
fn count_types_guards_and_formatter_round_trip() {
    let source = r#"extern type copy Hwnd

@trusted
@count(2, i32)
extern "GetWindowTextW" def get_text :: ref Hwnd -> ref mut [i16u..] -> i32

@count(2, i32u)
@trusted
extern "ReadUnsigned" def read_u :: ref Hwnd -> ref [i32u] -> i32

@trusted
@count(2, none)
extern "FillRaw" def fill_raw :: i32 -> ref mut [ubyte..] -> unit

@trusted
extern "MakeHwnd" def make_hwnd :: unit -> Hwnd

def main :: unit -> i32 = \() ->
    let h = make_hwnd ()
    let mut u: [i16u] = [0i16u, 0i16u]
    let d: [i32u] = [1i32u]
    let mut b: [ubyte] = [0ubyte, 0ubyte]
    fill_raw 2i32 (ref mut b)
    get_text (ref h) (ref mut u) + read_u (ref h) (ref d)
"#;
    let formatted = format_source("Main.tz", source, SourceKind::Code).expect("formats cleanly");
    assert_eq!(formatted.formatted, source);

    let ir = emit_ir(source);
    assert!(
        ir.contains("declare i32 @GetWindowTextW(ptr, ptr, i32)"),
        "{ir}"
    );
    assert!(
        ir.contains("declare i32 @ReadUnsigned(ptr, ptr, i32)"),
        "{ir}"
    );
    assert!(ir.contains("declare void @FillRaw(i32, ptr)"), "{ir}");
    assert!(ir.contains(", 2147483647"), "{ir}");
    assert!(ir.contains(", 4294967295"), "{ir}");

    // @count(1, i64) produces the exact same IR as omitting @count.
    let plain = "@trusted\nextern \"f\" def f :: ref [i64] -> i32\ndef main :: unit -> i32 = \\() -> f (ref [1, 2])\n";
    let explicit_i64 = "@trusted\n@count(1, i64)\nextern \"f\" def f :: ref [i64] -> i32\ndef main :: unit -> i32 = \\() -> f (ref [1, 2])\n";
    assert_eq!(emit_ir(plain), emit_ir(explicit_i64));

    // Calling an extern with @count(..., i32) has the `trap` effect even when @trusted no_trap is set.
    let trap_err = analyze(
        r#"
@trusted pure no_alloc no_copy no_trap terminates
@count(1, i32)
extern "c_len" def c_len :: ref [i64] -> i32

@no_trap
def caller :: ref [i64] -> i32
fn caller xs = c_len xs

def main :: unit -> i32 = \() -> caller (ref [1])
"#,
    )
    .expect_err("@no_trap must reject @count(..., i32) bounds guard");
    assert_eq!(trap_err.code, "E1031", "{trap_err:?}");
    assert_eq!(
        trap_err.kind,
        Some(DiagnosticKind::EffectTrapInNoTrap.id()),
        "{trap_err:?}"
    );
}

#[test]
fn rejections_cover_all_w13_rules() {
    let cases: &[(&str, &str, Option<DiagnosticKind>)] = &[
        (
            "extern \"f\" def f :: ref mut string -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "extern \"f\" def f :: ref mut utf8string -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "extern \"f\" def f :: ref mut [bool] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "extern \"f\" def f :: ref mut [char..] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "extern type OwnedH\nextern \"f\" def f :: ref mut OwnedH -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "extern \"f\" def f :: ref i32 -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "record R { x: i32 }\nexport def f :: ref mut R -> i32\nfn f r = r.x\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExportSignature),
        ),
        (
            "export def f :: ref [i16u] -> i32\nfn f _xs = 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExportSignature),
        ),
        (
            "extern \"f\" def f :: (ref mut i32 -> unit) -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::UnsupportedExternSignature),
        ),
        (
            "@count(0, i32)\nextern \"f\" def f :: ref [i64] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::CountPositionOutOfRange),
        ),
        (
            "@count(2, i32)\nextern \"f\" def f :: ref [i64] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::CountPositionOutOfRange),
        ),
        (
            "@count(1, i32)\nextern \"f\" def f :: i64 -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::CountNotBuffer),
        ),
        (
            "@count(1, i32)\n@count(1, i32u)\nextern \"f\" def f :: ref [i64] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E1008",
            Some(DiagnosticKind::CountRepeated),
        ),
        (
            "@count(1, i32)\ndef f :: ref [i64] -> unit\nfn f _xs = ()\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E0002",
            Some(DiagnosticKind::MisplacedCountAttribute),
        ),
        (
            "@count(1, i32)\nextern type H\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E0002",
            Some(DiagnosticKind::MisplacedCountAttribute),
        ),
        (
            "@count(1, u16)\nextern \"f\" def f :: ref [i64] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E0002",
            Some(DiagnosticKind::CountAttributeSyntax),
        ),
        (
            "@count 1, i32\nextern \"f\" def f :: ref [i64] -> unit\ndef main :: unit -> i32 = \\() -> 0i32\n",
            "E0002",
            Some(DiagnosticKind::CountAttributeSyntax),
        ),
        (
            "record R { x: i32 }\nextern \"f\" def f :: ref mut R -> ref R -> i32\ndef main :: unit -> i32 = \\() ->\n    let mut r = R { x: 1i32 }\n    f (ref mut r) (ref r)\n",
            "E1014",
            None,
        ),
    ];

    for (index, (source, expected_code, expected_kind)) in cases.iter().enumerate() {
        let err = analyze(source).expect_err(&format!("case #{index} should fail:\n{source}"));
        assert_eq!(
            err.code, *expected_code,
            "case #{index}:\n{source}\n{err:?}"
        );
        if let Some(kind) = expected_kind {
            assert_eq!(
                err.kind,
                Some(kind.id()),
                "case #{index}:\n{source}\n{err:?}"
            );
        }
    }
}

#[test]
fn wasm_rejects_reached_native_only_forms_and_allows_unreached() {
    let dir = std::env::temp_dir().join(format!("tz-w13-wasm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let bin = env!("CARGO_BIN_EXE_tsuzuri");

    for (idx, source) in [
        "record R { x: i32 }\nextern \"f\" def f :: ref mut R -> i32\nexport def run :: i32 -> i32\nfn run v = { let mut r = R { x: v }; f (ref mut r) }\n",
        "extern \"f\" def f :: ref [i16u] -> i32\nexport def run :: i32 -> i32\nfn run _v = f (ref [1i16u])\n",
        "@count(1, i32)\nextern \"f\" def f :: ref [i64] -> i32\nexport def run :: i32 -> i32\nfn run _v = f (ref [1])\n",
    ]
    .into_iter()
    .enumerate()
    {
        let module = analyze(source).unwrap();
        let err = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap_err();
        assert_eq!(err.code, "E1008", "{err:?}");
        assert_eq!(err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));

        let src_path = dir.join(format!("Case{idx}.tz"));
        fs::write(&src_path, source).unwrap();
        let out_js = dir.join(format!("out{idx}.mjs"));
        let bind_out = Command::new(bin)
            .args([
                "build",
                src_path.to_str().unwrap(),
                "--target",
                "wasm32",
                "--emit",
                "bindings-js",
                "--no-cache",
                "-o",
                out_js.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!bind_out.status.success());
        assert!(String::from_utf8_lossy(&bind_out.stderr).contains("E1008"));
    }

    let unreached = "record R { x: i32 }\n@count(2, i32)\nextern \"unused\" def unused :: ref mut R -> ref mut [i16u..] -> i32\nexport def run :: i32 -> i32\nfn run x = x + 1i32\n";
    let module = analyze(unreached).unwrap();
    llvm::emit_target(&module, llvm::Entry::Library, true)
        .expect("unreached native-only extern is ignored on WASM");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn header_prototypes_use_exact_mutable_and_count_types() {
    let source = r#"
extern type copy Hdc
record Rect { left: i32, top: i32, right: i32, bottom: i32 }

@count(2, i32)
@count(3, none)
extern def host_fill :: ref mut Rect -> ref mut [i16u..] -> ref [i32u] -> ref mut i16u -> ref mut bool -> ref mut Hdc -> i32

export def keep :: i32 -> i32
fn keep x = x
"#;
    let module = analyze(source).unwrap();
    let header = llvm::header(&module);
    assert!(
        header.contains(
            "int32_t tsuzuri_host_Main_host_fill(tz_record_4Main_4Rect *arg0, uint16_t *arg1_ptr, int32_t arg1_len, const uint32_t *arg2_ptr, uint16_t *arg3, int32_t *arg4, tz_handle_4Main_3Hdc *arg5);"
        ),
        "{header}"
    );
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(
        ir.contains("icmp ne ptr") && ir.contains("null"),
        "ref mut CopyHandle copy-out must guard against NULL:\n{ir}"
    );
}
