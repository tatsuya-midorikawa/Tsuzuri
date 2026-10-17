//! Compact strings (PM03): the form of the code units of a `string`, and how the compiler chooses it.
//!
//! A `string` is `{ ptr, i64 }` in both forms. In the UTF-16 form, the second field, the length
//! word, is the number of 16-bit code units, as it has always been. In the Latin-1 form, bit 62
//! of the word is set, the low 62 bits are the number of units, and each unit is one byte (a code
//! unit of at most 0xFF). Bit 63 is 0 in every string: PM01's niche makes a negative length mean
//! the absent case of `Maybe<string>`.
//!
//! `TSUZURI_STRINGS=utf16` (the default) never makes a Latin-1 string and writes the same IR as
//! before PM03. `compact` makes them wherever the form is known without a scan (literals, number
//! text, concatenations of Latin-1 strings, ...) and reads both forms everywhere.

use super::*;

/// The environment variable that chooses the form of the compiler's commands and test suites.
pub const ENVIRONMENT_VARIABLE: &str = "TSUZURI_STRINGS";

/// Which strings the emitter makes, and so which forms its code and runtime must read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StringForm {
    /// Every string has 16-bit units. The IR is the one of the compiler before PM03.
    #[default]
    Utf16,
    /// A string whose units are all at most 0xFF may have one byte per unit.
    Compact,
}

impl StringForm {
    /// The form when nothing chooses one.
    pub const DEFAULT: Self = Self::Utf16;

    pub fn name(self) -> &'static str {
        match self {
            Self::Utf16 => "utf16",
            Self::Compact => "compact",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [Self::Utf16, Self::Compact]
            .into_iter()
            .find(|form| form.name() == text)
    }

    /// The form that `TSUZURI_STRINGS` names; the default when it is unset or empty. Any other
    /// value is an error: a form that was asked for and does not exist must not fall back silently.
    pub fn from_environment() -> Result<Self, Diagnostic> {
        let invalid = |value: &str| {
            Diagnostic::new(
                "E2000",
                format!("{ENVIRONMENT_VARIABLE} must be utf16 or compact, not '{value}'"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_have_stable_names() {
        assert_eq!(StringForm::DEFAULT, StringForm::Utf16);
        for form in [StringForm::Utf16, StringForm::Compact] {
            assert_eq!(StringForm::parse(form.name()), Some(form));
        }
        for text in ["", "UTF16", "latin1", "compact ", "off"] {
            assert_eq!(StringForm::parse(text), None, "{text:?}");
        }
    }
}
