//! A21 Phase 5: `tsuzuri prove`. The verdicts come from z3 when it is installed (the tests that
//! need it print `skipped: z3 not found` otherwise); the solver's own protocol is checked with
//! fake solvers. The translation of the integer operators is compared with native execution.

use std::path::{Path, PathBuf};
use std::process::Command;

use tsuzuri::syntax::BinaryOp;

fn z3() -> bool {
    Command::new("z3").arg("--version").output().is_ok()
}

fn project(name: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tsuzuri-prove-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

fn prove(arguments: &[&str], root: &Path, solver: Option<&Path>) -> (i32, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command.arg("prove").args(arguments).arg(root);
    if let Some(solver) = solver {
        command.env("TSUZURI_SMT", solver);
    }
    let output = command.output().unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const SOURCE: &str = "def clamp :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r <= hi\n    if x < lo then lo elif x > hi then hi else x\n\
def bad :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r < hi\n    if x < lo then lo elif x > hi then hi else x\n\
def half :: i32 -> i32 -> i32 = \\a b ->\n    @requires b != 0i32\n    a / b\n\
def sum_to :: i64 -> i64 = \\n ->\n    @requires n >= 0\n    let mut total = 0\n    for i in 0 ..= n do\n        total = total + i\n    total\n\
def main :: unit -> i32 = \\() -> 0\n";

#[test]
fn proves_refutes_and_rejects() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    let root = project("verdicts", SOURCE);
    let (code, stdout, stderr) = prove(&[], &root, None);
    assert_eq!(code, 1);
    assert_eq!(
        stdout,
        "proved Main.clamp\ncounterexample Main.bad\ncounterexample Main.half\nunsupported Main.sum_to (a loop)\n"
    );
    assert!(
        stderr.contains("'@ensures \\r -> lo <= r && r < hi' of 'Main.bad' can fail: Main.bad "),
        "{stderr}"
    );
    assert!(stderr.contains("'the body' of 'Main.half' can fail: Main.half (-2147483648) (-1) traps: integer division overflow"), "{stderr}");
    assert!(
        stderr.contains("cannot prove 'Main.sum_to': a loop"),
        "{stderr}"
    );
    let (code, stdout, _) = prove(&["--function", "Main.clamp", "--json"], &root, None);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "{\"type\":\"prove\",\"function\":\"Main.clamp\",\"result\":\"proved\",\"clauses\":[{\"kind\":\"trap_free\",\"result\":\"proved\"},{\"kind\":\"ensures\",\"index\":0,\"result\":\"proved\"}]}\n{\"type\":\"summary\",\"proved\":1,\"failed\":0}\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn counterexample_breaks_the_contract_natively() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    let root = project("native", SOURCE);
    let (_, _, stderr) = prove(&["--function", "Main.bad"], &root, None);
    let call = stderr
        .split("can fail: Main.bad ")
        .nth(1)
        .and_then(|rest| rest.split(" returns").next())
        .unwrap_or_else(|| panic!("{stderr}"))
        .to_owned();
    let replay = project(
        "replay",
        &SOURCE.replace(
            "def main :: unit -> i32 = \\() -> 0\n",
            &format!("def main :: unit -> i32 = \\() -> (bad {call}) as i32\n"),
        ),
    );
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(&replay)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("trap: contract ensures failed"),
        "{call}"
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(replay);
}

/// Review of #64: an inlined callee numbers its locals in its own function; they must not
/// overwrite the caller's, or a false postcondition is proved. Native execution agrees.
#[test]
fn inlined_callee_locals_keep_the_callers() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    let source = "def helper :: i64 -> i64 = \\a ->\n    let z = 0\n    z + a - a\n\
                  def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r == 0\n    let y = x\n    let w = helper 1\n    y + w\n\
                  def main :: unit -> i32 = \\() -> (f 7) as i32\n";
    let root = project("locals", source);
    let (code, stdout, stderr) = prove(&["--function", "Main.f"], &root, None);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(stdout, "counterexample Main.f\n", "{stderr}");
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("trap: contract ensures failed"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Review of #64, round 2: a lambda parameter's `when` guard is not checked for exhaustiveness,
/// so a value that fails it traps with `pattern mismatch`; the prover must not read the last arm
/// as unconditional. An exhaustive `match` with no catch-all arm still proves.
#[test]
fn a_guard_that_can_fail_is_a_trap() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    let source = "def positive :: i64 -> i64 = \\x when x > 0 -> x\n\
                  def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r == x\n    positive x\n\
                  def sign :: bool -> i64 -> i64 = \\b x ->\n    @ensures \\r -> r == 0 || r == x\n    match b with\n    | true -> x\n    | false -> 0\n\
                  def main :: unit -> i32 = \\() -> (f 0) as i32\n";
    let root = project("guard", source);
    let (code, stdout, stderr) = prove(&["--function", "Main.f"], &root, None);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(stdout, "counterexample Main.f\n", "{stderr}");
    assert!(
        stderr.contains("'the body' of 'Main.f' can fail: Main.f ")
            && stderr.contains(" traps: pattern mismatch"),
        "{stderr}"
    );
    let (code, stdout, stderr) = prove(&["--function", "Main.sign"], &root, None);
    assert_eq!(
        (code, stdout.as_str()),
        (0, "proved Main.sign\n"),
        "{stderr}"
    );
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("trap: pattern mismatch"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Review of #64: a `--function` name that matches no function with contract lines is an error,
/// alone or beside valid names, before any solver runs.
#[test]
fn unmatched_function_names_are_errors() {
    let root = project("unmatched", SOURCE);
    for arguments in [
        &["--function", "Main.clmap"][..],
        &["--function", "Main.clamp", "--function", "Main.clmap"],
        &["--function", "Main.main"],
    ] {
        let (code, stdout, stderr) = prove(arguments, &root, None);
        assert_eq!(code, 1, "{arguments:?}: {stdout}{stderr}");
        assert!(stdout.is_empty(), "{stdout}");
        assert!(
            stderr.contains("error[E2000]: --function '")
                && stderr.contains("names no function with contract lines"),
            "{arguments:?}: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unsupported_constructs_are_named() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    for (body, reason) in [
        ("@requires s.length > 0\n    s.length", "a string"),
        ("@requires n > 0\n    Array.sum [n]", "an array or list"),
        ("@ensures \\r -> r == n\n    total n", "a recursive call"),
        ("@requires n / 2 > 0\n    n", "a division in a contract"),
    ] {
        let parameter = if reason == "a string" {
            "s :: ref string"
        } else {
            "n :: i64"
        };
        let (name, ty) = parameter.split_once(" :: ").unwrap();
        let root = project(
            "unsupported",
            &format!(
                "def rec total :: i64 -> i64 = \\k -> if k <= 0 then 0 else total (k - 1)\n\
                 def f :: {ty} -> i64 = \\{name} ->\n    {body}\n\
                 def main :: unit -> i32 = \\() -> 0\n"
            ),
        );
        let (code, stdout, stderr) = prove(&["--function", "Main.f"], &root, None);
        assert_eq!(code, 1);
        assert_eq!(
            stdout,
            format!("unsupported Main.f ({reason})\n"),
            "{stderr}"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

/// A fake solver: a shell script that prints `answer` for every script.
#[cfg(unix)]
fn fake(name: &str, answer: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tsuzuri-prove-{name}-{}.sh", std::process::id()));
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat > /dev/null\nprintf '{answer}'\n"),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[cfg(unix)]
#[test]
fn reads_the_first_line_of_the_solver() {
    let root = project("fake", SOURCE);
    let unknown = fake("unknown", "unknown\\n(error \"model is not available\")\\n");
    let (code, stdout, stderr) = prove(
        &["--function", "Main.clamp", "--prove-timeout", "3"],
        &root,
        Some(&unknown),
    );
    assert_eq!(code, 1);
    assert_eq!(stdout, "unknown Main.clamp\n");
    assert!(
        stderr.contains("the solver could not decide 'Main.clamp' within 3 seconds"),
        "{stderr}"
    );
    let proved = fake("unsat", "unsat\\n(error \"model is not available\")\\n");
    let (code, stdout, _) = prove(&["--function", "Main.clamp"], &root, Some(&proved));
    assert_eq!((code, stdout.as_str()), (0, "proved Main.clamp\n"));
    let broken = fake("broken", "nonsense\\n");
    let (code, stdout, stderr) = prove(&["--function", "Main.clamp"], &root, Some(&broken));
    assert_eq!((code, stdout.as_str()), (1, "error Main.clamp (solver)\n"));
    assert!(stderr.contains("failed: nonsense"), "{stderr}");
    let (code, _, stderr) = prove(&[], &root, Some(Path::new("/nonexistent/solver")));
    assert_eq!(code, 1);
    assert!(
        stderr.contains("the solver '/nonexistent/solver' failed: cannot execute"),
        "{stderr}"
    );
    for path in [unknown, proved, broken] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_prove_options_without_prove() {
    let root = project("options", SOURCE);
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["build", "--prove-timeout", "5"])
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--prove-timeout and --function need --contracts prove or the prove command")
    );
    let _ = std::fs::remove_dir_all(root);
}

/// One integer type of the differential test: its name, width, signedness and values.
struct Kind {
    name: &'static str,
    width: u16,
    signed: bool,
}

fn values(kind: &Kind) -> Vec<i128> {
    let width = u32::from(kind.width);
    if kind.signed {
        let min = -(1i128 << (width - 1));
        let max = (1i128 << (width - 1)) - 1;
        vec![min, min + 1, -7, -1, 0, 1, 7, max - 1, max]
    } else {
        let max = ((1u128 << width) - 1) as i128;
        let half = 1i128 << (width - 1);
        vec![0, 1, 7, half - 1, half, max - 1, max]
    }
}

const OPERATORS: [(&str, BinaryOp); 13] = [
    ("+", BinaryOp::Add),
    ("-", BinaryOp::Subtract),
    ("*", BinaryOp::Multiply),
    ("/", BinaryOp::Divide),
    ("%", BinaryOp::Remainder),
    ("<", BinaryOp::Less),
    (">=", BinaryOp::GreaterEqual),
    ("==", BinaryOp::Equal),
    ("&&&", BinaryOp::BitAnd),
    ("|||", BinaryOp::BitOr),
    ("^^^", BinaryOp::BitXor),
    ("<<<", BinaryOp::ShiftLeft),
    (">>>", BinaryOp::ShiftRight),
];

#[test]
fn smt_semantics_match_native_execution() {
    if !z3() {
        println!("skipped: z3 not found");
        return;
    }
    let kinds = [
        Kind {
            name: "i8",
            width: 8,
            signed: true,
        },
        Kind {
            name: "i8u",
            width: 8,
            signed: false,
        },
        Kind {
            name: "i32",
            width: 32,
            signed: true,
        },
        Kind {
            name: "i64u",
            width: 64,
            signed: false,
        },
    ];
    let mut total = 0;
    for kind in &kinds {
        let values = values(kind);
        let min = if kind.signed {
            -(1i128 << (kind.width - 1))
        } else {
            0
        };
        let mut cases = Vec::new();
        for (symbol, operator) in OPERATORS {
            for &a in &values {
                for &b in &values {
                    let divides = matches!(operator, BinaryOp::Divide | BinaryOp::Remainder);
                    if divides && (b == 0 || (kind.signed && a == min && b == -1)) {
                        continue;
                    }
                    cases.push((symbol, operator, a, b));
                }
            }
        }
        // The reference: the native program prints every result.
        let literal = |value: i128| {
            if value < 0 {
                format!("({value}{})", kind.name)
            } else {
                format!("{value}{}", kind.name)
            }
        };
        let mut program = String::from("def main :: unit -> i32 = \\() ->\n");
        for (index, &(symbol, _, a, b)) in cases.iter().enumerate() {
            program.push_str(&format!(
                "    let a{index}: {} = {}\n    let b{index}: {} = {}\n    Debug.print (ref (to_string (a{index} {symbol} b{index})))\n",
                kind.name,
                literal(a),
                kind.name,
                literal(b)
            ));
        }
        program.push_str("    0\n");
        let root = project(&format!("diff-{}", kind.name), &program);
        let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["run", "-O0", "--contracts", "off"])
            .arg(&root)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let native: Vec<String> = String::from_utf8_lossy(&run.stderr)
            .lines()
            .chain(String::from_utf8_lossy(&run.stdout).lines())
            .map(str::to_owned)
            .filter(|line| !line.is_empty())
            .collect();
        // The subject: the translation, simplified by z3.
        let term = |value: i128| tsuzuri::prove::literal(value as u128, kind.width);
        let mut script = String::new();
        for &(_, operator, a, b) in &cases {
            let simplified =
                tsuzuri::prove::binary_term(operator, kind.width, kind.signed, &term(a), &term(b))
                    .expect("an integer operator");
            script.push_str(&format!("(simplify {simplified})\n"));
        }
        let mut child = Command::new("z3")
            .args(["-smt2", "-in"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let smt: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| {
                let line = line.trim();
                if line == "true" || line == "false" {
                    return line.to_owned();
                }
                let bits = if let Some(hex) = line.strip_prefix("#x") {
                    u128::from_str_radix(hex, 16).unwrap()
                } else if let Some(binary) = line.strip_prefix("#b") {
                    u128::from_str_radix(binary, 2).unwrap()
                } else {
                    panic!("{line}");
                };
                let width = u32::from(kind.width);
                if kind.signed && bits >> (width - 1) & 1 == 1 {
                    (bits as i128 - (1i128 << width)).to_string()
                } else {
                    bits.to_string()
                }
            })
            .collect();
        assert_eq!(native.len(), cases.len(), "{}: native lines", kind.name);
        assert_eq!(smt.len(), cases.len(), "{}: z3 lines", kind.name);
        for (index, (native, smt)) in native.iter().zip(&smt).enumerate() {
            let (symbol, _, a, b) = cases[index];
            assert_eq!(native, smt, "{}: {a} {symbol} {b}", kind.name);
        }
        total += cases.len();
        let _ = std::fs::remove_dir_all(root);
    }
    assert_eq!(total, 3312);
}
