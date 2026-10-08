//! `tsuzuri repl` (G13): a read-eval-print loop that recompiles. The session is the source
//! text of the accepted declarations and top-level statements. Each input is checked and run
//! as part of a generated `Main.tz` through the pipeline of `tsuzuri run`, so nothing but that
//! source carries over between inputs.

use std::collections::BTreeSet;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::check::CheckedModule;
use crate::check::semantic::SemanticIndex;
use crate::diagnostic::{Diagnostic, DiagnosticSet, Severity, Span};
use crate::driver::{self, BuildOptions, Project};
use crate::syntax::{Expr, ExprKind, Ident, MAX_SOURCE_BYTES, Program, Provenance, TokenKind};

/// The REPL's command-line settings.
#[derive(Clone, Copy, Debug)]
pub struct ReplOptions {
    /// Native executable options. The REPL's default optimization level is 0.
    pub build: BuildOptions,
    /// How long one evaluation may run; `None` is unlimited.
    pub timeout: Option<Duration>,
}

impl Default for ReplOptions {
    fn default() -> Self {
        Self {
            build: BuildOptions {
                optimization: 0,
                trap_info: true,
                ..BuildOptions::default()
            },
            timeout: Some(Duration::from_secs(10)),
        }
    }
}

const COMMANDS: &str = "commands are :type, :load, :list, :reset, and :quit";
const NO_ARGUMENTS: &str = ":list, :reset, and :quit take no arguments";
const NAMESPACES: &str =
    "namespace and using declarations are not supported in the REPL; it has no other modules";
const EXTERNS: &str =
    "extern declarations are not supported in the REPL; use a project with 'tsuzuri run'";
const TESTS: &str = "test declarations are not supported in the REPL; use 'tsuzuri test'";
const MAIN: &str = "'main' cannot be defined in the REPL; enter its body as an input instead";

/// Runs the loop until `:quit` or the end of `input`. Results go to `output` and diagnostics
/// to `errors`; an evaluated program's own stderr goes to the process's stderr. When
/// `interactive`, it writes a banner and prompts to `output`.
pub fn run(
    options: ReplOptions,
    input: impl BufRead,
    mut output: impl Write,
    mut errors: impl Write,
    interactive: bool,
) -> io::Result<()> {
    let mut repl = Repl {
        options,
        session: Session::default(),
        messages: BTreeSet::new(),
    };
    let mut reader = Reader { input };
    if interactive {
        writeln!(
            output,
            "Tsuzuri {} REPL; enter :quit to exit",
            env!("CARGO_PKG_VERSION")
        )?;
    }
    loop {
        if interactive {
            write!(output, "> ")?;
            output.flush()?;
        }
        let prompt = if interactive {
            Some(&mut output as &mut dyn Write)
        } else {
            None
        };
        let Some(entered) = reader.read(prompt)? else {
            break;
        };
        let quit = match entered {
            Entered::Command(line) => repl.command(&line, &mut output, &mut errors)?,
            Entered::Source(text) => {
                repl.source("input", text, &mut output, &mut errors)?;
                false
            }
            Entered::Rejected(diagnostic) => {
                render(&mut errors, [(&diagnostic, "input", "")])?;
                false
            }
        };
        output.flush()?;
        if quit {
            break;
        }
    }
    output.flush()
}

/// What the reader collected for one input.
#[derive(Debug, PartialEq)]
enum Entered {
    /// A line that starts with `:`.
    Command(String),
    /// Source lines joined with `\n`.
    Source(String),
    /// An input that was too large or not UTF-8, already discarded.
    Rejected(Diagnostic),
}

enum Line {
    Text(String),
    TooLong,
    Invalid,
}

struct Reader<R> {
    input: R,
}

impl<R: BufRead> Reader<R> {
    /// The next line without its `\n` and a `\r` before it, or `None` at the end of the input.
    /// A line longer than the source limit is discarded whole.
    fn line(&mut self) -> io::Result<Option<Line>> {
        let mut bytes = Vec::new();
        (&mut self.input)
            .take(MAX_SOURCE_BYTES as u64 + 1)
            .read_until(b'\n', &mut bytes)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        } else if bytes.len() > MAX_SOURCE_BYTES {
            loop {
                let mut rest = Vec::new();
                let count = (&mut self.input)
                    .take(1 << 16)
                    .read_until(b'\n', &mut rest)?;
                if count == 0 || rest.last() == Some(&b'\n') {
                    break;
                }
            }
            return Ok(Some(Line::TooLong));
        }
        Ok(Some(
            String::from_utf8(bytes).map_or(Line::Invalid, Line::Text),
        ))
    }

    /// Reads one input: a command line, or source lines. Source whose first line fails to
    /// parse at its end continues up to an empty line or the end of the input (D7).
    fn read(&mut self, mut prompt: Option<&mut dyn Write>) -> io::Result<Option<Entered>> {
        let first = loop {
            match self.line()? {
                None => return Ok(None),
                Some(Line::Text(line)) if line.trim().is_empty() => {
                    if let Some(prompt) = prompt.as_deref_mut() {
                        write!(prompt, "> ")?;
                        prompt.flush()?;
                    }
                }
                Some(Line::Text(line)) => break line,
                Some(Line::TooLong) => return Ok(Some(too_large())),
                Some(Line::Invalid) => return Ok(Some(invalid_utf8())),
            }
        };
        if first.trim_start().starts_with(':') {
            return Ok(Some(Entered::Command(first)));
        }
        let mut text = first;
        let incomplete = crate::parser::parse(&text)
            .is_err_and(|error| error.span.start >= text.trim_end().len());
        if incomplete {
            loop {
                if let Some(prompt) = prompt.as_deref_mut() {
                    write!(prompt, ". ")?;
                    prompt.flush()?;
                }
                match self.line()? {
                    None => break,
                    Some(Line::Text(line)) if line.is_empty() => break,
                    Some(Line::Text(line)) => {
                        text.push('\n');
                        text.push_str(&line);
                        if text.len() > MAX_SOURCE_BYTES {
                            self.skip_block()?;
                            return Ok(Some(too_large()));
                        }
                    }
                    Some(Line::TooLong) => {
                        self.skip_block()?;
                        return Ok(Some(too_large()));
                    }
                    Some(Line::Invalid) => {
                        self.skip_block()?;
                        return Ok(Some(invalid_utf8()));
                    }
                }
            }
        }
        Ok(Some(Entered::Source(text)))
    }

    /// Discards lines up to an empty line or the end of the input.
    fn skip_block(&mut self) -> io::Result<()> {
        while let Some(line) = self.line()? {
            if matches!(line, Line::Text(text) if text.is_empty()) {
                break;
            }
        }
        Ok(())
    }
}

fn too_large() -> Entered {
    Entered::Rejected(Diagnostic::new(
        "E0003",
        format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit; split the program"),
        Span::default(),
    ))
}

