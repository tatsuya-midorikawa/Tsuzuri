//! W16: `@last_error` on `extern ... def` declarations (`GetLastError` / `errno` capture).

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::syntax::SourceKind;
use tsuzuri::target::{Platform, PlatformScope, Triple};
use tsuzuri::{analyze, formatter, llvm};

fn expect_error(source: &str, code: &str, kind: DiagnosticKind) {
    let err =
        analyze(source).expect_err(&format!("expected {code} ({}) for:\n{source}", kind.id()));
    assert_eq!(err.code, code, "unexpected code for:\n{source}\n{err:?}");
    assert_eq!(
        err.kind,
        Some(kind.id()),
        "unexpected kind for:\n{source}\n{err:?}"
    );
}

#[test]
fn last_error_needs_the_pair_result() {
    let accepted = r#"
extern type Handle
extern type copy CopyHandle
record Pair { a: i64, b: i32 }

@last_error
extern "CloseHandle" def close_bool :: i64 -> bool * i32u

@last_error
extern "FlushBuffers" def flush_unit :: i64 -> unit * i32u

@last_error
extern "ReadCount" def read_i64 :: i64 -> i64 * i32u

@last_error
extern def read_text :: i64 -> string * i32u

@last_error
extern def read_pair :: i64 -> Pair * i32u

@last_error
extern "OpenItem" def open_handle :: i64 -> Handle * i32u

@last_error
extern "TryOpenItem" def try_open_handle :: i64 -> Maybe<CopyHandle> * i32u

def check_header :: i64 -> i64 = \x ->
    let (s, e1) = read_text x
    let (p, e2) = read_pair x
    String.length (ref s) + p.a + (e1 as i64) + (e2 as i64)
"#;
    let module = analyze(accepted).expect("valid @last_error result shapes should check");
    let header = llvm::header(&module);
    assert!(
        header.contains(
            "void tsuzuri_host_Main_read_text(tsuzuri_string_buffer *out, int64_t arg0);"
        ),
        "owned-buffer @last_error prototype should pass out buffer without tuple:\n{header}"
    );
    assert!(
        header.contains(
            "void tsuzuri_host_Main_read_pair(tz_record_4Main_4Pair *out, int64_t arg0);"
        ),
        "record @last_error prototype should pass out struct without tuple:\n{header}"
    );

    for bad in [
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> bool\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> bool * i32\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> i32u * bool\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> bool * i32u * i32u\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> ref [i64] * i32u\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> [i32] * i32u\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> Array<string> * i32u\n",
        "@last_error\nextern \"CloseHandle\" def f :: i64 -> (i64 * i64) * i32u\n",
    ] {
        expect_error(bad, "E1008", DiagnosticKind::LastErrorResultShape);
    }
}

