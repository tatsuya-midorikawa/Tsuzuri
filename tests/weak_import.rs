//! M17: `@weak` weak imports on `extern "sym" def` and `M06` C data symbols.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::syntax::SourceKind;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, formatter, llvm};

fn check_err(source: &str) -> tsuzuri::diagnostic::Diagnostic {
    analyze(source).expect_err(source)
}

#[test]
fn weak_attribute_positions() {
    let ok = "\
extern type copy CFString

@trusted @weak
extern \"m17_new_api\" def new_api :: unit -> i32

@weak @trusted @link(framework = \"Foundation\")
extern \"NSDefaultRunLoopMode\" view def mode :: unit -> Maybe<CFString>

@trusted
extern \"abs\" def c_abs :: i32 -> i32
@weak @trusted
extern \"m17_after_bodyless\" def after_bodyless :: unit -> i32

def main :: unit -> i32 = \\() -> c_abs 0i32
";
    let module = analyze(ok).expect("valid @weak declarations must check");
    assert!(module.weak_symbols.contains("m17_new_api"));
    assert!(module.weak_symbols.contains("NSDefaultRunLoopMode"));
    assert!(module.weak_symbols.contains("m17_after_bodyless"));

    let formatted = formatter::format_source("Main.tz", ok, SourceKind::Code)
        .expect("format ok")
        .formatted;
    assert_eq!(formatted, ok);

    let bad_arg = check_err("@weak(\"x\")\nextern \"f\" def f :: unit -> i32\n0\n");
    assert_eq!(bad_arg.code, "E0002");
    assert_eq!(bad_arg.kind, Some(DiagnosticKind::WeakAttributeSyntax.id()));

    for misplaced in [
        "@weak\ndef f :: i32 -> i32 = \\x -> x\n0\n",
        "@weak\nexport def f :: i32 -> i32 = \\x -> x\n",
        "@weak\nextern type H\n0\n",
        "record P { x: i32 }\n@weak\nextern view def r :: i64 -> P\n0\n",
        "@weak @weak\nextern \"f\" def f :: unit -> i32\n0\n",
    ] {
        let err = check_err(misplaced);
        assert_eq!(err.code, "E0002", "{misplaced}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::MisplacedWeakAttribute.id()),
            "{misplaced}: {err:?}"
        );
    }

    let no_sym = check_err("@weak\nextern def host_fn :: unit -> i32\n0\n");
    assert_eq!(no_sym.code, "E1008");
    assert_eq!(no_sym.kind, Some(DiagnosticKind::WeakNeedsSymbol.id()));

    for non_maybe_data in [
        "@weak\nextern \"g\" view def f :: unit -> i64\n0\n",
        "extern type copy Q\n@weak\nextern \"g\" address def f :: unit -> Q\n0\n",
    ] {
        let err = check_err(non_maybe_data);
        assert_eq!(err.code, "E1008", "{non_maybe_data}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::WeakDataSymbolResult.id()),
            "{non_maybe_data}: {err:?}"
        );
    }
}

#[test]
fn weak_extern_lowers_to_extern_weak_and_is_per_symbol() {
    let source = "\
@trusted @weak
extern \"m17_new_api\" def api_a :: unit -> i32

@trusted
extern \"m17_new_api\" def api_b :: unit -> i32

export def call_both :: i32 = api_a () + api_b ()
";
    let module = analyze(source).unwrap();
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());

    let decls: Vec<&str> = ir
        .lines()
        .filter(|line| line.starts_with("declare ") && line.contains("@m17_new_api("))
        .collect();
    assert_eq!(
        decls,
        vec!["declare extern_weak i32 @m17_new_api()"],
        "{ir}"
    );
    assert_eq!(
        ir.matches("icmp ne ptr @m17_new_api, null").count(),
        2,
        "both api_a and api_b must null-check @m17_new_api:\n{ir}"
    );
    assert_eq!(
        ir.matches("define internal void @tz.weak.missing(").count(),
        1,
        "{ir}"
    );
    assert_eq!(ir.matches("@tz.weak.name.m17_new_api =").count(), 1, "{ir}");
}

