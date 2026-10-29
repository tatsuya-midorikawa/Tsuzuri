//! A21: contract lines (`@requires`, `@ensures`, `@old`): syntax, weaving, purity, the modes and
//! the run-time checks. Expected values are written by hand from the rules of the ticket.

use std::path::{Path, PathBuf};
use std::process::Command;

use tsuzuri::check::contracts::{ContractMode, ContractShape};
use tsuzuri::check::{CheckedModule, TypedExprKind};
use tsuzuri::syntax::ContractKind;
use tsuzuri::{analyze, analyze_modules_with_contracts, llvm, parser};

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

const CLAMP: &str = "def clamp :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    @ensures \\r -> lo <= r && r <= hi\n    if x < lo then lo elif x > hi then hi else x\n";

fn accepts(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn rejects(source: &str, code: &str, kind: Option<&str>) -> tsuzuri::diagnostic::Diagnostic {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    if let Some(kind) = kind {
        assert_eq!(error.kind, Some(kind), "{source}\n{}", error.message);
    }
    error
}

fn function<'a>(module: &'a CheckedModule, name: &str) -> &'a tsuzuri::check::CheckedFunction {
    module
        .functions
        .iter()
        .find(|function| function.qualified_name() == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

#[test]
fn parses_requires_and_ensures_on_lambda_and_fn_bodies() {
    let source = format!(
        "{CLAMP}def twice :: i64 -> i64\nfn twice x =\n    @requires x > 0\n    x * 2\n\
         def nested :: i64 -> i64 -> i64 = \\a -> \\b ->\n    @ensures \\_ -> a < b\n    a + b\n\
         def answer :: i64 =\n    @ensures \\value -> value == 42\n    42\n{MAIN}"
    );
    let program = parser::parse(&source).unwrap();
    let find = |name: &str| {
        program
            .functions
            .iter()
            .find(|function| function.name.text == name)
            .unwrap()
    };
    let clamp = find("clamp");
    let kinds: Vec<_> = clamp.contracts.iter().map(|clause| clause.kind).collect();
    assert_eq!(kinds, [ContractKind::Requires, ContractKind::Ensures]);
    assert_eq!(clamp.contracts[0].text, "lo <= hi");
    assert_eq!(clamp.contracts[1].binder.as_ref().unwrap().text, "r");
    assert_eq!(clamp.contracts[1].text, "lo <= r && r <= hi");
    assert_eq!(
        &source[clamp.contracts[0].span.start..clamp.contracts[0].span.end],
        "@requires lo <= hi"
    );
    assert_eq!(find("twice").contracts.len(), 1);
    assert_eq!(
        find("nested").contracts[0].binder.as_ref().unwrap().text,
        "_"
    );
    assert_eq!(find("answer").contracts.len(), 1);
    accepts(&source);
}

#[test]
fn keeps_requires_as_an_identifier() {
    accepts(&format!(
        "def ensures :: i64 -> i64 = \\requires -> requires + 1\n\
         def f :: i64 -> i64 = \\x ->\n    let requires = x\n    ensures requires\n{MAIN}"
    ));
}

#[test]
fn rejects_misplaced_contract_lines() {
    let misplaced = Some("contract_misplaced");
    let error = rejects(
        &format!("def f :: i64 -> i64 = \\x -> @requires x > 0\n    x\n{MAIN}"),
        "E1033",
        misplaced,
    );
    assert_eq!(error.span.start, "def f :: i64 -> i64 = \\x -> ".len());
    rejects(
        &format!("def f :: i64 -> i64 = \\x ->\n    let y = x\n    @requires x > 0\n    y\n{MAIN}"),
        "E1033",
        misplaced,
    );
    rejects(
        &format!("def f :: i64 -> i64 = \\x ->\n    @requires x > 0\n    {{ x }}\n{MAIN}"),
        "E1033",
        misplaced,
    );
    rejects(
        &format!("def f :: i64 -> i64 = \\x -> x\ndef g :: i64 = f (@requires 1 > 0)\n{MAIN}"),
        "E1033",
        misplaced,
    );
    rejects(
        &format!("test \"t\" =\n    @requires 1 > 0\n    assert true\n{MAIN}"),
        "E1033",
        misplaced,
    );
    rejects(
        &format!(
            "def f :: i64 -> i64 = \\x ->\n    let g = \\y ->\n        @requires y > 0\n        y\n    g x\n{MAIN}"
        ),
        "E1033",
        misplaced,
    );
    // Review of #64: an instance method takes no contract lines; the lines are not attached to
    // the function declared before the instance either.
    for method in [
        "    fn eq left right =\n        @requires left.key >= 0\n        left.key == right.key\n",
        "    fn eq left right = \\l r ->\n        @requires l >= 0\n        l == r\n",
    ] {
        let error = rejects(
            &format!(
                "record Keyed {{ key: i64 }}\ndef before :: i64 -> i64 = \\x -> x\ninstance Eq<Keyed> {{\n{method}    fn ne left right = left.key != right.key\n}}\n{MAIN}"
            ),
            "E1033",
            misplaced,
        );
        assert!(
            error
                .message
                .starts_with("contract lines are for 'def' functions"),
            "{}",
            error.message
        );
    }
    // Other names after `@` keep their old error.
    rejects(
        &format!("def f :: i64 -> i64 = \\x -> @serialize x\n{MAIN}"),
        "E0002",
        None,
    );
}

#[test]
fn rejects_contract_on_const_and_bad_result_lambdas() {
    rejects(
        &format!("const def k :: i64 -> i64 = \\n ->\n    @requires n >= 0\n    n\n{MAIN}"),
        "E1033",
        Some("contract_on_const"),
    );
    for line in [
        "@ensures x > 0",
        "@ensures \\(a, b) -> a > 0",
        "@ensures r -> r > 0",
    ] {
        rejects(
            &format!("def f :: i64 -> i64 = \\x ->\n    {line}\n    x\n{MAIN}"),
            "E1033",
            Some("contract_result_lambda"),
        );
    }
    rejects(
        &format!("def g :: ref i64 -> ref i64 = \\x ->\n    @ensures \\r -> true\n    x\n{MAIN}"),
        "E1033",
        Some("contract_ensures_reference_result"),
    );
    let many: String = (0..33)
        .map(|i| format!("    @requires x > {i}\n"))
        .collect();
    rejects(
        &format!("def f :: i64 -> i64 = \\x ->\n{many}    x\n{MAIN}"),
        "E1017",
        Some("contract_line_limit"),
    );
}

#[test]
fn formats_contract_lines_and_round_trips() {
    let source =
        "def f :: i64 -> i64 = \\x ->\n    @requires   x>0\n    @ensures \\r ->   r>x\n    x+1\n";
    let first =
        tsuzuri::formatter::format_source("Main.tz", source, tsuzuri::syntax::SourceKind::Code)
            .unwrap();
    assert_eq!(
        first.formatted,
        "def f :: i64 -> i64 = \\x ->\n    @requires x > 0\n    @ensures \\r -> r > x\n    x + 1\n"
    );
    let second = tsuzuri::formatter::format_source(
        "Main.tz",
        &first.formatted,
        tsuzuri::syntax::SourceKind::Code,
    )
    .unwrap();
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn weaves_requires_before_and_ensures_after_the_body() {
    let module = accepts(&format!("{CLAMP}{MAIN}"));
    let clamp = function(&module, "Main.clamp");
    assert_eq!(
        clamp.contracts,
        ContractShape {
            requires: 1,
            olds: 0,
            ensures: 1
        }
    );
    let TypedExprKind::Block { bindings, result } = &clamp.body.kind else {
        panic!("a woven body is a block");
    };
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[1].0.name, "$contract.result");
    assert!(matches!(result.kind, TypedExprKind::Local(id) if id == bindings[1].0.id));
    let requires_only = accepts(&format!(
        "def f :: i64 -> i64 = \\x ->\n    @requires x > 0\n    x + 1\n{MAIN}"
    ));
    let f = function(&requires_only, "Main.f");
    let TypedExprKind::Block { bindings, result } = &f.body.kind else {
        panic!("a woven body is a block");
    };
    assert_eq!(bindings.len(), 1);
    // The body itself is the block's result: it stays in tail position.
    assert!(
        !matches!(result.kind, TypedExprKind::Local(_)),
        "{result:?}"
    );
}

#[test]
fn ensures_names_alias_the_one_result() {
    accepts(&format!(
        "def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > x\n    @ensures \\value -> value == x + 1\n    x + 1\n{MAIN}"
    ));
}

#[test]
fn contract_not_bool_is_rejected() {
    let source = format!("def f :: i64 -> i64 = \\x ->\n    @requires x\n    x\n{MAIN}");
    let error = rejects(&source, "E1033", Some("contract_not_bool"));
    assert_eq!(&source[error.span.start..error.span.end], "x");
    assert!(
        error.message.contains("but this is 'i64'"),
        "{}",
        error.message
    );
}

#[test]
fn ownership_rules_apply_inside_contracts() {
    rejects(
        &format!(
            "def take :: own string -> string = \\s ->\n    @ensures \\r -> s.length > 0\n    s\n{MAIN}"
        ),
        "E1012",
        None,
    );
}

#[test]
fn generic_functions_carry_contracts_into_every_instance() {
    let module = accepts(&format!(
        "def count :: ref ['a] -> i64 = \\xs ->\n    @requires xs.length > 0\n    xs.length\n\
         def use :: unit -> i64 = \\() -> count [1, 2] + count [\"a\"]\n{MAIN}"
    ));
    let instances: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.name.starts_with("count"))
        .collect();
    assert!(instances.len() >= 2);
    assert!(
        instances
            .iter()
            .all(|function| function.contracts.requires == 1)
    );
}