#[test]
fn last_error_attribute_errors() {
    expect_error(
        "@last_error\n@last_error\nextern \"CloseHandle\" def f :: i64 -> bool * i32u\n",
        "E1008",
        DiagnosticKind::LastErrorRepeated,
    );
    expect_error(
        "@last_error\ndef f :: i64 -> i64 = \\x -> x\n",
        "E0002",
        DiagnosticKind::MisplacedLastErrorAttribute,
    );
    expect_error(
        "@last_error\nexport def f :: i64 -> i64 = \\x -> x\n",
        "E0002",
        DiagnosticKind::MisplacedLastErrorAttribute,
    );
    expect_error(
        "@last_error\nextern type Handle\n",
        "E0002",
        DiagnosticKind::MisplacedLastErrorAttribute,
    );
    expect_error(
        "record Point { x: i32, y: i32 }\n@last_error\nextern view def read_pt :: i64 -> Point\n",
        "E0002",
        DiagnosticKind::MisplacedLastErrorAttribute,
    );
    expect_error(
        "@last_error\nextern \"errno_val\" view def read_val :: unit -> i32\n",
        "E0002",
        DiagnosticKind::MisplacedLastErrorAttribute,
    );
    expect_error(
        "@last_error(1)\nextern \"CloseHandle\" def f :: i64 -> bool * i32u\n",
        "E0002",
        DiagnosticKind::LastErrorArguments,
    );
    expect_error(
        "@trusted pure\n@last_error\nextern \"CloseHandle\" def f :: i64 -> bool * i32u\n",
        "E1037",
        DiagnosticKind::TrustedPureLastError,
    );
    expect_error(
        "@last_error @trusted no_alloc pure\nextern \"CloseHandle\" def f :: i64 -> bool * i32u\n",
        "E1037",
        DiagnosticKind::TrustedPureLastError,
    );

    // Inline and multi-line `@trusted` + `@last_error` both parse, and a bodyless extern followed
    // by `@last_error` on the next declaration does not absorb `@last_error` into its signature.
    let valid = r#"@trusted
extern "GetCurrentProcessId" def raw_pid :: unit -> i32u

@trusted @last_error
extern "CloseHandle" def close1 :: i64 -> bool * i32u

@trusted no_alloc terminates
@last_error
extern "CloseHandle" def close2 :: i64 -> bool * i32u

def use_both :: i64 -> i32u = \h ->
    let (_, e1) = close1 h
    let (_, e2) = close2 h
    raw_pid () + e1 + e2
"#;
    analyze(valid).expect("valid @trusted + @last_error combinations should check");
    let formatted = formatter::format_source("Main.tz", valid, SourceKind::Code)
        .expect("format @last_error")
        .formatted;
    assert!(
        formatted.contains("@last_error"),
        "formatter must preserve @last_error:\n{formatted}"
    );
    let reformatted = formatter::format_source("Main.tz", &formatted, SourceKind::Code)
        .expect("idempotent format")
        .formatted;
    assert_eq!(formatted, reformatted);
}

#[test]
fn same_symbol_with_and_without_last_error() {
    let source = r#"
extern "CloseHandle" def close_plain :: i64 -> bool

@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u

export def run :: i64 -> i32u = \h ->
    let _ = close_plain h
    let (_, err) = close_checked h
    err
"#;
    let module = analyze(source).expect("same C symbol with and without @last_error should check");
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
    let ir = llvm::emit(&module, llvm::Entry::Library).expect("emit native IR");
    let declares = ir
        .lines()
        .filter(|line| line.starts_with("declare ") && line.contains("@CloseHandle("))
        .count();
    assert_eq!(
        declares, 1,
        "expected exactly one @CloseHandle declare:\n{ir}"
    );
}

