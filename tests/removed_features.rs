//! `E1040`: names that the language had and has no longer, with the help and the certain fix that
//! `tsuzuri fix` applies (TZ03: the channel with separate ends is `DataChannel`; TZ12 and TZ13 add
//! their rows to `check::removed_feature`; E17: the functions of `IO` named with `ln` and `err`).
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
        // E18: `std::` before `Channel` stays in the message and the fix.
        (
            "def f :: own DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match std::Channel.recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n",
            "'std::Channel.recv' of a DataChannel.Receiver is now 'std::DataChannel.recv'",
            "std::DataChannel.recv",
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
    // A module `Async` that the expression's module does not see changes nothing: `Async { }` is
    // still the removed one.
    let error = analyze_modules(&[
        (
            "Main.tz",
            "let job = Async { return 1 }
0
",
        ),
        (
            "Other/Async.tz",
            "namespace Other

def run :: i64 -> i64
fn run n = n + 1
",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(error.kind, Some("removed_async"), "{}", error.message);
}

/// E17: the old names of the functions of `IO` and the names that replaced them.
const RENAMED_IO: [(&str, &str); 8] = [
    ("try_read_line", "try_readln"),
    ("read_line", "readln"),
    ("try_write_line", "try_writeln"),
    ("try_write_error", "try_write_err"),
    ("try_write_error_line", "try_write_errln"),
    ("write_line", "writeln"),
    ("write_error", "write_err"),
    ("write_error_line", "write_errln"),
];

const IO_HELP: &str = "the line and stderr functions of IO use 'ln' and 'err' (IO.readln, IO.writeln, IO.write_err, IO.write_errln, and their 'try_' forms)";

/// The argument of a call of the `IO` function `name`.
fn io_argument(name: &str) -> &'static str {
    if name.contains("read") { "()" } else { "\"x\"" }
}

/// The `E1040` of the renamed `IO` function in `source`, written as `written` (`IO.write_line`,
/// `std::IO.write_line`, `IO . write_line`): its message, the title of its one certain fix, and
/// `source` with the fix applied. The diagnostic covers the whole qualified name, and the edit
/// the function's name alone.
fn renamed_io(source: &str, written: &str) -> (String, String, String) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1040", "{source}\n{}", error.message);
    assert_eq!(error.kind, Some("renamed_io_function"), "{source}");
    assert_eq!(error.help.as_deref(), Some(IO_HELP), "{source}");
    let start = source.find(written).expect("the old name");
    assert_eq!(
        (error.span.start, error.span.end),
        (start, start + written.len()),
        "{source}"
    );
    assert_eq!(error.fixes.len(), 1, "{source}");
    let fix = &error.fixes[0];
    assert_eq!(fix.applicability.as_str(), "certain", "{source}");
    assert_eq!(fix.edits.len(), 1, "{source}");
    let edit = &fix.edits[0];
    let old = written.rsplit(['.', ' ']).next().unwrap();
    assert_eq!(
        (edit.span.start, edit.span.end),
        (start + written.len() - old.len(), start + written.len()),
        "{source}"
    );
    let fixed = format!(
        "{}{}{}",
        &source[..edit.span.start],
        edit.text,
        &source[edit.span.end..]
    );
    (error.message.clone(), fix.title.clone(), fixed)
}

#[test]
fn names_the_renamed_io_functions() {
    for (old, new) in RENAMED_IO {
        let argument = io_argument(old);
        let source = format!("let action = IO.{old} {argument}\n0\n");
        let (message, title, fixed) = renamed_io(&source, &format!("IO.{old}"));
        assert_eq!(message, format!("'IO.{old}' is now 'IO.{new}'"), "{source}");
        assert_eq!(title, format!("write 'IO.{new}'"), "{source}");
        assert_eq!(fixed, format!("let action = IO.{new} {argument}\n0\n"));
        // The fixed source is a program of the new names.
        analyze(&fixed).unwrap_or_else(|e| panic!("{fixed}\n{}: {}", e.code, e.message));
    }
}

#[test]
fn the_io_fix_keeps_the_module_as_written() {
    // `std::IO`, spaces around the dot, parentheses, and a pipeline: the fix rewrites the
    // function's name alone.
    for (source, written, message, title, expected) in [
        (
            "let action = std::IO.write_line \"x\"\n0\n",
            "std::IO.write_line",
            "'std::IO.write_line' is now 'std::IO.writeln'",
            "write 'std::IO.writeln'",
            "let action = std::IO.writeln \"x\"\n0\n",
        ),
        (
            "let action = IO . write_line \"x\"\n0\n",
            "IO . write_line",
            "'IO.write_line' is now 'IO.writeln'",
            "write 'IO.writeln'",
            "let action = IO . writeln \"x\"\n0\n",
        ),
        (
            "let action = (IO.write_error_line) \"x\"\n0\n",
            "IO.write_error_line",
            "'IO.write_error_line' is now 'IO.write_errln'",
            "write 'IO.write_errln'",
            "let action = (IO.write_errln) \"x\"\n0\n",
        ),
        (
            "let action = \"x\" |> IO.try_write_error\n0\n",
            "IO.try_write_error",
            "'IO.try_write_error' is now 'IO.try_write_err'",
            "write 'IO.try_write_err'",
            "let action = \"x\" |> IO.try_write_err\n0\n",
        ),
        (
            "let action = std::IO . read_line ()\n0\n",
            "std::IO . read_line",
            "'std::IO.read_line' is now 'std::IO.readln'",
            "write 'std::IO.readln'",
            "let action = std::IO . readln ()\n0\n",
        ),
    ] {
        let (found, fix_title, fixed) = renamed_io(source, written);
        assert_eq!(found, message, "{source}");
        assert_eq!(fix_title, title, "{source}");
        assert_eq!(fixed, expected, "{source}");
        analyze(&fixed).unwrap_or_else(|e| panic!("{fixed}\n{}: {}", e.code, e.message));
    }
}

/// A `main` that uses every renamed function of `IO` under the names that `name` gives.
fn io_program(name: impl Fn(&str) -> String) -> String {
    format!(
        "def main :: unit -> i32 = \\() ->\n    let! line = {} ()\n    let! tried = {} ()\n    do! {} (Maybe.default_value \"\" line)\n    do! {} (Result.is_ok tried)\n    do! {} \"a\"\n    do! {} \"b\"\n    let! one = {} \"c\"\n    let! two = {} \"d\"\n    let! three = {} \"e\"\n    do! IO.writeln (Result.is_ok one && Result.is_ok two && Result.is_ok three)\n    0\n",
        name("read_line"),
        name("try_read_line"),
        name("write_line"),
        name("write_line"),
        name("write_error"),
        name("write_error_line"),
        name("try_write_line"),
        name("try_write_error"),
        name("try_write_error_line"),
    )
}

fn new_io_name(old: &str) -> &'static str {
    RENAMED_IO
        .iter()
        .find(|(from, _)| *from == old)
        .map(|(_, to)| *to)
        .expect("a renamed IO function")
}