#[test]
fn contract_impure_is_rejected() {
    let impure = Some("contract_impure");
    let error = rejects(
        &format!(
            "def noisy :: i64 -> bool = \\x ->\n    Debug.print (ref \"checking\")\n    x > 0\n\
             def f :: i64 -> i64 = \\x ->\n    @requires noisy x\n    x\n{MAIN}"
        ),
        "E1033",
        impure,
    );
    assert!(
        error
            .message
            .starts_with("a contract must be pure: it prints debug output (through 'Main.noisy'"),
        "{}",
        error.message
    );
    rejects(
        &format!(
            "extern \"abs\" def c_abs :: i32 -> i32\n\
             def f :: i32 -> i32 = \\x ->\n    @requires c_abs x > 0i32\n    x\n{MAIN}"
        ),
        "E1033",
        impure,
    );
    rejects(
        &format!(
            "def f :: i64 -> i64 = \\x ->\n    @requires (\\y -> {{ Debug.print (ref y); y > 0 }}) x\n    x\n{MAIN}"
        ),
        "E1033",
        impure,
    );
    rejects(
        &format!(
            "def f :: (i64 -> bool) -> i64 -> i64 = \\p x ->\n    @requires p x\n    x\n{MAIN}"
        ),
        "E1033",
        impure,
    );
    // A pure function that is not declared `pure def`, allocation and traps are allowed.
    accepts(&format!(
        "def positive :: i64 -> bool = \\x -> x > 0\n\
         def f :: ref [i64] -> i64 -> i64 = \\xs i ->\n    @requires positive i && xs[i] > 0 && (Array.init 2 (\\k -> k)).length == 2\n    xs[i]\n{MAIN}"
    ));
}

