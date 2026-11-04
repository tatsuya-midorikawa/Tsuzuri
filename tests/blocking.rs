// TZ08: in a native program that spawns tzroutines, every call of an extern function is a blocking
// call (between `tsuzuri_tz_blocking_enter` and `_exit`), and a callback or an export that the host
// calls takes the P back for its body. A program that does not spawn keeps its IR as it was.
use tsuzuri::{analyze, llvm};

fn ir(source: &str) -> String {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap()); // deterministic
    ir
}

/// The lines of `ir` around each call of `symbol`: the line before and the line after.
fn neighbours(ir: &str, symbol: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = ir.lines().collect();
    let call = format!("@{symbol}(");
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("call ") && line.contains(&call))
        .map(|(at, _)| {
            (
                lines[at - 1].trim().to_owned(),
                lines[at + 1].trim().to_owned(),
            )
        })
        .collect()
}

const EXTERN: &str = "extern def host_wait :: i64 -> i64\n";

#[test]
fn a_program_that_does_not_spawn_keeps_its_extern_calls() {
    let ir = ir(&format!(
        "{EXTERN}export def run :: own i64 -> i64\nfn run n = host_wait n + 1\n"
    ));
    assert!(!ir.contains("tsuzuri_tz_blocking"), "{ir}");
    assert!(!ir.contains("tsuzuri_tz_callback"), "{ir}");
    assert_eq!(neighbours(&ir, "tsuzuri_host_Main_host_wait").len(), 1);
}

#[test]
fn an_extern_call_of_a_spawning_program_is_a_blocking_call() {
    let ir = ir(&format!(
        "{EXTERN}export def run :: own i64 -> i64\nfn run n =\n    tzroutine {{\n        let _value = host_wait n\n    }}\n    host_wait (n + 1)\n"
    ));
    let calls = neighbours(&ir, "tsuzuri_host_Main_host_wait");
    assert!(!calls.is_empty(), "{ir}");
    for (before, after) in calls {
        assert_eq!(before, "call void @tsuzuri_tz_blocking_enter()");
        assert_eq!(after, "call void @tsuzuri_tz_blocking_exit()");
    }
    assert_eq!(
        ir.matches("call void @tsuzuri_tz_blocking_enter()").count(),
        ir.matches("call void @tsuzuri_tz_blocking_exit()").count()
    );
    assert_eq!(
        ir.matches("declare void @tsuzuri_tz_blocking_enter()")
            .count(),
        1
    );
    assert_eq!(
        ir.matches("declare void @tsuzuri_tz_blocking_exit()")
            .count(),
        1
    );
}

#[test]
fn a_callback_takes_the_p_back_for_its_body() {
    let source = "extern def host_apply :: (i64 -> i64) -> i64 -> i64\n\
                  def twice :: i64 -> i64 = \\x -> x * 2\n\
                  export def run :: own i64 -> i64\nfn run n =\n    tzroutine {\n        let _value = host_apply twice n\n    }\n    n\n";
    let ir = ir(source);
    let wrapper = ir
        .split("\ndefine ")
        .find(|function| function.contains("@tz.callback.Main.twice("))
        .unwrap_or_else(|| panic!("{ir}"));
    assert!(
        wrapper.contains("%tz.callback = call i32 @tsuzuri_tz_callback_enter()"),
        "{wrapper}"
    );
    let rets = wrapper
        .lines()
        .filter(|line| line.trim_start().starts_with("ret "))
        .count();
    assert!(rets > 0);
    assert_eq!(
        wrapper
            .matches("call void @tsuzuri_tz_callback_exit(i32 %tz.callback)")
            .count(),
        rets,
        "{wrapper}"
    );
    // the callback goes back into the call before each return
    let lines: Vec<&str> = wrapper.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("ret ") {
            assert_eq!(
                lines[at - 1].trim(),
                "call void @tsuzuri_tz_callback_exit(i32 %tz.callback)"
            );
        }
    }
    for (before, after) in neighbours(&ir, "tsuzuri_host_Main_host_apply") {
        assert_eq!(before, "call void @tsuzuri_tz_blocking_enter()");
        assert_eq!(after, "call void @tsuzuri_tz_blocking_exit()");
    }
}

#[test]
fn an_export_takes_the_p_back_as_a_callback_does() {
    // A host may call an export inside an extern call that a tzroutine made: the public wrapper
    // takes the P back first (before its boundary starts) and goes back into the call last (after
    // the boundary waited for its tzroutines).
    let source = "extern def host_call :: i64 -> i64\n\
                  export def run :: own i64 -> i64\nfn run n =\n    tzroutine {\n        let _value = host_call n\n    }\n    n\n";
    let text = ir(source);
    let wrapper = text
        .split("\ndefine ")
        .find(|function| function.contains(" @tz_run("))
        .unwrap_or_else(|| panic!("{text}"));
    let lines: Vec<&str> = wrapper.lines().map(str::trim).collect();
    assert_eq!(
        lines[2], "%tz.callback = call i32 @tsuzuri_tz_callback_enter()",
        "{wrapper}"
    );
    let enter = wrapper.find("@tsuzuri_tz_callback_enter()").unwrap();
    let scope = wrapper
        .find("@tsuzuri_tz_scope_enter()")
        .unwrap_or_else(|| panic!("{wrapper}"));
    assert!(enter < scope, "{wrapper}");
    let mut rets = 0;
    for (at, line) in lines.iter().enumerate() {
        if line.starts_with("ret ") {
            rets += 1;
            assert_eq!(
                lines[at - 1],
                "call void @tsuzuri_tz_callback_exit(i32 %tz.callback)",
                "{wrapper}"
            );
        }
    }
    assert!(rets > 0, "{wrapper}");
    // a program without an extern call has no call that an export could come from
    let plain = ir(
        "export def run :: own i64 -> i64\nfn run n =\n    tzroutine {\n        let _value = n\n    }\n    n\n",
    );
    assert!(!plain.contains("tsuzuri_tz_callback"), "{plain}");
}

#[test]
fn a_callback_of_a_program_that_does_not_spawn_is_unchanged() {
    let source = "extern def host_apply :: (i64 -> i64) -> i64 -> i64\n\
                  def twice :: i64 -> i64 = \\x -> x * 2\n\
                  export def run :: own i64 -> i64\nfn run n = host_apply twice n\n";
    let ir = ir(source);
    assert!(ir.contains("@tz.callback.Main.twice("), "{ir}");
    assert!(!ir.contains("tsuzuri_tz_"), "{ir}");
}

#[test]
fn with_locked_thread_locks_around_the_call_natively() {
    let source =
        "export def run :: own i64 -> i64\nfn run n = Routine.with_locked_thread (\\() -> n + 1)\n";
    let native = ir(source);
    let lock = native
        .find("call void @tsuzuri_tz_lock_thread()")
        .unwrap_or_else(|| panic!("{native}"));
    let unlock = native
        .find("call void @tsuzuri_tz_unlock_thread()")
        .unwrap_or_else(|| panic!("{native}"));
    assert!(lock < unlock);
    assert_eq!(
        native
            .matches("declare void @tsuzuri_tz_lock_thread()")
            .count(),
        1
    );
    // WebAssembly runs one thread: the function is called directly.
    let module = analyze(source).unwrap();
    let wasm = llvm::emit_target(&module, llvm::Entry::Library, true).unwrap();
    assert!(!wasm.contains("tsuzuri_tz_lock_thread"), "{wasm}");
    assert!(
        analyze("let x: string = Routine.with_locked_thread (\\() -> 1)\nx\n").is_err(),
        "the result is the function's"
    );
}
