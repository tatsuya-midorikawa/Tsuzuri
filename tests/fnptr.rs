use std::fs;
use std::process::Command;
use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::{analyze, llvm};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsuzuri")
}

fn emit_llvm_ir(source: &str) -> Result<String, tsuzuri::diagnostic::Diagnostic> {
    let module = analyze(source)?;
    llvm::emit_target(&module, llvm::Entry::Console, false)
}

fn emit_c_header(source: &str) -> Result<String, tsuzuri::diagnostic::Diagnostic> {
    let module = analyze(source)?;
    Ok(llvm::header(&module))
}

#[test]
fn fnptr_and_callback_argument_share_one_wrapper() {
    let source = r#"
@trusted
extern "host_direct" def host_direct :: (i64 -> i64) -> unit
@trusted
extern "host_ptr" def host_ptr :: FnPtr<(i64 -> i64)> -> unit

def step :: i64 -> i64 = \x -> x + 1

def main :: unit -> i32 = \() ->
    host_direct step
    let p = FnPtr.of step
    host_ptr p
    0
"#;
    let ir = emit_llvm_ir(source).expect("IR should emit");
    let ir2 = emit_llvm_ir(source).expect("IR should be deterministic");
    assert_eq!(ir, ir2);

    let defines: Vec<&str> = ir
        .lines()
        .filter(|line| {
            line.starts_with("define ")
                && line.contains("@tz.callback.")
                && !line.contains("@tz.callback.after_main(")
        })
        .collect();
    assert_eq!(
        defines.len(),
        1,
        "callback argument and FnPtr.of must share one wrapper:\n{ir}"
    );
    assert!(
        defines[0].contains("@tz.callback.Main.step("),
        "{defines:?}"
    );

    assert!(
        ir.contains("call void @host_direct(ptr @tz.callback.Main.step)"),
        "host_direct must receive @tz.callback.Main.step:\n{ir}"
    );
    assert!(
        ir.contains("store ptr @tz.callback.Main.step,")
            || ir.contains("call i8 @tz.fn.Main.host_ptr(ptr @tz.callback.Main.step)"),
        "FnPtr.of must materialize @tz.callback.Main.step:\n{ir}"
    );
}

#[test]
fn fnptr_is_checked_without_callback_externs() {
    let lambda = analyze(
        r#"
def main :: unit -> i32 = \() ->
    let _p = FnPtr.of (\(x: i64) -> x)
    0
"#,
    )
    .expect_err("lambda in FnPtr.of must fail even without callback externs");
    assert_eq!(lambda.code, "E1008");
    assert_eq!(lambda.kind, Some(DiagnosticKind::FnptrArgument.id()));

    let value_form = analyze(
        r#"
def step :: i64 -> i64 = \x -> x + 1
def main :: unit -> i32 = \() ->
    let make = FnPtr.of
    let _p = make step
    0
"#,
    )
    .expect_err("FnPtr.of as a first-class function value must fail");
    assert_eq!(value_form.code, "E1008");
    assert_eq!(value_form.kind, Some(DiagnosticKind::FnptrCallForm.id()));

    let piped = analyze(
        r#"
def step :: i64 -> i64 = \x -> x + 1
def main :: unit -> i32 = \() ->
    let _p = step |> FnPtr.of
    0
"#,
    )
    .expect_err("piped FnPtr.of must fail");
    assert_eq!(piped.code, "E1008");
    assert_eq!(piped.kind, Some(DiagnosticKind::FnptrCallForm.id()));
}

#[test]
fn accepts_fnptr_in_data_closures_partial_externs_and_pure_functions() {
    let source = r#"
extern type Hwnd

record Entry {
    handler: FnPtr<(ref Hwnd -> i32u -> i64u -> i64 -> i64)>,
    pure_step: std::FnPtr<(i64 -> i64)>,
}

@trusted
extern "reg_tagged" def reg_tagged :: i64 -> FnPtr<(i64 -> i64)> -> unit

def on_msg :: ref Hwnd -> i32u -> i64u -> i64 -> i64 = \_h _m w l -> (w as i64) + l

pure def inc :: i64 -> i64 = \x -> x + 1

def main :: unit -> i32 = \() ->
    let entry = Entry {
        handler: FnPtr.of on_msg,
        pure_step: FnPtr.of inc,
    }
    let arr: [FnPtr<(i64 -> i64)>] = [entry.pure_step]
    let opt: Maybe<FnPtr<(i64 -> i64)>> = Some arr[0]
    let reg = reg_tagged 7
    let runner = \() ->
        match opt with
        | Some p -> reg p
        | None -> ()
    runner ()
    0
"#;
    let ir_a = emit_llvm_ir(source).expect("valid FnPtr uses should compile");
    let ir_b = emit_llvm_ir(source).expect("deterministic IR");
    assert_eq!(ir_a, ir_b);
    assert!(ir_a.contains("@tz.callback.Main.on_msg("));
    assert!(ir_a.contains("@tz.callback.Main.inc("));
}

