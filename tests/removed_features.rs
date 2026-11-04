//! `E1040`: names that the language had and has no longer, with the help and the certain fix that
//! `tsuzuri fix` applies (TZ03: the channel with separate ends is `DataChannel`; TZ12 and TZ13 add
//! their rows to `check::removed_feature`).
use std::path::Path;
use std::process::Command;

use tsuzuri::{analyze, analyze_all, analyze_modules};

const HELP: &str = "the channel with separate Sender and Receiver ends was renamed to DataChannel; the name Channel is now Go's two-way channel (Channel.make)";

/// The `E1040` of `source`, its message and the text of its one fix.
fn renamed(source: &str) -> (String, String) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1040", "{source}\n{}", error.message);
    assert_eq!(error.kind, Some("renamed_to_data_channel"), "{source}");
    assert_eq!(error.help.as_deref(), Some(HELP), "{source}");
    assert_eq!(error.fixes.len(), 1, "{source}");
    let fix = &error.fixes[0];
    assert_eq!(fix.applicability.as_str(), "certain", "{source}");
    assert_eq!(fix.edits.len(), 1, "{source}");
    let edit = &fix.edits[0];
    // The edit replaces the whole qualified name that the diagnostic points at.
    assert_eq!(edit.span.start, error.span.start, "{source}");
    assert_eq!(edit.span.end, error.span.end, "{source}");
    (error.message.clone(), edit.text.clone())
}

#[test]
fn names_the_data_channel_names() {
    for (source, message, replacement) in [
        (
            "let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = Channel.bounded 3\n0\n",
            "'Channel.bounded' is now 'DataChannel.bounded'",
            "DataChannel.bounded",
        ),
        (
            "let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = DataChannel.bounded 3\nmatch pair with\n| (sender, _receiver) ->\n    let _other = Channel.clone_sender (ref sender)\n    0\n",
            "'Channel.clone_sender' is now 'DataChannel.clone_sender'",
            "DataChannel.clone_sender",
        ),
        (
            "def f :: ref Channel.Sender<i64> -> i64\nfn f sender = 0\n\n0\n",
            "'Channel.Sender' is now 'DataChannel.Sender'",
            "DataChannel.Sender",
        ),
        (
            "def f :: ref Channel.Receiver<i64> -> i64\nfn f receiver = 0\n\n0\n",
            "'Channel.Receiver' is now 'DataChannel.Receiver'",
            "DataChannel.Receiver",
        ),
        (
            "def f :: ref DataChannel.Sender<i64> -> i64\nfn f sender =\n    match Channel.send sender 1 with\n    | Result.Ok _ -> 1\n    | Result.Error _ -> 0\n\n0\n",
            "'Channel.send' of a DataChannel.Sender is now 'DataChannel.send'",
            "DataChannel.send",
        ),
        (
            "def f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match Channel.recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n",
            "'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'",
            "DataChannel.recv",
        ),
        // The name with spaces in it, in parentheses, and after `|>`.
        (
            "def f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match Channel . recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n",
            "'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'",
            "DataChannel . recv",
        ),
        (
            "def f :: ref DataChannel.Sender<i64> -> i64\nfn f sender =\n    match Channel. send sender 1 with\n    | Result.Ok _ -> 1\n    | Result.Error _ -> 0\n\n0\n",
            "'Channel.send' of a DataChannel.Sender is now 'DataChannel.send'",
            "DataChannel. send",
        ),
        (
            "def f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match (Channel.recv) (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n",
            "'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'",
            "DataChannel.recv",
        ),
        (
            "def f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match (ref receiver) |> Channel.recv with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n",
            "'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'",
            "DataChannel.recv",
        ),
    ] {
        assert_eq!(
            renamed(source),
            (message.to_owned(), replacement.to_owned()),
            "{source}"
        );
    }
}

#[test]
fn reports_every_old_name_in_one_analysis() {
    // An old name is checked as the name that replaced it, so the uses that depend on it are
    // checked too, and each one is reported once.
    let errors = analyze_all(
        "def take :: ref Channel.Receiver<i64> -> i64\nfn take receiver =\n    match Channel.recv receiver with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\nlet pair: (Channel.Sender<i64> * Channel.Receiver<i64>) = Channel.bounded 2\nmatch pair with\n| (sender, receiver) ->\n    let _sent = Channel.send (ref sender) 1\n    take (ref receiver)\n",
    )
    .expect_err("old names");
    let mut found: Vec<(usize, &str)> = errors
        .diagnostics
        .iter()
        .map(|error| {
            assert_eq!(error.code, "E1040", "{}", error.message);
            (error.span.start, error.message.as_str())
        })
        .collect();
    found.sort();
    let messages: Vec<&str> = found.iter().map(|(_, message)| *message).collect();
    assert_eq!(messages.len(), 6, "{messages:?}");
    for name in ["Receiver", "recv", "Sender", "bounded", "send"] {
        assert!(
            messages
                .iter()
                .any(|message| message.contains(&format!("'Channel.{name}'"))),
            "{name}: {messages:?}"
        );
    }
}

