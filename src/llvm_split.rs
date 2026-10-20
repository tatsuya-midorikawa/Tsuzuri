//! Codegen units (PB07): the final LLVM IR text of a native `-O0` build is cut into deterministic units that the
//! driver compiles to objects independently and caches by content. The IR text is the only input, so the emitter
//! does not change, and an edit to one module leaves the text of the other units alone as far as the text can be
//! arranged to say so:
//!
//! - A function `@tz.fn.<module>.<name>` (and its `tz.apply.`/`tz.env.` helpers) lives in the unit `m.<module>`.
//!   The runtime and the helpers that the emitter names after a type live in one of 16 buckets `g.<hex digit>`
//!   chosen by a hash of the name. A generated function (a lambda, an instance of a generic function) lives where the
//!   functions that use it live when they are all in one unit, and in a bucket otherwise.
//! - A name that holds a serial number (`$lambda.10`, `$mono.3`, `tz.specialized.4`) shifts when some other part of the
//!   program changes. Such a function is renamed by the hash of its own text, with the callee names it calls
//!   replaced by their own new names first (strongly connected components, callees first), so the name changes only
//!   when the function or something it calls changes.
//! - A constant that is private to the module and has no significant address (string literals, closure descriptors,
//!   trap messages) is copied into every unit that uses it, named `@tz.unit.<k>` by the order of first use in that
//!   unit; metadata nodes and attribute groups are copied and renumbered the same way. A global that is not such a
//!   constant (a variable, a `weak` definition) has one owner unit and is declared `external hidden` elsewhere.
//! - A definition that was `internal` becomes `hidden`, so the other units can call it and it still leaves no
//!   public symbol. A hidden definition is still a global symbol of the link, where it would clash with a host
//!   object or a library that defines the same name (`power`, `decode`, `store`: the helpers of the numeric runtime
//!   that a build without runtime objects embeds). So a promoted definition whose name does not start with `tz.`,
//!   which no C symbol does, is renamed `tz.rt.<name>` in every unit that defines, declares or uses it
//!   ([`promoted_name`]); the name depends on that name alone, never on which units use it.
//!
//! A text with a top-level form that is not listed in [`parse`] is not split (`SplitError`): the driver builds one
//! unit instead, which is always correct.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Range;
use std::sync::OnceLock;

use crate::cache::Sha256;

/// A fast, fixed hash for the tables of names: the text is hashed once per token, millions of times.
#[derive(Default, Clone, Copy)]
struct Fx(u64);

impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
}

type FastState = BuildHasherDefault<Fx>;
type FastMap<K, V> = HashMap<K, V, FastState>;
type FastSet<K> = HashSet<K, FastState>;

/// How many units hold what no module owns. A new generated function changes one bucket.
pub const BUCKETS: u64 = 16;

pub struct SplitInput<'a> {
    pub ir: &'a str,
    /// The modules whose functions are named `@tz.fn.<module>.<name>` (`CheckedFunction::module`).
    pub modules: &'a [&'a str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub name: String,
    pub text: String,
    /// The symbols that this unit defines `hidden` and that were `internal` in the text, by the names that the unit
    /// gives them: a relocatable object that is made of the units makes them local again, so that two objects of two
    /// programs do not define the same name.
    pub localize: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    /// A top-level line that none of the listed forms matches.
    UnknownLine {
        line: usize,
        text: String,
    },
    /// A form that is known but that a unit cannot hold (an alias, an `appending` global, `linkonce`).
    Unsupported {
        line: usize,
        what: &'static str,
    },
    Malformed {
        line: usize,
        what: &'static str,
    },
    /// The name that a promoted definition would get ([`promoted_name`]) is a name of the text already.
    Collision {
        name: String,
    },
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLine { line, text } => write!(f, "unknown top-level line {line}: {text}"),
            Self::Unsupported { line, what } => write!(f, "unsupported {what} at line {line}"),
            Self::Malformed { line, what } => write!(f, "malformed {what} at line {line}"),
            Self::Collision { name } => write!(f, "the reserved name @{name} is already in use"),
        }
    }
}

impl std::error::Error for SplitError {}

// ---------------------------------------------------------------------------------------------------------
// The lexer. Names and numbers are found without reading strings, comments, or the names inside them.
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// `@name`, `@"name"`
    Symbol,
    /// `%name`, `%"name"`: a type or a local value.
    Local,
    /// `#12`
    Attribute,
    /// `!12`
    Metadata,
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    class: Class,
    /// The token with its sigil and quotes.
    start: usize,
    end: usize,
    /// The name without sigil and quotes, or the digits.
    name: &'a str,
}

const PLAIN: u8 = 0;
const QUOTE: u8 = 1;
const COMMENT: u8 = 2;
const AT: u8 = 3;
const PERCENT: u8 = 4;
const HASH: u8 = 5;
const BANG: u8 = 6;

const fn byte_classes() -> [u8; 256] {
    let mut table = [PLAIN; 256];
    table[b'"' as usize] = QUOTE;
    table[b';' as usize] = COMMENT;
    table[b'@' as usize] = AT;
    table[b'%' as usize] = PERCENT;
    table[b'#' as usize] = HASH;
    table[b'!' as usize] = BANG;
    table
}

static CLASSES: [u8; 256] = byte_classes();

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'-')
}

fn identifier_end(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && identifier_byte(bytes[cursor]) {
        cursor += 1;
    }
    cursor
}

/// The index after the string that opens at `open`; a string with no end runs to the end of the text.
fn string_end(bytes: &[u8], open: usize) -> usize {
    let mut cursor = open + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return cursor + 1,
            b'\\' => cursor += 2,
            _ => cursor += 1,
        }
    }
    bytes.len()
}

fn digits_end(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }
    cursor
}

/// Calls `visit` for each name and number of `text`, in order, and for nothing inside a string (`"..."`, `c"..."`,
/// `!"..."`) or a `;` comment. A quoted name (`@"..."`, `%"..."`) is one token.
fn scan<'a>(text: &'a str, mut visit: impl FnMut(Token<'a>)) {
    let bytes = text.as_bytes();
    let length = bytes.len();
    let mut cursor = 0;
    while cursor < length {
        let byte = bytes[cursor];
        match CLASSES[byte as usize] {
            PLAIN => cursor += 1,
            QUOTE => cursor = string_end(bytes, cursor),
            COMMENT => {
                while cursor < length && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            AT | PERCENT => {
                let class = if byte == b'@' {
                    Class::Symbol
                } else {
                    Class::Local
                };
                let start = cursor;
                cursor += 1;
                if bytes.get(cursor) == Some(&b'"') {
                    let end = string_end(bytes, cursor);
                    let inner_end = if end > cursor + 1 && bytes[end - 1] == b'"' {
                        end - 1
                    } else {
                        end
                    };
                    visit(Token {
                        class,
                        start,
                        end,
                        name: &text[cursor + 1..inner_end],
                    });
                    cursor = end;
                } else {
                    let end = identifier_end(bytes, cursor);
                    if end > cursor {
                        visit(Token {
                            class,
                            start,
                            end,
                            name: &text[cursor..end],
                        });
                    }
                    cursor = end;
                }
            }
            HASH | BANG => {
                let start = cursor;
                cursor += 1;
                let end = digits_end(bytes, cursor);
                if end > cursor {
                    visit(Token {
                        class: if byte == b'#' {
                            Class::Attribute
                        } else {
                            Class::Metadata
                        },
                        start,
                        end,
                        name: &text[cursor..end],
                    });
                    cursor = end;
                }
            }
            _ => unreachable!("the table has no other class"),
        }
    }
}

/// How much `line` opens more brackets than it closes, ignoring strings and comments.
fn bracket_depth(line: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut depth = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => {
                cursor = string_end(bytes, cursor);
                continue;
            }
            b';' => break,
            b'[' | b'{' | b'(' => depth += 1,
            b']' | b'}' | b')' => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    depth
}

/// `name` as it follows an `@` or a `%`: quoted when it holds a character that an unquoted name cannot.
fn push_name(out: &mut String, sigil: char, name: &str) {
    out.push(sigil);
    if !name.is_empty() && name.bytes().all(identifier_byte) {
        out.push_str(name);
    } else {
        out.push('"');
        out.push_str(name);
        out.push('"');
    }
}

/// The name that a unit gives a definition that was `internal` or `private` in the text and that the unit makes
/// `hidden`, so that the other units can use it. A hidden definition is a global symbol of the object, and the link of
/// the whole program fails on two definitions of one name, so a generic name (`power`, `decode`, `tzrt_pow5`) must not
/// be left as it is: a host object or a library may define it too, and one module has it local. The names that the
/// emitter makes start with `tz.`, which no name in C does, and stay; the others get `tz.rt.` in front.
fn promoted_name(name: &str) -> Option<String> {
    (!name.starts_with("tz.")).then(|| format!("tz.rt.{name}"))
}

/// The length of the type that starts `text` (`i64`, `ptr`, `[5 x ptr]`, `{ i32, i32 }`, `<4 x i32>`, `%tz.string`,
/// `%"tz.record.A"`), if it is one.
fn type_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    match *bytes.first()? {
        b'[' | b'{' | b'<' => {
            let mut depth = 0i32;
            let mut cursor = 0;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'"' => {
                        cursor = string_end(bytes, cursor);
                        continue;
                    }
                    b'[' | b'{' | b'<' | b'(' => depth += 1,
                    b']' | b'}' | b'>' | b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(cursor + 1);
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            None
        }
        b'%' => {
            if bytes.get(1) == Some(&b'"') {
                Some(string_end(bytes, 1))
            } else {
                Some(identifier_end(bytes, 1))
            }
        }
        byte if byte.is_ascii_alphanumeric() || byte == b'_' => {
            let mut cursor = 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            Some(cursor)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------
// The top-level forms.
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Function(usize),
    Global(usize),
    Declare(usize),
}

#[derive(Debug)]
struct TypeDef<'a> {
    name: &'a str,
    text: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unnamed {
    No,
    Global,
    Local,
}

#[derive(Debug, PartialEq, Eq)]
enum GlobalForm {
    /// A `private` or `internal` constant with no significant address: copied into every unit that uses it.
    Copyable,
    /// A declaration that the text already holds (`@x = external global i32`): copied as it is.
    External,
    /// One owner unit; the other units declare it `external hidden`.
    Single,
}

#[derive(Debug)]
struct Global<'a> {
    name: &'a str,
    text: &'a str,
    form: GlobalForm,
    unnamed: Unnamed,
    /// The `internal` or `private` word, which a definition that other units use turns into `hidden`.
    linkage: Option<Range<usize>>,
    /// The name of a promoted definition with one owner in the units (`promoted_name`), when it is not `name`.
    emitted: Option<String>,
    /// Where the type and the initializer start (after `global` or `constant`).
    body: usize,
    /// What another unit declares for a global with one owner.
    declaration: String,
}

impl Global<'_> {
    /// The name that the definition has in a unit.
    fn unit_name(&self) -> &str {
        self.emitted.as_deref().unwrap_or(self.name)
    }
}

#[derive(Debug)]
struct Declare<'a> {
    name: &'a str,
    text: &'a str,
}

#[derive(Debug, Default)]
struct Linkage {
    local: bool,
    hidden: bool,
    weak: bool,
    word: Option<Range<usize>>,
}

#[derive(Debug)]
struct Function<'a> {
    name: &'a str,
    text: &'a str,
    head_end: usize,
    linkage: Linkage,
    declaration: OnceLock<Declaration>,
}

/// What another unit declares for a function: `before` up to the name and `after` from it, with the parameter
/// names and everything after the parameters removed.
#[derive(Debug)]
struct Declaration {
    before: String,
    after: String,
}

#[derive(Debug, Default)]
struct Parsed<'a> {
    header: Vec<&'a str>,
    types: Vec<TypeDef<'a>>,
    type_ids: FastMap<&'a str, usize>,
    type_first: Vec<bool>,
    globals: Vec<Global<'a>>,
    declares: Vec<Declare<'a>>,
    functions: Vec<Function<'a>>,
    symbols: FastMap<&'a str, Symbol>,
    attributes: Vec<Option<&'a str>>,
    metadata: Vec<Option<&'a str>>,
    named_metadata: Vec<&'a str>,
}