#[test]
fn reports_every_old_io_name_in_one_analysis() {
    // Recovery checks each old name as the new one, so one analysis reports all nine uses, and
    // nothing else: the values they give are checked with the new functions' types.
    let source = io_program(|old| {
        if old == "write_error" {
            format!("std::IO.{old}")
        } else {
            format!("IO.{old}")
        }
    });
    let errors = analyze_all(&source).expect_err("old names");
    let mut found: Vec<(usize, String)> = errors
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.starts_with('E'))
        .map(|error| {
            assert_eq!(error.code, "E1040", "{}", error.message);
            assert_eq!(error.kind, Some("renamed_io_function"), "{}", error.message);
            (error.span.start, error.message.clone())
        })
        .collect();
    found.sort();
    let messages: Vec<String> = found.into_iter().map(|(_, message)| message).collect();
    let expected: Vec<String> = [
        "read_line",
        "try_read_line",
        "write_line",
        "write_line",
        "write_error",
        "write_error_line",
        "try_write_line",
        "try_write_error",
        "try_write_error_line",
    ]
    .iter()
    .map(|old| {
        let module = if *old == "write_error" {
            "std::IO"
        } else {
            "IO"
        };
        format!("'{module}.{old}' is now '{module}.{}'", new_io_name(old))
    })
    .collect();
    assert_eq!(messages, expected, "{source}");
    // The program of the new names is accepted.
    let renamed = io_program(|old| format!("IO.{}", new_io_name(old)));
    analyze(&renamed).unwrap_or_else(|e| panic!("{renamed}\n{}: {}", e.code, e.message));
}