#[test]
fn helper_declarations_must_lower_to_32_bits() {
    expect_error(
        r#"
extern "GetLastError" def gle :: unit -> i64
@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u
"#,
        "E1008",
        DiagnosticKind::LastErrorHelperConflict,
    );
    expect_error(
        r#"
extern "SetLastError" def sle :: i64 -> unit
@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u
"#,
        "E1008",
        DiagnosticKind::LastErrorHelperConflict,
    );

    let ok = r#"
extern "GetLastError" def gle :: unit -> i32u
extern "SetLastError" def sle :: i32u -> unit

@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u

export def run :: i64 -> i32u = \h ->
    sle 123i32u
    let (_, err) = close_checked h
    err + gle ()
"#;
    let module =
        analyze(ok).expect("32-bit GetLastError/SetLastError should coexist with @last_error");
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
    let ir = llvm::emit(&module, llvm::Entry::Library).expect("emit Windows IR");
    let gle_declares = ir
        .lines()
        .filter(|line| *line == "declare i32 @GetLastError()")
        .count();
    let sle_declares = ir
        .lines()
        .filter(|line| *line == "declare void @SetLastError(i32)")
        .count();
    assert_eq!(gle_declares, 1, "expected 1 GetLastError declare:\n{ir}");
    assert_eq!(sle_declares, 1, "expected 1 SetLastError declare:\n{ir}");

    // POSIX / Apple errno helpers (`__errno_location` and `__error`):
    // user declarations starting with `__` are rejected with `ReservedExternSymbol` (`E1008`),
    // and if `module.extern_abis` carries an `i64 ()` entry for the helper symbol, `llvm::emit`
    // reconciles the helper call with `inttoptr` and emits a single `declare`.
    for reserved_errno in [
        "extern \"__errno_location\" def bad :: unit -> i64\n@last_error\nextern \"close\" def c :: i32 -> i32 * i32u\n",
        "extern \"__error\" def bad :: unit -> i64\n@last_error\nextern \"close\" def c :: i32 -> i32 * i32u\n",
    ] {
        expect_error(
            reserved_errno,
            "E1008",
            DiagnosticKind::ReservedExternSymbol,
        );
    }

    let ok_errno = r#"
@last_error
extern "close" def close_fd :: i32 -> i32 * i32u

export def run :: i32 -> i64 = \fd ->
    let (_, err) = close_fd fd
    err as i64
"#;
    let mut errno_mod = analyze(ok_errno).expect("analyze close_fd");
    for sym in ["__errno_location", "__error"] {
        errno_mod.extern_abis.insert(
            sym.to_owned(),
            tsuzuri::abi::CSignature {
                parameters: Vec::new(),
                result: tsuzuri::abi::CType::I64,
            },
        );
    }
    for (triple, sym) in [
        (Triple::X86_64LinuxGnu, "__errno_location"),
        (Triple::Aarch64AppleDarwin, "__error"),
    ] {
        let _guard = PlatformScope::enter(Platform::new(Some(triple)));
        let ir = llvm::emit(&errno_mod, llvm::Entry::Library).expect("emit POSIX IR");
        let decls: Vec<&str> = ir
            .lines()
            .filter(|l| l.starts_with("declare ") && l.contains(&format!("@{sym}(")))
            .collect();
        assert_eq!(
            decls,
            vec![format!("declare i64 @{sym}()")],
            "expected single merged i64 declare for {sym}:\n{ir}"
        );
        assert!(
            ir.contains("inttoptr i64 "),
            "expected inttoptr cast for {sym}:\n{ir}"
        );
    }
}

fn function_lines<'a>(ir: &'a str, needle: &str) -> Vec<&'a str> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in ir.lines() {
        if line.starts_with("define ") && line.contains(needle) {
            inside = true;
        }
        if inside {
            out.push(line.trim());
            if line == "}" {
                break;
            }
        }
    }
    assert!(!out.is_empty(), "missing function {needle} in:\n{ir}");
    out
}

