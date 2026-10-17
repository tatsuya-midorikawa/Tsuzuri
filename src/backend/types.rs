//! The types of the textual IR and their layout on the 64-bit little-endian targets.
//!
//! A scalar is a plain `Ty` value; structs, arrays and vectors are interned once in a table. The
//! module's table is built first and is then shared by the workers, which add the few anonymous
//! types of a function body to a private overlay, so interning never takes a lock.

use super::error::{Failure, FxMap, Res, bail};
use super::lex::Lexer;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Ty {
    Void,
    /// `iN`, 1 to 128 bits.
    Int(u16),
    Half,
    Float,
    Double,
    /// `ptr` in the default address space.
    Ptr,
    /// A struct, array or vector of the type table.
    Compound(u32),
    Label,
    Metadata,
    /// A type the translator does not lower: an index into `OTHER_NAMES`.
    Other(u8),
}

/// The names of the types that `Ty::Other` stands for, in index order.
const OTHER_NAMES: [&str; 7] = [
    "bfloat",
    "x86_fp80",
    "fp128",
    "ppc_fp128",
    "x86_amx",
    "token",
    "an integer wider than 128 bits",
];
const OPAQUE: u8 = 7;

pub(crate) const I1: Ty = Ty::Int(1);
pub(crate) const I8: Ty = Ty::Int(8);
pub(crate) const I32: Ty = Ty::Int(32);
pub(crate) const I64: Ty = Ty::Int(64);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Compound {
    Struct { fields: Box<[Ty]>, packed: bool },
    Array { len: u64, elem: Ty },
    Vector { len: u32, elem: Ty },
}

#[derive(Debug)]
pub(crate) struct Entry {
    pub shape: Compound,
    /// The distance between two elements in an array: the size rounded up to the alignment.
    pub size: u64,
    /// The bytes a load or store covers.
    pub store_size: u64,
    pub align: u32,
    /// The scalars the type flattens to; the lanes of a vector count one each.
    pub leaves: u64,
    /// The offset of each field of a struct.
    pub offsets: Box<[u64]>,
}

const TOO_LARGE: u64 = 1 << 40;

#[derive(Default)]
pub(crate) struct TypeTable<'a> {
    entries: Vec<Entry>,
    map: FxMap<Compound, u32>,
    named: FxMap<&'a str, Ty>,
}

/// The type table of one thread: the shared module table, if any, plus what this thread added.
pub(crate) struct Types<'a> {
    base: Option<&'a TypeTable<'a>>,
    base_len: u32,
    own: TypeTable<'a>,
    /// The definitions of named types that no use has needed yet, with the offset of their body.
    pending: FxMap<&'a str, usize>,
    in_progress: Vec<&'a str>,
    text: &'a str,
}

fn round_up(value: u64, align: u64) -> u64 {
    value.div_ceil(align) * align
}

pub(crate) fn int_store_size(bits: u16) -> u64 {
    u64::from(bits).div_ceil(8)
}

pub(crate) fn int_align(bits: u16) -> u32 {
    match bits {
        0..=8 => 1,
        9..=16 => 2,
        17..=32 => 4,
        33..=64 => 8,
        _ => 16,
    }
}

impl<'a> Types<'a> {
    /// The builder of the module's table; `text` is the module the named types are read from.
    pub(crate) fn builder(text: &'a str) -> Self {
        Self {
            base: None,
            base_len: 0,
            own: TypeTable::default(),
            pending: FxMap::default(),
            in_progress: Vec::new(),
            text,
        }
    }

    /// A thread's view of the finished module table.
    pub(crate) fn overlay(base: &'a TypeTable<'a>, text: &'a str) -> Self {
        Self {
            base: Some(base),
            base_len: u32::try_from(base.entries.len()).expect("type table size"),
            own: TypeTable::default(),
            pending: FxMap::default(),
            in_progress: Vec::new(),
            text,
        }
    }

