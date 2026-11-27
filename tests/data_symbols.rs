//! M06 Phase 1: C global data symbol `extern "sym" view def` and `extern "sym" address def`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::syntax::{Edition, SourceKind};
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, analyze_modules, analyze_with_edition, formatter, llvm};

fn check_err(source: &str) -> tsuzuri::diagnostic::Diagnostic {
    analyze(source).expect_err(source)
}

#[test]
fn parses_view_and_address_forms() {
    let source = "\
extern type copy Queue
extern type copy CFString

@trusted
extern \"NSDefaultRunLoopMode\" view def default_run_loop_mode :: unit -> CFString

@trusted
private extern \"_dispatch_main_q\" address def main_queue :: unit -> Queue

@trusted
extern \"c_addr_fn\" def address :: unit -> i64

def main :: unit -> i32 = \\() ->
    let address = 1i32
    address - 1i32
";
    for edition in [Edition::E2026, Edition::E2027, Edition::E2028] {
        analyze_with_edition(source, edition)
            .unwrap_or_else(|e| panic!("edition {edition:?}: {e:?}"));
    }
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .expect("format ok")
        .formatted;
    assert_eq!(formatted, source);
}

#[test]
fn syntax_rejections() {
    let mod_view = check_err("extern \"m\" \"s\" view def f :: unit -> i64\n0\n");
    assert_eq!(mod_view.code, "E0002");
    assert_eq!(
        mod_view.kind,
        Some(DiagnosticKind::DataSymbolModuleName.id())
    );

    let mod_addr =
        check_err("extern type copy Q\nextern \"m\" \"s\" address def f :: unit -> Q\n0\n");
    assert_eq!(mod_addr.code, "E0002");
    assert_eq!(
        mod_addr.kind,
        Some(DiagnosticKind::DataSymbolModuleName.id())
    );

    let no_link = check_err("extern type copy Q\nextern address def f :: unit -> Q\n0\n");
    assert_eq!(no_link.code, "E0002");
    assert_eq!(
        no_link.kind,
        Some(DiagnosticKind::DataSymbolNeedsLinkName.id())
    );

    for bad_view in [
        "record P { x: i32 }\nextern \"x\" view def r :: i64 -> P\n0\n",
        "record P { x: i32 }\nextern \"x\" view def w :: i64 -> ref P -> unit\n0\n",
    ] {
        let err = check_err(bad_view);
        assert_eq!(err.code, "E0002");
        assert_eq!(err.kind, Some(DiagnosticKind::ExternViewLinkName.id()));
        assert!(
            err.message.contains("an extern view that takes an address")
                && err.message.contains("read the data symbol 'x'"),
            "{}",
            err.message
        );
    }

    let unit_no_link = check_err("extern view def f :: unit -> i64\n0\n");
    assert_eq!(unit_no_link.code, "E1008");
    assert_eq!(
        unit_no_link.kind,
        Some(DiagnosticKind::ExternViewSignature.id())
    );
}

#[test]
fn types_and_conflicts() {
    let valid = "\
extern type copy H
@c_layout
record Pair { a: i8, b: i64 }

@trusted extern \"v_i8\" view def v_i8 :: unit -> i8
@trusted extern \"v_u16\" view def v_u16 :: unit -> i16u
@trusted extern \"v_f32\" view def v_f32 :: unit -> f32
@trusted extern \"v_f64\" view def v_f64 :: unit -> f64
@trusted extern \"v_h\" view def v_h :: unit -> H
@trusted extern \"v_mh\" view def v_mh :: unit -> Maybe<H>
@trusted extern \"v_rec\" view def v_rec :: unit -> Pair
@trusted extern \"v_h\" view def v_h_again :: unit -> H
@trusted extern \"v_h\" address def addr_h :: unit -> H
@trusted extern \"v_h\" address def addr_ea :: unit -> ExternAddr

def main :: unit -> i32 = \\() -> 0i32
";
    analyze(valid).expect("all allowed data symbol types and coexisting read+address must check");

    for bad_sig in [
        "extern \"b\" view def f :: unit -> bool\n0\n",
        "extern type Owned\nextern \"o\" view def f :: unit -> Owned\n0\n",
        "extern type local Loc\nextern \"l\" view def f :: unit -> Loc\n0\n",
        "extern type Owned\nextern \"o\" view def f :: unit -> Maybe<Owned>\n0\n",
        "extern \"s\" view def f :: unit -> string\n0\n",
        "extern \"u\" view def f :: unit -> unit\n0\n",
        "extern \"z\" view def f :: unit -> ZString\n0\n",
        "extern \"zu\" view def f :: unit -> ZUtf8String\n0\n",
        "extern \"ea\" view def f :: unit -> ExternAddr\n0\n",
        "extern \"a\" address def f :: unit -> i64\n0\n",
        "extern type copy H\nextern \"a\" address def f :: unit -> Maybe<H>\n0\n",
        "extern type Owned\nextern \"a\" address def f :: unit -> Owned\n0\n",
        "extern type copy H\nextern \"a\" address def f :: i64 -> H\n0\n",
    ] {
        let err = check_err(bad_sig);
        assert_eq!(err.code, "E1008", "{bad_sig}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::DataSymbolSignature.id()),
            "{bad_sig}: {err:?}"
        );
    }

    for conflict in [
        "extern \"sym\" view def a :: unit -> i32\nextern \"sym\" view def b :: unit -> i64\n0\n",
        "extern \"sym\" def a :: unit -> i32\nextern \"sym\" view def b :: unit -> i32\n0\n",
        "extern \"sym\" view def a :: unit -> i32\nextern \"sym\" def b :: unit -> i32\n0\n",
        "extern type copy H\nextern \"sym\" address def a :: unit -> H\nextern \"sym\" def b :: unit -> i32\n0\n",
    ] {
        let err = check_err(conflict);
        assert_eq!(err.code, "E1008", "{conflict}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::ConflictingExternDeclaration.id()),
            "{conflict}: {err:?}"
        );
    }

    // Function value / partial application is rejected with `extern_view_call_form`.
    let fn_val = check_err("@trusted extern \"sym\" view def v :: unit -> i32\nlet f = v\nf ()\n");
    assert_eq!(fn_val.code, "E1008");
    assert_eq!(fn_val.kind, Some(DiagnosticKind::ExternViewCallForm.id()));
}

