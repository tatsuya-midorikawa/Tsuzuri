//! The binary form of a parsed `Program` that the frontend cache stores (G17).
//!
//! The codec is written by hand so that it needs no crate. Every syntax type that `Program`
//! reaches has a `Wire` impl; the impls destructure every struct and match every enum variant
//! without a wildcard, so a new field or variant fails to compile here until it is encoded.
//! Enum tags are the variants' declaration order as one byte, integers are unsigned LEB128,
//! strings and sequences are a length followed by their contents.
//!
//! `Mode::Full` writes everything except the source index of spans, which `decode` sets to the
//! source being decoded, so a file keeps its entry when files are added or removed. A span of
//! another source cannot be encoded. `Mode::Interface` also leaves out spans, documentation,
//! function bodies, tests, benches, the entry code and the bodies of instance methods: what
//! other modules can observe (D6). It is only encoded, for `frontend_cache::interface_hash`.
//!
//! Changing the encoding changes `pins_encoding_format` and requires bumping
//! `frontend_cache::FRONTEND_FORMAT`.

use crate::diagnostic::Span;
use crate::syntax::*;

/// The deepest nesting of recursive syntax (expressions, patterns, types, computation blocks
/// and kinds) that the codec reads or writes. The parser bounds every chain to `MAX_NESTING`.
pub(crate) const MAX_DEPTH: u32 = 4 * MAX_NESTING as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Full,
    Interface,
}

/// A corrupt or unsupported encoding. Callers treat it as a cache miss and do not store.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invalid;

pub(crate) struct Writer {
    bytes: Vec<u8>,
    mode: Mode,
    source: usize,
    depth: u32,
    limit: u32,
    /// The variants written, by enum, so tests can check that a corpus reaches every one.
    #[cfg(test)]
    seen: std::collections::BTreeSet<(&'static str, u8)>,
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    source: usize,
    depth: u32,
}

pub(crate) trait Wire: Sized {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid>;
    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid>;
}

/// Encodes `program`, parsed as source `source`, in `mode`.
pub(crate) fn encode(program: &Program, source: usize, mode: Mode) -> Result<Vec<u8>, Invalid> {
    encode_within(program, source, mode, MAX_DEPTH)
}

fn encode_within(
    program: &Program,
    source: usize,
    mode: Mode,
    limit: u32,
) -> Result<Vec<u8>, Invalid> {
    let mut out = Writer::new(mode, source, limit);
    program.put(&mut out)?;
    Ok(out.bytes)
}

/// Decodes a `Mode::Full` encoding as the program of source `source`.
pub(crate) fn decode(bytes: &[u8], source: usize) -> Result<Program, Invalid> {
    let mut input = Reader {
        bytes,
        at: 0,
        source,
        depth: 0,
    };
    let program = Program::get(&mut input)?;
    if input.at != bytes.len() {
        return Err(Invalid);
    }
    Ok(program)
}

impl Writer {
    fn new(mode: Mode, source: usize, limit: u32) -> Self {
        Self {
            bytes: Vec::new(),
            mode,
            source,
            depth: 0,
            limit,
            #[cfg(test)]
            seen: std::collections::BTreeSet::new(),
        }
    }

    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// The tag of a variant of the enum `name`.
    fn tag(&mut self, name: &'static str, tag: u8) {
        #[cfg(test)]
        self.seen.insert((name, tag));
        #[cfg(not(test))]
        let _ = name;
        self.bytes.push(tag);
    }

    fn number(&mut self, mut value: u128) {
        loop {
            let low = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(low);
                return;
            }
            self.bytes.push(low | 0x80);
        }
    }

    fn full(&self) -> bool {
        self.mode == Mode::Full
    }

    fn enter(&mut self) -> Result<(), Invalid> {
        if self.depth >= self.limit {
            return Err(Invalid);
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, Invalid> {
        let byte = *self.bytes.get(self.at).ok_or(Invalid)?;
        self.at += 1;
        Ok(byte)
    }

    /// An unsigned LEB128 number of at most 128 bits in its shortest form.
    fn number(&mut self) -> Result<u128, Invalid> {
        let mut value = 0u128;
        for shift in (0..128).step_by(7) {
            let byte = self.byte()?;
            let bits = u128::from(byte & 0x7f);
            if shift == 126 && bits > 0b11 {
                return Err(Invalid);
            }
            value |= bits << shift;
            if byte & 0x80 == 0 {
                return if byte == 0 && shift != 0 {
                    Err(Invalid)
                } else {
                    Ok(value)
                };
            }
        }
        Err(Invalid)
    }

    /// A length prefix. Every encoded element takes at least one byte, so a length beyond
    /// the remaining bytes is corrupt; checking it first bounds every allocation.
    fn length(&mut self) -> Result<usize, Invalid> {
        let length = usize::try_from(self.number()?).map_err(|_| Invalid)?;
        if length > self.bytes.len() - self.at {
            return Err(Invalid);
        }
        Ok(length)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], Invalid> {
        let bytes = self.bytes.get(self.at..self.at + length).ok_or(Invalid)?;
        self.at += length;
        Ok(bytes)
    }

    fn enter(&mut self) -> Result<(), Invalid> {
        if self.depth >= MAX_DEPTH {
            return Err(Invalid);
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }
}

/// Reads one level of recursive syntax, keeping `enter` and `leave` paired on every path.
fn nested<T>(
    input: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, Invalid>,
) -> Result<T, Invalid> {
    input.enter()?;
    let value = read(input);
    input.leave();
    value
}

/// Writes one level of recursive syntax.
fn nest(
    out: &mut Writer,
    write: impl FnOnce(&mut Writer) -> Result<(), Invalid>,
) -> Result<(), Invalid> {
    out.enter()?;
    let result = write(out);
    out.leave();
    result
}

/// Calls `read` in a frame of its own, which keeps the frames of the large enum decoders on
/// the recursion path small in debug builds (their tests run on 2 MiB stacks).
fn variant<T>(
    input: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, Invalid>,
) -> Result<T, Invalid> {
    read(input)
}

macro_rules! integers {
    ($($type:ty),+) => {
        $(
            impl Wire for $type {
                fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
                    out.number(*self as u128);
                    Ok(())
                }

                fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
                    <$type>::try_from(input.number()?).map_err(|_| Invalid)
                }
            }
        )+
    };
}

integers!(u8, u16, u32, u64, usize, u128);

/// A fieldless enum, tagged by its declaration order. The `match` in `put` lists every variant.
macro_rules! unit_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        impl Wire for $name {
            fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
                let tag = match self {
                    $($name::$variant => $name::$variant as u8,)+
                };
                out.tag(stringify!($name), tag);
                Ok(())
            }

            fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
                let tag = input.byte()?;
                [$($name::$variant),+]
                    .into_iter()
                    .find(|variant| *variant as u8 == tag)
                    .ok_or(Invalid)
            }
        }
    };
}

unit_enum!(SourceKind {
    Code,
    TypeClass,
    Computation
});
unit_enum!(Provenance { User, Generated });
unit_enum!(Visibility { Public, Private });
unit_enum!(DeriveClass {
    Eq,
    Ord,
    Display,
    Hash,
    Default,
    Encode,
    Decode
});
unit_enum!(UnaryOp {
    Negate,
    Not,
    BitNot,
    Plus
});
unit_enum!(BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
    Power,
    Pipe,
});
unit_enum!(MatchOrigin {
    Explicit,
    FunctionGuard,
    LambdaDestructuring,
    ComputationDestructuring,
});
unit_enum!(Notation { Symbol, Keyword });
unit_enum!(FormatAlign {
    Left,
    Right,
    Center
});
unit_enum!(FormatKind {
    LowerHex,
    UpperHex,
    Octal,
    Binary,
    Exponent,
    Fixed,
});

impl Wire for bool {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        out.byte(u8::from(*self));
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        match input.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            2..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for char {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        u32::from(*self).put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        char::from_u32(u32::get(input)?).ok_or(Invalid)
    }
}