#[test]
fn a_definition_of_the_program_wins() {
    // `Channel.Sender` is a case of the program's own union, and `Sender` its own record.
    analyze("union Channel = Sender | Receiver\n\ndef name :: Channel -> i64\nfn name side =\n    match side with\n    | Channel.Sender -> 1\n    | Channel.Receiver -> 2\n\nname Channel.Sender\n")
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    analyze("record Sender { id: i64 }\n\ndef id :: Sender -> i64\nfn id sender = sender.id\n\nid (Sender { id: 4 })\n")
        .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn tsuzuri_fix_writes_the_data_channel_names() {
    // The DataChannel fixture written with the old names becomes the fixture again.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("tests/fixtures/data_channel");
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-removed-features-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&fixture).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let name = path.file_name().unwrap().to_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                directory.join(&name),
                text.replace("DataChannel.", "Channel."),
            )
            .unwrap();
            files.push((name, text));
        }
    }
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("fix")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    for (name, expected) in files {
        let fixed = std::fs::read_to_string(directory.join(&name)).unwrap();
        assert_eq!(fixed, expected, "{}", name.to_string_lossy());
    }
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_function_of_the_program_keeps_its_mismatch() {
    // A function `recv` of the program's own module `Channel` given an end of a DataChannel is a
    // plain mismatch: only the arguments of the builtin become E1040.
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-removed-own-{}", std::process::id()));
    std::fs::create_dir_all(directory.join("App")).unwrap();
    std::fs::write(
        directory.join("App/Channel.tz"),
        "namespace App\n\ndef recv :: ref std::Channel<i64> -> i64\nfn recv ch = std::Channel.length ch\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("Main.tz"),
        "namespace App\n\ndef f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver = Channel.recv (ref receiver)\n\n0\n",
    )
    .unwrap();
    let check = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("check")
        .arg(&directory)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(!check.status.success());
    assert!(stderr.contains("error[E1003]"), "{stderr}");
    assert!(!stderr.contains("E1040"), "{stderr}");
    std::fs::remove_dir_all(&directory).unwrap();
}

/// The `E1040` of a form that TZ12 removed: its kind, message, help, and the text of its certain fix,
/// if it has one.
fn removed_task(source: &str) -> (&'static str, String, String, Option<String>) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1040", "{source}\n{}", error.message);
    let fix = match error.fixes.as_slice() {
        [] => None,
        [fix] => {
            assert_eq!(fix.applicability.as_str(), "certain", "{source}");
            assert_eq!(fix.edits.len(), 1, "{source}");
            let edit = &fix.edits[0];
            assert_eq!(edit.span.start, error.span.start, "{source}");
            assert_eq!(edit.span.end, error.span.end, "{source}");
            Some(edit.text.clone())
        }
        fixes => panic!("{source}: {} fixes", fixes.len()),
    };
    (
        error.kind.expect(source),
        error.message.clone(),
        error.help.clone().expect(source),
        fix,
    )
}

#[test]
fn names_the_removed_task_forms() {
    for (source, kind, message, help, fix) in [
        (
            "let job = task { return 1 }\n0\n",
            "removed_task_expression",
            "'task { }' was removed",
            "run the work directly, or spawn it with 'tzroutine { ... }' and wait with WaitGroup or a Channel",
            None,
        ),
        (
            "def f :: ref Task<i64> -> i64\nfn f job = 0\n\n0\n",
            "removed_task_type",
            "the type 'Task' was removed",
            "return the value directly; to run work concurrently, use 'tzroutine { ... }'",
            None,
        ),
        (
            "let value = Task.run (\\() -> 1)\n0\n",
            "removed_task_api",
            "'Task.run' was removed",
            "call the work directly",
            None,
        ),
        (
            "let values = Task.parallel [1, 2]\n0\n",
            "removed_task_api",
            "'Task.parallel' was removed",
            "use Parallel.init or Parallel.map for data parallelism, or tzroutines with a WaitGroup",
            None,
        ),
        (
            "let values = Task.parallel_results [1, 2]\n0\n",
            "removed_task_api",
            "'Task.parallel_results' was removed",
            "use ErrGroup: ErrGroup.go for each job and ErrGroup.wait for the first error",
            None,
        ),
        (
            "let base = 10i64\nlet values = Task.scope (ref base) 4 (\\shared index -> deref shared + index)\nArray.sum (ref values)\n",
            "removed_task_api",
            "'Task.scope' was removed",
            "use Routine.scope, which takes the same arguments",
            Some("Routine.scope"),
        ),
    ] {
        assert_eq!(
            removed_task(source),
            (
                kind,
                message.to_owned(),
                help.to_owned(),
                fix.map(str::to_owned)
            ),
            "{source}"
        );
    }
}

