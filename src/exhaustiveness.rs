//! Compile-time exhaustiveness and usefulness of `match` arms.
//!
//! `pattern_alternatives` records a `CoveragePat` for every arm of an explicit
//! match or function guard from the same resolution that it lowers, so this
//! module never resolves case or recognizer names again. `check_coverage` runs
//! once inference has finished, so literal keys use final types and rounding.
//! The search follows Maranget's pattern matrices. It keeps its pending work in
//! a list instead of recursing per column, so wide patterns cannot exhaust the
//! compiler stack, and a work budget bounds pathological matrices.

use std::rc::Rc;

use super::control::MAX_PATTERN_ALTERNATIVES;
use super::*;

/// Row visits that the matrix search may spend on one match.
const MAX_WORK: usize = 1 << 24;

/// The values that one pattern covers.
#[derive(Clone, Debug)]
pub(super) enum CoveragePat {
    Wildcard,
    Constructor(Constructor, Vec<CoveragePat>),
    Or(Box<CoveragePat>, Box<CoveragePat>),
    And(Box<CoveragePat>, Box<CoveragePat>),
    /// A literal typed against the matched value. Its key is built after
    /// inference finishes, when its type and rounding are final.
    Literal(Box<TypedExpr>),
    /// A total recognizer covers every value only with an irrefutable payload
    /// pattern: the payload's domain cannot be mapped back to the input's.
    Total(Option<Box<CoveragePat>>),
    /// Values the checker cannot describe, such as those a partial recognizer
    /// accepts. They never count as covered, and they never make an arm
    /// unreachable.
    Opaque,
}

