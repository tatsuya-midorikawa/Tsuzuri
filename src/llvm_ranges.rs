//! The integer overflow flags that range proofs allow (PR02), and the audit that checks them.
//!
//! An integer `+ - *` wraps, so `nsw` and `nuw` (the result is poison when the instruction
//! overflows) may only go on an instruction that a proof shows cannot overflow in that sense. A
//! wrong flag is a miscompilation that tests rarely find, so the emitter has three modes:
//! `Off` writes the wrapping instruction, `Proven` writes the flags, and `Audit` writes the
//! wrapping instruction behind a check that traps when a proof is wrong. The compiler's own
//! commands read the mode from `TSUZURI_RANGE_FLAGS`; the library takes it as an argument.

use super::*;

/// The environment variable that chooses the mode of the compiler's commands and test suites.
pub const ENVIRONMENT_VARIABLE: &str = "TSUZURI_RANGE_FLAGS";

/// What the emitter writes where a proof shows that an integer `+ - *` cannot overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeFlags {
    /// The wrapping instruction without flags.
    Off,
    /// The instruction with `nsw` and `nuw` where the proof allows them.
    Proven,
    /// The wrapping instruction after a check that traps (`TrapKind::Overflow`) if the proof is
    /// wrong. A test mode: every flag that `Proven` writes is a trap here, so a suite that passes
    /// shows that no proof failed on its inputs.
    Audit,
}

impl RangeFlags {
    /// The mode when nothing chooses one.
    pub const DEFAULT: Self = Self::Off;

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Proven => "proven",
            Self::Audit => "audit",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [Self::Off, Self::Proven, Self::Audit]
            .into_iter()
            .find(|mode| mode.name() == text)
    }

    /// The mode that `TSUZURI_RANGE_FLAGS` names; the default when it is unset or empty. Any other
    /// value is an error: a mode that was asked for and does not exist must not fall back silently.
    pub fn from_environment() -> Result<Self, Diagnostic> {
        let invalid = |value: &str| {
            Diagnostic::new(
                "E2000",
                format!("{ENVIRONMENT_VARIABLE} must be off, proven or audit, not '{value}'"),
                Span::default(),
            )
        };
        match std::env::var(ENVIRONMENT_VARIABLE) {
            Ok(value) if value.is_empty() => Ok(Self::DEFAULT),
            Ok(value) => Self::parse(&value).ok_or_else(|| invalid(&value)),
            Err(std::env::VarError::NotPresent) => Ok(Self::DEFAULT),
            Err(std::env::VarError::NotUnicode(value)) => Err(invalid(&value.to_string_lossy())),
        }
    }
}

impl Default for RangeFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FunctionEmitter<'_, '_> {
    /// The words that `flags` puts before the type of an instruction that a proof covers.
    pub(super) fn flag_text(&self, flags: Flags) -> &'static str {
        match self.globals.range_flags {
            RangeFlags::Proven => flags.text(),
            RangeFlags::Off | RangeFlags::Audit => "",
        }
    }

    /// The label of the block that holds a loop increment, given `advance`, the block that the
    /// loop jumps to for the next iteration. They are the same block, except in audit mode, where
    /// `advance` first checks the increment and then jumps to the block that holds it: the
    /// predecessor that the loop header's `phi` names is the block that holds the increment.
    pub(super) fn increment_block(&mut self, advance: &str) -> String {
        if self.globals.range_flags == RangeFlags::Audit {
            self.label()
        } else {
            advance.to_owned()
        }
    }

    /// Starts a loop increment at the head of `advance`: in audit mode it checks that
    /// `current + stride` stays within the senses of `flags`, then moves on to `step`.
    pub(super) fn audit_increment(
        &mut self,
        step: &str,
        bits: u16,
        current: &str,
        stride: &str,
        flags: Flags,
    ) {
        if self.globals.range_flags != RangeFlags::Audit {
            return;
        }
        self.audit("add", bits, current, stride, flags);
        self.jump(step);
        self.begin(step);
    }

    /// The flag words of the integer `left operator right` whose operands have the values `lhs`
    /// and `rhs`. Proven mode returns what the intervals of the operands prove; audit mode
    /// writes no words and traps where a proof is wrong; off mode asks nothing.
    pub(super) fn arithmetic_flags(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        lhs: &str,
        rhs: &str,
    ) -> &'static str {
        if self.globals.range_flags == RangeFlags::Off {
            return "";
        }
        let flags = self
            .ranges
            .integer_flags(self.module, operator, left, right);
        let Type::Integer(bits, _) = left.ty else {
            return "";
        };
        if flags == Flags::default() {
            return "";
        }
        if self.globals.range_flags == RangeFlags::Proven {
            return flags.text();
        }
        let operation = match operator {
            BinaryOp::Add => "add",
            BinaryOp::Subtract => "sub",
            _ => "mul",
        };
        self.audit(operation, bits, lhs, rhs, flags);
        ""
    }

    /// Traps if `left operation right` overflows in a sense that `flags` claims it cannot.
    fn audit(&mut self, operation: &str, bits: u16, left: &str, right: &str, flags: Flags) {
        for (claimed, signed) in [(flags.nsw, true), (flags.nuw, false)] {
            if !claimed {
                continue;
            }
            let (_, overflow) =
                self.checked_integer_arithmetic(bits, signed, operation, left, right);
            let within = self.value(format!("xor i1 {overflow}, true"));
            self.guard(&within, TrapKind::Overflow);
        }
    }
}
