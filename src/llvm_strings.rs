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
//!
//! The runtime of the compact form is an additive layer. The runtime files of the UTF-16 form are
//! included as they are, except that each definition that a function of this layer takes over is
//! renamed `<name>.utf16`; the layer defines the name again, looks at the length word, and calls
//! the original for a UTF-16 string. The renames only touch `define` lines.

use super::*;
use std::borrow::Cow;

/// Bit 62 of the length word: the string is Latin-1.
#[allow(dead_code)] // the emitter reads it from WP4 on
pub const LATIN1: u64 = 1 << 62;
/// The bits of the length word that count the units.
#[allow(dead_code)]
pub const MASK: u64 = LATIN1 - 1;

const KERNELS: &str = include_str!("runtime/string_latin1.ll");
const STRING_LAYER: &str = include_str!("runtime/string_compact.ll");
const DISPLAY_LAYER: &str = include_str!("runtime/display_compact.ll");
const CHARACTER_LAYER: &str = include_str!("runtime/character_compact.ll");
const INTEGER_LAYER: &str = include_str!("runtime/integer_compact.ll");

/// The definitions of the string runtime that `string_compact.ll` defines again. The original of
/// each is kept as `<name>.utf16`.
const STRING_TAKEN: &[&str] = &[
    "tz.string.new",
    "tz.string.concat",
    "tz.string.append",
    "tz.string.concat_into",
    "tz.string.to_ascii",
    "tz.string.equal",
    "tz.string.compare",
    "tz.string.from_utf8",
    "tz.utf8string.from_string",
    "tz.string.is_well_formed",
    "tz.string.to_well_formed",
];
/// The definitions that the layer replaces and nothing calls any more: ASCII text is Latin-1.
const STRING_DROPPED: &[&str] = &["tz.string.from_ascii"];
const DISPLAY_TAKEN: &[&str] = &["tz.display.join", "tz.display.quote"];
const CHARACTER_TAKEN: &[&str] = &["tz.character.display", "tz.character.parse"];
const INTEGER_TAKEN: &[&str] = &["tz.integer.parse", "tz.integer.parse128"];
/// The integer parsers that get a copy that reads Latin-1 bytes, in the order that the copies are written.
const INTEGER_COPIED: &[&str] = &["tz.integer.scan", "tz.integer.parse", "tz.integer.parse128"];

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

    /// The string runtime for this form: `string.ll`, `utf8string.ll`, and the UTF-16 kernels that
    /// follow them, up to the heap. `utf16` is that text for the UTF-16 form.
    pub(crate) fn string_runtime<'a>(self, utf16: &'a str) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => layered(
                utf16,
                STRING_TAKEN,
                STRING_DROPPED,
                &[KERNELS, STRING_LAYER],
            )
            .map(Cow::Owned),
        }
    }

    /// `display.ll` for this form.
    pub(crate) fn display_runtime<'a>(self, utf16: &'a str) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => layered(utf16, DISPLAY_TAKEN, &[], &[DISPLAY_LAYER]).map(Cow::Owned),
        }
    }

    /// `character.ll` for this form.
    pub(crate) fn character_runtime<'a>(self, utf16: &'a str) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => {
                layered(utf16, CHARACTER_TAKEN, &[], &[CHARACTER_LAYER]).map(Cow::Owned)
            }
        }
    }

    /// `integer.ll` for this form: the parsers also exist as copies that read Latin-1 bytes.
    pub(crate) fn integer_runtime<'a>(self, utf16: &'a str) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => {
                let copies = latin1_integer_parsers(utf16)?;
                layered(utf16, INTEGER_TAKEN, &[], &[&copies, INTEGER_LAYER]).map(Cow::Owned)
            }
        }
    }
}

fn mismatch(name: &str, found: usize) -> Diagnostic {
    Diagnostic::new(
        "E1015",
        format!(
            "internal compiler error: the compact string runtime found {found} definitions of '{name}' to take over, not 1"
        ),
        Span::default(),
    )
}

/// The symbol that a `define` line defines, from the first `@` to the next `(`.
fn defined_symbol(line: &str) -> Option<&str> {
    let name = line.strip_prefix("define ")?.split_once('@')?.1;
    Some(&name[..name.find('(')?])
}

