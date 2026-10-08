//! Line and function coverage for `tsuzuri test --coverage` (G18 Phase 2).
//!
//! The plan numbers the regions of the user's typed function bodies: each body, both branches
//! of `if`, each `match` arm, each loop body, and each `try` handler. The code generator counts a
//! region where its block starts (`FunctionEmitter::cover`), so a line's count is the largest
//! count of the regions whose expressions start on it. Specialized copies of a generic body
//! share their spans and so their counters. Standard-library code, tests, and benchmarks are
//! not counted.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::check::{CheckedFunction, CheckedModule, ModuleOrigin, TypedExpr, TypedExprKind};
use crate::diagnostic::Span;
use crate::syntax::Provenance;

/// Where a counted region starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegionKind {
    /// A function body, counted on every entry (including tail calls back to its start).
    Body,
    Then,
    Else,
    /// The body of a `match` arm, counted after its pattern and guard matched.
    Arm,
    /// The body of a `while` or `for` loop, counted on every iteration.
    Loop,
    /// The handler of `try ... with`.
    Handler,
}

/// A region: its source, its span, and what starts it. Two constructs with one span stay apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RegionKey {
    source: usize,
    start: usize,
    end: usize,
    kind: RegionKind,
}

impl RegionKey {
    fn new(kind: RegionKind, span: Span) -> Self {
        Self {
            source: span.source.unwrap_or(0),
            start: span.start,
            end: span.end,
            kind,
        }
    }
}

/// The counters of one program. A region's index is its counter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoveragePlan {
    regions: Vec<RegionKey>,
    index: BTreeMap<RegionKey, usize>,
    /// `(source, offset, region)`: an expression starting at `offset` runs in `region`.
    points: BTreeSet<(usize, usize, usize)>,
    /// The body region of each named user function, with its name as `Module.name`.
    functions: BTreeMap<usize, String>,
}

impl CoveragePlan {
    /// The number of counters.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// The counter of the region of `kind` at `span`, if the plan counts it.
    pub fn region(&self, kind: RegionKind, span: Span) -> Option<usize> {
        self.index.get(&RegionKey::new(kind, span)).copied()
    }
}

/// Whether coverage counts `function`: user code that is not a test, a benchmark, a host
/// call, or a generated wrapper. Lambdas lifted out of counted code count as their own bodies.
pub(crate) fn counted(function: &CheckedFunction) -> bool {
    function.origin.module == ModuleOrigin::User
        && function.origin.test.is_none()
        // Lifted lambdas and tasks count; builtin, case, and export wrappers do not.
        && (!function.module.starts_with('$') || matches!(function.module.as_str(), "$lambda" | "$task"))
        && (function.origin.provenance == Provenance::User || function.origin.parent.is_some())
        && !matches!(
            function.body.kind,
            TypedExprKind::HostCall(..) | TypedExprKind::DynDispatch { .. }
        )
}

/// The name of a function written in source code, as `Geometry::Point.distance`; generated
/// helpers and instance methods have none. Each instance of a generic function has its name.
fn source_name(function: &CheckedFunction) -> Option<String> {
    let (name, instance) = match function.name.split_once(".$mono.") {
        Some((name, _)) => (name, true),
        None => (function.name.as_str(), false),
    };
    if name.starts_with('$')
        || !instance
            && (function.origin.provenance != Provenance::User || function.origin.parent.is_some())
    {
        return None;
    }
    Some(format!("{}.{name}", function.module.replace('.', "::")))
}