#[test]
fn capture_follows_the_call() {
    let source = r#"
@last_error
extern "demo_open" def demo_open :: ref string -> i32 * i32u

export def run :: ref string -> i32u = \s ->
    let (_, err) = demo_open s
    err
"#;
    let module = analyze(source).expect("analyze demo_open");

    let options = llvm::EmitOptions {
        entry: llvm::Entry::Library,
        wasm: false,
        wasm_simd128: false,
        debug_output: false,
        runtime_objects: false,
        allocator: llvm::Allocator::System,
        f16: llvm::F16Path::Soft,
    };

    {
        let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
        let ir = llvm::emit_with_strings(&module, options, llvm::StringForm::Compact)
            .expect("emit Windows IR");
        let lines = function_lines(&ir, "@tz.fn.Main.demo_open(");
        let widen_pos = lines
            .iter()
            .position(|l| l.contains("@tz.string.borrow_utf16("))
            .expect("borrow_utf16 before clear");
        let clear_pos = lines
            .iter()
            .position(|l| *l == "call void @SetLastError(i32 0)")
            .expect("SetLastError(0)");
        let call_pos = lines
            .iter()
            .position(|l| l.contains("call i32 @demo_open("))
            .expect("call @demo_open");
        let read_pos = lines
            .iter()
            .position(|l| l.ends_with("= call i32 @GetLastError()"))
            .expect("call @GetLastError");
        let free_pos = lines
            .iter()
            .position(|l| l.contains("@tz.free("))
            .expect("@tz.free of widened copy");
        assert!(widen_pos < clear_pos, "{lines:#?}");
        assert_eq!(
            call_pos,
            clear_pos + 1,
            "no instructions between SetLastError(0) and call:\n{lines:#?}"
        );
        assert_eq!(
            read_pos,
            call_pos + 1,
            "no instructions between call and GetLastError():\n{lines:#?}"
        );
        assert!(read_pos < free_pos, "{lines:#?}");
    }

    for (triple, helper) in [
        (Triple::X86_64LinuxGnu, "@__errno_location()"),
        (Triple::Aarch64AppleDarwin, "@__error()"),
    ] {
        let _guard = PlatformScope::enter(Platform::new(Some(triple)));
        let ir = llvm::emit_with_strings(&module, options, llvm::StringForm::Compact)
            .expect("emit POSIX IR");
        let lines = function_lines(&ir, "@tz.fn.Main.demo_open(");
        let widen_pos = lines
            .iter()
            .position(|l| l.contains("@tz.string.borrow_utf16("))
            .expect("borrow_utf16");
        let loc_pos = lines
            .iter()
            .position(|l| l.contains(&format!("call ptr {helper}")))
            .expect("errno pointer helper");
        let clear_pos = lines
            .iter()
            .position(|l| l.starts_with("store i32 0, ptr "))
            .expect("errno = 0 store");
        let call_pos = lines
            .iter()
            .position(|l| l.contains("call i32 @demo_open("))
            .expect("call @demo_open");
        let read_pos = lines
            .iter()
            .position(|l| l.contains("= load i32, ptr "))
            .expect("errno load");
        let free_pos = lines
            .iter()
            .position(|l| l.contains("@tz.free("))
            .expect("@tz.free");
        assert!(widen_pos < loc_pos, "{lines:#?}");
        assert_eq!(clear_pos, loc_pos + 1, "{lines:#?}");
        assert_eq!(call_pos, clear_pos + 1, "{lines:#?}");
        assert_eq!(read_pos, call_pos + 1, "{lines:#?}");
        assert!(read_pos < free_pos, "{lines:#?}");
    }
}

#[test]
fn capture_sits_inside_the_blocking_bracket() {
    let source = r#"
@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u

extern "GetCurrentProcessId" def plain_pid :: unit -> i32u

def main :: unit -> i32 = \() ->
    let ch: Channel<i64> = Channel.make 1
    let out = Channel.share (ref ch)
    tzroutine {
        let (_, err) = close_checked 0
        Channel.send (ref out) ((err + plain_pid ()) as i64)
    }
    let got = match Channel.recv (ref ch) with | Maybe.Some x -> x | Maybe.None -> 0
    got as i32
"#;
    let module = analyze(source).expect("analyze spawning @last_error program");
    let _guard = PlatformScope::enter(Platform::new(Some(Triple::X86_64WindowsMsvc)));
    let ir = llvm::emit(&module, llvm::Entry::Console).expect("emit spawning Windows IR");

    let checked_lines = function_lines(&ir, "@tz.fn.Main.close_checked(");
    let enters: Vec<usize> = checked_lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (*l == "call void @tsuzuri_tz_blocking_enter()").then_some(i))
        .collect();
    let exits: Vec<usize> = checked_lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (*l == "call void @tsuzuri_tz_blocking_exit()").then_some(i))
        .collect();
    assert_eq!(
        enters.len(),
        1,
        "expected 1 blocking_enter in close_checked:\n{checked_lines:#?}"
    );
    assert_eq!(
        exits.len(),
        1,
        "expected 1 blocking_exit in close_checked:\n{checked_lines:#?}"
    );
    let clear_pos = checked_lines
        .iter()
        .position(|l| *l == "call void @SetLastError(i32 0)")
        .unwrap();
    let call_pos = checked_lines
        .iter()
        .position(|l| l.contains("call i32 @CloseHandle("))
        .unwrap();
    let read_pos = checked_lines
        .iter()
        .position(|l| l.ends_with("= call i32 @GetLastError()"))
        .unwrap();
    assert!(
        enters[0] < clear_pos && clear_pos < call_pos && call_pos < read_pos && read_pos < exits[0]
    );

    let plain_lines = function_lines(&ir, "@tz.fn.Main.plain_pid(");
    let p_enter = plain_lines
        .iter()
        .position(|l| *l == "call void @tsuzuri_tz_blocking_enter()")
        .expect("plain extern still gets blocking_enter");
    let p_call = plain_lines
        .iter()
        .position(|l| l.contains("call i32 @GetCurrentProcessId()"))
        .unwrap();
    let p_exit = plain_lines
        .iter()
        .position(|l| *l == "call void @tsuzuri_tz_blocking_exit()")
        .expect("plain extern still gets blocking_exit");
    assert_eq!(p_call, p_enter + 1);
    assert_eq!(p_exit, p_call + 1);

    let enter_declares = ir
        .lines()
        .filter(|l| *l == "declare void @tsuzuri_tz_blocking_enter()")
        .count();
    let exit_declares = ir
        .lines()
        .filter(|l| *l == "declare void @tsuzuri_tz_blocking_exit()")
        .count();
    assert_eq!(enter_declares, 1);
    assert_eq!(exit_declares, 1);
}

