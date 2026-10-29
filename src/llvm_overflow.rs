//! A23: the instrumentation of the integer overflow report (`crate::overflow`).
//!
//! Each `+ - * ** -x` of a reported function that a range proof cannot keep within its type gets a
//! check before the operation itself, which stays as it was: the overflow flag of the same
//! operation, and on the cold path a record of the site. The value of the operation never comes
//! from the check, so a program computes the same values with and without the report (D1).

use std::rc::Rc;

use super::*;
use crate::overflow::{self, Operation, ScopeData, Site};
use crate::syntax::Provenance;

/// The sites of one emission, numbered in the order that emission first meets them (D3).
pub(super) struct Report {
    scope: Rc<ScopeData>,
    index: BTreeMap<Site, usize>,
    order: Vec<Site>,
    /// The line that a native program writes for each site, in number order.
    lines: Vec<String>,
    /// The line starts of each source that has a site.
    line_indexes: BTreeMap<usize, overflow::LineIndex>,
    /// `overflow::MAX_SITES`; a unit test takes a smaller one.
    limit: usize,
    exceeded: bool,
}

impl Report {
    pub(super) fn new(scope: Rc<ScopeData>) -> Self {
        Self {
            scope,
            index: BTreeMap::new(),
            order: Vec::new(),
            lines: Vec::new(),
            line_indexes: BTreeMap::new(),
            limit: overflow::MAX_SITES,
            exceeded: false,
        }
    }

    /// Whether the report covers the arithmetic of `function`: a function, lambda, task or test of
    /// the root package's sources, not a benchmark or a helper that the compiler made (D6).
    pub(super) fn covers(&self, function: &CheckedFunction) -> bool {
        function.origin.module == ModuleOrigin::User
            && function.origin.bench.is_none()
            && (!function.module.starts_with('$')
                || matches!(function.module.as_str(), "$lambda" | "$task"))
            && (function.origin.provenance == Provenance::User || function.origin.parent.is_some())
            && !matches!(
                function.body.kind,
                TypedExprKind::HostCall(..) | TypedExprKind::DynDispatch { .. }
            )
            && self.scope.reports(function.span.source)
    }

    /// The number of `site`, a new one if the emission has not met it. `None` past the limit.
    fn number(&mut self, site: Site) -> Option<usize> {
        if let Some(index) = self.index.get(&site) {
            return Some(*index);
        }
        if self.order.len() >= self.limit {
            self.exceeded = true;
            return None;
        }
        let index = self.order.len();
        let source = self
            .scope
            .source(site.source)
            .expect("a site's source is in the scope");
        let lines = self
            .line_indexes
            .entry(site.source)
            .or_insert_with(|| overflow::LineIndex::new(&source.text));
        self.lines.push(format!(
            "warning: {}\n",
            site.message_indexed(source, lines)
        ));
        self.index.insert(site, index);
        self.order.push(site);
        Some(index)
    }

    /// The bytes of the line that a native program writes for the site `index`.
    fn line(&self, index: usize) -> &str {
        &self.lines[index]
    }