/// Review of #64: the recorded text of a contract line keeps string literals as written and
/// makes only the space between tokens one space.
#[test]
fn contract_text_keeps_string_literals() {
    let source = "def f :: string -> i64 = \\text ->\n    @requires text == \"a  b\" ||\n        text.length   >  3\n    text.length\n";
    let program = parser::parse(source).unwrap();
    let clause = &program.functions[0].contracts[0];
    assert_eq!(clause.text, "text == \"a  b\" || text.length > 3");
    let single = parser::parse(&source.replace("a  b", "a b")).unwrap();
    assert_ne!(single.functions[0].contracts[0].text, clause.text);
}

/// Review of #64: the operand of `@old` is evaluated on entry in `check` and not at all in
/// `off`, so it must be pure like the conditions.
#[test]
fn old_operands_must_be_pure() {
    rejects(
        &format!(
            "def noisy :: i64 -> i64 = \\x ->\n    Debug.print (ref \"checking\")\n    x\n\
             def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r == @old (noisy x)\n    x\n{MAIN}"
        ),
        "E1033",
        Some("contract_impure"),
    );
    accepts(&format!(
        "def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r == @old (x * 2) / 2\n    x\n{MAIN}"
    ));
}

/// Review of #64: a contract line may read mutable arguments but not change them, or the value
/// it returns: `check` and `off` would compute differently.
#[test]
fn contract_lines_cannot_write_to_the_call() {
    let writes = Some("contract_writes");
    let helper =
        "def touch :: ref mut [i64..] -> bool = \\xs ->\n    Array.write 0 7 xs\n    true\n";
    let error = rejects(
        &format!(
            "{helper}def f :: ref mut [i64..] -> i64 = \\xs ->\n    @requires touch xs\n    xs.length\n{MAIN}"
        ),
        "E1033",
        writes,
    );
    assert!(
        error
            .message
            .starts_with("a contract line cannot change the data"),
        "{}",
        error.message
    );
    rejects(
        &format!(
            "{helper}def f :: ref mut [i64..] -> i64 = \\xs ->\n    @ensures \\r -> touch xs && r >= 0\n    xs.length\n{MAIN}"
        ),
        "E1033",
        writes,
    );
    // Round 2 of the review: a local that holds the argument's reference writes to it too.
    rejects(
        &format!(
            "def set :: ref mut i64 -> unit = \\p ->\n    @ensures \\_ -> {{ let q = p; deref q = 7; true }}\n    ()\n{MAIN}"
        ),
        "E1033",
        writes,
    );
    rejects(
        &format!(
            "def poke :: ref mut i64 -> bool = \\p ->\n    deref p = 1\n    true\ndef set :: ref mut i64 -> unit = \\p ->\n    @ensures \\_ -> {{ let q = p; let r = q; poke r }}\n    ()\n{MAIN}"
        ),
        "E1033",
        writes,
    );
    // Reading a mutable argument, and changing data of the condition's own, are allowed.
    accepts(&format!(
        "{helper}def f :: ref mut [i64..] -> i64 = \\xs ->\n    @requires xs.length > 0 && Array.sum xs >= 0\n    @ensures \\r -> {{ let mut copy = [1, 2]; touch (ref mut copy[0..]) }} && r >= 0\n    xs.length\n{MAIN}"
    ));
}

