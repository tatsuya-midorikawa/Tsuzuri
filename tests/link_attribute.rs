//! W17: per-`extern` `@link("name")` system libraries and Windows `dllimport`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::driver::LinkInputs;
use tsuzuri::syntax::SourceKind;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, formatter, llvm};

fn check_err(source: &str) -> tsuzuri::diagnostic::Diagnostic {
    analyze(source).expect_err(source)
}

#[test]
fn link_attribute_parses_and_checks_names() {
    let ok = "\
@link(\"user32\")
extern \"MessageBeep\" def beep :: i32u -> i32

@trusted @link(\"gdi32\")
extern \"GdiFlush\" def flush_one :: unit -> bool

@link(\"gdi32\")
@trusted
extern \"env\" \"GdiSetBatchLimit\" def set_batch :: i32u -> i32u

def main :: unit -> i32 = \\() -> 0i32
";
    let module = analyze(ok).expect("valid @link declarations must pass check");
    assert_eq!(
        module
            .link_libraries
            .get("MessageBeep")
            .map(|s| s.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["user32"])
    );
    assert_eq!(
        module
            .link_libraries
            .get("GdiFlush")
            .map(|s| s.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["gdi32"])
    );
    assert_eq!(
        module
            .link_libraries
            .get("GdiSetBatchLimit")
            .map(|s| s.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["gdi32"])
    );
    // Formatter preserves `@link(...)` byte-for-byte.
    let formatted = formatter::format_source("Main.tz", ok, SourceKind::Code)
        .expect("format ok")
        .formatted;
    assert_eq!(formatted, ok);

    // M22 interaction: `@link` on the line right after a bodyless extern starts the next declaration.
    let consecutive = "\
extern \"abs\" def c_abs :: i32 -> i32
@link(\"gdi32\")
extern \"GdiFlush\" def gdi_flush :: unit -> bool
def main :: unit -> i32 = \\() -> c_abs 0i32
";
    assert!(analyze(consecutive).is_ok());

    // Syntax errors (`E0002` `link_attribute_syntax`).
    for bad_syntax in [
        "@link\nextern \"f\" def f :: unit -> i32\n0\n",
        "@link()\nextern \"f\" def f :: unit -> i32\n0\n",
        "@link(user32)\nextern \"f\" def f :: unit -> i32\n0\n",
        "@link(\"a\", \"b\")\nextern \"f\" def f :: unit -> i32\n0\n",
    ] {
        let err = check_err(bad_syntax);
        assert_eq!(err.code, "E0002", "{bad_syntax}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::LinkAttributeSyntax.id()),
            "{bad_syntax}: {err:?}"
        );
    }

    // Invalid library names (`E1008` `link_invalid_name`).
    for bad_name in [
        "\"libuser32\"",
        "\"user32.lib\"",
        "\"../x\"",
        "\"C:/x\"",
        "\"\"",
    ] {
        let src = format!("@link({bad_name})\nextern \"f\" def f :: unit -> i32\n0\n");
        let err = check_err(&src);
        assert_eq!(err.code, "E1008", "{src}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::LinkInvalidName.id()),
            "{src}: {err:?}"
        );
    }

    // Misplaced `@link` (`E0002` `misplaced_link_attribute`).
    for misplaced in [
        "@link(\"m\")\ndef f :: i32 -> i32 = \\x -> x\n0\n",
        "@link(\"m\")\nexport def f :: i32 -> i32 = \\x -> x\n",
        "@link(\"m\")\nextern type H\n0\n",
        "record P { x: i32 }\n@link(\"m\")\nextern view def r :: i64 -> P\n0\n",
    ] {
        let err = check_err(misplaced);
        assert_eq!(err.code, "E0002", "{misplaced}: {err:?}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::MisplacedLinkAttribute.id()),
            "{misplaced}: {err:?}"
        );
    }

    // Implicit `tsuzuri_host_` symbol (`E1008` `link_needs_symbol`).
    let no_sym = check_err("@link(\"user32\")\nextern def host_fn :: unit -> i32\n0\n");
    assert_eq!(no_sym.code, "E1008");
    assert_eq!(no_sym.kind, Some(DiagnosticKind::LinkNeedsSymbol.id()));

    // Duplicate `@link` (`E1008` `link_repeated`).
    let dup =
        check_err("@link(\"user32\")\n@link(\"gdi32\")\nextern \"f\" def f :: unit -> i32\n0\n");
    assert_eq!(dup.code, "E1008");
    assert_eq!(dup.kind, Some(DiagnosticKind::LinkRepeated.id()));
}

#[test]
fn dllimport_follows_the_symbol() {
    let source = "\
@link(\"gdi32\")
extern \"GdiFlush\" def flush_a :: unit -> i32
extern \"GdiFlush\" def flush_b :: unit -> bool
extern \"PlainHost\" def plain :: unit -> i32

export def run_all :: i32 -> i32 = \\n ->
    let a = flush_a ()
    let b = if flush_b () then 1i32 else 0i32
    n + a + b + plain ()
";
    let module = analyze(source).unwrap();

    for platform in [
        Platform::new(Some(Triple::X86_64WindowsMsvc)),
        Platform::new(Some(Triple::Aarch64WindowsMsvc)),
        Platform::new(Some(Triple::X86_64WindowsGnu)),
    ] {
        let _guard = PlatformScope::enter(platform);
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
        let gdi_decls: Vec<&str> = ir
            .lines()
            .filter(|line| line.starts_with("declare ") && line.contains("@GdiFlush("))
            .collect();
        assert_eq!(gdi_decls, vec!["declare dllimport i32 @GdiFlush()"], "{ir}");
        assert!(
            ir.lines().any(|line| line == "declare i32 @PlainHost()"),
            "{ir}"
        );
    }

    {
        let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64LinuxGnu)));
        let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
        assert!(
            ir.lines().any(|line| line == "declare i32 @GdiFlush()"),
            "{ir}"
        );
        assert!(!ir.contains("dllimport"), "{ir}");
    }
}