#[test]
fn tsuzuri_fix_writes_the_new_io_names() {
    // One round of `tsuzuri fix` moves a project to the new names, with the module and the spaces
    // as written, and the project then checks.
    let directory = std::env::temp_dir().join(format!("tsuzuri-removed-io-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let old = io_program(|old| match old {
        "write_error" => format!("std::IO.{old}"),
        "write_error_line" => format!("IO . {old}"),
        _ => format!("IO.{old}"),
    });
    let new = io_program(|old| match old {
        "write_error" => format!("std::IO.{}", new_io_name(old)),
        "write_error_line" => format!("IO . {}", new_io_name(old)),
        _ => format!("IO.{}", new_io_name(old)),
    });
    std::fs::write(directory.join("Main.tz"), &old).unwrap();
    std::fs::write(
        directory.join("Other.tz"),
        "def greet :: unit -> IO<unit>\nfn greet _unit = \"hi\" |> IO.write_line\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("fix")
        .arg(&directory)
        .arg("--json")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    // Every fix is applied in the first round, and the second analysis finds nothing to fix.
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect(line))
        .collect();
    let fixes: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| event["event"] == "fix")
        .collect();
    assert_eq!(fixes.len(), 10, "{stdout}");
    for fix in fixes {
        assert_eq!(fix["status"], "applied", "{stdout}");
        assert_eq!(fix["round"], 1, "{stdout}");
        assert_eq!(fix["code"], "E1040", "{stdout}");
        assert_eq!(fix["kind"], "renamed_io_function", "{stdout}");
    }
    let summary = events.last().expect("a summary");
    assert_eq!(summary["event"], "summary", "{stdout}");
    assert_eq!(summary["rounds"], 1, "{stdout}");
    assert_eq!(summary["stop"], "fixed_point", "{stdout}");
    assert_eq!(summary["applied"], 10, "{stdout}");
    assert_eq!(summary["files"], 2, "{stdout}");
    assert_eq!(
        std::fs::read_to_string(directory.join("Main.tz")).unwrap(),
        new
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("Other.tz")).unwrap(),
        "def greet :: unit -> IO<unit>\nfn greet _unit = \"hi\" |> IO.writeln\n"
    );
    let check = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("check")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_module_io_of_the_program_keeps_its_unknown_member() {
    // A module `IO` of the program's namespace is not the std one: a function it lacks is the
    // E1002 of any module, written bare or with the namespace. `std::IO` still names the std one.
    let own = (
        "App/IO.tz",
        "namespace App\n\ndef hello :: unit -> i64\nfn hello _unit = 1\n",
    );
    for (call, module) in [
        ("IO.write_line \"x\"", "IO"),
        ("App::IO.read_line ()", "App::IO"),
        ("IO.try_write_error_line \"x\"", "IO"),
    ] {
        let main = format!("namespace App\n\nlet action = {call}\n0\n");
        let error = analyze_modules(&[("Main.tz", &main), own]).unwrap_err();
        assert_eq!(error.code, "E1002", "{main}\n{}", error.message);
        assert_eq!(error.kind, Some("unknown_module_member"), "{main}");
        assert!(
            error
                .message
                .starts_with(&format!("module '{module}' has no function")),
            "{main}\n{}",
            error.message
        );
    }
    let main = "namespace App\n\nlet action = std::IO.write_line \"x\"\n0\n";
    let error = analyze_modules(&[("Main.tz", main), own]).unwrap_err();
    assert_eq!(error.kind, Some("renamed_io_function"), "{}", error.message);
    // A module `IO` of the program that defines the old name has it.
    analyze_modules(&[
        ("Main.tz", "namespace App\n\nIO.write_line 4\n"),
        (
            "App/IO.tz",
            "namespace App\n\ndef write_line :: i64 -> i64\nfn write_line n = n + 1\n",
        ),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
}

#[test]
fn old_names_with_std_report_e1040_as_written() {
    // E18: `std::` before the module of an old name gives the E1040 of the bare name, and the
    // message and the fix keep `std::`.
    for (source, written, kind, message, replacement) in [
        (
            "let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = std::Channel.bounded 3\n0\n",
            "std::Channel.bounded",
            "renamed_to_data_channel",
            "'std::Channel.bounded' is now 'std::DataChannel.bounded'",
            Some("std::DataChannel.bounded"),
        ),
        (
            "def f :: ref std::Channel.Sender<i64> -> i64\nfn f _sender = 0\n\n0\n",
            "std::Channel.Sender",
            "renamed_to_data_channel",
            "'std::Channel.Sender' is now 'std::DataChannel.Sender'",
            Some("std::DataChannel.Sender"),
        ),
        (
            "let connect = std::Net.connect_async\n0\n",
            "std::Net.connect_async",
            "removed_net_async",
            "'std::Net.connect_async' was removed",
            None,
        ),
        (
            "let run = std::Task.run\n0\n",
            "std::Task.run",
            "removed_task_api",
            "'std::Task.run' was removed",
            None,
        ),
        (
            "let base = 10i64\nlet values = std::Task.scope (ref base) 4 (\\shared index -> deref shared + index)\nArray.sum (ref values)\n",
            "std::Task.scope",
            "removed_task_api",
            "'std::Task.scope' was removed",
            Some("std::Routine.scope"),
        ),
        (
            "let run = std::Async.run\n0\n",
            "std::Async.run",
            "removed_async",
            "'std::Async.run' was removed",
            None,
        ),
        (
            "def f :: ref std::Async.Operation -> i64\nfn f _operation = 0\n\n0\n",
            "std::Async.Operation",
            "removed_async",
            "the type 'std::Async.Operation' was removed",
            None,
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(error.code, "E1040", "{source}\n{}", error.message);
        assert_eq!(error.kind, Some(kind), "{source}");
        assert_eq!(error.message, message, "{source}");
        // The help of the name written without `std::`.
        let bare = analyze(&source.replacen("std::", "", 1)).expect_err(source);
        assert_eq!(bare.code, "E1040", "{source}");
        assert!(error.help.is_some(), "{source}");
        assert_eq!(error.help, bare.help, "{source}");
        let start = source.find(written).expect("the old name");
        assert_eq!(
            (error.span.start, error.span.end),
            (start, start + written.len()),
            "{source}"
        );
        match replacement {
            Some(text) => {
                assert_eq!(error.fixes.len(), 1, "{source}");
                let fix = &error.fixes[0];
                assert_eq!(fix.applicability.as_str(), "certain", "{source}");
                assert_eq!(fix.title, format!("write '{text}'"), "{source}");
                assert_eq!(fix.edits.len(), 1, "{source}");
                assert_eq!(fix.edits[0].text, text, "{source}");
                assert_eq!(
                    (fix.edits[0].span.start, fix.edits[0].span.end),
                    (error.span.start, error.span.end),
                    "{source}"
                );
            }
            None => assert!(error.fixes.is_empty(), "{source}"),
        }
    }
    // The same names without `std::` keep their messages.
    let error = analyze("let run = Task.run\n0\n").unwrap_err();
    assert_eq!(error.message, "'Task.run' was removed");
}

#[test]
fn a_module_of_the_program_with_an_old_module_name_is_its_own() {
    // E18: a module `Channel`, `Net`, `Task` or `Async` of the program's namespace is not the std
    // one (or the removed one): a name that it lacks is the E1002 or E1004 of any module, without
    // a fix, and a name that it defines is used. `std::Channel` still names the std module.
    let own = |module: &str| {
        (
            format!("App/{module}.tz"),
            "namespace App\n\nrecord Thing { id: i64 }\n\ndef bounded :: i64 -> i64\nfn bounded n = n + 1\n\ndef hello :: unit -> i64\nfn hello _unit = 1\n"
                .to_owned(),
        )
    };
    for (module, body, code, kind) in [
        (
            "Channel",
            "let a = Channel.clone_sender 3\n0\n",
            "E1002",
            "unknown_module_member",
        ),
        (
            "Net",
            "let a = Net.connect_async 3\n0\n",
            "E1002",
            "unknown_module_member",
        ),
        (
            "Task",
            "let a = Task.run 3\n0\n",
            "E1002",
            "unknown_module_member",
        ),
        (
            "Async",
            "let a = Async.run 3\n0\n",
            "E1002",
            "unknown_module_member",
        ),
        (
            "Channel",
            "def f :: ref Channel.Sender<i64> -> i64\nfn f _sender = 0\n\n0\n",
            "E1004",
            "unknown_type_name",
        ),
        (
            "Async",
            "def f :: ref Async.Operation -> i64\nfn f _operation = 0\n\n0\n",
            "E1004",
            "unknown_type_name",
        ),
    ] {
        let (path, text) = own(module);
        let main = format!("namespace App\n\n{body}");
        let error = analyze_modules(&[("Main.tz", &main), (&path, &text)]).unwrap_err();
        assert_eq!(error.code, code, "{main}\n{}", error.message);
        assert_eq!(error.kind, Some(kind), "{main}\n{}", error.message);
        assert!(error.fixes.is_empty(), "{main}");
        let prefix = if code == "E1002" {
            format!("module '{module}' has no function or union case")
        } else {
            "unknown ".to_owned()
        };
        assert!(
            error.message.starts_with(&prefix),
            "{main}\n{}",
            error.message
        );
        // E1002 points at the member, E1004 at the qualified type name.
        let at = main.find(&format!("{module}.")).unwrap();
        let member: String = main[at + module.len() + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let end = at + module.len() + 1 + member.len();
        let start = if code == "E1002" {
            at + module.len() + 1
        } else {
            at
        };
        assert_eq!((error.span.start, error.span.end), (start, end), "{main}");
    }
    // A type that the module defines is used, also one that the std module had.
    analyze_modules(&[
        (
            "Main.tz",
            "namespace App\n\ndef f :: Channel.Sender<i64> -> i64\nfn f sender = sender.id\n\nf (Channel.Sender { id: 3 })\n",
        ),
        ("App/Channel.tz", "namespace App\n\nrecord Sender<'a> { id: 'a }\n"),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    analyze_modules(&[
        (
            "Main.tz",
            "namespace App\n\ndef f :: Async.Operation -> i64\nfn f operation = operation.id\n\nf (Async.Operation { id: 3 })\n",
        ),
        ("App/Async.tz", "namespace App\n\nrecord Operation { id: i64 }\n"),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // A name that the module defines is used, also one that the std module had.
    let (path, text) = own("Channel");
    analyze_modules(&[
        ("Main.tz", "namespace App\n\nChannel.bounded 3\n"),
        (&path, &text),
    ])
    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    // `std::Channel` is the std module beside the program's own `Channel`.
    let main = "namespace App\n\nlet pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = std::Channel.bounded 3\n0\n";
    let error = analyze_modules(&[("Main.tz", main), (&path, &text)]).unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(error.kind, Some("renamed_to_data_channel"));
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.bounded");
}

#[test]
fn tsuzuri_fix_writes_the_data_channel_names_with_std() {
    // E18: one round of `tsuzuri fix` moves the old names written with `std::` and keeps it.
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-removed-std-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let old = "let pair: (std::Channel.Sender<i64> * std::Channel.Receiver<i64>) = std::Channel.bounded 3\nmatch pair with\n| (sender, _receiver) ->\n    let _other = std::Channel.clone_sender (ref sender)\n    0\n";
    std::fs::write(directory.join("Main.tz"), old).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("fix")
        .arg(&directory)
        .arg("--json")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_str(stdout.lines().last().expect("a summary")).unwrap();
    assert_eq!(summary["rounds"], 1, "{stdout}");
    assert_eq!(summary["applied"], 4, "{stdout}");
    assert_eq!(summary["remaining_errors"], 0, "{stdout}");
    assert_eq!(
        std::fs::read_to_string(directory.join("Main.tz")).unwrap(),
        old.replace("std::Channel.", "std::DataChannel.")
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_rename_shows_the_other_errors_of_the_function() {
    // E18 D1: the name that replaced an old one is checked as a whole expression, so the function
    // is not poisoned: the errors that the fixed program has (here E1015 of the unannotated
    // channel) come with the E1040, and `tsuzuri fix` applies the rename instead of refusing an
    // error that it would only reveal.
    let errors = analyze_all("let a = Channel.bounded 3\n0\n").expect_err("errors");
    let mut codes: Vec<&str> = errors.diagnostics.iter().map(|error| error.code).collect();
    codes.sort();
    assert_eq!(codes, ["E1015", "E1040"]);
    let fixed = analyze_all("let a = DataChannel.bounded 3\n0\n").expect_err("errors");
    assert_eq!(
        fixed
            .diagnostics
            .iter()
            .map(|error| error.code)
            .collect::<Vec<_>>(),
        ["E1015"]
    );
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-removed-reveal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("Main.tz"),
        "let a = IO.write_line 3\nlet b = Channel.bounded 3\n0\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("fix")
        .arg(&directory)
        .arg("--json")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let summary: serde_json::Value = serde_json::from_str(
        stdout
            .lines()
            .find(|line| line.contains("\"event\":\"summary\""))
            .expect("a summary"),
    )
    .unwrap();
    assert_eq!(summary["applied"], 2, "{stdout}");
    assert_eq!(summary["remaining_errors"], 1, "{stdout}");
    assert_eq!(
        std::fs::read_to_string(directory.join("Main.tz")).unwrap(),
        "let a = IO.writeln 3\nlet b = DataChannel.bounded 3\n0\n"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn the_replacement_says_std_where_the_program_has_the_module() {
    // E18: where the bare replacement module would name a module of the program (its own
    // `DataChannel` or `Routine`), the message, the recovery and the certain fix say `std::`, so
    // the fixed program calls the std function as the old name did.
    let modules = [
        (
            "App/DataChannel.tz",
            "namespace App\n\nrecord Sender<'a> { value: 'a }\n\ndef bounded :: i64 -> i64\nfn bounded n = n + 1\n",
        ),
        (
            "App/Routine.tz",
            "namespace App\n\ndef scope :: ref i64 -> i64 -> (ref i64 -> i64 -> i64) -> i64\nfn scope _base n _f = n\n",
        ),
    ];
    for (body, message, replacement) in [
        (
            "let pair: (std::DataChannel.Sender<i64> * std::DataChannel.Receiver<i64>) = Channel.bounded 3\n0\n",
            "'Channel.bounded' is now 'std::DataChannel.bounded'",
            "std::DataChannel.bounded",
        ),
        (
            "let base = 10i64\nlet values = Task.scope (ref base) 4 (\\shared index -> deref shared + index)\nArray.sum (ref values)\n",
            "'Task.scope' was removed",
            "std::Routine.scope",
        ),
        (
            "def f :: ref Channel.Sender<i64> -> i64\nfn f _sender = 0\n\n0\n",
            "'Channel.Sender' is now 'std::DataChannel.Sender'",
            "std::DataChannel.Sender",
        ),
    ] {
        let main = format!("namespace App\n\n{body}");
        let mut sources = vec![("Main.tz", main.as_str())];
        sources.extend(modules);
        let error = analyze_modules(&sources).unwrap_err();
        assert_eq!(error.code, "E1040", "{main}\n{}", error.message);
        assert_eq!(error.message, message, "{main}");
        assert_eq!(error.fixes[0].edits[0].text, replacement, "{main}");
        assert_eq!(error.fixes[0].title, format!("write '{replacement}'"));
        // The fixed program is the std one: it checks, beside the program's modules.
        let fixed = format!(
            "{}{}{}",
            &main[..error.fixes[0].edits[0].span.start],
            replacement,
            &main[error.fixes[0].edits[0].span.end..]
        );
        let mut sources = vec![("Main.tz", fixed.as_str())];
        sources.extend(modules);
        analyze_modules(&sources).unwrap_or_else(|e| panic!("{fixed}\n{}: {}", e.code, e.message));
    }
    // Without such a module, the replacement stays bare.
    let error = analyze("let base = 10i64\nlet values = Task.scope (ref base) 4 (\\shared index -> deref shared + index)\nArray.sum (ref values)\n").unwrap_err();
    assert_eq!(error.fixes[0].edits[0].text, "Routine.scope");
    // Two `using` namespaces that each hold a module `DataChannel` make the bare name ambiguous:
    // the fix says `std::` too.
    let main = "namespace App\nusing Lib1\nusing Lib2\n\ndef f :: ref Channel.Sender<i64> -> i64\nfn f _sender = 0\n\n0\n";
    let libraries = [
        (
            "Lib1/DataChannel.tz",
            "namespace Lib1\n\ndef one :: i64 -> i64\nfn one n = n\n",
        ),
        (
            "Lib2/DataChannel.tz",
            "namespace Lib2\n\ndef two :: i64 -> i64\nfn two n = n\n",
        ),
    ];
    let mut sources = vec![("Main.tz", main)];
    sources.extend(libraries);
    let error = analyze_modules(&sources).unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.Sender");
    let fixed = main.replace("Channel.Sender", "std::DataChannel.Sender");
    let mut sources = vec![("Main.tz", fixed.as_str())];
    sources.extend(libraries);
    analyze_modules(&sources).unwrap_or_else(|e| panic!("{fixed}\n{}: {}", e.code, e.message));
    // A namespace `std` of the program with a module `DataChannel` takes `std::DataChannel` from
    // `App`, and the program's own `DataChannel` takes the bare name: no spelling names the std
    // module, so the E1040 has no fix.
    let main = "namespace App\n\nlet pair = Channel.bounded 3\n0\n";
    let error = analyze_modules(&[
        ("Main.tz", main),
        (
            "App/DataChannel.tz",
            "namespace App\n\ndef bounded :: i64 -> i64\nfn bounded n = n\n",
        ),
        (
            "App/std/DataChannel.tz",
            "namespace App::std\n\ndef bounded :: i64 -> i64\nfn bounded n = n\n",
        ),
    ])
    .unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(error.kind, Some("renamed_to_data_channel"));
    assert!(error.fixes.is_empty(), "{:?}", error.fixes);
    // A local binding named like the replacement module takes the bare name in a value
    // position: the fix says `std::`, and the fixed program calls the std function.
    let source = "record Box { bounded: i64 -> i64, scope: i64 -> i64 }\n\ndef f :: unit -> i64\nfn f _unit = {\n    let DataChannel = Box { bounded: \\n -> n + 1, scope: \\n -> n };\n    let pair: (std::DataChannel.Sender<i64> * std::DataChannel.Receiver<i64>) = Channel.bounded 3;\n    0\n}\n\n0\n";
    let error = analyze(source).unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(
        error.message,
        "'Channel.bounded' is now 'std::DataChannel.bounded'"
    );
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.bounded");
    let source = "record Box { scope: i64 -> i64 }\n\ndef f :: unit -> i64\nfn f _unit = {\n    let Routine = Box { scope: \\n -> n };\n    let base = 10i64;\n    let values = Task.scope (ref base) 4 (\\shared index -> deref shared + index);\n    Array.sum (ref values)\n}\n\n0\n";
    let error = analyze(source).unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert_eq!(error.fixes[0].edits[0].text, "std::Routine.scope");
}

#[test]
fn the_channel_call_rewrite_writes_a_data_channel_that_keeps_its_meaning() {
    // E19: `Channel.recv` given an end of a DataChannel becomes `DataChannel.recv`, written so
    // that the name is the std module where the call is: bare as before, `std::` where a module or
    // a local binding of the program takes the bare name, and no fix where neither spelling names
    // the std module.
    let body = "def f :: own std::DataChannel.Receiver<i64> -> i64\nfn f receiver =\n    match Channel.recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n\n0\n";
    let check = |sources: &[(&str, &str)]| {
        let error = analyze_modules(sources).unwrap_err();
        assert_eq!(error.code, "E1040", "{}", error.message);
        assert_eq!(
            error.kind,
            Some("renamed_to_data_channel"),
            "{}",
            error.message
        );
        error
    };
    // A module `DataChannel` of the program's namespace takes the bare name.
    let main = format!("namespace App\n\n{body}");
    let own = (
        "App/DataChannel.tz",
        "namespace App\n\ndef recv :: ref i64 -> i64\nfn recv value = deref value\n",
    );
    let error = check(&[("Main.tz", &main), own]);
    assert_eq!(
        error.message,
        "'Channel.recv' of a DataChannel.Receiver is now 'std::DataChannel.recv'"
    );
    assert_eq!(error.fixes[0].title, "write 'std::DataChannel.recv'");
    let edit = &error.fixes[0].edits[0];
    assert_eq!(edit.text, "std::DataChannel.recv");
    let fixed = format!(
        "{}{}{}",
        &main[..edit.span.start],
        edit.text,
        &main[edit.span.end..]
    );
    analyze_modules(&[("Main.tz", &fixed), own])
        .unwrap_or_else(|e| panic!("{fixed}\n{}: {}", e.code, e.message));
    // A local binding named `DataChannel` takes the bare name too.
    let local = "record Box { recv: i64 }\n\ndef f :: own std::DataChannel.Receiver<i64> -> i64\nfn f receiver = {\n    let DataChannel = Box { recv: 1 };\n    match Channel.recv (ref receiver) with\n    | Maybe.Some v -> v + DataChannel.recv\n    | Maybe.None -> 0\n}\n\n0\n";
    let error = check(&[("Main.tz", local)]);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.recv");
    // A namespace `std` of the program with a module `DataChannel`, beside the program's own
    // `DataChannel`: no spelling names the std module from `App`, so there is no fix. The
    // receiver comes from another namespace, so `Main` does not name `DataChannel` itself.
    let nested = (
        "App/std/DataChannel.tz",
        "namespace App::std\n\ndef recv :: ref i64 -> i64\nfn recv value = deref value\n",
    );
    let make = (
        "Lib/Make.tz",
        "namespace Lib\n\ndef pair :: unit -> (DataChannel.Sender<i64> * DataChannel.Receiver<i64>)\nfn pair _unit = DataChannel.bounded 3\n",
    );
    let main = "namespace App\n\nlet p = Lib::Make.pair ()\nmatch p with\n| (_sender, receiver) ->\n    match Channel.recv (ref receiver) with\n    | Maybe.Some v -> v\n    | Maybe.None -> 0\n";
    let error = check(&[("Main.tz", main), own, nested, make]);
    assert_eq!(
        error.message,
        "'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'"
    );
    assert!(error.fixes.is_empty(), "{:?}", error.fixes);
    // With only the program's `DataChannel`, the same call gets `std::`.
    let error = check(&[("Main.tz", main), own, make]);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.recv");
    // A written `std::` that the program's namespace `std` takes: no fix either.
    let written = main.replace("match Channel.recv", "match std::Channel.recv");
    let error = analyze_modules(&[("Main.tz", &written), nested, make]).unwrap_err();
    assert_eq!(error.code, "E1040", "{}", error.message);
    assert!(error.fixes.is_empty(), "{:?}", error.fixes);
    // `std::Channel.recv` keeps `std::` where `std::DataChannel` is the std module, and has no
    // fix where the program's namespace `std` takes it.
    let written = body.replace("match Channel.recv", "match std::Channel.recv");
    let error = check(&[("Main.tz", &written)]);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.recv");
    // Without such modules or bindings, the bare name stays.
    let error = check(&[("Main.tz", body)]);
    assert_eq!(error.fixes[0].edits[0].text, "DataChannel.recv");
    // The pipeline form and `Channel.send` choose the same way, with a module and with a local
    // binding of the program.
    let piped = body.replace(
        "match Channel.recv (ref receiver)",
        "match (ref receiver) |> Channel.recv",
    );
    let main = format!("namespace App\n\n{piped}");
    let error = check(&[("Main.tz", &main), own]);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.recv");
    let local_piped = local.replace(
        "match Channel.recv (ref receiver)",
        "match (ref receiver) |> Channel.recv",
    );
    let error = check(&[("Main.tz", &local_piped)]);
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.recv");
    let send = "def f :: ref std::DataChannel.Sender<i64> -> i64\nfn f sender =\n    match Channel.send sender 1 with\n    | Result.Ok _ -> 1\n    | Result.Error _ -> 0\n\n0\n";
    let main = format!("namespace App\n\n{send}");
    let error = check(&[("Main.tz", &main), own]);
    assert_eq!(
        error.message,
        "'Channel.send' of a DataChannel.Sender is now 'std::DataChannel.send'"
    );
    assert_eq!(error.fixes[0].edits[0].text, "std::DataChannel.send");
    let error = check(&[("Main.tz", send)]);
    assert_eq!(error.fixes[0].edits[0].text, "DataChannel.send");
}