#[test]
fn wasm_rejects_reached_last_error_externs() {
    let unreached = r#"
@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u

export def answer :: i64 -> i64 = \x -> x + 1
"#;
    let m_unreached = analyze(unreached).expect("analyze unreached @last_error");
    assert!(
        llvm::emit_target(&m_unreached, llvm::Entry::Library, true).is_ok(),
        "unreached @last_error extern must not fail WASM emit"
    );

    let reached = r#"
@last_error
extern "CloseHandle" def close_checked :: i64 -> bool * i32u

export def run :: i64 -> i32u = \h ->
    let (_, err) = close_checked h
    err
"#;
    let m_reached = analyze(reached).expect("analyze reached @last_error");
    let err = llvm::emit_target(&m_reached, llvm::Entry::Library, true)
        .expect_err("reached @last_error extern must fail WASM emit");
    assert_eq!(err.code, "E1008");
    assert_eq!(err.kind, Some(DiagnosticKind::AbiNativeOnly.id()));
}

#[test]
fn rejects_last_error_on_objc_declarations_and_lowers_by_value_record_results() {
    for src in [
        "@last_error\nextern class A : ObjC.Object\n0\n",
        "extern class A : ObjC.Object\n@last_error\nextern category Cat : A {}\n0\n",
    ] {
        let err = analyze(src).expect_err("extern class / category must reject @last_error");
        assert_eq!(err.code, "E0002");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::MisplacedLastErrorAttribute.id())
        );
    }

    let by_value_last_error = r#"
@c_layout
record Point { x: i32, y: i32 }

@c_layout
record BigBox { a: i64, b: i64, c: i64, d: i64, e: i64 }

@trusted
@last_error
extern "make_point" def make_point :: i32 -> Point * i32u

@trusted
@last_error
extern "make_box" def make_box :: i32 -> BigBox * i32u

export def run :: i32 -> i64 = \v ->
    let (pt, err1) = make_point v
    let (bx, err2) = make_box v
    (pt.x + pt.y) as i64 + bx.a + bx.e + (err1 + err2) as i64
"#;
    let m =
        analyze(by_value_last_error).expect("analyze @last_error with @c_layout by-value result");
    for triple in [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
    ] {
        let _guard = PlatformScope::enter(Platform::new(Some(Triple::parse(triple).unwrap())));
        let ir = llvm::emit(&m, llvm::Entry::Library).expect("emit @last_error by-value IR");
        assert!(
            ir.contains("@make_point(") && ir.contains("@make_box("),
            "missing calls for {triple}:\n{ir}"
        );
    }
    let wasm_err = llvm::emit_target(&m, llvm::Entry::Library, true)
        .expect_err("wasm must reject @last_error by-value result");
    assert_eq!(wasm_err.code, "E1008");
}
