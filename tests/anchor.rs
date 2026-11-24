use std::fs;
use std::process::Command;
use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::{analyze, analyze_modules, llvm};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsuzuri")
}

#[test]
fn anchor_api_signatures_put_token_last_and_emit_deterministic_ir() {
    let source = r#"
record State { clicks: i64, label: string }

def main :: unit -> i32 = \() ->
    let mk: State -> Anchor<State> = Anchor.create
    let owner: Anchor<State> = mk (State { clicks: 1, label: "ok" })
    let get_tok: ref Anchor<State> -> i64 = Anchor.token
    let tok = ref owner |> get_tok
    let rd: (ref State -> i64) -> i64 -> Result<i64, Anchor.Error> = Anchor.read
    let up: (State -> State) -> i64 -> Result<unit, Anchor.Error> = Anchor.update
    let _u = tok |> up (\s ->
        let next = s.clicks + 2
        { s with clicks = next })
    match tok |> rd (\s -> s.clicks) with
    | Result.Ok n -> n as i32
    | Result.Error Anchor.Stale -> -1
    | Result.Error Anchor.WrongType -> -2
    | Result.Error Anchor.WrongThread -> -3
    | Result.Error Anchor.Busy -> -4
"#;
    let module = analyze(source).expect("Anchor API should type-check");
    let ir_a = llvm::emit_target(&module, llvm::Entry::Console, false).expect("native IR");
    let ir_b = llvm::emit_target(&module, llvm::Entry::Console, false).expect("deterministic IR");
    assert_eq!(ir_a, ir_b);

    let desc_lines: Vec<&str> = ir_a
        .lines()
        .filter(|line| line.starts_with("@tz.anchor.type."))
        .collect();
    assert_eq!(desc_lines.len(), 1, "{desc_lines:?}");
    assert!(
        !desc_lines[0].contains("unnamed_addr"),
        "anchor type descriptor must omit unnamed_addr for split codegen units: {}",
        desc_lines[0]
    );
}