impl Wire for String {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        out.number(self.len() as u128);
        out.bytes.extend_from_slice(self.as_bytes());
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        let length = input.length()?;
        String::from_utf8(input.take(length)?.to_vec()).map_err(|_| Invalid)
    }
}

impl Wire for Box<str> {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        out.number(self.len() as u128);
        out.bytes.extend_from_slice(self.as_bytes());
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        String::get(input).map(String::into_boxed_str)
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        out.number(self.len() as u128);
        self.iter().try_for_each(|value| value.put(out))
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        let length = input.length()?;
        let mut values = Vec::new();
        for _ in 0..length {
            values.push(T::get(input)?);
        }
        Ok(values)
    }
}

impl<T: Wire> Wire for Box<[T]> {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        out.number(self.len() as u128);
        self.iter().try_for_each(|value| value.put(out))
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Vec::get(input).map(Vec::into_boxed_slice)
    }
}

impl<T: Wire> Wire for Box<T> {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        (**self).put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        T::get(input).map(Box::new)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        match self {
            None => {
                out.byte(0);
                Ok(())
            }
            Some(value) => {
                out.byte(1);
                value.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        match input.byte()? {
            0 => Ok(None),
            1 => T::get(input).map(Some),
            2..=u8::MAX => Err(Invalid),
        }
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        self.0.put(out)?;
        self.1.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok((A::get(input)?, B::get(input)?))
    }
}

impl Wire for Span {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Span { start, end, source } = *self;
        if !out.full() {
            return Ok(());
        }
        start.put(out)?;
        end.put(out)?;
        match source {
            None => out.byte(0),
            Some(source) if source == out.source => out.byte(1),
            Some(_) => return Err(Invalid),
        }
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        let start = usize::get(input)?;
        let end = usize::get(input)?;
        let source = match input.byte()? {
            0 => None,
            1 => Some(input.source),
            2..=u8::MAX => return Err(Invalid),
        };
        Ok(Span { start, end, source })
    }
}

impl Wire for Documentation {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Documentation { text, span } = self;
        text.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Documentation {
            text: String::get(input)?,
            span: Span::get(input)?,
        })
    }
}

/// Documentation comments are not part of the interface.
fn put_doc(doc: &Option<Documentation>, out: &mut Writer) -> Result<(), Invalid> {
    if out.full() { doc.put(out) } else { Ok(()) }
}

impl Wire for Ident {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Ident {
            text,
            span,
            provenance,
        } = self;
        text.put(out)?;
        span.put(out)?;
        provenance.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Ident {
            text: String::get(input)?,
            span: Span::get(input)?,
            provenance: Provenance::get(input)?,
        })
    }
}

impl Wire for Program {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Program {
            source_kind,
            namespace,
            usings,
            type_aliases,
            constants,
            externs,
            extern_types,
            records,
            unions,
            functions,
            classes,
            instances,
            active_patterns,
            tests,
            benches,
            entry,
            dyn_types,
            cpu_attributes,
        } = self;
        source_kind.put(out)?;
        namespace.put(out)?;
        usings.put(out)?;
        type_aliases.put(out)?;
        constants.put(out)?;
        externs.put(out)?;
        extern_types.put(out)?;
        records.put(out)?;
        unions.put(out)?;
        functions.put(out)?;
        classes.put(out)?;
        instances.put(out)?;
        active_patterns.put(out)?;
        if out.full() {
            tests.put(out)?;
            benches.put(out)?;
            entry.put(out)?;
        }
        dyn_types.put(out)?;
        cpu_attributes.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Program {
            source_kind: Option::get(input)?,
            namespace: Option::get(input)?,
            usings: Vec::get(input)?,
            type_aliases: Vec::get(input)?,
            constants: Vec::get(input)?,
            externs: Vec::get(input)?,
            extern_types: Vec::get(input)?,
            records: Vec::get(input)?,
            unions: Vec::get(input)?,
            functions: Vec::get(input)?,
            classes: Vec::get(input)?,
            instances: Vec::get(input)?,
            active_patterns: Vec::get(input)?,
            tests: Vec::get(input)?,
            benches: Vec::get(input)?,
            entry: Option::get(input)?,
            dyn_types: Vec::get(input)?,
            cpu_attributes: Vec::get(input)?,
        })
    }
}

impl Wire for CpuAttribute {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let CpuAttribute { function, levels } = self;
        function.put(out)?;
        levels.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(CpuAttribute {
            function: Ident::get(input)?,
            levels: u8::get(input)?,
        })
    }
}

impl Wire for NamespaceDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let NamespaceDecl { path, span } = self;
        path.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(NamespaceDecl {
            path: Ident::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for TestDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let TestDecl {
            name,
            name_span,
            body,
            span,
        } = self;
        name.put(out)?;
        name_span.put(out)?;
        body.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(TestDecl {
            name: String::get(input)?,
            name_span: Span::get(input)?,
            body: Expr::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for BenchDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let BenchDecl {
            name,
            name_span,
            body,
            span,
        } = self;
        name.put(out)?;
        name_span.put(out)?;
        body.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(BenchDecl {
            name: String::get(input)?,
            name_span: Span::get(input)?,
            body: Expr::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for ActivePattern {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ActivePattern {
            cases,
            function,
            partial,
        } = self;
        cases.put(out)?;
        function.put(out)?;
        partial.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ActivePattern {
            cases: Vec::get(input)?,
            function: String::get(input)?,
            partial: bool::get(input)?,
        })
    }
}

impl Wire for TypeAliasDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let TypeAliasDecl {
            doc,
            visibility,
            name,
            parameters,
            target,
        } = self;
        put_doc(doc, out)?;
        visibility.put(out)?;
        name.put(out)?;
        parameters.put(out)?;
        target.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(TypeAliasDecl {
            doc: Option::get(input)?,
            visibility: Visibility::get(input)?,
            name: Ident::get(input)?,
            parameters: Vec::get(input)?,
            target: TypeExpr::get(input)?,
        })
    }
}

impl Wire for ConstDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ConstDecl {
            doc,
            visibility,
            name,
            ty,
            value,
        } = self;
        put_doc(doc, out)?;
        visibility.put(out)?;
        name.put(out)?;
        ty.put(out)?;
        value.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ConstDecl {
            doc: Option::get(input)?,
            visibility: Visibility::get(input)?,
            name: Ident::get(input)?,
            ty: TypeExpr::get(input)?,
            value: Expr::get(input)?,
        })
    }
}

impl Wire for RecordDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let RecordDecl {
            doc,
            visibility,
            name,
            parameters,
            regions,
            fields,
            derives,
        } = self;
        put_doc(doc, out)?;
        visibility.put(out)?;
        name.put(out)?;
        parameters.put(out)?;
        regions.put(out)?;
        fields.put(out)?;
        derives.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(RecordDecl {
            doc: Option::get(input)?,
            visibility: Visibility::get(input)?,
            name: Ident::get(input)?,
            parameters: Vec::get(input)?,
            regions: Vec::get(input)?,
            fields: Vec::get(input)?,
            derives: Vec::get(input)?,
        })
    }
}

impl Wire for UnionDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let UnionDecl {
            doc,
            visibility,
            name,
            parameters,
            cases,
            derives,
        } = self;
        put_doc(doc, out)?;
        visibility.put(out)?;
        name.put(out)?;
        parameters.put(out)?;
        cases.put(out)?;
        derives.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(UnionDecl {
            doc: Option::get(input)?,
            visibility: Visibility::get(input)?,
            name: Ident::get(input)?,
            parameters: Vec::get(input)?,
            cases: Vec::get(input)?,
            derives: Vec::get(input)?,
        })
    }
}

