//! Integer intervals and the overflow flags that they prove (PR02).
//!
//! An integer `+ - *` wraps in Tsuzuri, so LLVM's `nsw` and `nuw` (an overflow makes the result
//! poison) belong only on an instruction that a proof shows cannot overflow in that sense. This
//! module holds the pure part of the proof: the arithmetic on intervals of mathematical integers.
//! Every function returns a set that contains every value the operation can produce, so a wider
//! answer is always sound and a narrower one needs the proof in the comment above it.
//!
//! Intervals hold `i128` bounds; an integer type of at most 64 bits fits, and the product of two of
//! its values is the only thing that can leave `i128`, where the bounds saturate. A saturated bound
//! is never inside a 64-bit type, so it can only withhold a flag or widen a result.

use std::cmp::{max, min};

/// An integer type of at most 64 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Int {
    pub bits: u32,
    pub signed: bool,
}

impl Int {
    /// `None` for a width that the intervals do not cover (more than 64 bits, or none).
    pub(crate) fn new(bits: u16, signed: bool) -> Option<Self> {
        (1..=64).contains(&bits).then_some(Self {
            bits: u32::from(bits),
            signed,
        })
    }

    fn modulus(self) -> i128 {
        1 << self.bits
    }

    pub(crate) fn min(self) -> i128 {
        if self.signed {
            -(self.modulus() >> 1)
        } else {
            0
        }
    }

    pub(crate) fn max(self) -> i128 {
        if self.signed {
            (self.modulus() >> 1) - 1
        } else {
            self.modulus() - 1
        }
    }

    /// Every value of the type.
    pub(crate) fn full(self) -> Interval {
        Interval {
            low: self.min(),
            high: self.max(),
        }
    }

    fn with_sign(self, signed: bool) -> Self {
        Self { signed, ..self }
    }
}

/// The inclusive range of integers that a value can take: `low <= high`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Interval {
    pub low: i128,
    pub high: i128,
}

impl Interval {
    pub(crate) fn new(low: i128, high: i128) -> Self {
        debug_assert!(low <= high, "{low} > {high}");
        Self { low, high }
    }

    pub(crate) fn point(value: i128) -> Self {
        Self::new(value, value)
    }

    pub(crate) fn within(self, outer: Self) -> bool {
        outer.low <= self.low && self.high <= outer.high
    }

    /// The smallest interval that holds both.
    pub(crate) fn hull(self, other: Self) -> Self {
        Self::new(min(self.low, other.low), max(self.high, other.high))
    }

    /// The values in both; `None` when there are none.
    pub(crate) fn meet(self, other: Self) -> Option<Self> {
        let (low, high) = (max(self.low, other.low), min(self.high, other.high));
        (low <= high).then(|| Self::new(low, high))
    }
}

/// Bounds beyond this are products that saturated or came close to it. `wrap` does not try to
/// follow them, so its arithmetic on periods cannot overflow.
const LIMIT: i128 = 1 << 100;

/// The overflow flags that an integer `add`, `sub` or `mul` may carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    /// No signed overflow: the operands read as signed numbers.
    pub nsw: bool,
    /// No unsigned overflow: the operands read as unsigned numbers.
    pub nuw: bool,
}

impl Flags {
    pub(crate) const UNSIGNED: Self = Self {
        nsw: false,
        nuw: true,
    };

    /// The words that go before the type, in the order of LLVM's printer: "", "nsw ", "nuw ", "nuw nsw ".
    pub(crate) fn text(self) -> &'static str {
        match (self.nuw, self.nsw) {
            (false, false) => "",
            (false, true) => "nsw ",
            (true, false) => "nuw ",
            (true, true) => "nuw nsw ",
        }
    }
}

/// The integer operations whose overflow flags are decided here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Add,
    Subtract,
    Multiply,
}

/// What an operation is proved to do to its operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub flags: Flags,
    /// The values the operation can produce, read in the type of the operands.
    pub result: Interval,
}

