//! `select` (TZ04): its syntax, types, ownership and generated code. The schedules are in
//! `node tests/features.mjs <bin> select` and `select_single`.
use tsuzuri::{analyze, formatter::format_source, llvm, parser, syntax::SourceKind};

const CHANNELS: &str =
    "let a: Channel<i64> = Channel.make 1\nlet b: Channel<i64> = Channel.make 1\n";

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

fn entry_debug(source: &str) -> String {
    let program = parser::parse(source).unwrap_or_else(|e| panic!("{source}\n{}", e.message));
    format!("{:?}", program.entry)
}

#[test]
fn parses_select_arms() {
    let parsed = entry_debug(&format!(
        "{CHANNELS}let go = true\nselect\n| recv x from a when go -> 1\n| default -> 0\n| send 2 to b -> 2\n"
    ));
    assert!(parsed.contains("Select("), "{parsed}");
    assert!(parsed.contains("Recv {") && parsed.contains("Send {") && parsed.contains("Default"));
    // `select` stays an identifier unless `|` and an arm's word follow it.
    for source in [
        "def select :: i64 -> i64 = \\x -> x\nlet xs = 1\nselect xs\n",
        "let select = 1\nselect + 1\n",
        "def f :: i64 -> i64 = \\x -> x\nlet select = 1\nf select\n",
        "def f :: i64 -> i64 = \\x -> x\nlet select = 1\nselect |> f\n",
        "union U = A | B\nlet select = 1\nlet v = U.A\nmatch v with\n| U.A -> select\n| U.B -> 0\n",
    ] {
        assert!(!entry_debug(source).contains("Select("), "{source}");
        analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    }
}

#[test]
fn rejects_bad_arms() {
    let message = rejects(
        &format!("{CHANNELS}select\n| recv x from a -> 1\n| x -> 2\n"),
        "E0002",
        "select_arm_expected",
    );
    assert!(message.contains("a select arm starts with 'recv NAME from CHANNEL'"));
    rejects(
        &format!("{CHANNELS}select\n| recv x from a -> 1\n| default -> 2\n| default -> 3\n"),
        "E0002",
        "select_default_repeated",
    );
    rejects(
        &format!("{CHANNELS}select\n| recv x from a -> 1\n| default when true -> 2\n"),
        "E0002",
        "select_arm_expected",
    );
    rejects(
        &format!("{CHANNELS}select\n| recv x a -> 1\n"),
        "E0002",
        "select_arm_expected",
    );
    let mut many = String::from(CHANNELS);
    many.push_str("select\n");
    for _ in 0..65_536 {
        many.push_str("| recv _ from a -> 0\n");
    }
    rejects(&many, "E1017", "select_arm_limit");
}

#[test]
fn formats_select() {
    let source = format!(
        "{CHANNELS}let r = select\n    |   recv x from a when true ->  1\n    | send 2 to b -> 2\n    | default -> 3\nr\n"
    );
    let formatted = format_source("Main.tz", &source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(
        formatted.contains("    | recv x from a when true -> 1\n    | send 2 to b -> 2\n"),
        "{formatted}"
    );
    let again = format_source("Main.tz", &formatted, SourceKind::Code).unwrap();
    assert!(!again.changed, "{formatted}");
}

#[test]
fn types_select() {
    // The bodies take one type, a receive gives a Maybe, and the handles of TZ03 Phase 2 work.
    accepts(&format!(
        "{CHANNELS}let out = Channel.send_only a\nlet input = Channel.receive_only b\nlet r: i64 = select\n| recv x from input -> match x with | Maybe.Some v -> v | Maybe.None -> 0\n| send 3 to out -> 3\n| default -> 0\nr\n"
    ));
    for (source, code, kind) in [
        (
            "let r = select\n| recv x from 5 -> 1\n| default -> 0\nr\n",
            "E1003",
            "type_mismatch",
        ),
        (
            &format!("{CHANNELS}select\n| recv x from a when 1 -> 1\n| default -> 0\n") as &str,
            "E1003",
            "type_mismatch",
        ),
        (
            &format!("{CHANNELS}select\n| recv x from a -> 1\n| default -> \"no\"\n"),
            "E1003",
            "type_mismatch",
        ),
        (
            &format!("{CHANNELS}select\n| send \"text\" to a -> 1\n| default -> 0\n"),
            "E1003",
            "type_mismatch",
        ),
        (
            &format!(
                "{CHANNELS}let out = Channel.send_only a\nselect\n| recv x from out -> 1\n| default -> 0\n"
            ),
            "E1003",
            "type_mismatch",
        ),
        (
            "let pair: (DataChannel.Sender<i64> * DataChannel.Receiver<i64>) = DataChannel.bounded 1\nmatch pair with\n| (_s, r) ->\n    select\n    | recv x from r -> 1\n    | default -> 0\n",
            "E1003",
            "type_mismatch",
        ),
        (
            "def make :: unit -> Channel<i64>\nfn make _unit = Channel.make 1\n\nselect\n| recv x from make () -> 1\n| default -> 0\n",
            "E1013",
            "borrow_of_temporary",
        ),
    ] {
        let error = analyze(source).expect_err(source);
        assert_eq!(
            (error.code, error.kind),
            (code, Some(kind)),
            "{source}\n{}",
            error.message
        );
    }
}

#[test]
fn the_value_of_a_send_goes_into_the_select() {
    rejects(
        "let a: Channel<string> = Channel.make 1\nlet text = \"abc\"\nselect\n| send text to a -> ()\n| default -> ()\nString.length text\n",
        "E1012",
        "use_after_move",
    );
    rejects(
        "def push :: ref Channel<i64 -> i64> -> ref [i64] -> unit\nfn push ch data =\n    select\n    | send (\\x -> x + data[0]) to ch -> ()\n    | default -> ()\n\n0\n",
        "E1013",
        "channel_item_holds_borrow",
    );
    // A function of copies is an item.
    accepts(
        "let a: Channel<i64 -> i64> = Channel.make 1\nlet k = 2i64\nselect\n| send (\\x -> x + k) to a -> ()\n| default -> ()\n0\n",
    );
}

#[test]
fn a_select_keeps_its_cases_in_the_frame() {
    let module = accepts(&format!(
        "{CHANNELS}let mut sum = 0i64\nfor i in 0 ..< 10 do\n    select\n    | send (i as i64) to a -> ()\n    | recv x from a -> match x with | Maybe.Some v -> sum = sum + v | Maybe.None -> ()\nsum\n"
    ));
    let ir = llvm::emit(&module, llvm::Entry::Console).unwrap();
    assert_eq!(
        ir.lines()
            .filter(|line| *line == "declare i64 @tsuzuri_tz_select(ptr, i64, i32, ptr)")
            .count(),
        1
    );
    // The cases are an alloca of the entry block, so a select in a loop does not grow the frame.
    let cases = ir
        .lines()
        .position(|line| line.contains("alloca [2 x { ptr, ptr, i32, i32, [6 x i64] }]"))
        .expect("the cases");
    let lines: Vec<&str> = ir.lines().collect();
    let define = lines[..cases]
        .iter()
        .rposition(|line| line.starts_with("define "))
        .unwrap();
    assert!(
        lines[define + 2..cases]
            .iter()
            .all(|line| !line.ends_with(':')),
        "the cases are in the entry block"
    );
}