/// The coverage plan of a checked, specialized module.
pub fn plan<'a>(module: &'a CheckedModule) -> CoveragePlan {
    let mut keys = BTreeSet::new();
    let mut points = BTreeSet::new();
    let mut functions = BTreeMap::new();
    for function in module.functions.iter().filter(|function| counted(function)) {
        let body = RegionKey::new(RegionKind::Body, function.body.span);
        keys.insert(body);
        if let Some(name) = source_name(function) {
            functions.entry(body).or_insert(name);
        }
        let mut pending = vec![(&function.body, body)];
        while let Some((expression, region)) = pending.pop() {
            if expression.span.start < expression.span.end {
                points.insert((
                    expression.span.source.unwrap_or(0),
                    expression.span.start,
                    region,
                ));
            }
            let mut enter =
                |kind, child: &'a TypedExpr, pending: &mut Vec<(&'a TypedExpr, RegionKey)>| {
                    let key = RegionKey::new(kind, child.span);
                    keys.insert(key);
                    pending.push((child, key));
                };
            match &expression.kind {
                TypedExprKind::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    pending.push((condition, region));
                    enter(RegionKind::Then, then_branch, &mut pending);
                    // `if c then x` has a generated `()` else branch with the span of `x`.
                    let generated = matches!(else_branch.kind, TypedExprKind::Unit)
                        && else_branch.span == then_branch.span;
                    if !generated {
                        enter(RegionKind::Else, else_branch, &mut pending);
                    }
                }
                TypedExprKind::While { condition, body } => {
                    pending.push((condition, region));
                    enter(RegionKind::Loop, body, &mut pending);
                }
                TypedExprKind::ForRange {
                    start,
                    step,
                    finish,
                    body,
                    ..
                } => {
                    pending.extend([(&**start, region), (&**step, region), (&**finish, region)]);
                    enter(RegionKind::Loop, body, &mut pending);
                }
                TypedExprKind::ForEach { source, body, .. } => {
                    pending.push((source, region));
                    enter(RegionKind::Loop, body, &mut pending);
                }
                // Patterns and guards run before an arm is chosen, so they have no count.
                TypedExprKind::Match { value, arms, .. } => {
                    pending.push((value, region));
                    for arm in arms {
                        enter(RegionKind::Arm, &arm.body, &mut pending);
                    }
                }
                TypedExprKind::Try(handled) => {
                    pending.push((&handled.body, region));
                    enter(RegionKind::Handler, &handled.handler, &mut pending);
                    if let Some(finally) = &handled.finally {
                        pending.push((finally, region));
                    }
                }
                _ => pending.extend(
                    expression
                        .children()
                        .into_iter()
                        .map(|child| (child, region)),
                ),
            }
        }
    }
    let regions: Vec<RegionKey> = keys.into_iter().collect();
    let index: BTreeMap<RegionKey, usize> = regions
        .iter()
        .enumerate()
        .map(|(number, key)| (*key, number))
        .collect();
    CoveragePlan {
        points: points
            .into_iter()
            .map(|(source, offset, region)| (source, offset, index[&region]))
            .collect(),
        functions: functions
            .into_iter()
            .map(|(region, name)| (index[&region], name))
            .collect(),
        regions,
        index,
    }
}

/// Adds the native-endian `u64` counters of one test run to `counts`, saturating.
pub fn merge(counts: &mut [u64], bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != counts.len() * 8 {
        return Err(format!(
            "expected {} bytes of coverage counters, found {}",
            counts.len() * 8,
            bytes.len()
        ));
    }
    for (count, chunk) in counts.iter_mut().zip(bytes.chunks_exact(8)) {
        let value = u64::from_ne_bytes(chunk.try_into().expect("chunks of eight bytes"));
        *count = count.saturating_add(value);
    }
    Ok(())
}

/// A user source that the report covers, at its index among the analyzed sources.
#[derive(Clone, Copy, Debug)]
pub struct CoverageSource<'a> {
    /// The path that the report names (lcov's `SF`).
    pub path: &'a str,
    pub text: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCoverage {
    pub line: usize,
    pub name: String,
    pub count: u64,
}

/// The counts of one source file, by line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCoverage {
    pub path: String,
    pub functions: Vec<FunctionCoverage>,
    pub lines: Vec<(usize, u64)>,
}

impl FileCoverage {
    pub fn lines_hit(&self) -> usize {
        self.lines.iter().filter(|(_, count)| *count > 0).count()
    }

    pub fn functions_hit(&self) -> usize {
        self.functions
            .iter()
            .filter(|function| function.count > 0)
            .count()
    }
}

/// The 1-based line of `offset` in a text whose lines start at `starts`.
fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|start| *start <= offset)
}

