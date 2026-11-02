//! `E1040`: names that the language had and has no longer, with the help and the certain fix that
//! `tsuzuri fix` applies (TZ03: the channel with separate ends is `DataChannel`; TZ12 and TZ13 add
//! their rows to `check::removed_feature`).
use std::path::Path;
use std::process::Command;

use tsuzuri::{analyze, analyze_all};

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
