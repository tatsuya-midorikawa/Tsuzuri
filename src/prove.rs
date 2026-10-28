//! `tsuzuri prove` and `--contracts prove` (A21 Phase 5): each function with contract lines is
//! translated to SMT-LIB (`QF_BV`) and an external solver (z3, or cvc5) decides whether the body
//! can trap where the `@requires` lines hold, and whether each `@ensures` line holds where it
//! does not trap. One question is one script and one solver process. The proof removes no
//! run-time check (D8).

#[path = "prove_smt.rs"]
mod smt;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::check::{CheckedFunction, CheckedModule, ModuleOrigin, Type, TypedExprKind};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::syntax::Provenance;

pub use smt::{binary_term, literal};

/// What `prove` checks and with what.
#[derive(Clone, Debug)]
pub struct ProveOptions {
    pub timeout_seconds: u64,
    /// `Module.name` of the functions to prove; empty proves every function with contracts.
    pub functions: Vec<String>,
}

impl Default for ProveOptions {
    fn default() -> Self {
        Self {
            timeout_seconds: 10,
            functions: Vec::new(),
        }
    }
}

/// The verdict of one function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Proved,
    Counterexample,
    Unknown,
    Unsupported,
    Error,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Proved => "proved",
            Verdict::Counterexample => "counterexample",
            Verdict::Unknown => "unknown",
            Verdict::Unsupported => "unsupported",
            Verdict::Error => "error",
        }
    }
}

/// The verdict of one question: `trap_free`, or the `@ensures` line `index`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClauseResult {
    pub ensures: Option<usize>,
    pub verdict: Verdict,
}

/// What `prove` found for one function.
#[derive(Clone, Debug)]
pub struct Report {
    pub function: String,
    pub verdict: Verdict,
    /// The reason of `unsupported`, for the text line.
    pub detail: Option<String>,
    pub clauses: Vec<ClauseResult>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The outcome of `prove`: the reports, or the error that stopped it before any question.
pub struct Outcome {
    pub reports: Vec<Report>,
    pub error: Option<Diagnostic>,
}

impl Outcome {
    /// Whether every selected function is proved (and nothing stopped the proof).
    pub fn proved(&self) -> bool {
        self.error.is_none()
            && self
                .reports
                .iter()
                .all(|report| report.verdict == Verdict::Proved)
    }

    /// The text lines: one per function, `proved Main.clamp` or `unsupported Main.f (a loop)`.
    pub fn text(&self) -> String {
        self.reports
            .iter()
            .map(|report| {
                let detail = match (report.verdict, &report.detail) {
                    (Verdict::Unsupported, Some(reason)) => format!(" ({reason})"),
                    (Verdict::Error, _) => " (solver)".into(),
                    _ => String::new(),
                };
                format!("{} {}{detail}\n", report.verdict.name(), report.function)
            })
            .collect()
    }

    /// JSON Lines: one per function and a summary, in a fixed key order and without times.
    pub fn json(&self) -> String {
        let mut out = String::new();
        for report in &self.reports {
            let clauses: Vec<String> = report
                .clauses
                .iter()
                .map(|clause| match clause.ensures {
                    None => format!(
                        "{{\"kind\":\"trap_free\",\"result\":\"{}\"}}",
                        clause.verdict.name()
                    ),
                    Some(index) => format!(
                        "{{\"kind\":\"ensures\",\"index\":{index},\"result\":\"{}\"}}",
                        clause.verdict.name()
                    ),
                })
                .collect();
            out.push_str(&format!(
                "{{\"type\":\"prove\",\"function\":{},\"result\":\"{}\",\"clauses\":[{}]}}\n",
                crate::diagnostic::json_string(&report.function),
                report.verdict.name(),
                clauses.join(",")
            ));
        }
        let proved = self
            .reports
            .iter()
            .filter(|report| report.verdict == Verdict::Proved)
            .count();
        out.push_str(&format!(
            "{{\"type\":\"summary\",\"proved\":{proved},\"failed\":{}}}\n",
            self.reports.len() - proved
        ));
        out
    }
}

fn shown(function: &CheckedFunction) -> String {
    function
        .qualified_name()
        .split(".$mono.")
        .next()
        .unwrap()
        .to_owned()
}

/// The solver: `TSUZURI_SMT`, else `z3`, else `cvc5`, as found on the distribution or `PATH`.
fn solver() -> Result<(String, bool), Diagnostic> {
    let missing = || {
        Diagnostic::of(
            DiagnosticKind::ProveSolverMissing,
            "no SMT solver found; install z3 or cvc5, or set TSUZURI_SMT to its path",
            Span::default(),
        )
        .with_help("install z3 (https://github.com/Z3Prover/z3) or cvc5")
    };
    if let Some(value) = std::env::var_os("TSUZURI_SMT") {
        let name = value.to_string_lossy().into_owned();
        let cvc5 = std::path::Path::new(&name)
            .file_stem()
            .is_some_and(|stem| stem.to_string_lossy().starts_with("cvc5"));
        return Ok((name, cvc5));
    }
    for (name, cvc5) in [("z3", false), ("cvc5", true)] {
        let (_, path) = crate::driver::resolve_tool("TSUZURI_SMT_UNSET", name);
        if Command::new(&path)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return Ok((path.to_string_lossy().into_owned(), cvc5));
        }
    }
    Err(missing())
}