/// The line that holds `offset`.
fn line_number(ir: &str, offset: usize) -> usize {
    ir.as_bytes()[..offset.min(ir.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

impl SplitError {
    /// The error with the line numbers that the parser recorded as byte offsets (counting the lines of every
    /// entity while parsing would read the text once for each).
    fn locate(self, ir: &str) -> Self {
        match self {
            Self::UnknownLine { line, text } => Self::UnknownLine {
                line: line_number(ir, line),
                text,
            },
            Self::Unsupported { line, what } => Self::Unsupported {
                line: line_number(ir, line),
                what,
            },
            Self::Malformed { line, what } => Self::Malformed {
                line: line_number(ir, line),
                what,
            },
            Self::Collision { name } => Self::Collision { name },
        }
    }
}

fn next_line(ir: &str, from: usize) -> usize {
    match ir[from..].find('\n') {
        Some(index) => from + index + 1,
        None => ir.len(),
    }
}

fn line_end(ir: &str, from: usize) -> usize {
    ir[from..].find('\n').map_or(ir.len(), |index| from + index)
}

/// The end (before the line break) of the entity that starts at `start`, which goes on until its brackets close.
fn entity_end(ir: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut cursor = start;
    loop {
        let end = line_end(ir, cursor);
        depth += bracket_depth(&ir[cursor..end]);
        if depth <= 0 {
            return Some(end);
        }
        if end >= ir.len() {
            return None;
        }
        cursor = end + 1;
    }
}

fn first_symbol(text: &str) -> Option<Token<'_>> {
    let mut found = None;
    let mut done = false;
    scan(text, |token| {
        if !done && token.class == Class::Symbol {
            found = Some(token);
            done = true;
        }
    });
    found
}

/// The linkage words of a definition, from `from`, up to the first word that is not one. A linkage that a unit
/// cannot hold is an error.
fn linkage_words(
    text: &str,
    from: usize,
    line: usize,
    global: bool,
) -> Result<(Linkage, bool, Unnamed, usize), SplitError> {
    let mut linkage = Linkage::default();
    let mut thread_local = false;
    let mut unnamed = Unnamed::No;
    let mut cursor = from;
    loop {
        let rest = &text[cursor..];
        let length = rest.find([' ', '\n']).unwrap_or(rest.len());
        let word = &rest[..length];
        let known = match word {
            "private" | "internal" => {
                linkage.local = true;
                linkage.word = Some(cursor..cursor + length);
                true
            }
            "hidden" => {
                linkage.hidden = true;
                true
            }
            "weak" => {
                linkage.weak = true;
                true
            }
            "external" | "default" | "dso_local" | "dso_preemptable" => true,
            "unnamed_addr" if global => {
                unnamed = Unnamed::Global;
                true
            }
            "local_unnamed_addr" if global => {
                unnamed = Unnamed::Local;
                true
            }
            "thread_local" if global => {
                thread_local = true;
                true
            }
            word if global && word.starts_with("thread_local(") => {
                thread_local = true;
                true
            }
            "linkonce"
            | "linkonce_odr"
            | "weak_odr"
            | "common"
            | "appending"
            | "extern_weak"
            | "available_externally" => {
                return Err(SplitError::Unsupported {
                    line,
                    what: "linkage",
                });
            }
            _ => false,
        };
        if !known {
            return Ok((linkage, thread_local, unnamed, cursor));
        }
        cursor += length + 1;
        if cursor >= text.len() {
            return Err(SplitError::Malformed {
                line,
                what: "definition",
            });
        }
    }
}

fn parse_global<'a>(text: &'a str, line: usize) -> Result<Global<'a>, SplitError> {
    let name = first_symbol(text).ok_or(SplitError::Malformed {
        line,
        what: "global",
    })?;
    let equals = text.find(" = ").ok_or(SplitError::Malformed {
        line,
        what: "global",
    })?;
    let (linkage, thread_local, unnamed, cursor) = linkage_words(text, equals + 3, line, true)?;
    let rest = &text[cursor..];
    let length = rest.find([' ', '\n']).unwrap_or(rest.len());
    let constant = match &rest[..length] {
        "global" => false,
        "constant" => true,
        "alias" | "ifunc" => {
            return Err(SplitError::Unsupported {
                line,
                what: "alias or ifunc",
            });
        }
        _ => {
            return Err(SplitError::Unsupported {
                line,
                what: "global form",
            });
        }
    };
    let external = text[equals + 3..cursor]
        .split(' ')
        .any(|word| word == "external");
    let form = if external {
        GlobalForm::External
    } else if linkage.local && constant && unnamed != Unnamed::No && !thread_local {
        GlobalForm::Copyable
    } else {
        GlobalForm::Single
    };
    let body = cursor + length;
    let hidden = linkage.local || linkage.hidden;
    // A variable with one owner that the units make `hidden` from `internal` or `private` (a copied constant is
    // `private` to every unit, which no link sees).
    let emitted = if form == GlobalForm::Single && linkage.local {
        promoted_name(name.name)
    } else {
        None
    };
    let mut declaration = String::new();
    if form == GlobalForm::Single {
        let after = text[body..].trim_start();
        let type_length = type_len(after).ok_or(SplitError::Malformed {
            line,
            what: "global type",
        })?;
        push_name(
            &mut declaration,
            '@',
            emitted.as_deref().unwrap_or(name.name),
        );
        declaration.push_str(" = external ");
        if hidden {
            declaration.push_str("hidden ");
        }
        if thread_local {
            declaration.push_str("thread_local ");
        }
        declaration.push_str(match unnamed {
            Unnamed::Global => "unnamed_addr ",
            Unnamed::Local => "local_unnamed_addr ",
            Unnamed::No => "",
        });
        declaration.push_str(if constant { "constant " } else { "global " });
        declaration.push_str(&after[..type_length]);
        if let Some(index) = text.rfind(", align ") {
            let digits = text[index + 8..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            declaration.push_str(&text[index..index + 8 + digits]);
        }
    }
    Ok(Global {
        name: name.name,
        text,
        form,
        unnamed,
        linkage: linkage.word,
        emitted,
        body,
        declaration,
    })
}

fn parse_function<'a>(
    ir: &'a str,
    start: usize,
    line: usize,
) -> Result<(Function<'a>, usize), SplitError> {
    let head_end = line_end(ir, start);
    let head = &ir[start..head_end];
    if !head.trim_end().ends_with('{') {
        return Err(SplitError::Malformed {
            line,
            what: "function head",
        });
    }
    let name = first_symbol(head).ok_or(SplitError::Malformed {
        line,
        what: "function name",
    })?;
    let (linkage, _, _, _) = linkage_words(head, "define ".len(), line, false)?;
    let mut cursor = next_line(ir, start);
    loop {
        if cursor >= ir.len() {
            return Err(SplitError::Malformed {
                line,
                what: "function without a closing brace",
            });
        }
        if ir.as_bytes()[cursor] == b'}' {
            let end = cursor + 1;
            let function = Function {
                name: name.name,
                text: &ir[start..end],
                head_end: head_end - start,
                linkage,
                declaration: OnceLock::new(),
            };
            return Ok((function, end));
        }
        cursor = next_line(ir, cursor);
    }
}

fn parse_type_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('%')?;
    let (name, after) = if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted.find('"')?;
        (&quoted[..end], &quoted[end + 1..])
    } else {
        let end = identifier_end(rest.as_bytes(), 0);
        (&rest[..end], &rest[end..])
    };
    after.starts_with(" = type").then_some(name)
}

fn parse(ir: &str) -> Result<Parsed<'_>, SplitError> {
    parse_entities(ir).map_err(|error| error.locate(ir))
}

/// [`parse`], with the byte offset of each entity where an error says its line.
fn parse_entities(ir: &str) -> Result<Parsed<'_>, SplitError> {
    let mut parsed = Parsed::default();
    let mut cursor = 0;
    while cursor < ir.len() {
        let end = line_end(ir, cursor);
        let line = &ir[cursor..end];
        if line.is_empty() || line.starts_with(';') {
            cursor = next_line(ir, cursor);
            continue;
        }
        let number = cursor;
        if line.starts_with("define ") {
            let (function, after) = parse_function(ir, cursor, number)?;
            parsed
                .symbols
                .insert(function.name, Symbol::Function(parsed.functions.len()));
            parsed.functions.push(function);
            cursor = after;
            continue;
        }
        let end = entity_end(ir, cursor).ok_or(SplitError::Malformed {
            line: number,
            what: "unbalanced entity",
        })?;
        let text = &ir[cursor..end];
        if line.starts_with("source_filename") || line.starts_with("target ") {
            parsed.header.push(text);
        } else if line.starts_with("declare ") {
            let name = first_symbol(line).ok_or(SplitError::Malformed {
                line: number,
                what: "declaration",
            })?;
            parsed
                .symbols
                .insert(name.name, Symbol::Declare(parsed.declares.len()));
            parsed.declares.push(Declare {
                name: name.name,
                text,
            });
        } else if line.starts_with('%') {
            let name = parse_type_name(line).ok_or_else(|| SplitError::UnknownLine {
                line: number,
                text: line.chars().take(80).collect(),
            })?;
            parsed.type_ids.insert(name, parsed.types.len());
            parsed.types.push(TypeDef { name, text });
        } else if line.starts_with('@') {
            let global = parse_global(text, number)?;
            parsed
                .symbols
                .insert(global.name, Symbol::Global(parsed.globals.len()));
            parsed.globals.push(global);
        } else if let Some(rest) = line.strip_prefix("attributes #") {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let group: usize = rest[..digits].parse().map_err(|_| SplitError::Malformed {
                line: number,
                what: "attribute group",
            })?;
            let body =
                text.split_once(" = ")
                    .map(|(_, body)| body)
                    .ok_or(SplitError::Malformed {
                        line: number,
                        what: "attribute group",
                    })?;
            if parsed.attributes.len() <= group {
                parsed.attributes.resize(group + 1, None);
            }
            parsed.attributes[group] = Some(body);
        } else if let Some(rest) = line.strip_prefix('!') {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                let node: usize = rest[..digits].parse().map_err(|_| SplitError::Malformed {
                    line: number,
                    what: "metadata node",
                })?;
                let body =
                    text.split_once(" = ")
                        .map(|(_, body)| body)
                        .ok_or(SplitError::Malformed {
                            line: number,
                            what: "metadata node",
                        })?;
                if parsed.metadata.len() <= node {
                    parsed.metadata.resize(node + 1, None);
                }
                parsed.metadata[node] = Some(body);
            } else if rest.contains(" = ") {
                parsed.named_metadata.push(text);
            } else {
                return Err(SplitError::UnknownLine {
                    line: number,
                    text: line.chars().take(80).collect(),
                });
            }
        } else {
            return Err(SplitError::UnknownLine {
                line: number,
                text: line.chars().take(80).collect(),
            });
        }
        cursor = if end >= ir.len() { ir.len() } else { end + 1 };
    }
    let mut first = vec![false; 256];
    for definition in &parsed.types {
        if let Some(byte) = definition.name.bytes().next() {
            first[byte as usize] = true;
        }
    }
    parsed.type_first = first;
    Ok(parsed)
}

// ---------------------------------------------------------------------------------------------------------
// Names. A serial number in a name is replaced by the hash of the text of the function that has it.
// ---------------------------------------------------------------------------------------------------------

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The ranges of the components of a name, which a dot outside brackets separates (a type name inside a symbol
/// name holds dots of its own).
fn component_ranges(name: &str) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (index, byte) in name.bytes().enumerate() {
        match byte {
            b'[' | b'(' | b'<' => depth += 1,
            b']' | b')' | b'>' => depth -= 1,
            b'.' if depth == 0 => {
                parts.push(start..index);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(start..name.len());
    parts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Naming<'a> {
    /// The name holds no serial number. A user function and its helpers know the module they belong to.
    Stable {
        module: Option<&'a str>,
    },
    Unstable,
}

/// Whether `name` is the same in every build of the same code, and the module a function of the program belongs
/// to. `tz.fn.<module>.<name>` and the helpers `tz.apply.`, `tz.env.clone.`, `tz.env.drop.` followed by the same
/// and an index (`tz.apply.Main.divide.0`) are the names of a user function; a component of only digits
/// anywhere else is a serial number.
fn classify<'a>(name: &'a str, modules: &FastSet<&str>) -> Naming<'a> {
    let mut qualified = None;
    // `tz.trap.report.<module>` is the trap report of one file (`--trap-info`): the name is all module.
    let mut whole = false;
    for (prefix, helper) in [
        ("tz.fn.", false),
        ("tz.apply.", true),
        ("tz.env.clone.", true),
        ("tz.env.drop.", true),
        ("tz.trap.report.", false),
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            whole = prefix == "tz.trap.report.";
            qualified = Some(if helper {
                match rest.rsplit_once('.') {
                    Some((owner, index)) if all_digits(index) => owner,
                    _ => rest,
                }
            } else {
                rest
            });
            break;
        }
    }
    let checked = qualified.unwrap_or(name);
    let parts = component_ranges(checked);
    if parts.iter().any(|part| all_digits(&checked[part.clone()])) {
        return Naming::Unstable;
    }
    let module = qualified.and_then(|owner| {
        let longest = if whole { parts.len() } else { parts.len() - 1 };
        (1..=longest)
            .rev()
            .map(|count| &owner[..parts[count - 1].end])
            .find(|candidate| modules.contains(candidate))
    });
    Naming::Stable { module }
}

/// `name` without its components of only digits.
fn without_serials(name: &str) -> String {
    let mut kept = String::with_capacity(name.len());
    for part in component_ranges(name) {
        let text = &name[part];
        if all_digits(text) {
            continue;
        }
        if !kept.is_empty() {
            kept.push('.');
        }
        kept.push_str(text);
    }
    kept
}

/// For an adapter `tz.apply.<symbol>.<index>`: its index, its symbol, and the function `tz.fn.<symbol>` that it is for.
fn adapter_function<'a>(parsed: &Parsed<'a>, name: &'a str) -> Option<(&'a str, &'a str, usize)> {
    let (symbol, index) = name.strip_prefix("tz.apply.")?.rsplit_once('.')?;
    if !all_digits(index) {
        return None;
    }
    match parsed.symbols.get(format!("tz.fn.{symbol}").as_str()) {
        Some(Symbol::Function(id)) => Some((index, symbol, *id)),
        _ => None,
    }
}

