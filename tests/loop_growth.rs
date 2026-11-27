//! A35: regression check that settling loops across the repository corpus stay well below A31's
//! `LoanGrowth` stop limits (`ownership::settled_loop_growth`, `ownership::loop_growth_limits`).
//!
//! Recorded corpus maxima (2026-11-27, base commit `9e081eeb`, 1,511 accepted / 1,812 corpus
//! inputs across `examples/`, `tests/fixtures/`, `tests/conformance/`, `tests/diagnostic_baseline/`,
//! and `std/` doc examples; 110.96 s on 4 threads in the dev profile):
//! - `faster`: 0 / limit 5 (fail at >= 3)
//! - `growing`: 0 / limit 24 (fail at >= 12)
//! - `growing_large` (> 25 loans): 0 / limit 8 (fail at >= 4)
//! - `state` (growing iteration): 0 / limit 8 (fail at >= 4)
//! - `iterations` (informational): 2 (`tests/fixtures/borrowed_records` :: `Main.target_loop`, `[10]`)

use std::path::{Path, PathBuf};
use std::time::Instant;

use tsuzuri::doctest::{self, Expectation};
use tsuzuri::driver::{AnalysisKind, Project};
use tsuzuri::ownership::{
    self, LoopGrowthLimits, LoopGrowthMetric, LoopGrowthSite, SettledLoopGrowth,
};
use tsuzuri::stdlib;
use tsuzuri::syntax::Edition;

const MARGIN_NUMERATOR: usize = 1;
const MARGIN_DENOMINATOR: usize = 2;

#[derive(Clone, Debug)]
struct CorpusSite {
    input: String,
    site: LoopGrowthSite,
}

#[derive(Clone, Debug, Default)]
struct CorpusMetric {
    value: usize,
    found: Option<CorpusSite>,
}