/// What the solver answered to one script.
enum Answer {
    Unsat,
    Sat(BTreeMap<String, ModelValue>),
    Unknown,
    Error(String),
}

#[derive(Clone, Copy, Debug)]
enum ModelValue {
    Bits(u128),
    Bool(bool),
}

const OUTPUT_LIMIT: usize = 1 << 20;

fn ask(solver: &(String, bool), script: &str, timeout_seconds: u64) -> Answer {
    let milliseconds = timeout_seconds.saturating_mul(1000);
    let mut command = Command::new(&solver.0);
    if solver.1 {
        command.args([
            "--lang=smt2",
            "--produce-models",
            &format!("--tlimit={milliseconds}"),
        ]);
    } else {
        command.args(["-smt2", "-in", &format!("-t:{milliseconds}")]);
    }
    let mut child = match command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Answer::Error("cannot execute".into()),
    };
    let mut stdin = child.stdin.take().unwrap();
    let input = script.to_owned();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    output.extend_from_slice(&buffer[..read]);
                    if output.len() > OUTPUT_LIMIT {
                        break;
                    }
                }
            }
        }
        output
    });
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds + 5);
    let killed = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break true,
        }
    };
    let _ = writer.join();
    let output = reader.join().unwrap_or_default();
    if killed {
        return Answer::Error("timed out".into());
    }
    if output.len() > OUTPUT_LIMIT {
        return Answer::Error("output too large".into());
    }
    let text = String::from_utf8_lossy(&output);
    let mut lines = text.lines();
    match lines.next().map(str::trim) {
        Some("unsat") => Answer::Unsat,
        Some("unknown") | Some("timeout") => Answer::Unknown,
        Some("sat") => Answer::Sat(model(&lines.collect::<Vec<_>>().join(" "))),
        Some(other) => Answer::Error(other.chars().take(200).collect()),
        None => Answer::Error("no output".into()),
    }
}

/// The values of a `get-value` answer: `((p0 #x..) (t3 (_ bv5 64)) (o0 true))`.
fn model(text: &str) -> BTreeMap<String, ModelValue> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        match character {
            '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(character.to_string());
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index] == "(" && index + 2 < tokens.len() && tokens[index + 1] != "(" {
            let name = tokens[index + 1].clone();
            let value = &tokens[index + 2];
            let parsed = if value == "true" {
                Some(ModelValue::Bool(true))
            } else if value == "false" {
                Some(ModelValue::Bool(false))
            } else if let Some(hex) = value.strip_prefix("#x") {
                u128::from_str_radix(hex, 16).ok().map(ModelValue::Bits)
            } else if let Some(binary) = value.strip_prefix("#b") {
                u128::from_str_radix(binary, 2).ok().map(ModelValue::Bits)
            } else if value == "("
                && tokens.get(index + 3).map(String::as_str) == Some("_")
                && let Some(decimal) = tokens.get(index + 4).and_then(|t| t.strip_prefix("bv"))
            {
                decimal.parse().ok().map(ModelValue::Bits)
            } else {
                None
            };
            if let Some(parsed) = parsed {
                values.insert(name, parsed);
            }
        }
        index += 1;
    }
    values
}