/// The values of `ty` in `interval`, as the numbers that their bit patterns are when read as
/// `signed` numbers of the same width. An interval that crosses the point where the reading changes
/// (zero for a signed type, half the range for an unsigned one) maps to two pieces; the whole range
/// is their hull, so the answer still contains every value.
fn reading(ty: Int, interval: Interval, signed: bool) -> Interval {
    if ty.signed == signed {
        return interval;
    }
    let modulus = ty.modulus();
    if signed {
        // An unsigned pattern from half the range up is a negative number.
        if interval.high < modulus >> 1 {
            interval
        } else if interval.low >= modulus >> 1 {
            Interval::new(interval.low - modulus, interval.high - modulus)
        } else {
            ty.with_sign(true).full()
        }
    } else if interval.low >= 0 {
        interval
    } else if interval.high < 0 {
        Interval::new(interval.low + modulus, interval.high + modulus)
    } else {
        ty.with_sign(false).full()
    }
}

/// The result of `operation` over the integers, without any wrapping.
fn exact(operation: Operation, left: Interval, right: Interval) -> Interval {
    match operation {
        Operation::Add => Interval::new(
            left.low.saturating_add(right.low),
            left.high.saturating_add(right.high),
        ),
        Operation::Subtract => Interval::new(
            left.low.saturating_sub(right.high),
            left.high.saturating_sub(right.low),
        ),
        Operation::Multiply => {
            let products = [
                left.low.saturating_mul(right.low),
                left.low.saturating_mul(right.high),
                left.high.saturating_mul(right.low),
                left.high.saturating_mul(right.high),
            ];
            Interval::new(
                products.into_iter().min().unwrap(),
                products.into_iter().max().unwrap(),
            )
        }
    }
}

/// The values of `ty` that the integers in `interval` become when they are cut to its width. A
/// piece that stays inside one period of the width moves as a whole; one that crosses a period
/// boundary can become any value of the type.
fn wrap(ty: Int, interval: Interval) -> Interval {
    if interval.within(ty.full()) {
        return interval;
    }
    if interval.low < -LIMIT || interval.high > LIMIT {
        return ty.full();
    }
    let modulus = ty.modulus();
    let period = |value: i128| (value - ty.min()).div_euclid(modulus);
    if period(interval.low) != period(interval.high) {
        return ty.full();
    }
    let shift = period(interval.low) * modulus;
    Interval::new(interval.low - shift, interval.high - shift)
}

/// What `left op right` does on `ty` when the operands lie in the given intervals. `nsw` holds when
/// no pair of values overflows as signed numbers, and `nuw` when none overflows as unsigned numbers;
/// each reading of the operands is checked against the whole range of its sense.
pub(crate) fn arithmetic(
    operation: Operation,
    ty: Int,
    left: Interval,
    right: Interval,
) -> Outcome {
    let signed = ty.with_sign(true);
    let unsigned = ty.with_sign(false);
    let as_signed = exact(operation, reading(ty, left, true), reading(ty, right, true));
    let as_unsigned = exact(
        operation,
        reading(ty, left, false),
        reading(ty, right, false),
    );
    // Both readings are congruent to the true result modulo the width, so the result lies in the
    // wrapped image of each, and in their common part.
    let result = wrap(ty, as_signed)
        .meet(wrap(ty, as_unsigned))
        .unwrap_or_else(|| ty.full());
    Outcome {
        flags: Flags {
            nsw: as_signed.within(signed.full()),
            nuw: as_unsigned.within(unsigned.full()),
        },
        result,
    }
}

/// The values of `to` that the integers in `interval` become: sign or zero extension keeps the
/// value, and truncation or a change of sign cuts it to the width.
pub(crate) fn cast(to: Int, interval: Interval) -> Interval {
    wrap(to, interval)
}

/// `left & right`: as bit patterns the result is at most each operand, so it is at most the smaller
/// bound. The patterns read as unsigned numbers; a signed type keeps the bound only while it stays
/// below the sign bit.
pub(crate) fn bit_and(ty: Int, left: Interval, right: Interval) -> Interval {
    let bound = min(
        reading(ty, left, false).high,
        reading(ty, right, false).high,
    );
    wrap(ty, Interval::new(0, bound))
}

