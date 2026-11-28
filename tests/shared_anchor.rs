//! M09 Phase 3: shared anchors (`Anchor.create_shared`, `shared_token`, `read_shared`,
//! `detach_shared`, `release_shared`), which a thread that has not joined the runtime reads too.
//! The values are checked by `features.mjs anchor`, the host thread by `tests/main_thread.mjs`.

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message))
}

fn rejects(source: &str, code: &str, kind: DiagnosticKind) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind.id()), "{source}\n{}", error.message);
}

const PROGRAM: &str = "\
def main :: unit -> i32 = \\() ->
    let state = Anchor.create_shared 41i64
    let token = Anchor.shared_token (ref state)
    let seen = match Anchor.read_shared (\\(n: ref i64) -> deref n + 1) token with
        | Result.Ok value -> value
        | Result.Error _ -> 0
    let kept = Anchor.detach_shared state
    if Anchor.release_shared kept then seen as i32 else 1
";

#[test]
fn the_functions_have_the_types_of_the_reference() {
    accepts(PROGRAM);
    accepts(
        "let make: i64 -> Anchor.Shared<i64> = Anchor.create_shared\n\
         let key: ref Anchor.Shared<i64> -> i64 = Anchor.shared_token\n\
         let give: Anchor.Shared<i64> -> i64 = Anchor.detach_shared\n\
         let free: i64 -> bool = Anchor.release_shared\n0\n",
    );
    // The token is the last argument, as in `Anchor.read` (D-59).
    accepts(
        "let state = Anchor.create_shared 1i64\n\
         let _read = Anchor.shared_token (ref state) |> Anchor.read_shared (\\(n: ref i64) -> deref n)\n0\n",
    );
    // The owner moves between threads, and several may ask it for the token.
    accepts(
        "let state = Anchor.create_shared 1i64\n\
         let _tokens = Routine.scope (ref state) 2 (\\shared _index -> Anchor.shared_token shared)\n\
         tzroutine {\n    let _key = Anchor.shared_token (ref state)\n    ()\n}\n0\n",
    );
}

#[test]
fn the_value_is_send_and_sync_and_keeps_no_borrow() {
    // Every thread reads it at once (Sync), and the thread of the last read drops it (Send).
    rejects(
        "let _state = Anchor.create_shared (Rc.new 1i64)\n0\n",
        "E1013",
        DiagnosticKind::TaskRequiresSend,
    );
    rejects(
        "extern type Device\n@trusted\nextern \"open_device\" def open_device :: unit -> Device\n\
         let _state = Anchor.create_shared (open_device ())\n0\n",
        "E1013",
        DiagnosticKind::TaskRequiresSync,
    );
    rejects(
        "def main :: unit -> i32 = \\() ->\n    let text = \"hi\"\n    let _state = Anchor.create_shared (ref text)\n    0\n",
        "E1013",
        DiagnosticKind::TaskRequiresOwnedValue,
    );
    rejects(
        "def main :: unit -> i32 = \\() ->\n    let state = Anchor.create_shared \"hi\"\n    let _r = Anchor.read_shared (\\(s: ref string) -> s) (Anchor.shared_token (ref state))\n    0\n",
        "E1013",
        DiagnosticKind::AnchorResultRetainsBorrow,
    );
}

#[test]
fn shared_anchors_are_state_like_the_others() {
    let error = analyze(
        "pure def f :: i64 -> i64 = \\n ->\n    let _state = Anchor.create_shared n\n    n\n\nf 1\n",
    )
    .expect_err("pure");
    assert_eq!(error.code, "E1029", "{}", error.message);
    // The private builtin of `detach_shared` belongs to `std/Anchor.tz`.
    assert!(analyze("Anchor.__detach 1\n0\n").is_err());
    // The record is made only by `create_shared`.
    assert!(analyze("let _forged = Anchor.Shared { token: 1 }\n0\n").is_err());
}

#[test]
fn the_ir_calls_the_shared_entries_of_the_runtime() {
    let module = accepts(PROGRAM);
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Console).unwrap());
    for declaration in [
        "declare i64 @tsuzuri_tz_anchor_create_shared(ptr, ptr, ptr, ptr, ptr)\n",
        "declare i32 @tsuzuri_tz_anchor_begin_shared(i64, ptr, ptr)\n",
        "declare i32 @tsuzuri_tz_anchor_end(i64, i32)\n",
        "declare void @tsuzuri_tz_anchor_detach(i64)\n",
        "declare i32 @tsuzuri_tz_anchor_release_shared(i64)\n",
        "declare void @tsuzuri_tz_anchor_release(i64)\n",
    ] {
        assert_eq!(ir.matches(declaration).count(), 1, "{declaration}\n{ir}");
    }
    // A shared read never begins as a read of one thread's anchor.
    assert!(!ir.contains("@tsuzuri_tz_anchor_begin("), "{ir}");
    assert!(!ir.contains("@tsuzuri_tz_anchor_create("), "{ir}");
    // A program of thread anchors only names none of the shared entries.
    let plain = accepts(
        "let state = Anchor.create 1i64\nlet _r = Anchor.read (\\(n: ref i64) -> deref n) (Anchor.token (ref state))\n0\n",
    );
    let ir = llvm::emit(&plain, llvm::Entry::Console).unwrap();
    assert!(
        !ir.contains("_shared") && !ir.contains("anchor_detach"),
        "{ir}"
    );
}
