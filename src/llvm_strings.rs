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
pub const LATIN1: u64 = 1 << 62;
/// The bits of the length word that count the units.
pub const MASK: u64 = LATIN1 - 1;

const KERNELS: &str = include_str!("runtime/string_latin1.ll");
/// The same kernels with a 128-bit vector form where they have one (generate_utf.py writes it).
const KERNELS_V128: &str = include_str!("runtime/string_latin1_v128.ll");
const STRING_LAYER: &str = include_str!("runtime/string_compact.ll");
const DISPLAY_LAYER: &str = include_str!("runtime/display_compact.ll");
const CHARACTER_LAYER: &str = include_str!("runtime/character_compact.ll");
const FORMAT_LAYER: &str = include_str!("runtime/format_compact.ll");
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

/// `@tz.string.latin1_fits`, which `allocate_latin1` and the Latin-1 `append` call before they ask the heap
/// for a block. Where the heap refuses a block above a limit (WebAssembly, whose heap is part of the module),
/// a Latin-1 string must not be longer than the UTF-16 string of the same units could be: the length that
/// traps in the UTF-16 form traps here too, in the same call and as the same failure of the heap (the refused
/// request is the UTF-16 one), and `to_utf16` and `borrow_utf16` are never refused for the limit. The limit
/// is the heap's, on twice the length; `with_wasm_heap_limit` rewrites this line with the heap's three
/// (`--wasm-max-memory`), so the text of the line is the one that it matches.
const LATIN1_FITS_LIMITED: &str = "\
define internal void @tz.string.latin1_fits(i64 %length) nounwind {
entry:
  %size = shl i64 %length, 1
  %latin1_fits = icmp ule i64 %size, 16777184
  br i1 %latin1_fits, label %done, label %refuse
refuse:
  %block = call ptr @tz.alloc(i64 %size)
  call void @llvm.trap()
  unreachable