/// `left % right`, which has the sign of the dividend and is smaller than the divisor. A zero divisor
/// traps, so only the other values of `right` matter.
pub(crate) fn remainder(ty: Int, left: Interval, right: Interval) -> Interval {
    let largest = max(right.low.unsigned_abs(), right.high.unsigned_abs());
    if largest == 0 {
        return ty.full();
    }
    let most = i128::try_from(largest - 1).unwrap_or(i128::MAX);
    Interval::new(
        if left.low >= 0 {
            0
        } else {
            max(left.low, -most)
        },
        if left.high <= 0 {
            0
        } else {
            min(left.high, most)
        },
    )
}

/// `left / right`, rounded toward zero. The quotient is monotonic in each operand on each side of
/// zero, so the extremes are at the corners of the two pieces of the divisor; a zero divisor traps
/// and `MIN / -1`, which traps too, is the only value outside the type.
pub(crate) fn divide(ty: Int, left: Interval, right: Interval) -> Interval {
    let pieces = [
        right.meet(Interval::new(i128::MIN, -1)),
        right.meet(Interval::new(1, i128::MAX)),
    ];
    let mut quotient: Option<Interval> = None;
    for divisor in pieces.into_iter().flatten() {
        let corners = [
            left.low / divisor.low,
            left.low / divisor.high,
            left.high / divisor.low,
            left.high / divisor.high,
        ];
        let piece = Interval::new(
            corners.into_iter().min().unwrap(),
            corners.into_iter().max().unwrap(),
        );
        quotient = Some(quotient.map_or(piece, |known| known.hull(piece)));
    }
    quotient
        .and_then(|quotient| quotient.meet(ty.full()))
        .unwrap_or_else(|| ty.full())
}

/// `left >>> count` for a count below the width: an arithmetic shift of a signed value or a logical
/// shift of an unsigned one, which both are a division by a power of two that rounds down.
pub(crate) fn shift_right(left: Interval, count: u32) -> Interval {
    Interval::new(left.low >> count, left.high >> count)
}

/// `left` shifted right by `count`, below the width, with zeros coming in (`Bits.ushr`): the bit
/// patterns, read as unsigned numbers, shifted, and read in the type again.
pub(crate) fn shift_right_logical(ty: Int, left: Interval, count: u32) -> Interval {
    wrap(ty, shift_right(reading(ty, left, false), count))
}

/// `~value`, which is `-value - 1` for a signed type and `max - value` for an unsigned one: every
/// value maps to a value of the type, in the opposite order.
pub(crate) fn complement(ty: Int, value: Interval) -> Interval {
    let (low, high) = if ty.signed {
        (-value.high - 1, -value.low - 1)
    } else {
        (ty.max() - value.high, ty.max() - value.low)
    };
    Interval::new(low, high)
}

