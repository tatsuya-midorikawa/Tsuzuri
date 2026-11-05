//! Go's two-way `Channel` (TZ03): types, `for x in ch`, the items' borrows, the generated code and
//! the targets. The schedules are in `node tests/features.mjs <bin> channel` and `channel_single`.
use std::path::Path;
use std::process::Command;

use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> tsuzuri::check::CheckedModule {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Console).unwrap(),
        "{source}: IR is deterministic"
    );
    module
}

fn rejects(source: &str, code: &str, kind: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    error.message.clone()
}

#[test]
fn makes_sends_and_receives() {
    let module = accepts(
        "let ch: Channel<i64> = Channel.make 1\n\
         Channel.send ch 1\n\
         let first = match Channel.recv ch with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\
         let tried = match Channel.try_send ch 2 with\n    | Result.Ok _ -> 1\n    | Result.Error (Channel.SendError.Full item) -> item\n    | Result.Error (Channel.SendError.Closed item) -> item\n\
         let taken = match Channel.try_recv ch with\n    | Channel.Received.Item v -> v\n    | Channel.Received.Empty -> 0\n    | Channel.Received.Disconnected -> 0\n\
         let other = Channel.share ch\n\
         Channel.close other\n\
         first + tried + taken + Channel.length ch + Channel.capacity ch\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    for declaration in [
        "declare void @tsuzuri_tz_chan_init(ptr, i64, i64)",
        "declare i32 @tsuzuri_tz_chan_send(ptr, ptr, i32)",
        "declare i32 @tsuzuri_tz_chan_recv(ptr, ptr, i32)",
        "declare i32 @tsuzuri_tz_chan_close(ptr)",
        "declare i64 @tsuzuri_tz_chan_length(ptr)",
        "declare i64 @tsuzuri_tz_chan_capacity(ptr)",
        "declare void @tsuzuri_tz_chan_share(ptr)",
        "declare i32 @tsuzuri_tz_chan_release(ptr)",
        "declare void @tsuzuri_tz_report(i32)",
    ] {
        assert_eq!(
            ir.lines().filter(|line| *line == declaration).count(),
            1,
            "{declaration}"
        );
    }
    // A waiting send and receive, and the non-waiting ones.
    assert!(ir.contains("@tsuzuri_tz_chan_send(ptr %"));
    assert!(ir.contains(", i32 1)") && ir.contains(", i32 0)"));
    // A channel of plain items frees its block without taking items out of it.
    assert!(!ir.contains("@tsuzuri_tz_chan_drain"));
    let owned = accepts(
        "let ch: Channel<string> = Channel.make 2\nChannel.send ch \"left\"\nChannel.length ch\n",
    );
    assert!(
        llvm::emit(&owned, llvm::Entry::Console)
            .unwrap()
            .contains("declare i32 @tsuzuri_tz_chan_drain(ptr, ptr)")
    );
}

#[test]
fn checks_the_types_of_a_channel() {
    rejects(
        "let ch = Channel.make 0\n0\n",
        "E1015",
        "ambiguous_polymorphic_use",
    );
    assert_eq!(
        analyze("let ch: Channel<Rc<i64>> = Channel.make 1\n0\n")
            .expect_err("Rc is not Send")
            .code,
        "E1013"
    );
    for source in [
        "let ch: Channel<i64> = Channel { block: 0 }\n0\n",
        "let ch: Channel<i64> = Channel.make 1\nch.block\n",
        "let ch: Channel<i64> = Channel.make 1\nChannel.__release ch\n0\n",
    ] {
        assert_eq!(analyze(source).expect_err(source).code, "E1022", "{source}");
    }
    assert_eq!(
        analyze("let ch: Channel<i64> = Channel.make 1\nlet f: unit -> i64 = \\() -> Channel.length ch\nf ()\n")
            .expect_err("a Channel is not Capture")
            .code,
        "E1005"
    );
    assert_eq!(
        analyze("export def f :: Channel<i64> -> i64\nfn f ch = 0\n")
            .expect_err("not at a boundary")
            .code,
        "E1008"
    );
    // A channel moves into a tzroutine, and both channel kinds live in one program.
    accepts(
        "let ch: Channel<i64> = Channel.make 1\nlet mine = Channel.share ch\ntzroutine {\n    Channel.send mine 1\n}\nlet pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = DataChannel.bounded 1\nmatch Channel.recv ch with\n| Maybe.Some v -> v\n| Maybe.None -> 0\n",
    );
}