fn invalid_utf8() -> Entered {
    Entered::Rejected(Diagnostic::new(
        "E2001",
        "cannot read the input: it is not valid UTF-8",
        Span::default(),
    ))
}

/// Reports a command's own error at `input:1:1`.
fn command_error(errors: &mut impl Write, line: &str, message: &str) -> io::Result<()> {
    render(
        errors,
        [(
            &Diagnostic::new("E2000", message, Span::default()),
            "input",
            line,
        )],
    )
}

/// Writes rendered errors, separated by empty lines like the other commands' diagnostics.
fn render<'a>(
    errors: &mut impl Write,
    diagnostics: impl IntoIterator<Item = (&'a Diagnostic, &'a str, &'a str)>,
) -> io::Result<()> {
    for (index, (diagnostic, path, text)) in diagnostics.into_iter().enumerate() {
        if index != 0 {
            writeln!(errors)?;
        }
        writeln!(
            errors,
            "{}",
            diagnostic.render_with_severity("error", path, text)
        )?;
    }
    Ok(())
}

/// What an item defines; an input item replaces the session's items with the same key (D4).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// A function, constant, or active pattern case.
    Value(String),
    /// A record, union, type alias, or type class.
    Type(String),
    /// An instance: its class and its type without spaces.
    Instance(String, String),
    /// A top-level `let`.
    Binding(String),
}

/// A declaration, or a top-level statement, as the session keeps it.
#[derive(Clone, Debug)]
struct Item {
    /// The names it defines. A statement such as `c = c + 1;` has none and stays until `:reset`.
    keys: Vec<Key>,
    text: String,
    /// `(offset in text, offset in the input)` at the start of each run copied from the input.
    pieces: Vec<(usize, usize)>,
    /// A `let`'s name and the name's offset in `text`.
    binding: Option<(String, usize)>,
}

/// Maps an offset in an item's text to the input through the item's pieces.
fn input_offset(pieces: &[(usize, usize)], offset: usize) -> usize {
    let (at, from) = pieces
        .iter()
        .rev()
        .find(|(at, _)| *at <= offset)
        .copied()
        .unwrap_or_default();
    from + (offset - at)
}

/// The accepted declarations and top-level statements, in the order `:list` shows them.
#[derive(Clone, Debug, Default)]
struct Session {
    declarations: Vec<Item>,
    bindings: Vec<Item>,
}

impl Session {
    fn items(&self) -> impl Iterator<Item = &Item> {
        self.declarations.iter().chain(&self.bindings)
    }

    /// The source that `:list` shows; `session:<line>` positions count its lines.
    fn listing(&self) -> String {
        self.items()
            .map(|item| format!("{}\n", item.text))
            .collect()
    }

    /// The offset of each item in [`Session::listing`].
    fn offsets(&self) -> Vec<usize> {
        let mut offset = 0;
        self.items()
            .map(|item| {
                let start = offset;
                offset += item.text.len() + 1;
                start
            })
            .collect()
    }

    fn merged(&self, input: &Input) -> Self {
        let pick = |items: &[Item], inputs: &[Item]| {
            merge(items, inputs)
                .into_iter()
                .map(|placed| match placed {
                    Placed::Session(index) => items[index].clone(),
                    Placed::Input(index) => inputs[index].clone(),
                })
                .collect()
        };
        Self {
            declarations: pick(&self.declarations, &input.declarations),
            bindings: pick(&self.bindings, &input.bindings),
        }
    }
}

/// Where an item of the merged program comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placed {
    Session(usize),
    Input(usize),
}

/// The session's items with the input's (D4): an input item takes the place of the first
/// session item that shares one of its keys and removes the other such items; the remaining
/// input items follow, in order. Items of one input never replace each other, so `let c = 1;
/// let c = c + 1` keeps both.
fn merge(session: &[Item], input: &[Item]) -> Vec<Placed> {
    let mut placed: Vec<_> = (0..session.len()).map(Placed::Session).collect();
    for (index, item) in input.iter().enumerate() {
        let mut first = None;
        let mut position = 0;
        while position < placed.len() {
            let shares = matches!(placed[position], Placed::Session(old)
                if session[old].keys.iter().any(|key| item.keys.contains(key)));
            if shares && first.is_some() {
                placed.remove(position);
                continue;
            }
            if shares {
                first = Some(position);
            }
            position += 1;
        }
        match first {
            Some(position) => placed[position] = Placed::Input(index),
            None => placed.push(Placed::Input(index)),
        }
    }
    placed
}

/// The part of an input that is evaluated once and not kept.
#[derive(Clone, Debug, PartialEq)]
enum Body {
    None,
    /// An expression and its offset in the input.
    Expression(String, usize),
    /// Top-level `let!`, `do!`, or `match!` code and its offset in the input.
    Action(String, usize),
}

#[derive(Debug)]
struct Input {
    /// `input`, or the path of `:load`.
    path: String,
    text: String,
    declarations: Vec<Item>,
    bindings: Vec<Item>,
    body: Body,
}

impl Input {
    fn is_empty(&self) -> bool {
        self.declarations.is_empty() && self.bindings.is_empty() && self.body == Body::None
    }

    /// Where generated code and diagnostics without a position point: the start of the body,
    /// else of the first binding or declaration.
    fn anchor(&self) -> usize {
        match &self.body {
            Body::Expression(_, offset) => *offset,
            Body::Action(text, offset) => offset + (text.len() - text.trim_start().len()),
            Body::None => self
                .bindings
                .first()
                .or(self.declarations.first())
                .map_or(0, |item| item.pieces[0].1),
        }
    }
}

fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

/// `from` moved past whitespace and `;`, but not past `to`.
fn skip_separators(text: &str, from: usize, to: usize) -> usize {
    let rest = &text[from..to];
    from + (rest.len()
        - rest
            .trim_start_matches(|c: char| c.is_whitespace() || c == ';')
            .len())
}

/// Splits a parsed input into declarations, top-level statements, and its body (D2).
fn classify(path: &str, text: String, program: &Program) -> Result<Input, Diagnostic> {
    let user = |name: &Ident| name.provenance == Provenance::User;
    let mut rejected: Vec<(Span, &str)> = Vec::new();
    if let Some(namespace) = &program.namespace {
        rejected.push((namespace.path.span, NAMESPACES));
    }
    rejected.extend(
        program
            .usings
            .iter()
            .map(|using| (using.path.span, NAMESPACES)),
    );
    rejected.extend(
        program
            .externs
            .iter()
            .map(|external| (external.name.span, EXTERNS)),
    );
    rejected.extend(
        program
            .extern_types
            .iter()
            .map(|handle| (handle.name.span, EXTERNS)),
    );
    rejected.extend(program.tests.iter().map(|test| (test.name_span, TESTS)));
    rejected.extend(
        program
            .functions
            .iter()
            .filter(|function| user(&function.name) && function.name.text == "main")
            .map(|function| (function.name.span, MAIN)),
    );
    if let Some((span, message)) = rejected.into_iter().min_by_key(|(span, _)| span.start) {
        return Err(Diagnostic::new("E2000", message, span));
    }
    let end = program
        .entry
        .as_ref()
        .map_or(text.len(), |entry| entry.span.start);
    let declarations = declarations(&text, end, program);
    let (bindings, body) = match &program.entry {
        Some(entry) => statements(&text, entry),
        None => (Vec::new(), Body::None),
    };
    Ok(Input {
        path: path.to_owned(),
        text,
        declarations,
        bindings,
        body,
    })
}