    /// The offset that a site's message names: the operator found in the text from `after` to
    /// `before` (between the operands, or before the operand of `-x`), else `fallback`.
    pub(super) fn anchor(
        &self,
        source: Option<usize>,
        after: usize,
        before: usize,
        symbol: &str,
        fallback: usize,
    ) -> usize {
        let Some(text) = source
            .and_then(|source| self.scope.source(source))
            .map(|source| source.text.as_str())
        else {
            return fallback;
        };
        let Some(between) = text.get(after..before.max(after)) else {
            return fallback;
        };
        let mut offset = 0;
        while offset < between.len() {
            let rest = &between[offset..];
            if rest.starts_with("//") {
                offset += rest.find('\n').unwrap_or(rest.len());
            } else if rest.starts_with(symbol) {
                return after + offset;
            } else {
                offset += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        fallback
    }

    /// The globals and functions that the instrumented code uses, once the emission has numbered
    /// every site: the byte of each site, and on WebAssembly the two exports that read them, on
    /// native targets the messages and the function that writes them.
    pub(super) fn runtime(&self, wasm: bool) -> Result<String, Diagnostic> {
        if self.exceeded {
            return Err(Diagnostic::of(
                DiagnosticKind::TooManyOverflowSites,
                format!(
                    "integer overflow report needs more than {} sites; build without --overflow-report",
                    overflow::MAX_SITES
                ),
                Span::default(),
            ));
        }
        let count = self.order.len();
        let mut output = format!(
            "@tsuzuri_overflow_hits = internal global [{count} x i8] zeroinitializer, align 1\n"
        );
        if wasm {
            let _ = write!(
                output,
                "define i32 @tsuzuri_overflow_count() nounwind {{\nentry:\n  ret i32 {count}\n}}\n\
                 define i32 @tsuzuri_overflow_hit(i32 %index) nounwind {{\nentry:\n  \
                 %within = icmp ult i32 %index, {count}\n  br i1 %within, label %read, label %outside\n\
                 read:\n  %slot = getelementptr inbounds i8, ptr @tsuzuri_overflow_hits, i32 %index\n  \
                 %hit = load atomic i8, ptr %slot monotonic, align 1\n  %value = zext i8 %hit to i32\n  \
                 ret i32 %value\noutside:\n  ret i32 0\n}}\n"
            );
            return Ok(output);
        }
        for index in 0..count {
            let text = self.line(index);
            let bytes: String = text.bytes().map(|byte| format!("\\{byte:02X}")).collect();
            let _ = writeln!(
                output,
                "@tz.overflow.message.{index} = private unnamed_addr constant [{} x i8] c\"{bytes}\"",
                text.len()
            );
        }
        let stopped = format!("{}\n", overflow::STOPPED_LINE);
        let bytes: String = stopped
            .bytes()
            .map(|byte| format!("\\{byte:02X}"))
            .collect();
        let _ = write!(
            output,
            "@tz.overflow.reported = internal global i32 0\n\
             @tz.overflow.stopped = private unnamed_addr constant [{length} x i8] c\"{bytes}\"\n\
             define internal void @tz.overflow.report(ptr %message, i64 %length) cold nounwind {{\nentry:\n  \
             %count = atomicrmw add ptr @tz.overflow.reported, i32 1 monotonic\n  \
             %print = icmp ult i32 %count, {printed}\n  br i1 %print, label %write, label %last\n\
             write:\n  call void @tz.overflow.write(ptr %message, i64 %length)\n  ret void\n\
             last:\n  %stop = icmp eq i32 %count, {printed}\n  br i1 %stop, label %stopped, label %done\n\
             stopped:\n  call void @tz.overflow.write(ptr @tz.overflow.stopped, i64 {length})\n  br label %done\n\
             done:\n  ret void\n}}\n\
             define internal void @tz.overflow.write(ptr %data, i64 %length) cold nounwind {{\nentry:\n  \
             br label %loop\nloop:\n  %offset = phi i64 [0, %entry], [%next, %advance]\n  \
             %finished = icmp eq i64 %offset, %length\n  br i1 %finished, label %exit, label %write\n\
             write:\n  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset\n  \
             %remaining = sub i64 %length, %offset\n  \
             %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)\n  \
             %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %advance, label %exit\n\
             advance:\n  %next = add i64 %offset, %written\n  br label %loop\nexit:\n  ret void\n}}\n",
            length = stopped.len(),
            printed = overflow::MAX_PRINTED,
        );
        Ok(output)
    }

    /// Hands the sites of this emission to the scope; the last emission's table is the one that
    /// matches the IR.
    pub(super) fn publish(&self) {
        self.scope.publish(self.order.clone());
    }
}

impl FunctionEmitter<'_, '_> {
    /// Whether the range proof shows that the integer `left operator right` cannot overflow in the
    /// sense of its type: `nsw` for a signed type, `nuw` for an unsigned one (pitfall 1).
    pub(super) fn ranges_proven(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
    ) -> bool {
        let Type::Integer(_, signed) = left.ty else {
            return false;
        };
        if !self.overflow_counted {
            return false;
        }
        let flags = self
            .ranges
            .integer_flags(self.module, operator, left, right);
        if signed { flags.nsw } else { flags.nuw }
    }

    /// Writes the check of one integer operation before the operation itself; the caller skips
    /// the operations that the range proof keeps within their type. For `Negate`, `left` is the
    /// operand and `right` is unused.
    /// The offset of the operator of an operation at `span` whose operator lies between `after`
    /// and `before` (see `Report::anchor`); the start of `span` when the report does not cover
    /// this body.
    pub(super) fn overflow_anchor(
        &self,
        span: Span,
        after: usize,
        before: usize,
        symbol: &str,
    ) -> usize {
        match &self.globals.overflow {
            Some(report) if self.overflow_counted => {
                report.anchor(span.source, after, before, symbol, span.start)
            }
            _ => span.start,
        }
    }

    pub(super) fn overflow_check(
        &mut self,
        operation: Operation,
        ty: &Type,
        span: Span,
        anchor: usize,
        left: &str,
        right: &str,
    ) {
        let Type::Integer(bits, signed) = *ty else {
            return;
        };
        let Some(source) = span.source else {
            return;
        };
        if !self.overflow_counted {
            return;
        }
        let site = Site {
            source,
            start: span.start,
            end: span.end,
            anchor,
            operation,
            bits,
            signed,
        };
        let report = self
            .globals
            .overflow
            .as_mut()
            .expect("a counted function has a report");
        let Some(index) = report.number(site) else {
            return;
        };
        let length = (!self.globals.wasm).then(|| report.line(index).len());
        let (name, a, b) = match operation {
            Operation::Add => ("add", left, right),
            Operation::Subtract => ("sub", left, right),
            Operation::Multiply | Operation::Power => ("mul", left, right),
            Operation::Negate => ("sub", "0", left),
        };
        let (_, overflow) = self.checked_integer_arithmetic(bits, signed, name, a, b);
        self.intrinsics
            .insert("declare i1 @llvm.expect.i1(i1, i1)".into());
        let likely = self.value(format!("call i1 @llvm.expect.i1(i1 {overflow}, i1 false)"));
        let wrapped = self.label();
        let done = self.label();
        self.branch(&likely, &wrapped, &done);
        self.begin(&wrapped);
        let hit = self.value(format!(
            "getelementptr inbounds i8, ptr @tsuzuri_overflow_hits, i64 {index}"
        ));
        match length {
            None => {
                self.instruction(format!("store atomic i8 1, ptr {hit} monotonic, align 1"));
            }
            Some(length) => {
                let old = self.value(format!("atomicrmw xchg ptr {hit}, i8 1 monotonic"));
                let fresh = self.value(format!("icmp eq i8 {old}, 0"));
                let report = self.label();
                self.branch(&fresh, &report, &done);
                self.begin(&report);
                self.instruction(format!(
                    "call void @tz.overflow.report(ptr @tz.overflow.message.{index}, i64 {length})"
                ));
            }
        }
        self.jump(&done);
        self.begin(&done);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overflow::Source;

    #[test]
    fn number_stops_at_the_limit() {
        let _scope = overflow::Scope::enter(vec![Source {
            path: "./Main.tz".into(),
            text: "a + b + c + d".into(),
            reported: true,
        }]);
        let mut report = Report::new(overflow::active().unwrap());
        report.limit = 2;
        let site = |start| Site {
            source: 0,
            start,
            end: start + 1,
            anchor: start,
            operation: Operation::Add,
            bits: 32,
            signed: true,
        };
        assert_eq!(report.number(site(0)), Some(0));
        assert_eq!(report.number(site(4)), Some(1));
        assert_eq!(report.number(site(0)), Some(0));
        assert!(!report.exceeded);
        assert_eq!(report.number(site(8)), None);
        assert!(report.exceeded);
        assert_eq!(report.number(site(4)), Some(1));
        let error = report.runtime(false).unwrap_err();
        assert_eq!(error.code, "E1017");
        assert_eq!(
            error.message,
            "integer overflow report needs more than 65536 sites; build without --overflow-report"
        );
    }
}