/// The per-file coverage of `counts` (one per region of `plan`), for the sources that are
/// `Some`, sorted by path. Files without counted code are left out.
pub fn files(
    plan: &CoveragePlan,
    counts: &[u64],
    sources: &[Option<CoverageSource<'_>>],
) -> Vec<FileCoverage> {
    let mut files = BTreeMap::new();
    let mut starts = BTreeMap::new();
    let mut line = |source: usize, offset: usize| -> Option<usize> {
        let text = sources.get(source).copied().flatten()?.text;
        if offset > text.len() {
            return None;
        }
        let starts = starts.entry(source).or_insert_with(|| {
            std::iter::once(0)
                .chain(text.match_indices('\n').map(|(index, _)| index + 1))
                .collect::<Vec<_>>()
        });
        Some(line_of(starts, offset))
    };
    for &(source, offset, region) in &plan.points {
        if let Some(number) = line(source, offset) {
            let file: &mut (BTreeMap<usize, u64>, Vec<FunctionCoverage>) =
                files.entry(source).or_default();
            let count = file.0.entry(number).or_default();
            *count = (*count).max(counts[region]);
        }
    }
    for (&region, name) in &plan.functions {
        let key = plan.regions[region];
        if let Some(number) = line(key.source, key.start) {
            files
                .entry(key.source)
                .or_default()
                .1
                .push(FunctionCoverage {
                    line: number,
                    name: name.clone(),
                    count: counts[region],
                });
        }
    }
    let mut result: Vec<FileCoverage> = files
        .into_iter()
        .map(|(source, (lines, mut functions))| {
            functions
                .sort_by(|left, right| (left.line, &left.name).cmp(&(right.line, &right.name)));
            FileCoverage {
                path: sources[source].expect("covered source").path.to_owned(),
                functions,
                lines: lines.into_iter().collect(),
            }
        })
        .collect();
    result.sort_by(|left, right| left.path.cmp(&right.path));
    result
}

/// The lcov tracefile of `files`.
pub fn render_lcov(files: &[FileCoverage]) -> String {
    let mut output = String::new();
    for file in files {
        let _ = writeln!(output, "TN:\nSF:{}", file.path);
        for function in &file.functions {
            let _ = writeln!(output, "FN:{},{}", function.line, function.name);
        }
        for function in &file.functions {
            let _ = writeln!(output, "FNDA:{},{}", function.count, function.name);
        }
        let _ = writeln!(
            output,
            "FNF:{}\nFNH:{}",
            file.functions.len(),
            file.functions_hit()
        );
        for (line, count) in &file.lines {
            let _ = writeln!(output, "DA:{line},{count}");
        }
        let _ = writeln!(
            output,
            "LF:{}\nLH:{}\nend_of_record",
            file.lines.len(),
            file.lines_hit()
        );
    }
    output
}

/// The totals of a coverage report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Totals {
    pub lines_hit: usize,
    pub lines: usize,
    pub functions_hit: usize,
    pub functions: usize,
}

pub fn totals(files: &[FileCoverage]) -> Totals {
    files.iter().fold(
        Totals {
            lines_hit: 0,
            lines: 0,
            functions_hit: 0,
            functions: 0,
        },
        |totals, file| Totals {
            lines_hit: totals.lines_hit + file.lines_hit(),
            lines: totals.lines + file.lines.len(),
            functions_hit: totals.functions_hit + file.functions_hit(),
            functions: totals.functions + file.functions.len(),
        },
    )
}