#[test]
fn receives_with_for_until_the_channel_is_closed() {
    accepts(
        "let ch: Channel<string> = Channel.make 2\nChannel.send ch \"a\"\nChannel.close ch\nlet mut total = 0i64\nfor text in ch do\n    if String.length text > 5 then\n        break\n    if String.length text == 0 then\n        continue\n    total = total + String.length text\nfor _again in ch do\n    total = total + 1\ntotal\n",
    );
    // A borrowed channel and a temporary one.
    accepts(
        "def sum :: ref Channel<i64> -> i64\nfn sum ch =\n    let mut total = 0i64\n    for v in ch do\n        total = total + v\n    total\n\ndef fresh :: unit -> Channel<i64>\nfn fresh _unit =\n    let ch: Channel<i64> = Channel.make 1\n    Channel.close ch\n    ch\n\nlet mut n = 0i64\nfor v in fresh () do\n    n = n + v\nn\n",
    );
    let message = rejects(
        "for x in 5 do\n    ()\n0\n",
        "E1005",
        "for_source_not_iterable",
    );
    assert!(
        message.contains("Seq, Channel, or ReceiveOnly"),
        "{message}"
    );
    // DataChannel's ends are not for sources.
    assert_eq!(
        analyze("let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = DataChannel.bounded 1\nmatch pair with\n| (_s, r) ->\n    for x in r do\n        ()\n0\n")
            .expect_err("a Receiver is not a for source")
            .code,
        "E1005"
    );
}

#[test]
fn an_item_holds_no_borrow() {
    let message = rejects(
        "def push :: ref Channel<i64 -> i64> -> ref [i64] -> unit\nfn push ch data =\n    Channel.send ch (\\x -> x + data[0])\n\n0\n",
        "E1013",
        "channel_item_holds_borrow",
    );
    assert!(message.contains("a channel item cannot hold a borrow"));
    for (function, source) in [
        (
            "Channel.send",
            "def f :: ref Channel<i64 -> i64> -> unit\nfn f ch =\n    let send = Channel.send ch\n    ()\n\n0\n",
        ),
        (
            "Channel.try_send",
            "def f :: ref Channel<i64 -> i64> -> unit\nfn f ch =\n    let send = Channel.try_send ch\n    ()\n\n0\n",
        ),
    ] {
        let message = rejects(source, "E1013", "must_apply_directly");
        assert!(
            message.starts_with(&format!("{function} must be fully applied directly")),
            "{message}"
        );
        assert!(message.contains("with the channel and the item together"));
    }
    // A function of copies, and an owned function, are items.
    accepts(
        "let ch: Channel<i64 -> i64> = Channel.make 1\nlet k = 5i64\nChannel.send ch (\\x -> x + k)\nmatch Channel.recv ch with\n| Maybe.Some f -> f 1\n| Maybe.None -> 0\n",
    );
    accepts(
        "let ch: Channel<Owned.Function<unit, unit>> = Channel.make 1\nChannel.send ch (Owned.function (\\() -> ()))\nChannel.length ch\n",
    );
    // DataChannel's message names the sender, as before.
    let message = rejects(
        "def f :: ref DataChannel.Sender<i64 -> i64> -> unit\nfn f sender =\n    let send = DataChannel.send sender\n    ()\n\n0\n",
        "E1013",
        "must_apply_directly",
    );
    assert!(message.starts_with("DataChannel.send must be fully applied directly"));
    assert!(message.contains("with the sender and the item together"));
}