#[test]
fn lowers_to_one_external_global() {
    let mod_a = "\
extern type copy H
@trusted extern \"g_byte\" view def read_byte :: unit -> i8
@trusted extern \"g_short\" view def read_short :: unit -> i16u
@trusted extern \"g_handle\" view def read_handle :: unit -> H
@trusted extern \"g_maybe\" view def read_maybe :: unit -> Maybe<H>
@trusted extern \"g_queue\" address def queue_addr :: unit -> H
@trusted extern \"use_h\" def use_h :: H -> i32

def from_a :: unit -> i32 = \\() ->
    let b = read_byte () as i32
    let s = read_short () as i32
    let h = use_h (read_handle ())
    let m = match read_maybe () with
        | Maybe.Some x -> use_h x
        | Maybe.None -> 0i32
    let q = use_h (queue_addr ())
    b + s + h + m + q
";
    let mod_b = "\
@trusted extern \"g_byte\" view def read_byte_b :: unit -> i8

export def run :: i32 = A.from_a () + (read_byte_b () as i32)
";
    let module = analyze_modules(&[("A.tz", mod_a), ("Main.tz", mod_b)]).expect("modules analyze");
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64LinuxGnu)));
    let ir = llvm::emit(&module, llvm::Entry::Library).expect("emit on linux");
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());

    let byte_globals: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("@g_byte ="))
        .collect();
    assert_eq!(
        byte_globals,
        vec!["@g_byte = external global [0 x i8]"],
        "{ir}"
    );
    assert!(
        ir.contains("load i8, ptr @g_byte, align 1"),
        "expected exact 8-bit load:\n{ir}"
    );
    assert!(
        ir.contains("load i16, ptr @g_short, align 2"),
        "expected exact 16-bit load:\n{ir}"
    );
    assert!(
        ir.contains("load ptr, ptr @g_handle, align 8"),
        "expected handle load:\n{ir}"
    );
    assert!(
        ir.contains("ptr @g_queue"),
        "expected direct global address:\n{ir}"
    );
    assert!(
        !ir.contains("inttoptr"),
        "data symbols must not pass through host_pointer inttoptr:\n{ir}"
    );
}

#[test]
fn generated_declare_conflict() {
    // Runtime C symbols in RESERVED_HOST_SYMBOLS are rejected up front.
    for reserved in ["write", "malloc", "putchar"] {
        let src = format!("@trusted\nextern \"{reserved}\" view def r :: unit -> i64\n0\n");
        let err = check_err(&src);
        assert_eq!(err.code, "E1008");
        assert_eq!(err.kind, Some(DiagnosticKind::ReservedExternSymbol.id()));
    }
    // Any non-reserved runtime `declare` (such as M01 `@objc_retain`) colliding with a reached data symbol
    // is caught by `llvm::check_data_symbol_declares` with `conflicting_extern_declaration`.
    let mut map = std::collections::BTreeMap::new();
    map.insert(
        "objc_retain".to_owned(),
        tsuzuri::diagnostic::Span::default(),
    );
    let err = llvm::check_data_symbol_declares(
        "declare ptr @objc_retain(ptr)\n@objc_retain = external global [0 x i8]\n",
        &map,
    )
    .expect_err("must reject declare + global collision");
    assert_eq!(err.code, "E1008");
    assert_eq!(
        err.kind,
        Some(DiagnosticKind::ConflictingExternDeclaration.id())
    );
}

