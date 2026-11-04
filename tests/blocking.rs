// TZ08: in a native program that spawns tzroutines, every call of an extern function is a blocking
// call (between `tsuzuri_tz_blocking_enter` and `_exit`), and a callback that the host calls takes
// the P back for its body. A program that does not spawn keeps its IR as it was.
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
fn a_callback_of_a_program_that_does_not_spawn_is_unchanged() {
    let source = "extern def host_apply :: (i64 -> i64) -> i64 -> i64\n\
                  def twice :: i64 -> i64 = \\x -> x * 2\n\
                  export def run :: own i64 -> i64\nfn run n = host_apply twice n\n";
    let ir = ir(source);
    assert!(ir.contains("@tz.callback.Main.twice("), "{ir}");
    assert!(!ir.contains("tsuzuri_tz_"), "{ir}");
}