#[test]
fn wasm_rejects_reached_link_externs() {
    let unreached = "\
@link(\"gdi32\")
extern \"GdiFlush\" def gdi_flush :: unit -> bool

export def add_one :: i32 -> i32 = \\x -> x + 1i32
";
    let m_unreached = analyze(unreached).unwrap();
    assert!(llvm::emit_target(&m_unreached, llvm::Entry::Library, true).is_ok());

    let reached = "\
@link(\"gdi32\")
extern \"GdiFlush\" def gdi_flush :: unit -> bool

export def call_flush :: i32 -> i32 = \\x -> if gdi_flush () then x else 0i32
";
    let m_reached = analyze(reached).unwrap();
    let err = llvm::emit_target(&m_reached, llvm::Entry::Library, true).unwrap_err();
    assert_eq!(err.code, "E2000");
    assert_eq!(err.kind, Some(DiagnosticKind::LinkAttributeWasm.id()));
    assert!(err.message.contains("@link(\"gdi32\")"), "{}", err.message);
}

#[test]
fn only_reached_symbols_add_libraries() {
    let source = "\
@link(\"gdi32\")
extern \"GdiFlush\" def gdi_flush :: unit -> bool
@link(\"no_such_library\")
extern \"NeverCalled\" def never_called :: unit -> i32

def main :: unit -> i32 = \\() -> if gdi_flush () then 7i32 else 8i32
";
    let module = analyze(source).unwrap();
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(
        tsuzuri::driver::system_libraries(&module, &ir, &LinkInputs::default()),
        vec!["gdi32".to_owned()]
    );
    // Combining `-l gdi32` with `@link("gdi32")` deduplicates without error.
    let explicit = LinkInputs {
        libraries: vec!["gdi32".into()],
        ..LinkInputs::default()
    };
    explicit.check_shape().unwrap();
    assert!(tsuzuri::driver::system_libraries(&module, &ir, &explicit).is_empty());
}

#[test]
fn link_attribute_keeps_the_build_cache() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-w17-cache-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let src = root.join("Main.tz");
    fs::write(
        &src,
        "@trusted @link(\"m\")\nextern \"cos\" def c_cos :: f64 -> f64\ndef main :: unit -> i32 = \\() -> if c_cos 0.0 > 0.5 then 0i32 else 1i32\n",
    )
    .unwrap();
    let cache_dir = root.join("cache");
    let log_file = root.join("clang.log");

    // Build a tiny cross-platform fake clang binary using rustc so it runs identically on Unix and Windows.
    let fake_src = root.join("fake_clang.rs");
    let fake_bin = root.join(if cfg!(windows) {
        "fake-clang.exe"
    } else {
        "fake-clang"
    });
    fs::write(
        &fake_src,
        r#"
use std::io::Write;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("clang version 22.0.0-fake");
        return;
    }
    if let Ok(log) = std::env::var("FAKE_CLANG_LOG") {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();
        writeln!(f, "clang {}", args.join(" ")).unwrap();
    }
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        if let Some(out) = args.get(pos + 1) {
            std::fs::write(out, b"fake-binary").unwrap();
        }
    }
}
"#,
    )
    .unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let compiled = Command::new(rustc)
        .arg(&fake_src)
        .arg("-o")
        .arg(&fake_bin)
        .output()
        .expect("compile fake clang with rustc");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let out_exe = root.join(if cfg!(windows) { "app.exe" } else { "app" });
    let run_build = || {
        let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                src.to_str().unwrap(),
                "-o",
                out_exe.to_str().unwrap(),
            ])
            .env("TSUZURI_CLANG", &fake_bin)
            .env("TSUZURI_CACHE_DIR", &cache_dir)
            .env("FAKE_CLANG_LOG", &log_file)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        fs::read_to_string(&log_file)
            .unwrap_or_default()
            .lines()
            .count()
    };

    let first_calls = run_build();
    assert!(first_calls >= 1);
    let log_text = fs::read_to_string(&log_file).unwrap();
    assert!(log_text.contains("-lm"), "expected -lm in: {log_text}");
    // Second build must hit the build cache (0 additional clang invocations).
    let second_calls = run_build();
    assert_eq!(second_calls, first_calls, "expected cache hit:\n{log_text}");

    let _ = fs::remove_dir_all(root);
}