    pub(crate) fn finish(self) -> TypeTable<'a> {
        self.own
    }

    fn entry(&self, id: u32) -> &Entry {
        if id < self.base_len {
            &self.base.expect("an overlay has a base").entries[id as usize]
        } else {
            &self.own.entries[(id - self.base_len) as usize]
        }
    }

    pub(crate) fn shape(&self, ty: Ty) -> Option<&Compound> {
        match ty {
            Ty::Compound(id) => Some(&self.entry(id).shape),
            _ => None,
        }
    }

    /// The fields of a struct type, or the element of an array or vector as one repeated field.
    pub(crate) fn struct_fields(&self, ty: Ty) -> Option<&[Ty]> {
        match self.shape(ty)? {
            Compound::Struct { fields, .. } => Some(fields),
            _ => None,
        }
    }

    pub(crate) fn intern(&mut self, shape: Compound) -> Res<Ty> {
        if let Some(base) = self.base
            && let Some(&id) = base.map.get(&shape)
        {
            return Ok(Ty::Compound(id));
        }
        if let Some(&id) = self.own.map.get(&shape) {
            return Ok(Ty::Compound(id + self.base_len));
        }
        let entry = self.measure(shape)?;
        let id = u32::try_from(self.own.entries.len()).expect("type table size");
        self.own.map.insert(entry.shape.clone(), id);
        self.own.entries.push(entry);
        Ok(Ty::Compound(id + self.base_len))
    }

    fn measure(&self, shape: Compound) -> Res<Entry> {
        let (size, store_size, align, leaves, offsets) = match &shape {
            Compound::Struct { fields, packed } => {
                let mut offset = 0u64;
                let mut align = 1u32;
                let mut leaves = 0u64;
                let mut offsets = Vec::with_capacity(fields.len());
                for &field in fields.iter() {
                    let field_align = if *packed { 1 } else { self.align_of(field)? };
                    offset = round_up(offset, u64::from(field_align));
                    offsets.push(offset);
                    offset = offset.saturating_add(self.size_of(field)?);
                    align = align.max(field_align);
                    leaves = leaves.saturating_add(self.leaves(field)?);
                }
                let size = round_up(offset, u64::from(align));
                (size, size, align, leaves, offsets)
            }
            Compound::Array { len, elem } => {
                let size = self.size_of(*elem)?.saturating_mul(*len);
                (
                    size,
                    size,
                    self.align_of(*elem)?,
                    self.leaves(*elem)?.saturating_mul(*len),
                    Vec::new(),
                )
            }
            Compound::Vector { len, elem } => {
                let bits = match *elem {
                    Ty::Int(bits) => u64::from(bits),
                    Ty::Half => 16,
                    Ty::Float => 32,
                    Ty::Double | Ty::Ptr => 64,
                    other => bail!("a vector of {}", self.describe(other)),
                };
                let store = (bits * u64::from(*len)).div_ceil(8);
                let align = u32::try_from(store.max(1).next_power_of_two()).unwrap_or(u32::MAX);
                (
                    round_up(store, u64::from(align)),
                    store,
                    align,
                    u64::from(*len),
                    Vec::new(),
                )
            }
        };
        if size > TOO_LARGE {
            bail!("a type of {size} bytes");
        }
        Ok(Entry {
            shape,
            size,
            store_size,
            align,
            leaves,
            offsets: offsets.into_boxed_slice(),
        })
    }

    pub(crate) fn describe(&self, ty: Ty) -> String {
        match ty {
            Ty::Void => "void".into(),
            Ty::Int(bits) => format!("i{bits}"),
            Ty::Half => "half".into(),
            Ty::Float => "float".into(),
            Ty::Double => "double".into(),
            Ty::Ptr => "ptr".into(),
            Ty::Label => "label".into(),
            Ty::Metadata => "metadata".into(),
            Ty::Other(index) => OTHER_NAMES
                .get(usize::from(index))
                .map_or("an opaque type", |name| *name)
                .into(),
            Ty::Compound(id) => match &self.entry(id).shape {
                Compound::Struct { fields, packed } => {
                    let inner: Vec<String> = fields.iter().map(|&f| self.describe(f)).collect();
                    if *packed {
                        format!("<{{ {} }}>", inner.join(", "))
                    } else {
                        format!("{{ {} }}", inner.join(", "))
                    }
                }
                Compound::Array { len, elem } => format!("[{len} x {}]", self.describe(*elem)),
                Compound::Vector { len, elem } => format!("<{len} x {}>", self.describe(*elem)),
            },
        }
    }

    /// The distance between two values of the type in an array.
    pub(crate) fn size_of(&self, ty: Ty) -> Res<u64> {
        Ok(match ty {
            Ty::Int(bits) => round_up(int_store_size(bits), u64::from(int_align(bits))),
            Ty::Half => 2,
            Ty::Float => 4,
            Ty::Double | Ty::Ptr => 8,
            Ty::Compound(id) => self.entry(id).size,
            other => bail!("the size of {}", self.describe(other)),
        })
    }

    /// The bytes that a load or store of the type covers.
    pub(crate) fn store_size_of(&self, ty: Ty) -> Res<u64> {
        Ok(match ty {
            Ty::Int(bits) => int_store_size(bits),
            Ty::Compound(id) => self.entry(id).store_size,
            other => self.size_of(other)?,
        })
    }

    pub(crate) fn align_of(&self, ty: Ty) -> Res<u32> {
        Ok(match ty {
            Ty::Int(bits) => int_align(bits),
            Ty::Half => 2,
            Ty::Float => 4,
            Ty::Double | Ty::Ptr => 8,
            Ty::Compound(id) => self.entry(id).align,
            other => bail!("the alignment of {}", self.describe(other)),
        })
    }

    /// The number of scalars the type flattens to.
    pub(crate) fn leaves(&self, ty: Ty) -> Res<u64> {
        Ok(match ty {
            Ty::Void => 0,
            Ty::Int(_) | Ty::Half | Ty::Float | Ty::Double | Ty::Ptr => 1,
            Ty::Compound(id) => self.entry(id).leaves,
            other => bail!("a value of type {}", self.describe(other)),
        })
    }

    pub(crate) fn field_offset(&self, ty: Ty, index: usize) -> Option<u64> {
        match ty {
            Ty::Compound(id) => self.entry(id).offsets.get(index).copied(),
            _ => None,
        }
    }

    /// The scalar types of the leaves of `ty`, in memory order, with their offsets from `base`.
    pub(crate) fn leaf_layout(&self, ty: Ty, base: u64, out: &mut Vec<(u64, Ty)>) -> Res<()> {
        match ty {
            Ty::Int(_) | Ty::Half | Ty::Float | Ty::Double | Ty::Ptr => out.push((base, ty)),
            Ty::Void => {}
            Ty::Compound(id) => match &self.entry(id).shape {
                Compound::Struct { fields, .. } => {
                    let offsets = &self.entry(id).offsets;
                    for (index, &field) in fields.iter().enumerate() {
                        self.leaf_layout(field, base + offsets[index], out)?;
                    }
                }
                Compound::Array { len, elem } => {
                    let stride = self.size_of(*elem)?;
                    for index in 0..*len {
                        self.leaf_layout(*elem, base + index * stride, out)?;
                    }
                }
                Compound::Vector { len, elem } => {
                    let bits = match *elem {
                        Ty::Int(bits) => u32::from(bits),
                        Ty::Half => 16,
                        Ty::Float => 32,
                        _ => 64,
                    };
                    if bits % 8 != 0 {
                        bail!("a vector of {} in memory", self.describe(*elem));
                    }
                    for index in 0..u64::from(*len) {
                        out.push((base + index * u64::from(bits / 8), *elem));
                    }
                }
            },
            other => bail!("a value of type {}", self.describe(other)),
        }
        Ok(())
    }

    /// Records the definition of a named type; its body is read when a use first needs it.
    pub(crate) fn declare_named(&mut self, name: &'a str, body: usize) {
        self.pending.insert(name, body);
    }

    /// Reads every pending definition, so that the table is complete before workers share it.
    pub(crate) fn resolve_all(&mut self, order: &[&'a str]) -> Res<()> {
        for &name in order {
            self.named_type(name)?;
        }
        Ok(())
    }

    fn named_type(&mut self, name: &str) -> Res<Ty> {
        if let Some(&ty) = self.own.named.get(name) {
            return Ok(ty);
        }
        if let Some(base) = self.base
            && let Some(&ty) = base.named.get(name)
        {
            return Ok(ty);
        }
        let Some((&key, &body)) = self.pending.get_key_value(name) else {
            bail!("the unknown type %{name}");
        };
        if self.in_progress.contains(&key) {
            bail!("the recursive type %{name}");
        }
        self.in_progress.push(key);
        let mut lexer = Lexer::new(self.text, body);
        let ty = if lexer.eat_word("opaque") {
            Ok(Ty::Other(OPAQUE))
        } else {
            self.parse_type(&mut lexer)
        };
        self.in_progress.pop();
        let ty = ty?;
        self.own.named.insert(key, ty);
        Ok(ty)
    }

    pub(crate) fn parse_type(&mut self, lex: &mut Lexer<'a>) -> Res<Ty> {
        lex.skip_ws();
        match lex.peek() {
            b'%' => {
                lex.bump();
                let name = lex.name()?;
                self.named_type(name)
            }
            b'{' => {
                lex.bump();
                self.parse_struct(lex, false)
            }
            b'[' => {
                lex.bump();
                let len = lex.uint()?;
                lex.expect_word("x")?;
                let elem = self.parse_type(lex)?;
                lex.expect(b']')?;
                self.intern(Compound::Array { len, elem })
            }
            b'<' => {
                lex.bump();
                if lex.eat(b'{') {
                    let ty = self.parse_struct(lex, true)?;
                    lex.expect(b'>')?;
                    return Ok(ty);
                }
                if lex.eat_word("vscale") {
                    bail!("a scalable vector near {}", lex.context());
                }
                let len = lex.uint()?;
                lex.expect_word("x")?;
                let elem = self.parse_type(lex)?;
                lex.expect(b'>')?;
                let len = u32::try_from(len).map_err(|_| Failure::new("a vector this long"))?;
                self.intern(Compound::Vector { len, elem })
            }
            _ => {
                let word = lex.word();
                Ok(match word {
                    "void" => Ty::Void,
                    "float" => Ty::Float,
                    "double" => Ty::Double,
                    "half" => Ty::Half,
                    "ptr" => {
                        lex.skip_ws();
                        if lex.peek_word() == "addrspace" {
                            lex.word();
                            lex.expect(b'(')?;
                            let space = lex.uint()?;
                            lex.expect(b')')?;
                            if space != 0 {
                                bail!("a pointer in address space {space}");
                            }
                        }
                        Ty::Ptr
                    }
                    "label" => Ty::Label,
                    "metadata" => Ty::Metadata,
                    "bfloat" => Ty::Other(0),
                    "x86_fp80" => Ty::Other(1),
                    "fp128" => Ty::Other(2),
                    "ppc_fp128" => Ty::Other(3),
                    "x86_amx" => Ty::Other(4),
                    "token" => Ty::Other(5),
                    _ => {
                        let digits = word.strip_prefix('i').unwrap_or("");
                        match digits.parse::<u32>() {
                            Ok(bits @ 1..=128) if !digits.is_empty() => Ty::Int(bits as u16),
                            Ok(_) if !digits.is_empty() => Ty::Other(6),
                            _ => bail!("the type '{}' near {}", word, lex.context()),
                        }
                    }
                })
            }
        }
    }

    fn parse_struct(&mut self, lex: &mut Lexer<'a>, packed: bool) -> Res<Ty> {
        let mut fields = Vec::new();
        if !lex.eat(b'}') {
            loop {
                fields.push(self.parse_type(lex)?);
                if lex.eat(b',') {
                    continue;
                }
                lex.expect(b'}')?;
                break;
            }
        }
        self.intern(Compound::Struct {
            fields: fields.into_boxed_slice(),
            packed,
        })
    }
}

impl<'a> TypeTable<'a> {
    pub(crate) fn named_count(&self) -> usize {
        self.named.len()
    }
}