fn hex16(digest: &[u8; 32]) -> String {
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The strongly connected components of a graph, callees first (iterative Tarjan: a program has tens of
/// thousands of functions).
fn strongly_connected(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let count = edges.len();
    let mut index = vec![UNSEEN; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut work: Vec<(usize, usize)> = Vec::new();
    let mut next = 0;
    for root in 0..count {
        if index[root] != UNSEEN {
            continue;
        }
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        work.push((root, 0));
        while let Some(top) = work.last_mut() {
            let node = top.0;
            if top.1 < edges[node].len() {
                let target = edges[node][top.1];
                top.1 += 1;
                if index[target] == UNSEEN {
                    index[target] = next;
                    low[target] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    work.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(index[target]);
                }
            } else {
                work.pop();
                if let Some(parent) = work.last() {
                    low[parent.0] = low[parent.0].min(low[node]);
                }
                if low[node] == index[node] {
                    let mut component = Vec::new();
                    loop {
                        let member = stack.pop().expect("a component member is on the stack");
                        on_stack[member] = false;
                        component.push(member);
                        if member == node {
                            break;
                        }
                    }
                    component.sort_unstable();
                    components.push(component);
                }
            }
        }
    }
    components
}

/// The unstable functions that `text` refers to, directly or through the constants it copies.
fn function_edges(parsed: &Parsed<'_>, text: &str, nodes: &FastMap<usize, usize>) -> Vec<usize> {
    let mut targets = BTreeSet::new();
    let mut seen = FastSet::default();
    let mut pending = vec![text];
    while let Some(text) = pending.pop() {
        scan(text, |token| {
            if token.class != Class::Symbol {
                return;
            }
            match parsed.symbols.get(token.name) {
                Some(Symbol::Function(id)) => {
                    if let Some(node) = nodes.get(id) {
                        targets.insert(*node);
                    }
                }
                Some(Symbol::Global(id))
                    if parsed.globals[*id].form == GlobalForm::Copyable && seen.insert(*id) =>
                {
                    pending.push(parsed.globals[*id].text);
                }
                _ => {}
            }
        });
    }
    targets.into_iter().collect()
}

/// The unit that a function is defined in: a module's, or one of the buckets.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Home<'a> {
    Bucket(u64),
    Module(&'a str),
}

/// The homes of the functions that refer to one generated function.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Users<'a> {
    Nobody,
    One(Home<'a>),
    Several,
}

impl<'a> Users<'a> {
    fn add(&mut self, home: Home<'a>) {
        *self = match *self {
            Users::Nobody => Users::One(home),
            Users::One(other) if other == home => Users::One(home),
            _ => Users::Several,
        };
    }

    fn merge(&mut self, other: Users<'a>) {
        match other {
            Users::Nobody => {}
            Users::One(home) => self.add(home),
            Users::Several => *self = Users::Several,
        }
    }
}

struct Plan<'a> {
    /// The new name of a function whose name holds a serial number.
    renamed: Vec<Option<String>>,
    /// The unit that defines each function that no other function defines.
    homes: Vec<Home<'a>>,
    /// The function that defines each function: itself, or the first of the functions that have its new name and
    /// the same text (two equal lambdas in two modules are one function, as two equal functions are for the linker's
    /// identical-code folding; nothing in the language can tell them apart, because a function value cannot be
    /// compared).
    canonical: Vec<usize>,
}

/// The home of every function. A function with a stable name belongs to the module in its name, or, for the runtime
/// and the helpers that the emitter names after a type, to the bucket that its name hashes to. A generated function
/// (a lambda, an instance of a generic function, a specialization) has no name to say where it belongs, so it follows
/// the functions that use it: when all of them are in one unit it goes there, so that an edit to a module changes the
/// unit of that module alone however many lambdas it has; when they are in several it goes to the bucket that its
/// name hashes to. The functions are placed users first, so a lambda inside a lambda follows the outer one.
fn place<'a>(
    parsed: &Parsed<'a>,
    owners: &[Option<&'a str>],
    renamed: &[Option<String>],
    canonical: &[usize],
    unstable: &[usize],
    nodes: &FastMap<usize, usize>,
    edges: &[Vec<usize>],
) -> Vec<Home<'a>> {
    let count = parsed.functions.len();
    let by_name = |id: usize| {
        Home::Bucket(bucket(
            renamed[id].as_deref().unwrap_or(parsed.functions[id].name),
        ))
    };
    let mut homes: Vec<Home<'a>> = (0..count)
        .map(|id| match owners[id] {
            Some(module) => Home::Module(module),
            None => by_name(id),
        })
        .collect();
    if unstable.is_empty() {
        return homes;
    }
    // A function that is merged into another one is not defined, so it uses nothing; what it referred to is
    // referred to by the function that defines it.
    let representative = |node: usize| nodes[&canonical[unstable[node]]];
    let mut users = vec![Users::Nobody; unstable.len()];
    for id in 0..count {
        if canonical[id] == id && !nodes.contains_key(&id) {
            for &target in &edges[id] {
                users[representative(target)].add(homes[id]);
            }
        }
    }
    let graph: Vec<Vec<usize>> = (0..unstable.len())
        .map(|node| {
            if canonical[unstable[node]] != unstable[node] {
                return Vec::new();
            }
            let mut targets: Vec<usize> = edges[unstable[node]]
                .iter()
                .map(|&target| representative(target))
                .collect();
            targets.sort_unstable();
            targets.dedup();
            targets
        })
        .collect();
    let mut component_of = vec![usize::MAX; unstable.len()];
    // The components come callees first; the users of a function are placed before it when they run the other way.
    for (number, component) in strongly_connected(&graph).into_iter().enumerate().rev() {
        let members: Vec<usize> = component
            .into_iter()
            .filter(|&node| canonical[unstable[node]] == unstable[node])
            .collect();
        for &node in &members {
            component_of[node] = number;
        }
        let mut together = Users::Nobody;
        for &node in &members {
            together.merge(users[node]);
        }
        for &node in &members {
            if let Users::One(home) = together {
                homes[unstable[node]] = home;
            }
        }
        for &node in &members {
            for &target in &graph[node] {
                if component_of[target] != number {
                    users[target].add(homes[unstable[node]]);
                }
            }
        }
    }
    homes
}

fn plan<'a>(parsed: &Parsed<'a>, modules: &FastSet<&str>) -> Result<Plan<'a>, SplitError> {
    let count = parsed.functions.len();
    let mut owners = Vec::with_capacity(count);
    let mut unstable = Vec::new();
    let mut renamed: Vec<Option<String>> = vec![None; count];
    // The names that are taken, with the digest of the text they stand for and the function that has it.
    let mut used: FastMap<String, ([u8; 32], usize)> = FastMap::default();
    for global in &parsed.globals {
        if let Some(name) = &global.emitted
            && parsed.symbols.contains_key(name.as_str())
        {
            return Err(SplitError::Collision { name: name.clone() });
        }
    }
    for (id, function) in parsed.functions.iter().enumerate() {
        match classify(function.name, modules) {
            Naming::Stable { module } => {
                owners.push(module);
                if function.linkage.word.is_some()
                    && let Some(name) = promoted_name(function.name)
                {
                    if parsed.symbols.contains_key(name.as_str()) {
                        return Err(SplitError::Collision { name });
                    }
                    // A digest of no text, so that a generated function never takes the name.
                    used.insert(name.clone(), ([0; 32], id));
                    renamed[id] = Some(name);
                }
            }
            Naming::Unstable => {
                owners.push(None);
                unstable.push(id);
            }
        }
    }
    let mut canonical: Vec<usize> = (0..count).collect();
    let nodes: FastMap<usize, usize> = unstable
        .iter()
        .enumerate()
        .map(|(node, id)| (*id, node))
        .collect();
    // The generated functions that each function refers to. They name the generated functions (what a function
    // calls decides its own name) and place them (who calls a function decides where it is defined).
    let edges: Vec<Vec<usize>> = if unstable.is_empty() {
        Vec::new()
    } else {
        (0..count)
            .map(|id| function_edges(parsed, parsed.functions[id].text, &nodes))
            .collect()
    };
    if !unstable.is_empty() {
        let graph: Vec<Vec<usize>> = unstable.iter().map(|id| edges[*id].clone()).collect();
        let edges = graph;
        for component in strongly_connected(&edges) {
            let members: Vec<usize> = component.iter().map(|node| unstable[*node]).collect();
            let placeholders: Vec<String> =
                (0..members.len()).map(|n| format!("tz.scc.{n}")).collect();
            let mut hash = Sha256::new();
            for &member in &members {
                let hashing = Hashing {
                    members: &members,
                    placeholders: &placeholders,
                    current: member,
                };
                let mut builder = Builder::new(parsed, &renamed, Some(hashing), None);
                let text = builder.hash_text(member);
                hash.update(text.as_bytes());
                hash.update(&[0]);
            }
            let full = hash.finalize();
            let digest = hex16(&full);
            // An adapter `tz.apply.<symbol>.<index>` is named after the function `tz.fn.<symbol>` that it calls, as it
            // was before: the debugger finds a function value's function by cutting the index off the adapter's name.
            // The function is named first when both are in this component.
            let callees: Vec<Option<(&str, &str, usize)>> = members
                .iter()
                .map(|&member| adapter_function(parsed, parsed.functions[member].name))
                .collect();
            let together = |callee: usize| members.contains(&callee);
            for second in [false, true] {
                for (position, &member) in members.iter().enumerate() {
                    let adapter = callees[position];
                    if adapter.is_some_and(|(_, _, callee)| together(callee)) != second {
                        continue;
                    }
                    let mut name = format!(
                        "{}.h{digest}",
                        without_serials(parsed.functions[member].name)
                    );
                    if members.len() > 1 {
                        name.push_str(&format!(".{position}"));
                    }
                    if let Some((index, _, callee)) = adapter
                        && let Some(symbol) = renamed[callee]
                            .as_deref()
                            .and_then(|new| new.strip_prefix("tz.fn."))
                    {
                        name = format!("tz.apply.{symbol}.{index}");
                    }
                    // A definition that a unit makes `hidden` from `internal` has a name of the reserved kind. Such a
                    // name must not be one of the text either: no function has it, but a generic name that is
                    // promoted and the name of a generated one could meet.
                    let reserved = parsed.functions[member]
                        .linkage
                        .word
                        .is_some()
                        .then(|| promoted_name(&name))
                        .flatten();
                    if let Some(reserved) = &reserved {
                        name.clone_from(reserved);
                    }
                    // The same name for the same text is one function; for another text (a collision of the
                    // 64 bits of the name) it is another name.
                    let mut candidate = name.clone();
                    let mut number = 1;
                    loop {
                        match used.get(&candidate) {
                            None if reserved.is_some()
                                && parsed.symbols.contains_key(candidate.as_str()) =>
                            {
                                number += 1;
                                candidate = format!("{name}.{number}");
                            }
                            None => {
                                used.insert(candidate.clone(), (full, member));
                                break;
                            }
                            Some((other, first)) if *other == full => {
                                canonical[member] = *first;
                                break;
                            }
                            Some(_) => {
                                number += 1;
                                candidate = format!("{name}.{number}");
                            }
                        }
                    }
                    renamed[member] = Some(candidate);
                }
            }
        }
    }
    let homes = place(
        parsed, &owners, &renamed, &canonical, &unstable, &nodes, &edges,
    );
    Ok(Plan {
        renamed,
        homes,
        canonical,
    })
}

fn bucket(name: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash ^ (hash >> 32)) % BUCKETS
}

// ---------------------------------------------------------------------------------------------------------
// One unit's text.
// ---------------------------------------------------------------------------------------------------------

/// The function a hash text is for, and the names that stand for it and for its component.
#[derive(Clone, Copy)]
struct Hashing<'p> {
    members: &'p [usize],
    placeholders: &'p [String],
    current: usize,
}

/// The unit of every function and of every global with one owner.
struct Homes {
    functions: Vec<usize>,
    globals: Vec<usize>,
}

struct Builder<'p, 'a> {
    parsed: &'p Parsed<'a>,
    renamed: &'p [Option<String>],
    hashing: Option<Hashing<'p>>,
    homes: Option<&'p Homes>,
    this: usize,
    consts: Vec<usize>,
    const_slots: FastMap<usize, usize>,
    attrs: Vec<usize>,
    attr_slots: FastMap<usize, usize>,
    metas: Vec<usize>,
    meta_slots: FastMap<usize, usize>,
    type_seen: Vec<bool>,
    type_list: Vec<usize>,
    foreign_functions: FastSet<usize>,
    foreign_globals: FastSet<usize>,
    declares: FastSet<usize>,
    copied_externals: FastSet<usize>,
}

fn slot(list: &mut Vec<usize>, slots: &mut FastMap<usize, usize>, old: usize) -> usize {
    *slots.entry(old).or_insert_with(|| {
        list.push(old);
        list.len() - 1
    })
}

fn function_name<'p>(
    parsed: &'p Parsed<'_>,
    renamed: &'p [Option<String>],
    hashing: Option<Hashing<'p>>,
    id: usize,
) -> &'p str {
    if let Some(hashing) = hashing {
        if id == hashing.current {
            return "tz.self";
        }
        if let Some(position) = hashing.members.iter().position(|member| *member == id) {
            return &hashing.placeholders[position];
        }
    }
    renamed[id].as_deref().unwrap_or(parsed.functions[id].name)
}

