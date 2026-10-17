//! The top level of a module: its types, its symbols, and which of them a build needs.
//!
//! The scan reads the headers and skips the bodies and initializers, which are only found by their
//! last line. A body is parsed later, on a worker, and only if a root reaches it.

use std::borrow::Cow;

use super::error::{Failure, FxMap, Res, bail};
use super::lex::Lexer;
use super::types::{Ty, TypeTable, Types};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Linkage {
    /// `private` and `internal`.
    Local,
    External,
    /// `weak`, `weak_odr`, `linkonce` and `linkonce_odr`: one definition wins at link time.
    Weak,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Ext {
    #[default]
    None,
    Sext,
    Zext,
}

#[derive(Clone, Debug)]
pub(crate) struct Param<'a> {
    pub ty: Ty,
    pub name: Option<&'a str>,
    pub ext: Ext,
    /// `byval(T)`: the callee gets a private copy of the pointed-to `T`.
    pub byval: Option<Ty>,
}

/// What a function header says.
#[derive(Debug)]
pub(crate) struct Header<'a> {
    pub linkage: Linkage,
    pub hidden: bool,
    pub name: &'a str,
    pub ret: Ty,
    pub ret_ext: Ext,
    pub params: Vec<Param<'a>>,
    pub variadic: bool,
    pub is_define: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Sig {
    pub ret: Ty,
    pub ret_ext: Ext,
    pub params: Vec<(Ty, Ext)>,
    pub variadic: bool,
}

#[derive(Debug)]
pub(crate) enum SymKind {
    Function {
        sig: Sig,
        /// The offset of `define` or `declare`, and the end of the body when there is one.
        header: usize,
        body_end: Option<usize>,
    },
    Global {
        ty: Ty,
        constant: bool,
        tls: bool,
        align: u32,
        /// The initializer's text; `None` for a declaration.
        init: Option<(usize, usize)>,
    },
    /// An alias or other definition that the translator does not lower; refusing it is up to
    /// whoever reaches it.
    Unsupported(&'static str),
}

#[derive(Debug)]
pub(crate) struct Sym<'a> {
    pub name: &'a str,
    pub linkage: Linkage,
    pub hidden: bool,
    pub kind: SymKind,
}

impl Sym<'_> {
    pub(crate) fn is_defined(&self) -> bool {
        match &self.kind {
            SymKind::Function { body_end, .. } => body_end.is_some(),
            SymKind::Global { init, .. } => init.is_some(),
            SymKind::Unsupported(_) => true,
        }
    }

    /// The name as the object file spells it: quoted escapes decoded.
    pub(crate) fn object_name(&self) -> Res<Cow<'_, str>> {
        decode_name(self.name)
    }
}

pub(crate) fn decode_name(name: &str) -> Res<Cow<'_, str>> {
    if !name.contains('\\') {
        return Ok(Cow::Borrowed(name));
    }
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && let Some(pair) = name.get(index + 1..index + 3)
            && let Ok(byte) = u8::from_str_radix(pair, 16)
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out)
        .map(Cow::Owned)
        .map_err(|_| Failure::new(format!("the symbol name \"{name}\" that is not UTF-8")))
}

pub(crate) struct Module<'a> {
    pub text: &'a str,
    pub types: TypeTable<'a>,
    pub syms: Vec<Sym<'a>>,
    pub by_name: FxMap<&'a str, u32>,
}

/// The end of the item that starts at `start`: after its last line. A function ends at its closing
/// brace, which the generator may follow with the next item on the same line.
fn item_end(text: &str, start: usize) -> usize {
    if text[start..].starts_with("define ") {
        return match text[start..].find("\n}") {
            Some(offset) => start + offset + 2,
            None => text.len(),
        };
    }
    // A global's initializer may span lines: read to the end of the line where every bracket
    // that opened has closed. Strings are skipped whole, so their contents do not count.
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += 1;
                }
            }
            b'[' | b'{' | b'(' => depth += 1,
            b']' | b'}' | b')' => depth -= 1,
            b';' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'\n' if depth <= 0 => return index + 1,
            _ => {}
        }
        index += 1;
    }
    text.len()
}