#[test]
fn rejects_invalid_fnptr_forms_and_positions() {
    let cases: &[(&str, &str, Option<DiagnosticKind>)] = &[
        (
            "def step :: i64 -> i64 = \\x -> x\ndef main :: unit -> i32 = \\() ->\n    let local = step\n    let _p = FnPtr.of local\n    0\n",
            "E1008",
            Some(DiagnosticKind::FnptrArgument),
        ),
        (
            "def add :: i64 -> i64 -> i64 = \\x y -> x + y\ndef main :: unit -> i32 = \\() ->\n    let _p = FnPtr.of (add 1)\n    0\n",
            "E1008",
            Some(DiagnosticKind::FnptrArgument),
        ),
        (
            "def main :: unit -> i32 = \\() ->\n    let _p: FnPtr<(i64 -> i64)> = FnPtr.of Int.abs\n    0\n",
            "E1008",
            Some(DiagnosticKind::FnptrArgument),
        ),
        (
            "@trusted\nextern \"c_abs\" def c_abs :: i64 -> i64\ndef main :: unit -> i32 = \\() ->\n    let _p = FnPtr.of c_abs\n    0\n",
            "E1008",
            Some(DiagnosticKind::FnptrArgument),
        ),
        (
            "def id :: 'a -> 'a = \\x -> x\ndef main :: unit -> i32 = \\() ->\n    let _p: FnPtr<(i64 -> i64)> = FnPtr.of id\n    0\n",
            "E1008",
            Some(DiagnosticKind::FnptrArgument),
        ),
        (
            "record Bad { f: FnPtr<i64> }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrSignature),
        ),
        (
            "record Bad { f: FnPtr<(string -> i64)> }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrSignature),
        ),
        (
            "@trusted\nextern \"bad\" def bad :: FnPtr<(string -> i64)> -> unit\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrSignature),
        ),
        (
            "record Bad { f: FnPtr }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1004",
            Some(DiagnosticKind::TypeArgumentCount),
        ),
        (
            "record Bad { f: FnPtr<(i64 -> i64), (i64 -> i64)> }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1004",
            Some(DiagnosticKind::TypeArgumentCount),
        ),
        (
            "record Bad<'a> { f: FnPtr<('a -> 'a)> }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1015",
            Some(DiagnosticKind::ExternSignatureNotConcrete),
        ),
        (
            "@trusted\nextern \"from_host\" def from_host :: unit -> FnPtr<(i64 -> i64)>\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrFromHost),
        ),
        (
            "export def give :: FnPtr<(i64 -> i64)> -> i64 = \\_p -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrFromHost),
        ),
        (
            "def step :: i64 -> i64 = \\x -> x\nexport def take :: unit -> FnPtr<(i64 -> i64)> = \\() -> FnPtr.of step\n",
            "E1008",
            Some(DiagnosticKind::FnptrFromHost),
        ),
        (
            "@trusted\nextern \"cb_host\" def cb_host :: (FnPtr<(i64 -> i64)> -> unit) -> unit\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1008",
            Some(DiagnosticKind::FnptrFromHost),
        ),
        (
            "def step :: i64 -> i64 = \\x -> x + 1\nconst P: FnPtr<(i64 -> i64)> = FnPtr.of step\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1026",
            Some(DiagnosticKind::ConstFnptr),
        ),
        (
            "record FnPtr { x: i64 }\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1001",
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

    let call_err = analyze(
        r#"
def step :: i64 -> i64 = \x -> x + 1
def main :: unit -> i32 = \() ->
    let p = FnPtr.of step
    let _y = p 1
    0
"#,
    )
    .expect_err("calling FnPtr must fail with E1005");
    assert_eq!(call_err.code, "E1005");
    assert_eq!(call_err.kind, Some(DiagnosticKind::CallNonFunction.id()));
    let help = call_err.help.as_deref().unwrap_or("");
    assert!(
        help.contains("cannot be called from Tsuzuri"),
        "expected FnPtr call help, got: {call_err:?}"
    );
}

#[test]
fn header_declarator_matches_callback_parameter() {
    let with_callback = r#"
extern type Hwnd
@trusted
extern def register_cb :: (ref Hwnd -> i32u -> i64u -> i64 -> i64) -> i32
export def keep :: i64 -> i32 = \_x -> 0
"#;
    let with_fnptr = r#"
extern type Hwnd
@trusted
extern def register_cb :: FnPtr<(ref Hwnd -> i32u -> i64u -> i64 -> i64)> -> i32
export def keep :: i64 -> i32 = \_x -> 0
"#;
    let h_cb = emit_c_header(with_callback).expect("callback header");
    let h_fp = emit_c_header(with_fnptr).expect("fnptr header");
    assert_eq!(h_cb, h_fp);
}

#[test]
fn wasm_build_and_test_reject_reached_fnptr_while_check_succeeds() {
    let dir = std::env::temp_dir().join(format!("tz-w22-fnptr-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tempdir");
    let main_tz = dir.join("Main.tz");
    fs::write(
        &main_tz,
        r#"
def step :: i64 -> i64 = \x -> x + 1

def main :: unit -> i32 = \() ->
    let _p = FnPtr.of step
    0

test "wasm fnptr" =
    let _p = FnPtr.of step
    assert true
"#,
    )
    .unwrap();

    let check = Command::new(bin())
        .args(["check", main_tz.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check should succeed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let out_wasm = dir.join("out.wasm");
    let build = Command::new(bin())
        .args([
            "build",
            main_tz.to_str().unwrap(),
            "--target",
            "wasm32",
            "--no-cache",
            "-o",
            out_wasm.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!build.status.success());
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(stderr.contains("E1008"), "stderr: {stderr}");
    assert!(stderr.contains("FnPtr.of"), "stderr: {stderr}");

    let test_wasm = Command::new(bin())
        .args(["test", main_tz.to_str().unwrap(), "--target", "wasm32"])
        .output()
        .unwrap();
    assert!(!test_wasm.status.success());
    let test_stderr = String::from_utf8_lossy(&test_wasm.stderr);
    assert!(test_stderr.contains("E1008"), "stderr: {test_stderr}");
    let _ = fs::remove_dir_all(&dir);
}