/// `left <<< count` for a count below the width: the product with a power of two, cut to the width.
pub(crate) fn shift_left(ty: Int, left: Interval, count: u32) -> Interval {
    wrap(
        ty,
        exact(Operation::Multiply, left, Interval::point(1_i128 << count)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPERATIONS: [Operation; 3] = [Operation::Add, Operation::Subtract, Operation::Multiply];

    impl Interval {
        fn contains(self, value: i128) -> bool {
            self.low <= value && value <= self.high
        }
    }

    fn int(bits: u16, signed: bool) -> Int {
        Int::new(bits, signed).unwrap()
    }

    #[test]
    fn flags_print_in_the_order_of_the_llvm_printer() {
        let flags = |nsw, nuw| Flags { nsw, nuw }.text();
        assert_eq!(flags(false, false), "");
        assert_eq!(flags(true, false), "nsw ");
        assert_eq!(flags(false, true), "nuw ");
        assert_eq!(flags(true, true), "nuw nsw ");
        assert_eq!(Flags::UNSIGNED.text(), "nuw ");
    }

    #[test]
    fn integer_types_cover_up_to_64_bits() {
        assert_eq!(Int::new(0, true), None);
        assert_eq!(Int::new(65, false), None);
        assert_eq!(Int::new(128, true), None);
        assert_eq!(int(8, true).full(), Interval::new(-128, 127));
        assert_eq!(int(8, false).full(), Interval::new(0, 255));
        assert_eq!(
            int(64, true).full(),
            Interval::new(-(1 << 63), (1 << 63) - 1)
        );
        assert_eq!(int(64, false).full(), Interval::new(0, (1 << 64) - 1));
    }

    // The reference uses the definitions only: bit patterns, and arithmetic on the numbers that
    // they are when read as signed or as unsigned.

    /// The numbers that the bit pattern of a value of `ty` is, read as signed and as unsigned.
    fn readings(ty: Int, value: i128) -> (i128, u128) {
        let modulus = ty.modulus();
        let pattern = value.rem_euclid(modulus);
        let signed = if pattern >= modulus >> 1 {
            pattern - modulus
        } else {
            pattern
        };
        (signed, pattern as u128)
    }

    /// The value of `ty` whose bit pattern is `pattern`.
    fn value_of(ty: Int, pattern: u128) -> i128 {
        let pattern = pattern as i128;
        if ty.signed && pattern >= ty.modulus() >> 1 {
            pattern - ty.modulus()
        } else {
            pattern
        }
    }

    /// Whether `left op right` overflows as signed numbers and as unsigned numbers, and the value
    /// that it produces.
    fn reference(operation: Operation, ty: Int, left: i128, right: i128) -> (bool, bool, i128) {
        let modulus = ty.modulus() as u128;
        let ((ls, lu), (rs, ru)) = (readings(ty, left), readings(ty, right));
        let wide = match operation {
            Operation::Add => ls + rs,
            Operation::Subtract => ls - rs,
            Operation::Multiply => ls * rs,
        };
        let signed_overflow = !ty.with_sign(true).full().contains(wide);
        let (unsigned_overflow, pattern) = match operation {
            Operation::Add => (lu + ru >= modulus, (lu + ru) % modulus),
            Operation::Subtract => (lu < ru, (lu + modulus - ru) % modulus),
            Operation::Multiply => (lu * ru >= modulus, (lu * ru) % modulus),
        };
        (signed_overflow, unsigned_overflow, value_of(ty, pattern))
    }

    macro_rules! native_agrees {
        ($ty:expr, $signed:ty, $unsigned:ty, $grid:expr) => {
            for &l in $grid {
                for &r in $grid {
                    let ty = $ty;
                    let (ls, lu) = ((l as $unsigned) as $signed, l as $unsigned);
                    let (rs, ru) = ((r as $unsigned) as $signed, r as $unsigned);
                    let natives = [
                        (
                            ls.overflowing_add(rs).1,
                            lu.overflowing_add(ru).1,
                            lu.wrapping_add(ru),
                        ),
                        (
                            ls.overflowing_sub(rs).1,
                            lu.overflowing_sub(ru).1,
                            lu.wrapping_sub(ru),
                        ),
                        (
                            ls.overflowing_mul(rs).1,
                            lu.overflowing_mul(ru).1,
                            lu.wrapping_mul(ru),
                        ),
                    ];
                    for (operation, (signed, unsigned, pattern)) in
                        OPERATIONS.into_iter().zip(natives)
                    {
                        let expected = (signed, unsigned, value_of(ty, pattern as u128));
                        assert_eq!(
                            reference(operation, ty, l, r),
                            expected,
                            "{operation:?} {ty:?} {l} {r}"
                        );
                    }
                }
            }
        };
    }

    #[test]
    fn the_reference_agrees_with_the_native_integer_types() {
        macro_rules! native {
            ($signed:ty, $unsigned:ty, $bits:expr) => {
                for signed in [false, true] {
                    let ty = int($bits, signed);
                    let grid: Vec<i128> = [
                        ty.min(),
                        ty.min() + 1,
                        ty.min() / 2,
                        -3,
                        -1,
                        0,
                        1,
                        2,
                        3,
                        ty.max() / 2,
                        ty.max() - 1,
                        ty.max(),
                    ]
                    .into_iter()
                    .filter(|value| ty.full().contains(*value))
                    .collect();
                    native_agrees!(ty, $signed, $unsigned, &grid);
                }
            };
        }
        native!(i8, u8, 8);
        native!(i16, u16, 16);
        native!(i32, u32, 32);
        native!(i64, u64, 64);
    }

    fn all_intervals(ty: Int) -> Vec<Interval> {
        (ty.min()..=ty.max())
            .flat_map(|low| (low..=ty.max()).map(move |high| Interval::new(low, high)))
            .collect()
    }

    /// Whether a value of the interval can read on either side of the point where `reading` changes
    /// the reading of the patterns, so that its answer is a hull.
    fn crosses(ty: Int, interval: Interval, signed: bool) -> bool {
        if ty.signed == signed {
            return false;
        }
        let edge = if signed { ty.modulus() >> 1 } else { 0 };
        interval.low < edge && interval.high >= edge
    }

    fn check_arithmetic(ty: Int, left: Interval, right: Interval, exhaustive: bool) {
        let samples = |interval: Interval| -> Vec<i128> {
            if exhaustive {
                return (interval.low..=interval.high).collect();
            }
            let middle = interval.low + (interval.high - interval.low) / 2;
            let mut samples = vec![interval.low, middle, interval.high];
            if interval.high > interval.low {
                samples.extend([interval.low + 1, interval.high - 1]);
            }
            samples.sort_unstable();
            samples.dedup();
            samples
        };
        let (lefts, rights) = (samples(left), samples(right));
        for operation in OPERATIONS {
            let outcome = arithmetic(operation, ty, left, right);
            let (mut signed_overflow, mut unsigned_overflow) = (false, false);
            for &l in &lefts {
                for &r in &rights {
                    let (signed, unsigned, value) = reference(operation, ty, l, r);
                    signed_overflow |= signed;
                    unsigned_overflow |= unsigned;
                    assert!(
                        outcome.result.contains(value),
                        "{operation:?} {ty:?} {left:?} {right:?}: {value} is not in {:?}",
                        outcome.result
                    );
                }
            }
            let context = format!("{operation:?} {ty:?} {left:?} {right:?}");
            // A flag is claimed only when no pair of values overflows ...
            assert!(!outcome.flags.nsw || !signed_overflow, "nsw {context}");
            assert!(!outcome.flags.nuw || !unsigned_overflow, "nuw {context}");
            // ... and whenever intervals can show it: the extremes of an operation are at the corners.
            if !crosses(ty, left, true) && !crosses(ty, right, true) {
                assert_eq!(outcome.flags.nsw, !signed_overflow, "nsw {context}");
            }
            if !crosses(ty, left, false) && !crosses(ty, right, false) {
                assert_eq!(outcome.flags.nuw, !unsigned_overflow, "nuw {context}");
            }
        }
    }

    #[test]
    fn arithmetic_agrees_with_every_pair_of_values_up_to_4_bits() {
        for bits in 1..=4 {
            for signed in [false, true] {
                let ty = int(bits, signed);
                let intervals = all_intervals(ty);
                for &left in &intervals {
                    for &right in &intervals {
                        check_arithmetic(ty, left, right, true);
                    }
                }
            }
        }
    }

    #[test]
    fn arithmetic_agrees_with_the_reference_at_the_edges_of_8_to_64_bit_types() {
        for bits in [8, 16, 32, 64] {
            for signed in [false, true] {
                let ty = int(bits, signed);
                let mut edges: Vec<i128> = [
                    ty.min(),
                    ty.min() + 1,
                    ty.min() / 2,
                    -2,
                    -1,
                    0,
                    1,
                    2,
                    ty.max() / 2,
                    ty.max() / 2 + 1,
                    ty.max() - 1,
                    ty.max(),
                ]
                .into_iter()
                .filter(|edge| ty.full().contains(*edge))
                .collect();
                edges.sort_unstable();
                edges.dedup();
                let intervals: Vec<Interval> = edges
                    .iter()
                    .flat_map(|&low| {
                        edges
                            .iter()
                            .filter(move |&&high| low <= high)
                            .map(move |&high| Interval::new(low, high))
                    })
                    .collect();
                for &left in &intervals {
                    for &right in &intervals {
                        check_arithmetic(ty, left, right, false);
                    }
                }
            }
        }
    }

    #[test]
    fn products_that_leave_i128_claim_nothing() {
        for signed in [false, true] {
            let ty = int(64, signed);
            let outcome = arithmetic(Operation::Multiply, ty, ty.full(), ty.full());
            assert_eq!(outcome.flags, Flags::default());
            assert_eq!(outcome.result, ty.full());
        }
    }

    #[test]
    fn the_cases_of_the_ticket_get_exactly_these_flags() {
        let flags = |nsw, nuw| Flags { nsw, nuw };
        let run = |operation, ty: Int, left: (i128, i128), right: (i128, i128)| {
            arithmetic(
                operation,
                ty,
                Interval::new(left.0, left.1),
                Interval::new(right.0, right.1),
            )
            .flags
        };
        let (i8, u8, i64) = (int(8, true), int(8, false), int(64, true));
        let one = (1, 1);
        let max = (1_i128 << 63) - 1;
        // `i + 1` for an `i` below the maximum has both flags; at the maximum only the unsigned one
        // (the sum 2^63 still fits 64 bits); a negative `i` wraps as unsigned.
        assert_eq!(
            run(Operation::Add, i64, (0, max - 1), one),
            flags(true, true)
        );
        assert_eq!(run(Operation::Add, i64, (0, max), one), flags(false, true));
        assert_eq!(run(Operation::Add, i64, (-5, 5), one), flags(true, false));
        assert_eq!(run(Operation::Add, i64, (-5, max), one), Flags::default());
        // `i * 2` for `0i8 .. 127i8` fits 8 bits as unsigned (254) but not as signed.
        assert_eq!(
            run(Operation::Multiply, i8, (0, 127), (2, 2)),
            flags(false, true)
        );
        assert_eq!(
            run(Operation::Multiply, i8, (0, 63), (2, 2)),
            flags(true, true)
        );
        assert_eq!(
            run(Operation::Multiply, i8, (-64, 63), (2, 2)),
            flags(true, false)
        );
        // Unsigned `x - 1` needs `x >= 1`; read as a signed number, the patterns from 128 up are negative.
        assert_eq!(
            run(Operation::Subtract, u8, (1, 127), one),
            flags(true, true)
        );
        assert_eq!(
            run(Operation::Subtract, u8, (1, 255), one),
            flags(false, true)
        );
        assert_eq!(
            run(Operation::Subtract, u8, (0, 127), one),
            flags(true, false)
        );
        assert_eq!(
            run(Operation::Subtract, u8, (0, 255), one),
            Flags::default()
        );
        assert_eq!(
            run(Operation::Subtract, i64, (-max, 5), one),
            flags(true, false)
        );
        assert_eq!(
            run(Operation::Subtract, i64, (-max - 1, 5), one),
            Flags::default()
        );
        // Two unknown numbers prove nothing.
        let any = (-max - 1, max);
        for operation in OPERATIONS {
            assert_eq!(run(operation, i64, any, any), Flags::default());
        }
    }

    #[test]
    fn a_result_that_wraps_is_known_only_inside_one_period() {
        let u8 = int(8, false);
        let result = |left: (i128, i128), right: (i128, i128)| {
            arithmetic(
                Operation::Add,
                u8,
                Interval::new(left.0, left.1),
                Interval::new(right.0, right.1),
            )
            .result
        };
        // 250 + 10 wraps to 4, and the whole interval wraps together.
        assert_eq!(result((250, 255), (10, 10)), Interval::new(4, 9));
        // Some sums wrap and some do not.
        assert_eq!(result((250, 255), (0, 10)), u8.full());
    }

    #[test]
    fn casts_agree_with_every_value_up_to_4_bits() {
        for from_bits in 1..=4 {
            for from_signed in [false, true] {
                let from = int(from_bits, from_signed);
                for to_bits in 1..=4 {
                    for to_signed in [false, true] {
                        let to = int(to_bits, to_signed);
                        for interval in all_intervals(from) {
                            let result = cast(to, interval);
                            for value in interval.low..=interval.high {
                                let (_, pattern) = readings(to, value);
                                assert!(
                                    result.contains(value_of(to, pattern)),
                                    "{from:?} -> {to:?} {interval:?}: {result:?}"
                                );
                            }
                            if interval.within(to.full()) {
                                assert_eq!(result, interval, "{from:?} -> {to:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bit_operations_and_division_agree_with_every_pair_of_values_up_to_4_bits() {
        for bits in 1..=4 {
            for signed in [false, true] {
                let ty = int(bits, signed);
                let intervals = all_intervals(ty);
                for &left in &intervals {
                    for &right in &intervals {
                        let masked = bit_and(ty, left, right);
                        let remainders = remainder(ty, left, right);
                        let quotients = divide(ty, left, right);
                        for l in left.low..=left.high {
                            for r in right.low..=right.high {
                                let context = format!("{ty:?} {l} {r} in {left:?} {right:?}");
                                let (lp, rp) = (readings(ty, l).1, readings(ty, r).1);
                                assert!(masked.contains(value_of(ty, lp & rp)), "& {context}");
                                let trap = r == 0 || (ty.signed && l == ty.min() && r == -1);
                                if !trap {
                                    assert!(remainders.contains(l % r), "% {context}");
                                    assert!(quotients.contains(l / r), "/ {context}");
                                }
                            }
                        }
                    }
                }
                for interval in intervals {
                    for count in 0..u32::from(bits) {
                        let shifted = shift_right(interval, count);
                        let widened = shift_left(ty, interval, count);
                        for value in interval.low..=interval.high {
                            assert!(
                                shifted.contains(value >> count),
                                ">>> {ty:?} {value} {count}"
                            );
                            let (_, pattern) = readings(ty, value << count);
                            assert!(
                                widened.contains(value_of(ty, pattern)),
                                "<<< {ty:?} {value} {count}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn logical_shifts_and_complements_agree_with_every_value_up_to_4_bits() {
        for bits in 1..=4 {
            for signed in [false, true] {
                let ty = int(bits, signed);
                for interval in all_intervals(ty) {
                    let flipped = complement(ty, interval);
                    assert!(flipped.within(ty.full()), "{ty:?} {interval:?}");
                    for value in interval.low..=interval.high {
                        let (_, pattern) = readings(ty, value);
                        let expected = value_of(ty, !pattern & (ty.modulus() as u128 - 1));
                        assert!(flipped.contains(expected), "~ {ty:?} {value}");
                        for count in 0..u32::from(bits) {
                            let shifted = shift_right_logical(ty, interval, count);
                            assert!(
                                shifted.contains(value_of(ty, pattern >> count)),
                                "ushr {ty:?} {value} {count} in {interval:?}: {shifted:?}"
                            );
                        }
                    }
                    // A complement reverses the order, so it is as tight as the interval.
                    assert_eq!(
                        flipped.high - flipped.low,
                        interval.high - interval.low,
                        "{ty:?} {interval:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn masks_and_remainders_of_constants_are_tight() {
        let i64 = int(64, true);
        let any = i64.full();
        assert_eq!(
            bit_and(i64, any, Interval::point(255)),
            Interval::new(0, 255)
        );
        assert_eq!(
            bit_and(i64, Interval::point(255), any),
            Interval::new(0, 255)
        );
        // A mask with the sign bit set says nothing about the sign.
        assert_eq!(bit_and(i64, any, Interval::point(-2)), any);
        assert_eq!(
            remainder(i64, any, Interval::point(10)),
            Interval::new(-9, 9)
        );
        assert_eq!(
            remainder(i64, Interval::new(0, 1000), Interval::point(10)),
            Interval::new(0, 9)
        );
        assert_eq!(
            remainder(i64, Interval::new(-1000, 0), Interval::point(10)),
            Interval::new(-9, 0)
        );
        assert_eq!(
            divide(i64, Interval::new(-7, 7), Interval::point(2)),
            Interval::new(-3, 3)
        );
        // A zero divisor traps, and nothing follows it.
        assert_eq!(remainder(i64, any, Interval::point(0)), any);
        assert_eq!(divide(i64, any, Interval::point(0)), any);
    }
}