/// `ir` with the definition of each of `taken` renamed `<name>.utf16` and each of `dropped` removed,
/// and then `layers`, which define the names again. Each name has to be defined exactly once.
fn layered(
    ir: &str,
    taken: &[&str],
    dropped: &[&str],
    layers: &[&str],
) -> Result<String, Diagnostic> {
    let mut text =
        String::with_capacity(ir.len() + layers.iter().map(|layer| layer.len()).sum::<usize>());
    let mut counts = vec![0usize; taken.len() + dropped.len()];
    let mut removing = false;
    for line in ir.split_inclusive('\n') {
        if removing {
            removing = !line.starts_with('}');
            continue;
        }
        let symbol = defined_symbol(line);
        if let Some(index) = symbol.and_then(|symbol| taken.iter().position(|name| *name == symbol))
        {
            counts[index] += 1;
            let name = taken[index];
            text.push_str(&line.replacen(&format!("@{name}("), &format!("@{name}.utf16("), 1));
        } else if let Some(index) =
            symbol.and_then(|symbol| dropped.iter().position(|name| *name == symbol))
        {
            counts[taken.len() + index] += 1;
            removing = true;
        } else {
            text.push_str(line);
        }
    }
    for (name, found) in taken.iter().chain(dropped).zip(counts) {
        if found != 1 {
            return Err(mismatch(name, found));
        }
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for layer in layers {
        text.push_str(layer);
    }
    Ok(text)
}

/// The text of the definition of `name` in `ir`: from its `define` line to the `}` that closes it.
fn definition<'a>(ir: &'a str, name: &str) -> Option<&'a str> {
    let mut start = None;
    let mut offset = 0;
    for line in ir.split_inclusive('\n') {
        match start {
            None if defined_symbol(line) == Some(name) => start = Some(offset),
            Some(begin) if line.starts_with('}') => return Some(&ir[begin..offset + line.len()]),
            _ => {}
        }
        offset += line.len();
    }
    None
}