/// Reads the words in front of a global's or function's type: linkage, visibility and the like.
/// Returns `(linkage, hidden, thread_local)`, leaving the lexer at the first word it does not know.
fn parse_modifiers(lex: &mut Lexer<'_>) -> Res<(Linkage, bool, bool)> {
    let mut linkage = Linkage::External;
    let mut hidden = false;
    let mut tls = false;
    loop {
        let saved = lex.pos;
        let word = lex.word();
        match word {
            "private" | "internal" => linkage = Linkage::Local,
            "external" => linkage = Linkage::External,
            "weak" | "weak_odr" | "linkonce" | "linkonce_odr" => linkage = Linkage::Weak,
            "available_externally" | "common" | "appending" | "extern_weak" => {
                bail!("the linkage '{word}'")
            }
            "hidden" => hidden = true,
            "default"
            | "protected"
            | "dllexport"
            | "dllimport"
            | "unnamed_addr"
            | "local_unnamed_addr"
            | "dso_local"
            | "dso_preemptable"
            | "ccc"
            | "fastcc"
            | "coldcc"
            | "externally_initialized" => {}
            "thread_local" => {
                tls = true;
                lex.skip_ws();
                if lex.peek() == b'(' {
                    lex.skip_parens()?;
                }
            }
            "addrspace" => {
                lex.expect(b'(')?;
                let space = lex.uint()?;
                lex.expect(b')')?;
                if space != 0 {
                    bail!("address space {space}");
                }
            }
            _ => {
                lex.pos = saved;
                return Ok((linkage, hidden, tls));
            }
        }
    }
}

const TYPE_WORDS: [&str; 12] = [
    "void",
    "half",
    "bfloat",
    "float",
    "double",
    "ptr",
    "x86_fp80",
    "fp128",
    "ppc_fp128",
    "label",
    "metadata",
    "token",
];

pub(crate) fn starts_type(lex: &mut Lexer<'_>) -> bool {
    lex.skip_ws();
    match lex.peek() {
        b'%' | b'{' | b'[' | b'<' => true,
        _ => {
            let word = lex.peek_word();
            TYPE_WORDS.contains(&word)
                || word.strip_prefix('i').is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                })
        }
    }
}

/// Whether the word names a calling convention other than the C one that `fastcc` and `coldcc`
/// share with it.
fn is_foreign_convention(word: &str) -> bool {
    word == "cc" || (word.ends_with("cc") && !matches!(word, "ccc" | "fastcc" | "coldcc"))
}

/// Parameter attributes after a type: the extension kind, the `byval` type, and the local name of
/// a definition's parameter.
fn parse_param_attributes<'a>(
    lex: &mut Lexer<'a>,
    types: &mut Types<'a>,
    param: &mut Param<'a>,
) -> Res<()> {
    loop {
        lex.skip_ws();
        match lex.peek() {
            b'%' => {
                lex.bump();
                param.name = Some(lex.name()?);
                return Ok(());
            }
            b',' | b')' => return Ok(()),
            _ => {}
        }
        let word = lex.word();
        match word {
            "" => bail!("a parameter near {}", lex.context()),
            "signext" => param.ext = Ext::Sext,
            "zeroext" => param.ext = Ext::Zext,
            "byval" => {
                lex.expect(b'(')?;
                param.byval = Some(types.parse_type(lex)?);
                lex.expect(b')')?;
            }
            "sret" | "elementtype" | "inalloca" | "preallocated" | "byref" => {
                lex.expect(b'(')?;
                types.parse_type(lex)?;
                lex.expect(b')')?;
            }
            "align" => {
                lex.skip_ws();
                if lex.peek() == b'(' {
                    lex.skip_parens()?;
                } else {
                    lex.uint()?;
                }
            }
            "inreg" | "swiftself" | "swifterror" | "swiftasync" => {
                bail!("the parameter attribute '{word}'")
            }
            _ => {
                lex.skip_ws();
                if lex.peek() == b'(' {
                    lex.skip_parens()?;
                }
            }
        }
    }
}