impl<'p, 'a> Builder<'p, 'a> {
    fn new(
        parsed: &'p Parsed<'a>,
        renamed: &'p [Option<String>],
        hashing: Option<Hashing<'p>>,
        homes: Option<(&'p Homes, usize)>,
    ) -> Self {
        Self {
            parsed,
            renamed,
            hashing,
            homes: homes.map(|(homes, _)| homes),
            this: homes.map_or(0, |(_, unit)| unit),
            consts: Vec::new(),
            const_slots: FastMap::default(),
            attrs: Vec::new(),
            attr_slots: FastMap::default(),
            metas: Vec::new(),
            meta_slots: FastMap::default(),
            type_seen: vec![false; parsed.types.len()],
            type_list: Vec::new(),
            foreign_functions: FastSet::default(),
            foreign_globals: FastSet::default(),
            declares: FastSet::default(),
            copied_externals: FastSet::default(),
        }
    }

    fn owns_function(&self, id: usize) -> bool {
        self.homes
            .is_none_or(|homes| homes.functions[id] == self.this)
    }

    fn owns_global(&self, id: usize) -> bool {
        self.homes
            .is_none_or(|homes| homes.globals[id] == self.this)
    }

    fn type_used(&mut self, id: usize) {
        if !self.type_seen[id] {
            self.type_seen[id] = true;
            self.type_list.push(id);
        }
    }

    /// Writes `text` with the names, constants, attribute groups, and metadata nodes of the unit.
    fn rewrite(&mut self, text: &str, out: &mut String) {
        let parsed = self.parsed;
        let renamed = self.renamed;
        let hashing = self.hashing;
        let mut last = 0;
        scan(text, |token| match token.class {
            Class::Symbol => match parsed.symbols.get(token.name) {
                Some(&Symbol::Function(id)) => {
                    if !self.owns_function(id) {
                        self.foreign_functions.insert(id);
                    }
                    let name = function_name(parsed, renamed, hashing, id);
                    if name != token.name {
                        out.push_str(&text[last..token.start]);
                        push_name(out, '@', name);
                        last = token.end;
                    }
                }
                Some(&Symbol::Global(id)) => match parsed.globals[id].form {
                    GlobalForm::Copyable => {
                        let number = slot(&mut self.consts, &mut self.const_slots, id);
                        out.push_str(&text[last..token.start]);
                        out.push_str("@tz.unit.");
                        out.push_str(&number.to_string());
                        last = token.end;
                    }
                    GlobalForm::Single => {
                        if !self.owns_global(id) {
                            self.foreign_globals.insert(id);
                        }
                        if let Some(name) = &parsed.globals[id].emitted {
                            out.push_str(&text[last..token.start]);
                            push_name(out, '@', name);
                            last = token.end;
                        }
                    }
                    GlobalForm::External => {
                        self.copied_externals.insert(id);
                    }
                },
                Some(&Symbol::Declare(id)) => {
                    self.declares.insert(id);
                }
                None => {}
            },
            Class::Local => {
                let first = token.name.as_bytes().first().copied().unwrap_or(0);
                if parsed.type_first[first as usize]
                    && let Some(&id) = parsed.type_ids.get(token.name)
                {
                    self.type_used(id);
                }
            }
            Class::Attribute => {
                if let Ok(old) = token.name.parse::<usize>() {
                    let number = slot(&mut self.attrs, &mut self.attr_slots, old);
                    out.push_str(&text[last..token.start]);
                    out.push('#');
                    out.push_str(&number.to_string());
                    last = token.end;
                }
            }
            Class::Metadata => {
                if let Ok(old) = token.name.parse::<usize>() {
                    let number = slot(&mut self.metas, &mut self.meta_slots, old);
                    out.push_str(&text[last..token.start]);
                    out.push('!');
                    out.push_str(&number.to_string());
                    last = token.end;
                }
            }
        });
        out.push_str(&text[last..]);
    }

    /// Notes the types that `text` names, without writing it.
    fn note_types(&mut self, text: &str) {
        let parsed = self.parsed;
        scan(text, |token| {
            if token.class == Class::Local {
                let first = token.name.as_bytes().first().copied().unwrap_or(0);
                if parsed.type_first[first as usize]
                    && let Some(&id) = parsed.type_ids.get(token.name)
                {
                    self.type_used(id);
                }
            }
        });
    }

    fn function(&mut self, id: usize, out: &mut String, hidden: bool) {
        let parsed = self.parsed;
        let function = &parsed.functions[id];
        let text = function.text;
        match &function.linkage.word {
            Some(word) if hidden => {
                let mut head = String::with_capacity(function.head_end);
                head.push_str(&text[..word.start]);
                head.push_str("hidden");
                head.push_str(&text[word.end..function.head_end]);
                self.rewrite(&head, out);
                self.rewrite(&text[function.head_end..], out);
            }
            _ => self.rewrite(text, out),
        }
        out.push('\n');
    }

    fn global(&mut self, id: usize, out: &mut String) {
        let parsed = self.parsed;
        let global = &parsed.globals[id];
        match &global.linkage {
            Some(word) => {
                let mut text = String::with_capacity(global.text.len());
                text.push_str(&global.text[..word.start]);
                text.push_str("hidden");
                text.push_str(&global.text[word.end..]);
                self.rewrite(&text, out);
            }
            None => self.rewrite(global.text, out),
        }
        out.push('\n');
    }

    /// The copied constants, in the order of their first use; one constant can bring more.
    fn constants(&mut self, out: &mut String) {
        let parsed = self.parsed;
        let mut index = 0;
        while index < self.consts.len() {
            let global = &parsed.globals[self.consts[index]];
            out.push_str("@tz.unit.");
            out.push_str(&index.to_string());
            out.push_str(" = private ");
            if global.unnamed == Unnamed::Global {
                out.push_str("unnamed_addr");
            } else {
                out.push_str("local_unnamed_addr");
            }
            out.push_str(" constant");
            self.rewrite(&global.text[global.body..], out);
            out.push('\n');
            index += 1;
        }
    }

    fn attribute_groups(&mut self, out: &mut String) {
        let parsed = self.parsed;
        let mut index = 0;
        while index < self.attrs.len() {
            if let Some(body) = parsed.attributes.get(self.attrs[index]).copied().flatten() {
                out.push_str("attributes #");
                out.push_str(&index.to_string());
                out.push_str(" = ");
                self.rewrite(body, out);
                out.push('\n');
            }
            index += 1;
        }
    }

    /// Notes the metadata nodes that the nodes in use refer to, without writing them.
    fn close_metadata(&mut self) {
        let parsed = self.parsed;
        let mut index = 0;
        while index < self.metas.len() {
            if let Some(body) = parsed.metadata.get(self.metas[index]).copied().flatten() {
                scan(body, |token| {
                    if token.class == Class::Metadata
                        && let Ok(old) = token.name.parse::<usize>()
                    {
                        slot(&mut self.metas, &mut self.meta_slots, old);
                    }
                });
            }
            index += 1;
        }
    }

    fn metadata_nodes(&mut self, out: &mut String) {
        let parsed = self.parsed;
        let mut index = 0;
        while index < self.metas.len() {
            if let Some(body) = parsed.metadata.get(self.metas[index]).copied().flatten() {
                out.push('!');
                out.push_str(&index.to_string());
                out.push_str(" = ");
                self.rewrite(body, out);
                out.push('\n');
            }
            index += 1;
        }
    }

    /// The text that names a function, its constants, its attribute groups, and its metadata: what the hash of a
    /// renamed function is taken over.
    fn hash_text(&mut self, id: usize) -> String {
        let mut functions = String::new();
        self.function(id, &mut functions, false);
        let mut text = String::with_capacity(functions.len() + 256);
        self.constants(&mut text);
        text.push_str(&functions);
        self.attribute_groups(&mut text);
        self.metadata_nodes(&mut text);
        text
    }

    /// The text of a unit that holds `functions` and the globals `globals`, in the order they are given.
    fn unit(mut self, functions: &[usize], globals: &[usize]) -> String {
        let parsed = self.parsed;
        let mut global_text = String::new();
        for &id in globals {
            self.global(id, &mut global_text);
        }
        let mut function_text = String::new();
        for &id in functions {
            self.function(id, &mut function_text, true);
        }
        let mut const_text = String::new();
        self.constants(&mut const_text);

        // What the unit uses and does not define, by name.
        let mut declarations: BTreeMap<String, String> = BTreeMap::new();
        let mut foreign: Vec<usize> = self.foreign_functions.iter().copied().collect();
        foreign.sort_unstable();
        for id in foreign {
            let name = function_name(parsed, self.renamed, None, id);
            let declaration = parsed.functions[id]
                .declaration
                .get_or_init(|| declaration_of(&parsed.functions[id]));
            let mut text = declaration.before.clone();
            push_name(&mut text, '@', name);
            text.push_str(&declaration.after);
            self.note_types(&text);
            text.push('\n');
            declarations.insert(name.to_owned(), text);
        }
        let mut originals: Vec<usize> = self.declares.iter().copied().collect();
        originals.sort_unstable();
        for id in originals {
            let declare = &parsed.declares[id];
            let mut text = String::new();
            self.rewrite(declare.text, &mut text);
            text.push('\n');
            declarations.insert(declare.name.to_owned(), text);
        }
        let mut external_text: BTreeMap<String, String> = BTreeMap::new();
        let mut foreign: Vec<usize> = self.foreign_globals.iter().copied().collect();
        foreign.sort_unstable();
        for id in foreign {
            let global = &parsed.globals[id];
            let mut text = global.declaration.clone();
            self.note_types(&text);
            text.push('\n');
            external_text.insert(global.unit_name().to_owned(), text);
        }
        let mut copied: Vec<usize> = self.copied_externals.iter().copied().collect();
        copied.sort_unstable();
        for id in copied {
            let global = &parsed.globals[id];
            let mut text = String::new();
            self.rewrite(global.text, &mut text);
            text.push('\n');
            external_text.insert(global.name.to_owned(), text);
        }

        // Named metadata names nodes of its own. The list of compile units goes with a unit only when one of its
        // functions uses a compile unit, directly or through the nodes of its debug information.
        self.close_metadata();
        let mut named_text = String::new();
        for line in &parsed.named_metadata {
            if line.starts_with("!llvm.dbg.cu") {
                let mut used = false;
                scan(line, |token| {
                    if token.class == Class::Metadata
                        && token
                            .name
                            .parse::<usize>()
                            .is_ok_and(|node| self.meta_slots.contains_key(&node))
                    {
                        used = true;
                    }
                });
                if !used {
                    continue;
                }
            }
            self.rewrite(line, &mut named_text);
            named_text.push('\n');
        }
        let mut attribute_text = String::new();
        // The groups may name more metadata (never) and more types (rarely): types come last.
        self.attribute_groups(&mut attribute_text);
        let mut metadata_text = String::new();
        self.metadata_nodes(&mut metadata_text);
        // A type is defined with the types that its body names.
        let mut cursor = 0;
        while cursor < self.type_list.len() {
            let text = parsed.types[self.type_list[cursor]].text;
            self.note_types(text);
            cursor += 1;
        }
        let mut types = self.type_list.clone();
        types.sort_unstable();

        let mut out = String::with_capacity(
            function_text.len() + global_text.len() + const_text.len() + 4096,
        );
        for line in &parsed.header {
            out.push_str(line);
            out.push('\n');
        }
        for id in types {
            out.push_str(parsed.types[id].text);
            out.push('\n');
        }
        out.push_str(&const_text);
        out.push_str(&global_text);
        for text in external_text.values() {
            out.push_str(text);
        }
        for text in declarations.values() {
            out.push_str(text);
        }
        out.push_str(&function_text);
        out.push_str(&attribute_text);
        out.push_str(&metadata_text);
        out.push_str(&named_text);
        out
    }
}

/// What another unit declares for `function`: its calling convention, result and parameter types with their ABI
/// attributes (`sret`, `byval`, `signext`, `noundef`), without parameter names and function attributes.
fn declaration_of(function: &Function<'_>) -> Declaration {
    let head = &function.text[..function.head_end];
    let (_, _, _, start) = linkage_words(head, "define ".len(), 0, false)
        .expect("the linkage of a parsed function head is valid");
    let name = first_symbol(head).expect("a parsed function head has a name");
    let result = head[start..name.start].trim();
    let rest = &head[name.end..];
    let open = rest.find('(').unwrap_or(0);
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    let mut parameters: Vec<&str> = Vec::new();
    let mut piece = open + 1;
    let mut cursor = open;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => {
                cursor = string_end(bytes, cursor);
                continue;
            }
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => {
                depth -= 1;
                if depth == 0 {
                    parameters.push(&rest[piece..cursor]);
                    break;
                }
            }
            b',' if depth == 1 => {
                parameters.push(&rest[piece..cursor]);
                piece = cursor + 1;
            }
            _ => {}
        }
        cursor += 1;
    }
    let mut list = String::from("(");
    let mut first = true;
    for parameter in parameters {
        let parameter = parameter.trim();
        if parameter.is_empty() {
            continue;
        }
        let kept = match parameter.rsplit_once(' ') {
            Some((before, last)) if last.starts_with('%') => before.trim_end(),
            _ => parameter,
        };
        if !first {
            list.push_str(", ");
        }
        first = false;
        list.push_str(kept);
    }
    list.push(')');
    let mut before = String::from("declare ");
    if function.linkage.local || function.linkage.hidden {
        before.push_str("hidden ");
    }
    before.push_str(result);
    before.push(' ');
    Declaration {
        before,
        after: list,
    }
}

