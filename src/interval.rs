//! Integer intervals and the overflow flags that they prove (PR02).
//!
//! An integer `+ - *` wraps in Tsuzuri, so LLVM's `nsw` and `nuw` (an overflow makes the result
//! poison) belong only on an instruction that a proof shows cannot overflow in that sense. This
//! module holds the pure part of the proof: the flags, and the arithmetic on intervals.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_print_in_the_order_of_the_llvm_printer() {
        let flags = |nsw, nuw| Flags { nsw, nuw }.text();
        assert_eq!(flags(false, false), "");
        assert_eq!(flags(true, false), "nsw ");
        assert_eq!(flags(false, true), "nuw ");
        assert_eq!(flags(true, true), "nuw nsw ");
        assert_eq!(Flags::UNSIGNED.text(), "nuw ");
    }
}
