//! The body of one function: the instructions of the textual IR as compact records.
//!
//! Values and blocks are numbered at their first mention, so a use that comes before its
//! definition (a phi's operand from a later block) needs no second pass. The records keep what
//! lowering needs and drop what it does not: flags, alignments of non-memory operations, metadata.

use super::error::{Failure, Res, bail};
use super::lex::Lexer;
use super::module::{Ext, Header, Module};
use super::types::{Compound, Ty, Types};

pub(crate) const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Op {
    Local(u32),
    Const(u32),
}

/// A constant operand or initializer.
#[derive(Clone, Debug)]
pub(crate) enum Const {
    Int(i128),
    /// The bits of a `double`; a `float` constant is converted when it is used.
    Float(u64),
    /// `zeroinitializer`, `undef` and `poison`: every bit is zero.
    Zero,
    /// A pointer: a symbol (or null) and a byte offset.
    Addr {
        sym: Option<u32>,
        offset: i64,
    },
    /// The elements of a struct, array or vector, one per field in order.
    Agg(Vec<Const>),
    /// `c"..."`.
    Bytes(Vec<u8>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    UDiv,
    SDiv,
    URem,
    SRem,
    Shl,
    LShr,
    AShr,
    And,
    Or,
    Xor,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CastOp {
    Trunc,
    ZExt,
    SExt,
    FpTrunc,
    FpExt,
    FpToUi,
    FpToSi,
    UiToFp,
    SiToFp,
    PtrToInt,
    IntToPtr,
    BitCast,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IPred {
    Eq,
    Ne,
    Ugt,
    Uge,
    Ult,
    Ule,
    Sgt,
    Sge,
    Slt,
    Sle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FPred {
    False,
    Oeq,
    Ogt,
    Oge,
    Olt,
    Ole,
    One,
    Ord,
    Ueq,
    Ugt,
    Uge,
    Ult,
    Ule,
    Une,
    Uno,
    True,
}

/// The memory orderings; the translator gives every atomic the strongest one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ordering {
    NotAtomic,
    Atomic,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RmwOp {
    Xchg,
    Add,
    Sub,
    And,
    Nand,
    Or,
    Xor,
    Max,
    Min,
    UMax,
    UMin,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Callee<'a> {
    Direct(u32),
    Intrinsic(&'a str),
    Indirect(Op),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CallArg {
    pub ty: Ty,
    pub op: Op,
    pub byval: Option<Ty>,
    /// `sret(T)`: the pointer is where the callee puts its result.
    pub sret: bool,
    pub ext: Ext,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Inst<'a> {
    Bin {
        dst: u32,
        op: BinOp,
        ty: Ty,
        a: Op,
        b: Op,
    },
    FNeg {
        dst: u32,
        ty: Ty,
        a: Op,
    },
    ICmp {
        dst: u32,
        pred: IPred,
        ty: Ty,
        a: Op,
        b: Op,
    },
    FCmp {
        dst: u32,
        pred: FPred,
        ty: Ty,
        a: Op,
        b: Op,
    },
    Cast {
        dst: u32,
        op: CastOp,
        from: Ty,
        to: Ty,
        a: Op,
    },
    Select {
        dst: u32,
        cond_ty: Ty,
        cond: Op,
        ty: Ty,
        a: Op,
        b: Op,
    },
    Phi {
        dst: u32,
        ty: Ty,
        start: u32,
        len: u32,
    },
    Alloca {
        dst: u32,
        ty: Ty,
        count: u64,
        align: u32,
    },
    Load {
        dst: u32,
        ty: Ty,
        ptr: Op,
        atomic: Ordering,
    },
    Store {
        ty: Ty,
        val: Op,
        ptr: Op,
        atomic: Ordering,
    },
    Gep {
        dst: u32,
        ty: Ty,
        base: Op,
        start: u32,
        len: u32,
    },
    ExtractValue {
        dst: u32,
        ty: Ty,
        agg: Op,
        start: u32,
        len: u32,
    },
    InsertValue {
        dst: u32,
        ty: Ty,
        agg: Op,
        val_ty: Ty,
        val: Op,
        start: u32,
        len: u32,
    },
    ExtractElement {
        dst: u32,
        ty: Ty,
        vec: Op,
        idx_ty: Ty,
        idx: Op,
    },
    InsertElement {
        dst: u32,
        ty: Ty,
        vec: Op,
        elem: Op,
        idx_ty: Ty,
        idx: Op,
    },
    ShuffleVector {
        dst: u32,
        ty: Ty,
        a: Op,
        b: Op,
        start: u32,
        len: u32,
    },
    Call {
        dst: u32,
        ret: Ty,
        callee: Callee<'a>,
        start: u32,
        len: u32,
        variadic: bool,
    },
    AtomicRmw {
        dst: u32,
        op: RmwOp,
        ty: Ty,
        ptr: Op,
        val: Op,
    },
    CmpXchg {
        dst: u32,
        ty: Ty,
        ptr: Op,
        cmp: Op,
        new: Op,
    },
    Fence,
    Freeze {
        dst: u32,
        ty: Ty,
        a: Op,
    },
    Unreachable,
    Ret {
        ty: Ty,
        val: Op,
    },
    Br {
        target: u32,
    },
    CondBr {
        cond: Op,
        then_block: u32,
        else_block: u32,
    },
    Switch {
        ty: Ty,
        cond: Op,
        default: u32,
        start: u32,
        len: u32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    /// The instructions `first..last` of the function, the terminator last.
    pub first: u32,
    pub last: u32,
    pub defined: bool,
}

pub(crate) struct Func<'a> {
    pub header: Header<'a>,
    pub n_locals: u32,
    /// The type of each local value, known once its definition has been read.
    pub local_ty: Vec<Ty>,
    pub blocks: Vec<Block>,
    pub insts: Vec<Inst<'a>>,
    pub consts: Vec<Const>,
    pub call_args: Vec<CallArg>,
    pub phi_in: Vec<(Op, u32)>,
    pub gep_idx: Vec<(Ty, Op)>,
    pub indices: Vec<u32>,
    pub cases: Vec<(i128, u32)>,
    pub masks: Vec<i32>,
    /// With `-g`: the metadata id of the `!dbg` location of each instruction, `NONE` where it has none.
    pub dbg: Vec<u32>,
    /// With `-g`: the metadata id of the function's `!dbg` subprogram, or `NONE`.
    pub subprogram: u32,
}

/// What a constant needs to be read: the module's symbols and the thread's types.
pub(crate) struct Cx<'a> {
    pub module: &'a Module<'a>,
    pub types: Types<'a>,
}

/// Whether the word starts a constant, so that it is not an attribute of an argument.
pub(crate) fn is_value_word(word: &str) -> bool {
    VALUE_WORDS.contains(&word)
}

const VALUE_WORDS: [&str; 52] = [
    "true",
    "false",
    "null",
    "none",
    "undef",
    "poison",
    "zeroinitializer",
    "getelementptr",
    "ptrtoint",
    "inttoptr",
    "bitcast",
    "addrspacecast",
    "trunc",
    "zext",
    "sext",
    "fptrunc",
    "fpext",
    "fptoui",
    "fptosi",
    "uitofp",
    "sitofp",
    "add",
    "sub",
    "mul",
    "udiv",
    "sdiv",
    "urem",
    "srem",
    "shl",
    "lshr",
    "ashr",
    "and",
    "or",
    "xor",
    "select",
    "icmp",
    "fcmp",
    "extractvalue",
    "insertvalue",
    "extractelement",
    "insertelement",
    "shufflevector",
    "fadd",
    "fsub",
    "fmul",
    "fdiv",
    "frem",
    "fneg",
    "blockaddress",
    "dso_local_equivalent",
    "no_cfi",
    "splat",
];

fn is_decimal_start(byte: u8) -> bool {
    byte.is_ascii_digit() || byte == b'-' || byte == b'+'
}

impl<'a> Cx<'a> {
    fn symbol(&self, lex: &mut Lexer<'a>) -> Res<u32> {
        lex.bump();
        let name = lex.name()?;
        match self.module.by_name.get(name) {
            Some(&id) => Ok(id),
            None => bail!("the unknown symbol @{name}"),
        }
    }

    /// Reads one constant of type `ty`.
    pub(crate) fn parse_const(&mut self, lex: &mut Lexer<'a>, ty: Ty) -> Res<Const> {
        lex.skip_ws();
        match lex.peek() {
            b'@' => {
                let sym = self.symbol(lex)?;
                Ok(Const::Addr {
                    sym: Some(sym),
                    offset: 0,
                })
            }
            b'{' => {
                lex.bump();
                self.parse_elements(lex, ty, b'}')
            }
            b'[' => {
                lex.bump();
                self.parse_elements(lex, ty, b']')
            }
            b'<' => {
                lex.bump();
                if lex.eat(b'{') {
                    let value = self.parse_elements(lex, ty, b'}')?;
                    lex.expect(b'>')?;
                    Ok(value)
                } else {
                    self.parse_elements(lex, ty, b'>')
                }
            }
            b'c' if lex.peek_at(1) == b'"' => {
                lex.bump();
                let text = lex.quoted()?;
                Ok(Const::Bytes(decode_string(text)))
            }
            b'"' => bail!("a string constant near {}", lex.context()),
            byte if is_decimal_start(byte) => match ty {
                Ty::Float | Ty::Double => Ok(Const::Float(parse_float_literal(lex)?)),
                _ => Ok(Const::Int(lex.int()?)),
            },
            _ => {
                if matches!(ty, Ty::Float | Ty::Double)
                    && lex.peek() == b'0'
                    && lex.peek_at(1) == b'x'
                {
                    return Ok(Const::Float(parse_float_literal(lex)?));
                }
                let word = lex.word();
                match word {
                    "true" => Ok(Const::Int(1)),
                    "false" => Ok(Const::Int(0)),
                    "null" => Ok(Const::Addr {
                        sym: None,
                        offset: 0,
                    }),
                    "undef" | "poison" | "zeroinitializer" | "none" => Ok(Const::Zero),
                    "getelementptr" => self.const_gep(lex),
                    "ptrtoint" | "inttoptr" | "bitcast" | "trunc" | "zext" | "sext" => {
                        self.const_cast(lex, word)
                    }
                    "add" | "sub" | "mul" | "and" | "or" | "xor" | "shl" | "lshr" | "ashr" => {
                        self.const_binary(lex, word)
                    }
                    "" => bail!("a constant near {}", lex.context()),
                    other => bail!("the constant expression '{other}'"),
                }
            }
        }
    }

    /// The elements of a struct, array or vector constant, up to `close`.
    fn parse_elements(&mut self, lex: &mut Lexer<'a>, ty: Ty, close: u8) -> Res<Const> {
        let mut items = Vec::new();
        if !lex.eat_nl(close) {
            loop {
                lex.skip_ws_nl();
                let element_ty = self.types.parse_type(lex)?;
                items.push(self.parse_const(lex, element_ty)?);
                if lex.eat_nl(b',') {
                    continue;
                }
                lex.expect_nl(close)?;
                break;
            }
        }
        let _ = ty;
        Ok(Const::Agg(items))
    }

    /// `getelementptr [inbounds] (<ty>, ptr <base>, <idx>...)` of constants.
    fn const_gep(&mut self, lex: &mut Lexer<'a>) -> Res<Const> {
        loop {
            lex.skip_ws();
            match lex.peek_word() {
                "inbounds" | "nuw" | "nusw" => {
                    lex.word();
                }
                "inrange" => {
                    lex.word();
                    lex.skip_parens()?;
                }
                _ => break,
            }
        }
        lex.expect(b'(')?;
        let source = self.types.parse_type(lex)?;
        lex.expect(b',')?;
        let base_ty = self.types.parse_type(lex)?;
        if base_ty != Ty::Ptr {
            bail!("a vector getelementptr near {}", lex.context());
        }
        let base = self.parse_const(lex, Ty::Ptr)?;
        let Const::Addr { sym, offset } = base else {
            bail!("a getelementptr of a value that is not an address");
        };
        let mut offset = offset;
        let mut current = source;
        let mut first = true;
        while lex.eat(b',') {
            let index_ty = self.types.parse_type(lex)?;
            let Const::Int(index) = self.parse_const(lex, index_ty)? else {
                bail!("a getelementptr index that is not an integer");
            };
            let index = index as i64;
            if first {
                first = false;
                let stride = self.types.size_of(source)? as i64;
                offset = offset.wrapping_add(index.wrapping_mul(stride));
                continue;
            }
            match self.types.shape(current).cloned() {
                Some(Compound::Struct { fields, .. }) => {
                    let Some(&field) = fields.get(index as usize) else {
                        bail!("a struct index out of range");
                    };
                    offset += self
                        .types
                        .field_offset(current, index as usize)
                        .ok_or_else(|| Failure::new("a struct index out of range"))?
                        as i64;
                    current = field;
                }
                Some(Compound::Array { elem, .. } | Compound::Vector { elem, .. }) => {
                    offset =
                        offset.wrapping_add(index.wrapping_mul(self.types.size_of(elem)? as i64));
                    current = elem;
                }
                None => bail!("a getelementptr index into a scalar"),
            }
        }
        lex.expect(b')')?;
        Ok(Const::Addr { sym, offset })
    }

    /// `ptrtoint (ptr @g to i64)`, `inttoptr (...)`, `bitcast (...)`, `trunc (...)`...
    fn const_cast(&mut self, lex: &mut Lexer<'a>, word: &str) -> Res<Const> {
        lex.expect(b'(')?;
        let from = self.types.parse_type(lex)?;
        let value = self.parse_const(lex, from)?;
        lex.expect_word("to")?;
        let to = self.types.parse_type(lex)?;
        lex.expect(b')')?;
        match (word, value) {
            ("ptrtoint", Const::Addr { sym: None, offset }) => Ok(Const::Int(i128::from(offset))),
            ("ptrtoint", value @ Const::Addr { .. }) => {
                // To the integer of the width of a pointer, or one that a pointer extends to.
                let pointer_bits = (self.types.pointer_bytes() * 8) as u16;
                if to != Ty::Int(64) && to != Ty::Int(pointer_bits) {
                    bail!("a constant ptrtoint to {}", self.types.describe(to));
                }
                Ok(value)
            }
            ("inttoptr", Const::Int(value)) => Ok(Const::Addr {
                sym: None,
                offset: value as i64,
            }),
            ("inttoptr", value @ Const::Addr { .. }) => Ok(value),
            ("bitcast", value @ Const::Addr { .. }) if to == Ty::Ptr => Ok(value),
            ("bitcast", value) if from == to => Ok(value),
            ("trunc" | "zext" | "sext", Const::Int(value)) => {
                let (Ty::Int(from_bits), Ty::Int(to_bits)) = (from, to) else {
                    bail!("a constant {word} of a vector");
                };
                let narrow = |value: i128, bits: u16| {
                    if bits >= 128 {
                        value
                    } else {
                        (value << (128 - bits)) >> (128 - bits)
                    }
                };
                let unsigned = |value: i128, bits: u16| {
                    if bits >= 128 {
                        value
                    } else {
                        value & ((1i128 << bits) - 1)
                    }
                };
                Ok(Const::Int(match word {
                    "trunc" => narrow(value, to_bits),
                    "zext" => unsigned(value, from_bits),
                    _ => narrow(value, from_bits),
                }))
            }
            (_, Const::Zero) if word == "bitcast" => Ok(Const::Zero),
            _ => bail!("the constant expression '{word}' of this value"),
        }
    }

    /// `add (i64 a, i64 b)` and the other integer operations of two constants.
    fn const_binary(&mut self, lex: &mut Lexer<'a>, word: &str) -> Res<Const> {
        loop {
            lex.skip_ws();
            match lex.peek_word() {
                "nuw" | "nsw" => {
                    lex.word();
                }
                _ => break,
            }
        }
        lex.expect(b'(')?;
        let ty = self.types.parse_type(lex)?;
        let a = self.parse_const(lex, ty)?;
        lex.expect(b',')?;
        let ty_b = self.types.parse_type(lex)?;
        let b = self.parse_const(lex, ty_b)?;
        lex.expect(b')')?;
        let Ty::Int(bits) = ty else {
            bail!("a constant {word} of {}", self.types.describe(ty));
        };
        let wrap = |value: i128| {
            if bits >= 128 {
                value
            } else {
                (value << (128 - bits)) >> (128 - bits)
            }
        };
        match (a, b) {
            (Const::Int(a), Const::Int(b)) => {
                let unsigned = |value: i128| {
                    if bits >= 128 {
                        value as u128
                    } else {
                        (value as u128) & ((1u128 << bits) - 1)
                    }
                };
                Ok(Const::Int(wrap(match word {
                    "add" => a.wrapping_add(b),
                    "sub" => a.wrapping_sub(b),
                    "mul" => a.wrapping_mul(b),
                    "and" => a & b,
                    "or" => a | b,
                    "xor" => a ^ b,
                    "shl" => ((unsigned(a)) << (b as u32 & 127)) as i128,
                    "lshr" => (unsigned(a) >> (b as u32 & 127)) as i128,
                    _ => a >> (b as u32 & 127),
                })))
            }
            (Const::Addr { sym, offset }, Const::Int(delta)) if word == "add" => Ok(Const::Addr {
                sym,
                offset: offset.wrapping_add(delta as i64),
            }),
            (Const::Int(delta), Const::Addr { sym, offset }) if word == "add" => Ok(Const::Addr {
                sym,
                offset: offset.wrapping_add(delta as i64),
            }),
            (Const::Addr { sym, offset }, Const::Int(delta)) if word == "sub" => Ok(Const::Addr {
                sym,
                offset: offset.wrapping_sub(delta as i64),
            }),
            _ => bail!("the constant expression '{word}' of an address"),
        }
    }
}

/// The bytes of a `c"..."` string: `\XX` is a hex byte and `\\` a backslash.
fn decode_string(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if bytes.get(index + 1) == Some(&b'\\') {
                out.push(b'\\');
                index += 2;
                continue;
            }
            if let Some(pair) = text.get(index + 1..index + 3)
                && let Ok(byte) = u8::from_str_radix(pair, 16)
            {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

/// The bits of a floating-point literal as a `double`: `0x` followed by 16 hex digits, or decimal.
fn parse_float_literal(lex: &mut Lexer<'_>) -> Res<u64> {
    lex.skip_ws();
    let start = lex.pos;
    if lex.peek() == b'0' && lex.peek_at(1) == b'x' {
        lex.bump();
        lex.bump();
        if matches!(lex.peek(), b'H' | b'K' | b'L' | b'M' | b'R') {
            bail!(
                "a floating-point literal of another format near {}",
                lex.context()
            );
        }
        let digits = lex.pos;
        while lex.peek().is_ascii_hexdigit() {
            lex.bump();
        }
        return u64::from_str_radix(&lex.text[digits..lex.pos], 16).map_err(|_| {
            Failure::new(format!("the floating-point literal near {}", lex.context()))
        });
    }
    while matches!(lex.peek(), b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E') {
        lex.bump();
    }
    lex.text[start..lex.pos]
        .parse::<f64>()
        .map(f64::to_bits)
        .map_err(|_| Failure::new(format!("the floating-point literal near {}", lex.context())))
}

// The parser of function bodies follows in `parse.rs`.

impl<'a> Func<'a> {
    pub(crate) fn new(header: Header<'a>) -> Self {
        Self {
            header,
            n_locals: 0,
            local_ty: Vec::new(),
            blocks: Vec::new(),
            insts: Vec::new(),
            consts: Vec::new(),
            call_args: Vec::new(),
            phi_in: Vec::new(),
            gep_idx: Vec::new(),
            indices: Vec::new(),
            cases: Vec::new(),
            masks: Vec::new(),
            dbg: Vec::new(),
            subprogram: NONE,
        }
    }
}

impl Op {
    /// The operand of an instruction that has none, such as `ret void`.
    pub(crate) const NONE: Op = Op::Local(NONE);
}