/// Reads `define` or `declare` up to and including the parameter list.
pub(crate) fn parse_header<'a>(lex: &mut Lexer<'a>, types: &mut Types<'a>) -> Res<Header<'a>> {
    let keyword = lex.word();
    let is_define = match keyword {
        "define" => true,
        "declare" => false,
        _ => bail!("a function header near {}", lex.context()),
    };
    let (linkage, hidden, _) = parse_modifiers(lex)?;
    let mut ret_ext = Ext::None;
    // Return attributes, then the result type.
    while !starts_type(lex) {
        let word = lex.word();
        match word {
            "" => bail!("a result type near {}", lex.context()),
            "signext" => ret_ext = Ext::Sext,
            "zeroext" => ret_ext = Ext::Zext,
            "inreg" => bail!("the return attribute 'inreg'"),
            _ if is_foreign_convention(word) => bail!("the calling convention '{word}'"),
            _ => {
                lex.skip_ws();
                if lex.peek() == b'(' {
                    lex.skip_parens()?;
                } else if word == "align" {
                    lex.uint()?;
                }
            }
        }
    }
    let ret = types.parse_type(lex)?;
    lex.skip_ws();
    if lex.peek() != b'@' {
        bail!("a function name near {}", lex.context());
    }
    lex.bump();
    let name = lex.name()?;
    lex.expect(b'(')?;
    let mut params = Vec::new();
    let mut variadic = false;
    if !lex.eat(b')') {
        loop {
            lex.skip_ws();
            if lex.peek() == b'.' {
                for _ in 0..3 {
                    if lex.peek() != b'.' {
                        bail!("'...' near {}", lex.context());
                    }
                    lex.bump();
                }
                variadic = true;
                lex.expect(b')')?;
                break;
            }
            let ty = types.parse_type(lex)?;
            let mut param = Param {
                ty,
                name: None,
                ext: Ext::None,
                byval: None,
            };
            parse_param_attributes(lex, types, &mut param)?;
            params.push(param);
            if lex.eat(b',') {
                continue;
            }
            lex.expect(b')')?;
            break;
        }
    }
    Ok(Header {
        linkage,
        hidden,
        name,
        ret,
        ret_ext,
        params,
        variadic,
        is_define,
    })
}

/// Skips the function attributes between the parameter list and the `{` or the line end, and
/// refuses the ones that change what the code means.
pub(crate) fn skip_function_attributes(lex: &mut Lexer<'_>) -> Res<()> {
    loop {
        lex.skip_ws();
        match lex.peek() {
            b'{' | b'\n' | b';' => return Ok(()),
            0 if lex.eof() => return Ok(()),
            b'#' => {
                lex.bump();
                lex.uint()?;
            }
            b'"' => {
                lex.quoted()?;
                if lex.eat(b'=') {
                    lex.skip_ws();
                    lex.quoted()?;
                }
            }
            b'!' => {
                // Metadata attachments: `!dbg !12`.
                lex.bump();
                let _ = lex.word();
                lex.skip_ws();
                if lex.peek() == b'!' {
                    lex.bump();
                    let _ = lex.word();
                }
            }
            _ => {
                let word = lex.word();
                match word {
                    "" => bail!("a function attribute near {}", lex.context()),
                    "section" | "comdat" | "personality" | "gc" | "prefix" | "prologue" => {
                        bail!("the function attribute '{word}'")
                    }
                    "align" => {
                        lex.uint()?;
                    }
                    _ => {
                        lex.skip_ws();
                        if lex.peek() == b'(' {
                            lex.skip_parens()?;
                        }
                    }
                }
            }
        }
    }
}