/// A value of the model as the language writes it (C.9).
fn render(
    value: &smt::Value,
    ty: &Type,
    module: &CheckedModule,
    model: &BTreeMap<String, ModelValue>,
) -> String {
    let types = module.types();
    match (value, ty) {
        (_, Type::Reference(inner, _)) => render(value, inner, module, model),
        (smt::Value::Unit, _) => "()".into(),
        (smt::Value::Bool(term), _) => match model.get(term) {
            Some(ModelValue::Bool(value)) => value.to_string(),
            _ => term.clone(),
        },
        (smt::Value::Bits(term, width), Type::Integer(_, signed)) => {
            let Some(ModelValue::Bits(bits)) = model.get(term) else {
                return term.clone();
            };
            let width = u32::from(*width);
            if *signed && width < 128 && bits >> (width - 1) & 1 == 1 {
                let negative = (1u128 << width) - bits;
                format!("(-{negative})")
            } else if *signed && width == 128 && bits >> 127 == 1 {
                format!("(-{})", bits.wrapping_neg())
            } else {
                bits.to_string()
            }
        }
        (smt::Value::Bits(term, _), Type::Union(id, _)) => {
            let Some(ModelValue::Bits(tag)) = model.get(term) else {
                return term.clone();
            };
            let union = &module.unions[*id];
            union.cases.get(*tag as usize).map_or_else(
                || term.clone(),
                |(case, _)| format!("{}.{case}", union.module()),
            )
        }
        (smt::Value::Tuple(values), Type::Tuple(types_)) => format!(
            "({})",
            values
                .iter()
                .zip(types_)
                .map(|(value, ty)| render(value, ty, module, model))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (smt::Value::Tuple(values), Type::Record(id, arguments)) => {
            let record = &module.records[*id];
            let fields = types.record_fields(*id, arguments);
            format!(
                "{} {{ {} }}",
                record.name,
                values
                    .iter()
                    .zip(record.fields.iter().zip(fields))
                    .map(|(value, ((name, _), ty))| format!(
                        "{name}: {}",
                        render(value, &ty, module, model)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        (smt::Value::Bits(term, _), _) => term.clone(),
        (smt::Value::Tuple(_), _) => "?".into(),
    }
}

/// The questions of one function and their verdicts.
fn prove_function(
    module: &CheckedModule,
    function: &CheckedFunction,
    solver: &(String, bool),
    options: &ProveOptions,
    clause_texts: &[String],
) -> Report {
    let name = shown(function);
    let shape = function.contracts;
    let unsupported_report = |reason: String, span: Span| Report {
        function: name.clone(),
        verdict: Verdict::Unsupported,
        detail: Some(reason.clone()),
        clauses: Vec::new(),
        diagnostics: vec![Diagnostic::of(
            DiagnosticKind::ProveUnsupported,
            format!(
                "cannot prove '{name}': {reason}; see 'Prove' in the language reference for the supported subset"
            ),
            span,
        )],
    };
    let TypedExprKind::Block { bindings, result } = &function.body.kind else {
        return unsupported_report("an unresolved expression".into(), function.span);
    };
    let mut translator = smt::Translator::new(module);
    if let Err(error) = translator.parameters(function) {
        return unsupported_report(error.reason, error.span);
    }
    let requires = usize::from(shape.requires);
    let olds = usize::from(shape.olds);
    let condition_of = |value: &crate::check::TypedExpr| match &value.kind {
        TypedExprKind::Call(_, arguments) => arguments.first().cloned(),
        _ => None,
    };
    let mut premises = Vec::new();
    let mut result_value = smt::Value::Unit;
    let mut ensures = Vec::new();
    let translated = (|| -> Result<(), smt::Unsupported> {
        for (_, value) in &bindings[..requires] {
            if let Some(condition) = condition_of(value) {
                premises.push(translator.condition(&condition, "true")?);
            }
        }
        for (local, value) in &bindings[requires..requires + olds] {
            let value = translator.translate(value, "true")?;
            translator.bind(local.id, value);
        }
        if shape.ensures == 0 {
            result_value = translator.translate(result, "true")?;
        } else {
            let (local, body) = &bindings[requires + olds];
            result_value = translator.translate(body, "true")?;
            translator.bind(local.id, result_value.clone());
            for (_, value) in &bindings[requires + olds + 1..] {
                if let Some(condition) = condition_of(value) {
                    ensures.push((translator.condition(&condition, "true")?, value.span));
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = translated {
        if error.reason == "a proof script that is too large" {
            return Report {
                function: name.clone(),
                verdict: Verdict::Unsupported,
                detail: Some(error.reason),
                clauses: Vec::new(),
                diagnostics: vec![Diagnostic::of(
                    DiagnosticKind::ProveLimit,
                    format!(
                        "a proof script is limited to {} definitions; split the function or its callees",
                        smt::MAX_DEFINITIONS
                    ),
                    function.span,
                )],
            };
        }
        return unsupported_report(error.reason, error.span);
    }
    let mut asked = Vec::new();
    for argument in &translator.arguments {
        smt::constants(&argument.0, &mut asked);
    }
    smt::constants(&result_value, &mut asked);
    let obligations: Vec<String> = translator
        .obligations
        .iter()
        .map(|obligation| obligation.name.clone())
        .collect();
    asked.extend(obligations.iter().cloned());
    let mut premise_lines: Vec<String> = translator
        .premises
        .iter()
        .chain(&premises)
        .map(|premise| format!("(assert {premise})"))
        .collect();
    premise_lines.dedup();
    let script = |assertions: &[String]| {
        let mut text = String::from("(set-logic QF_BV)\n(set-option :produce-models true)\n");
        for line in translator
            .declarations
            .iter()
            .chain(&translator.definitions)
            .chain(&premise_lines)
            .chain(assertions)
        {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("(check-sat)\n");
        if !asked.is_empty() {
            text.push_str(&format!("(get-value ({}))\n", asked.join(" ")));
        }
        text
    };
    let traps = match obligations.len() {
        0 => None,
        1 => Some(obligations[0].clone()),
        _ => Some(format!("(or {})", obligations.join(" "))),
    };
    let call = |model: &BTreeMap<String, ModelValue>| {
        let mut text = name.clone();
        for (value, ty) in &translator.arguments {
            text.push(' ');
            text.push_str(&render(value, ty, module, model));
        }
        text
    };
    let mut clauses = Vec::new();
    let mut diagnostics = Vec::new();
    let mut verdict = Verdict::Proved;
    let worse = |verdict: &mut Verdict, found: Verdict| {
        let rank = |verdict: Verdict| match verdict {
            Verdict::Proved => 0,
            Verdict::Unknown => 1,
            Verdict::Error => 2,
            Verdict::Counterexample => 3,
            Verdict::Unsupported => 4,
        };
        if rank(found) > rank(*verdict) {
            *verdict = found;
        }
    };
    // trap_free: where the premises hold, no obligation holds.
    let trap_free = match &traps {
        None => Verdict::Proved,
        Some(traps) => match ask(
            solver,
            &script(&[format!("(assert {traps})")]),
            options.timeout_seconds,
        ) {
            Answer::Unsat => Verdict::Proved,
            Answer::Sat(model) => {
                let what = translator
                    .obligations
                    .iter()
                    .find(|obligation| {
                        matches!(model.get(&obligation.name), Some(ModelValue::Bool(true)))
                    })
                    .map_or("a trap", |obligation| obligation.what);
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::ProveCounterexample,
                    format!(
                        "'the body' of '{name}' can fail: {} traps: {what}",
                        call(&model)
                    ),
                    function.span,
                ));
                Verdict::Counterexample
            }
            Answer::Unknown => {
                diagnostics.push(unknown(&name, options.timeout_seconds, function.span));
                Verdict::Unknown
            }
            Answer::Error(detail) => {
                diagnostics.push(solver_error(&solver.0, &detail, function.span));
                Verdict::Error
            }
        },
    };
    worse(&mut verdict, trap_free);
    clauses.push(ClauseResult {
        ensures: None,
        verdict: trap_free,
    });
    for (index, (condition, span)) in ensures.iter().enumerate() {
        let mut assertions = Vec::new();
        if let Some(traps) = &traps {
            assertions.push(format!("(assert (not {traps}))"));
        }
        assertions.push(format!("(assert (not {condition}))"));
        let found = match ask(solver, &script(&assertions), options.timeout_seconds) {
            Answer::Unsat => Verdict::Proved,
            Answer::Sat(model) => {
                let clause = clause_texts
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "@ensures".into());
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::ProveCounterexample,
                    format!(
                        "'{clause}' of '{name}' can fail: {} returns {}",
                        call(&model),
                        render(&result_value, &function.signature.result, module, &model)
                    ),
                    *span,
                ));
                Verdict::Counterexample
            }
            Answer::Unknown => {
                diagnostics.push(unknown(&name, options.timeout_seconds, function.span));
                Verdict::Unknown
            }
            Answer::Error(detail) => {
                diagnostics.push(solver_error(&solver.0, &detail, function.span));
                Verdict::Error
            }
        };
        worse(&mut verdict, found);
        clauses.push(ClauseResult {
            ensures: Some(index),
            verdict: found,
        });
    }
    Report {
        function: name,
        verdict,
        detail: None,
        clauses,
        diagnostics,
    }
}

fn unknown(name: &str, seconds: u64, span: Span) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::ProveUnknown,
        format!(
            "the solver could not decide '{name}' within {seconds} seconds; raise --prove-timeout or simplify the contract"
        ),
        span,
    )
}

fn solver_error(solver: &str, detail: &str, span: Span) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::ProveSolverError,
        format!("the solver '{solver}' failed: {detail}"),
        span,
    )
}

/// Proves the functions of `module` that have contract lines (all of them, or the ones that
/// `options.functions` names), in the order of their modules and positions. `clauses` gives the
/// `@ensures` lines of each function as written (`Module.name` -> `@ensures \r -> ...`).
pub fn run(
    module: &CheckedModule,
    options: &ProveOptions,
    clauses: &BTreeMap<String, Vec<String>>,
) -> Outcome {
    let mut selected: Vec<&CheckedFunction> = module
        .functions
        .iter()
        .filter(|function| {
            !function.contracts.is_empty()
                && function.origin.module == ModuleOrigin::User
                && function.origin.provenance == Provenance::User
                && function.origin.test.is_none()
                && (options.functions.is_empty() || options.functions.contains(&shown(function)))
        })
        .collect();
    selected.sort_by_key(|function| {
        (
            function.module.clone(),
            function.span.source.unwrap_or(usize::MAX),
            function.span.start,
            function.name.clone(),
        )
    });
    if selected.is_empty() {
        return Outcome {
            reports: Vec::new(),
            error: None,
        };
    }
    let solver = match solver() {
        Ok(solver) => solver,
        Err(error) => {
            return Outcome {
                reports: Vec::new(),
                error: Some(error),
            };
        }
    };
    let empty = Vec::new();
    let reports = selected
        .into_iter()
        .map(|function| {
            let lines = clauses.get(&shown(function)).unwrap_or(&empty);
            prove_function(module, function, &solver, options, lines)
        })
        .collect();
    Outcome {
        reports,
        error: None,
    }
}

/// The `@ensures` lines of each function of `programs` as written, by `Module.name`, for the
/// counterexamples of `run`.
pub fn ensures_lines<'a>(
    programs: impl IntoIterator<Item = (&'a str, &'a crate::syntax::Program)>,
) -> BTreeMap<String, Vec<String>> {
    let mut lines = BTreeMap::new();
    for (module, program) in programs {
        for function in &program.functions {
            let ensures: Vec<String> = crate::docgen::contract_lines(&function.contracts)
                .into_iter()
                .filter(|line| line.starts_with("@ensures"))
                .collect();
            if !ensures.is_empty() {
                lines.insert(format!("{module}.{}", function.name.text), ensures);
            }
        }
    }
    lines
}