/// Cuts `input.ir` into units, in a fixed order: the buckets `g.0` to `g.f`, then the modules by name. A unit that
/// would hold nothing is not made.
pub fn split(input: &SplitInput<'_>) -> Result<Vec<Unit>, SplitError> {
    let parsed = parse(input.ir)?;
    let modules: FastSet<&str> = input.modules.iter().copied().collect();
    let plan = plan(&parsed, &modules)?;

    let mut members: BTreeMap<Home<'_>, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (id, _) in parsed.functions.iter().enumerate() {
        // A function that another one defines is not defined again.
        if plan.canonical[id] != id {
            continue;
        }
        members.entry(plan.homes[id]).or_default().0.push(id);
    }
    for (id, global) in parsed.globals.iter().enumerate() {
        if global.form == GlobalForm::Single {
            members
                .entry(Home::Bucket(bucket(global.unit_name())))
                .or_default()
                .1
                .push(id);
        }
    }
    let mut homes = Homes {
        functions: vec![0; parsed.functions.len()],
        globals: vec![0; parsed.globals.len()],
    };
    for (unit, (functions, globals)) in members.values().enumerate() {
        for &id in functions {
            homes.functions[id] = unit;
        }
        for &id in globals {
            homes.globals[id] = unit;
        }
    }
    for id in 0..parsed.functions.len() {
        homes.functions[id] = homes.functions[plan.canonical[id]];
    }
    let mut units = Vec::with_capacity(members.len());
    for (unit, (home, (functions, globals))) in members.iter().enumerate() {
        let builder = Builder::new(&parsed, &plan.renamed, None, Some((&homes, unit)));
        let text = builder.unit(functions, globals);
        let name = match home {
            Home::Bucket(bucket) => format!("g.{bucket:x}"),
            Home::Module(module) => format!("m.{module}"),
        };
        let mut localize: Vec<String> = functions
            .iter()
            .filter(|&&id| parsed.functions[id].linkage.word.is_some())
            .map(|&id| function_name(&parsed, &plan.renamed, None, id).to_owned())
            .chain(
                globals
                    .iter()
                    .filter(|&&id| parsed.globals[id].linkage.is_some())
                    .map(|&id| parsed.globals[id].unit_name().to_owned()),
            )
            .collect();
        localize.sort_unstable();
        units.push(Unit {
            name,
            text,
            localize,
        });
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"; Tsuzuri - deterministic LLVM IR
source_filename = "tsuzuri"
%tz.string = type { ptr, i64 }
%"tz.union.Main.Color" = type i8
%"tz.record.Main.Pair" = type { i64, %"tz.union.Main.Color" }
%tz.closure = type { ptr, ptr }
%tz.closure.desc = type { ptr, ptr, ptr }

declare void @llvm.trap()
declare i32 @putchar(i32)

@tsuzuri_trap_hooks = weak hidden global [5 x ptr] zeroinitializer, align 8
@tz.async.next_id = internal thread_local global i64 0, align 8
@tz.literal.7 = private unnamed_addr constant [2 x i16] [i16 97, i16 98]
@tz.table.0 = internal unnamed_addr constant [4 x i32] [
  i32 1, i32 2,
  i32 3, i32 4
]
@tz.desc.Main.f.0 = internal unnamed_addr constant %tz.closure.desc { ptr @tz.apply.Main.f.0, ptr null, ptr null }
@tz.desc.$lambda.12.0 = internal unnamed_addr constant %tz.closure.desc { ptr @tz.apply.$lambda.12.0, ptr null, ptr null }

define internal i64 @tz.fn.Helper.twice(i64 noundef %arg0) nounwind {
entry:
  %v0 = add i64 %arg0, %arg0
  ret i64 %v0
}

define internal i64 @tz.fn.Main.f(i64 noundef %arg0) nounwind {
entry:
  %v0 = call i64 @tz.fn.Helper.twice(i64 %arg0)
  %v1 = call i64 @tz.fn.$lambda.12(i64 %v0)
  %v2 = load i64, ptr @tz.async.next_id
  %v3 = getelementptr [2 x i16], ptr @tz.literal.7, i32 0, i32 1
  %v4 = insertvalue %tz.closure zeroinitializer, ptr @tz.desc.$lambda.12.0, 0
  %v5 = alloca %"tz.record.Main.Pair", align 16
  call void @llvm.trap()
  unreachable
}

define internal %tz.closure @tz.apply.Main.f.0(i64 %argument, ptr %env, i1 %borrow) nounwind {
entry:
  %v0 = insertvalue %tz.closure zeroinitializer, ptr @tz.desc.Main.f.0, 0
  ret %tz.closure %v0
}

define internal i64 @tz.fn.$lambda.12(i64 %arg0) nounwind {
entry:
  %v0 = call i64 @tz.fn.Helper.twice(i64 %arg0)
  ret i64 %v0
}

define internal i64 @tz.apply.$lambda.12.0(i64 %argument, ptr %env, i1 %borrow) nounwind {
entry:
  %v0 = call i64 @tz.fn.$lambda.12(i64 %argument)
  ret i64 %v0
}

define weak ptr @tsuzuri_alloc(i64 %size) nounwind {
entry:
  %v0 = load [5 x ptr], ptr @tsuzuri_trap_hooks
  ret ptr null
}

define i32 @main() {
entry:
  %v0 = call i64 @tz.fn.Main.f(i64 1)
  ret i32 0
}
"#;

    fn modules() -> &'static [&'static str] {
        &["Main", "Helper"]
    }

    fn split_text(ir: &str) -> Vec<Unit> {
        split(&SplitInput {
            ir,
            modules: modules(),
        })
        .expect("the IR can be split")
    }

    fn unit<'a>(units: &'a [Unit], name: &str) -> &'a Unit {
        units
            .iter()
            .find(|unit| unit.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "no unit {name} in {:?}",
                    units.iter().map(|unit| &unit.name).collect::<Vec<_>>()
                )
            })
    }

    /// What is wrong with a unit as a module of its own: a name, a type, an attribute group, or a metadata node
    /// that it uses and does not define or declare.
    fn problems(unit: &Unit, all_types: &FastSet<String>) -> Vec<String> {
        let parsed = match parse(&unit.text) {
            Ok(parsed) => parsed,
            Err(error) => return vec![format!("{}: not parsed: {error}", unit.name)],
        };
        let mut problems = Vec::new();
        let mut check = |text: &str| {
            scan(text, |token| match token.class {
                Class::Symbol => {
                    if !parsed.symbols.contains_key(token.name) {
                        problems.push(format!("{}: @{} is not defined", unit.name, token.name));
                    }
                }
                Class::Local => {
                    if all_types.contains(token.name) && !parsed.type_ids.contains_key(token.name) {
                        problems.push(format!("{}: %{} is not defined", unit.name, token.name));
                    }
                }
                Class::Attribute => {
                    let group: usize = token.name.parse().unwrap();
                    if parsed.attributes.get(group).copied().flatten().is_none() {
                        problems.push(format!("{}: #{group} is not defined", unit.name));
                    }
                }
                Class::Metadata => {
                    let node: usize = token.name.parse().unwrap();
                    if parsed.metadata.get(node).copied().flatten().is_none() {
                        problems.push(format!("{}: !{node} is not defined", unit.name));
                    }
                }
            });
        };
        for function in &parsed.functions {
            check(function.text);
        }
        for global in &parsed.globals {
            check(global.text);
        }
        for declare in &parsed.declares {
            check(declare.text);
        }
        for node in parsed.metadata.iter().flatten() {
            check(node);
        }
        for line in &parsed.named_metadata {
            check(line);
        }
        problems
    }

    fn all_types(ir: &str) -> FastSet<String> {
        parse(ir)
            .expect("the IR parses")
            .types
            .iter()
            .map(|definition| definition.name.to_owned())
            .collect()
    }

    /// The units together define each name once, and each unit is a complete module.
    fn check(ir: &str, modules: &[&str], units: &[Unit]) -> Vec<String> {
        let types = all_types(ir);
        let mut defined: BTreeMap<String, String> = BTreeMap::new();
        let mut found = Vec::new();
        for unit in units {
            found.extend(problems(unit, &types));
            let Ok(parsed) = parse(&unit.text) else {
                continue;
            };
            let owners = parsed.functions.iter().map(|function| function.name).chain(
                parsed
                    .globals
                    .iter()
                    .filter(|global| global.form == GlobalForm::Single)
                    .map(|global| global.name),
            );
            for name in owners {
                if let Some(previous) = defined.insert(name.to_owned(), unit.name.clone()) {
                    found.push(format!(
                        "{name} is defined in {previous} and in {}",
                        unit.name
                    ));
                }
            }
        }
        let original = parse(ir).unwrap();
        let modules: FastSet<&str> = modules.iter().copied().collect();
        let plan = plan(&original, &modules).unwrap();
        let duplicates = (0..original.functions.len())
            .filter(|id| plan.canonical[*id] != *id)
            .count();
        let count = original.functions.len() - duplicates
            + original
                .globals
                .iter()
                .filter(|global| global.form == GlobalForm::Single)
                .count();
        if defined.len() != count {
            found.push(format!(
                "{} names are defined, the text has {count}: every function and variable has one owner",
                defined.len()
            ));
        }
        found
    }

    fn assert_self_contained(ir: &str, units: &[Unit]) {
        let found = check(ir, modules(), units);
        assert!(found.is_empty(), "{}", found.join("\n"));
    }

    /// The names of the symbols that `text` mentions.
    fn mentioned(text: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        scan(text, |token| {
            if token.class == Class::Symbol {
                names.insert(token.name.to_owned());
            }
        });
        names
    }

    /// What the units make of the definitions that were `internal` or `private` in `ir` and that a unit makes `hidden`: a
    /// hidden definition is a global symbol of the object, where a host object or a library may define the same name,
    /// and a link fails on two definitions of one. So none of them keeps a name that does not start with `tz.` (the
    /// emitter's own, which no C name has) in any unit, and the units define the `tz.rt.` names of exactly those and
    /// nothing else. Returns how many definitions have a `tz.rt.` name.
    fn assert_reserved_names(ir: &str, units: &[Unit]) -> usize {
        let original = parse(ir).expect("the IR parses");
        let mut generic: BTreeSet<&str> = original
            .functions
            .iter()
            .filter(|function| function.linkage.word.is_some())
            .map(|function| function.name)
            .collect();
        generic.extend(
            original
                .globals
                .iter()
                .filter(|global| global.form == GlobalForm::Single && global.linkage.is_some())
                .map(|global| global.name),
        );
        generic.retain(|name| !name.starts_with("tz."));
        let mut promoted = 0;
        for unit in units {
            let mentioned = mentioned(&unit.text);
            for name in &generic {
                assert!(
                    !mentioned.contains(*name),
                    "{} names @{name}, which was internal",
                    unit.name
                );
            }
            let parsed = parse(&unit.text).expect("a unit parses");
            let defined = parsed.functions.iter().map(|function| function.name).chain(
                parsed
                    .globals
                    .iter()
                    .filter(|global| global.form == GlobalForm::Single)
                    .map(|global| global.name),
            );
            for name in defined {
                if let Some(origin) = name.strip_prefix("tz.rt.") {
                    // A name with a serial number became `<name without serials>.h<digest>`.
                    let known = generic.contains(origin)
                        || origin.rsplit_once(".h").is_some_and(|(base, _)| {
                            generic.iter().any(|name| without_serials(name) == base)
                        });
                    assert!(
                        known,
                        "{} defines @{name}, which no internal definition @{origin} has",
                        unit.name
                    );
                    promoted += 1;
                }
            }
            for name in &unit.localize {
                assert!(
                    name.starts_with("tz."),
                    "{} makes @{name} local, which a host may define",
                    unit.name
                );
            }
        }
        promoted
    }

    /// A development aid: `PB07_CORPUS=<directory of .ll files> cargo test --lib split_corpus -- --ignored`.
    #[test]
    #[ignore]
    fn split_corpus() {
        let Some(directory) = std::env::var_os("PB07_CORPUS") else {
            return;
        };
        let output = std::env::var_os("PB07_CORPUS_OUT");
        let mut files: Vec<_> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ll"))
            .collect();
        files.sort();
        let mut failures = 0;
        for path in files {
            let ir = std::fs::read_to_string(&path).unwrap();
            let parsed = match parse(&ir) {
                Ok(parsed) => parsed,
                Err(error) => {
                    println!("{}: not split: {error}", path.display());
                    failures += 1;
                    continue;
                }
            };
            let modules: BTreeSet<String> = parsed
                .functions
                .iter()
                .filter_map(|function| {
                    let rest = function.name.strip_prefix("tz.fn.")?;
                    let first = rest.split('.').next()?;
                    (!first.starts_with('$')).then(|| first.to_owned())
                })
                .collect();
            let modules: Vec<&str> = modules.iter().map(String::as_str).collect();
            let started = std::time::Instant::now();
            let units = split(&SplitInput {
                ir: &ir,
                modules: &modules,
            })
            .unwrap();
            let elapsed = started.elapsed();
            let found = check(&ir, &modules, &units);
            println!(
                "{}: {} units, {} bytes -> {} bytes, {:?}, {} problems",
                path.file_name().unwrap().to_string_lossy(),
                units.len(),
                ir.len(),
                units.iter().map(|unit| unit.text.len()).sum::<usize>(),
                elapsed,
                found.len()
            );
            for problem in found.iter().take(5) {
                println!("    {problem}");
            }
            failures += usize::from(!found.is_empty());
            if let Some(output) = &output {
                let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
                let directory = std::path::Path::new(output).join(&stem);
                std::fs::create_dir_all(&directory).unwrap();
                for (index, unit) in units.iter().enumerate() {
                    std::fs::write(directory.join(format!("u{index:05}.ll")), &unit.text).unwrap();
                }
            }
        }
        assert_eq!(failures, 0);
    }

    /// Every program of `examples/`, `benchmarks/` and `tests/fixtures/` that is one directory of sources, built for
    /// `-O0` with and without trap information (stable numbers), is cut into units that define every function and
    /// variable once and name nothing that no unit defines or declares.
    #[test]
    fn splits_the_programs_of_the_repository() {
        use crate::llvm::TrapNumbering;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut programs = 0;
        let mut units_made = 0;
        let mut promoted = 0;
        for parent in ["examples", "benchmarks", "tests/fixtures"] {
            let mut directories: Vec<_> = std::fs::read_dir(root.join(parent))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.is_dir())
                .collect();
            directories.sort();
            for directory in directories {
                let mut files = Vec::new();
                let mut flat = true;
                for entry in std::fs::read_dir(&directory).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        flat = false;
                    } else if path.extension().is_some_and(|extension| extension == "tz") {
                        let name = path.file_name().unwrap().to_string_lossy().into_owned();
                        files.push((name, std::fs::read_to_string(&path).unwrap()));
                    }
                }
                files.sort();
                if !flat || !files.iter().any(|(name, _)| name == "Main.tz") {
                    continue;
                }
                let sources: Vec<(&str, &str)> = files
                    .iter()
                    .map(|(name, text)| (name.as_str(), text.as_str()))
                    .collect();
                // A directory that does not check as a program on its own (an example of an error) is left out.
                let Ok(module) = crate::analyze_modules(&sources) else {
                    continue;
                };
                // A program with an entry is built for an executable with and without trap information; the others
                // are libraries (an object of exports) and are cut without.
                let numberings: &[Option<TrapNumbering>] = if module.entry.is_some() {
                    &[None, Some(TrapNumbering::Stable)]
                } else {
                    &[None]
                };
                for &numbering in numberings {
                    let (ir, modules, _) = executable_ir_with(&sources, numbering);
                    let modules: Vec<&str> = modules.iter().map(String::as_str).collect();
                    let units = split(&SplitInput {
                        ir: &ir,
                        modules: &modules,
                    })
                    .unwrap_or_else(|error| panic!("{}: {error}", directory.display()));
                    let found = check(&ir, &modules, &units);
                    assert!(
                        found.is_empty(),
                        "{} ({numbering:?}): {}",
                        directory.display(),
                        found.join("\n")
                    );
                    let again = split(&SplitInput {
                        ir: &ir,
                        modules: &modules,
                    })
                    .unwrap();
                    assert_eq!(units, again, "{} is deterministic", directory.display());
                    units_made += units.len();
                    promoted += assert_reserved_names(&ir, &units);
                }
                programs += 1;
            }
        }
        assert!(programs > 60, "{programs} programs");
        assert!(units_made > 1000, "{units_made} units");
        // The builds with trap information embed the numeric runtime of a program that needs it, whose helpers
        // (`decode`, `power`, ...) have generic names.
        assert!(promoted >= 19, "{promoted} promoted generic names");
    }

    #[test]
    fn parses_every_top_level_form() {
        let ir = format!(
            "{}define internal void @tz.glue() {{\nentry:\n  ret void\n}}\n",
            SAMPLE.trim_end()
        );
        let parsed = parse(&ir).unwrap();
        assert_eq!(parsed.header, ["source_filename = \"tsuzuri\""]);
        assert_eq!(parsed.types.len(), 5);
        assert!(parsed.type_ids.contains_key("tz.union.Main.Color"));
        assert_eq!(parsed.declares.len(), 2);
        assert_eq!(parsed.globals.len(), 6);
        let form = |name: &str| match parsed.symbols[name] {
            Symbol::Global(id) => &parsed.globals[id].form,
            other => panic!("{name} is {other:?}"),
        };
        assert_eq!(form("tsuzuri_trap_hooks"), &GlobalForm::Single);
        assert_eq!(form("tz.async.next_id"), &GlobalForm::Single);
        assert_eq!(form("tz.literal.7"), &GlobalForm::Copyable);
        assert_eq!(form("tz.table.0"), &GlobalForm::Copyable);
        assert_eq!(form("tz.desc.Main.f.0"), &GlobalForm::Copyable);
        let Symbol::Global(id) = parsed.symbols["tz.table.0"] else {
            unreachable!()
        };
        assert!(
            parsed.globals[id].text.ends_with("\n]"),
            "a constant of several lines is one entity"
        );
        assert_eq!(parsed.functions.len(), 8);
        assert_eq!(parsed.functions.last().unwrap().name, "tz.glue");
        assert!(parsed.functions[0].linkage.local);
        assert!(parsed.functions[5].linkage.weak);
    }

    #[test]
    fn parses_attributes_metadata_and_named_metadata() {
        let ir = "attributes #3 = { nounwind }\n!0 = !{i32 1, !\"wchar_size\", i32 4}\n!1 = distinct !{!1, !0}\n!llvm.module.flags = !{!0}\ndefine void @f() #3 !dbg !1 {\nentry:\n  ret void\n}\n";
        let parsed = parse(ir).unwrap();
        assert_eq!(parsed.attributes[3], Some("{ nounwind }"));
        assert_eq!(parsed.metadata[1], Some("distinct !{!1, !0}"));
        assert_eq!(parsed.named_metadata, ["!llvm.module.flags = !{!0}"]);
    }

    #[test]
    fn rejects_unknown_top_level_line() {
        for (text, expected) in [
            ("module asm \"nop\"\n", "unknown"),
            ("$c = comdat any\n", "unknown"),
            ("uselistorder ptr @f, { 1, 0 }\n", "unknown"),
            ("@a = alias i32, ptr @b\n", "alias"),
            ("@a = ifunc void (), ptr @r\n", "alias"),
            (
                "@llvm.used = appending global [1 x ptr] [ptr @f]\n",
                "linkage",
            ),
            ("@x = linkonce_odr global i32 0\n", "linkage"),
            ("define void @f() {\nentry:\n  ret void\n", "closing brace"),
            (
                "define void @f()\n{\nentry:\n  ret void\n}\n",
                "function head",
            ),
            ("@x = global i32 [\n", "unbalanced"),
        ] {
            let error = parse(text).expect_err(text);
            assert!(
                error.to_string().contains(expected),
                "{text}: {error} should mention {expected}"
            );
        }
        let error = split(&SplitInput {
            ir: "source_filename = \"x\"\nmodule asm \"nop\"\n",
            modules: &[],
        })
        .unwrap_err();
        assert_eq!(
            error,
            SplitError::UnknownLine {
                line: 2,
                text: "module asm \"nop\"".into()
            }
        );
    }

    #[test]
    fn skips_quoted_strings_when_scanning() {
        let text = "@\"quoted name\" = c\"@x %y !12 #3 \\22 @z\" ; @comment %c !9\n%\"tz.record.A[i64]\" !5 #7 @plain, !\"@str !4\"";
        let mut tokens = Vec::new();
        scan(text, |token| {
            tokens.push((token.class, token.name.to_owned()))
        });
        assert_eq!(
            tokens,
            [
                (Class::Symbol, "quoted name".to_owned()),
                (Class::Local, "tz.record.A[i64]".to_owned()),
                (Class::Metadata, "5".to_owned()),
                (Class::Attribute, "7".to_owned()),
                (Class::Symbol, "plain".to_owned()),
            ]
        );
    }

    #[test]
    fn classifies_serial_numbers_by_component() {
        let modules: FastSet<&str> = ["Main", "Geometry.Shapes"].into_iter().collect();
        fn stable<'a>(name: &'a str, modules: &FastSet<&str>) -> Naming<'a> {
            classify(name, modules)
        }
        let stable = |name| stable(name, &modules);
        assert_eq!(
            stable("tz.fn.Main.divide"),
            Naming::Stable {
                module: Some("Main")
            }
        );
        assert_eq!(
            stable("tz.apply.Main.divide.0"),
            Naming::Stable {
                module: Some("Main")
            }
        );
        assert_eq!(
            stable("tz.env.drop.Main.divide.1"),
            Naming::Stable {
                module: Some("Main")
            }
        );
        assert_eq!(
            stable("tz.fn.Geometry.Shapes.area"),
            Naming::Stable {
                module: Some("Geometry.Shapes")
            }
        );
        assert_eq!(
            stable("tz.fn.Main.$entry"),
            Naming::Stable {
                module: Some("Main")
            }
        );
        assert_eq!(stable("tz.fn.Other.f"), Naming::Stable { module: None });
        assert_eq!(stable("tz.string.concat"), Naming::Stable { module: None });
        assert_eq!(
            stable("tz.builtin.to_string.i64"),
            Naming::Stable { module: None }
        );
        assert_eq!(
            stable("tz.clone.step.rec.Result.Result[Main.E,string]"),
            Naming::Stable { module: None }
        );
        for unstable in [
            "tz.fn.$lambda.8",
            "tz.apply.$lambda.8.1",
            "tz.fn.Result.Run.$mono.2",
            "tz.apply.Result.Run.$mono.2.0",
            "tz.env.clone.Result.Bind.$mono.4.1",
            "tz.specialized.3",
            "tz.parallel.chunk.4",
            "tz.dyn.slot.Main.$instance.3.area",
            "tz.fn.$builtin.unreachable.11",
        ] {
            assert_eq!(stable(unstable), Naming::Unstable, "{unstable}");
        }
        assert_eq!(
            without_serials("tz.apply.Result.Run.$mono.2.0"),
            "tz.apply.Result.Run.$mono"
        );
        assert_eq!(
            without_serials("tz.dyn.slot.Main.$instance.3.area"),
            "tz.dyn.slot.Main.$instance.area"
        );
    }

    #[test]
    fn assigns_stable_functions_to_module_units() {
        let units = split_text(SAMPLE);
        assert_self_contained(SAMPLE, &units);
        let names: Vec<_> = units.iter().map(|unit| unit.name.as_str()).collect();
        assert!(names.contains(&"m.Main") && names.contains(&"m.Helper"));
        let buckets: Vec<_> = names.iter().filter(|name| name.starts_with("g.")).collect();
        assert!(!buckets.is_empty());
        assert!(
            names
                .iter()
                .position(|name| name.starts_with("g."))
                .unwrap()
                < names.iter().position(|name| *name == "m.Helper").unwrap(),
            "buckets come before modules: {names:?}"
        );
        let main = unit(&units, "m.Main");
        assert!(main.text.contains("define hidden i64 @tz.fn.Main.f("));
        assert!(
            main.text
                .contains("define hidden %tz.closure @tz.apply.Main.f.0(")
        );
        assert!(!main.text.contains("define internal"));
        assert!(
            main.text
                .contains("declare hidden i64 @tz.fn.Helper.twice(i64 noundef)")
        );
        // The lambda is used by `Main.f` alone: it is defined in the unit of `Main.f`, with its helper.
        for definition in [
            "define hidden i64 @tz.fn.$lambda.h",
            "define hidden i64 @tz.apply.$lambda.h",
        ] {
            assert!(
                main.text.lines().any(|line| line.starts_with(definition)),
                "{}",
                main.text
            );
        }
        let helper = unit(&units, "m.Helper");
        assert!(
            helper
                .text
                .contains("define hidden i64 @tz.fn.Helper.twice(")
        );
        assert!(!helper.text.contains("@tz.fn.Main"));
    }

    /// A generated function is defined where the functions that use it are, when they are all in one unit: a lambda of
    /// one module is in that module's unit, with the lambda it calls; a function that two modules use is in a bucket.
    #[test]
    fn generated_functions_follow_their_users() {
        let lambda = |number: u32, body: &str| {
            format!(
                "define internal i64 @tz.fn.$lambda.{number}(i64 %x) {{\nentry:\n{body}  ret i64 %r\n}}\n"
            )
        };
        let ir = format!(
            "{}{}{}{}{}{}",
            lambda(3, "  %r = add i64 %x, 3\n"),
            lambda(1, "  %r = call i64 @tz.fn.$lambda.3(i64 %x)\n"),
            lambda(2, "  %r = add i64 %x, 2\n"),
            lambda(4, "  %r = add i64 %x, 4\n"),
            "define internal i64 @tz.fn.Main.run(i64 %x) {\nentry:\n  %a = call i64 @tz.fn.$lambda.1(i64 %x)\n  %r = call i64 @tz.fn.$lambda.2(i64 %a)\n  ret i64 %r\n}\n",
            "define internal i64 @tz.fn.Helper.run(i64 %x) {\nentry:\n  %r = call i64 @tz.fn.$lambda.2(i64 %x)\n  ret i64 %r\n}\n",
        );
        let units = split_text(&ir);
        assert_self_contained(&ir, &units);
        // The unit that holds the body of a function: a declaration in another unit has none.
        let home = |number: u32| -> Vec<&str> {
            let body = format!("%r = add i64 %x, {number}\n");
            units
                .iter()
                .filter(|unit| unit.text.contains(&body))
                .map(|unit| unit.name.as_str())
                .collect()
        };
        // `$lambda.3` is called by `$lambda.1` alone, which `Main.run` alone calls: both are in `m.Main`.
        assert_eq!(home(3), ["m.Main"]);
        assert!(
            unit(&units, "m.Main")
                .text
                .contains("define hidden i64 @tz.fn.$lambda.h")
        );
        // `$lambda.2` is used by two modules, and `$lambda.4` by none: they are in buckets.
        for number in [2, 4] {
            let homes = home(number);
            assert!(
                homes.len() == 1 && homes[0].starts_with("g."),
                "$lambda.{number}: {homes:?}"
            );
        }
    }

    /// The debugger finds the function that a function value calls by cutting the index off the adapter's name
    /// (`tz.apply.<symbol>.<index>` to `tz.fn.<symbol>`): a renamed adapter keeps that.
    #[test]
    fn an_adapter_is_named_after_its_function() {
        let units = split_text(SAMPLE);
        let main = unit(&units, "m.Main");
        let symbol = main
            .text
            .lines()
            .find_map(|line| line.strip_prefix("define hidden i64 @tz.fn.$lambda."))
            .and_then(|rest| rest.split('(').next())
            .unwrap();
        assert!(symbol.starts_with('h'), "{symbol}");
        let adapter = format!("define hidden i64 @tz.apply.$lambda.{symbol}.0(");
        assert!(main.text.contains(&adapter), "{}", main.text);
    }

    /// A relocatable object of the units makes local again what was `internal`, and only that: a `weak` definition and the
    /// public `main` stay as they are.
    #[test]
    fn units_list_the_symbols_that_were_internal() {
        let units = split_text(SAMPLE);
        let listed: BTreeSet<&str> = units
            .iter()
            .flat_map(|unit| unit.localize.iter().map(String::as_str))
            .collect();
        for name in [
            "tz.fn.Main.f",
            "tz.fn.Helper.twice",
            "tz.apply.Main.f.0",
            "tz.async.next_id",
        ] {
            assert!(listed.contains(name), "{name} in {listed:?}");
        }
        assert!(
            listed
                .iter()
                .any(|name| name.starts_with("tz.fn.$lambda.h")),
            "{listed:?}"
        );
        for name in ["tsuzuri_alloc", "main", "tsuzuri_trap_hooks"] {
            assert!(!listed.contains(name), "{name} is not listed");
        }
        // Each listed name is defined, `hidden`, in the unit that lists it.
        for unit in &units {
            for name in &unit.localize {
                let definition = format!("@{name}");
                assert!(
                    unit.text.lines().any(|line| {
                        (line.starts_with("define hidden ") || line.contains(" = hidden "))
                            && line.contains(&definition)
                    }),
                    "{name} is not defined in {}",
                    unit.name
                );
            }
        }
    }

    /// What the numeric runtime that a build embeds has: internal and private definitions under generic names, among
    /// the names of the emitter, a weak helper, and an exported function.
    const GENERIC: &str = r#"; Tsuzuri - deterministic LLVM IR