/// The one-line text summary, as `coverage: 3/4 lines (75.0%), 1/2 functions`.
pub fn render_summary(totals: Totals, excluded_failed: usize) -> String {
    let percent = if totals.lines == 0 {
        0.0
    } else {
        totals.lines_hit as f64 * 100.0 / totals.lines as f64
    };
    let mut summary = format!(
        "coverage: {}/{} lines ({percent:.1}%), {}/{} functions",
        totals.lines_hit, totals.lines, totals.functions_hit, totals.functions
    );
    if excluded_failed > 0 {
        let _ = write!(
            summary,
            "; excluded {excluded_failed} failed test{}",
            if excluded_failed == 1 { "" } else { "s" }
        );
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(source: usize, start: usize, end: usize, kind: RegionKind) -> RegionKey {
        RegionKey {
            source,
            start,
            end,
            kind,
        }
    }

    /// A plan over the given regions, points, and named body regions.
    fn manual(
        regions: Vec<RegionKey>,
        points: &[(usize, usize, usize)],
        functions: &[(usize, &str)],
    ) -> CoveragePlan {
        CoveragePlan {
            index: regions
                .iter()
                .enumerate()
                .map(|(number, key)| (*key, number))
                .collect(),
            regions,
            points: points.iter().copied().collect(),
            functions: functions
                .iter()
                .map(|(region, name)| (*region, (*name).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn plan_is_sorted_deduplicated_and_skips_std_and_tests() {
        let source = "def classify :: i64 -> i64\nfn classify x =\n    if x > 0 then\n        1\n    else\n        0\n\n\
            def id :: 'a -> 'a\nfn id value = value\n\
            def lengths :: [i64] -> i64\nfn lengths values = Array.length (ref values) + id 1 + { let text = id \"x\"; text.length }\n\
            test \"positive\" = Test.is_true (classify 5 == 1 && (if true then 1 else 2) == 1)\n";
        let module = crate::analyze(source).unwrap();
        let first = plan(&module);
        assert_eq!(first, plan(&module));
        assert!(first.regions.windows(2).all(|pair| pair[0] < pair[1]));
        // Every region and point is in the user's file, outside the test.
        let test = source.find("test \"positive\"").unwrap();
        assert!(
            first
                .regions
                .iter()
                .all(|key| key.source == 0 && key.end <= test)
        );
        assert!(
            first
                .points
                .iter()
                .all(|(source, offset, _)| *source == 0 && *offset < test)
        );
        // classify: body, then, else; id (two specializations share one body); lengths.
        let kinds: Vec<RegionKind> = first.regions.iter().map(|key| key.kind).collect();
        assert_eq!(
            kinds,
            [
                RegionKind::Body,
                RegionKind::Then,
                RegionKind::Else,
                RegionKind::Body,
                RegionKind::Body
            ]
        );
        assert_eq!(
            first.functions.values().cloned().collect::<Vec<_>>(),
            ["Main.classify", "Main.id", "Main.lengths"]
        );
        let then = source.find("1\n    else").unwrap();
        assert_eq!(
            first.region(RegionKind::Then, Span::new(then, then + 1)),
            Some(1)
        );
        assert_eq!(
            first.region(RegionKind::Else, Span::new(then, then + 1)),
            None
        );
    }

    #[test]
    fn lines_take_the_max_region_count() {
        let text = "a b\n\nc\nd";
        let plan = manual(
            vec![
                key(0, 0, 9, RegionKind::Body),
                key(0, 2, 3, RegionKind::Then),
                key(0, 5, 6, RegionKind::Else),
            ],
            &[(0, 0, 0), (0, 2, 1), (0, 5, 2), (0, 9, 0)],
            &[(0, "Main.f")],
        );
        let sources = [Some(CoverageSource {
            path: "/p/Main.tz",
            text,
        })];
        let files = files(&plan, &[0, 2, 0], &sources);
        assert_eq!(
            files,
            [FileCoverage {
                path: "/p/Main.tz".into(),
                functions: vec![FunctionCoverage {
                    line: 1,
                    name: "Main.f".into(),
                    count: 0
                }],
                // Line 1 has counts 0 and 2; line 2 has no point; offset 9 is past the end.
                lines: vec![(1, 2), (3, 0)],
            }]
        );
        assert_eq!(
            totals(&files),
            Totals {
                lines_hit: 1,
                lines: 2,
                functions_hit: 0,
                functions: 1
            }
        );
        assert_eq!(
            render_summary(totals(&files), 0),
            "coverage: 1/2 lines (50.0%), 0/1 functions"
        );
        assert_eq!(
            render_summary(totals(&files), 2),
            "coverage: 1/2 lines (50.0%), 0/1 functions; excluded 2 failed tests"
        );
    }

    #[test]
    fn merge_rejects_wrong_length_and_saturates() {
        let mut counts = vec![u64::MAX, 5];
        assert!(merge(&mut counts, &[0; 7]).is_err());
        assert!(merge(&mut counts, &[0; 24]).is_err());
        assert_eq!(counts, [u64::MAX, 5]);
        let mut bytes = 1u64.to_ne_bytes().to_vec();
        bytes.extend(7u64.to_ne_bytes());
        merge(&mut counts, &bytes).unwrap();
        assert_eq!(counts, [u64::MAX, 12]);
        let mut empty: Vec<u64> = Vec::new();
        merge(&mut empty, &[]).unwrap();
    }

    #[test]
    fn lcov_is_sorted_and_exact() {
        let plan = manual(
            vec![
                key(0, 0, 3, RegionKind::Body),
                key(1, 4, 5, RegionKind::Body),
                key(1, 6, 7, RegionKind::Loop),
            ],
            &[(0, 0, 0), (1, 4, 1), (1, 6, 2)],
            &[(0, "B.g"), (1, "A.f")],
        );
        let sources = [
            Some(CoverageSource {
                path: "/p/B.tz",
                text: "g x",
            }),
            Some(CoverageSource {
                path: "/p/A.tz",
                text: "one\nf\nx",
            }),
            None,
        ];
        let files = files(&plan, &[0, 1, 3], &sources);
        assert_eq!(
            render_lcov(&files),
            "TN:\nSF:/p/A.tz\nFN:2,A.f\nFNDA:1,A.f\nFNF:1\nFNH:1\nDA:2,1\nDA:3,3\nLF:2\nLH:2\nend_of_record\n\
             TN:\nSF:/p/B.tz\nFN:1,B.g\nFNDA:0,B.g\nFNF:1\nFNH:0\nDA:1,0\nLF:1\nLH:0\nend_of_record\n"
        );
        // A source that is not a user source is left out.
        let hidden = [
            Some(CoverageSource {
                path: "/p/B.tz",
                text: "g x",
            }),
            None,
        ];
        assert_eq!(
            files_paths(&super::files(&plan, &[0, 1, 3], &hidden)),
            ["/p/B.tz"]
        );
    }

    fn files_paths(files: &[FileCoverage]) -> Vec<&str> {
        files.iter().map(|file| file.path.as_str()).collect()
    }
}