/// Review of #64, round 2: `@old` is resolved where the checker reads an argument without
/// `expression` (a qualified call) and inside a computation.
#[test]
fn old_values_reach_call_arguments_and_computations() {
    accepts(&format!(
        "def grow :: i64 -> i64 = \\x ->\n    @ensures \\r -> Int.max (@old (x)) 0 <= r || r < 0\n    @ensures \\r -> {{ let m = Maybe {{ return @old (x) + 1 }}; Maybe.default_value 0 m == r }}\n    x + 1\n{MAIN}"
    ));
}

/// The functions of `off_ir_equals_the_ir_without_contract_lines`, each with contract lines.
const CONTRACTED: &str = "record Point { x: i64, y: i64 }\n\
def sum :: ref [i64] -> i64 = \\xs ->\n    @requires xs.length >= 0\n    @ensures \\r -> r == r\n    Array.sum xs\n\
def shout :: own string -> string = \\s ->\n    @requires s.length > 0\n    $\"{s}!\"\n\
def norm :: Point -> i64 = \\p ->\n    @ensures \\r -> r >= 0\n    Int.abs p.x + Int.abs p.y\n\
def first :: ref ['a] -> i64 = \\xs ->\n    @requires xs.length > 0\n    xs.length\n\
def bump :: ref mut [i64..] -> unit = \\xs ->\n    @requires xs.length > 0\n    Array.write 0 (xs[0] + 1) xs\n\
def rec down :: i64 -> i64 = \\n ->\n    @requires n >= 0\n    if n == 0 then 0 else down (n - 1)\n\
@cpu [\"avx2\"]\ndef scaled :: i64 -> i64 = \\x ->\n    @ensures \\r -> r == x * 3\n    x * 3\n\
export def exported :: i64 -> i64 = \\x ->\n    @requires x != 0\n    @ensures \\r -> r != 0\n    x\n\
def lambda_body :: i64 -> i64 = \\x ->\n    @requires x >= 0\n    let f = \\y -> y + x\n    f 1\n\
def use :: unit -> i64 = \\() ->\n    let xs = [1, 2, 3]\n    sum xs + norm (Point { x: 1, y: -2 }) + first [\"a\"] + first [1] + down 3 + scaled 2 + exported 1 + lambda_body 4\n";