impl Wire for UnionCaseDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let UnionCaseDecl {
            name,
            payload,
            json,
        } = self;
        name.put(out)?;
        payload.put(out)?;
        json.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(UnionCaseDecl {
            name: Ident::get(input)?,
            payload: Option::get(input)?,
            json: Option::get(input)?,
        })
    }
}

impl Wire for FunctionDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let FunctionDecl {
            doc,
            name,
            regions,
            recursion,
            visibility,
            exported,
            parameters,
            result,
            constraints,
            body,
        } = self;
        put_doc(doc, out)?;
        name.put(out)?;
        regions.put(out)?;
        recursion.put(out)?;
        visibility.put(out)?;
        exported.put(out)?;
        parameters.put(out)?;
        result.put(out)?;
        constraints.put(out)?;
        if out.full() {
            body.put(out)?;
        }
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(FunctionDecl {
            doc: Option::get(input)?,
            name: Ident::get(input)?,
            regions: Vec::get(input)?,
            recursion: Option::get(input)?,
            visibility: Visibility::get(input)?,
            exported: bool::get(input)?,
            parameters: Vec::get(input)?,
            result: TypeExpr::get(input)?,
            constraints: Vec::get(input)?,
            body: Expr::get(input)?,
        })
    }
}

impl Wire for SignatureDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let SignatureDecl {
            doc,
            name,
            regions,
            recursion,
            visibility,
            exported,
            parameters,
            result,
            constraints,
            link,
        } = self;
        put_doc(doc, out)?;
        name.put(out)?;
        regions.put(out)?;
        recursion.put(out)?;
        visibility.put(out)?;
        exported.put(out)?;
        parameters.put(out)?;
        result.put(out)?;
        constraints.put(out)?;
        link.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(SignatureDecl {
            doc: Option::get(input)?,
            name: Ident::get(input)?,
            regions: Vec::get(input)?,
            recursion: Option::get(input)?,
            visibility: Visibility::get(input)?,
            exported: bool::get(input)?,
            parameters: Vec::get(input)?,
            result: TypeExpr::get(input)?,
            constraints: Vec::get(input)?,
            link: Option::get(input)?,
        })
    }
}

impl Wire for LinkName {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let LinkName {
            module,
            symbol,
            span,
        } = self;
        module.put(out)?;
        symbol.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(LinkName {
            module: Option::get(input)?,
            symbol: String::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for ExternTypeDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ExternTypeDecl {
            doc,
            visibility,
            name,
        } = self;
        put_doc(doc, out)?;
        visibility.put(out)?;
        name.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ExternTypeDecl {
            doc: Option::get(input)?,
            visibility: Visibility::get(input)?,
            name: Ident::get(input)?,
        })
    }
}

impl Wire for Definition {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Definition {
            name,
            recursion,
            parameters,
            body,
        } = self;
        name.put(out)?;
        recursion.put(out)?;
        parameters.put(out)?;
        body.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Definition {
            name: Ident::get(input)?,
            recursion: Option::get(input)?,
            parameters: Vec::get(input)?,
            body: Expr::get(input)?,
        })
    }
}

impl Wire for ConstraintExpr {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ConstraintExpr { name, ty } = self;
        name.put(out)?;
        ty.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ConstraintExpr {
            name: ConstraintName::get(input)?,
            ty: TypeExpr::get(input)?,
        })
    }
}

impl Wire for ConstraintName {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        match self {
            ConstraintName::Class(name) => {
                out.tag("ConstraintName", 0);
                name.put(out)
            }
            ConstraintName::Function(name, ty) => {
                out.tag("ConstraintName", 1);
                name.put(out)?;
                ty.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        match input.byte()? {
            0 => Ok(ConstraintName::Class(Ident::get(input)?)),
            1 => Ok(ConstraintName::Function(
                Ident::get(input)?,
                Option::get(input)?,
            )),
            2..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for ClassDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ClassDecl {
            doc,
            name,
            variable,
            kind,
            superclasses,
            methods,
            defaults,
        } = self;
        put_doc(doc, out)?;
        name.put(out)?;
        variable.put(out)?;
        kind.put(out)?;
        superclasses.put(out)?;
        methods.put(out)?;
        // The bodies of default methods are part of the interface (D6).
        defaults.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ClassDecl {
            doc: Option::get(input)?,
            name: Ident::get(input)?,
            variable: Ident::get(input)?,
            kind: Kind::get(input)?,
            superclasses: Vec::get(input)?,
            methods: Vec::get(input)?,
            defaults: Vec::get(input)?,
        })
    }
}

impl Wire for Kind {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        nest(out, |out| match self {
            Kind::Type => {
                out.tag("Kind", 0);
                Ok(())
            }
            Kind::Arrow(argument, result) => {
                out.tag("Kind", 1);
                argument.put(out)?;
                result.put(out)
            }
        })
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        nested(input, |input| match input.byte()? {
            0 => Ok(Kind::Type),
            1 => Ok(Kind::Arrow(Box::get(input)?, Box::get(input)?)),
            2..=u8::MAX => Err(Invalid),
        })
    }
}

impl Wire for InstanceDecl {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let InstanceDecl {
            class,
            ty,
            constraints,
            methods,
        } = self;
        class.put(out)?;
        ty.put(out)?;
        constraints.put(out)?;
        if out.full() {
            return methods.put(out);
        }
        // The interface keeps which methods an instance defines, but not their bodies.
        out.number(methods.len() as u128);
        for method in methods {
            let Definition {
                name,
                recursion,
                parameters,
                body: _,
            } = method;
            name.put(out)?;
            recursion.put(out)?;
            parameters.put(out)?;
        }
        Ok(())
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(InstanceDecl {
            class: Ident::get(input)?,
            ty: TypeExpr::get(input)?,
            constraints: Vec::get(input)?,
            methods: Vec::get(input)?,
        })
    }
}

impl Wire for Parameter {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Parameter {
            name,
            ty,
            mutable,
            json,
        } = self;
        name.put(out)?;
        ty.put(out)?;
        mutable.put(out)?;
        json.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Parameter {
            name: Ident::get(input)?,
            ty: TypeExpr::get(input)?,
            mutable: bool::get(input)?,
            json: Option::get(input)?,
        })
    }
}