source_filename = "tsuzuri"

declare i32 @putchar(i32)

@counter = internal global i64 0, align 8
@"odd name" = internal global i32 0, align 4
@scratch = private thread_local global i64 0, align 8
@tz.keep = internal global i64 0, align 8
@table = internal unnamed_addr constant [2 x i32] [i32 1, i32 2], align 4
@shared = weak hidden global i64 0, align 8
@public = global i64 0, align 8
@users = internal global [2 x ptr] [ptr @power, ptr @decode], align 8

define internal i64 @power(i64 %a) {
entry:
  %v0 = call i64 @decode(i64 %a)
  %v1 = load i64, ptr @counter
  %v2 = load i32, ptr @"odd name"
  ret i64 %v0
}

define private i64 @decode(i64 %a) {
entry:
  %v0 = load i64, ptr @scratch
  %v1 = getelementptr [2 x i32], ptr @table, i32 0, i32 1
  ret i64 %a
}

define internal void @"odd fn"() {
entry:
  %v0 = load i64, ptr @tz.keep
  ret void
}

define internal i64 @tz.fn.Main.f(i64 %arg0) {
entry:
  %v0 = call i64 @power(i64 %arg0)
  call void @"odd fn"()
  %v1 = load [2 x ptr], ptr @users
  ret i64 %v0
}

define internal i64 @tz.fn.Helper.g(i64 %arg0) {
entry:
  %v0 = call i64 @decode(i64 %arg0)
  %v1 = load i64, ptr @scratch
  %v2 = load i64, ptr @counter
  %v3 = load i64, ptr @shared
  ret i64 %v0
}

define weak hidden i64 @tz_soft_op(i64 %a) {
entry:
  ret i64 %a
}

define i64 @tz_exported(i64 %a) {
entry:
  %v0 = load i64, ptr @public
  ret i64 %a
}