/// `source` with every contract line replaced by a comment of its length, so every other span
/// stays where it was.
fn without_contract_lines(source: &str) -> String {
    source
        .split_inclusive('\n')
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with("@requires") || trimmed.starts_with("@ensures") {
                let body = line.trim_end_matches('\n');
                let indent = body.len() - trimmed.trim_end_matches('\n').len();
                format!(
                    "{}//{}{}",
                    &body[..indent],
                    " ".repeat(body.len() - indent - 2),
                    if line.ends_with('\n') { "\n" } else { "" }
                )
            } else {
                line.to_owned()
            }
        })
        .collect()
}

#[test]
fn off_ir_equals_the_ir_without_contract_lines() {
    let source = format!("{CONTRACTED}{MAIN}");
    let plain = without_contract_lines(&source);
    assert_eq!(plain.len(), source.len());
    let emit = |text: &str, mode| {
        let module = analyze_modules_with_contracts(&[("Main.tz", text)], mode)
            .unwrap_or_else(|errors| panic!("{:?}", errors.first().map(|e| e.message.clone())));
        llvm::emit(&module, llvm::Entry::Library).unwrap()
    };
    let off = emit(&source, ContractMode::Off);
    assert_eq!(off, emit(&plain, ContractMode::Off));
    assert_eq!(off, emit(&plain, ContractMode::Check));
    assert!(!off.contains("$requires") && !off.contains("$ensures"));
}

#[test]
fn off_checks_the_woven_form_first() {
    let source = format!(
        "def take :: own string -> string = \\s ->\n    @ensures \\r -> s.length > 0\n    s\n{MAIN}"
    );
    let error = analyze_modules_with_contracts(&[("Main.tz", &source)], ContractMode::Off)
        .expect_err("the woven form is checked in off too");
    assert_eq!(error.first().unwrap().code, "E1012");
}

#[test]
fn check_ir_has_the_contract_calls_and_is_deterministic() {
    let source = format!("{CLAMP}def use :: unit -> i64 = \\() -> clamp 0 10 5\n{MAIN}");
    let emit = || {
        let module =
            analyze_modules_with_contracts(&[("Main.tz", &source)], ContractMode::Check).unwrap();
        llvm::emit(&module, llvm::Entry::Library).unwrap()
    };
    let first = emit();
    assert!(first.contains("@tz.builtin.$requires") && first.contains("@tz.builtin.$ensures"));
    assert_eq!(first, emit());
    assert_eq!(
        first
            .matches("define internal i8 @tz.builtin.$requires")
            .count(),
        1
    );
}

#[test]
fn old_is_hoisted_before_the_body() {
    let source = format!(
        "def bump :: ref mut [i64..] -> i64 -> unit = \\xs v ->\n    @ensures \\_ -> xs[0] == v && @old (xs[0]) != v\n    Array.write 0 v xs\n{MAIN}"
    );
    let module = accepts(&source);
    let bump = function(&module, "Main.bump");
    assert_eq!(bump.contracts.olds, 1);
    rejects(
        &format!(
            "def f :: ref [i64] -> i64 = \\xs ->\n    @ensures \\r -> r == (@old (xs)).length\n    xs.length\n{MAIN}"
        ),
        "E1033",
        Some("contract_old_not_copy"),
    );
    rejects(
        &format!(
            "def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > @old (@old (x))\n    x + 1\n{MAIN}"
        ),
        "E1033",
        Some("contract_old_nested"),
    );
    rejects(
        &format!(
            "def f :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > @old (r)\n    x + 1\n{MAIN}"
        ),
        "E1033",
        Some("contract_old_operand"),
    );
    rejects(
        &format!("def f :: i64 -> i64 = \\x ->\n    @requires @old (x) > 0\n    x\n{MAIN}"),
        "E1033",
        Some("contract_old_misplaced"),
    );
    rejects(
        &format!("def f :: i64 -> i64 = \\x ->\n    @old (x)\n{MAIN}"),
        "E1033",
        Some("contract_old_misplaced"),
    );
}