#[test]
fn anchor_rejects_opaque_construction_private_release_capture_send_sync_and_borrows() {
    let cases: &[(&str, &str, Option<DiagnosticKind>)] = &[
        // Opaque record construction & destructuring -> E1022
        (
            "def main :: unit -> i32 = \\() ->\n    let _a: Anchor<i64> = Anchor { token: 1 }\n    0\n",
            "E1022",
            None,
        ),
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create 1i64\n    match a with\n    | Anchor { token: t } -> t as i32\n",
            "E1022",
            None,
        ),
        // Private Anchor.__release -> E1022
        (
            "def main :: unit -> i32 = \\() ->\n    Anchor.__release 1\n    0\n",
            "E1022",
            Some(DiagnosticKind::PrivateStdPrimitive),
        ),
        // Capturing Anchor<T> in reusable function value -> E1005
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create 1i64\n    let f = \\() -> Anchor.token (ref a)\n    f () as i32\n",
            "E1005",
            None,
        ),
        // Moving Anchor<T> into tzroutine { } (!Send) -> E1013
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create 1i64\n    tzroutine {\n        let moved = a\n        let _t = Anchor.token (ref moved)\n        ()\n    }\n    0\n",
            "E1013",
            Some(DiagnosticKind::TaskRequiresSend),
        ),
        // Sharing Anchor<T> via Routine.scope (!Sync) -> E1013
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create 1i64\n    let _r = Routine.scope (ref a) 2 (\\s _i -> Anchor.token s)\n    0\n",
            "E1013",
            Some(DiagnosticKind::TaskRequiresSync),
        ),
        // Anchor.read returning `ref` to anchored value -> E1013 / anchor_result_retains_borrow
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create \"hi\"\n    let _r = Anchor.read (\\(s: ref string) -> s) (Anchor.token (ref a))\n    0\n",
            "E1013",
            Some(DiagnosticKind::AnchorResultRetainsBorrow),
        ),
        // Anchor.read returning closure borrowing anchored value -> E1013 / anchor_result_retains_borrow
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create \"hi\"\n    let _r = Anchor.read (\\(s: ref string) -> \\() -> String.length s) (Anchor.token (ref a))\n    0\n",
            "E1013",
            Some(DiagnosticKind::AnchorResultRetainsBorrow),
        ),
        // Anchor.create holding borrowed reference -> E1013 / anchor_value_retains_borrow
        (
            "def main :: unit -> i32 = \\() ->\n    let local = \"hi\"\n    let _a = Anchor.create (ref local)\n    0\n",
            "E1013",
            Some(DiagnosticKind::AnchorValueRetainsBorrow),
        ),
        // Anchor.create holding closure with borrowed environment -> E1013 / anchor_value_retains_borrow
        (
            "def main :: unit -> i32 = \\() ->\n    let local = \"hi\"\n    let r = ref local\n    let _a = Anchor.create (\\() -> String.length r)\n    0\n",
            "E1013",
            Some(DiagnosticKind::AnchorValueRetainsBorrow),
        ),
        // Anchor.update returning closure with borrowed environment -> E1013 / anchor_value_retains_borrow
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create (\\() -> 0)\n    let local = \"hi\"\n    let r = ref local\n    let _u = Anchor.update (\\(_old: unit -> i64) -> \\() -> String.length r) (Anchor.token (ref a))\n    0\n",
            "E1013",
            Some(DiagnosticKind::AnchorValueRetainsBorrow),
        ),
        // pure def calling Anchor.read -> E1029
        (
            "pure def bad :: i64 -> i64 = \\tok ->\n    match Anchor.read (\\(n: ref i64) -> deref n) tok with\n    | Result.Ok v -> v\n    | Result.Error _ -> 0\ndef main :: unit -> i32 = \\() -> 0\n",
            "E1029",
            None,
        ),
        // Domain.run calling Anchor.read -> E1038
        (
            "def main :: unit -> i32 = \\() ->\n    let a = Anchor.create 1i64\n    let tok = Anchor.token (ref a)\n    let _r = Domain.run (\\() -> Anchor.read (\\(n: ref i64) -> deref n) tok)\n    0\n",
            "E1038",
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
        // Diagnostics must point to the user module (source 0), never inside std/Anchor.tz!
        assert_eq!(
            err.span.source,
            Some(0),
            "case #{index} span should be in user code:\n{err:?}"
        );
    }

    // Reserved module name Anchor -> E1011
    let mod_err = analyze_modules(&[
        ("Anchor.tz", "record Anchor { x: i64 }\n"),
        ("Main.tz", "def main :: unit -> i32 = \\() -> 0\n"),
    ])
    .expect_err("user module named Anchor must be rejected");
    assert_eq!(mod_err.code, "E1011", "{mod_err:?}");
}

#[test]
fn anchor_cli_rejects_trap_return_wasm_threads_and_freestanding() {
    let dir = std::env::temp_dir().join(format!("tz-w23-anchor-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("tempdir");
    let lib_tz = dir.join("Main.tz");
    fs::write(
        &lib_tz,
        r#"
export def run :: i64 -> i64 = \x ->
    let a = Anchor.create x
    let tok = Anchor.token (ref a)
    match Anchor.read (\v -> deref v) tok with
    | Result.Ok n -> n
    | Result.Error _ -> -1
"#,
    )
    .unwrap();

    for (tag, extra) in [
        (
            "trap-return",
            vec!["--emit", "object", "--trap-mode", "return", "-o", "out.o"],
        ),
        (
            "wasm-threads",
            vec![
                "--target",
                "wasm32",
                "--wasm-feature",
                "threads",
                "-o",
                "out.wasm",
            ],
        ),
        (
            "freestanding",
            vec![
                "--emit",
                "object",
                "--freestanding",
                "--allocator",
                "host",
                "-o",
                "free.o",
            ],
        ),
    ] {
        let mut cmd = Command::new(bin());
        cmd.arg("build")
            .arg(&lib_tz)
            .arg("--no-cache")
            .current_dir(&dir)
            .args(&extra);
        let out = cmd.output().unwrap();
        assert!(!out.status.success(), "{tag} should fail");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("E2000"), "{tag} stderr: {stderr}");
        assert!(stderr.contains("Anchor"), "{tag} stderr: {stderr}");
    }

    let _ = fs::remove_dir_all(&dir);
}