#[test]
fn a_task_of_the_program_is_its_own() {
    // `task` is an identifier, and the program may define `Task` as a type or a module.
    for source in [
        "let task = 5\ntask + 1\n",
        "record Task { id: i64 }\n\ndef id :: ref Task -> i64\nfn id job = job.id\n\nlet job = Task { id: 4 }\nid (ref job)\n",
    ] {
        analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    }
    analyze_modules(&[
        ("Main.tz", "Task.run 4\n"),
        ("Task.tz", "def run :: i64 -> i64\nfn run n = n + 1\n"),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // A builder `@alias task` of the program names `task { }`.
    analyze_modules(&[
        (
            "Main.tz",
            "def one :: i64 -> i64\nfn one n = task { let! a = n; return a + 1 }\n\none 1\n",
        ),
        (
            "Steps.tc",
            "@alias task\n\ndef Return :: 'a -> 'a\nfn Return value = value\n\ndef Bind :: 'a -> ('a -> 'b) -> 'b\nfn Bind value next = next value\n",
        ),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn tsuzuri_fix_writes_routine_scope() {
    // Only `Task.scope` has a fix: `Routine.scope` takes the same arguments and means the same.
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-removed-task-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let program = "let base = 10i64\nlet values = Task.scope (ref base) 4 (\\shared index -> deref shared + index)\nArray.sum (ref values)\n";
    std::fs::write(directory.join("Main.tz"), program).unwrap();
    let tsuzuri = |arguments: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(arguments)
            .arg(&directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arguments:?}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    tsuzuri(&["fix"]);
    assert_eq!(
        std::fs::read_to_string(directory.join("Main.tz")).unwrap(),
        program.replace("Task.scope", "Routine.scope")
    );
    // 10 + 11 + 12 + 13.
    assert_eq!(tsuzuri(&["run"]), "46\n");
    std::fs::remove_dir_all(&directory).unwrap();
}

/// The `E1040` of a form that TZ13 removed: its kind, message, and help. None has a fix, since no
/// replacement means the same.
fn removed_async(source: &str) -> (&'static str, String, String) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1040", "{source}\n{}", error.message);
    assert!(error.fixes.is_empty(), "{source}");
    (
        error.kind.expect(source),
        error.message.clone(),
        error.help.clone().expect(source),
    )
}

#[test]
fn names_the_removed_async_forms() {
    const EXPRESSION: &str =
        "run the work directly; to wait for I/O while other work runs, spawn tzroutines";
    const TYPE: &str = "return the value directly, or use a Channel to deliver it later";
    const RUN: &str =
        "call the work directly (in a tzroutine, I/O waits without stopping other tzroutines)";
    const ALL: &str =
        "spawn a tzroutine per job and wait with WaitGroup, or with ErrGroup for the first error";
    const CLOCK: &str = "use Time.sleep, Time.monotonic_ns, or Routine.yield";
    const HOST: &str = "the host-driven executor was removed; call exports from host threads (native) or await the export's Promise (WebAssembly with --wasm-feature jspi)";
    for (source, message, help) in [
        (
            "let job = async { return 1 }\n0\n",
            "'async { }' was removed",
            EXPRESSION,
        ),
        (
            "let job = Async { return 1 }\n0\n",
            "'Async { }' was removed",
            EXPRESSION,
        ),
        (
            "def f :: ref Async<i64> -> i64\nfn f job = 0\n\n0\n",
            "the type 'Async' was removed",
            TYPE,
        ),
        (
            "def f :: ref Async.Async<i64> -> i64\nfn f job = 0\n\n0\n",
            "the type 'Async.Async' was removed",
            TYPE,
        ),
        (
            "def f :: ref Async.Operation -> i64\nfn f operation = 0\n\n0\n",
            "the type 'Async.Operation' was removed",
            TYPE,
        ),
        (
            "let operation = Async.Operation { start: \\id -> (), cancel: \\id -> () }\n0\n",
            "the type 'Async.Operation' was removed",
            TYPE,
        ),
        (
            "let value = Async.run 1\n0\n",
            "'Async.run' was removed",
            RUN,
        ),
        (
            "let value = Async.block_on 1\n0\n",
            "'Async.block_on' was removed",
            RUN,
        ),
        (
            "let values = Async.all [1, 2]\n0\n",
            "'Async.all' was removed",
            ALL,
        ),
        (
            "let values = Async.all_results [1, 2]\n0\n",
            "'Async.all_results' was removed",
            ALL,
        ),
        (
            "let done = Async.sleep 10\n0\n",
            "'Async.sleep' was removed",
            CLOCK,
        ),
        (
            "let now = Async.now ()\n0\n",
            "'Async.now' was removed",
            CLOCK,
        ),
        // `yield` after a dot is a member name, as in `Routine.yield`.
        (
            "let next = Async.yield ()\n0\n",
            "'Async.yield' was removed",
            CLOCK,
        ),
        (
            "let started = Async.start 1\n0\n",
            "'Async.start' was removed",
            HOST,
        ),
        (
            "let id = Async.host 1\n0\n",
            "'Async.host' was removed",
            HOST,
        ),
        // The operations of the builder, which a program could call by name.
        (
            "let job = Async.Return 1\n0\n",
            "'Async.Return' was removed",
            EXPRESSION,
        ),
    ] {
        assert_eq!(
            removed_async(source),
            ("removed_async", message.to_owned(), help.to_owned()),
            "{source}"
        );
    }
}

#[test]
fn names_the_removed_net_async_operations() {
    const HELP: &str = "call Net.connect (and the others without '_async') in a tzroutine; it waits without stopping other tzroutines";
    let address = "let address = Maybe.get (Net.parse_address \"127.0.0.1:0\")\n";
    for (name, arguments) in [
        ("connect_async", "address (Maybe.Some 1000)"),
        ("accept_async", "1 Maybe.None"),
        ("read_async", "1 4096 Maybe.None"),
        ("write_async", "1 [1ubyte] Maybe.None"),
        ("recv_from_async", "1 1500 Maybe.None"),
        ("send_to_async", "1 [1ubyte] address"),
        ("bind_async", "address"),
        ("bind_udp_async", "address"),
        ("close_async", "1"),
        ("close_listener_async", "1"),
        ("close_udp_async", "1"),
        ("shutdown_async", "1 Net.Write"),
    ] {
        let source = format!("{address}let result = Net.{name} {arguments}\n0\n");
        assert_eq!(
            removed_async(&source),
            (
                "removed_net_async",
                format!("'Net.{name}' was removed"),
                HELP.to_owned()
            ),
            "{source}"
        );
        // The diagnostic covers the qualified name.
        let error = analyze(&source).unwrap_err();
        let start = source.find(&format!("Net.{name}")).unwrap();
        assert_eq!(
            (error.span.start, error.span.end),
            (start, start + "Net.".len() + name.len()),
            "{source}"
        );
    }
}

#[test]
fn an_async_of_the_program_is_its_own() {
    // `async` is an identifier, and the program may define `Async` as a type or a module.
    for source in [
        "let async = 5\nasync + 1\n",
        "def async :: i64 -> i64\nfn async n = n + 1\n\nasync 4\n",
        "record Async { id: i64 }\n\ndef id :: ref Async -> i64\nfn id job = job.id\n\nlet job = Async { id: 4 }\nid (ref job)\n",
    ] {
        analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    }
    // A module `Async` of the program: its functions and types are found first.
    analyze_modules(&[
        (
            "Main.tz",
            "let operation = Async.Operation { id: 2 }\nAsync.run (Async.start operation)\n",
        ),
        (
            "Async.tz",
            "record Operation { id: i64 }\n\ndef start :: Operation -> i64\nfn start operation = operation.id\n\ndef run :: i64 -> i64\nfn run n = n + 1\n",
        ),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    let builder = "def Return :: 'a -> 'a\nfn Return value = value\n\ndef Bind :: 'a -> ('a -> 'b) -> 'b\nfn Bind value next = next value\n";
    // A builder `@alias async` of the program names `async { }`.
    analyze_modules(&[
        (
            "Main.tz",
            "def one :: i64 -> i64\nfn one n = async { let! a = n; return a + 1 }\n\none 1\n",
        ),
        ("Steps.tc", &format!("@alias async\n\n{builder}")),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // A builder `Async.tc` of the program names `Async { }`.
    analyze_modules(&[
        (
            "Main.tz",
            "def one :: i64 -> i64\nfn one n = Async { let! a = n; return a + 1 }\n\none 1\n",
        ),
        ("Async.tc", builder),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // A module `Async` of the program that is no builder is not the removed one: `Async { }` is
    // the error of any other module.
    let error = analyze_modules(&[
        ("Main.tz", "let job = Async { return 1 }\n0\n"),
        ("Async.tz", "def run :: i64 -> i64\nfn run n = n + 1\n"),
    ])
    .unwrap_err();
    assert_eq!(
        error.kind,
        Some("unknown_computation_builder"),
        "{}",
        error.message
    );
}