impl Wire for JsonName {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let JsonName { units, span } = self;
        units.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(JsonName {
            units: Vec::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for TypeExpr {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let TypeExpr { kind, span } = self;
        nest(out, |out| kind.put(out))?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(TypeExpr {
            kind: nested(input, TypeExprKind::get)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for TypeExprKind {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        use TypeExprKind::*;
        match self {
            Named(name) => {
                out.tag("TypeExprKind", 0);
                name.put(out)
            }
            Variable(name) => {
                out.tag("TypeExprKind", 1);
                name.put(out)
            }
            Apply(head, arguments) => {
                out.tag("TypeExprKind", 2);
                head.put(out)?;
                arguments.put(out)
            }
            Regions(ty, regions) => {
                out.tag("TypeExprKind", 3);
                ty.put(out)?;
                regions.put(out)
            }
            Quantified(regions, ty) => {
                out.tag("TypeExprKind", 4);
                regions.put(out)?;
                ty.put(out)
            }
            Array(element) => {
                out.tag("TypeExprKind", 5);
                element.put(out)
            }
            ArrayView(element) => {
                out.tag("TypeExprKind", 6);
                element.put(out)
            }
            FixedArray(element, length) => {
                out.tag("TypeExprKind", 7);
                element.put(out)?;
                length.put(out)
            }
            Length(length) => {
                out.tag("TypeExprKind", 8);
                length.put(out)
            }
            List(element) => {
                out.tag("TypeExprKind", 9);
                element.put(out)
            }
            Tuple(elements) => {
                out.tag("TypeExprKind", 10);
                elements.put(out)
            }
            Task(result) => {
                out.tag("TypeExprKind", 11);
                result.put(out)
            }
            Function(parameters, result) => {
                out.tag("TypeExprKind", 12);
                parameters.put(out)?;
                result.put(out)
            }
            Reference(target, mutable) => {
                out.tag("TypeExprKind", 13);
                target.put(out)?;
                mutable.put(out)
            }
            Dyn(dyn_type) => {
                out.tag("TypeExprKind", 14);
                dyn_type.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        use TypeExprKind::*;
        match input.byte()? {
            0 => variant(input, |input| Ok(Named(String::get(input)?))),
            1 => variant(input, |input| Ok(Variable(String::get(input)?))),
            2 => variant(input, |input| {
                Ok(Apply(Box::get(input)?, Box::<[TypeExpr]>::get(input)?))
            }),
            3 => variant(input, |input| {
                Ok(Regions(Box::get(input)?, Box::<[Ident]>::get(input)?))
            }),
            4 => variant(input, |input| {
                Ok(Quantified(Box::<[Ident]>::get(input)?, Box::get(input)?))
            }),
            5 => variant(input, |input| Ok(Array(Box::get(input)?))),
            6 => variant(input, |input| Ok(ArrayView(Box::get(input)?))),
            7 => variant(input, |input| {
                Ok(FixedArray(Box::get(input)?, Box::get(input)?))
            }),
            8 => variant(input, |input| Ok(Length(u64::get(input)?))),
            9 => variant(input, |input| Ok(List(Box::get(input)?))),
            10 => variant(input, |input| Ok(Tuple(Vec::get(input)?))),
            11 => variant(input, |input| Ok(Task(Box::get(input)?))),
            12 => variant(input, |input| {
                Ok(Function(Vec::get(input)?, Box::get(input)?))
            }),
            13 => variant(input, |input| {
                Ok(Reference(Box::get(input)?, bool::get(input)?))
            }),
            14 => variant(input, |input| Ok(Dyn(Box::get(input)?))),
            15..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for DynTypeExpr {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let DynTypeExpr { classes, borrowed } = self;
        classes.put(out)?;
        borrowed.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(DynTypeExpr {
            classes: Vec::get(input)?,
            borrowed: bool::get(input)?,
        })
    }
}

impl Wire for StringLiteral {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        match self {
            StringLiteral::Utf16(units) => {
                out.tag("StringLiteral", 0);
                units.put(out)
            }
            StringLiteral::Utf8(text) => {
                out.tag("StringLiteral", 1);
                text.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        match input.byte()? {
            0 => Ok(StringLiteral::Utf16(Vec::get(input)?)),
            1 => Ok(StringLiteral::Utf8(String::get(input)?)),
            2..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for Expr {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Expr { kind, span, depth } = self;
        nest(out, |out| kind.put(out))?;
        span.put(out)?;
        depth.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Expr {
            kind: nested(input, ExprKind::get)?,
            span: Span::get(input)?,
            depth: usize::get(input)?,
        })
    }
}

impl Wire for ExprKind {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        use ExprKind::*;
        match self {
            Integer(value, suffix) => {
                out.tag("ExprKind", 0);
                value.put(out)?;
                suffix.put(out)
            }
            BigInt(digits) => {
                out.tag("ExprKind", 1);
                digits.put(out)
            }
            Float(text, suffix) => {
                out.tag("ExprKind", 2);
                text.put(out)?;
                suffix.put(out)
            }
            String(literal) => {
                out.tag("ExprKind", 3);
                literal.put(out)
            }
            Char(unit) => {
                out.tag("ExprKind", 4);
                unit.put(out)
            }
            Utf8Char(scalar) => {
                out.tag("ExprKind", 5);
                scalar.put(out)
            }
            Bool(value) => {
                out.tag("ExprKind", 6);
                value.put(out)
            }
            Unit => {
                out.tag("ExprKind", 7);
                Ok(())
            }
            Break => {
                out.tag("ExprKind", 8);
                Ok(())
            }
            Continue => {
                out.tag("ExprKind", 9);
                Ok(())
            }
            Name(name) => {
                out.tag("ExprKind", 10);
                name.put(out)
            }
            QualifiedFunction(name) => {
                out.tag("ExprKind", 11);
                name.put(out)
            }
            TypeFunction(ty, function) => {
                out.tag("ExprKind", 12);
                ty.put(out)?;
                function.put(out)
            }
            Unary(op, value) => {
                out.tag("ExprKind", 13);
                op.put(out)?;
                value.put(out)
            }
            Binary(op, left, right) => {
                out.tag("ExprKind", 14);
                op.put(out)?;
                left.put(out)?;
                right.put(out)
            }
            Call(callee, arguments) => {
                out.tag("ExprKind", 15);
                callee.put(out)?;
                arguments.put(out)
            }
            Lambda(parameters, body) => {
                out.tag("ExprKind", 16);
                parameters.put(out)?;
                body.put(out)
            }
            Task(body) => {
                out.tag("ExprKind", 17);
                body.put(out)
            }
            TaskRun(body) => {
                out.tag("ExprKind", 18);
                body.put(out)
            }
            Computation(builder, block) => {
                out.tag("ExprKind", 19);
                builder.put(out)?;
                block.put(out)
            }
            ComputationBoundary(value) => {
                out.tag("ExprKind", 20);
                value.put(out)
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                out.tag("ExprKind", 21);
                condition.put(out)?;
                then_branch.put(out)?;
                else_branch.put(out)
            }
            While { condition, body } => {
                out.tag("ExprKind", 22);
                condition.put(out)?;
                body.put(out)
            }
            For {
                pattern,
                source,
                body,
            } => {
                out.tag("ExprKind", 23);
                pattern.put(out)?;
                source.put(out)?;
                body.put(out)
            }
            Range {
                start,
                step,
                finish,
                counted,
                descending,
            } => {
                out.tag("ExprKind", 24);
                start.put(out)?;
                step.put(out)?;
                finish.put(out)?;
                counted.put(out)?;
                descending.put(out)
            }
            Match {
                value,
                arms,
                origin,
            } => {
                out.tag("ExprKind", 25);
                value.put(out)?;
                arms.put(out)?;
                origin.put(out)
            }
            Try(handled) => {
                out.tag("ExprKind", 26);
                handled.put(out)
            }
            Checked(value) => {
                out.tag("ExprKind", 27);
                value.put(out)
            }
            Block { bindings, result } => {
                out.tag("ExprKind", 28);
                bindings.put(out)?;
                result.put(out)
            }
            Record { name, fields } => {
                out.tag("ExprKind", 29);
                name.put(out)?;
                fields.put(out)
            }
            RecordUpdate { base, fields } => {
                out.tag("ExprKind", 30);
                base.put(out)?;
                fields.put(out)
            }
            Array(values) => {
                out.tag("ExprKind", 31);
                values.put(out)
            }
            List(values) => {
                out.tag("ExprKind", 32);
                values.put(out)
            }
            Tuple(values) => {
                out.tag("ExprKind", 33);
                values.put(out)
            }
            NewArray(ty, length, initializer) => {
                out.tag("ExprKind", 34);
                ty.put(out)?;
                length.put(out)?;
                initializer.put(out)
            }
            NewList(ty, length, initializer) => {
                out.tag("ExprKind", 35);
                ty.put(out)?;
                length.put(out)?;
                initializer.put(out)
            }
            NewLiteral(value) => {
                out.tag("ExprKind", 36);
                value.put(out)
            }
            Field(value, field) => {
                out.tag("ExprKind", 37);
                value.put(out)?;
                field.put(out)
            }
            Index(value, index) => {
                out.tag("ExprKind", 38);
                value.put(out)?;
                index.put(out)
            }
            Slice {
                value,
                start,
                end,
                mutable,
            } => {
                out.tag("ExprKind", 39);
                value.put(out)?;
                start.put(out)?;
                end.put(out)?;
                mutable.put(out)
            }
            Borrow(value, mutable, notation) => {
                out.tag("ExprKind", 40);
                value.put(out)?;
                mutable.put(out)?;
                notation.put(out)
            }
            Dereference(value, notation) => {
                out.tag("ExprKind", 41);
                value.put(out)?;
                notation.put(out)
            }
            Assign(target, value) => {
                out.tag("ExprKind", 42);
                target.put(out)?;
                value.put(out)
            }
            Cast(value, ty) => {
                out.tag("ExprKind", 43);
                value.put(out)?;
                ty.put(out)
            }
            Interpolated(interpolation) => {
                out.tag("ExprKind", 44);
                interpolation.put(out)
            }
            DynDispatch { slot, slots } => {
                out.tag("ExprKind", 45);
                slot.put(out)?;
                slots.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        use ExprKind::*;
        match input.byte()? {
            0 => variant(input, |input| {
                Ok(Integer(u128::get(input)?, Option::get(input)?))
            }),
            1 => variant(input, |input| Ok(BigInt(Box::<str>::get(input)?))),
            2 => variant(input, |input| {
                Ok(Float(std::string::String::get(input)?, Option::get(input)?))
            }),
            3 => variant(input, |input| Ok(String(StringLiteral::get(input)?))),
            4 => variant(input, |input| Ok(Char(u16::get(input)?))),
            5 => variant(input, |input| Ok(Utf8Char(u32::get(input)?))),
            6 => variant(input, |input| Ok(Bool(bool::get(input)?))),
            7 => Ok(Unit),
            8 => Ok(Break),
            9 => Ok(Continue),
            10 => variant(input, |input| Ok(Name(Ident::get(input)?))),
            11 => variant(input, |input| Ok(QualifiedFunction(Ident::get(input)?))),
            12 => variant(input, |input| {
                Ok(TypeFunction(Box::get(input)?, Box::get(input)?))
            }),
            13 => variant(input, |input| {
                Ok(Unary(UnaryOp::get(input)?, Box::get(input)?))
            }),
            14 => variant(input, |input| {
                Ok(Binary(
                    BinaryOp::get(input)?,
                    Box::get(input)?,
                    Box::get(input)?,
                ))
            }),
            15 => variant(input, |input| Ok(Call(Box::get(input)?, Vec::get(input)?))),
            16 => variant(input, |input| {
                Ok(Lambda(Vec::get(input)?, Box::get(input)?))
            }),
            17 => variant(input, |input| Ok(Task(Box::get(input)?))),
            18 => variant(input, |input| Ok(TaskRun(Box::get(input)?))),
            19 => variant(input, |input| {
                Ok(Computation(Ident::get(input)?, Box::get(input)?))
            }),
            20 => variant(input, |input| Ok(ComputationBoundary(Box::get(input)?))),
            21 => variant(input, |input| {
                Ok(If {
                    condition: Box::get(input)?,
                    then_branch: Box::get(input)?,
                    else_branch: Box::get(input)?,
                })
            }),
            22 => variant(input, |input| {
                Ok(While {
                    condition: Box::get(input)?,
                    body: Box::get(input)?,
                })
            }),
            23 => variant(input, |input| {
                Ok(For {
                    pattern: Box::get(input)?,
                    source: Box::get(input)?,
                    body: Box::get(input)?,
                })
            }),
            24 => variant(input, |input| {
                Ok(Range {
                    start: Box::get(input)?,
                    step: Option::get(input)?,
                    finish: Box::get(input)?,
                    counted: bool::get(input)?,
                    descending: bool::get(input)?,
                })
            }),
            25 => variant(input, |input| {
                Ok(Match {
                    value: Box::get(input)?,
                    arms: Vec::get(input)?,
                    origin: MatchOrigin::get(input)?,
                })
            }),
            26 => variant(input, |input| Ok(Try(Box::get(input)?))),
            27 => variant(input, |input| Ok(Checked(Box::get(input)?))),
            28 => variant(input, |input| {
                Ok(Block {
                    bindings: Vec::get(input)?,
                    result: Box::get(input)?,
                })
            }),
            29 => variant(input, |input| {
                Ok(Record {
                    name: Box::get(input)?,
                    fields: Vec::get(input)?,
                })
            }),
            30 => variant(input, |input| {
                Ok(RecordUpdate {
                    base: Box::get(input)?,
                    fields: Vec::get(input)?,
                })
            }),
            31 => variant(input, |input| Ok(Array(Vec::get(input)?))),
            32 => variant(input, |input| Ok(List(Vec::get(input)?))),
            33 => variant(input, |input| Ok(Tuple(Vec::get(input)?))),
            34 => variant(input, |input| {
                Ok(NewArray(
                    Box::get(input)?,
                    Box::get(input)?,
                    Box::get(input)?,
                ))
            }),
            35 => variant(input, |input| {
                Ok(NewList(
                    Box::get(input)?,
                    Box::get(input)?,
                    Box::get(input)?,
                ))
            }),
            36 => variant(input, |input| Ok(NewLiteral(Box::get(input)?))),
            37 => variant(input, |input| {
                Ok(Field(Box::get(input)?, Ident::get(input)?))
            }),
            38 => variant(input, |input| Ok(Index(Box::get(input)?, Box::get(input)?))),
            39 => variant(input, |input| {
                Ok(Slice {
                    value: Box::get(input)?,
                    start: Option::get(input)?,
                    end: Option::get(input)?,
                    mutable: bool::get(input)?,
                })
            }),
            40 => variant(input, |input| {
                Ok(Borrow(
                    Box::get(input)?,
                    bool::get(input)?,
                    Notation::get(input)?,
                ))
            }),
            41 => variant(input, |input| {
                Ok(Dereference(Box::get(input)?, Notation::get(input)?))
            }),
            42 => variant(input, |input| {
                Ok(Assign(Box::get(input)?, Box::get(input)?))
            }),
            43 => variant(input, |input| {
                Ok(Cast(Box::get(input)?, TypeExpr::get(input)?))
            }),
            44 => variant(input, |input| Ok(Interpolated(Box::get(input)?))),
            45 => variant(input, |input| {
                Ok(DynDispatch {
                    slot: u32::get(input)?,
                    slots: u32::get(input)?,
                })
            }),
            46..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for Interpolation {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Interpolation { texts, holes } = self;
        texts.put(out)?;
        holes.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Interpolation {
            texts: Vec::get(input)?,
            holes: Vec::get(input)?,
        })
    }
}

impl Wire for InterpolationHole {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let InterpolationHole { value, spec } = self;
        value.put(out)?;
        spec.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(InterpolationHole {
            value: Expr::get(input)?,
            spec: Option::get(input)?,
        })
    }
}

impl Wire for FormatSpec {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let FormatSpec {
            fill,
            align,
            plus,
            width,
            precision,
            kind,
            span,
        } = self;
        fill.put(out)?;
        align.put(out)?;
        plus.put(out)?;
        width.put(out)?;
        precision.put(out)?;
        kind.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(FormatSpec {
            fill: char::get(input)?,
            align: Option::get(input)?,
            plus: bool::get(input)?,
            width: u16::get(input)?,
            precision: Option::get(input)?,
            kind: Option::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for TryExpr {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let TryExpr {
            body,
            arms,
            finally,
        } = self;
        body.put(out)?;
        arms.put(out)?;
        finally.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(TryExpr {
            body: Expr::get(input)?,
            arms: Vec::get(input)?,
            finally: Option::get(input)?,
        })
    }
}

impl Wire for MatchArm {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let MatchArm {
            pattern,
            guard,
            body,
            span,
        } = self;
        pattern.put(out)?;
        guard.put(out)?;
        body.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(MatchArm {
            pattern: Pattern::get(input)?,
            guard: Option::get(input)?,
            body: Expr::get(input)?,
            span: Span::get(input)?,
        })
    }
}

impl Wire for Binding {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Binding {
            name,
            mutable,
            using,
            annotation,
            value,
        } = self;
        name.put(out)?;
        mutable.put(out)?;
        using.put(out)?;
        annotation.put(out)?;
        value.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Binding {
            name: Ident::get(input)?,
            mutable: bool::get(input)?,
            using: bool::get(input)?,
            annotation: Option::get(input)?,
            value: Expr::get(input)?,
        })
    }
}

impl Wire for Pattern {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let Pattern { kind, span, depth } = self;
        nest(out, |out| kind.put(out))?;
        span.put(out)?;
        depth.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(Pattern {
            kind: nested(input, PatternKind::get)?,
            span: Span::get(input)?,
            depth: usize::get(input)?,
        })
    }
}

impl Wire for PatternKind {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        use PatternKind::*;
        match self {
            Wildcard => {
                out.tag("PatternKind", 0);
                Ok(())
            }
            Binding(name) => {
                out.tag("PatternKind", 1);
                name.put(out)
            }
            Apply(name, arguments) => {
                out.tag("PatternKind", 2);
                name.put(out)?;
                arguments.put(out)
            }
            Argument(value) => {
                out.tag("PatternKind", 3);
                value.put(out)
            }
            Literal(value) => {
                out.tag("PatternKind", 4);
                value.put(out)
            }
            Tuple(elements) => {
                out.tag("PatternKind", 5);
                elements.put(out)
            }
            Record(name, fields) => {
                out.tag("PatternKind", 6);
                name.put(out)?;
                fields.put(out)
            }
            Array(elements) => {
                out.tag("PatternKind", 7);
                elements.put(out)
            }
            List(elements) => {
                out.tag("PatternKind", 8);
                elements.put(out)
            }
            Cons(head, tail) => {
                out.tag("PatternKind", 9);
                head.put(out)?;
                tail.put(out)
            }
            Or(left, right) => {
                out.tag("PatternKind", 10);
                left.put(out)?;
                right.put(out)
            }
            And(left, right) => {
                out.tag("PatternKind", 11);
                left.put(out)?;
                right.put(out)
            }
            As(pattern, name) => {
                out.tag("PatternKind", 12);
                pattern.put(out)?;
                name.put(out)
            }
            Annotated(pattern, ty) => {
                out.tag("PatternKind", 13);
                pattern.put(out)?;
                ty.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        use PatternKind::*;
        match input.byte()? {
            0 => Ok(Wildcard),
            1 => variant(input, |input| Ok(Binding(Ident::get(input)?))),
            2 => variant(input, |input| {
                Ok(Apply(Ident::get(input)?, Vec::get(input)?))
            }),
            3 => variant(input, |input| Ok(Argument(Box::get(input)?))),
            4 => variant(input, |input| Ok(Literal(Box::get(input)?))),
            5 => variant(input, |input| Ok(Tuple(Vec::get(input)?))),
            6 => variant(input, |input| {
                Ok(Record(Option::get(input)?, Vec::get(input)?))
            }),
            7 => variant(input, |input| Ok(Array(Vec::get(input)?))),
            8 => variant(input, |input| Ok(List(Vec::get(input)?))),
            9 => variant(input, |input| Ok(Cons(Box::get(input)?, Box::get(input)?))),
            10 => variant(input, |input| Ok(Or(Box::get(input)?, Box::get(input)?))),
            11 => variant(input, |input| Ok(And(Box::get(input)?, Box::get(input)?))),
            12 => variant(input, |input| Ok(As(Box::get(input)?, Ident::get(input)?))),
            13 => variant(input, |input| {
                Ok(Annotated(Box::get(input)?, TypeExpr::get(input)?))
            }),
            14..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for ComputationBlock {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ComputationBlock {
            statements,
            span,
            depth,
        } = self;
        nest(out, |out| statements.put(out))?;
        span.put(out)?;
        depth.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ComputationBlock {
            statements: nested(input, Vec::get)?,
            span: Span::get(input)?,
            depth: usize::get(input)?,
        })
    }
}

impl Wire for ComputationStatement {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ComputationStatement { kind, span } = self;
        kind.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ComputationStatement {
            kind: ComputationStatementKind::get(input)?,
            span: Span::get(input)?,
        })
    }
}

/// The builder operations that a `ComputationStatementKind::Operation` can name, by position.
const OPERATIONS: [&str; 15] = [
    "Bind",
    "Return",
    "ReturnFrom",
    "Yield",
    "YieldFrom",
    "Zero",
    "Combine",
    "Delay",
    "Run",
    "For",
    "While",
    "MergeSources",
    "BindReturn",
    "Bind2",
    "Using",
];

impl Wire for ComputationStatementKind {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        use ComputationStatementKind::*;
        match self {
            Let(binding, bang) => {
                out.tag("ComputationStatementKind", 0);
                binding.put(out)?;
                bang.put(out)
            }
            LetAnd(bindings) => {
                out.tag("ComputationStatementKind", 1);
                bindings.put(out)
            }
            Match(value, arms) => {
                out.tag("ComputationStatementKind", 2);
                value.put(out)?;
                arms.put(out)
            }
            Do(value) => {
                out.tag("ComputationStatementKind", 3);
                value.put(out)
            }
            Operation(operation, value) => {
                out.tag("ComputationStatementKind", 4);
                let index = OPERATIONS
                    .iter()
                    .position(|known| known == operation)
                    .ok_or(Invalid)?;
                index.put(out)?;
                value.put(out)
            }
            If(condition, then_block, else_block) => {
                out.tag("ComputationStatementKind", 5);
                condition.put(out)?;
                then_block.put(out)?;
                else_block.put(out)
            }
            For(pattern, source, body) => {
                out.tag("ComputationStatementKind", 6);
                pattern.put(out)?;
                source.put(out)?;
                body.put(out)
            }
            While(condition, body) => {
                out.tag("ComputationStatementKind", 7);
                condition.put(out)?;
                body.put(out)
            }
            Expression(value) => {
                out.tag("ComputationStatementKind", 8);
                value.put(out)
            }
        }
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        use ComputationStatementKind::*;
        match input.byte()? {
            0 => variant(input, |input| {
                Ok(Let(Binding::get(input)?, bool::get(input)?))
            }),
            1 => variant(input, |input| Ok(LetAnd(Vec::get(input)?))),
            2 => variant(input, |input| Ok(Match(Box::get(input)?, Vec::get(input)?))),
            3 => variant(input, |input| Ok(Do(Expr::get(input)?))),
            4 => variant(input, |input| {
                let operation = *OPERATIONS.get(usize::get(input)?).ok_or(Invalid)?;
                Ok(Operation(operation, Expr::get(input)?))
            }),
            5 => variant(input, |input| {
                Ok(If(
                    Expr::get(input)?,
                    ComputationBlock::get(input)?,
                    Option::get(input)?,
                ))
            }),
            6 => variant(input, |input| {
                Ok(For(
                    Box::get(input)?,
                    Expr::get(input)?,
                    ComputationBlock::get(input)?,
                ))
            }),
            7 => variant(input, |input| {
                Ok(While(Expr::get(input)?, ComputationBlock::get(input)?))
            }),
            8 => variant(input, |input| Ok(Expression(Expr::get(input)?))),
            9..=u8::MAX => Err(Invalid),
        }
    }
}

impl Wire for ComputationMatchArm {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid> {
        let ComputationMatchArm {
            pattern,
            guard,
            body,
            span,
        } = self;
        pattern.put(out)?;
        guard.put(out)?;
        body.put(out)?;
        span.put(out)
    }

    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid> {
        Ok(ComputationMatchArm {
            pattern: Pattern::get(input)?,
            guard: Option::get(input)?,
            body: ComputationBlock::get(input)?,
            span: Span::get(input)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_with_source_all;
    use std::path::Path;

    fn round_trip<T: Wire>(value: &T, source: usize, into: usize) -> Result<T, Invalid> {
        let mut out = Writer::new(Mode::Full, source, MAX_DEPTH);
        value.put(&mut out)?;
        let mut input = Reader {
            bytes: &out.bytes,
            at: 0,
            source: into,
            depth: 0,
        };
        let value = T::get(&mut input)?;
        assert_eq!(input.at, out.bytes.len());
        Ok(value)
    }

    fn read<T: Wire>(bytes: &[u8]) -> Result<T, Invalid> {
        let mut input = Reader {
            bytes,
            at: 0,
            source: 0,
            depth: 0,
        };
        T::get(&mut input)
    }

    /// Encodes the program that `text` parses to and checks that it decodes to the same tree.
    /// Returns the variants written, or `None` when `text` does not parse.
    fn round_trips(
        text: &str,
        source: usize,
    ) -> Option<std::collections::BTreeSet<(&'static str, u8)>> {
        let program = parse_with_source_all(text, source).ok()?;
        let mut out = Writer::new(Mode::Full, source, MAX_DEPTH);
        program.put(&mut out).expect("a parsed program encodes");
        let decoded = decode(&out.bytes, source).expect("an encoded program decodes");
        assert_eq!(
            format!("{decoded:?}"),
            format!("{program:?}"),
            "round trip changed the tree of:\n{text}"
        );
        Some(out.seen)
    }

    #[test]
    fn round_trips_primitives() {
        for (value, length) in [(0u64, 1), (127, 1), (128, 2), (u64::MAX, 10)] {
            let mut out = Writer::new(Mode::Full, 0, MAX_DEPTH);
            value.put(&mut out).unwrap();
            assert_eq!(out.bytes.len(), length, "{value}");
            assert_eq!(round_trip(&value, 0, 0), Ok(value));
        }
        assert_eq!(round_trip(&u128::MAX, 0, 0), Ok(u128::MAX));
        for text in ["", "ascii", "日本語 ✓ 🎉"] {
            assert_eq!(round_trip(&text.to_owned(), 0, 0), Ok(text.to_owned()));
        }
        for value in [None, Some(false), Some(true)] {
            assert_eq!(round_trip(&value, 0, 0), Ok(value));
        }
        assert_eq!(round_trip(&'✓', 0, 0), Ok('✓'));
        assert_eq!(read::<bool>(&[2]), Err(Invalid));
        // A length beyond the remaining bytes is corrupt before anything is allocated.
        assert_eq!(read::<Vec<bool>>(&[5, 1]), Err(Invalid));
        assert_eq!(
            read::<String>(&[0xff, 0xff, 0xff, 0xff, 0x0f]),
            Err(Invalid)
        );
        assert_eq!(read::<String>(&[2, 0xc3, 0x28]), Err(Invalid));
        // Truncated, overlong and oversized numbers.
        assert_eq!(read::<u64>(&[0x80]), Err(Invalid));
        assert_eq!(read::<u64>(&[0x80, 0x00]), Err(Invalid));
        assert_eq!(read::<u8>(&[0x80, 0x02]), Err(Invalid));
        assert_eq!(read::<u64>(&[0xff; 11]), Err(Invalid));
        assert_eq!(read::<char>(&[0x80, 0xb0, 0x03]), Err(Invalid));
    }

    #[test]
    fn round_trips_spans_into_another_source() {
        let own = Span {
            start: 1,
            end: 4,
            source: Some(3),
        };
        let none = Span::new(0, 0);
        assert_eq!(round_trip(&own, 3, 3), Ok(own));
        assert_eq!(round_trip(&own, 3, 5), Ok(own.in_source(5)));
        assert_eq!(round_trip(&none, 3, 5), Ok(none));
    }

    #[test]
    fn round_trips_every_std_source() {
        for (path, text) in crate::stdlib::SOURCES {
            assert!(round_trips(text, 7).is_some(), "{path} does not parse");
        }
    }

    fn sources(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                sources(&path, found);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| SourceKind::from_extension(extension).is_some())
            {
                found.push(path);
            }
        }
    }

    #[test]
    fn round_trips_fixture_and_example_sources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut paths = Vec::new();
        sources(&root.join("tests/fixtures"), &mut paths);
        sources(&root.join("examples"), &mut paths);
        let mut parsed = 0;
        for path in &paths {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            parsed += usize::from(round_trips(&text, 1).is_some());
        }
        assert!(
            parsed > 100,
            "only {parsed} of {} sources parse",
            paths.len()
        );
    }

    /// The language reference's examples, the std sources, the fixtures and the examples write
    /// every variant that the parser produces; the rest come from hand-built programs.
    #[test]
    fn round_trips_every_variant() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut pages = Vec::new();
        let mut pending = vec![root.join("_tsuzuri/language-reference")];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "md") {
                    pages.push(path);
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut blocks = 0;
        for page in pages {
            let text = std::fs::read_to_string(&page).unwrap();
            let mut lines = text.lines();
            while let Some(line) = lines.next() {
                if !line.starts_with("```tsuzuri") {
                    continue;
                }
                let block: Vec<_> = lines
                    .by_ref()
                    .take_while(|line| !line.starts_with("```"))
                    .collect();
                if let Some(variants) = round_trips(&(block.join("\n") + "\n"), 2) {
                    blocks += 1;
                    seen.extend(variants);
                }
            }
        }
        assert!(blocks > 100, "only {blocks} examples parse");
        for (_, text) in crate::stdlib::SOURCES {
            seen.extend(round_trips(text, 0).unwrap());
        }
        let mut paths = Vec::new();
        sources(&root.join("tests/fixtures"), &mut paths);
        sources(&root.join("examples"), &mut paths);
        for path in paths {
            if let Ok(text) = std::fs::read_to_string(path) {
                seen.extend(round_trips(&text, 0).unwrap_or_default());
            }
        }
        // Array patterns, active pattern arguments and unary plus appear only in the Rust tests.
        let patterns = "def (|Above|_|) :: &i64 -> i64 -> bool\nfn (|Above|_|) limit n = n > *limit\n\
            def f :: i64 -> i64\nfn f n = { let limit = 3; match n with | Above (&limit) -> 1 | _ -> 0 }\n\
            def g :: [i64] -> i64 = \\xs ->\n    match xs with\n    | [x; y] -> +x + y\n    | _ -> 0\n";
        seen.extend(round_trips(patterns, 0).expect("the pattern sample parses"));
        // Only the checker, the computation expansion and `Bits.ushr` make these variants.
        let mut program = parse_with_source_all("def f :: i64 = 1\n", 0).unwrap();
        let body = program.functions[0].body.clone();
        let name = Ident {
            provenance: Provenance::Generated,
            ..program.functions[0].name.clone()
        };
        let made = |kind| Expr {
            kind,
            ..body.clone()
        };
        program.functions[0].body.kind = ExprKind::Tuple(vec![
            made(ExprKind::DynDispatch { slot: 3, slots: 7 }),
            made(ExprKind::QualifiedFunction(name)),
            made(ExprKind::ComputationBoundary(Box::new(body.clone()))),
            made(ExprKind::Binary(
                BinaryOp::ShiftRightUnsigned,
                Box::new(body.clone()),
                Box::new(body.clone()),
            )),
            made(ExprKind::Match {
                value: Box::new(body.clone()),
                arms: Vec::new(),
                origin: MatchOrigin::ComputationDestructuring,
            }),
        ]);
        let mut out = Writer::new(Mode::Full, 0, MAX_DEPTH);
        program.put(&mut out).unwrap();
        assert_eq!(
            format!("{:?}", decode(&out.bytes, 0).unwrap()),
            format!("{program:?}")
        );
        seen.extend(out.seen);
        for (name, count) in [
            ("ConstraintName", 2),
            ("Kind", 2),
            ("TypeExprKind", 15),
            ("StringLiteral", 2),
            ("ExprKind", 46),
            ("PatternKind", 14),
            ("ComputationStatementKind", 9),
            ("SourceKind", 0),
            ("Provenance", 2),
            ("Visibility", 2),
            ("DeriveClass", 7),
            ("UnaryOp", 4),
            ("BinaryOp", 21),
            ("MatchOrigin", 4),
            ("Notation", 2),
            ("FormatAlign", 3),
            ("FormatKind", 6),
        ] {
            let missing: Vec<_> = (0..count)
                .filter(|tag| !seen.contains(&(name, *tag)))
                .collect();
            assert!(
                missing.is_empty(),
                "no example writes {name} tags {missing:?}"
            );
        }
    }

    /// The largest `count` up to 200 for which `make(count)` parses.
    fn deepest(make: impl Fn(usize) -> String) -> String {
        (0..=200)
            .rev()
            .map(make)
            .find(|text| parse_with_source_all(text, 0).is_ok())
            .expect("some depth parses")
    }

    #[test]
    fn round_trips_deepest_accepted_nesting() {
        let mut texts = vec![
            deepest(|n| format!("fn f() -> i64 {{ {}1{} }}", "(".repeat(n), ")".repeat(n))),
            deepest(|n| format!("fn f() -> i64 {{ {}1 }}", "1 + ".repeat(n))),
            deepest(|n| {
                format!(
                    "fn f() -> i64 {{ {} {{ 1 }} }}",
                    "if true { 0 } else ".repeat(n)
                )
            }),
            deepest(|n| format!("type T = {}i64{}\n", "[".repeat(n), "]".repeat(n))),
            deepest(|n| {
                format!(
                    "def f :: i64 -> i64 = \\x ->\n    match x with\n    | {}y{} -> y\n",
                    "(".repeat(n),
                    ")".repeat(n)
                )
            }),
        ];
        for (prefix, suffix) in [
            ("Module.f(", ")"),
            ("values[", "]"),
            ("R { x: ", "}"),
            ("Module.R { x: ", "}"),
            ("[", "]"),
            ("[|", "|]"),
            ("new [i64](1, i -> ", ")"),
            ("new [|i64|](1, i -> ", ")"),
            ("{ value with x = ", " }"),
            ("- ", ""),
        ] {
            texts.push(deepest(|n| {
                format!(
                    "fn f() -> i64 {{ {}0{} }}",
                    prefix.repeat(n),
                    suffix.repeat(n)
                )
            }));
        }
        for text in texts {
            assert!(round_trips(&text, 4).is_some());
        }
    }

    const SMALL: &str = "/// A point.\nrecord P { x: i64 }\n\ndef f :: i64 -> i64 = \\x -> x + 1\n\ntest \"f\" = f 1 == 2\n\nf 2\n";

    #[test]
    fn rejects_every_truncation() {
        let program = parse_with_source_all(SMALL, 0).unwrap();
        let bytes = encode(&program, 0, Mode::Full).unwrap();
        for length in 0..bytes.len() {
            assert_eq!(decode(&bytes[..length], 0).err(), Some(Invalid), "{length}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode(&longer, 0).err(), Some(Invalid));
    }

    #[test]
    fn rejects_flipped_bytes() {
        let program = parse_with_source_all(SMALL, 0).unwrap();
        let bytes = encode(&program, 0, Mode::Full).unwrap();
        for index in 0..bytes.len() {
            for mask in [0x01, 0x80, 0xff] {
                let mut flipped = bytes.clone();
                flipped[index] ^= mask;
                // Any result but a panic or an unbounded allocation is acceptable.
                let _ = decode(&flipped, 0);
            }
        }
    }

    #[test]
    fn rejects_excessive_depth() {
        let mut program = parse_with_source_all("def f :: i64 = 1\n", 0).unwrap();
        let mut ty = TypeExpr {
            kind: TypeExprKind::Named("i64".into()),
            span: Span::default(),
        };
        for _ in 0..600 {
            ty = TypeExpr {
                kind: TypeExprKind::Array(Box::new(ty)),
                span: Span::default(),
            };
        }
        program.dyn_types.push(ty);
        assert_eq!(encode(&program, 0, Mode::Full).err(), Some(Invalid));
        let bytes = encode_within(&program, 0, Mode::Full, u32::MAX).unwrap();
        assert_eq!(decode(&bytes, 0).err(), Some(Invalid));
    }

    #[test]
    fn rejects_foreign_source_spans() {
        let program = parse_with_source_all("def f :: i64 = 1\n", 2).unwrap();
        assert!(encode(&program, 2, Mode::Full).is_ok());
        assert_eq!(encode(&program, 1, Mode::Full).err(), Some(Invalid));
    }

    fn interface(text: &str) -> [u8; 32] {
        let program =
            parse_with_source_all(text, 0).unwrap_or_else(|errors| panic!("{text}\n{errors:?}"));
        crate::frontend_cache::interface_hash(&[1; 32], &program, 0).unwrap()
    }

    const BASE: &str = "/// Adds one.
def add_one :: i64 -> i64 = \\x -> x + 1

record Point { x: i64, y: i64 }

record Pair { left: i64, right: i64 }

union Shape = Dot | Circle of i64

const Limit: i64 = 10

class Size<'value> {
    def size :: ref 'value -> i64
    def twice :: ref 'value -> i64 = \\value -> Size.size value * 2
}

instance Size<Point> {
    fn size point = point.x
}

test \"adds\" = add_one 1 == 2

add_one 41
";

    /// Replaces the only occurrence of `from` in `BASE`.
    fn edited(from: &str, to: &str) -> String {
        assert_eq!(BASE.matches(from).count(), 1, "{from}");
        BASE.replacen(from, to, 1)
    }

    #[test]
    fn interface_ignores_bodies_docs_and_positions() {
        let base = interface(BASE);
        for text in [
            edited("\\x -> x + 1", "\\x -> 1 + x"),
            edited("/// Adds one.", "/// Increments."),
            format!("\n\n{BASE}"),
            edited("add_one 1 == 2", "add_one 2 == 3"),
            edited("add_one 41\n", "add_one 1\n"),
            edited("fn size point = point.x", "fn size point = point.y"),
        ] {
            assert_eq!(interface(&text), base, "{text}");
        }
    }

    #[test]
    fn interface_changes_with_headers() {
        let base = interface(BASE);
        for text in [
            edited("def add_one :: i64 -> i64", "def add_one :: i32 -> i64"),
            edited("def add_one :: i64 -> i64", "def add_one :: i64 -> i32"),
            edited("def add_one", "private def add_one"),
            edited(
                "record Point { x: i64, y: i64 }",
                "record Point { x: i64, z: i64 }",
            ),
            edited("Dot | Circle of i64", "Dot | Circle of i64 | Square of i64"),
            edited("Size.size value * 2", "Size.size value * 3"),
            edited("const Limit: i64 = 10", "const Limit: i64 = 11"),
            // `Program` keeps the order within each kind of declaration.
            edited(
                "record Point { x: i64, y: i64 }\n\nrecord Pair { left: i64, right: i64 }",
                "record Pair { left: i64, right: i64 }\n\nrecord Point { x: i64, y: i64 }",
            ),
        ] {
            assert_ne!(interface(&text), base, "{text}");
        }
    }

    #[test]
    fn pins_encoding_format() {
        // A failure here means the encoding changed: bump `FRONTEND_FORMAT` and update the golden.
        let program = parse_with_source_all(SMALL, 0).unwrap();
        let hex: String = encode(&program, 0, Mode::Full)
            .unwrap()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hex, GOLDEN);
    }

    const GOLDEN: &str = concat!(
        "000000000000000101084120706f696e742e000c0100015014150100000001017818190100000369",
        "36341b1e0100000000010001662627010000000000010178393a010000036936342b2e0100000003",
        "693634323501000e000a01783e3f01003e3f0101000100424301013e4301020000000101664a4d01",
        "0e050f0a016650510100505101010100010052530101505301020002005758010150580103455801",
        "00011c000f0a01665a5b01005a5b0101010002005c5d01015a5d01025a5d01030000",
    );
}
