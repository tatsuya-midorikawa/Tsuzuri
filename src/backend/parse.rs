//! Reads one function body into the records of `func.rs`.

use super::error::{Failure, FxMap, Res, bail};
use super::func::{
    BinOp, Block, CallArg, Callee, CastOp, Cx, FPred, Func, IPred, Inst, NONE, Op, Ordering, RmwOp,
};
use super::lex::Lexer;
use super::module::{Ext, parse_header, skip_function_attributes, starts_type};
use super::types::{Compound, I1, Ty};

struct Parser<'a, 'c> {
    cx: &'c mut Cx<'a>,
    lex: Lexer<'a>,
    func: Func<'a>,
    locals: FxMap<&'a str, u32>,
    labels: FxMap<&'a str, u32>,
    /// The number of the entry block when the body leaves it unnamed.
    unnamed_entry: Option<u32>,
    /// False once the second block begins: an alloca after that would be a dynamic one.
    in_entry: bool,
}

const SKIPPED_INTRINSICS: [&str; 8] = [
    "llvm.lifetime.",
    "llvm.assume",
    "llvm.experimental.noalias.scope.decl",
    "llvm.dbg.",
    "llvm.donothing",
    "llvm.sideeffect",
    "llvm.prefetch",
    "llvm.invariant.",
];

/// The metadata id of the `!dbg !N` attachment in the tail of an instruction's line, or `NONE`.
fn dbg_location(tail: &str) -> u32 {
    let Some(at) = tail.find("!dbg !") else {
        return NONE;
    };
    let digits = &tail[at + "!dbg !".len()..];
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    digits[..end].parse().unwrap_or(NONE)
}

/// Words that start a constant, so that an argument attribute is not mistaken for one.
fn starts_value(word: &str) -> bool {
    super::func::is_value_word(word)
}

/// Reads the function whose `define` is at `header_pos`.
pub(crate) fn parse_function<'a>(cx: &mut Cx<'a>, header_pos: usize) -> Res<Func<'a>> {
    let mut lex = Lexer::new(cx.module.text, header_pos);
    let header = parse_header(&mut lex, &mut cx.types)?;
    let subprogram = skip_function_attributes(&mut lex)?;
    lex.expect(b'{')?;
    let mut parser = Parser {
        cx,
        lex,
        func: Func::new(header),
        locals: FxMap::default(),
        labels: FxMap::default(),
        unnamed_entry: None,
        in_entry: true,
    };
    parser.func.subprogram = subprogram;
    parser.run()?;
    Ok(parser.func)
}