#[test]
fn targets_reject() {
    let unreached = "\
@trusted
extern \"g_val\" view def read_val :: unit -> i64

export def ok :: i32 -> i32 = \\x -> x + 1i32
";
    let reached = "\
@trusted
extern \"g_val\" view def read_val :: unit -> i64

export def bad :: i64 = read_val ()
";
    let m_unreached = analyze(unreached).unwrap();
    let m_reached = analyze(reached).unwrap();

    // Unreached succeeds on Windows and WASM.
    {
        let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
        assert!(llvm::emit(&m_unreached, llvm::Entry::Library).is_ok());
        let win_err = llvm::emit(&m_reached, llvm::Entry::Library).unwrap_err();
        assert_eq!(win_err.code, "E1008");
        assert_eq!(win_err.kind, Some(DiagnosticKind::DataSymbolTarget.id()));
    }
    assert!(llvm::emit_target(&m_unreached, llvm::Entry::Library, true).is_ok());
    let wasm_err = llvm::emit_target(&m_reached, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(wasm_err.code, "E1008");
    assert_eq!(wasm_err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
}

#[test]
fn effects_and_trust() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-m06-warn-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let src_file = root.join("Main.tz");
    fs::write(
        &src_file,
        "extern \"g_val\" view def read_val :: unit -> i64\ndef main :: unit -> i32 = \\() -> read_val () as i32\n",
    )
    .unwrap();
    let warn_out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args([
            "check",
            "--warn",
            "untrusted-extern",
            src_file.to_str().unwrap(),
        ])
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    assert!(warn_out.status.success());
    let stderr = String::from_utf8_lossy(&warn_out.stderr);
    assert!(stderr.contains("W1010"), "{stderr}");
    let _ = fs::remove_dir_all(root);

    let bad_prop =
        check_err("@trusted pure\nextern \"g_val\" view def read_val :: unit -> i64\n0\n");
    assert_eq!(bad_prop.code, "E1037");
    assert_eq!(
        bad_prop.kind,
        Some(DiagnosticKind::TrustedViewProperty.id())
    );

    let bad_count =
        check_err("@count(1, none)\nextern \"g_val\" view def read_val :: unit -> i64\n0\n");
    assert_eq!(bad_count.code, "E0002");
    assert_eq!(
        bad_count.kind,
        Some(DiagnosticKind::MisplacedCountAttribute.id())
    );

    let no_alloc_ok = "\
@trusted
extern \"g_val\" view def read_val :: unit -> i64

@no_alloc @no_copy @terminates
def caller :: unit -> i64 = \\() -> read_val ()

def main :: unit -> i32 = \\() -> caller () as i32
";
    analyze(no_alloc_ok).expect("@no_alloc/@no_copy/@terminates accept data symbol");

    let no_trap_bad = check_err(
        "@trusted\nextern \"g_val\" view def read_val :: unit -> i64\n@no_trap\ndef caller :: unit -> i64 = \\() -> read_val ()\n0\n",
    );
    assert_eq!(no_trap_bad.code, "E1031");
}

#[test]
fn audit_rows() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-m06-audit-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Main.tz"),
        "\
extern type copy Queue
@trusted
extern \"NSDefaultRunLoopMode\" view def mode :: unit -> i64
@trusted
extern \"_dispatch_main_q\" address def main_q :: unit -> Queue
@trusted
extern \"puts\" def c_puts :: i64 -> i32

def main :: unit -> i32 = \\() ->
    let _q = main_q ()
    c_puts (mode ())
",
    )
    .unwrap();

    let text_out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["audit", root.to_str().unwrap()])
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    assert!(text_out.status.success());
    let text = String::from_utf8_lossy(&text_out.stdout);
    assert!(
        text.contains("mode -> \"NSDefaultRunLoopMode\" data read  Main.tz:3:40  trusted"),
        "{text}"
    );
    assert!(
        text.contains("main_q -> \"_dispatch_main_q\" data address  Main.tz:5:39  trusted"),
        "{text}"
    );
    assert!(
        text.contains("c_puts -> \"puts\"  Main.tz:7:19  trusted"),
        "{text}"
    );

    let json_out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["audit", "--json", root.to_str().unwrap()])
        .env("TSUZURI_CACHE_DIR", "")
        .output()
        .unwrap();
    assert!(json_out.status.success());
    let json = String::from_utf8_lossy(&json_out.stdout);
    assert!(
        json.contains("\"name\":\"mode\",\"symbol\":\"NSDefaultRunLoopMode\",\"data\":\"read\""),
        "{json}"
    );
    assert!(
        json.contains("\"name\":\"main_q\",\"symbol\":\"_dispatch_main_q\",\"data\":\"address\""),
        "{json}"
    );
    let puts_line = json
        .lines()
        .find(|line| line.contains("\"name\":\"c_puts\""))
        .unwrap();
    assert!(!puts_line.contains("\"data\""), "{puts_line}");

    let _ = fs::remove_dir_all(root);
}