impl CorpusMetric {
    fn merge(&mut self, input: &str, metric: &LoopGrowthMetric) {
        let Some(site) = &metric.site else {
            return;
        };
        if self.found.is_none() || metric.value > self.value {
            self.value = metric.value;
            self.found = Some(CorpusSite {
                input: input.to_owned(),
                site: site.clone(),
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
struct CorpusGrowth {
    faster: CorpusMetric,
    growing: CorpusMetric,
    growing_large: CorpusMetric,
    state: CorpusMetric,
    iterations: CorpusMetric,
}

impl CorpusGrowth {
    fn merge(&mut self, input: &str, stats: &SettledLoopGrowth) {
        self.faster.merge(input, &stats.faster);
        self.growing.merge(input, &stats.growing);
        self.growing_large.merge(input, &stats.growing_large);
        self.state.merge(input, &stats.state);
        self.iterations.merge(input, &stats.iterations);
    }
}

fn check_margins(growth: &CorpusGrowth, limits: LoopGrowthLimits) -> Result<(), String> {
    let checks = [
        ("faster", &growth.faster, limits.faster),
        ("growing", &growth.growing, limits.growing),
        (
            "growing_large",
            &growth.growing_large,
            limits.growing_large_iterations,
        ),
        ("state", &growth.state, limits.state),
    ];
    for (name, metric, limit) in checks {
        if metric.value * MARGIN_DENOMINATOR >= limit * MARGIN_NUMERATOR {
            let where_at = match &metric.found {
                Some(found) => format!(
                    "input `{}` function `{}` (#{}) span {}..{} loans {:?}",
                    found.input,
                    found.site.function_name,
                    found.site.function_id,
                    found.site.body_span.start,
                    found.site.body_span.end,
                    found.site.iteration_loans,
                ),
                None => String::from("<no settled loop recorded>"),
            };
            return Err(format!(
                "A35 margin exceeded for `{name}`: value {} reached >= {MARGIN_NUMERATOR}/{MARGIN_DENOMINATOR} of limit {limit} ({where_at}). \
                 An ownership-check change moved a real settling loop closer to A31's `LoanGrowth` stop; \
                 inspect that loop before changing any fraction or limit, and if the behavior is intended, \
                 recalibrate per `_features/_completed/A31-in-place-writes-keep-stored-borrows.md` and record \
                 the new values in `docs/architecture.md` and `tests/loop_growth.rs`.",
                metric.value,
            ));
        }
    }
    Ok(())
}

enum CorpusItem {
    Path(String, PathBuf),
    StdDoc(String, String),
}

impl CorpusItem {
    fn label(&self) -> &str {
        match self {
            Self::Path(label, _) | Self::StdDoc(label, _) => label,
        }
    }
}

fn collect_tree(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if path.join("Main.tz").is_file() || path.join("Tsuzuri.toml").is_file() {
                let label = path.to_string_lossy().replace('\\', "/");
                out.push((label, path));
            } else {
                collect_tree(&path, out);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("tz") {
            let label = path.to_string_lossy().replace('\\', "/");
            out.push((label, path));
        }
    }
}

fn collect_corpus() -> Vec<CorpusItem> {
    let mut items = Vec::new();

    let mut projects = Vec::new();
    for parent in ["tests/fixtures", "examples"] {
        for entry in std::fs::read_dir(parent).unwrap().flatten() {
            let path = entry.path();
            if path.join("Main.tz").is_file() {
                let label = path.to_string_lossy().replace('\\', "/");
                projects.push((label, path));
            }
        }
    }
    projects.sort_by(|left, right| left.0.cmp(&right.0));
    for (label, path) in projects {
        items.push(CorpusItem::Path(label, path));
    }

    let mut standalone = Vec::new();
    for root in ["tests/conformance", "tests/diagnostic_baseline"] {
        collect_tree(Path::new(root), &mut standalone);
    }
    standalone.sort_by(|left, right| left.0.cmp(&right.0));
    for (label, path) in standalone {
        items.push(CorpusItem::Path(label, path));
    }

    for (index, (path, text)) in stdlib::SOURCES.iter().enumerate() {
        let module = path
            .trim_start_matches("std/")
            .split('.')
            .next()
            .expect("std module name");
        let examples = doctest::examples_in_source(text, Edition::LATEST, index, module)
            .unwrap_or_else(|errors| panic!("{path}: {}", errors[0].message));
        for example in examples {
            if matches!(
                example.expectation,
                Expectation::Ignore | Expectation::Fails(_)
            ) {
                continue;
            }
            items.push(CorpusItem::StdDoc(
                format!("std::{}", example.label()),
                example.program,
            ));
        }
    }

    items
}

fn chain_source(links: usize) -> String {
    let mut source = String::from("record R0 { v: i64 }\n");
    let mut literals = vec![String::from("R0 { v: 1 }")];
    for k in 1..=links {
        source.push_str(&format!("record R{k} {{ a: R{}, v: i64 }}\n", k - 1));
        literals.push(format!("R{k} {{ a: {}, v: {} }}", literals[k - 1], k + 1));
    }
    source.push_str("def run :: i64 -> i64\nfn run n =\n");
    for (k, literal) in literals.iter().enumerate() {
        source.push_str(&format!("    let y{k} = {literal}\n"));
    }
    for k in 0..=links {
        source.push_str(&format!("    let mut q{k} = ref y{k}\n"));
    }
    source.push_str("    let mut i = 0\n    while i < n do\n");
    for k in 0..links {
        source.push_str(&format!("        q{k} = ref (deref q{}).a\n", k + 1));
    }
    source.push_str("        i = i + 1\n    q0.v + q1.v\ndo! IO.writeln (to_string (run 3))\n");
    source
}

fn copy_chain(n: usize, fillers: usize) -> String {
    let mut source = String::from(
        "def run :: i64 -> i64\nfn run n =\n    let y = \"filler\"\n    let a = \"other\"\n",
    );
    for k in 0..n {
        source.push_str(&format!("    let mut u{k} = ref y\n"));
    }
    source.push_str(&format!(
        "    let mut z = 0\n    let mut i = 0\n    while i < n do\n        z = z + String.length u{}\n",
        n - 1
    ));
    for k in (1..n).rev() {
        source.push_str(&format!("        u{k} = u{}\n", k - 1));
    }
    source.push_str("        u0 = ref a\n");
    for _ in 0..fillers {
        source.push_str("        z = z + String.length y\n");
    }
    source.push_str("        i = i + 1\n    z\ndo! IO.writeln (to_string (run 3))\n");
    source
}

fn interpreter(ops: usize, subs: usize) -> String {
    let calls: Vec<String> = (0..subs).map(|j| format!("Call{j}")).collect();
    let opcodes: Vec<String> = (0..ops)
        .map(|j| format!("Op{j} of string * string"))
        .collect();
    let mut source = format!(
        "union Instr = Ret | Halt | {} | {}\n",
        calls.join(" | "),
        opcodes.join(" | ")
    );
    source.push_str(&format!(
        "def exec :: ref [Instr] -> {}i64 -> i64\nfn exec main {}fuel =\n",
        "ref [Instr] -> ".repeat(subs),
        (0..subs).map(|j| format!("sub{j} ")).collect::<String>()
    ));
    source.push_str("    let mut code = main\n    let mut saved = main\n    let mut pc = 0\n    let mut back = 0\n    let mut steps = 0\n    let mut acc = 0\n    let mut running = true\n    while running && steps < fuel do\n        steps = steps + 1\n        match code[pc] with\n");
    for j in 0..subs {
        source.push_str(&format!(
            "        | Call{j} ->\n            saved = code\n            back = pc + 1\n            code = sub{j}\n            pc = 0\n"
        ));
    }
    source.push_str("        | Ret ->\n            code = saved\n            pc = back\n        | Halt -> running = false\n");
    for j in 0..ops {
        source.push_str(&format!(
            "        | Op{j} (a, b) ->\n            acc = acc + String.length a * {} - String.length b\n            pc = pc + 1\n",
            j + 1
        ));
    }
    let main: Vec<String> = (0..subs)
        .map(|j| format!("Call{j}, Op{} (\"x\", \"yz\")", (j + 2) % ops))
        .collect();
    source.push_str(&format!(
        "    acc\ndef run :: i64 -> i64\nfn run n =\n    let main = [Op0 (\"ab\", \"c\"), {}, Halt]\n",
        main.join(", ")
    ));
    for j in 0..subs {
        source.push_str(&format!("    let sub{j} = [Op1 (\"abc\", \"d\"), Ret]\n"));
    }
    source.push_str(&format!(
        "    exec (ref main) {}n\ndo! IO.writeln (to_string (run 100))\n",
        (0..subs)
            .map(|j| format!("(ref sub{j}) "))
            .collect::<String>()
    ));
    source
}

fn keeping_interpreter(ops: usize, subs: usize, depth: usize) -> String {
    let mut source = interpreter(ops, subs);
    let mut declarations = String::new();
    let mut push = String::new();
    let mut pop = String::from("            code = ret0\n            pc = back0\n");
    for r in 0..depth {
        declarations.push_str(&format!(
            "    let mut ret{r} = main\n    let mut back{r} = 0\n"
        ));
    }
    for r in (1..depth).rev() {
        push.push_str(&format!(
            "            ret{r} = ret{}\n            back{r} = back{}\n",
            r - 1,
            r - 1
        ));
    }
    push.push_str("            ret0 = code\n            back0 = pc + 1\n");
    for r in 1..depth {
        pop.push_str(&format!(
            "            ret{} = ret{r}\n            back{} = back{r}\n",
            r - 1,
            r - 1
        ));
    }
    let lengths: Vec<String> = (0..depth).map(|r| format!("Array.length ret{r}")).collect();
    source = source
        .replacen("    let mut saved = main\n", &declarations, 1)
        .replacen("    let mut back = 0\n", "", 1)
        .replace(
            "            saved = code\n            back = pc + 1\n",
            &push,
        )
        .replacen("            code = saved\n            pc = back\n", &pop, 1)
        .replacen(
            "        | Halt -> running = false\n",
            &format!(
                "        | Halt ->\n            acc = acc + {}\n            running = false\n",
                lengths.join(" + ")
            ),
            1,
        );
    for j in 0..subs - 1 {
        source = source.replacen(
            &format!("    let sub{j} = [Op1 (\"abc\", \"d\"), Ret]\n"),
            &format!(
                "    let sub{j} = [Op1 (\"abc\", \"d\"), Call{}, Ret]\n",
                j + 1
            ),
            1,
        );
    }
    source
}

fn pipeline(stages: usize, places: usize, reads: usize) -> String {
    let mut source = String::from(
        "record Instr { op: i64, a: i64, b: i64, name: string }\ndef run :: i64 -> i64\nfn run n =\n",
    );
    for p in 0..places {
        source.push_str(&format!(
            "    let i{p} = Instr {{ op: {p}, a: {}, b: {}, name: \"op{p}\" }}\n",
            p + 1,
            p + 2
        ));
    }
    for j in 0..stages {
        source.push_str(&format!("    let mut s{j} = ref i0\n"));
    }
    source.push_str("    let mut pc = 0\n    let mut acc = 0\n    while pc < n do\n");
    for j in 0..stages {
        for _ in 0..reads {
            source.push_str(&format!("        acc = acc + String.length s{j}.name\n"));
        }
    }
    for j in (1..stages).rev() {
        source.push_str(&format!("        s{j} = s{}\n", j - 1));
    }
    source.push_str(&format!(
        "        s0 = match (pc * 7 + acc) % {places} with\n"
    ));
    for p in 0..places - 1 {
        source.push_str(&format!("            | {p} -> ref i{p}\n"));
    }
    source.push_str(&format!(
        "            | _ -> ref i{}\n        pc = pc + 1\n    acc % 1000\ndo! IO.writeln (to_string (run 50))\n",
        places - 1
    ));
    source
}

fn state_machine(states: usize, reads: usize, history: usize, fields: usize) -> String {
    let mut source = String::from(
        "record State { name: string, accept: bool, weight: i64 }\ndef run :: ref [i64] -> i64\nfn run input =\n",
    );
    for k in 0..states {
        source.push_str(&format!(
            "    let s{k} = State {{ name: \"s{k}\", accept: {}, weight: {} }}\n",
            k % 3 == 0,
            k + 1
        ));
    }
    source.push_str("    let mut cur = ref s0\n");
    for h in 0..history {
        source.push_str(&format!("    let mut prev{h} = ref s0\n"));
    }
    source.push_str("    let mut score = 0\n    let mut matches = 0\n    let mut i = 0\n    while i < input.length do\n        let c = input[i]\n        match c % 4 with\n");
    for arm in 0..4 {
        if arm == 3 {
            source.push_str("        | _ ->\n");
        } else {
            source.push_str(&format!("        | {arm} ->\n"));
        }
        for read in 0..reads.div_ceil(4) {
            if read % 2 == 0 {
                source.push_str(&format!(
                    "            score = score + String.length cur.name * {}\n",
                    arm + 1
                ));
            } else {
                source.push_str(&format!(
                    "            score = score + cur.weight * {read}\n"
                ));
            }
        }
    }
    let mut checks = Vec::new();
    for h in 0..history {
        checks.push(format!("String.length prev{h}.name > 1"));
        if fields >= 2 {
            checks.push(format!("prev{h}.weight > {h}"));
        }
        if fields >= 3 {
            checks.push(format!("prev{h}.accept"));
        }
    }
    source.push_str(&format!(
        "        if cur.accept && {} then matches = matches + 1\n",
        checks.join(" && ")
    ));
    for h in (1..history).rev() {
        source.push_str(&format!("        prev{h} = prev{}\n", h - 1));
    }
    source.push_str(&format!(
        "        prev0 = cur\n        cur = match (cur.weight + c) % {states} with\n"
    ));
    for k in 0..states {
        if k == states - 1 {
            source.push_str(&format!("            | _ -> ref s{k}\n"));
        } else {
            source.push_str(&format!("            | {k} -> ref s{k}\n"));
        }
    }
    source.push_str("        i = i + 1\n    score + matches\ndo! IO.writeln (to_string (run (ref [3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5, 8, 9, 7, 9])))\n");
    source
}

#[test]
fn the_statistic_counts_what_the_stops_count() {
    let chain = tsuzuri::analyze_with_edition(&chain_source(12), Edition::E2027).unwrap();
    let chain_stats = ownership::settled_loop_growth(&chain);
    assert_eq!(chain_stats.faster.value, 1);
    assert_eq!(chain_stats.growing.value, 1);
    assert_eq!(chain_stats.iterations.value, 13);
    assert_eq!(
        chain_stats
            .iterations
            .site
            .as_ref()
            .unwrap()
            .function_name
            .as_str(),
        "Main.run"
    );

    let interp =
        tsuzuri::analyze_with_edition(&keeping_interpreter(20, 4, 4), Edition::E2027).unwrap();
    let interp_stats = ownership::settled_loop_growth(&interp);
    assert_eq!(interp_stats.faster.value, 1);
    assert_eq!(interp_stats.growing.value, 2);
    assert_eq!(
        interp_stats
            .growing
            .site
            .as_ref()
            .unwrap()
            .iteration_loans
            .as_slice(),
        &[180, 716, 720, 724, 728]
    );

    let sm = tsuzuri::analyze_with_edition(&state_machine(16, 72, 6, 2), Edition::E2027).unwrap();
    let sm_stats = ownership::settled_loop_growth(&sm);
    assert_eq!(sm_stats.faster.value, 1);
    assert_eq!(sm_stats.growing.value, 1);
    assert_eq!(
        sm_stats
            .iterations
            .site
            .as_ref()
            .unwrap()
            .iteration_loans
            .as_slice(),
        &[105, 693, 735, 775, 813, 849, 883]
    );

    let pipe = tsuzuri::analyze_with_edition(&pipeline(16, 2, 4), Edition::E2027).unwrap();
    let pipe_stats = ownership::settled_loop_growth(&pipe);
    assert_eq!(pipe_stats.faster.value, 1);
    assert_eq!(pipe_stats.growing.value, 1);
    let pipe_loans = &pipe_stats.iterations.site.as_ref().unwrap().iteration_loans;
    assert_eq!(pipe_loans.first().copied(), Some(71));
    assert_eq!(pipe_loans.last().copied(), Some(671));

    for n in [2, 6, 12] {
        let copied = tsuzuri::analyze_with_edition(&copy_chain(n, 0), Edition::E2027).unwrap();
        let stats = ownership::settled_loop_growth(&copied);
        assert_eq!(stats.growing.value, n - 1, "copy_chain({n}, 0)");
        assert_eq!(stats.growing_large.value, 0, "copy_chain({n}, 0)");
        assert_eq!(stats.iterations.value, n + 1, "copy_chain({n}, 0)");
    }

    let large_copied = tsuzuri::analyze_with_edition(&copy_chain(6, 30), Edition::E2027).unwrap();
    let large_stats = ownership::settled_loop_growth(&large_copied);
    assert_eq!(large_stats.growing.value, 5);
    assert_eq!(large_stats.growing_large.value, 5);

    for stopped in [copy_chain(25, 0), copy_chain(9, 200)] {
        let error = tsuzuri::analyze_with_edition(&stopped, Edition::E2027).unwrap_err();
        assert_eq!(
            (error.code, error.kind),
            ("E1017", Some("loop_ownership_limit"))
        );
    }
}

#[test]
fn the_margins_fail_at_their_fractions() {
    let limits = ownership::loop_growth_limits();
    let dummy_site = || {
        Some(CorpusSite {
            input: String::from("synthetic"),
            site: LoopGrowthSite {
                function_id: 0,
                function_name: String::from("Main.run"),
                body_span: Default::default(),
                iteration_loans: vec![1, 2],
            },
        })
    };

    let make = |faster, growing, growing_large, state| CorpusGrowth {
        faster: CorpusMetric {
            value: faster,
            found: dummy_site(),
        },
        growing: CorpusMetric {
            value: growing,
            found: dummy_site(),
        },
        growing_large: CorpusMetric {
            value: growing_large,
            found: dummy_site(),
        },
        state: CorpusMetric {
            value: state,
            found: dummy_site(),
        },
        iterations: CorpusMetric::default(),
    };

    let pass_faster = (limits.faster * MARGIN_NUMERATOR).div_ceil(MARGIN_DENOMINATOR) - 1;
    let fail_faster = pass_faster + 1;
    assert!(check_margins(&make(pass_faster, 0, 0, 0), limits).is_ok());
    assert!(
        check_margins(&make(fail_faster, 0, 0, 0), limits)
            .unwrap_err()
            .contains("`faster`")
    );

    let pass_growing = (limits.growing * MARGIN_NUMERATOR).div_ceil(MARGIN_DENOMINATOR) - 1;
    let fail_growing = pass_growing + 1;
    assert!(check_margins(&make(0, pass_growing, 0, 0), limits).is_ok());
    assert!(
        check_margins(&make(0, fail_growing, 0, 0), limits)
            .unwrap_err()
            .contains("`growing`")
    );

    let pass_large =
        (limits.growing_large_iterations * MARGIN_NUMERATOR).div_ceil(MARGIN_DENOMINATOR) - 1;
    let fail_large = pass_large + 1;
    assert!(check_margins(&make(0, 0, pass_large, 0), limits).is_ok());
    assert!(
        check_margins(&make(0, 0, fail_large, 0), limits)
            .unwrap_err()
            .contains("`growing_large`")
    );

    let pass_state = (limits.state * MARGIN_NUMERATOR).div_ceil(MARGIN_DENOMINATOR) - 1;
    let fail_state = pass_state + 1;
    assert!(check_margins(&make(0, 0, 0, pass_state), limits).is_ok());
    assert!(
        check_margins(&make(0, 0, 0, fail_state), limits)
            .unwrap_err()
            .contains("`state`")
    );
}

#[test]
fn settling_loops_of_the_corpus_stay_far_from_the_loop_stops() {
    type CorpusOutcome = (usize, Option<SettledLoopGrowth>, Option<String>);
    let started = Instant::now();
    let items = collect_corpus();
    let total = items.len();
    let results: std::sync::Mutex<Vec<CorpusOutcome>> =
        std::sync::Mutex::new(Vec::with_capacity(total));
    let next = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let project = match item {
                        CorpusItem::Path(_, path) => match Project::load_for_tests(path) {
                            Ok(project) => project,
                            Err(_) => {
                                results.lock().unwrap().push((index, None, None));
                                continue;
                            }
                        },
                        CorpusItem::StdDoc(_, program) => Project::single_main(
                            PathBuf::from("<doctest>").join("Main.tz"),
                            program.clone(),
                        ),
                    };
                    match project.analyze_cached(false, AnalysisKind::Tests) {
                        Ok(module) => {
                            let growth = ownership::settled_loop_growth(&module);
                            results.lock().unwrap().push((index, Some(growth), None));
                        }
                        Err(errors) => {
                            let hit_limit = errors
                                .diagnostics
                                .iter()
                                .find(|diagnostic| diagnostic.kind == Some("loop_ownership_limit"));
                            let failure = hit_limit.map(|diagnostic| {
                                format!(
                                    "corpus input `{}` hit E1017 loop_ownership_limit: {}",
                                    item.label(),
                                    diagnostic.message
                                )
                            });
                            results.lock().unwrap().push((index, None, failure));
                        }
                    }
                }
            });
        }
    });

    let mut collected = results.into_inner().unwrap();
    collected.sort_by_key(|(index, _, _)| *index);

    let mut analyzed = 0usize;
    let mut merged = CorpusGrowth::default();
    for (index, stats, limit_failure) in collected {
        if let Some(failure) = limit_failure {
            panic!("{failure}");
        }
        if let Some(stats) = stats {
            analyzed += 1;
            merged.merge(items[index].label(), &stats);
        }
    }

    let limits = ownership::loop_growth_limits();
    let describe = |metric: &CorpusMetric| match &metric.found {
        Some(found) => format!(
            "{} ({} :: {} {:?})",
            metric.value, found.input, found.site.function_name, found.site.iteration_loans
        ),
        None => format!("{} (<none>)", metric.value),
    };
    eprintln!(
        "A35 corpus ({analyzed}/{total} accepted in {:.2}s): faster={} / {}, growing={} / {}, growing_large={} / {} (>{} loans), state={} / {}, iterations={}",
        started.elapsed().as_secs_f64(),
        describe(&merged.faster),
        limits.faster,
        describe(&merged.growing),
        limits.growing,
        describe(&merged.growing_large),
        limits.growing_large_iterations,
        limits.growing_large_loans / 8,
        describe(&merged.state),
        limits.state,
        describe(&merged.iterations),
    );

    if let Err(message) = check_margins(&merged, limits) {
        panic!("{message}");
    }
}