done:
  ret void
}
";
/// Without such a limit (native, and a WebAssembly module whose allocator is the host's) there is nothing
/// to check: the allocator gives what it can, and a Latin-1 string takes half of the bytes of a UTF-16 one.
const LATIN1_FITS_UNLIMITED: &str = "\
define internal void @tz.string.latin1_fits(i64 %length) nounwind alwaysinline {
entry:
  ret void
}
";

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
    /// follow them, up to the heap. `utf16` is that text for the UTF-16 form. The Latin-1 kernels are
    /// the vector ones when `vector` is set, which is when the UTF-16 runtime is the vector one too:
    /// native, and WebAssembly with `simd128`. `limited` says that the heap refuses a block above a
    /// limit that the module has (WebAssembly, unless the allocator is the host's).
    pub(crate) fn string_runtime<'a>(
        self,
        utf16: &'a str,
        vector: bool,
        limited: bool,
    ) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => layered(
                utf16,
                STRING_TAKEN,
                STRING_DROPPED,
                &[
                    if vector { KERNELS_V128 } else { KERNELS },
                    if limited {
                        LATIN1_FITS_LIMITED
                    } else {
                        LATIN1_FITS_UNLIMITED
                    },
                    STRING_LAYER,
                ],
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

    /// `format.ll` for this form: the layer only adds helpers.
    pub(crate) fn format_runtime<'a>(self, utf16: &'a str) -> Result<Cow<'a, str>, Diagnostic> {
        match self {
            Self::Utf16 => Ok(Cow::Borrowed(utf16)),
            Self::Compact => layered(utf16, &[], &[], &[FORMAT_LAYER]).map(Cow::Owned),
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

/// What the emitter does with the length word of a string. With `utf16` these are the instructions that
/// the emitter always wrote, and the word is the length; with `compact` the word is taken apart.
impl FunctionEmitter<'_, '_> {
    /// Whether a value of `ty` can be a Latin-1 string.
    pub(super) fn marks_length(&self, ty: &Type) -> bool {
        *ty == Type::String && self.globals.strings == StringForm::Compact
    }

    /// The length word of the string `value`, which is its length only in the UTF-16 form.
    pub(super) fn string_word(&mut self, ty: &Type, value: &str) -> String {
        self.value(format!("extractvalue {} {value}, 1", self.ty(ty)))
    }

    /// The number of units that the length word `word` of a string of type `ty` counts.
    pub(super) fn string_count(&mut self, ty: &Type, word: &str) -> String {
        if self.marks_length(ty) {
            self.value(format!("and i64 {word}, {MASK}"))
        } else {
            word.to_owned()
        }
    }

    /// The number of units of the string `value`.
    pub(super) fn string_length(&mut self, ty: &Type, value: &str) -> String {
        let word = self.string_word(ty, value);
        self.string_count(ty, &word)
    }

    /// Whether the length word `word` is the one of a Latin-1 string (an `i1`).
    pub(super) fn string_is_latin1(&mut self, word: &str) -> String {
        let marked = self.value(format!("and i64 {word}, {LATIN1}"));
        self.value(format!("icmp ne i64 {marked}, 0"))
    }

    /// The unit at `index` of the string with the data `data` and the length word `word`, as an
    /// `i16`: a byte of a Latin-1 string is widened. The index is in bounds.
    pub(super) fn string_unit(&mut self, data: &str, word: &str, index: &str) -> String {
        let latin1 = self.string_is_latin1(word);
        self.string_unit_with(&latin1, data, index)
    }

    /// `string_unit` for a string that `latin1` (an `i1` that the caller has computed once) says
    /// is Latin-1 or not.
    pub(super) fn string_unit_with(&mut self, latin1: &str, data: &str, index: &str) -> String {
        let bytes = self.label();
        let units = self.label();
        let join = self.label();
        self.branch(latin1, &bytes, &units);
        self.begin(&bytes);
        let pointer = self.value(format!(
            "getelementptr inbounds i8, ptr {data}, i64 {index}"
        ));
        let byte = self.value(format!("load i8, ptr {pointer}"));
        let widened = self.value(format!("zext i8 {byte} to i16"));
        let from_bytes = self.block.clone();
        self.jump(&join);
        self.begin(&units);
        let pointer = self.value(format!(
            "getelementptr inbounds i16, ptr {data}, i64 {index}"
        ));
        let unit = self.value(format!("load i16, ptr {pointer}"));
        let from_units = self.block.clone();
        self.jump(&join);
        self.begin(&join);
        self.value(format!(
            "phi i16 [ {widened}, %{from_bytes} ], [ {unit}, %{from_units} ]"
        ))
    }

    /// In the compact form, `String.slice` and `String.join` of the standard library are runtime
    /// calls: written in the language, each builds a vector of UTF-16 units and `from_code_units`
    /// narrows it, which costs a second allocation and pass for every Latin-1 result. Returns
    /// whether it wrote the body of the function being emitted, whose parameters are `%p0`, ....
    pub(super) fn compact_body(&mut self) -> bool {
        if self.globals.strings != StringForm::Compact
            || self.function.origin.module != ModuleOrigin::Std
            || self.function.module != "String"
        {
            return false;
        }
        let signature = &self.function.signature;
        match (signature.parameters.as_slice(), self.function.name.as_str()) {
            ([Type::Reference(text, false), first, last], "slice")
                if **text == Type::String && *first == Type::I64 && *last == Type::I64 =>
            {
                let result = signature.result.clone();
                self.slice_body(&result);
                true
            }
            (
                [
                    Type::Reference(separator, false),
                    Type::Reference(parts, false),
                ],
                "join",
            ) if **separator == Type::String
                && matches!(parts.as_ref(), Type::Array(element) if **element == Type::String)
                && signature.result == Type::String =>
            {
                let text = self.value("load %tz.string, ptr %p0");
                let data = self.value(format!("extractvalue %tz.string {text}, 0"));
                let word = self.value(format!("extractvalue %tz.string {text}, 1"));
                let items = self.value("extractvalue %tz.array %p1, 0");
                let count = self.value("extractvalue %tz.array %p1, 1");
                let result = self.value(format!(
                    "call %tz.string @tz.string.join(ptr {data}, i64 {word}, ptr {items}, i64 {count})"
                ));
                self.instruction(format!("ret %tz.string {result}"));
                true
            }
            _ => false,
        }
    }

    /// `String.slice text first last`: `None` unless 0 <= first <= last <= length.
    fn slice_body(&mut self, result: &Type) {
        let Type::Union(id, _) = result else {
            unreachable!("String.slice returns a Maybe")
        };
        let none = self.module.unions[*id]
            .cases
            .iter()
            .position(|(name, _)| name == "None")
            .expect("Maybe.None exists");
        let text = self.value("load %tz.string, ptr %p0");
        let data = self.value(format!("extractvalue %tz.string {text}, 0"));
        let word = self.value(format!("extractvalue %tz.string {text}, 1"));
        let length = self.value(format!("and i64 {word}, {MASK}"));
        let before = self.value("icmp slt i64 %p1, 0");
        let inverted = self.value("icmp slt i64 %p2, %p1");
        let beyond = self.value(format!("icmp sgt i64 %p2, {length}"));
        let rejected = self.value(format!("or i1 {before}, {inverted}"));
        let invalid = self.value(format!("or i1 {rejected}, {beyond}"));
        let missing = self.label();
        let present = self.label();
        self.branch(&invalid, &missing, &present);
        self.begin(&missing);
        let empty = self.construct_value(result, none, None);
        self.instruction(format!("ret {} {empty}", self.ty(result)));
        self.begin(&present);
        let piece = self.value(format!(
            "call %tz.string @tz.string.slice(ptr {data}, i64 {word}, i64 %p1, i64 %p2)"
        ));
        let some = self.integer_some(result, &Type::String, &piece);
        self.instruction(format!("ret {} {some}", self.ty(result)));
    }

    /// The constant that holds the text of a string literal, and the length word of the string that
    /// it makes. In the compact form a UTF-16 literal of units that are all at most 0xFF is a
    /// constant of bytes and its word is marked; any other literal is as it always was.
    pub(super) fn string_literal(&mut self, text: &StringLiteral) -> (String, u64) {
        if self.globals.strings == StringForm::Compact
            && let StringLiteral::Utf16(units) = text
            && !units.is_empty()
            && units.iter().all(|unit| *unit <= 0xFF)
        {
            let name = format!("@tz.literal.{}", self.globals.definitions.len());
            let escaped = units
                .iter()
                .map(|unit| format!("\\{unit:02X}"))
                .collect::<String>();
            self.globals.definitions.push(format!(
                "{name} = private unnamed_addr constant [{} x i8] c\"{escaped}\"",
                units.len()
            ));
            return (name, units.len() as u64 | LATIN1);
        }
        (self.string_constant(text), text.len() as u64)
    }

    /// The address of the unit at `index`: the unit itself in a UTF-16 string, and a stack slot
    /// that holds the widened byte in a Latin-1 one. `slot` is an `i16` alloca of the loop.
    pub(super) fn string_unit_pointer(
        &mut self,
        latin1: &str,
        data: &str,
        index: &str,
        slot: &str,
    ) -> String {
        let bytes = self.label();
        let units = self.label();
        let join = self.label();
        self.branch(latin1, &bytes, &units);
        self.begin(&bytes);
        let pointer = self.value(format!(
            "getelementptr inbounds i8, ptr {data}, i64 {index}"
        ));
        let byte = self.value(format!("load i8, ptr {pointer}"));
        let widened = self.value(format!("zext i8 {byte} to i16"));
        self.instruction(format!("store i16 {widened}, ptr {slot}"));
        let from_bytes = self.block.clone();
        self.jump(&join);
        self.begin(&units);
        let pointer = self.value(format!(
            "getelementptr inbounds i16, ptr {data}, i64 {index}"
        ));
        let from_units = self.block.clone();
        self.jump(&join);
        self.begin(&join);
        self.value(format!(
            "phi ptr [ {slot}, %{from_bytes} ], [ {pointer}, %{from_units} ]"
        ))
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
            for vector in [false, true] {
                for limited in [false, true] {
                    let compact = StringForm::Compact
                        .string_runtime(&utf16, vector, limited)
                        .unwrap();
                    assert_layer(&utf16, &compact, STRING_TAKEN, STRING_DROPPED, STRING_LAYER);
                    assert!(
                        compact.contains(if vector { KERNELS_V128 } else { KERNELS }),
                        "{shape} {vector}"
                    );
                    assert!(
                        !compact.contains(if vector { KERNELS } else { KERNELS_V128 }),
                        "{shape} {vector}"
                    );
                    // Exactly one `latin1_fits`, the one that the heap's limit asks for.
                    assert_eq!(
                        defined(&compact)
                            .iter()
                            .filter(|name| **name == "tz.string.latin1_fits")
                            .count(),
                        1,
                        "{shape} {vector} {limited}"
                    );
                    assert_eq!(
                        compact.contains(LATIN1_FITS_LIMITED),
                        limited,
                        "{shape} {vector} {limited}"
                    );
                    assert_eq!(
                        compact.contains(LATIN1_FITS_UNLIMITED),
                        !limited,
                        "{shape} {vector} {limited}"
                    );
                }
            }
            let borrowed = StringForm::Utf16
                .string_runtime(&utf16, true, true)
                .unwrap();
            assert!(
                matches!(borrowed, Cow::Borrowed(text) if text == utf16),
                "{shape}"
            );
        }
    }

    #[test]
    fn the_limit_of_a_latin1_string_follows_the_heap_limit_of_the_module() {
        // `--wasm-max-memory` rewrites the line of the limit with the heap's, and the default leaves it.
        let rewritten = with_wasm_heap_limit(LATIN1_FITS_LIMITED.to_owned(), 64 << 20);
        assert!(rewritten.contains("  %latin1_fits = icmp ule i64 %size, 67108832\n"));
        assert!(!rewritten.contains("16777184"));
        assert_eq!(
            with_wasm_heap_limit(
                LATIN1_FITS_LIMITED.to_owned(),
                crate::driver::DEFAULT_WASM_MAX_MEMORY
            ),
            LATIN1_FITS_LIMITED
        );
        // The heap's own limit is what the line copies.
        let heap = include_str!("runtime/heap-wasm.ll");
        assert!(
            heap.lines()
                .any(|line| line == "  %fits = icmp ule i64 %size, 16777184")
        );
        // The kernels and the layer call it and none of them defines it: the form of the module chooses.
        for text in [KERNELS, KERNELS_V128, STRING_LAYER] {
            assert!(text.contains("call void @tz.string.latin1_fits(i64 %length)"));
            assert!(!text.contains("define internal void @tz.string.latin1_fits("));
        }
    }

    #[test]
    fn the_vector_kernels_define_every_scalar_kernel_and_only_the_helpers_more() {
        let scalar = defined(KERNELS);
        let vector = defined(KERNELS_V128);
        for name in &scalar {
            assert!(
                vector.contains(name),
                "{name} is missing from the vector kernels"
            );
        }
        let scalar_tails = vector
            .iter()
            .filter(|name| name.ends_with(".scalar"))
            .count();
        let helpers: Vec<_> = vector
            .iter()
            .filter(|name| !scalar.contains(name) && !name.ends_with(".scalar"))
            .collect();
        assert_eq!(
            helpers,
            [
                &"tz.utf8string.encode_latin1_run",
                &"tz.string.utf8_to_latin1_run"
            ]
        );
        assert!(scalar_tails >= 6, "{scalar_tails}");
        for name in &vector {
            if let Some(original) = name.strip_suffix(".scalar") {
                assert!(scalar.contains(&original), "{name} has no reference");
            }
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
        let format = include_str!("runtime/format.ll");
        let compact_format = StringForm::Compact.format_runtime(format).unwrap();
        assert_layer(format, &compact_format, &[], &[], FORMAT_LAYER);
        assert!(matches!(
            StringForm::Utf16.format_runtime(format).unwrap(),
            Cow::Borrowed(_)
        ));
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