#[test]
fn weak_data_symbol_is_maybe_and_non_trapping() {
    let source = "\
extern type copy Queue
extern type copy CFString

@trusted @weak
extern \"m17_ver\" view def read_ver :: unit -> Maybe<f64>

@trusted @weak
extern \"m17_str\" view def read_str :: unit -> Maybe<CFString>

@trusted @weak
extern \"m17_q\" address def queue_addr :: unit -> Maybe<Queue>

@no_alloc @no_copy @terminates
def probe :: unit -> i32 = \\() ->
    let a = match read_ver () with | Maybe.Some _ -> 1i32 | Maybe.None -> 0i32
    let b = match read_str () with | Maybe.Some _ -> 2i32 | Maybe.None -> 0i32
    let c = match queue_addr () with | Maybe.Some _ -> 4i32 | Maybe.None -> 0i32
    a + b + c

export def run :: i32 = probe ()
";
    let module = analyze(source).expect("@no_alloc/@no_copy/@terminates accept @weak data symbols");
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::Aarch64AppleDarwin)));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(!ir.contains("@tz.weak.missing"), "{ir}");
    assert!(
        ir.contains("@m17_ver = extern_weak global [0 x i8]"),
        "{ir}"
    );
    assert!(
        ir.contains("@m17_str = extern_weak global [0 x i8]"),
        "{ir}"
    );
    assert!(ir.contains("@m17_q = extern_weak global [0 x i8]"), "{ir}");
    assert!(ir.contains("icmp ne ptr @m17_ver, null"), "{ir}");
}

#[test]
fn non_apple_targets_reject_and_unreached_succeeds() {
    let unreached = "\
@trusted @weak
extern \"m17_new_api\" def new_api :: unit -> i32
export def ok :: i32 -> i32 = \\x -> x + 1i32
";
    let reached = "\
@trusted @weak
extern \"m17_new_api\" def new_api :: unit -> i32
export def bad :: i32 = new_api ()
";
    let m_unreached = analyze(unreached).unwrap();
    let m_reached = analyze(reached).unwrap();

    for triple in [Triple::X86_64LinuxGnu, Triple::X86_64WindowsMsvc] {
        let _guard = PlatformScope::enter(Platform::new(Some(triple)));
        assert!(llvm::emit(&m_unreached, llvm::Entry::Library).is_ok());
        let err = llvm::emit(&m_reached, llvm::Entry::Library).unwrap_err();
        assert_eq!(err.code, "E1008");
        assert_eq!(err.kind, Some(DiagnosticKind::AbiAppleOnly.id()));
    }
    assert!(llvm::emit_target(&m_unreached, llvm::Entry::Library, true).is_ok());
    let wasm_err = llvm::emit_target(&m_reached, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(wasm_err.code, "E1008");
    assert_eq!(wasm_err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
}

#[test]
fn weak_effects_trust_and_audit() {
    let conflict =
        check_err("@trusted no_trap @weak\nextern \"m17_new_api\" def new_api :: unit -> i32\n0\n");
    assert_eq!(conflict.code, "E1037");
    assert_eq!(conflict.kind, Some(DiagnosticKind::TrustedNoTrapWeak.id()));

    let no_trap_caller = check_err(
        "@trusted @weak\nextern \"m17_new_api\" def new_api :: unit -> i32\n@no_trap\ndef f :: unit -> i32 = \\() -> new_api ()\n0\n",
    );
    assert_eq!(no_trap_caller.code, "E1031");

    // Weak scalar/nullable-handle data imports are non-trapping (`Maybe.None` when absent),
    // whereas a `@c_layout` record containing a non-nullable handle can still trap on a null field.
    analyze(
        "extern type copy H\n@c_layout\nrecord Safe { x: i64, h: Maybe<H> }\n@trusted @weak\nextern \"g_safe\" view def read_safe :: unit -> Maybe<Safe>\n@no_trap\ndef ok_read :: unit -> Maybe<Safe> = \\() -> read_safe ()\n0\n",
    )
    .expect("weak data read without non-nullable handle field does not trap");

    let weak_record_handle_trap = check_err(
        "extern type copy H\n@c_layout\nrecord Trapping { h: H }\n@trusted @weak\nextern \"g_trap\" view def read_trapping :: unit -> Maybe<Trapping>\n@no_trap\ndef bad_read :: unit -> Maybe<Trapping> = \\() -> read_trapping ()\n0\n",
    );
    assert_eq!(weak_record_handle_trap.code, "E1031");

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-m17-audit-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("Main.tz"),
        "@trusted @weak\nextern \"m17_new_api\" def new_api :: unit -> i32\n@trusted\nextern \"abs\" def c_abs :: i32 -> i32\ndef main :: unit -> i32 = \\() -> new_api () + c_abs 1i32\n",
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
        text.contains("new_api -> \"m17_new_api\"  Main.tz:2:26  trusted  weak\n"),
        "{text}"
    );
    assert!(
        text.contains("c_abs -> \"abs\"  Main.tz:4:18  trusted\n"),
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
        json.contains("\"name\":\"new_api\"") && json.contains("\"weak\":true"),
        "{json}"
    );
    let abs_line = json
        .lines()
        .find(|l| l.contains("\"name\":\"c_abs\""))
        .unwrap();
    assert!(!abs_line.contains("\"weak\""), "{abs_line}");

    let _ = fs::remove_dir_all(root);
}