fn project(name: &str, source: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("tsuzuri-contracts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    root
}

fn tsuzuri(arguments: &[&str], root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .arg(root)
        .output()
        .unwrap()
}

const BROKEN: &str = "def bad :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > x\n    x - 1\n\
def checked :: i64 -> i64 = \\x ->\n    @requires x > 0\n    x\n\
def main :: unit -> i32 = \\() -> (bad 3 + checked 1) as i32\n";

#[test]
fn default_modes_follow_the_command() {
    let root = project("modes", BROKEN);
    let ir = root.join("out.ll");
    assert!(
        tsuzuri(
            &["build", "--emit", "llvm", "-o", ir.to_str().unwrap()],
            &root
        )
        .status
        .success()
    );
    assert!(!std::fs::read_to_string(&ir).unwrap().contains("$ensures"));
    assert!(
        tsuzuri(
            &[
                "build",
                "--contracts",
                "check",
                "--emit",
                "llvm",
                "-o",
                ir.to_str().unwrap()
            ],
            &root
        )
        .status
        .success()
    );
    assert!(std::fs::read_to_string(&ir).unwrap().contains("$ensures"));
    let run = tsuzuri(&["run"], &root);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("trap: contract ensures failed at"));
    let off = tsuzuri(&["run", "--contracts", "off"], &root);
    let stderr = String::from_utf8_lossy(&off.stderr);
    assert!(stderr.contains("program exited with code 3"), "{stderr}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn run_reports_the_contract_line_as_e2005() {
    let root = project("json", BROKEN);
    let run = tsuzuri(&["run", "--json"], &root);
    let stderr = String::from_utf8_lossy(&run.stderr);
    let line = stderr
        .lines()
        .find(|line| line.contains("\"E2005\""))
        .unwrap_or_else(|| panic!("{stderr}"));
    let diagnostic: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(diagnostic["kind"], "contract_ensures");
    assert_eq!(diagnostic["span"]["line"], 2);
    assert_eq!(diagnostic["span"]["column"], 5);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_runner_prints_the_contract_trap_line() {
    let root = project(
        "runner",
        "def bad :: i64 -> i64 = \\x ->\n    @ensures \\r -> r > x\n    x - 1\n\ntest \"bad\" = assert (bad 3 == 2)\n",
    );
    let test = tsuzuri(&["test"], &root);
    let stdout = String::from_utf8_lossy(&test.stdout);
    assert!(!test.status.success());
    assert!(
        stdout.contains("trap: contract ensures failed at"),
        "{stdout}"
    );
    assert!(stdout.contains("Main.tz:2:5"), "{stdout}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rejects_contracts_where_a_command_takes_none() {
    let root = project("cli", BROKEN);
    for (arguments, message) in [
        (
            &["doc", "--contracts", "check"][..],
            "--contracts is only valid with build, run, test, or check",
        ),
        (
            &["build", "--contracts", "test"][..],
            "--contracts test is only valid with test",
        ),
        (
            &["build", "--contracts", "maybe"][..],
            "unknown contract mode 'maybe'",
        ),
    ] {
        let output = tsuzuri(arguments, &root);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn requires_keeps_the_self_tail_call() {
    // Ten million frames would exhaust the stack if `@requires` broke the tail call.
    let root = project(
        "tail",
        "def rec down :: i64 -> i64 -> i64 = \\n acc ->\n    @requires n >= 0\n    if n == 0 then acc else down (n - 1) (acc + 1)\n\
         def main :: unit -> i32 = \\() -> (down 10000000 0 - 10000000) as i32\n",
    );
    let run = tsuzuri(&["run", "-O0"], &root);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(root);
}