/// Copies of the integer parsers that read their input as bytes: `<name>.latin1` for each of
/// `INTEGER_COPIED`. A load of a unit becomes a load of a byte and a `zext` that gives the value the
/// name that the load had, and `scan` is the copy that the copies call.
fn latin1_integer_parsers(integer: &str) -> Result<String, Diagnostic> {
    let mut copies = String::new();
    for name in INTEGER_COPIED {
        let original = definition(integer, name).ok_or_else(|| mismatch(name, 0))?;
        for line in original.split_inclusive('\n') {
            let line = line.replacen(&format!("@{name}("), &format!("@{name}.latin1("), 1);
            let line = line.replacen("@tz.integer.scan(", "@tz.integer.scan.latin1(", 1);
            let line = line.replacen(
                "getelementptr inbounds i16, ptr %input,",
                "getelementptr inbounds i8, ptr %input,",
                1,
            );
            match line.trim_start().split_once(" = load i16, ptr ") {
                // A load that claims the alignment of a unit is not true of a byte.
                Some((_, pointer)) if pointer.contains("align") => {
                    return Err(mismatch("an aligned load in an integer parser", 1));
                }
                Some((value, pointer)) => {
                    let indent = &line[..line.len() - line.trim_start().len()];
                    copies.push_str(&format!(
                        "{indent}{value}.byte = load i8, ptr {pointer}{indent}{value} = zext i8 {value}.byte to i16\n"
                    ));
                }
                None => copies.push_str(&line),
            }
        }
        copies.push('\n');
    }
    if copies.contains("i16, ptr %input") {
        return Err(mismatch("an integer parser's load of a unit", 1));
    }
    Ok(copies)
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

    fn defined(ir: &str) -> Vec<&str> {
        ir.lines().filter_map(defined_symbol).collect()
    }

    fn assert_layer(utf16: &str, compact: &str, taken: &[&str], dropped: &[&str], layer: &str) {
        let before = defined(utf16);
        let after = defined(compact);
        let mut unique = after.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), after.len(), "a definition is repeated");
        for name in taken {
            assert_eq!(
                before.iter().filter(|symbol| *symbol == name).count(),
                1,
                "{name}"
            );
            let original = format!("{name}.utf16");
            assert_eq!(
                after.iter().filter(|symbol| **symbol == original).count(),
                1,
                "{original}"
            );
            assert_eq!(
                after.iter().filter(|symbol| *symbol == name).count(),
                1,
                "{name}"
            );
        }
        for name in dropped {
            assert_eq!(
                before.iter().filter(|symbol| *symbol == name).count(),
                1,
                "{name}"
            );
            assert_eq!(
                after.iter().filter(|symbol| *symbol == name).count(),
                1,
                "{name}"
            );
            assert!(!after.contains(&format!("{name}.utf16").as_str()), "{name}");
        }
        assert!(compact.contains(layer));
    }

    /// The string runtime in each of the shapes that `emit_program_in` assembles.
    fn string_runtimes() -> Vec<(&'static str, String)> {
        let head = [
            include_str!("runtime/string.ll"),
            include_str!("runtime/utf8string.ll"),
        ]
        .concat();
        let portable = |text: &str, from_string: bool| {
            let mut text = text
                .replace(
                    "@tz.string.validate_utf8(ptr",
                    "@tz.string.validate_utf8.portable(ptr",
                )
                .replace(
                    "@tz.string.from_utf8(ptr",
                    "@tz.string.from_utf8.portable(ptr",
                );
            if from_string {
                text = text.replace(
                    "@tz.utf8string.from_string(ptr",
                    "@tz.utf8string.from_string.portable(ptr",
                );
            }
            text
        };
        let v128 = include_str!("runtime/string_v128.ll");
        let cpu = include_str!("runtime/string_cpu.ll");
        let cpu_utf16 = include_str!("runtime/string_cpu_utf16.ll");
        vec![
            (
                "scalar",
                format!("{head}{}", include_str!("runtime/string_scalar.ll")),
            ),
            ("v128", format!("{head}{v128}")),
            ("cpu", format!("{head}{}{cpu}", portable(v128, false))),
            (
                "cpu on x86",
                format!("{head}{}{cpu}{cpu_utf16}", portable(v128, true)),
            ),
        ]
    }

    #[test]
    fn the_string_layer_takes_over_exactly_one_definition_of_each_name() {
        for (shape, utf16) in string_runtimes() {
            let compact = StringForm::Compact.string_runtime(&utf16).unwrap();
            assert_layer(&utf16, &compact, STRING_TAKEN, STRING_DROPPED, STRING_LAYER);
            assert!(compact.contains(KERNELS), "{shape}");
            let borrowed = StringForm::Utf16.string_runtime(&utf16).unwrap();
            assert!(
                matches!(borrowed, Cow::Borrowed(text) if text == utf16),
                "{shape}"
            );
        }
    }

    #[test]
    fn the_other_layers_take_over_exactly_one_definition_of_each_name() {
        let display = include_str!("runtime/display.ll");
        let compact = StringForm::Compact.display_runtime(display).unwrap();
        assert_layer(display, &compact, DISPLAY_TAKEN, &[], DISPLAY_LAYER);
        let character = include_str!("runtime/character.ll");
        let compact = StringForm::Compact.character_runtime(character).unwrap();
        assert_layer(character, &compact, CHARACTER_TAKEN, &[], CHARACTER_LAYER);
        let integer = include_str!("runtime/integer.ll");
        let compact = StringForm::Compact.integer_runtime(integer).unwrap();
        assert_layer(integer, &compact, INTEGER_TAKEN, &[], INTEGER_LAYER);
        for name in INTEGER_COPIED {
            assert!(
                defined(&compact).contains(&format!("{name}.latin1").as_str()),
                "{name}"
            );
        }
        assert!(matches!(
            StringForm::Utf16.display_runtime(display).unwrap(),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            StringForm::Utf16.character_runtime(character).unwrap(),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            StringForm::Utf16.integer_runtime(integer).unwrap(),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn the_latin1_integer_parsers_read_bytes_and_nothing_else_changes() {
        let integer = include_str!("runtime/integer.ll");
        let copies = latin1_integer_parsers(integer).unwrap();
        assert!(!copies.contains("load i16"), "{copies}");
        assert!(!copies.contains("i16, ptr %input"), "{copies}");
        assert!(!copies.contains("@tz.integer.scan("), "{copies}");
        assert_eq!(
            copies.matches("load i8, ptr").count(),
            integer_loads(integer)
        );
        for name in INTEGER_COPIED {
            let original = definition(integer, name).unwrap();
            let copy = definition(&copies, &format!("{name}.latin1")).unwrap();
            // Only the loads, the pointers they read, and the names of the functions differ.
            let rest = |text: &str| {
                text.lines()
                    .filter(|line| !line.contains(" load i") && !line.contains(" zext i8 "))
                    .filter(|line| !line.contains("getelementptr inbounds i"))
                    .filter(|line| !line.starts_with("define "))
                    .filter(|line| !line.contains("@tz.integer.scan"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            assert_eq!(rest(original), rest(copy), "{name}");
        }
    }

    fn integer_loads(integer: &str) -> usize {
        INTEGER_COPIED
            .iter()
            .map(|name| {
                definition(integer, name)
                    .unwrap()
                    .matches(" = load i16, ptr ")
                    .count()
            })
            .sum()
    }

    #[test]
    fn a_missing_or_repeated_definition_is_an_error() {
        let ir = "define internal i1 @f(ptr %p) {\nentry:\n  ret i1 true\n}\n";
        assert!(layered(ir, &["f"], &[], &[""]).is_ok());
        assert!(layered(ir, &["g"], &[], &[""]).is_err());
        assert!(layered(&format!("{ir}{ir}"), &["f"], &[], &[""]).is_err());
        assert!(layered(&format!("{ir}{ir}"), &[], &["f"], &[""]).is_err());
        let renamed = layered(ir, &["f"], &[], &["layer\n"]).unwrap();
        assert_eq!(
            renamed,
            "define internal i1 @f.utf16(ptr %p) {\nentry:\n  ret i1 true\n}\nlayer\n"
        );
        assert_eq!(layered(ir, &[], &["f"], &["layer\n"]).unwrap(), "layer\n");
    }
}
