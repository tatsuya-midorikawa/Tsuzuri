//! The one way the translator refuses a program: E2002, naming the construct and the function.

use std::hash::{BuildHasherDefault, Hasher};

use crate::diagnostic::{Diagnostic, Span};

/// A construct the translator does not lower. It becomes E2002; the translator never turns it into
/// code with another meaning.
#[derive(Debug, Clone)]
pub(crate) struct Failure {
    construct: String,
    function: Option<String>,
}

pub(crate) type Res<T> = Result<T, Failure>;

impl Failure {
    pub(crate) fn new(construct: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            function: None,
        }
    }

    /// Names the function being translated, unless an inner step already did.
    pub(crate) fn in_function(mut self, name: &str) -> Self {
        if self.function.is_none() {
            self.function = Some(name.to_owned());
        }
        self
    }

    pub(crate) fn into_diagnostic(self) -> Diagnostic {
        let place = self
            .function
            .map(|name| format!(" in @{name}"))
            .unwrap_or_default();
        Diagnostic::new(
            "E2002",
            format!(
                "the fast backend cannot translate {}{place}; build with --backend llvm and report this program",
                self.construct
            ),
            Span::default(),
        )
    }
}

macro_rules! bail {
    ($($argument:tt)*) => {
        return Err($crate::backend::error::Failure::new(format!($($argument)*)))
    };
}
pub(crate) use bail;

/// A multiplicative hash for the symbol and type tables. The tables are never iterated for output,
/// so the result does not depend on it; it only has to be fast on short names.
#[derive(Default, Clone, Copy)]
pub(crate) struct Fx(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl Fx {
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for Fx {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while rest.len() >= 8 {
            self.add(u64::from_le_bytes(rest[..8].try_into().expect("8 bytes")));
            rest = &rest[8..];
        }
        if rest.len() >= 4 {
            self.add(u64::from(u32::from_le_bytes(
                rest[..4].try_into().expect("4 bytes"),
            )));
            rest = &rest[4..];
        }
        for &byte in rest {
            self.add(u64::from(byte));
        }
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

pub(crate) type FxMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<Fx>>;