/// Skips one constant: up to the `,` or line end that follows it at bracket depth 0.
fn skip_constant(lex: &mut Lexer<'_>) -> Res<()> {
    let mut depth = 0i32;
    loop {
        match lex.peek() {
            b'"' => {
                lex.quoted()?;
                continue;
            }
            b'[' | b'{' | b'(' | b'<' => depth += 1,
            b']' | b'}' | b')' | b'>' => depth -= 1,
            b',' | b'\n' | b';' if depth <= 0 => return Ok(()),
            0 if lex.eof() => return Ok(()),
            _ => {}
        }
        lex.bump();
    }
}

fn parse_global<'a>(
    lex: &mut Lexer<'a>,
    types: &mut Types<'a>,
    syms: &mut Vec<Sym<'a>>,
    by_name: &mut FxMap<&'a str, u32>,
) -> Res<()> {
    lex.bump();
    let name = lex.name()?;
    lex.expect(b'=')?;
    let (linkage, hidden, tls) = parse_modifiers(lex)?;
    let kind = lex.word();
    let sym = match kind {
        "alias" | "ifunc" => Sym {
            name,
            linkage,
            hidden,
            kind: SymKind::Unsupported(if kind == "alias" {
                "an alias"
            } else {
                "an ifunc"
            }),
        },
        "constant" | "global" => {
            let ty = types.parse_type(lex)?;
            lex.skip_ws();
            let init = if matches!(lex.peek(), b',' | b'\n' | b';') || lex.eof() {
                None
            } else {
                let start = lex.pos;
                skip_constant(lex)?;
                Some((start, lex.pos))
            };
            let mut align = 0u32;
            while lex.eat(b',') {
                lex.skip_ws();
                if lex.peek() == b'!' {
                    lex.skip_to_eol();
                    break;
                }
                match lex.word() {
                    "align" => {
                        align = u32::try_from(lex.uint()?)
                            .map_err(|_| Failure::new("an alignment this large"))?;
                    }
                    "section" => bail!("a section on @{name}"),
                    "comdat" => {
                        lex.skip_ws();
                        if lex.peek() == b'(' {
                            lex.skip_parens()?;
                        }
                    }
                    other => bail!("the global attribute '{other}' on @{name}"),
                }
            }
            Sym {
                name,
                linkage,
                hidden,
                kind: SymKind::Global {
                    ty,
                    constant: kind == "constant",
                    tls,
                    align,
                    init,
                },
            }
        }
        other => bail!("the global '@{name}' of kind '{other}'"),
    };
    add_symbol(syms, by_name, sym);
    Ok(())
}

fn add_symbol<'a>(syms: &mut Vec<Sym<'a>>, by_name: &mut FxMap<&'a str, u32>, sym: Sym<'a>) {
    match by_name.get(sym.name) {
        // A definition replaces an earlier declaration; a declaration after a definition adds nothing.
        Some(&id) => {
            if sym.is_defined() && !syms[id as usize].is_defined() {
                syms[id as usize] = sym;
            }
        }
        None => {
            by_name.insert(sym.name, u32::try_from(syms.len()).expect("symbol count"));
            syms.push(sym);
        }
    }
}