define i32 @main() {
entry:
  %v0 = call i64 @tz.fn.Main.f(i64 1)
  %v1 = call i64 @tz.fn.Helper.g(i64 1)
  ret i32 0
}
"#;

    /// A hidden definition is a global symbol of the object, so a host object or a library that defines `power` or
    /// `decode` too fails the link of a program that one module had them local in. What a unit makes `hidden` gets a
    /// name of the reserved kind, in the unit that defines it, in every unit that declares or uses it, and in the list
    /// of the symbols to make local; the names that the emitter makes, and what was not internal, stay.
    #[test]
    fn promoted_generic_names_get_the_reserved_prefix() {
        let units = split_text(GENERIC);
        assert_self_contained(GENERIC, &units);
        assert_eq!(
            assert_reserved_names(GENERIC, &units),
            7,
            "three functions and four variables are promoted"
        );
        // `names` below are the names that were internal and generic: no unit mentions any.
        let names = [
            "power", "decode", "odd fn", "counter", "odd name", "scratch", "users",
        ];
        for unit in &units {
            let mentioned = mentioned(&unit.text);
            for name in names {
                assert!(!mentioned.contains(name), "{} names @{name}", unit.name);
            }
        }
        let defined = |needle: &str| {
            units
                .iter()
                .filter(|unit| unit.text.lines().any(|line| line.starts_with(needle)))
                .count()
        };
        for needle in [
            "define hidden i64 @tz.rt.power(i64 %a) {",
            "define hidden i64 @tz.rt.decode(i64 %a) {",
            "define hidden void @\"tz.rt.odd fn\"() {",
            "@tz.rt.counter = hidden global i64 0, align 8",
            "@\"tz.rt.odd name\" = hidden global i32 0, align 4",
            "@tz.rt.scratch = hidden thread_local global i64 0, align 8",
            "@tz.rt.users = hidden global [2 x ptr] [ptr @tz.rt.power, ptr @tz.rt.decode], align 8",
            // Already reserved, or not internal: as they were.
            "@tz.keep = hidden global i64 0, align 8",
            "@shared = weak hidden global i64 0, align 8",
            "@public = global i64 0, align 8",
            "define weak hidden i64 @tz_soft_op(i64 %a) {",
            "define i64 @tz_exported(i64 %a) {",
            "define i32 @main() {",
        ] {
            assert_eq!(defined(needle), 1, "{needle}");
        }
        // The units that use them declare them under the same names.
        let helper = unit(&units, "m.Helper");
        for needle in [
            "declare hidden i64 @tz.rt.decode(i64)",
            "@tz.rt.scratch = external hidden thread_local global i64, align 8",
            "@tz.rt.counter = external hidden global i64, align 8",
        ] {
            assert!(helper.text.contains(needle), "{needle}\n{}", helper.text);
        }
        let main = unit(&units, "m.Main");
        for needle in [
            "declare hidden i64 @tz.rt.power(i64)",
            "declare hidden void @\"tz.rt.odd fn\"()",
            "@tz.rt.users = external hidden global [2 x ptr], align 8",
        ] {
            assert!(main.text.contains(needle), "{needle}\n{}", main.text);
        }
        // The relocatable object makes exactly these local, under these names.
        let listed: BTreeSet<&str> = units
            .iter()
            .flat_map(|unit| unit.localize.iter().map(String::as_str))
            .collect();
        assert_eq!(
            listed,
            BTreeSet::from([
                "tz.rt.power",
                "tz.rt.decode",
                "tz.rt.odd fn",
                "tz.rt.counter",
                "tz.rt.odd name",
                "tz.rt.scratch",
                "tz.rt.users",
                "tz.keep",
                "tz.fn.Main.f",
                "tz.fn.Helper.g",
            ])
        );
        for unit in &units {
            for name in &unit.localize {
                assert!(
                    unit.text.lines().any(|line| {
                        (line.starts_with("define hidden ") || line.contains(" = hidden "))
                            && first_symbol(line).is_some_and(|token| token.name == name)
                    }),
                    "{name} is not defined hidden in {}",
                    unit.name
                );
            }
        }
    }

    /// The new name of a definition depends on that name alone, so the text of the unit that defines it does not change
    /// with the functions that use it, wherever they are.
    #[test]
    fn promoted_names_do_not_depend_on_the_users() {
        let other = GENERIC.replace(
            "  %v0 = call i64 @decode(i64 %arg0)\n  %v1 = load i64, ptr @scratch\n  %v2 = load i64, ptr @counter\n  %v3 = load i64, ptr @shared\n  ret i64 %v0\n",
            "  %v0 = load i64, ptr @shared\n  ret i64 %v0\n",
        );
        assert_ne!(other, GENERIC);
        let before = split_text(GENERIC);
        let after = split_text(&other);
        assert_self_contained(&other, &after);
        let buckets = |units: &[Unit]| -> Vec<Unit> {
            units
                .iter()
                .filter(|unit| unit.name.starts_with("g."))
                .cloned()
                .collect()
        };
        assert!(!buckets(&before).is_empty());
        assert_eq!(buckets(&before), buckets(&after));
        assert_ne!(unit(&before, "m.Helper"), unit(&after, "m.Helper"));
    }

    /// A promoted name that the text uses already is not made over: the text is not split.
    #[test]
    fn a_taken_reserved_name_is_not_split() {
        let function = |name: &str| {
            format!("define internal i64 @{name}(i64 %a) {{\nentry:\n  ret i64 %a\n}}\n\n")
        };
        let split_of = |ir: String| {
            split(&SplitInput {
                ir: &ir,
                modules: modules(),
            })
        };
        let taken = SplitError::Collision {
            name: "tz.rt.power".to_owned(),
        };
        assert_eq!(
            split_of(format!("{}{}", function("power"), function("tz.rt.power"))),
            Err(taken.clone())
        );
        assert_eq!(
            split_of(format!(
                "@tz.rt.power = global i64 0\n\n{}",
                function("power")
            )),
            Err(taken.clone())
        );
        assert_eq!(
            split_of(format!(
                "@power = internal global i64 0\n@tz.rt.power = global i64 0\n\n{}",
                function("tz.fn.Main.f")
            )),
            Err(taken)
        );
        // Without the clash the same text is split, and a variable with a name of its own is promoted as well.
        assert!(split_of(format!("{}{}", function("power"), function("tz.rt.other"))).is_ok());
        let units = split_of(String::from(
            "@x = internal global i64 0\n@y = global i64 0\n",
        ))
        .unwrap();
        assert!(
            units
                .iter()
                .any(|unit| unit.text.contains("@tz.rt.x = hidden global i64 0\n"))
        );
        assert!(
            units
                .iter()
                .any(|unit| unit.text.contains("@y = global i64 0\n"))
        );
    }

    /// A generic name that holds a serial number is named by the hash of its text, with the reserved prefix in front, as
    /// the generated functions are named.
    #[test]
    fn serial_numbers_of_generic_names_are_replaced_and_prefixed() {
        let program = |serial: u32| {
            format!(
                "define internal i64 @helper.{serial}(i64 %a) {{\nentry:\n  ret i64 %a\n}}\n\ndefine internal i64 @tz.fn.Main.f(i64 %arg0) {{\nentry:\n  %v0 = call i64 @helper.{serial}(i64 %arg0)\n  ret i64 %v0\n}}\n"
            )
        };
        let first = split_text(&program(12));
        let second = split_text(&program(97));
        assert_eq!(first, second, "the name does not hold the serial number");
        assert_self_contained(&program(12), &first);
        assert_eq!(assert_reserved_names(&program(12), &first), 1);
        let main = unit(&first, "m.Main");
        assert!(
            main.text.contains("define hidden i64 @tz.rt.helper.h"),
            "{}",
            main.text
        );
        assert!(
            main.localize
                .iter()
                .any(|name| name.starts_with("tz.rt.helper.h"))
        );
        assert!(
            !mentioned(&main.text)
                .iter()
                .any(|name| name.starts_with("helper"))
        );
    }

    #[test]
    fn copies_constants_and_declares_variables() {
        let units = split_text(SAMPLE);
        let main = unit(&units, "m.Main");
        // The literal and the closure descriptor of the lambda are private copies, numbered by first use.
        assert!(
            main.text
                .contains("@tz.unit.0 = private unnamed_addr constant [2 x i16] [i16 97, i16 98]")
        );
        assert!(
            main.text.contains(
                "= private unnamed_addr constant %tz.closure.desc { ptr @tz.apply.$lambda."
            )
        );
        assert!(!main.text.contains("@tz.literal"));
        // The thread-local variable has one owner and is declared `external hidden thread_local` here.
        assert!(
            main.text
                .contains("@tz.async.next_id = external hidden thread_local global i64, align 8")
        );
        let owners: Vec<_> = units
            .iter()
            .filter(|unit| {
                unit.text
                    .contains("@tz.async.next_id = hidden thread_local global i64 0, align 8")
            })
            .collect();
        assert_eq!(owners.len(), 1);
        // A weak definition has one owner as well; the others declare it.
        let weak: Vec<_> = units
            .iter()
            .filter(|unit| unit.text.contains("define weak ptr @tsuzuri_alloc("))
            .collect();
        assert_eq!(weak.len(), 1);
        let hooks: Vec<_> = units
            .iter()
            .filter(|unit| {
                unit.text.contains(
                    "@tsuzuri_trap_hooks = weak hidden global [5 x ptr] zeroinitializer, align 8",
                )
            })
            .collect();
        assert_eq!(hooks.len(), 1);
        // The multi-line table is copied whole where it is used (nowhere here).
        assert!(units.iter().all(|unit| !unit.text.contains("tz.table")));
    }

    #[test]
    fn keeps_type_definition_order() {
        let units = split_text(SAMPLE);
        let main = unit(&units, "m.Main");
        let position = |needle: &str| main.text.find(needle).unwrap_or_else(|| panic!("{needle}"));
        // The alias of the enum comes before the record that has it as a field, as it does in the whole program.
        assert!(
            position("%\"tz.union.Main.Color\" = type i8")
                < position("%\"tz.record.Main.Pair\" = type")
        );
        assert!(position("%\"tz.record.Main.Pair\" = type") < position("%tz.closure = type"));
        assert!(
            !main.text.contains("%tz.string"),
            "a unit holds only the types it uses:\n{}",
            main.text
        );
        let helper = unit(&units, "m.Helper");
        assert!(!helper.text.contains("%tz."), "{}", helper.text);
    }

    #[test]
    fn declaration_keeps_abi_attributes() {
        let ir = "%T = type { i64, i64 }\ndefine internal fastcc void @f(ptr noalias nonnull sret(%T) align 8 %out, i8 signext %x, ptr byval(%T) align 8 %p, ...) local_unnamed_addr nounwind #4 !dbg !5 {\nentry:\n  ret void\n}\ndefine internal noundef i1 @g(i32 %a) {\nentry:\n  ret i1 true\n}\n";
        let parsed = parse(ir).unwrap();
        let declaration = declaration_of(&parsed.functions[0]);
        assert_eq!(declaration.before, "declare hidden fastcc void ");
        assert_eq!(
            declaration.after,
            "(ptr noalias nonnull sret(%T) align 8, i8 signext, ptr byval(%T) align 8, ...)"
        );
        let declaration = declaration_of(&parsed.functions[1]);
        assert_eq!(declaration.before, "declare hidden noundef i1 ");
        assert_eq!(declaration.after, "(i32)");
    }

    /// `llvm.lifetime.*` calls and `musttail` calls (PM09) go through the split as they are: each unit declares the
    /// intrinsics it calls, and a `musttail` call stays directly before its `ret`.
    #[test]
    fn keeps_lifetime_intrinsics_and_musttail_calls() {
        let ir = r#"source_filename = "tsuzuri"
declare void @llvm.lifetime.start.p0(i64 immarg, ptr nocapture)
declare void @llvm.lifetime.end.p0(i64 immarg, ptr nocapture)

define internal i64 @tz.fn.Helper.count(i64 noundef %arg0, i64 noundef %arg1) nounwind {
entry:
  %slot = alloca i64, align 8
  call void @llvm.lifetime.start.p0(i64 8, ptr %slot)
  store i64 %arg0, ptr %slot, align 8
  %v0 = load i64, ptr %slot, align 8
  call void @llvm.lifetime.end.p0(i64 8, ptr %slot)
  %v1 = icmp eq i64 %arg1, 0
  br i1 %v1, label %done, label %again
again:
  %v2 = sub i64 %arg1, 1
  %v3 = musttail call i64 @tz.fn.Helper.count(i64 %v0, i64 %v2)
  ret i64 %v3
done:
  ret i64 %v0
}

define internal i64 @tz.fn.Main.run(i64 noundef %arg0, i64 noundef %arg1) nounwind {
entry:
  %slot = alloca i64, align 8
  call void @llvm.lifetime.start.p0(i64 8, ptr %slot)
  call void @llvm.lifetime.end.p0(i64 8, ptr %slot)
  %v0 = musttail call i64 @tz.fn.Helper.count(i64 %arg0, i64 %arg1)
  ret i64 %v0
}

define i32 @main() {
entry:
  %v0 = call i64 @tz.fn.Main.run(i64 1, i64 3)
  ret i32 0
}
"#;
        let units = split_text(ir);
        assert_self_contained(ir, &units);
        let mut tail_calls = 0;
        for name in ["m.Main", "m.Helper"] {
            let text = &unit(&units, name).text;
            for intrinsic in ["start", "end"] {
                let declaration = format!(
                    "declare void @llvm.lifetime.{intrinsic}.p0(i64 immarg, ptr nocapture)"
                );
                assert_eq!(text.matches(&declaration).count(), 1, "{name}:\n{text}");
            }
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if line.contains("musttail call") {
                    tail_calls += 1;
                    assert!(
                        lines[index + 1].trim_start().starts_with("ret "),
                        "{name}: {line}\n{text}"
                    );
                }
            }
        }
        assert_eq!(tail_calls, 2);
    }

    #[test]
    fn renumbers_constants_attributes_and_metadata() {
        let ir = "source_filename = \"tsuzuri\"\n@tz.literal.40 = private unnamed_addr constant [1 x i8] c\"@\"\n@tz.literal.41 = private unnamed_addr constant [1 x i8] c\"!\"\nattributes #5 = { nounwind }\nattributes #9 = { cold }\n!7 = !{i32 1, !\"a\", i32 2}\n!8 = distinct !{!8, !7}\n!9 = !{!8}\n!11 = !{}\n!llvm.module.flags = !{!7}\n!llvm.dbg.cu = !{!11}\ndefine internal void @tz.fn.Main.f() #9 {\nentry:\n  %a = getelementptr i8, ptr @tz.literal.41, i32 0\n  %b = getelementptr i8, ptr @tz.literal.40, i32 0\n  call void @tz.fn.Main.f() #5, !llvm.loop !8\n  ret void\n}\n";
        let units = split(&SplitInput {
            ir,
            modules: &["Main"],
        })
        .unwrap();
        assert_eq!(units.len(), 1);
        let text = &units[0].text;
        // The literal that appears first gets the first number; the loop node is self-referential.
        assert!(
            text.contains("@tz.unit.0 = private unnamed_addr constant [1 x i8] c\"!\""),
            "{text}"
        );
        assert!(text.contains("@tz.unit.1 = private unnamed_addr constant [1 x i8] c\"@\""));
        assert!(text.contains("define hidden void @tz.fn.Main.f() #0 {"));
        assert!(text.contains("call void @tz.fn.Main.f() #1, !llvm.loop !0"));
        assert!(text.contains("attributes #0 = { cold }\nattributes #1 = { nounwind }\n"));
        assert!(text.contains("!0 = distinct !{!0, !1}\n!1 = !{i32 1, !\"a\", i32 2}\n"));
        assert!(text.contains("!llvm.module.flags = !{!1}"));
        // Nothing here uses debug metadata, so the debug-info unit is left out.
        assert!(!text.contains("llvm.dbg.cu"), "{text}");
        assert_self_contained(ir, &units);
    }

    #[test]
    fn split_is_deterministic_and_self_contained() {
        let first = split_text(SAMPLE);
        let second = split_text(SAMPLE);
        assert_eq!(first, second);
        assert_self_contained(SAMPLE, &first);
    }

    /// The text of a unit changes with its functions, not with serial numbers elsewhere.
    #[test]
    fn renames_counter_names_by_content() {
        let build = |lambda: &str, mono: &str| {
            format!(
                "define internal i64 @tz.fn.$lambda.{lambda}(i64 %a) {{\nentry:\n  %r = call i64 @tz.fn.Util.id.$mono.{mono}(i64 %a)\n  ret i64 %r\n}}\n\
                 define internal i64 @tz.fn.Util.id.$mono.{mono}(i64 %a) {{\nentry:\n  ret i64 %a\n}}\n\
                 define internal i64 @tz.fn.Main.run(i64 %a) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{lambda}(i64 %a)\n  ret i64 %r\n}}\n"
            )
        };
        let one = split(&SplitInput {
            ir: &build("10", "3"),
            modules: &["Main"],
        })
        .unwrap();
        let two = split(&SplitInput {
            ir: &build("77", "9"),
            modules: &["Main"],
        })
        .unwrap();
        assert_eq!(one, two, "only the serial numbers differ");
        let main = unit(&one, "m.Main");
        assert!(main.text.contains("@tz.fn.$lambda.h"), "{}", main.text);
    }

    #[test]
    fn stable_names_ignore_unrelated_counters() {
        let module = |extra: &str, lambda: u32| {
            format!(
                "{extra}define internal i64 @tz.fn.$lambda.{lambda}(i64 %a) {{\nentry:\n  %r = add i64 %a, 1\n  ret i64 %r\n}}\n\
                 define internal i64 @tz.fn.Main.run(i64 %a) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{lambda}(i64 %a)\n  ret i64 %r\n}}\n"
            )
        };
        let plain = split(&SplitInput {
            ir: &module("", 4),
            modules: &["Main"],
        })
        .unwrap();
        // Another module gains a lambda before ours: every serial after it shifts by one.
        let shifted = split(&SplitInput {
            ir: &module(
                "define internal i64 @tz.fn.$lambda.4(i64 %a) {\nentry:\n  ret i64 %a\n}\n",
                5,
            ),
            modules: &["Main"],
        })
        .unwrap();
        assert_eq!(unit(&plain, "m.Main"), unit(&shifted, "m.Main"));
    }

    #[test]
    fn names_recursive_components_deterministically() {
        let build = |a: u32, b: u32| {
            format!(
                "define internal i64 @tz.fn.$lambda.{a}(i64 %x) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{b}(i64 %x)\n  ret i64 %r\n}}\n\
                 define internal i64 @tz.fn.$lambda.{b}(i64 %x) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{a}(i64 %x)\n  %q = add i64 %r, 1\n  ret i64 %q\n}}\n\
                 define internal i64 @tz.fn.Main.run(i64 %x) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{a}(i64 %x)\n  ret i64 %r\n}}\n"
            )
        };
        let first = split(&SplitInput {
            ir: &build(3, 4),
            modules: &["Main"],
        })
        .unwrap();
        let second = split(&SplitInput {
            ir: &build(30, 41),
            modules: &["Main"],
        })
        .unwrap();
        assert_eq!(first, second);
        let all: String = first.iter().map(|unit| unit.text.as_str()).collect();
        let names: BTreeSet<_> = all
            .lines()
            .filter_map(|line| line.strip_prefix("define hidden i64 @"))
            .map(|rest| rest.split('(').next().unwrap())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(
            names.iter().any(|name| name.ends_with(".0"))
                && names.iter().any(|name| name.ends_with(".1"))
        );
    }

    /// The IR that the driver hands to Clang for a project at `-O0`, and the modules of its functions.
    fn executable_ir(sources: &[(&str, &str)]) -> (String, Vec<String>) {
        let (ir, modules, _) = executable_ir_with(sources, None);
        (ir, modules)
    }

    /// `executable_ir` with `--trap-info` when `numbering` says how to number the sites, and the sites.
    fn executable_ir_with(
        sources: &[(&str, &str)],
        numbering: Option<crate::llvm::TrapNumbering>,
    ) -> (String, Vec<String>, Vec<crate::trap::TrapSite>) {
        use crate::llvm::{Allocator, EmitOptions, Entry, F16Path, emit_native_build_numbered};
        let module = crate::analyze_modules(sources)
            .unwrap_or_else(|error| panic!("{}: {}", error.code, error.message));
        let options = EmitOptions {
            entry: if module.entry.is_some() {
                Entry::Console
            } else {
                Entry::Library
            },
            wasm: false,
            wasm_simd128: false,
            debug_output: false,
            runtime_objects: numbering.is_none(),
            allocator: Allocator::System,
            f16: F16Path::Soft,
        };
        let traps: Vec<_> = sources
            .iter()
            .map(|(path, text)| crate::trap::TrapSource { path, text })
            .collect();
        let output = emit_native_build_numbered(
            &module,
            options,
            if numbering.is_some() { &traps } else { &[] },
            None,
            numbering.is_some(),
            &|_| numbering.unwrap_or(crate::llvm::TrapNumbering::Dense),
        )
        .unwrap();
        let names: BTreeSet<String> = module
            .functions
            .iter()
            .map(|function| function.module.clone())
            .filter(|name| !name.starts_with('$'))
            .collect();
        (
            crate::llvm::without_closure_inlining(output.ir),
            names.into_iter().collect(),
            output.trap_sites,
        )
    }

    const HELPER: &str = "def twice :: Add<'a> -> 'a = \\value -> value + value\n\ndef run :: i64u -> i64u = \\seed ->\n    let offset = seed % 7\n    let add = \\value -> value + offset\n    add (add (twice seed))\n";
    const OTHER: &str = "def run :: i64u -> i64u = \\seed ->\n    let shift = seed % 3\n    let scale = \\value -> value * 2 + shift\n    scale (scale seed)\n";
    const MAIN: &str = "def main :: unit -> i32 = \\() ->\n    let a = Helper.run 5i64u\n    let b = Other.run 6i64u\n    do! IO.write_line (to_string (a + b))\n    0\n";
    /// `MAIN` with a lambda and an instance of a generic function added before everything else: every serial
    /// number of the generated functions after them shifts.
    const MAIN_WITH_NEW_CODE: &str = "def main :: unit -> i32 = \\() ->\n    let inc = \\value -> value + 1i32\n    do! IO.write_line (to_string (inc (Helper.twice 4i32)))\n    let a = Helper.run 5i64u\n    let b = Other.run 6i64u\n    do! IO.write_line (to_string (a + b))\n    0\n";

    /// A change in `Main` that adds a lambda and an instance leaves the text of the other modules' units, and the
    /// text and the names of the generated functions that did not change.
    #[test]
    fn unrelated_edit_keeps_other_unit_texts() {
        let project = |main: &str| {
            let (ir, modules) = executable_ir(&[
                ("Main.tz", main),
                ("Helper.tz", HELPER),
                ("Other.tz", OTHER),
            ]);
            let modules: Vec<&str> = modules.iter().map(String::as_str).collect();
            let units = split(&SplitInput {
                ir: &ir,
                modules: &modules,
            })
            .unwrap();
            let found = check(&ir, &modules, &units);
            assert!(found.is_empty(), "{}", found.join("\n"));
            units
        };
        let before = project(MAIN);
        let after = project(MAIN_WITH_NEW_CODE);
        for name in ["m.Helper", "m.Other"] {
            assert_eq!(unit(&before, name), unit(&after, name), "{name}");
        }
        assert_ne!(unit(&before, "m.Main"), unit(&after, "m.Main"));
        let defined = |units: &[Unit]| -> BTreeSet<String> {
            units
                .iter()
                .filter(|unit| unit.name.starts_with("g."))
                .flat_map(|unit| {
                    unit.text
                        .lines()
                        .filter(|line| line.starts_with("define "))
                        .map(|line| line[..line.find('(').unwrap()].to_owned())
                })
                .collect()
        };
        let kept = defined(&before);
        let now = defined(&after);
        assert!(
            kept.len() > 20,
            "the runtime and the helpers are in the buckets"
        );
        assert!(
            kept.is_subset(&now),
            "a generated function keeps its name: {:?}",
            kept.difference(&now).collect::<Vec<_>>()
        );
        assert!(now.len() > kept.len(), "the edit added generated functions");
        let changed = before
            .iter()
            .zip(&after)
            .filter(|(old, new)| old.name.starts_with("g.") && old != new)
            .count();
        assert!(
            changed <= 6,
            "{changed} of the 16 buckets changed for a few new functions"
        );
    }

    const TRAP_MAIN: &str = "def main :: unit -> i32 = \\() ->\n    do! IO.write_line (to_string (A.divide 100 5))\n    do! IO.write_line (to_string (B.divide 100 7))\n    0\n";
    const TRAP_A: &str = "def divide :: i64 -> i64 -> i64 = \\a b -> a / b\n";
    const TRAP_B: &str = "def divide :: i64 -> i64 -> i64 = \\a b -> a / b\n";

    /// The units of a project with `--trap-info` and stable site numbers; the sites are the ones the emitter lists.
    fn trap_units(sources: &[(&str, &str)]) -> (String, Vec<Unit>, Vec<crate::trap::TrapSite>) {
        let (ir, modules, sites) =
            executable_ir_with(sources, Some(crate::llvm::TrapNumbering::Stable));
        let modules: Vec<&str> = modules.iter().map(String::as_str).collect();
        let units = split(&SplitInput {
            ir: &ir,
            modules: &modules,
        })
        .unwrap();
        let found = check(&ir, &modules, &units);
        assert!(found.is_empty(), "{}", found.join("\n"));
        (ir, units, sites)
    }

    /// With stable site numbers an edit that moves the sites of one file leaves the unit of every other file, and the
    /// one function that names the files, as they were; the file's own unit holds its report.
    #[test]
    fn trap_sites_keep_their_numbers_across_edits() {
        let project =
            |a: &str| trap_units(&[("Main.tz", TRAP_MAIN), ("A.tz", a), ("B.tz", TRAP_B)]);
        let (ir, before, sites) = project(TRAP_A);
        let shifted = format!("def twice :: i64 -> i64 = \\x -> x + x\n\n{TRAP_A}");
        let (_, after, moved) = project(&shifted);
        for name in ["m.B", "m.Main"] {
            assert_eq!(unit(&before, name), unit(&after, name), "{name}");
        }
        for (old, new) in before.iter().zip(&after) {
            if old.name.starts_with("g.") {
                assert_eq!(old, new, "{}", old.name);
            }
        }
        assert_ne!(unit(&before, "m.A"), unit(&after, "m.A"));
        assert!(unit(&before, "m.A").text.contains("@tz.trap.report.A("));
        assert!(unit(&before, "m.B").text.contains("@tz.trap.report.B("));
        assert!(
            before
                .iter()
                .filter(|unit| unit.name.starts_with("g."))
                .any(|unit| unit.text.contains("define hidden void @tz.trap.report("))
        );
        // The sites of B are the same in both, and the sites of A moved.
        let of = |sites: &[crate::trap::TrapSite], source: usize| -> Vec<(u32, usize)> {
            sites
                .iter()
                .filter(|site| site.span.source == Some(source))
                .map(|site| (site.id, site.span.start))
                .collect()
        };
        assert_eq!(of(&sites, 2), of(&moved, 2));
        assert_ne!(of(&sites, 1), of(&moved, 1));
        // The numbers in the code are the numbers of the side table.
        let listed: BTreeSet<u32> = sites.iter().map(|site| site.id).collect();
        let mut reported = 0;
        for line in ir.lines() {
            if let Some(rest) = line.trim().strip_prefix("call void @tz.trap.report(i32 ")
                && let Ok(id) = rest.trim_end_matches(')').parse::<u32>()
            {
                assert!(listed.contains(&id), "site {id} is not in the table");
                reported += 1;
            }
        }
        assert!(reported >= 2, "{reported} reports");
        let unique: BTreeSet<u32> = sites.iter().map(|site| site.id).collect();
        assert_eq!(unique.len(), sites.len());
        assert!(
            sites
                .iter()
                .all(|site| site.id > 0 && site.id < i32::MAX as u32)
        );
        // Moving the entry function changes its file's unit and nothing else: the runtime's traps do not name the
        // position of the entry function.
        let (_, edited, _) = trap_units(&[
            ("Main.tz", &format!("\n{TRAP_MAIN}")),
            ("A.tz", TRAP_A),
            ("B.tz", TRAP_B),
        ]);
        let changed: Vec<&str> = before
            .iter()
            .zip(&edited)
            .filter(|(old, new)| old != new)
            .map(|(old, _)| old.name.as_str())
            .collect();
        assert_eq!(changed, ["m.Main"]);
    }

    /// A project with more trap sites than the dense numbering gives one number to each keeps that numbering: the
    /// numbers count the locations from 1, in the order that they appear.
    #[test]
    fn dense_trap_numbers_count_the_locations() {
        let (ir, _, sites) = executable_ir_with(
            &[("Main.tz", TRAP_MAIN), ("A.tz", TRAP_A), ("B.tz", TRAP_B)],
            Some(crate::llvm::TrapNumbering::Dense),
        );
        let ids: BTreeSet<u32> = sites.iter().map(|site| site.id).collect();
        let kinds = crate::trap::TrapKind::ALL.len() as u32;
        let locations: BTreeSet<u32> = ids.iter().map(|id| (id - 1) / kinds).collect();
        assert_eq!(
            locations.iter().copied().collect::<Vec<_>>(),
            (0..locations.len() as u32).collect::<Vec<_>>(),
            "no gap in the numbers of the locations"
        );
        assert!(ir.contains("define internal void @tz.trap.report(i32 %site)"));
        assert!(!ir.contains("@tz.trap.report."));
    }

    /// Two functions with the same text are one function: nothing in the language can tell them apart, and a name that
    /// counted them would change whenever one of them is edited.
    #[test]
    fn identical_bodies_are_one_function() {
        let build = |a: u32, b: u32| {
            format!(
                "define internal i64 @tz.fn.$lambda.{a}(i64 %x) {{\nentry:\n  ret i64 %x\n}}\n\
                 define internal i64 @tz.fn.$lambda.{b}(i64 %x) {{\nentry:\n  ret i64 %x\n}}\n\
                 define internal i64 @tz.fn.Main.one(i64 %x) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{a}(i64 %x)\n  ret i64 %r\n}}\n\
                 define internal i64 @tz.fn.Main.two(i64 %x) {{\nentry:\n  %r = call i64 @tz.fn.$lambda.{b}(i64 %x)\n  ret i64 %r\n}}\n"
            )
        };
        let units = split(&SplitInput {
            ir: &build(5, 6),
            modules: &["Main"],
        })
        .unwrap();
        let all: String = units.iter().map(|unit| unit.text.as_str()).collect();
        let defined: Vec<_> = all
            .lines()
            .filter_map(|line| line.strip_prefix("define hidden i64 @tz.fn.$lambda."))
            .collect();
        assert_eq!(defined.len(), 1, "{all}");
        let name = defined[0].split('(').next().unwrap();
        assert_eq!(
            all.matches(&format!("call i64 @tz.fn.$lambda.{name}("))
                .count(),
            2,
            "both callers call it"
        );
        // Editing one of them separates them, and the name of the other does not change.
        let edited = split(&SplitInput {
            ir: &build(5, 6).replacen("ret i64 %x", "%y = add i64 %x, 1\n  ret i64 %y", 1),
            modules: &["Main"],
        })
        .unwrap();
        let all_edited: String = edited.iter().map(|unit| unit.text.as_str()).collect();
        assert!(
            all_edited.contains(&format!("define hidden i64 @tz.fn.$lambda.{name}(")),
            "{all_edited}"
        );
        assert_eq!(
            all_edited
                .lines()
                .filter(|line| line.starts_with("define hidden i64 @tz.fn.$lambda."))
                .count(),
            2
        );
    }
}