/// The declarations before `end`, one item per group of top-level chunks that share a name.
/// A chunk starts at a line whose first token, outside brackets, can begin a declaration (the
/// parser's recovery rule); doc comments and attributes start the chunk of the declaration
/// after them, and `and` continues a recursive group. A signature and its separate `fn` or
/// `let` definition become one item.
fn declarations(text: &str, end: usize, program: &Program) -> Vec<Item> {
    let tokens = crate::lexer::lex(text).unwrap_or_default();
    let mut starts = Vec::new();
    let mut depth = 0usize;
    let mut prefix = false;
    for token in tokens.iter().take_while(|token| token.span.start < end) {
        let line = line_start(text, token.span.start);
        if depth == 0 && token.span.start == line {
            match &token.kind {
                TokenKind::DocComment(_) | TokenKind::At => {
                    if !prefix {
                        starts.push(line);
                    }
                    prefix = true;
                }
                kind if crate::parser::is_top_level_declaration_start(kind) => {
                    if !prefix {
                        starts.push(line);
                    }
                    prefix = false;
                }
                _ => {}
            }
        }
        depth = match token.kind {
            TokenKind::LeftParen
            | TokenKind::LeftBracket
            | TokenKind::LeftBrace
            | TokenKind::LeftList => depth + 1,
            TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::RightList => depth.saturating_sub(1),
            _ => depth,
        };
    }
    let user = |name: &Ident| name.provenance == Provenance::User;
    let mut anchors: Vec<(usize, Key)> = Vec::new();
    let value = |name: &Ident| (name.span.start, Key::Value(name.text.clone()));
    let named_type = |name: &Ident| (name.span.start, Key::Type(name.text.clone()));
    anchors.extend(
        program
            .functions
            .iter()
            .map(|f| &f.name)
            .filter(|name| user(name))
            .map(value),
    );
    anchors.extend(
        program
            .constants
            .iter()
            .map(|constant| value(&constant.name)),
    );
    anchors.extend(
        program
            .active_patterns
            .iter()
            .flat_map(|pattern| &pattern.cases)
            .filter(|case| user(case))
            .map(value),
    );
    anchors.extend(
        program
            .records
            .iter()
            .map(|r| &r.name)
            .filter(|name| user(name))
            .map(named_type),
    );
    anchors.extend(
        program
            .unions
            .iter()
            .map(|u| &u.name)
            .filter(|name| user(name))
            .map(named_type),
    );
    anchors.extend(
        program
            .type_aliases
            .iter()
            .map(|alias| named_type(&alias.name)),
    );
    anchors.extend(program.classes.iter().map(|class| named_type(&class.name)));
    anchors.extend(program.instances.iter().map(|instance| {
        let ty: String = text[instance.ty.span.start..instance.ty.span.end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        (
            instance.class.span.start,
            Key::Instance(instance.class.text.clone(), ty),
        )
    }));
    let chunks: Vec<(usize, usize)> = starts
        .iter()
        .enumerate()
        .map(|(index, start)| (*start, starts.get(index + 1).copied().unwrap_or(end)))
        .collect();
    // Items as chunk lists with their keys; a chunk joins every item that shares a key.
    let mut groups: Vec<(Vec<usize>, Vec<Key>)> = Vec::new();
    for (index, &(start, end)) in chunks.iter().enumerate() {
        let mut keys: Vec<Key> = anchors
            .iter()
            .filter(|(at, _)| (start..end).contains(at))
            .map(|(_, key)| key.clone())
            .collect();
        keys.extend(signature_names(&tokens, start, end));
        let mut target: Option<usize> = None;
        let mut group = 0;
        while group < groups.len() {
            if !groups[group].1.iter().any(|key| keys.contains(key)) {
                group += 1;
            } else if let Some(first) = target {
                let (members, more) = groups.remove(group);
                groups[first].0.extend(members);
                groups[first].1.extend(more);
            } else {
                target = Some(group);
                group += 1;
            }
        }
        match target {
            Some(first) => {
                groups[first].0.push(index);
                groups[first].1.extend(keys);
            }
            None => groups.push((vec![index], keys)),
        }
    }
    groups
        .into_iter()
        .map(|(mut members, mut keys)| {
            members.sort_unstable();
            keys.sort();
            keys.dedup();
            let mut item = Item {
                keys,
                text: String::new(),
                pieces: Vec::new(),
                binding: None,
            };
            for member in members {
                let (start, end) = chunks[member];
                if !item.text.is_empty() {
                    item.text.push('\n');
                }
                item.pieces.push((item.text.len(), start));
                item.text.push_str(text[start..end].trim_end());
            }
            item
        })
        .collect()
}

/// The names after `def` and `and` (past `rec`) outside brackets in `start..end`: a signature
/// whose definition is a separate `fn` or `let` has no AST name in its own chunk.
fn signature_names(tokens: &[crate::syntax::Token], start: usize, end: usize) -> Vec<Key> {
    let mut names = Vec::new();
    let mut depth = 0usize;
    let chunk: Vec<_> = tokens
        .iter()
        .filter(|token| (start..end).contains(&token.span.start))
        .collect();
    for (index, token) in chunk.iter().enumerate() {
        match token.kind {
            TokenKind::LeftParen
            | TokenKind::LeftBracket
            | TokenKind::LeftBrace
            | TokenKind::LeftList => depth += 1,
            TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::RightList => depth = depth.saturating_sub(1),
            TokenKind::Def | TokenKind::And if depth == 0 => {
                let mut next = index + 1;
                if chunk
                    .get(next)
                    .is_some_and(|token| token.kind == TokenKind::Rec)
                {
                    next += 1;
                }
                if let Some(TokenKind::Ident(name)) = chunk.get(next).map(|token| &token.kind) {
                    names.push(Key::Value(name.clone()));
                }
            }
            _ => {}
        }
    }
    names
}

/// The entry code's top-level statements (named `let`s and `;`-ended statements, kept in the
/// session) and its body, which is evaluated once.
fn statements(text: &str, entry: &Expr) -> (Vec<Item>, Body) {
    let action = |start: usize| {
        // Keep the first line's column, so the following lines keep their layout.
        let column = start - line_start(text, start);
        Body::Action(
            format!("{}{}", " ".repeat(column), text[start..].trim_end()),
            start - column,
        )
    };
    let ExprKind::Block { bindings, result } = &entry.kind else {
        return match entry.kind {
            ExprKind::Computation(..) | ExprKind::ComputationBoundary(_) => {
                (Vec::new(), action(entry.span.start))
            }
            _ => (
                Vec::new(),
                Body::Expression(
                    text[entry.span.start..].trim_end().to_owned(),
                    entry.span.start,
                ),
            ),
        };
    };
    let mut items = Vec::new();
    let mut boundary = entry.span.start;
    for binding in bindings {
        let end = binding.value.span.end;
        let start = skip_separators(text, boundary, end);
        let named = binding.name.provenance == Provenance::User;
        items.push(Item {
            keys: if named {
                vec![Key::Binding(binding.name.text.clone())]
            } else {
                Vec::new()
            },
            text: text[start..end].to_owned(),
            pieces: vec![(0, start)],
            binding: named.then(|| (binding.name.text.clone(), binding.name.span.start - start)),
        });
        boundary = end;
    }
    let start = skip_separators(text, boundary, text.len());
    let body = match result.kind {
        ExprKind::Unit if result.span.start == result.span.end => Body::None,
        ExprKind::Computation(..) | ExprKind::ComputationBoundary(_) => action(start),
        _ => Body::Expression(text[start..].trim_end().to_owned(), start),
    };
    (items, body)
}

/// What a part of a generated `Main.tz` maps back to.
#[derive(Clone, Debug, PartialEq)]
enum Origin {
    /// A session item at this offset of `:list`.
    Session(usize),
    /// An input item, through its pieces.
    Input(Vec<(usize, usize)>),
    /// Input text copied whole from this offset: the expression or the action.
    Body(usize),
    /// Generated text, shown at the input's anchor.
    Wrapper,
    /// The `Display.display (ref it)` line of the shown value.
    Display,
}

#[derive(Clone, Debug)]
struct Segment {
    start: usize,
    end: usize,
    origin: Origin,
}

/// How a generated `Main.tz` ends.
#[derive(Clone, Copy)]
enum Tail<'a> {
    None,
    /// `let it = (E)`, which types the expression.
    Check(&'a str, usize),
    /// `let it = (E)` and `Display.display (ref it)`, which also prints its value (D3).
    Show(&'a str, usize),
    Action(&'a str, usize),
}

/// A generated `Main.tz`: the merged declarations, then the merged statements, then the tail.
#[derive(Debug, Default)]
struct Generated {
    text: String,
    segments: Vec<Segment>,
    /// The offset of `it` in `let it = (`.
    it: Option<usize>,
    /// The input's named `let`s with the offsets of their names, in the input's order.
    bindings: Vec<(String, usize)>,
    /// See [`Input::anchor`].
    anchor: usize,
}

impl Generated {
    fn push(&mut self, origin: Origin, text: &str) {
        let start = self.text.len();
        self.text.push_str(text);
        self.segments.push(Segment {
            start,
            end: self.text.len(),
            origin,
        });
    }

    /// The path, text, and offset where a diagnostic at `span` is shown (D9).
    fn locate<'a>(
        &self,
        span: Span,
        project: &'a Project,
        input: &'a Input,
        listing: &'a str,
    ) -> (&'a str, &'a str, usize) {
        let at_input = |offset| (input.path.as_str(), input.text.as_str(), offset);
        let Some(source) = span.source else {
            return at_input(self.anchor);
        };
        if source != project.root {
            let file = &project.sources[source];
            return (
                file.relative_path.to_str().unwrap_or_default(),
                &file.text,
                span.start,
            );
        }
        let Some(segment) = self
            .segments
            .iter()
            .find(|segment| span.start < segment.end)
            .or(self.segments.last())
        else {
            return at_input(self.anchor);
        };
        let inside = span.start.saturating_sub(segment.start);
        match &segment.origin {
            Origin::Session(offset) => ("session", listing, offset + inside),
            Origin::Input(pieces) => at_input(input_offset(pieces, inside)),
            Origin::Body(offset) => at_input(offset + inside),
            Origin::Wrapper | Origin::Display => at_input(self.anchor),
        }
    }

    fn in_display(&self, span: Span, project: &Project) -> bool {
        span.source == Some(project.root)
            && self.segments.iter().any(|segment| {
                segment.origin == Origin::Display
                    && (segment.start..segment.end).contains(&span.start)
            })
    }
}

/// The program for `input` after the session's items, with D4's merge, ending with `tail`.
fn generate(session: &Session, input: &Input, tail: Tail<'_>) -> Generated {
    let offsets = session.offsets();
    let (declaration_offsets, binding_offsets) = offsets.split_at(session.declarations.len());
    let mut generated = Generated {
        anchor: input.anchor(),
        ..Generated::default()
    };
    let mut names = Vec::new();
    for (items, offsets, inputs) in [
        (
            &session.declarations,
            declaration_offsets,
            &input.declarations,
        ),
        (&session.bindings, binding_offsets, &input.bindings),
    ] {
        for placed in merge(items, inputs) {
            let (text, origin) = match placed {
                Placed::Session(index) => (&items[index].text, Origin::Session(offsets[index])),
                Placed::Input(index) => {
                    let item = &inputs[index];
                    if let Some((name, at)) = &item.binding {
                        names.push((index, name.clone(), generated.text.len() + at));
                    }
                    (&item.text, Origin::Input(item.pieces.clone()))
                }
            };
            generated.push(origin, &format!("{text}\n"));
        }
    }
    names.sort_by_key(|(index, ..)| *index);
    generated.bindings = names
        .into_iter()
        .map(|(_, name, offset)| (name, offset))
        .collect();
    match tail {
        Tail::None => {}
        Tail::Check(expression, offset) | Tail::Show(expression, offset) => {
            generated.it = Some(generated.text.len() + "let ".len());
            generated.push(Origin::Wrapper, "let it = (\n");
            generated.push(Origin::Body(offset), expression);
            generated.push(Origin::Wrapper, "\n)\n");
            if let Tail::Show(..) = tail {
                generated.push(Origin::Display, "Display.display (ref it)\n");
            }
        }
        Tail::Action(action, offset) => {
            generated.push(Origin::Body(offset), action);
            generated.push(Origin::Wrapper, "\n");
        }
    }
    generated
}

/// The path of the generated `Main.tz`. It names no file: the program reads no other source,
/// and a trap that the program reports itself names this path.
fn main_path() -> PathBuf {
    Path::new("<repl>").join("Main.tz")
}

/// Checks `text` as the whole program, with the semantic index when `index`.
fn analyze(
    text: &str,
    index: bool,
) -> (
    Project,
    Result<(CheckedModule, Option<SemanticIndex>), DiagnosticSet>,
) {
    let project = Project::single_main(main_path(), text.to_owned());
    let mut semantic = index.then(SemanticIndex::default);
    let result = {
        let inputs: Vec<_> = project
            .sources
            .iter()
            .map(|source| crate::SourceInput {
                path: source
                    .relative_path
                    .to_str()
                    .expect("REPL and std paths are UTF-8"),
                text: &source.text,
                origin: source.origin,
                namespace: &source.namespace,
            })
            .collect();
        crate::analyze_inputs_indexed_all(&inputs, semantic.as_mut())
    };
    (project, result.map(|module| (module, semantic)))
}

/// The type in the semantic entry `name: T` of the name at `offset` of the generated program.
fn type_at(index: &SemanticIndex, offset: usize, name: &str) -> Option<String> {
    let prefix = format!("{name}: ");
    index
        .entries
        .iter()
        .find(|entry| {
            entry.span.source == Some(0)
                && entry.span.start == offset
                && entry.detail.starts_with(&prefix)
        })
        .map(|entry| entry.detail[prefix.len()..].to_owned())
}

struct Repl {
    options: ReplOptions,
    session: Session,
    /// Build messages already shown, such as a disabled cache, so that each shows once.
    messages: BTreeSet<String>,
}

type Checked = (Project, CheckedModule, Option<SemanticIndex>);

impl Repl {
    /// Runs a command line; returns whether it was `:quit`.
    fn command(
        &mut self,
        line: &str,
        output: &mut impl Write,
        errors: &mut impl Write,
    ) -> io::Result<bool> {
        let start = line.len() - line.trim_start().len();
        let name_end = line[start..]
            .find(char::is_whitespace)
            .map_or(line.len(), |end| start + end);
        let name = &line[start + 1..name_end];
        let rest = &line[name_end..];
        let offset = name_end + (rest.len() - rest.trim_start().len());
        let argument = rest.trim();
        match name {
            "quit" | "list" | "reset" if !argument.is_empty() => {
                command_error(errors, line, NO_ARGUMENTS)?
            }
            "quit" => return Ok(true),
            "list" => output.write_all(self.session.listing().as_bytes())?,
            "reset" => self.session = Session::default(),
            "type" if argument.is_empty() => {
                command_error(errors, line, ":type requires an expression")?
            }
            "type" => self.type_of(line, offset, output, errors)?,
            "load"
                if Path::new(argument)
                    .extension()
                    .is_none_or(|extension| extension != "tz") =>
            {
                command_error(errors, line, ":load requires one .tz file path")?
            }
            "load" => match driver::read_source(Path::new(argument)) {
                Ok(text) => {
                    let text = text
                        .strip_prefix('\u{feff}')
                        .unwrap_or(&text)
                        .replace("\r\n", "\n");
                    self.source(argument, text, output, errors)?;
                }
                Err(error) => render(errors, [(&error, "input", line)])?,
            },
            _ => command_error(
                errors,
                line,
                &format!("unknown command ':{name}'; {COMMANDS}"),
            )?,
        }
        Ok(false)
    }

    /// `:type E`: prints the type of the expression at `offset` of `line` without running it.
    fn type_of(
        &mut self,
        line: &str,
        offset: usize,
        output: &mut impl Write,
        errors: &mut impl Write,
    ) -> io::Result<()> {
        let expression = line[offset..].trim_end();
        let parsed = crate::parser::parse(expression).and_then(|program| {
            classify("input", expression.to_owned(), &program)
                .ok()
                .filter(|input| {
                    input.declarations.is_empty()
                        && input.bindings.is_empty()
                        && matches!(input.body, Body::Expression(..))
                })
                .ok_or_else(|| {
                    Diagnostic::new("E2000", ":type requires an expression", Span::default())
                })
        });
        if let Err(mut error) = parsed {
            error.span = Span::new(error.span.start + offset, error.span.end + offset);
            if error.code == "E2000" {
                error.span = Span::default();
            }
            return render(errors, [(&error, "input", line)]);
        }
        let input = Input {
            path: "input".to_owned(),
            text: line.to_owned(),
            declarations: Vec::new(),
            bindings: Vec::new(),
            body: Body::Expression(expression.to_owned(), offset),
        };
        let check = generate(&self.session, &input, Tail::Check(expression, offset));
        let Some((_, _, Some(index))) = self.check(&check, &input, errors, true)? else {
            return Ok(());
        };
        writeln!(output, "{}", it_type(&index, &check))
    }

    /// Evaluates one source input (D2–D5). A failure reports its diagnostics and leaves the
    /// session as it was.
    fn source(
        &mut self,
        path: &str,
        text: String,
        output: &mut impl Write,
        errors: &mut impl Write,
    ) -> io::Result<()> {
        let program = match crate::parser::parse(&text) {
            Ok(program) => program,
            Err(error) => return render(errors, [(&error, path, text.as_str())]),
        };
        let input = match classify(path, text.clone(), &program) {
            Ok(input) => input,
            Err(error) => return render(errors, [(&error, path, text.as_str())]),
        };
        if input.is_empty() {
            return Ok(());
        }
        let accepted = match input.body.clone() {
            Body::None => self.statements(&input, output, errors)?,
            Body::Expression(expression, offset) => {
                self.expression(&input, &expression, offset, output, errors)?
            }
            Body::Action(action, offset) => {
                let generated = generate(&self.session, &input, Tail::Action(&action, offset));
                match self.check(&generated, &input, errors, false)? {
                    Some(checked) => match self.evaluate(&checked, &generated, &input, errors)? {
                        Some(stdout) => {
                            output.write_all(&stdout)?;
                            true
                        }
                        None => false,
                    },
                    None => false,
                }
            }
        };
        if accepted {
            self.session = self.session.merged(&input);
        }
        Ok(())
    }

    /// Declarations and statements without an expression: statements run once now, so that a
    /// trap rejects them, and each new `let` shows its type.
    fn statements(
        &mut self,
        input: &Input,
        output: &mut impl Write,
        errors: &mut impl Write,
    ) -> io::Result<bool> {
        let generated = generate(&self.session, input, Tail::None);
        let index = !generated.bindings.is_empty();
        let Some(checked) = self.check(&generated, input, errors, index)? else {
            return Ok(false);
        };
        if !input.bindings.is_empty()
            && self
                .evaluate(&checked, &generated, input, errors)?
                .is_none()
        {
            return Ok(false);
        }
        if let Some(index) = &checked.2 {
            for (name, offset) in &generated.bindings {
                let ty = type_at(index, *offset, name).unwrap_or_else(|| "?".to_owned());
                writeln!(output, "{name}: {ty}")?;
            }
        }
        Ok(true)
    }

    /// An expression: `it: T = value`, `it: T` without a `Display` instance, or nothing for
    /// `unit` (D3). The program that also shows the value is checked first: for most
    /// expressions it is the only analysis, and the plain `let it = (E)` is checked only when
    /// it fails (Phase 3). The outcome is the same as checking the plain program first.
    fn expression(
        &mut self,
        input: &Input,
        expression: &str,
        offset: usize,
        output: &mut impl Write,
        errors: &mut impl Write,
    ) -> io::Result<bool> {
        let show = generate(&self.session, input, Tail::Show(expression, offset));
        if show.text.len() > MAX_SOURCE_BYTES {
            // Reports the limit like any other program.
            self.check(&show, input, errors, false)?;
            return Ok(false);
        }
        let (project, shown) = analyze(&show.text, true);
        let failed = match shown {
            Ok((module, index)) => {
                let ty = it_type(index.as_ref().expect("checked with an index"), &show);
                let Some(stdout) = self.evaluate(&(project, module, None), &show, input, errors)?
                else {
                    return Ok(false);
                };
                if ty != "unit" {
                    let value = String::from_utf8_lossy(&stdout);
                    let value = value.strip_suffix('\n').unwrap_or(&value);
                    writeln!(output, "it: {ty} = {value}")?;
                }
                return Ok(true);
            }
            Err(set) => set,
        };
        let check = generate(&self.session, input, Tail::Check(expression, offset));
        let Some(checked) = self.check(&check, input, errors, true)? else {
            return Ok(false);
        };
        let ty = it_type(checked.2.as_ref().expect("checked with an index"), &check);
        let hidden = failed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .all(|diagnostic| {
                diagnostic.code == "E1005" && show.in_display(diagnostic.span, &project)
            });
        if ty != "unit" && !hidden {
            self.report(errors, &failed, &show, &project, input)?;
            return Ok(false);
        }
        if self.evaluate(&checked, &check, input, errors)?.is_none() {
            return Ok(false);
        }
        if ty != "unit" {
            writeln!(output, "it: {ty}")?;
        }
        Ok(true)
    }

    /// Checks a generated program; on failure reports the errors at the input or the session.
    fn check(
        &self,
        generated: &Generated,
        input: &Input,
        errors: &mut impl Write,
        index: bool,
    ) -> io::Result<Option<Checked>> {
        if generated.text.len() > MAX_SOURCE_BYTES {
            let error = Diagnostic::new(
                "E0003",
                format!(
                    "source exceeds the {MAX_SOURCE_BYTES}-byte limit; split the program or use :reset"
                ),
                Span::new(generated.anchor, generated.anchor),
            );
            render(errors, [(&error, input.path.as_str(), input.text.as_str())])?;
            return Ok(None);
        }
        let (project, result) = analyze(&generated.text, index);
        match result {
            Ok((module, index)) => Ok(Some((project, module, index))),
            Err(set) => {
                self.report(errors, &set, generated, &project, input)?;
                Ok(None)
            }
        }
    }

    /// Builds and runs a checked program like `tsuzuri run`; returns its stdout, or `None`
    /// after reporting a failure.
    fn evaluate(
        &mut self,
        (project, module, _): &Checked,
        generated: &Generated,
        input: &Input,
        errors: &mut impl Write,
    ) -> io::Result<Option<Vec<u8>>> {
        match driver::run_captured(module, project, self.options.build, self.options.timeout) {
            Ok((messages, stdout)) => {
                for message in messages {
                    let message = message.trim().to_owned();
                    if self.messages.insert(message.clone()) {
                        writeln!(errors, "{message}")?;
                    }
                }
                Ok(Some(stdout))
            }
            Err(error) => {
                let set = DiagnosticSet::from_diagnostics([error], 0);
                self.report(errors, &set, generated, project, input)?;
                Ok(None)
            }
        }
    }

    fn report(
        &self,
        errors: &mut impl Write,
        set: &DiagnosticSet,
        generated: &Generated,
        project: &Project,
        input: &Input,
    ) -> io::Result<()> {
        let listing = self.session.listing();
        let located: Vec<_> = set
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(|diagnostic| {
                let (path, text, offset) =
                    generated.locate(diagnostic.span, project, input, &listing);
                (
                    Diagnostic {
                        span: Span::new(offset, offset),
                        ..diagnostic.clone()
                    },
                    path,
                    text,
                )
            })
            .collect();
        render(
            errors,
            located
                .iter()
                .map(|(diagnostic, path, text)| (diagnostic, *path, *text)),
        )?;
        if let Some(note) = set.omission_note() {
            writeln!(errors, "\nerror: {note}")?;
        }
        Ok(())
    }
}

/// The type of `it` in a checked `let it = (E)` program.
fn it_type(index: &SemanticIndex, generated: &Generated) -> String {
    generated
        .it
        .and_then(|offset| type_at(index, offset, "it"))
        .unwrap_or_else(|| "?".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn classify_text(text: &str) -> Input {
        let program = crate::parser::parse(text).expect("the input parses");
        classify("input", text.to_owned(), &program).expect("the input is accepted")
    }

    fn texts(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.text.as_str()).collect()
    }

    /// Runs the loop over `input`; these inputs never reach Clang.
    fn session(input: &str) -> (String, String) {
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        run(
            ReplOptions::default(),
            Cursor::new(input),
            &mut output,
            &mut errors,
            false,
        )
        .unwrap();
        (
            String::from_utf8(output).unwrap(),
            String::from_utf8(errors).unwrap(),
        )
    }

    #[test]
    fn continues_incomplete_input() {
        let mut reader = Reader {
            input: Cursor::new(
                "let word = (\n    match 3 with\n    | 3 -> \"three\"\r\n    | _ -> \"other\"\n)\n\n\n  \nsquare base\n:list\nlet x = (\n:quit\n",
            ),
        };
        assert_eq!(
            reader.read(None).unwrap(),
            Some(Entered::Source(
                "let word = (\n    match 3 with\n    | 3 -> \"three\"\n    | _ -> \"other\"\n)"
                    .into()
            ))
        );
        assert_eq!(
            reader.read(None).unwrap(),
            Some(Entered::Source("square base".into()))
        );
        assert_eq!(
            reader.read(None).unwrap(),
            Some(Entered::Command(":list".into()))
        );
        // A continued input takes a line that starts with ':' as source.
        assert_eq!(
            reader.read(None).unwrap(),
            Some(Entered::Source("let x = (\n:quit".into()))
        );
        assert_eq!(reader.read(None).unwrap(), None);
    }

    #[test]
    fn reports_errors_before_the_end() {
        let mut reader = Reader {
            input: Cursor::new("let = 3\n1 + 1\n"),
        };
        assert_eq!(
            reader.read(None).unwrap(),
            Some(Entered::Source("let = 3".into()))
        );
        let (output, errors) = session("let = 3\n");
        assert_eq!(output, "");
        assert!(
            errors.starts_with("input:1:5: error[E0002]: expected an identifier"),
            "{errors}"
        );
        let mut long = Reader {
            input: Cursor::new(format!("\"{}\"\n1\n", "a".repeat(MAX_SOURCE_BYTES))),
        };
        assert!(
            matches!(long.read(None).unwrap(), Some(Entered::Rejected(error)) if error.code == "E0003")
        );
        assert_eq!(long.read(None).unwrap(), Some(Entered::Source("1".into())));
    }

    #[test]
    fn splits_declarations_bindings_and_expression() {
        let input =
            classify_text("def square :: i64 -> i64 = \\x -> x * x\nlet base = 6\nsquare base");
        assert_eq!(
            texts(&input.declarations),
            ["def square :: i64 -> i64 = \\x -> x * x"]
        );
        assert_eq!(input.declarations[0].keys, [Key::Value("square".into())]);
        assert_eq!(texts(&input.bindings), ["let base = 6"]);
        assert_eq!(input.bindings[0].keys, [Key::Binding("base".into())]);
        assert_eq!(input.bindings[0].binding, Some(("base".into(), 4)));
        assert_eq!(input.body, Body::Expression("square base".into(), 52));
    }

    #[test]
    fn keeps_bindings_without_expression() {
        let input = classify_text("let a = 1; let b = a;  c = c + 1;");
        assert_eq!(
            texts(&input.bindings),
            ["let a = 1", "let b = a", "c = c + 1"]
        );
        assert_eq!(input.bindings[2].keys, []);
        assert_eq!(input.bindings[2].binding, None);
        assert_eq!(input.body, Body::None);
        let commented = classify_text("let x = 1 // note\n");
        assert_eq!(texts(&commented.bindings), ["let x = 1"]);
        assert_eq!(commented.body, Body::None);
        assert!(classify_text("// only a comment").is_empty());
    }

    #[test]
    fn classifies_bang_statements_as_actions() {
        let action = classify_text("do! IO.write_line \"hello\"");
        assert_eq!(
            action.body,
            Body::Action("do! IO.write_line \"hello\"".into(), 0)
        );
        // `let` and `do!` together are one computation that the session does not keep.
        let mixed = classify_text("let base = 6\ndo! IO.write_line (to_string base)");
        assert!(mixed.bindings.is_empty());
        assert!(matches!(mixed.body, Body::Action(..)));
        let value = classify_text("IO.write_line \"y\"");
        assert_eq!(
            value.body,
            Body::Expression("IO.write_line \"y\"".into(), 0)
        );
    }

    #[test]
    fn keys_instances_by_class_and_type() {
        let input = classify_text(
            "record Point { x: i64, y: i64 }\ninstance Display<Point> {\n    fn display p = \"p\"\n}\ninstance Display< Maybe<Point> > {\n    fn display p = \"m\"\n}\n",
        );
        let keys: Vec<_> = input
            .declarations
            .iter()
            .map(|item| item.keys.clone())
            .collect();
        assert_eq!(
            keys,
            [
                vec![Key::Type("Point".into())],
                vec![Key::Instance("Display".into(), "Point".into())],
                vec![Key::Instance("Display".into(), "Maybe<Point>".into())],
            ]
        );
    }

    #[test]
    fn rejects_extern_test_main_and_namespaces() {
        for (text, message) in [
            ("extern def drop_log :: i64 -> unit", EXTERNS),
            ("extern type Handle", EXTERNS),
            ("test \"adds numbers\" = assert (1 + 2 == 3)", TESTS),
            ("def main :: unit -> i32 = \\() -> 1", MAIN),
            ("namespace Sample::Shapes\nlet x = 1", NAMESPACES),
            ("using Sample::Shapes\nlet x = 1", NAMESPACES),
        ] {
            let program = crate::parser::parse(text).expect(text);
            let error = classify("input", text.to_owned(), &program).expect_err(text);
            assert_eq!(
                (error.code, error.message.as_str()),
                ("E2000", message),
                "{text}"
            );
        }
        let (output, errors) = session("def main :: unit -> i32 = \\() -> 1\n");
        assert_eq!(output, "");
        assert!(
            errors.starts_with(&format!("input:1:5: error[E2000]: {MAIN}")),
            "{errors}"
        );
    }

    #[test]
    fn groups_signatures_recursive_groups_and_doc_comments() {
        let input = classify_text(
            "/// Doubles.\n@cpu [\"avx2\"]\ndef double :: i64 -> i64 = \\x -> x + x\ndef f :: i64 -> i64\ndef rec even :: i64 -> bool = \\n -> if n == 0 then true else odd (n - 1)\nand odd :: i64 -> bool = \\n -> if n == 0 then false else even (n - 1)\nfn f x = double x\ndef a :: i64 = 1; def b :: i64 = 2\n",
        );
        assert_eq!(
            texts(&input.declarations),
            [
                "/// Doubles.\n@cpu [\"avx2\"]\ndef double :: i64 -> i64 = \\x -> x + x",
                "def f :: i64 -> i64\nfn f x = double x",
                "def rec even :: i64 -> bool = \\n -> if n == 0 then true else odd (n - 1)\nand odd :: i64 -> bool = \\n -> if n == 0 then false else even (n - 1)",
                "def a :: i64 = 1; def b :: i64 = 2",
            ]
        );
        let keys: Vec<_> = input
            .declarations
            .iter()
            .map(|item| item.keys.clone())
            .collect();
        assert_eq!(keys[1], [Key::Value("f".into())]);
        assert_eq!(
            keys[2],
            [Key::Value("even".into()), Key::Value("odd".into())]
        );
        assert_eq!(keys[3], [Key::Value("a".into()), Key::Value("b".into())]);
        // A file's `#!` line is not part of its first declaration.
        let script = classify_text("#!/usr/bin/env tsuzuri script\ndef one :: i32 = 1\n");
        assert_eq!(texts(&script.declarations), ["def one :: i32 = 1"]);
        // The definition of `f` maps back to its own line.
        assert_eq!(
            input_offset(&input.declarations[1].pieces, 20),
            input.text.find("fn f").unwrap()
        );
    }

    #[test]
    fn replaces_in_place_and_keeps_input_shadowing() {
        let mut current = Session::default();
        for text in [
            "let a = 2\nlet b = a * 10",
            "let a = 5",
            "let c = 1; let c = c + 1",
        ] {
            current = current.merged(&classify_text(text));
        }
        assert_eq!(
            texts(&current.bindings),
            ["let a = 5", "let b = a * 10", "let c = 1", "let c = c + 1"]
        );
        // An input item replaces the first item that shares a key and removes the others.
        current = current.merged(&classify_text("let c = 7"));
        assert_eq!(
            texts(&current.bindings),
            ["let a = 5", "let b = a * 10", "let c = 7"]
        );
        let mut groups = Session::default();
        for text in [
            "def rec even :: i64 -> bool = \\n -> if n == 0 then true else odd (n - 1)\nand odd :: i64 -> bool = \\n -> if n == 0 then false else even (n - 1)",
            "def odd :: i64 -> bool = \\n -> n % 2 == 1",
        ] {
            groups = groups.merged(&classify_text(text));
        }
        assert_eq!(
            texts(&groups.declarations),
            ["def odd :: i64 -> bool = \\n -> n % 2 == 1"]
        );
    }

    #[test]
    fn generates_declarations_before_bindings() {
        let current = Session::default().merged(&classify_text("let base = 6"));
        let input = classify_text("def square :: i64 -> i64 = \\x -> x * x");
        let generated = generate(&current, &input, Tail::None);
        assert_eq!(
            generated.text,
            "def square :: i64 -> i64 = \\x -> x * x\nlet base = 6\n"
        );
        let show = generate(&current, &classify_text("base"), Tail::Show("base", 0));
        assert_eq!(
            show.text,
            "let base = 6\nlet it = (\nbase\n)\nDisplay.display (ref it)\n"
        );
        assert_eq!(&show.text[show.it.unwrap()..][..2], "it");
        // The generated program checks; `tsuzuri run` would see the same Main.tz.
        assert!(analyze(&show.text, false).1.is_ok());
    }

    #[test]
    fn maps_spans_to_input_and_session() {
        let current = Session::default()
            .merged(&classify_text("def square :: i64 -> i64 = \\x -> x * x"))
            .merged(&classify_text(
                "def quad :: i64 -> i64 = \\x -> square (square x)",
            ));
        let listing = current.listing();
        let input = classify_text("def square :: bool -> bool = \\x -> x");
        let generated = generate(&current, &input, Tail::None);
        let (project, result) = analyze(&generated.text, false);
        let errors = result.expect_err("quad no longer checks").diagnostics;
        let (path, text, offset) = generated.locate(errors[0].span, &project, &input, &listing);
        assert_eq!(path, "session");
        assert_eq!(crate::diagnostic::location(text, offset).0, 2);
        let (output, errors) = session(
            "def square :: i64 -> i64 = \\x -> x * x\ndef quad :: i64 -> i64 = \\x -> square (square x)\ndef square :: bool -> bool = \\x -> x\n:list\n  1 + true\n",
        );
        assert_eq!(
            output,
            "def square :: i64 -> i64 = \\x -> x * x\ndef quad :: i64 -> i64 = \\x -> square (square x)\n"
        );
        assert!(errors.starts_with("session:2:"), "{errors}");
        assert!(errors.contains("\ninput:1:"), "{errors}");
        // Generated text points at the start of the expression.
        let input = classify_text("1 + 1");
        let show = generate(&Session::default(), &input, Tail::Show("1 + 1", 0));
        let display = Span::new(show.text.find("Display").unwrap(), 0).in_source(0);
        let project = Project::single_main(main_path(), show.text.clone());
        assert_eq!(
            show.locate(display, &project, &input, ""),
            ("input", "1 + 1", 0)
        );
        assert!(show.in_display(display, &project));
    }

    #[test]
    fn single_main_project_reads_no_other_source() {
        let project = Project::single_main(main_path(), "let x = Json.Null\n0".into());
        assert_eq!(project.input(), main_path());
        assert_eq!(project.root, 0);
        let user: Vec<_> = project
            .sources
            .iter()
            .filter(|source| source.origin == crate::check::ModuleOrigin::User)
            .map(|source| source.name.as_str())
            .collect();
        assert_eq!(user, ["Main"]);
        // The opt-in std modules come only when the program names them, as for `tsuzuri run`.
        assert!(project.sources.iter().any(|source| source.name == "Json"));
        let plain = Project::single_main(main_path(), "0".into());
        assert!(!plain.sources.iter().any(|source| source.name == "Json"));
    }

    #[test]
    fn reads_types_from_the_semantic_index() {
        let declaration = "def square :: i64 -> i64 = \\x -> x * x";
        let current =
            Session::default().merged(&classify_text(&format!("{declaration}\nlet base = 6")));
        for (expression, expected) in [
            ("square base", "i64"),
            ("square", "i64 -> i64"),
            ("\"hi\"", "string"),
            ("IO.write_line \"y\"", "IO<unit>"),
            ("1 + 1", "i32"),
            (
                "(\n    match base with\n    | 6 -> \"six\"\n    | _ -> \"other\"\n)",
                "string",
            ),
        ] {
            let input = classify_text(expression);
            let check = generate(&current, &input, Tail::Check(expression, 0));
            let (_, result) = analyze(&check.text, true);
            let (_, index) = result.unwrap_or_else(|errors| panic!("{expression}: {errors:?}"));
            assert_eq!(it_type(&index.unwrap(), &check), expected, "{expression}");
        }
        let (output, errors) = session(&format!(
            "{declaration}\n:type square 2\n:type 1 + true\n:type let y = 1\n"
        ));
        assert_eq!(output, "i64\n");
        assert!(errors.starts_with("input:1:"), "{errors}");
        assert!(
            errors.contains("error[E2000]: :type requires an expression"),
            "{errors}"
        );
        // The shown program alone types a displayable expression; without a Display instance it
        // fails only at its display line, which sends the REPL to the plain program.
        let show = generate(
            &current,
            &classify_text("square base"),
            Tail::Show("square base", 0),
        );
        let (_, result) = analyze(&show.text, true);
        assert_eq!(it_type(&result.unwrap().1.unwrap(), &show), "i64");
        let show = generate(&current, &classify_text("square"), Tail::Show("square", 0));
        let (project, result) = analyze(&show.text, true);
        let errors = result.unwrap_err().diagnostics;
        assert!(errors.iter().any(|error| error.severity == Severity::Error));
        assert!(
            errors
                .iter()
                .filter(|error| error.severity == Severity::Error)
                .all(|error| error.code == "E1005" && show.in_display(error.span, &project)),
            "{errors:?}"
        );
    }

    #[test]
    fn prompts_only_when_interactive() {
        let mut output = Vec::new();
        let input = Cursor::new("def one :: i32 = 1\n\n");
        run(ReplOptions::default(), input, &mut output, Vec::new(), true).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!(
                "Tsuzuri {} REPL; enter :quit to exit\n> > > ",
                env!("CARGO_PKG_VERSION")
            )
        );
        let mut reader = Reader {
            input: Cursor::new("let word = (\n1\n)\n\n:quit\n"),
        };
        let mut prompts = Vec::new();
        assert_eq!(
            reader.read(Some(&mut prompts)).unwrap(),
            Some(Entered::Source("let word = (\n1\n)".into()))
        );
        assert_eq!(String::from_utf8(prompts).unwrap(), ". . . ");
        assert_eq!(
            session("def one :: i32 = 1\n:list\n").0,
            "def one :: i32 = 1\n"
        );
    }

    #[test]
    fn commands_report_their_arguments() {
        let (output, errors) = session(
            ":foo\n:type\n:load notes.txt\n:load missing-g13.tz\n:list extra\n:reset now\ndef one :: i32 = 1\n:list\n:reset\n:list\n:quit\n:list\n",
        );
        assert_eq!(output, "def one :: i32 = 1\n");
        let codes: Vec<_> = errors
            .lines()
            .filter(|line| line.starts_with("input:1:1: error["))
            .map(|line| &line[17..22])
            .collect();
        assert_eq!(
            codes,
            ["E2000", "E2000", "E2000", "E2001", "E2000", "E2000"],
            "{errors}"
        );
        assert!(errors.contains(&format!("unknown command ':foo'; {COMMANDS}")));
        assert!(errors.contains(":load requires one .tz file path"));
        assert!(errors.contains(NO_ARGUMENTS));
    }
}