/// Splits the module into its types and symbols.
pub(crate) fn scan(text: &str) -> Res<Module<'_>> {
    let mut types = Types::builder(text);
    // First every named type, because a use may come before its definition.
    let mut order = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let mut lex = Lexer::new(text, pos);
        let end = if lex.peek() == b'%' {
            lex.bump();
            let name = lex.name()?;
            lex.expect(b'=')?;
            lex.expect_word("type")?;
            types.declare_named(name, lex.pos);
            order.push(name);
            item_end(text, pos)
        } else if matches!(lex.peek(), b'\n' | b';' | b'!' | b' ' | b'\t' | b'$') {
            lex.skip_line();
            lex.pos
        } else {
            item_end(text, pos)
        };
        pos = end.max(pos + 1);
    }
    types.resolve_all(&order)?;

    let mut syms = Vec::new();
    let mut by_name = FxMap::default();
    let mut pos = 0;
    while pos < text.len() {
        let mut lex = Lexer::new(text, pos);
        let end = item_end(text, pos);
        let rest = &text[pos..];
        match lex.peek() {
            b'd' if rest.starts_with("define ") || rest.starts_with("declare ") => {
                let header = parse_header(&mut lex, &mut types)?;
                skip_function_attributes(&mut lex)?;
                if !header.name.starts_with("llvm.") {
                    let sig = Sig {
                        ret: header.ret,
                        ret_ext: header.ret_ext,
                        params: header
                            .params
                            .iter()
                            .map(|param| (param.ty, param.ext))
                            .collect(),
                        variadic: header.variadic,
                    };
                    add_symbol(
                        &mut syms,
                        &mut by_name,
                        Sym {
                            name: header.name,
                            linkage: header.linkage,
                            hidden: header.hidden,
                            kind: SymKind::Function {
                                sig,
                                header: pos,
                                body_end: header.is_define.then_some(end),
                            },
                        },
                    );
                }
            }
            b'@' => parse_global(&mut lex, &mut types, &mut syms, &mut by_name)?,
            b'%' | b'!' | b'\n' | b';' | b'}' | b' ' | b'\t' | b'$' => {}
            b'a' if rest.starts_with("attributes ") => {}
            b's' if rest.starts_with("source_filename") => {}
            b't' if rest.starts_with("target ") => {}
            b'u' if rest.starts_with("uselistorder") => {}
            _ => bail!("the top-level item near {}", lex.context()),
        }
        pos = end.max(pos + 1);
    }
    Ok(Module {
        text,
        types: types.finish(),
        syms,
        by_name,
    })
}

/// Calls `visit` with the name of every `@symbol` spelled in `text[start..end]`. A name inside a
/// string literal is visited too; a symbol that does not exist is not an error for the caller.
fn for_each_reference<'a>(text: &'a str, start: usize, end: usize, mut visit: impl FnMut(&'a str)) {
    let bytes = text.as_bytes();
    let mut pos = start;
    while let Some(offset) = text[pos..end].find('@') {
        let at = pos + offset + 1;
        let (name_start, name_end) = if bytes.get(at) == Some(&b'"') {
            match text[at + 1..end].find('"') {
                Some(length) => (at + 1, at + 1 + length),
                None => return,
            }
        } else {
            let mut index = at;
            while index < end
                && (bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'_' | b'.' | b'$' | b'-'))
            {
                index += 1;
            }
            (at, index)
        };
        visit(&text[name_start..name_end]);
        pos = name_end.max(at);
    }
}

impl<'a> Module<'a> {
    /// The text a symbol's definition covers.
    pub(crate) fn extent(&self, sym: &Sym<'a>) -> Option<(usize, usize)> {
        match &sym.kind {
            SymKind::Function {
                header, body_end, ..
            } => body_end.map(|end| (*header, end)),
            SymKind::Global { init, .. } => *init,
            SymKind::Unsupported(_) => None,
        }
    }

    /// The symbols a build needs: every definition that other object files can see, and what those
    /// definitions refer to, transitively.
    pub(crate) fn reachable(&self) -> Vec<bool> {
        let mut needed = vec![false; self.syms.len()];
        let mut work: Vec<u32> = Vec::new();
        for (id, sym) in self.syms.iter().enumerate() {
            if sym.is_defined() && sym.linkage != Linkage::Local {
                needed[id] = true;
                work.push(id as u32);
            }
        }
        while let Some(id) = work.pop() {
            let sym = &self.syms[id as usize];
            let Some((start, end)) = self.extent(sym) else {
                continue;
            };
            for_each_reference(self.text, start, end, |name| {
                if let Some(&target) = self.by_name.get(name)
                    && !needed[target as usize]
                {
                    needed[target as usize] = true;
                    work.push(target);
                }
            });
        }
        needed
    }
}