impl CoveragePat {
    /// A list literal pattern is nested `Cons` cells that end in `Nil`. A list
    /// longer than any nesting-bounded pattern stays opaque, which is sound for
    /// both checks and keeps every normalized pattern shallow.
    pub(super) fn list(elements: Vec<Self>) -> Self {
        if elements.len() > MAX_NESTING {
            return Self::Opaque;
        }
        elements.into_iter().rev().fold(
            Self::Constructor(Constructor::ListNil, Vec::new()),
            |tail, head| Self::Constructor(Constructor::ListCons, vec![head, tail]),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Constructor {
    Bool(bool),
    Unit,
    Union {
        union_id: usize,
        case_id: usize,
    },
    Tuple(usize),
    Record {
        record_id: usize,
        field_count: usize,
    },
    /// An array of exactly this length; arrays have infinitely many lengths.
    ArrayLen(usize),
    ListNil,
    ListCons,
    /// An integer, floating-point, or string literal of an infinite domain.
    Literal(LiteralKey),
}

/// A literal's resolved type and a canonical text that equal runtime values
/// share: both zeros, and every decimal cohort of one value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LiteralKey {
    ty: Type,
    bits_or_text: String,
}

/// The arms of one explicit match or function guard.
pub(super) struct MatchCoverage {
    pub(super) span: Span,
    pub(super) arms: Vec<CoverageArm>,
}

pub(super) struct CoverageArm {
    pub(super) pattern: CoveragePat,
    /// A guarded arm may be unreachable, but it never covers values.
    pub(super) guarded: bool,
    pub(super) span: Span,
}

/// A normalized pattern: OR, AND, literals, and recognizers are resolved.
enum Node {
    Wild,
    Ctor(Constructor, Vec<Pat>),
}

type Pat = Rc<Node>;

/// One matrix row or query vector, as a persistent list of column patterns.
struct Cell {
    head: Pat,
    tail: Row,
    /// Constructor columns in this cell and its tail; a row without any
    /// covers every remaining value.
    concrete: usize,
}

type Row = Option<Rc<Cell>>;

impl Drop for Cell {
    fn drop(&mut self) {
        let mut tail = self.tail.take();
        while let Some(cell) = tail {
            tail = match Rc::try_unwrap(cell) {
                Ok(mut cell) => cell.tail.take(),
                Err(_) => None,
            };
        }
    }
}

fn push(head: Pat, tail: Row) -> Row {
    let concrete = usize::from(matches!(*head, Node::Ctor(..))) + concrete(&tail);
    Some(Rc::new(Cell {
        head,
        tail,
        concrete,
    }))
}

fn concrete(row: &Row) -> usize {
    row.as_ref().map_or(0, |cell| cell.concrete)
}

#[derive(Clone, Copy)]
enum Mode {
    /// A previous unguarded arm: values it may not cover count as uncovered.
    Row,
    /// The arm under test: values it may match count as matched.
    Query,
}

#[derive(Clone)]
enum Witness {
    Wild,
    Ctor(Constructor, Vec<Witness>),
}

enum Step {
    /// The next `arity` witnesses are the arguments of this constructor.
    Apply(Constructor, usize),
    /// A column that no row decides.
    Head(Witness),
}

struct Task {
    rows: Vec<Row>,
    query: Row,
    trace: Option<usize>,
}

enum Signature {
    Complete(Vec<Constructor>),
    Missing(Witness),
}

enum Literal {
    Constructor(Constructor),
    /// A NaN literal equals no value.
    Never,
}

struct Search<'c, 'a> {
    checker: &'c Checker<'a>,
    span: Span,
    wild: Pat,
    work: usize,
    /// Witness steps; each task refers to its latest step.
    steps: Vec<(Step, Option<usize>)>,
    distinct: usize,
}

impl Checker<'_> {
    /// Rejects the first non-exhaustive match with `E1021` and returns a
    /// `W1003` warning for each arm that previous unguarded arms cover.
    pub(super) fn check_coverage(&mut self) -> Result<Vec<Diagnostic>, Diagnostic> {
        let matches = std::mem::take(&mut self.coverage);
        let mut warnings = Vec::new();
        for coverage in &matches {
            let mut search = Search {
                checker: self,
                span: coverage.span,
                wild: Rc::new(Node::Wild),
                work: 0,
                steps: Vec::new(),
                distinct: 0,
            };
            let mut rows: Vec<Row> = Vec::new();
            for arm in &coverage.arms {
                let queries = search.normalize(&arm.pattern, Mode::Query)?;
                // A pattern that matches no value by itself is not covered by
                // previous arms, so it is not reported as unreachable.
                let mut useful = queries.is_empty();
                for query in queries {
                    if search
                        .run(rows.clone(), push(query, None), false)?
                        .is_some()
                    {
                        useful = true;
                        break;
                    }
                }
                if !useful {
                    warnings.push(Diagnostic::new(
                        "W1003",
                        "unreachable match arm; previous patterns already cover this arm",
                        arm.span,
                    ));
                }
                if !arm.guarded {
                    for pattern in search.normalize(&arm.pattern, Mode::Row)? {
                        rows.push(push(pattern, None));
                    }
                }
            }
            let wild = search.wild.clone();
            if let Some(witness) = search.run(rows, push(wild, None), true)? {
                return Err(Diagnostic::new(
                    "E1021",
                    format!(
                        "match is not exhaustive; missing: {}",
                        self.witness(&witness, coverage.span)
                    ),
                    coverage.span,
                ));
            }
        }
        Ok(warnings)
    }

    fn witness(&self, witness: &Witness, span: Span) -> String {
        let Witness::Ctor(constructor, arguments) = witness else {
            return "_".into();
        };
        let argument = |witness: &Witness| {
            let text = self.witness(witness, span);
            if Self::atomic(witness) {
                text
            } else {
                format!("({text})")
            }
        };
        let list = |arguments: &[Witness]| {
            arguments
                .iter()
                .map(|argument| self.witness(argument, span))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match constructor {
            Constructor::Bool(value) => value.to_string(),
            Constructor::Unit => "()".into(),
            Constructor::Union { union_id, case_id } => {
                let name = self.case_name(*union_id, *case_id, span);
                match arguments.first() {
                    Some(payload) => format!("{name} {}", argument(payload)),
                    None => name,
                }
            }
            Constructor::Tuple(_) => format!("({})", list(arguments)),
            Constructor::Record { record_id, .. } => {
                let fields: Vec<_> = self.types.records[*record_id]
                    .fields
                    .iter()
                    .zip(arguments)
                    .filter(|(_, witness)| !matches!(witness, Witness::Wild))
                    .map(|((name, _), witness)| format!("{name} = {}", self.witness(witness, span)))
                    .collect();
                if fields.is_empty() {
                    "_".into()
                } else {
                    format!("{{ {} }}", fields.join(", "))
                }
            }
            Constructor::ArrayLen(_) => format!("[{}]", list(arguments)),
            Constructor::ListNil => "[||]".into(),
            Constructor::ListCons => format!(
                "{} :: {}",
                argument(&arguments[0]),
                self.witness(&arguments[1], span)
            ),
            Constructor::Literal(_) => "_".into(),
        }
    }

    /// Whether a witness can be a payload or list head without parentheses.
    fn atomic(witness: &Witness) -> bool {
        match witness {
            Witness::Wild => true,
            Witness::Ctor(Constructor::Union { .. } | Constructor::ListCons, arguments) => {
                arguments.is_empty()
            }
            Witness::Ctor(Constructor::Record { .. }, arguments) => arguments
                .iter()
                .all(|argument| matches!(argument, Witness::Wild)),
            Witness::Ctor(..) => true,
        }
    }

    /// A case's plain name when it resolves to the same case here, and its
    /// `Module.Case` path otherwise.
    fn case_name(&self, union_id: usize, case_id: usize, span: Span) -> String {
        let union = &self.types.unions[union_id];
        let name = &union.cases[case_id].0;
        match self.names.case_path(self.module, name, span) {
            Ok(Some(case)) if case.info.id == union_id && case.case == case_id => name.clone(),
            _ => format!("{}.{name}", union.module()),
        }
    }

    fn literal(&self, literal: &TypedExpr, distinct: &mut usize) -> Literal {
        let (negative, value) = match &literal.kind {
            TypedExprKind::Unary(UnaryOp::Negate, value) => (true, &**value),
            _ => (false, literal),
        };
        let ty = self.inference.resolve(&literal.ty);
        let text = match &value.kind {
            TypedExprKind::Bool(value) if !negative => {
                return Literal::Constructor(Constructor::Bool(*value));
            }
            TypedExprKind::Unit if !negative => return Literal::Constructor(Constructor::Unit),
            TypedExprKind::String(text) if !negative => Some(text.clone()),
            TypedExprKind::Int(bits) if !negative => Some(bits.to_string()),
            TypedExprKind::GenericInteger(magnitude, minus) if !negative => {
                if ty.is_integer() {
                    match Self::integer_literal(
                        *magnitude,
                        None,
                        Some(&ty),
                        *minus,
                        literal.span,
                        &self.types,
                    ) {
                        Ok((TypedExprKind::Int(bits), _)) => Some(bits.to_string()),
                        _ => None,
                    }
                } else {
                    let sign = if *minus && *magnitude != 0 { "-" } else { "" };
                    Some(format!("{sign}{magnitude}"))
                }
            }
            TypedExprKind::Float(bits) => float_key(&ty, bits, negative),
            TypedExprKind::GenericFloat(text) if ty.is_float() => {
                crate::numeric::float_literal(text, &ty, literal.span)
                    .ok()
                    .and_then(|bits| float_key(&ty, &bits, negative))
            }
            TypedExprKind::GenericFloat(text) => {
                Some(format!("{}{text}", if negative { "-" } else { "" }))
            }
            _ => None,
        };
        let bits_or_text = match text {
            Some(text) if text == "NaN" => return Literal::Never,
            Some(text) => text,
            // A key that equals no other literal only loses precision.
            None => {
                *distinct += 1;
                format!("#{distinct}")
            }
        };
        Literal::Constructor(Constructor::Literal(LiteralKey { ty, bits_or_text }))
    }
}

/// The canonical text of a floating-point literal: `"0"` for both zeros, the
/// normalized coefficient and exponent of a decimal, or `"NaN"`.
fn float_key(ty: &Type, bits: &str, negative: bool) -> Option<String> {
    match *ty {
        Type::Binary(width) => {
            // f32 literals are stored as their exact f64 value.
            let (mut raw, width) = if width == 32 || width == 64 {
                (u128::from_str_radix(bits.strip_prefix("0x")?, 16).ok()?, 64)
            } else {
                (bits.parse::<u128>().ok()?, u32::from(width))
            };
            let (exponent_bits, fraction_bits) = match width {
                16 => (5, 10),
                64 => (11, 52),
                128 => (15, 112),
                _ => return None,
            };
            let sign = 1u128 << (width - 1);
            if negative {
                raw ^= sign;
            }
            let magnitude = raw & (sign - 1);
            let exponent = ((1u128 << exponent_bits) - 1) << fraction_bits;
            let fraction = (1u128 << fraction_bits) - 1;
            Some(
                if magnitude & exponent == exponent && magnitude & fraction != 0 {
                    "NaN".into()
                } else if magnitude == 0 {
                    "0".into()
                } else {
                    format!("{raw:x}")
                },
            )
        }
        Type::Decimal(width) => {
            let mut raw = bits.parse::<u128>().ok()?;
            let (precision, minimum, _, fraction) = crate::numeric::decimal_format(width);
            let width = u32::from(width);
            let sign = 1u128 << (width - 1);
            if negative {
                raw ^= sign;
            }
            let minus = raw & sign != 0;
            raw &= sign - 1;
            let combination = raw >> (width - 6);
            if combination == 31 {
                return Some("NaN".into());
            }
            if combination == 30 {
                return Some(format!("{}inf", if minus { "-" } else { "" }));
            }
            let mask = |bits: u32| (1u128 << bits) - 1;
            // Mirrors the runtime decoder, including non-canonical coefficients.
            let (exponent, mut coefficient) = if raw >> (width - 3) == 3 && width != 128 {
                (
                    (raw >> (fraction - 2)) & mask(width - fraction - 1),
                    (raw & mask(fraction - 2)) | (1u128 << fraction),
                )
            } else {
                (raw >> fraction, raw & mask(fraction))
            };
            if coefficient >= 10u128.pow(precision as u32) || (width == 128 && raw >> 125 == 3) {
                coefficient = 0;
            }
            if coefficient == 0 {
                return Some("0".into());
            }
            let mut exponent = exponent as i64 + i64::from(minimum);
            while coefficient % 10 == 0 {
                coefficient /= 10;
                exponent += 1;
            }
            Some(format!(
                "{}{coefficient}e{exponent}",
                if minus { "-" } else { "" }
            ))
        }
        _ => None,
    }
}

impl Search<'_, '_> {
    fn limit(&self, alternatives: usize) -> Result<(), Diagnostic> {
        if alternatives > MAX_PATTERN_ALTERNATIVES {
            return Err(Diagnostic::new(
                "E1017",
                "pattern alternatives exceed the compiler limit; split the match",
                self.span,
            ));
        }
        Ok(())
    }

    fn charge(&mut self, work: usize) -> Result<(), Diagnostic> {
        self.work = self.work.saturating_add(work);
        if self.work > MAX_WORK {
            return Err(Diagnostic::new(
                "E1017",
                "the match is too large to check for exhaustiveness; split the match",
                self.span,
            ));
        }
        Ok(())
    }

    /// Expands a pattern into its OR alternatives, intersecting AND sides.
    fn normalize(&mut self, pattern: &CoveragePat, mode: Mode) -> Result<Vec<Pat>, Diagnostic> {
        Ok(match pattern {
            CoveragePat::Wildcard | CoveragePat::Total(None) => vec![self.wild.clone()],
            CoveragePat::Constructor(constructor, arguments) => {
                let mut products = vec![Vec::new()];
                for argument in arguments {
                    let alternatives = self.normalize(argument, mode)?;
                    self.limit(products.len().saturating_mul(alternatives.len()))?;
                    products = products
                        .iter()
                        .flat_map(|prefix| {
                            alternatives.iter().map(move |alternative| {
                                let mut product: Vec<Pat> = Vec::clone(prefix);
                                product.push(alternative.clone());
                                product
                            })
                        })
                        .collect();
                }
                products
                    .into_iter()
                    .map(|arguments| Rc::new(Node::Ctor(constructor.clone(), arguments)))
                    .collect()
            }
            CoveragePat::Or(left, right) => {
                let mut alternatives = self.normalize(left, mode)?;
                alternatives.extend(self.normalize(right, mode)?);
                self.limit(alternatives.len())?;
                alternatives
            }
            CoveragePat::And(left, right) => {
                let left = self.normalize(left, mode)?;
                let right = self.normalize(right, mode)?;
                self.limit(left.len().saturating_mul(right.len()))?;
                left.iter()
                    .flat_map(|left| right.iter().filter_map(|right| intersect(left, right)))
                    .collect()
            }
            CoveragePat::Literal(literal) => {
                match self.checker.literal(literal, &mut self.distinct) {
                    Literal::Constructor(constructor) => {
                        vec![Rc::new(Node::Ctor(constructor, Vec::new()))]
                    }
                    Literal::Never => Vec::new(),
                }
            }
            CoveragePat::Total(Some(payload)) => {
                let rows = self
                    .normalize(payload, Mode::Row)?
                    .into_iter()
                    .map(|pattern| push(pattern, None))
                    .collect();
                let wild = self.wild.clone();
                if self.run(rows, push(wild, None), false)?.is_none() {
                    vec![self.wild.clone()]
                } else {
                    self.opaque(mode)
                }
            }
            CoveragePat::Opaque => self.opaque(mode),
        })
    }

    fn opaque(&self, mode: Mode) -> Vec<Pat> {
        match mode {
            Mode::Row => Vec::new(),
            Mode::Query => vec![self.wild.clone()],
        }
    }

    /// Finds a value that `query` matches and no row covers. The value is
    /// described only when `describe` is set; otherwise it is a placeholder.
    fn run(
        &mut self,
        rows: Vec<Row>,
        query: Row,
        describe: bool,
    ) -> Result<Option<Witness>, Diagnostic> {
        self.steps.clear();
        let mut tasks = vec![Task {
            rows,
            query,
            trace: None,
        }];
        while let Some(Task { rows, query, trace }) = tasks.pop() {
            self.charge(rows.len() + 1)?;
            // A row of wildcards covers every remaining value, which also ends
            // the search once every column is decided.
            if rows.iter().any(|row| concrete(row) == 0) {
                continue;
            }
            let Some(cell) = query else {
                return Ok(Some(if describe {
                    self.rebuild(trace)
                } else {
                    Witness::Wild
                }));
            };
            match &*cell.head {
                Node::Ctor(constructor, arguments) => {
                    let rows = self.specialize(&rows, constructor, arguments.len())?;
                    let query = arguments
                        .iter()
                        .rev()
                        .fold(cell.tail.clone(), |tail, argument| {
                            push(argument.clone(), tail)
                        });
                    let trace = self.step(
                        describe,
                        trace,
                        Step::Apply(constructor.clone(), arguments.len()),
                    );
                    tasks.push(Task { rows, query, trace });
                }
                Node::Wild => match self.signature(&rows) {
                    Signature::Complete(constructors) => {
                        // The first constructor is searched first, which keeps
                        // the reported witness stable.
                        for constructor in constructors.into_iter().rev() {
                            let arity = self.arity(&constructor);
                            let rows = self.specialize(&rows, &constructor, arity)?;
                            let query = (0..arity)
                                .fold(cell.tail.clone(), |tail, _| push(self.wild.clone(), tail));
                            let trace = self.step(describe, trace, Step::Apply(constructor, arity));
                            tasks.push(Task { rows, query, trace });
                        }
                    }
                    Signature::Missing(head) => {
                        let rows: Vec<Row> = rows
                            .iter()
                            .filter_map(|row| {
                                let cell = row.as_ref().expect("rows are as wide as the query");
                                matches!(*cell.head, Node::Wild).then(|| cell.tail.clone())
                            })
                            .collect();
                        let trace = self.step(describe, trace, Step::Head(head));
                        tasks.push(Task {
                            rows,
                            query: cell.tail.clone(),
                            trace,
                        });
                    }
                },
            }
        }
        Ok(None)
    }

    fn step(&mut self, describe: bool, parent: Option<usize>, step: Step) -> Option<usize> {
        if !describe {
            return None;
        }
        self.steps.push((step, parent));
        Some(self.steps.len() - 1)
    }

    /// Replays the steps from the last column back to the first.
    fn rebuild(&self, mut trace: Option<usize>) -> Witness {
        let mut witnesses = Vec::new();
        while let Some(index) = trace {
            let (step, parent) = &self.steps[index];
            match step {
                Step::Apply(constructor, arity) => {
                    let arguments = (0..*arity)
                        .map(|_| witnesses.pop().expect("each argument has a witness"))
                        .collect();
                    witnesses.push(Witness::Ctor(constructor.clone(), arguments));
                }
                Step::Head(head) => witnesses.push(head.clone()),
            }
            trace = *parent;
        }
        witnesses.pop().expect("the query has one column")
    }

    /// Rows that match `constructor`, with its arguments as new columns.
    fn specialize(
        &mut self,
        rows: &[Row],
        constructor: &Constructor,
        arity: usize,
    ) -> Result<Vec<Row>, Diagnostic> {
        self.charge(rows.len().saturating_mul(arity))?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                let cell = row.as_ref().expect("rows are as wide as the query");
                match &*cell.head {
                    Node::Wild => Some(
                        (0..arity).fold(cell.tail.clone(), |tail, _| push(self.wild.clone(), tail)),
                    ),
                    Node::Ctor(head, arguments) if head == constructor => Some(
                        arguments
                            .iter()
                            .rev()
                            .fold(cell.tail.clone(), |tail, argument| {
                                push(argument.clone(), tail)
                            }),
                    ),
                    Node::Ctor(..) => None,
                }
            })
            .collect())
    }

    fn arity(&self, constructor: &Constructor) -> usize {
        match constructor {
            Constructor::Union { union_id, case_id } => usize::from(
                self.checker.types.unions[*union_id].cases[*case_id]
                    .1
                    .is_some(),
            ),
            Constructor::Tuple(count) | Constructor::ArrayLen(count) => *count,
            Constructor::Record { field_count, .. } => *field_count,
            Constructor::ListCons => 2,
            Constructor::Bool(_)
            | Constructor::Unit
            | Constructor::ListNil
            | Constructor::Literal(_) => 0,
        }
    }

    /// Whether the constructors that head the first column cover its type, and
    /// otherwise a head for the witness: a missing constructor or `_`.
    fn signature(&self, rows: &[Row]) -> Signature {
        let mut heads = rows.iter().filter_map(|row| {
            match &*row.as_ref().expect("rows are as wide as the query").head {
                Node::Ctor(constructor, _) => Some(constructor),
                Node::Wild => None,
            }
        });
        let Some(first) = heads.next() else {
            return Signature::Missing(Witness::Wild);
        };
        let all = match first {
            Constructor::Bool(_) => vec![Constructor::Bool(true), Constructor::Bool(false)],
            Constructor::Union { union_id, .. } => {
                (0..self.checker.types.unions[*union_id].cases.len())
                    .map(|case_id| Constructor::Union {
                        union_id: *union_id,
                        case_id,
                    })
                    .collect()
            }
            Constructor::ListNil | Constructor::ListCons => {
                vec![Constructor::ListNil, Constructor::ListCons]
            }
            Constructor::Unit | Constructor::Tuple(_) | Constructor::Record { .. } => {
                return Signature::Complete(vec![first.clone()]);
            }
            Constructor::ArrayLen(_) | Constructor::Literal(_) => {
                return Signature::Missing(Witness::Wild);
            }
        };
        let index = |constructor: &Constructor| match constructor {
            Constructor::Bool(value) => usize::from(!value),
            Constructor::Union { case_id, .. } => *case_id,
            Constructor::ListCons => 1,
            _ => 0,
        };
        let mut present = vec![false; all.len()];
        present[index(first)] = true;
        for head in heads {
            present[index(head)] = true;
        }
        match present.iter().position(|present| !present) {
            Some(missing) => {
                let arity = self.arity(&all[missing]);
                Signature::Missing(Witness::Ctor(
                    all[missing].clone(),
                    vec![Witness::Wild; arity],
                ))
            }
            None => Signature::Complete(all),
        }
    }
}

/// The values both patterns match, or `None` when they share none.
fn intersect(left: &Pat, right: &Pat) -> Option<Pat> {
    match (&**left, &**right) {
        (Node::Wild, _) => Some(right.clone()),
        (_, Node::Wild) => Some(left.clone()),
        (Node::Ctor(a, left), Node::Ctor(b, right)) if a == b => {
            let arguments = left
                .iter()
                .zip(right)
                .map(|(left, right)| intersect(left, right))
                .collect::<Option<Vec<_>>>()?;
            Some(Rc::new(Node::Ctor(a.clone(), arguments)))
        }
        _ => None,
    }
}