#[test]
fn a_wait_inside_a_lock_waits_in_a_program_with_channels() {
    // TZ06: the lock of a program that uses the tzroutine runtime is the G's, so a channel
    // operation may wait inside it (Go's sync.Mutex); the thread's mark of TZ03 is gone.
    let module = accepts(
        "let lock = Mutex.create 0i64\nlet ch: Channel<i64> = Channel.make 1\nlet borrowed = ref ch\nMutex.with_lock (ref lock) (\\value -> {\n    Channel.send borrowed 1;\n    deref value\n})\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert!(ir.contains("call i32 @tsuzuri_tz_mutex32_lock(ptr"), "{ir}");
    assert!(
        !ir.contains("tz.mutex.mark") && !ir.contains("tz.routine.unlocked"),
        "{ir}"
    );
}

#[test]
fn programs_without_a_channel_keep_their_ir() {
    // Natively a Mutex, Parallel and a DataChannel use the tzroutine scheduler (TZ12), but not its
    // channels, and no program marks a lock for them.
    for source in [
        "let lock = Mutex.create 0i64\nMutex.with_lock (ref lock) (\\value -> deref value)\n",
        "let results = Parallel.init 4 (\\index -> index * index)\nArray.sum ref results\n",
        "let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = DataChannel.bounded 1\n0\n",
    ] {
        let ir = llvm::emit(&accepts(source), llvm::Entry::Console).unwrap();
        assert!(!ir.contains("tsuzuri_tz_chan_"), "{source}\n{ir}");
        assert!(!ir.contains("tz.mutex.mark"), "{source}");
    }
}

#[test]
fn parallel_work_that_traps_in_a_failure_domain_leaves_the_detector_working() {
    // Inside Domain.run every chunk of Parallel runs on the calling thread (TZ12). A trap in a chunk,
    // nested in another one or after a call of parallel work that ended, must leave nothing that
    // holds off the detector: the deadlock after the domain is reported, not a hang. Domain.run
    // needs a POSIX build (Windows: E1038).
    if !cfg!(unix) {
        return;
    }
    for (name, item) in [
        ("nested-batches", "inner (n + index)"),
        (
            "nested-items",
            "{\n        let base = fine (n + index);\n        let zero = n - n;\n        base / zero\n    }",
        ),
    ] {
        let directory = project(
            name,
            &format!(
                "def inner :: i64 -> i64\nfn inner n =\n    let results = Parallel.init 2 (\\index -> {{\n        let zero = n - n;\n        index / zero\n    }})\n    results[0]\n\ndef fine :: i64 -> i64\nfn fine n =\n    let results = Parallel.init 2 (\\index -> n + index)\n    results[0]\n\ndef outer :: i64 -> i64\nfn outer n =\n    let results = Parallel.init 8 (\\index -> {item})\n    results[0]\n\nlet failed = match Domain.run (\\() -> outer 3) with\n    | Result.Ok _ -> 0\n    | Result.Error _ -> 1\nlet ch: Channel<i64> = Channel.make 0\nmatch Channel.recv ch with\n| Maybe.Some v -> v + failed\n| Maybe.None -> failed\n"
            ),
        );
        let (status, stderr) = run_bounded(&directory);
        assert!(!status.success(), "{name}");
        assert!(
            stderr.contains("all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)"),
            "{name}: {stderr}"
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }
}

#[test]
fn a_for_loop_receives_with_the_standard_channel() {
    // A module Channel of the program, and two that `using` declarations make visible, do not
    // change what `for v in ch` calls.
    let main = "let ch: std::Channel<i64> = std::Channel.make 2\nstd::Channel.send ch 4\nstd::Channel.close ch\nlet mut total = 0i64\nfor v in ch do\n    total = total + v\ntotal\n";
    let user = |namespace: &str| {
        format!("namespace {namespace}\n\ndef recv :: i64 -> i64\nfn recv x = x\n")
    };
    for (name, files) in [
        (
            "namespace",
            vec![
                ("Main.tz", format!("namespace App\n\n{main}")),
                ("App/Channel.tz", user("App")),
            ],
        ),
        (
            "usings",
            vec![
                ("Main.tz", format!("using A\nusing B\n\n{main}")),
                ("A/Channel.tz", user("A")),
                ("B/Channel.tz", user("B")),
            ],
        ),
    ] {
        let directory = project(name, "");
        for (path, text) in files {
            let path = directory.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("run")
            .arg(&directory)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "4", "{name}");
        std::fs::remove_dir_all(&directory).unwrap();
    }
}

fn project(name: &str, source: &str) -> std::path::PathBuf {
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-channel-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("Main.tz"), source).unwrap();
    directory
}

/// The last JSON diagnostic that the command printed.
fn last_diagnostic(output: &std::process::Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rfind(|line| line.starts_with('{'))
        .unwrap_or_else(|| panic!("no JSON diagnostic in\n{stderr}"));
    serde_json::from_str(line).unwrap()
}

#[test]
fn threads_take_data_channel_and_freestanding_takes_neither() {
    let directory = project(
        "threads",
        "export def f :: i64 -> i64\nfn f n =\n    let ch: Channel<i64> = Channel.make 1\n    Channel.send ch n\n    Channel.length ch\n",
    );
    let output_path = directory.join("out.wasm");
    let output = output_path.to_str().unwrap();
    for (args, kind, text) in [
        (
            vec![
                "--target",
                "wasm32",
                "--wasm-feature",
                "threads",
                "-o",
                output,
            ],
            "option_conflict",
            "Channel cannot be used with --wasm-feature threads; use DataChannel",
        ),
        (
            vec![
                "--freestanding",
                "--allocator",
                "host",
                "--emit",
                "object",
                "-o",
                output,
            ],
            "option_conflict",
            "tzroutine",
        ),
    ] {
        let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("build")
            .arg(&directory)
            .args(&args)
            .arg("--json")
            .output()
            .unwrap();
        assert!(!run.status.success(), "{args:?}");
        let error = last_diagnostic(&run);
        assert_eq!(error["code"], "E2000", "{args:?}: {error}");
        assert_eq!(error["kind"], kind, "{args:?}: {error}");
        assert!(
            error["message"].as_str().unwrap().contains(text),
            "{args:?}: {error}"
        );
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_deadlock_traps_at_the_wait() {
    let directory = project(
        "deadlock",
        "let ch: Channel<i64> = Channel.make 0\nmatch Channel.recv ch with\n| Maybe.Some v -> v\n| Maybe.None -> 0\n",
    );
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(Path::new(&directory))
        .args(["--trap-info", "--json"])
        .output()
        .unwrap();
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)"),
        "{stderr}"
    );
    let error = last_diagnostic(&run);
    assert_eq!(error["code"], "E2005", "{error}");
    assert_eq!(error["span"]["line"], 2, "{error}");
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn direction_handles_send_or_receive_only() {
    let module = accepts(
        "def produce :: own SendOnly<i64> -> unit\nfn produce out =\n    SendOnly.send out 1\n    let _tried = SendOnly.try_send out 2\n    let copy = SendOnly.share out\n    let _len = SendOnly.length copy + SendOnly.capacity copy\n    SendOnly.close out\n\n\
         let ch: Channel<i64> = Channel.make 2\nlet input = Channel.receive_only ch\nproduce (Channel.send_only ch)\nlet mut sum = 0i64\nfor v in input do\n    sum = sum + v\nlet more = ReceiveOnly.share input\nmatch ReceiveOnly.try_recv more with\n| Channel.Received.Disconnected -> sum + ReceiveOnly.length input + ReceiveOnly.capacity more\n| _ -> 0\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    // The three handles share the channel's runtime calls.
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare void @tsuzuri_tz_chan_share(ptr)")
            .count(),
        1
    );
    let message = rejects(
        "let ch: Channel<i64> = Channel.make 1\nlet out = Channel.send_only ch\nSendOnly.recv out\n",
        "E1002",
        "unknown_module_member",
    );
    assert!(message.contains("module 'SendOnly' has no function or union case 'recv'"));
    let message = rejects(
        "let ch: Channel<i64> = Channel.make 1\nlet out = Channel.send_only ch\nChannel.recv out\n",
        "E1003",
        "type_mismatch",
    );
    assert!(message.contains("found SendOnly<i64>"), "{message}");
    for (source, code) in [
        (
            "let ch: Channel<i64> = Channel.make 1\nlet out = Channel.send_only ch\nSendOnly.__release out\n0\n",
            "E1022",
        ),
        (
            "let ch: Channel<i64> = Channel.make 1\nlet input = Channel.receive_only ch\nReceiveOnly.__release input\n0\n",
            "E1022",
        ),
        (
            "let ch: Channel<i64> = Channel.make 1\nlet out = Channel.send_only ch\nlet f: unit -> i64 = \\() -> SendOnly.length out\nf ()\n",
            "E1005",
        ),
        (
            "let ch: Channel<i64> = Channel.make 1\nlet out = Channel.send_only ch\nfor x in out do\n    ()\n0\n",
            "E1005",
        ),
        (
            "def push :: ref SendOnly<i64 -> i64> -> ref [i64] -> unit\nfn push out data =\n    SendOnly.send out (\\x -> x + data[0])\n\n0\n",
            "E1013",
        ),
    ] {
        assert_eq!(analyze(source).expect_err(source).code, code, "{source}");
    }
}

/// Runs the program of `directory` and returns its status and standard error, or fails when it does
/// not end within 60 seconds (a deadlock that the runtime missed).
fn run_bounded(directory: &Path) -> (std::process::ExitStatus, String) {
    let binary = directory.join("program");
    let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(directory)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut child = Command::new(&binary)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut stderr = String::new();
            use std::io::Read;
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut stderr)
                .unwrap();
            return (status, stderr);
        }
        if started.elapsed() > std::time::Duration::from_secs(60) {
            child.kill().unwrap();
            panic!("{} did not end", directory.display());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn the_children_of_a_scope_that_wait_for_ever_are_a_deadlock() {
    // Two children of a Routine.scope wait on a channel that nobody sends on, in a program with a
    // Mutex: the detector sees both children and the scope's wait, and reports them.
    let directory = project(
        "scope-deadlock",
        "let lock = Mutex.create 0i64\nlet _x = Mutex.with_lock (ref lock) (\\value -> deref value)\nlet ch: Channel<i64> = Channel.make 0\nlet results = Routine.scope (ref ch) 2 (\\shared _index -> match Channel.recv shared with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0)\nresults[0]\n",
    );
    let (status, stderr) = run_bounded(&directory);
    assert!(!status.success());
    assert!(
        stderr.contains("all tzroutines are asleep - deadlock!"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

// Domain.run needs a POSIX build: Windows refuses it (E1038).
#[cfg(unix)]
#[test]
fn a_failure_domain_leaves_no_parallel_work_that_a_trap_left() {
    // A chunk of Parallel traps inside Domain.run, where the chunks run on the calling thread
    // (TZ12): the call never ends, and nothing that it left may hold off the verdict of the deadlock
    // after it.
    let directory = project(
        "domain-batches",
        "def explode :: i64 -> i64\nfn explode n =\n    let results = Parallel.init 2 (\\index -> {\n        let zero = n - n;\n        index / zero\n    })\n    results[0]\n\nlet failed = match Domain.run (\\() -> explode 3) with\n    | Result.Ok _ -> 0\n    | Result.Error _ -> 1\nlet ch: Channel<i64> = Channel.make 0\nmatch Channel.recv ch with\n| Maybe.Some v -> v + failed\n| Maybe.None -> failed\n",
    );
    let (status, stderr) = run_bounded(&directory);
    assert!(!status.success());
    assert!(
        stderr.contains("all tzroutines are asleep - deadlock! (1 waiting: Channel.recv)"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn trap_mode_return_refuses_a_channel() {
    let directory = project(
        "trap-return",
        "export def f :: i64 -> i64\nfn f n =\n    let ch: Channel<i64> = Channel.make 1\n    Channel.send ch n\n    Channel.length ch\n",
    );
    let output = directory.join("out.o");
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("build")
        .arg(&directory)
        .args(["--trap-mode", "return", "--emit", "object", "-o"])
        .arg(&output)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!run.status.success());
    let error = last_diagnostic(&run);
    assert_eq!(error["code"], "E2000", "{error}");
    assert_eq!(error["kind"], "option_conflict", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--trap-mode return cannot be combined with Channel"),
        "{error}"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_partial_try_send_borrows_the_channel() {
    // `Channel.try_send ch` is a function that borrows `ch`: it cannot leave the channel's scope,
    // and is fine where the channel lives. Only a full call gives back an item without a borrow.
    rejects(
        "def make :: unit -> (i64 -> Result<unit, Channel.SendError<i64>>)\nfn make _u =\n    let ch: Channel<i64> = Channel.make 1\n    Channel.try_send ch\n\nlet send = make ()\nmatch send 3 with\n| Result.Ok _ -> 1\n| Result.Error _ -> 0\n",
        "E1013",
        "borrow_outlives_block",
    );
    accepts(
        "let ch: Channel<i64> = Channel.make 1\nlet send = Channel.try_send ch\nmatch send 3 with\n| Result.Ok _ -> Channel.length ch\n| Result.Error _ -> 0\n",
    );
}

#[test]
fn objects_link_the_one_thread_channels() {
    // An object for the default WebAssembly targets has the one-thread channels in it (wasm64
    // links as wasm64).
    let directory = project(
        "wasm-object",
        "export def f :: i64 -> i64\nfn f n =\n    let ch: Channel<i64> = Channel.make 1\n    Channel.send ch n\n    Channel.length ch\n",
    );
    for target in ["wasm32", "wasm64"] {
        let output = directory.join(format!("{target}.o"));
        let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("build")
            .arg(&directory)
            .args(["--target", target, "--emit", "object", "-o"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(
            std::fs::read(&output).unwrap().starts_with(b"\0asm"),
            "{target}"
        );
    }
    std::fs::remove_dir_all(&directory).unwrap();
}