impl<'a> Parser<'a, '_> {
    fn run(&mut self) -> Res<()> {
        let mut numbered = 0u32;
        for index in 0..self.func.header.params.len() {
            let (name, ty) = {
                let param = &self.func.header.params[index];
                (param.name.unwrap_or(""), param.ty)
            };
            let local = self.local(name);
            self.func.local_ty[local as usize] = ty;
            if !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()) {
                numbered += 1;
            }
        }
        self.lex.skip_ws_nl();
        let mut current = match self.try_label() {
            Some(label) => self.begin_block(Some(label))?,
            None => {
                self.unnamed_entry = Some(numbered);
                self.begin_block(None)?
            }
        };
        // After a terminator the rest of the block is dead: `call @llvm.trap` is followed by its
        // `unreachable`, and a trap is never followed by code that runs.
        let mut closed = false;
        loop {
            self.lex.skip_ws_nl();
            if self.lex.peek() == b'}' || self.lex.eof() {
                break;
            }
            // The generator may put the first instruction of a block on the line of its label.
            if let Some(label) = self.try_label() {
                self.func.blocks[current as usize].last = self.func.insts.len() as u32;
                current = self.begin_block(Some(label))?;
                self.in_entry = false;
                closed = false;
                continue;
            }
            let before = self.func.insts.len();
            self.instruction()?;
            let location = self.end_of_instruction()?;
            if closed {
                self.func.insts.truncate(before);
            } else if let Some(inst) = self.func.insts[before..].last() {
                closed = matches!(
                    inst,
                    Inst::Unreachable
                        | Inst::Ret { .. }
                        | Inst::Br { .. }
                        | Inst::CondBr { .. }
                        | Inst::Switch { .. }
                );
            }
            if self.cx.module.debug {
                self.func.dbg.resize(self.func.insts.len(), location);
            }
        }
        self.func.blocks[current as usize].last = self.func.insts.len() as u32;
        for (index, block) in self.func.blocks.iter().enumerate() {
            if !block.defined {
                bail!("a branch to an undefined label (block {index})");
            }
            if block.last == block.first
                || !matches!(
                    self.func.insts[block.last as usize - 1],
                    Inst::Unreachable
                        | Inst::Ret { .. }
                        | Inst::Br { .. }
                        | Inst::CondBr { .. }
                        | Inst::Switch { .. }
                )
            {
                bail!("a block that does not end in a terminator");
            }
        }
        self.func.n_locals = self.locals.len() as u32;
        Ok(())
    }

    /// Only attachments, a comment or the closing brace may follow an instruction on its line. A
    /// second instruction is not read, so it is refused rather than dropped. Returns the metadata
    /// id of the `!dbg` location of the instruction, or `NONE`.
    fn end_of_instruction(&mut self) -> Res<u32> {
        self.lex.skip_ws();
        let next = self.lex.peek();
        if next == b'%'
            || matches!(
                self.lex.peek_word(),
                "ret"
                    | "br"
                    | "switch"
                    | "unreachable"
                    | "call"
                    | "store"
                    | "tail"
                    | "musttail"
                    | "fence"
            )
        {
            bail!(
                "more than one instruction on a line near {}",
                self.lex.context()
            );
        }
        let start = self.lex.pos;
        self.lex.skip_to_eol();
        if !self.cx.module.debug {
            return Ok(NONE);
        }
        let rest = &self.lex.text[start..self.lex.pos];
        let rest = &rest[..rest.find(';').unwrap_or(rest.len())];
        Ok(dbg_location(rest))
    }

    fn begin_block(&mut self, label: Option<&'a str>) -> Res<u32> {
        let id = match label {
            Some(label) => self.block_id(label),
            None => self.new_block(),
        };
        let block = &mut self.func.blocks[id as usize];
        if block.defined {
            bail!("the same label twice");
        }
        block.defined = true;
        block.first = self.func.insts.len() as u32;
        Ok(id)
    }

    fn new_block(&mut self) -> u32 {
        let id = self.func.blocks.len() as u32;
        self.func.blocks.push(Block {
            first: 0,
            last: 0,
            defined: false,
        });
        id
    }

    /// A label at the start of the line: `name:` or `"quoted name":`.
    fn try_label(&mut self) -> Option<&'a str> {
        let saved = self.lex.pos;
        let label = if self.lex.peek() == b'"' {
            self.lex.quoted().ok()
        } else {
            let start = self.lex.pos;
            while self.lex.peek().is_ascii_alphanumeric()
                || matches!(self.lex.peek(), b'_' | b'.' | b'$' | b'-')
            {
                self.lex.bump();
            }
            (self.lex.pos > start).then(|| &self.lex.text[start..self.lex.pos])
        };
        if let Some(label) = label
            && self.lex.peek() == b':'
        {
            self.lex.bump();
            return Some(label);
        }
        self.lex.pos = saved;
        None
    }

    fn local(&mut self, name: &'a str) -> u32 {
        if let Some(&index) = self.locals.get(name) {
            return index;
        }
        let index = self.locals.len() as u32;
        self.locals.insert(name, index);
        self.func.local_ty.push(Ty::Void);
        index
    }

    fn block_id(&mut self, label: &'a str) -> u32 {
        if let Some(&id) = self.labels.get(label) {
            return id;
        }
        // The unnamed entry block is the first block, and other blocks refer to it by its number.
        if let Some(number) = self.unnamed_entry
            && !label.is_empty()
            && label.bytes().all(|byte| byte.is_ascii_digit())
            && label.parse::<u32>().ok() == Some(number)
        {
            return 0;
        }
        let id = self.new_block();
        self.labels.insert(label, id);
        id
    }

    /// `label %name`.
    fn label_operand(&mut self) -> Res<u32> {
        self.lex.expect_word("label")?;
        self.lex.expect(b'%')?;
        let name = self.lex.name()?;
        Ok(self.block_id(name))
    }

    fn ty(&mut self) -> Res<Ty> {
        self.cx.types.parse_type(&mut self.lex)
    }

    fn operand(&mut self, ty: Ty) -> Res<Op> {
        self.lex.skip_ws();
        if self.lex.peek() == b'%' {
            self.lex.bump();
            let name = self.lex.name()?;
            return Ok(Op::Local(self.local(name)));
        }
        let value = self.cx.parse_const(&mut self.lex, ty)?;
        let id = self.func.consts.len() as u32;
        self.func.consts.push(value);
        Ok(Op::Const(id))
    }

    /// A type and an operand of that type, as in `i64 %x`.
    fn typed_operand(&mut self) -> Res<(Ty, Op)> {
        let ty = self.ty()?;
        let op = self.operand(ty)?;
        Ok((ty, op))
    }

    fn comma(&mut self) -> Res<()> {
        self.lex.expect(b',')
    }

    /// Skips the flags of an instruction (`nuw`, `inbounds`, ...) and refuses fast-math flags.
    fn skip_flags(&mut self) -> Res<()> {
        loop {
            let word = self.lex.peek_word();
            match word {
                "nuw" | "nsw" | "exact" | "disjoint" | "nneg" | "samesign" | "inbounds"
                | "nusw" => {
                    self.lex.word();
                }
                "fast" | "nnan" | "ninf" | "nsz" | "arcp" | "contract" | "afn" | "reassoc" => {
                    bail!("the fast-math flag '{word}'")
                }
                _ => return Ok(()),
            }
        }
    }

    /// The trailing `, align N`, `, !md !N` of an instruction. Returns the alignment, if any.
    fn trailer(&mut self) -> Res<u32> {
        let mut align = 0;
        while self.lex.eat(b',') {
            self.lex.skip_ws();
            if self.lex.peek() == b'!' {
                break;
            }
            match self.lex.word() {
                "align" => align = u32::try_from(self.lex.uint()?).unwrap_or(0),
                other => bail!(
                    "the instruction operand '{other}' near {}",
                    self.lex.context()
                ),
            }
        }
        Ok(align)
    }

    /// `[syncscope("...")] [ordering]` after the pointer of an atomic access.
    fn atomic_suffix(&mut self) -> Res<()> {
        loop {
            match self.lex.peek_word() {
                "syncscope" => {
                    self.lex.word();
                    self.lex.skip_parens()?;
                }
                "unordered" | "monotonic" | "acquire" | "release" | "acq_rel" | "seq_cst" => {
                    self.lex.word();
                }
                _ => return Ok(()),
            }
        }
    }

    fn define(&mut self, dst: u32, ty: Ty) {
        if dst != NONE {
            self.func.local_ty[dst as usize] = ty;
        }
    }

    fn push(&mut self, inst: Inst<'a>) {
        self.func.insts.push(inst);
    }

    fn instruction(&mut self) -> Res<()> {
        self.lex.skip_ws();
        let mut dst = NONE;
        if self.lex.peek() == b'%' {
            self.lex.bump();
            let name = self.lex.name()?;
            self.lex.expect(b'=')?;
            dst = self.local(name);
        }
        let word = self.lex.word();
        match word {
            "load" => self.load(dst),
            "store" => self.store(),
            "call" => self.call(dst),
            "tail" | "notail" => {
                self.lex.expect_word("call")?;
                self.call(dst)
            }
            // A guaranteed tail call keeps the stack from growing; a plain call would not.
            "musttail" => bail!("a musttail call"),
            "br" => self.branch(),
            "icmp" => self.icmp(dst),
            "getelementptr" => self.gep(dst),
            "alloca" => self.alloca(dst),
            "ret" => self.ret(),
            "add" | "sub" | "mul" | "udiv" | "sdiv" | "urem" | "srem" | "shl" | "lshr" | "ashr"
            | "and" | "or" | "xor" | "fadd" | "fsub" | "fmul" | "fdiv" | "frem" => {
                self.binary(dst, word)
            }
            "trunc" | "zext" | "sext" | "fptrunc" | "fpext" | "fptoui" | "fptosi" | "uitofp"
            | "sitofp" | "ptrtoint" | "inttoptr" | "bitcast" => self.cast(dst, word),
            "phi" => self.phi(dst),
            "select" => self.select(dst),
            "switch" => self.switch(),
            "unreachable" => {
                self.push(Inst::Unreachable);
                Ok(())
            }
            "extractvalue" => self.extract_value(dst),
            "insertvalue" => self.insert_value(dst),
            "fcmp" => self.fcmp(dst),
            "fneg" => {
                self.skip_flags()?;
                let (ty, a) = self.typed_operand()?;
                self.define(dst, ty);
                self.push(Inst::FNeg { dst, ty, a });
                Ok(())
            }
            "extractelement" => self.extract_element(dst),
            "insertelement" => self.insert_element(dst),
            "shufflevector" => self.shuffle_vector(dst),
            "atomicrmw" => self.atomic_rmw(dst),
            "cmpxchg" => self.cmpxchg(dst),
            "fence" => {
                self.atomic_suffix()?;
                self.push(Inst::Fence);
                Ok(())
            }
            "freeze" => {
                let (ty, a) = self.typed_operand()?;
                self.define(dst, ty);
                self.push(Inst::Freeze { dst, ty, a });
                Ok(())
            }
            "" => bail!("an instruction near {}", self.lex.context()),
            other => bail!("the instruction '{other}'"),
        }
    }

    fn binary(&mut self, dst: u32, word: &str) -> Res<()> {
        self.skip_flags()?;
        let ty = self.ty()?;
        let a = self.operand(ty)?;
        self.comma()?;
        let b = self.operand(ty)?;
        let op = match word {
            "add" => BinOp::Add,
            "sub" => BinOp::Sub,
            "mul" => BinOp::Mul,
            "udiv" => BinOp::UDiv,
            "sdiv" => BinOp::SDiv,
            "urem" => BinOp::URem,
            "srem" => BinOp::SRem,
            "shl" => BinOp::Shl,
            "lshr" => BinOp::LShr,
            "ashr" => BinOp::AShr,
            "and" => BinOp::And,
            "or" => BinOp::Or,
            "xor" => BinOp::Xor,
            "fadd" => BinOp::FAdd,
            "fsub" => BinOp::FSub,
            "fmul" => BinOp::FMul,
            "fdiv" => BinOp::FDiv,
            _ => BinOp::FRem,
        };
        self.define(dst, ty);
        self.push(Inst::Bin { dst, op, ty, a, b });
        Ok(())
    }

    /// The type of a comparison of values of type `operand`: `i1`, or a vector of `i1`.
    fn compare_result(&mut self, operand: Ty) -> Res<Ty> {
        match self.cx.types.shape(operand).cloned() {
            Some(Compound::Vector { len, .. }) => {
                self.cx.types.intern(Compound::Vector { len, elem: I1 })
            }
            _ => Ok(I1),
        }
    }

    fn icmp(&mut self, dst: u32) -> Res<()> {
        self.skip_flags()?;
        let pred = match self.lex.word() {
            "eq" => IPred::Eq,
            "ne" => IPred::Ne,
            "ugt" => IPred::Ugt,
            "uge" => IPred::Uge,
            "ult" => IPred::Ult,
            "ule" => IPred::Ule,
            "sgt" => IPred::Sgt,
            "sge" => IPred::Sge,
            "slt" => IPred::Slt,
            "sle" => IPred::Sle,
            other => bail!("the comparison '{other}'"),
        };
        let ty = self.ty()?;
        let a = self.operand(ty)?;
        self.comma()?;
        let b = self.operand(ty)?;
        let result = self.compare_result(ty)?;
        self.define(dst, result);
        self.push(Inst::ICmp {
            dst,
            pred,
            ty,
            a,
            b,
        });
        Ok(())
    }

    fn fcmp(&mut self, dst: u32) -> Res<()> {
        self.skip_flags()?;
        let pred = match self.lex.word() {
            "false" => FPred::False,
            "oeq" => FPred::Oeq,
            "ogt" => FPred::Ogt,
            "oge" => FPred::Oge,
            "olt" => FPred::Olt,
            "ole" => FPred::Ole,
            "one" => FPred::One,
            "ord" => FPred::Ord,
            "ueq" => FPred::Ueq,
            "ugt" => FPred::Ugt,
            "uge" => FPred::Uge,
            "ult" => FPred::Ult,
            "ule" => FPred::Ule,
            "une" => FPred::Une,
            "uno" => FPred::Uno,
            "true" => FPred::True,
            other => bail!("the comparison '{other}'"),
        };
        let ty = self.ty()?;
        let a = self.operand(ty)?;
        self.comma()?;
        let b = self.operand(ty)?;
        let result = self.compare_result(ty)?;
        self.define(dst, result);
        self.push(Inst::FCmp {
            dst,
            pred,
            ty,
            a,
            b,
        });
        Ok(())
    }

    fn cast(&mut self, dst: u32, word: &str) -> Res<()> {
        self.skip_flags()?;
        let from = self.ty()?;
        let a = self.operand(from)?;
        self.lex.expect_word("to")?;
        let to = self.ty()?;
        let op = match word {
            "trunc" => CastOp::Trunc,
            "zext" => CastOp::ZExt,
            "sext" => CastOp::SExt,
            "fptrunc" => CastOp::FpTrunc,
            "fpext" => CastOp::FpExt,
            "fptoui" => CastOp::FpToUi,
            "fptosi" => CastOp::FpToSi,
            "uitofp" => CastOp::UiToFp,
            "sitofp" => CastOp::SiToFp,
            "ptrtoint" => CastOp::PtrToInt,
            "inttoptr" => CastOp::IntToPtr,
            _ => CastOp::BitCast,
        };
        self.define(dst, to);
        self.push(Inst::Cast {
            dst,
            op,
            from,
            to,
            a,
        });
        Ok(())
    }

    fn select(&mut self, dst: u32) -> Res<()> {
        self.skip_flags()?;
        let (cond_ty, cond) = self.typed_operand()?;
        self.comma()?;
        let (ty, a) = self.typed_operand()?;
        self.comma()?;
        let (_, b) = self.typed_operand()?;
        self.define(dst, ty);
        self.push(Inst::Select {
            dst,
            cond_ty,
            cond,
            ty,
            a,
            b,
        });
        Ok(())
    }

    fn phi(&mut self, dst: u32) -> Res<()> {
        self.skip_flags()?;
        let ty = self.ty()?;
        let start = self.func.phi_in.len() as u32;
        loop {
            self.lex.expect(b'[')?;
            let value = self.operand(ty)?;
            self.comma()?;
            self.lex.expect(b'%')?;
            let name = self.lex.name()?;
            let block = self.block_id(name);
            self.lex.expect(b']')?;
            self.func.phi_in.push((value, block));
            if !self.lex.eat(b',') {
                break;
            }
            // The attachments follow the last entry: `, !dbg !12`.
            self.lex.skip_ws();
            if self.lex.peek() == b'!' {
                break;
            }
        }
        let len = self.func.phi_in.len() as u32 - start;
        self.define(dst, ty);
        self.push(Inst::Phi {
            dst,
            ty,
            start,
            len,
        });
        Ok(())
    }

    fn alloca(&mut self, dst: u32) -> Res<()> {
        if self.lex.peek_word() == "inalloca" {
            bail!("an inalloca alloca");
        }
        if !self.in_entry {
            // Each run of such an alloca takes new stack; a fixed slot would be another meaning.
            bail!("an alloca outside the entry block");
        }
        let ty = self.ty()?;
        let mut count = 1u64;
        let mut align = 0u32;
        while self.lex.eat(b',') {
            self.lex.skip_ws();
            if self.lex.peek() == b'!' {
                break;
            }
            if self.lex.peek_word() == "align" {
                self.lex.word();
                align = u32::try_from(self.lex.uint()?).unwrap_or(0);
            } else if self.lex.peek_word() == "addrspace" {
                bail!("an alloca in an address space");
            } else {
                let (count_ty, op) = self.typed_operand()?;
                let Op::Const(id) = op else {
                    bail!("an alloca with a variable count");
                };
                let super::func::Const::Int(value) = self.func.consts[id as usize] else {
                    bail!("an alloca with a count that is not an integer");
                };
                let _ = count_ty;
                count =
                    u64::try_from(value).map_err(|_| Failure::new("a negative alloca count"))?;
            }
        }
        self.define(dst, Ty::Ptr);
        self.push(Inst::Alloca {
            dst,
            ty,
            count,
            align,
        });
        Ok(())
    }

    fn load(&mut self, dst: u32) -> Res<()> {
        let mut atomic = Ordering::NotAtomic;
        loop {
            match self.lex.peek_word() {
                "atomic" => {
                    self.lex.word();
                    atomic = Ordering::Atomic;
                }
                "volatile" => {
                    self.lex.word();
                }
                _ => break,
            }
        }
        let ty = self.ty()?;
        self.comma()?;
        let (_, ptr) = self.typed_operand()?;
        self.atomic_suffix()?;
        self.trailer()?;
        self.define(dst, ty);
        self.push(Inst::Load {
            dst,
            ty,
            ptr,
            atomic,
        });
        Ok(())
    }

    fn store(&mut self) -> Res<()> {
        let mut atomic = Ordering::NotAtomic;
        loop {
            match self.lex.peek_word() {
                "atomic" => {
                    self.lex.word();
                    atomic = Ordering::Atomic;
                }
                "volatile" => {
                    self.lex.word();
                }
                _ => break,
            }
        }
        let (ty, val) = self.typed_operand()?;
        self.comma()?;
        let (_, ptr) = self.typed_operand()?;
        self.atomic_suffix()?;
        self.trailer()?;
        self.push(Inst::Store {
            ty,
            val,
            ptr,
            atomic,
        });
        Ok(())
    }

    fn gep(&mut self, dst: u32) -> Res<()> {
        self.skip_flags()?;
        let ty = self.ty()?;
        self.comma()?;
        let (base_ty, base) = self.typed_operand()?;
        if base_ty != Ty::Ptr {
            bail!("a getelementptr of a vector of pointers");
        }
        let start = self.func.gep_idx.len() as u32;
        while self.lex.eat(b',') {
            self.lex.skip_ws();
            if self.lex.peek() == b'!' {
                break;
            }
            let (index_ty, index) = self.typed_operand()?;
            self.func.gep_idx.push((index_ty, index));
        }
        let len = self.func.gep_idx.len() as u32 - start;
        self.define(dst, Ty::Ptr);
        self.push(Inst::Gep {
            dst,
            ty,
            base,
            start,
            len,
        });
        Ok(())
    }

    fn indices(&mut self) -> Res<(u32, u32)> {
        let start = self.func.indices.len() as u32;
        while self.lex.eat(b',') {
            self.lex.skip_ws();
            if self.lex.peek() == b'!' {
                break;
            }
            let index =
                u32::try_from(self.lex.uint()?).map_err(|_| Failure::new("a huge index"))?;
            self.func.indices.push(index);
        }
        Ok((start, self.func.indices.len() as u32 - start))
    }

    fn extract_value(&mut self, dst: u32) -> Res<()> {
        let (ty, agg) = self.typed_operand()?;
        let (start, len) = self.indices()?;
        let result = self.walk_indices(ty, start, len)?;
        self.define(dst, result);
        self.push(Inst::ExtractValue {
            dst,
            ty,
            agg,
            start,
            len,
        });
        Ok(())
    }

    fn insert_value(&mut self, dst: u32) -> Res<()> {
        let (ty, agg) = self.typed_operand()?;
        self.comma()?;
        let (val_ty, val) = self.typed_operand()?;
        let (start, len) = self.indices()?;
        self.define(dst, ty);
        self.push(Inst::InsertValue {
            dst,
            ty,
            agg,
            val_ty,
            val,
            start,
            len,
        });
        Ok(())
    }

    /// The type that the `indices[start..start + len]` of an aggregate lead to.
    fn walk_indices(&self, mut ty: Ty, start: u32, len: u32) -> Res<Ty> {
        for &index in &self.func.indices[start as usize..(start + len) as usize] {
            ty = match self.cx.types.shape(ty) {
                Some(Compound::Struct { fields, .. }) => *fields
                    .get(index as usize)
                    .ok_or_else(|| Failure::new("a struct index out of range"))?,
                Some(Compound::Array { elem, .. }) => *elem,
                _ => bail!("an extractvalue of a value that is not an aggregate"),
            };
        }
        Ok(ty)
    }

    fn extract_element(&mut self, dst: u32) -> Res<()> {
        let (ty, vec) = self.typed_operand()?;
        self.comma()?;
        let (idx_ty, idx) = self.typed_operand()?;
        let Some(Compound::Vector { elem, .. }) = self.cx.types.shape(ty).cloned() else {
            bail!("an extractelement of a value that is not a vector");
        };
        self.define(dst, elem);
        self.push(Inst::ExtractElement {
            dst,
            ty,
            vec,
            idx_ty,
            idx,
        });
        Ok(())
    }

    fn insert_element(&mut self, dst: u32) -> Res<()> {
        let (ty, vec) = self.typed_operand()?;
        self.comma()?;
        let (_, elem) = self.typed_operand()?;
        self.comma()?;
        let (idx_ty, idx) = self.typed_operand()?;
        self.define(dst, ty);
        self.push(Inst::InsertElement {
            dst,
            ty,
            vec,
            elem,
            idx_ty,
            idx,
        });
        Ok(())
    }

    fn shuffle_vector(&mut self, dst: u32) -> Res<()> {
        let (ty, a) = self.typed_operand()?;
        self.comma()?;
        let (_, b) = self.typed_operand()?;
        self.comma()?;
        let mask_ty = self.ty()?;
        let Some(Compound::Vector { len: lanes, .. }) = self.cx.types.shape(mask_ty).cloned()
        else {
            bail!("a shufflevector mask that is not a vector");
        };
        let mask = self.operand(mask_ty)?;
        let Op::Const(id) = mask else {
            bail!("a shufflevector with a variable mask");
        };
        let start = self.func.masks.len() as u32;
        match &self.func.consts[id as usize] {
            super::func::Const::Zero => {
                for _ in 0..lanes {
                    self.func.masks.push(0);
                }
            }
            super::func::Const::Agg(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(match item {
                        super::func::Const::Int(value) => *value as i32,
                        super::func::Const::Zero => 0,
                        _ => bail!("a shufflevector mask that is not integers"),
                    });
                }
                self.func.masks.extend(values);
            }
            _ => bail!("a shufflevector mask that is not a constant vector"),
        }
        let out = match self.cx.types.shape(ty).cloned() {
            Some(Compound::Vector { elem, .. }) => self
                .cx
                .types
                .intern(Compound::Vector { len: lanes, elem })?,
            _ => bail!("a shufflevector of a value that is not a vector"),
        };
        self.define(dst, out);
        self.push(Inst::ShuffleVector {
            dst,
            ty,
            a,
            b,
            start,
            len: lanes,
        });
        Ok(())
    }

    fn atomic_rmw(&mut self, dst: u32) -> Res<()> {
        if self.lex.peek_word() == "volatile" {
            self.lex.word();
        }
        let op = match self.lex.word() {
            "xchg" => RmwOp::Xchg,
            "add" => RmwOp::Add,
            "sub" => RmwOp::Sub,
            "and" => RmwOp::And,
            "nand" => RmwOp::Nand,
            "or" => RmwOp::Or,
            "xor" => RmwOp::Xor,
            "max" => RmwOp::Max,
            "min" => RmwOp::Min,
            "umax" => RmwOp::UMax,
            "umin" => RmwOp::UMin,
            other => bail!("the atomicrmw operation '{other}'"),
        };
        let (_, ptr) = self.typed_operand()?;
        self.comma()?;
        let (ty, val) = self.typed_operand()?;
        self.atomic_suffix()?;
        self.trailer()?;
        self.define(dst, ty);
        self.push(Inst::AtomicRmw {
            dst,
            op,
            ty,
            ptr,
            val,
        });
        Ok(())
    }

    fn cmpxchg(&mut self, dst: u32) -> Res<()> {
        while matches!(self.lex.peek_word(), "weak" | "volatile") {
            self.lex.word();
        }
        let (_, ptr) = self.typed_operand()?;
        self.comma()?;
        let (ty, cmp) = self.typed_operand()?;
        self.comma()?;
        let (_, new) = self.typed_operand()?;
        self.atomic_suffix()?;
        self.trailer()?;
        let result = self.cx.types.intern(Compound::Struct {
            fields: vec![ty, I1].into_boxed_slice(),
            packed: false,
        })?;
        self.define(dst, result);
        self.push(Inst::CmpXchg {
            dst,
            ty,
            ptr,
            cmp,
            new,
        });
        Ok(())
    }

    fn ret(&mut self) -> Res<()> {
        let ty = self.ty()?;
        if ty == Ty::Void {
            self.push(Inst::Ret { ty, val: Op::NONE });
            return Ok(());
        }
        let val = self.operand(ty)?;
        self.push(Inst::Ret { ty, val });
        Ok(())
    }

    fn branch(&mut self) -> Res<()> {
        self.lex.skip_ws();
        if self.lex.peek_word() == "label" {
            let target = self.label_operand()?;
            self.push(Inst::Br { target });
            return Ok(());
        }
        let (_, cond) = self.typed_operand()?;
        self.comma()?;
        let then_block = self.label_operand()?;
        self.comma()?;
        let else_block = self.label_operand()?;
        self.push(Inst::CondBr {
            cond,
            then_block,
            else_block,
        });
        Ok(())
    }

    fn switch(&mut self) -> Res<()> {
        let (ty, cond) = self.typed_operand()?;
        self.comma()?;
        let default = self.label_operand()?;
        self.lex.expect(b'[')?;
        let start = self.func.cases.len() as u32;
        loop {
            self.lex.skip_ws_nl();
            if self.lex.peek() == b']' {
                self.lex.bump();
                break;
            }
            let case_ty = self.ty()?;
            let value = match self.cx.parse_const(&mut self.lex, case_ty)? {
                super::func::Const::Int(value) => value,
                _ => bail!("a switch case that is not an integer"),
            };
            self.comma()?;
            let block = self.label_operand()?;
            self.func.cases.push((value, block));
        }
        let len = self.func.cases.len() as u32 - start;
        self.push(Inst::Switch {
            ty,
            cond,
            default,
            start,
            len,
        });
        Ok(())
    }

    /// Attributes between an argument's type and its value; the extension, the `byval` type and `sret`.
    fn argument_attributes(&mut self) -> Res<(Ext, Option<Ty>, bool)> {
        let mut ext = Ext::None;
        let mut byval = None;
        let mut sret = false;
        loop {
            self.lex.skip_ws();
            let byte = self.lex.peek();
            if !byte.is_ascii_alphabetic() {
                return Ok((ext, byval, sret));
            }
            let word = self.lex.peek_word();
            if (word == "c" && self.lex.peek_at(1) == b'"') || starts_value(word) {
                return Ok((ext, byval, sret));
            }
            self.lex.word();
            match word {
                "signext" => ext = Ext::Sext,
                "zeroext" => ext = Ext::Zext,
                "byval" => {
                    self.lex.expect(b'(')?;
                    byval = Some(self.ty()?);
                    self.lex.expect(b')')?;
                }
                "sret" => {
                    sret = true;
                    self.lex.expect(b'(')?;
                    self.ty()?;
                    self.lex.expect(b')')?;
                }
                "elementtype" | "inalloca" | "preallocated" | "byref" => {
                    self.lex.expect(b'(')?;
                    self.ty()?;
                    self.lex.expect(b')')?;
                }
                "align" => {
                    self.lex.skip_ws();
                    if self.lex.peek() == b'(' {
                        self.lex.skip_parens()?;
                    } else {
                        self.lex.uint()?;
                    }
                }
                "inreg" | "swiftself" | "swifterror" | "swiftasync" => {
                    bail!("the argument attribute '{word}'")
                }
                _ => {
                    self.lex.skip_ws();
                    if self.lex.peek() == b'(' {
                        self.lex.skip_parens()?;
                    }
                }
            }
        }
    }

    fn call(&mut self, dst: u32) -> Res<()> {
        // Flags, calling convention and return attributes, then the type. The extension of a result
        // comes from the callee's own declaration.
        while !starts_type(&mut self.lex) {
            let word = self.lex.word();
            match word {
                "" => bail!("a call near {}", self.lex.context()),
                "ccc" | "fastcc" | "coldcc" | "signext" | "zeroext" => {}
                "fast" | "nnan" | "ninf" | "nsz" | "arcp" | "contract" | "afn" | "reassoc" => {
                    bail!("the fast-math flag '{word}'")
                }
                "inreg" => bail!("the return attribute 'inreg'"),
                _ if word == "cc" || word.ends_with("cc") => {
                    bail!("the calling convention '{word}'")
                }
                _ => {
                    self.lex.skip_ws();
                    if self.lex.peek() == b'(' {
                        self.lex.skip_parens()?;
                    } else if word == "align" {
                        self.lex.uint()?;
                    }
                }
            }
        }
        let ret = self.ty()?;
        self.lex.skip_ws();
        let mut variadic = false;
        if self.lex.peek() == b'(' {
            // An explicit function type: `i32 (ptr, ...)`.
            self.lex.bump();
            if !self.lex.eat(b')') {
                loop {
                    self.lex.skip_ws();
                    if self.lex.peek() == b'.' {
                        for _ in 0..3 {
                            self.lex.bump();
                        }
                        variadic = true;
                        self.lex.expect(b')')?;
                        break;
                    }
                    self.ty()?;
                    if self.lex.eat(b',') {
                        continue;
                    }
                    self.lex.expect(b')')?;
                    break;
                }
            }
            self.lex.skip_ws();
        }
        let callee = match self.lex.peek() {
            b'@' => {
                self.lex.bump();
                let name = self.lex.name()?;
                if name.starts_with("llvm.") {
                    if name == "llvm.trap" {
                        // A trap does not return: it is an `unreachable` that the CPU reports.
                        self.lex.skip_to_eol();
                        self.push(Inst::Unreachable);
                        return Ok(());
                    }
                    if SKIPPED_INTRINSICS
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                    {
                        self.lex.skip_to_eol();
                        return Ok(());
                    }
                    Callee::Intrinsic(name)
                } else {
                    match self.cx.module.by_name.get(name) {
                        Some(&id) => Callee::Direct(id),
                        None => bail!("a call to the unknown function @{name}"),
                    }
                }
            }
            b'%' => {
                self.lex.bump();
                let name = self.lex.name()?;
                Callee::Indirect(Op::Local(self.local(name)))
            }
            _ => {
                if self.lex.peek_word() == "asm" {
                    bail!("inline assembly");
                }
                bail!("a callee near {}", self.lex.context());
            }
        };
        self.lex.expect(b'(')?;
        let start = self.func.call_args.len() as u32;
        if !self.lex.eat(b')') {
            loop {
                let ty = self.ty()?;
                if ty == Ty::Metadata {
                    bail!("a metadata argument");
                }
                let (ext, byval, sret) = self.argument_attributes()?;
                let op = self.operand(ty)?;
                self.func.call_args.push(CallArg {
                    ty,
                    op,
                    byval,
                    sret,
                    ext,
                });
                if self.lex.eat(b',') {
                    continue;
                }
                self.lex.expect(b')')?;
                break;
            }
        }
        let len = self.func.call_args.len() as u32 - start;
        if dst != NONE {
            self.define(dst, ret);
        }
        self.push(Inst::Call {
            dst,
            ret,
            callee,
            start,
            len,
            variadic,
        });
        Ok(())
    }
}
