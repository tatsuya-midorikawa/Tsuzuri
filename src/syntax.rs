use crate::diagnostic::Span;

pub const MAX_NESTING: usize = 128;
/// The largest source file (`E0003`). A file of this size checks in about 2.3 s and 1 GiB on the
/// G17 Phase 3 measurement machine; 16 MiB took 9.5 s and 3.9 GiB.
pub const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Code,
    TypeClass,
    Computation,
}

impl SourceKind {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "tz" => Some(Self::Code),
            "tt" => Some(Self::TypeClass),
            "tc" => Some(Self::Computation),
            _ => None,
        }
    }
}

/// A source language edition, including its default parameter convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edition(pub(crate) u16);

impl Edition {
    pub const E2026: Self = Self(2026);
    pub const E2027: Self = Self(2027);
    /// What CLI projects without a manifest and the embedded standard library are read with.
    /// Rust's source-only compatibility APIs retain E2026; explicit inputs choose their edition.
    pub const LATEST: Self = Self::E2027;
    /// What a manifest without `edition` means. It never changes, or a compiler update would
    /// give the manifests that already exist new reserved words.
    pub const MANIFEST_DEFAULT: Self = Self::E2026;
    pub const SUPPORTED: &'static [Self] = &[Self::E2026, Self::E2027];
    /// What `tsuzuri new` writes into a new manifest (A22).
    pub const NEW_PROJECT: Self = Self::E2027;

    pub fn borrows_parameters(self) -> bool {
        matches!(self, Self::E2027)
    }

    /// Whether the edition rejects the spellings that A22 replaced with canonical forms (`E0004`).
    /// `>=` on purpose: a spelling that one edition rejects stays rejected in later ones.
    pub fn rejects_noncanonical(self) -> bool {
        self >= Self::E2027
    }

    /// The supported edition that `text` names, such as `2026`.
    pub fn parse(text: &str) -> Option<Self> {
        Self::SUPPORTED
            .iter()
            .copied()
            .find(|edition| edition.to_string() == text)
    }

    /// The supported editions' names, comma separated.
    pub fn supported_names() -> String {
        let names: Vec<_> = Self::SUPPORTED.iter().map(ToString::to_string).collect();
        names.join(", ")
    }
}

impl std::fmt::Display for Edition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    DocComment(String),
    TypeVariable(String),
    Integer(String),
    /// An integer literal with the `I` suffix: the digits, with any radix prefix.
    BigInteger(String),
    Float(String),
    String(StringLiteral),
    /// `"ascii"B`: the bytes of an ASCII string.
    ByteString(Box<[u8]>),
    /// `u8"text"B`: the Unicode scalars of a UTF-8 string.
    ScalarString(Box<[u32]>),
    /// `$"text{` or `u8$"text{`: the text before the first hole.
    InterpolationStart(Box<InterpolationPiece>),
    /// `}text{` or `:spec}text{`: closes a hole and opens the next.
    InterpolationMiddle(Box<InterpolationPiece>),
    /// `}text"` or `:spec}text"`: closes the last hole and the literal.
    InterpolationEnd(Box<InterpolationPiece>),
    Char(u16),
    Utf8Char(u32),
    Fn,
    Def,
    Rec,
    And,
    Export,
    Extern,
    Private,
    Record,
    Union,
    Type,
    Const,
    Test,
    /// `bench "name" = body`, a benchmark that `tsuzuri bench` measures (G18).
    Bench,
    Class,
    Instance,
    Deriving,
    /// `dyn` before the class names of a dynamically dispatched value type (A14).
    Dyn,
    Let,
    Task,
    Do,
    Return,
    Yield,
    For,
    In,
    To,
    Downto,
    While,
    Break,
    Continue,
    Mut,
    Ref,
    Deref,
    New,
    As,
    If,
    Then,
    Elif,
    Else,
    Match,
    With,
    When,
    True,
    False,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftList,
    RightList,
    Colon,
    DoubleColon,
    /// The `::` of a namespace path such as `Sample::Shape.area` (see `lexer::mark_paths`).
    PathSep,
    Semicolon,
    Comma,
    Dot,
    DotDot,
    /// `..=`: a range that includes its end (A22).
    DotDotEqual,
    /// `..<`: a range that excludes its end (A22).
    DotDotLess,
    At,
    Hash,
    Arrow,
    FatArrow,
    Equal,
    Plus,
    Minus,
    Star,
    Slash,
    Backslash,
    Percent,
    Bang,
    Tilde,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Ampersand,
    Pipe,
    Caret,
    /// `<<`: backward function composition.
    DoubleLess,
    /// `>>`: function composition, or two generic closes in a type.
    DoubleGreater,
    /// `<<<`: left shift.
    TripleLess,
    /// `>>>`: right shift (arithmetic for signed types), or three generic closes in a type.
    TripleGreater,
    /// `**`: power.
    DoubleStar,
    /// `&&&`, `|||`, `^^^`, `~~~`: bitwise and, or, xor, and not.
    TripleAmpersand,
    TriplePipe,
    TripleCaret,
    TripleTilde,
    PipeForward,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StringLiteral {
    Utf16(Vec<u16>),
    Utf8(String),
}

/// The text segment carried by an interpolation token and the format spec of
/// the hole that the token closes (`None` for the first token).
#[derive(Clone, Debug, PartialEq)]
pub struct InterpolationPiece {
    pub text: StringLiteral,
    pub spec: Option<FormatSpec>,
}

/// `[[fill]align][+][width][.precision][type]` after the `:` of a hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    pub fill: char,
    pub align: Option<FormatAlign>,
    pub plus: bool,
    pub width: u16,
    pub precision: Option<u16>,
    pub kind: Option<FormatKind>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatAlign {
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKind {
    LowerHex,
    UpperHex,
    Octal,
    Binary,
    Exponent,
    Fixed,
}

/// The most holes one interpolated string may have.
pub const MAX_INTERPOLATION_HOLES: usize = 1024;
/// The largest width and precision of a format spec.
pub const MAX_FORMAT_FIELD: u16 = 4096;

impl StringLiteral {
    pub fn len(&self) -> usize {
        match self {
            Self::Utf16(units) => units.len(),
            Self::Utf8(text) => text.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub text: String,
    pub span: Span,
    pub provenance: Provenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    User,
    Generated,
}

#[derive(Debug)]
pub struct Program {
    pub source_kind: Option<SourceKind>,
    /// `namespace A::B`, the file's first declaration.
    pub namespace: Option<NamespaceDecl>,
    /// `using A::B` declarations, after the namespace and before other declarations.
    pub usings: Vec<NamespaceDecl>,
    pub type_aliases: Vec<TypeAliasDecl>,
    pub constants: Vec<ConstDecl>,
    pub externs: Vec<SignatureDecl>,
    pub extern_types: Vec<ExternTypeDecl>,
    pub records: Vec<RecordDecl>,
    pub unions: Vec<UnionDecl>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
    pub instances: Vec<InstanceDecl>,
    pub active_patterns: Vec<ActivePattern>,
    pub tests: Vec<TestDecl>,
    pub benches: Vec<BenchDecl>,
    pub entry: Option<Expr>,
    /// Every `dyn` type written in this module, in source order (A14). The checker reports the
    /// classes that cannot be dispatched here and generates the dispatching instances.
    pub dyn_types: Vec<TypeExpr>,
    /// The `@cpu [...]` attributes of this module's functions (F08 Phase 3).
    pub cpu_attributes: Vec<CpuAttribute>,
    /// The effect attributes (`@no_alloc` ...) of this module's functions, in source order (A20).
    pub effect_attributes: Vec<EffectAttribute>,
    /// The `@trusted` marks of this module's extern declarations, in source order (E15).
    pub trusted: Vec<TrustedAttribute>,
    /// The spellings that have a canonical form, in parse order (A22).
    pub noncanonical: Vec<Noncanonical>,
    /// The `@alias name` declarations of a `.tc` builder file, in source order: other names for
    /// the builder, so that `name { ... }` builds the same computation as `Builder { ... }`.
    pub aliases: Vec<Ident>,
    /// Where each top-level declaration starts (its first token, a doc comment or an attribute
    /// included), in source order: a signature and its separate definition count apart. The
    /// REPL splits an input there (G13).
    pub declaration_starts: Vec<usize>,
}

/// The CPU targets that `@cpu` names, with their level in `src/runtime/cpu.c` (F08 Phase 3).
pub const CPU_TARGETS: [(&str, u8); 5] = [
    ("sse4.2", 1),
    ("avx2", 2),
    ("avx512", 3),
    ("sve", 4),
    ("sve2", 5),
];

/// Words that are keywords only in their own syntax and identifiers elsewhere. A builder alias
/// (`@alias`) cannot take one of them.
pub const CONTEXTUAL_KEYWORDS: [&str; 8] = [
    "finally",
    "is",
    "namespace",
    "of",
    "pure",
    "try",
    "using",
    "where",
];

/// `@cpu ["avx2", ...]` before a `def` signature: native builds also compile the function for
/// these CPU targets and pick one at run time (F08 Phase 3).
#[derive(Clone, Debug)]
pub struct CpuAttribute {
    pub function: Ident,
    /// Bit `L` is set for each named level `L` of `CPU_TARGETS`.
    pub levels: u8,
}

/// An effect attribute before a `def` signature (A20): a promise that the function's inferred
/// effect summary does not hold an effect. It is only checked; it changes no emitted code.
#[derive(Clone, Debug)]
pub struct EffectAttribute {
    pub function: Ident,
    pub kind: EffectKind,
    /// The `@` and the name.
    pub span: Span,
}

/// The four effect attributes, in the order that `check` reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectKind {
    NoAlloc,
    NoCopy,
    NoTrap,
    Terminates,
}

impl EffectKind {
    pub const ALL: [EffectKind; 4] = [
        EffectKind::NoAlloc,
        EffectKind::NoCopy,
        EffectKind::NoTrap,
        EffectKind::Terminates,
    ];

    /// The attribute's name after `@`.
    pub fn name(self) -> &'static str {
        match self {
            EffectKind::NoAlloc => "no_alloc",
            EffectKind::NoCopy => "no_copy",
            EffectKind::NoTrap => "no_trap",
            EffectKind::Terminates => "terminates",
        }
    }

    pub fn from_name(name: &str) -> Option<EffectKind> {
        EffectKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The attribute's bit in a set of attributes.
    pub fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The properties that `@trusted` may state about an extern function, bit `i` for entry `i` (E15).
/// They are the words of A20's effect attributes and A19's `pure`.
pub const TRUSTED_PROPERTIES: [&str; 5] = ["pure", "no_alloc", "no_copy", "no_trap", "terminates"];

/// The bit of a `@trusted` property in `TrustedAttribute::properties`.
pub fn trusted_property_bit(name: &str) -> Option<u8> {
    TRUSTED_PROPERTIES
        .iter()
        .position(|property| *property == name)
        .map(|index| 1 << index)
}

/// `@trusted [properties]` before an extern declaration (E15): a person checked the external
/// function's ABI, ownership and lifetime contract. The compiler checks none of it; the mark
/// changes no emitted code.
#[derive(Clone, Debug)]
pub struct TrustedAttribute {
    /// The name of the extern function or type.
    pub target: Ident,
    /// Bit `i` for `TRUSTED_PROPERTIES[i]`.
    pub properties: u8,
    /// From `@trusted` through the last property.
    pub span: Span,
}

/// A `namespace A::B` or `using A::B` declaration.
#[derive(Clone, Debug)]
pub struct NamespaceDecl {
    /// The namespace path as one identifier, its segments joined by `::`.
    pub path: Ident,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TestDecl {
    pub name: String,
    pub name_span: Span,
    pub body: Expr,
    pub span: Span,
}

/// `bench "name" = body`: the body has type `i64 -> i64`, from an iteration count to the
/// nanoseconds that many iterations took (G18).
#[derive(Clone, Debug)]
pub struct BenchDecl {
    pub name: String,
    pub name_span: Span,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ActivePattern {
    pub cases: Vec<Ident>,
    pub function: String,
    pub partial: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Debug)]
pub struct Documentation {
    pub text: String,
    pub span: Span,
}

impl Documentation {
    /// The note of the `@deprecated` tag (G19): the rest of the first line that starts with the
    /// tag, which may be empty. `@deprecatedX` is not the tag.
    pub fn deprecation(&self) -> Option<&str> {
        if !self.text.contains("@deprecated") {
            return None;
        }
        self.text.lines().find_map(|line| {
            let rest = line.trim_start().strip_prefix("@deprecated")?;
            (rest.is_empty() || rest.starts_with([' ', '\t'])).then(|| rest.trim())
        })
    }
}

#[derive(Clone, Debug)]
pub struct TypeAliasDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub target: TypeExpr,
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: TypeExpr,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub regions: Vec<Ident>,
    pub fields: Vec<Parameter>,
    pub derives: Vec<(DeriveClass, Span)>,
}

#[derive(Clone, Debug)]
pub struct UnionDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub cases: Vec<UnionCaseDecl>,
    pub derives: Vec<(DeriveClass, Span)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeriveClass {
    Eq,
    Ord,
    Display,
    Hash,
    Default,
    Encode,
    Decode,
    /// A21: a generator of values for the property tests of contracts.
    Arbitrary,
}

impl DeriveClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "Eq",
            Self::Ord => "Ord",
            Self::Display => "Display",
            Self::Hash => "Hash",
            Self::Default => "Default",
            Self::Encode => "Encode",
            Self::Decode => "Decode",
            Self::Arbitrary => "Arbitrary",
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnionCaseDecl {
    pub name: Ident,
    pub payload: Option<TypeExpr>,
    /// `@json "name"` on the case: its tag in derived `Encode`/`Decode` (D08).
    pub json: Option<JsonName>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub doc: Option<Documentation>,
    pub name: Ident,
    pub regions: Vec<Ident>,
    pub recursion: Option<String>,
    pub visibility: Visibility,
    pub exported: bool,
    /// Declared `const def`: a const initializer may call it, and the compiler evaluates the call
    /// (D11). It is an ordinary function at run time.
    pub constant: bool,
    pub pure: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
    pub body: Expr,
    /// The `@requires` and `@ensures` lines that start the body (A21), in source order.
    pub contracts: Vec<ContractClause>,
}

/// Whether a contract line is checked on entry or on exit (A21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractKind {
    Requires,
    Ensures,
}

impl ContractKind {
    /// The attribute's name after `@`.
    pub fn name(self) -> &'static str {
        match self {
            ContractKind::Requires => "requires",
            ContractKind::Ensures => "ensures",
        }
    }
}

/// One `@requires condition` or `@ensures \result -> condition` line of a function body (A21).
#[derive(Clone, Debug)]
pub struct ContractClause {
    pub kind: ContractKind,
    /// `@ensures \r -> ..`: the name of the result (`_` for `\_`). `None` for `@requires`.
    pub binder: Option<Ident>,
    pub condition: Expr,
    /// The condition's source text with line breaks and the indentation after them collapsed to
    /// one space; docgen, hover, api-diff and facts print it. The AST fingerprint clears it.
    pub text: String,
    /// From the `@` to the end of the condition.
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SignatureDecl {
    pub doc: Option<Documentation>,
    pub name: Ident,
    pub regions: Vec<Ident>,
    pub recursion: Option<String>,
    pub visibility: Visibility,
    pub exported: bool,
    /// Written `const def`, or an `and` signature that continues a `const def rec` group (D11).
    pub constant: bool,
    pub pure: bool,
    pub parameters: Vec<TypeExpr>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
    pub link: Option<Box<LinkName>>,
}

/// The strings of `extern "symbol" def` or `extern "module" "symbol" def`.
#[derive(Clone, Debug)]
pub struct LinkName {
    /// The WASM import module and the span of its string.
    pub module: Option<(String, Span)>,
    pub symbol: String,
    /// The span of the string that holds the symbol.
    pub span: Span,
}

/// `extern type Name`: an opaque host handle.
#[derive(Clone, Debug)]
pub struct ExternTypeDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: Ident,
    pub pure: bool,
    pub recursion: Option<String>,
    pub parameters: Vec<(Ident, bool)>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct ConstraintExpr {
    pub name: ConstraintName,
    pub ty: TypeExpr,
}

#[derive(Clone, Debug)]
pub enum ConstraintName {
    Class(Ident),
    /// `#name`, or `(#name: Type)` with the function's type.
    Function(Ident, Option<Box<TypeExpr>>),
}

#[derive(Debug)]
pub struct ClassDecl {
    pub doc: Option<Documentation>,
    pub name: Ident,
    pub variable: Ident,
    pub kind: Kind,
    pub superclasses: Vec<ConstraintExpr>,
    pub methods: Vec<SignatureDecl>,
    pub defaults: Vec<Definition>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    #[default]
    Type,
    Arrow(Box<Kind>, Box<Kind>),
}

impl Kind {
    pub fn display(&self) -> String {
        match self {
            Self::Type => "*".into(),
            Self::Arrow(argument, result) => {
                let left = if matches!(**argument, Self::Arrow(..)) {
                    format!("({})", argument.display())
                } else {
                    argument.display()
                };
                format!("{left} -> {}", result.display())
            }
        }
    }
}

#[derive(Debug)]
pub struct InstanceDecl {
    pub class: Ident,
    pub ty: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
    pub methods: Vec<Definition>,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: Ident,
    pub ty: TypeExpr,
    pub mutable: bool,
    /// `@json "name"` on a record field: the key of derived `Encode`/`Decode` (D08).
    pub json: Option<JsonName>,
}

/// The JSON name that `@json "name"` gives a record field or a union case, as UTF-16 code units.
#[derive(Clone, Debug)]
pub struct JsonName {
    pub units: Vec<u16>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TypeExprKind {
    Named(String),
    Variable(String),
    /// Type application such as `Pair<i64, string>` or `Add<'a>`; the
    /// checker decides whether the head names a record or a type class.
    /// Both parts are boxed so every `TypeExpr` and expression embedding one
    /// stays as small as before, which bounds the parser's recursion stack.
    Apply(Box<Ident>, Box<[TypeExpr]>),
    Regions(Box<TypeExpr>, Box<[Ident]>),
    /// `{r s} A -> B`: a function type with regions of its own, which only a named function's
    /// parameter can have (A12 Phase 2).
    Quantified(Box<[Ident]>, Box<TypeExpr>),
    Array(Box<TypeExpr>),
    /// `[T..]`: the target of an exclusive slice, valid only directly after `ref mut` (C08).
    ArrayView(Box<TypeExpr>),
    /// `[T; N]`: a fixed-length array value (A16). The length is a `Length` or the `Named` length
    /// parameter or integer constant (Phase 2).
    FixedArray(Box<TypeExpr>, Box<TypeExpr>),
    /// A literal length: of a fixed-length array, or a length argument such as the `3` of
    /// `Grid<f64, 3>` (A16). Lengths above `u64::MAX` saturate so the checker reports E1010.
    Length(u64),
    List(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Task(Box<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Reference(Box<TypeExpr>, bool),
    Own(Box<TypeExpr>),
    Mut(Box<TypeExpr>),
    Pure(Box<TypeExpr>),
    /// `dyn Shapes.Shape` or `dyn (Shape, Send)`: an owned value of some type with instances of
    /// the classes, dispatched at run time (A14). Boxed like `Apply` so `TypeExpr` does not grow.
    Dyn(Box<DynTypeExpr>),
}

/// The classes of a `dyn` type as written, and whether a region follows it (A14).
#[derive(Clone, Debug)]
pub struct DynTypeExpr {
    pub classes: Vec<Ident>,
    /// `dyn C {r}`: the stored value may hold shared borrows of the region (Phase 2).
    pub borrowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    BitNot,
    /// Unary `+`: a numeric value unchanged.
    Plus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
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
    /// `**`: power.
    Power,
    Pipe,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub depth: usize,
}

impl Expr {
    pub(crate) fn visit(&self, visitor: &mut impl FnMut(&Expr)) {
        visitor(self);
        use ExprKind::*;
        match &self.kind {
            Unary(_, value)
            | Lambda(_, value)
            | Task(value)
            | TaskRun(value)
            | ComputationBoundary(value)
            | Checked(value)
            | NewLiteral(value)
            | Field(value, _)
            | Borrow(value, ..)
            | Dereference(value, _)
            | Cast(value, _) => value.visit(visitor),
            Binary(_, left, right)
            | Index(left, right)
            | Assign(left, right)
            | NewArray(_, left, right)
            | NewList(_, left, right) => {
                left.visit(visitor);
                right.visit(visitor);
            }
            Call(callee, arguments) => {
                callee.visit(visitor);
                for argument in arguments {
                    argument.visit(visitor);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.visit(visitor);
                then_branch.visit(visitor);
                else_branch.visit(visitor);
            }
            While { condition, body } => {
                condition.visit(visitor);
                body.visit(visitor);
            }
            For {
                pattern,
                source,
                body,
            } => {
                pattern.visit_expressions(visitor);
                source.visit(visitor);
                body.visit(visitor);
            }
            Range {
                start,
                step,
                finish,
                ..
            } => {
                start.visit(visitor);
                if let Some(step) = step {
                    step.visit(visitor);
                }
                finish.visit(visitor);
            }
            Match { value, arms, .. } => {
                value.visit(visitor);
                for arm in arms {
                    arm.pattern.visit_expressions(visitor);
                    if let Some(guard) = &arm.guard {
                        guard.visit(visitor);
                    }
                    arm.body.visit(visitor);
                }
            }
            Try(handled) => {
                handled.body.visit(visitor);
                for arm in &handled.arms {
                    arm.pattern.visit_expressions(visitor);
                    if let Some(guard) = &arm.guard {
                        guard.visit(visitor);
                    }
                    arm.body.visit(visitor);
                }
                if let Some(finally) = &handled.finally {
                    finally.visit(visitor);
                }
            }
            Block { bindings, result } => {
                for binding in bindings {
                    binding.value.visit(visitor);
                }
                result.visit(visitor);
            }
            Record { fields, .. } => {
                for (_, value) in fields {
                    value.visit(visitor);
                }
            }
            RecordUpdate { base, fields } => {
                base.visit(visitor);
                for (_, value) in fields {
                    value.visit(visitor);
                }
            }
            Array(values) | List(values) | Tuple(values) => {
                for value in values {
                    value.visit(visitor);
                }
            }
            Slice {
                value, start, end, ..
            } => {
                value.visit(visitor);
                for bound in start.iter().chain(end) {
                    bound.visit(visitor);
                }
            }
            Computation(_, body) => body.visit_expressions(visitor),
            Interpolated(interpolation) => {
                for hole in &interpolation.holes {
                    hole.value.visit(visitor);
                }
            }
            Integer(..)
            | BigInt(_)
            | Float(..)
            | String(_)
            | Char(_)
            | Utf8Char(_)
            | Bool(_)
            | Unit
            | Break
            | Continue
            | Name(_)
            | QualifiedFunction(_)
            | TypeFunction(..)
            | DynDispatch { .. } => {}
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Integer(u128, Option<String>),
    /// `123I`: a bigint literal's digits, with any `0x`/`0b` prefix.
    BigInt(Box<str>),
    Float(String, Option<String>),
    String(StringLiteral),
    Char(u16),
    Utf8Char(u32),
    Bool(bool),
    Unit,
    Break,
    Continue,
    Name(Ident),
    QualifiedFunction(Ident),
    TypeFunction(Box<Ident>, Box<Ident>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Lambda(Vec<(Ident, bool)>, Box<Expr>),
    Task(Box<Expr>),
    TaskRun(Box<Expr>),
    Computation(Ident, Box<ComputationBlock>),
    ComputationBoundary(Box<Expr>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    While {
        condition: Box<Expr>,
        body: Box<Expr>,
    },
    For {
        pattern: Box<Pattern>,
        source: Box<Expr>,
        body: Box<Expr>,
    },
    Range {
        start: Box<Expr>,
        step: Option<Box<Expr>>,
        finish: Box<Expr>,
        counted: bool,
        descending: bool,
        /// The operator before `finish`; `for i = a to b` is `Closed` (A22).
        op: RangeOp,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
        origin: MatchOrigin,
    },
    /// `try body with | pattern -> handler ... [finally cleanup]`.
    Try(Box<TryExpr>),
    /// `@checked expression`: integer `+ - * **` and negation in it raise OverflowException.
    Checked(Box<Expr>),
    Block {
        bindings: Vec<Binding>,
        result: Box<Expr>,
    },
    Record {
        name: Box<Ident>,
        fields: Vec<(Ident, Expr)>,
    },
    RecordUpdate {
        base: Box<Expr>,
        fields: Vec<(Ident, Expr)>,
    },
    Array(Vec<Expr>),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    NewArray(Box<TypeExpr>, Box<Expr>, Box<Expr>),
    NewList(Box<TypeExpr>, Box<Expr>, Box<Expr>),
    /// `new [a, b]` / `new [|a, b|]`: an array or list literal allocated on the heap.
    NewLiteral(Box<Expr>),
    Field(Box<Expr>, Ident),
    Index(Box<Expr>, Box<Expr>),
    /// `ref xs[a..b]`, or with `mutable` the exclusive `ref mut xs[a..b]` (C08).
    Slice {
        value: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        mutable: bool,
        /// `..` and `..<` exclude `end`; `..=` includes it (A22).
        op: RangeOp,
    },
    Borrow(Box<Expr>, bool, Notation),
    Dereference(Box<Expr>, Notation),
    Assign(Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, TypeExpr),
    /// `$"a{x}b"`: `texts` has one more element than `holes`.
    Interpolated(Box<Interpolation>),
    /// The generated body of a method of an instance for a `dyn` type (A14): calls vtable slot
    /// `slot` of a vtable with `slots` slots. The parser never produces it.
    DynDispatch {
        slot: u32,
        slots: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Interpolation {
    pub texts: Vec<StringLiteral>,
    pub holes: Vec<InterpolationHole>,
}

/// A `try` expression: its value is `Ok body`, or `Error handler` when an
/// exception raised in `body` matches an arm; `finally` runs on every exit.
#[derive(Clone, Debug)]
pub struct TryExpr {
    pub body: Expr,
    pub arms: Vec<MatchArm>,
    pub finally: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct InterpolationHole {
    pub value: Expr,
    pub spec: Option<FormatSpec>,
}

/// A spelling that has a canonical form (A22): `W1007` reports it, and edition 2027 rejects it
/// (`E0004`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoncanonicalKind {
    /// The last `..` of a `for` range: `..=`.
    BareRange,
    /// A short numeric suffix such as `86uy`: the type name.
    ShortSuffix,
    /// `&x` and `&mut x` in an expression: `ref x`, `ref mut x`.
    SymbolicBorrow,
    /// `*r` and `**r`: `deref r`, `deref deref r`.
    SymbolicDeref,
    /// `&T`, `&mut T`, `&&T` in a type: `ref T`, `ref mut T`, `ref ref T`.
    SymbolicReferenceType,
    /// `x -> e` and `x -> y -> e`: `\x -> e`, `\x y -> e`.
    LegacyLambda,
}

/// How sure a rewrite of a noncanonical spelling is to keep the program's meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixLevel {
    Certain,
    Likely,
    /// No rewrite: the canonical spelling would mean something else.
    Unavailable,
}

/// A replacement of the text at `span`; an empty span inserts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub span: Span,
    pub text: String,
}

/// One noncanonical spelling and its rewrite (A22).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Noncanonical {
    pub kind: NoncanonicalKind,
    /// The span that the diagnostic points at.
    pub span: Span,
    /// The spelling as written, for the message: `..`, `86uy`, `&mut`, `*`, `x -> y ->`.
    pub spelling: String,
    /// Its canonical form: `..=`, `86i8u`, `ref mut`, `deref`, `\x y ->`.
    pub canonical: String,
    pub level: FixLevel,
    /// Empty when `level` is `Unavailable`.
    pub edits: Vec<TextEdit>,
}

/// The operator of a range (A22): `..`, which a `for` reads as closed and a slice as half-open,
/// `..=` (closed) and `..<` (half-open).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangeOp {
    Bare,
    Closed,
    Open,
}

/// Spelling of a borrow or dereference. Both spellings produce the same typed tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notation {
    /// `&x`, `&mut x`, `*r`: Rust-compatible; `&r` borrows the reference itself.
    Symbol,
    /// `ref x`, `ref mut x`, `deref r`: `ref` reborrows when `x` is statically a reference.
    Keyword,
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Binding(Ident),
    Apply(Ident, Vec<Pattern>),
    Argument(Box<Expr>),
    Literal(Box<Expr>),
    Tuple(Vec<Pattern>),
    Record(Option<Ident>, Vec<(Ident, Pattern)>),
    Array(Vec<Pattern>),
    List(Vec<Pattern>),
    Cons(Box<Pattern>, Box<Pattern>),
    Or(Box<Pattern>, Box<Pattern>),
    And(Box<Pattern>, Box<Pattern>),
    As(Box<Pattern>, Ident),
    Annotated(Box<Pattern>, TypeExpr),
}

impl Pattern {
    fn visit_expressions(&self, visitor: &mut impl FnMut(&Expr)) {
        use PatternKind::*;
        match &self.kind {
            Literal(value) | Argument(value) => value.visit(visitor),
            Apply(_, patterns) | Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    pattern.visit_expressions(visitor);
                }
            }
            Record(_, fields) => {
                for (_, pattern) in fields {
                    pattern.visit_expressions(visitor);
                }
            }
            Cons(left, right) | Or(left, right) | And(left, right) => {
                left.visit_expressions(visitor);
                right.visit_expressions(visitor);
            }
            As(pattern, _) | Annotated(pattern, _) => pattern.visit_expressions(visitor),
            Wildcard | Binding(_) => {}
        }
    }
}

/// Where a `match` comes from. Explicit matches and function guards must be
/// exhaustive; destructuring keeps its runtime trap when a value does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchOrigin {
    /// `match value with ...`.
    Explicit,
    /// The clauses of `fn f x | pattern -> ...`.
    FunctionGuard,
    /// A destructuring lambda parameter.
    LambdaDestructuring,
    /// A destructuring `for` inside a computation expression.
    ComputationDestructuring,
}

impl MatchOrigin {
    /// Whether the checker rejects non-exhaustive arms at compile time.
    pub fn checks_coverage(self) -> bool {
        matches!(self, Self::Explicit | Self::FunctionGuard)
    }
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// How a binding takes part in a recursive group (B09).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recursion {
    /// An ordinary binding.
    #[default]
    None,
    /// `let rec name = \x -> ...`: the function's body can call `name`, and so can the bodies of
    /// the `and` members that follow it.
    Rec,
    /// `and name = \x -> ...`: a member of the group that the binding before it starts.
    And,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub name: Ident,
    pub mutable: bool,
    /// A `use` binding: a `let` whose value type must implement `Drop` (B07).
    pub using: bool,
    pub recursion: Recursion,
    pub annotation: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct ComputationBlock {
    pub statements: Vec<ComputationStatement>,
    pub span: Span,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct ComputationStatement {
    pub kind: ComputationStatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ComputationStatementKind {
    Let(Binding, bool),
    LetAnd(Vec<Binding>),
    Match(Box<Expr>, Vec<ComputationMatchArm>),
    Do(Expr),
    Operation(&'static str, Expr),
    If(Expr, ComputationBlock, Option<ComputationBlock>),
    For(Box<Pattern>, Expr, ComputationBlock),
    While(Expr, ComputationBlock),
    Expression(Expr),
}

#[derive(Clone, Debug)]
pub struct ComputationMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: ComputationBlock,
    pub span: Span,
}

impl ComputationBlock {
    fn visit_expressions(&self, visitor: &mut impl FnMut(&Expr)) {
        use ComputationStatementKind::*;
        for statement in &self.statements {
            match &statement.kind {
                Let(binding, _) => binding.value.visit(visitor),
                LetAnd(bindings) => {
                    for binding in bindings {
                        binding.value.visit(visitor);
                    }
                }
                Do(value) | Operation(_, value) | Expression(value) => value.visit(visitor),
                Match(value, arms) => {
                    value.visit(visitor);
                    for arm in arms {
                        arm.pattern.visit_expressions(visitor);
                        if let Some(guard) = &arm.guard {
                            guard.visit(visitor);
                        }
                        arm.body.visit_expressions(visitor);
                    }
                }
                If(condition, yes, no) => {
                    condition.visit(visitor);
                    yes.visit_expressions(visitor);
                    if let Some(no) = no {
                        no.visit_expressions(visitor);
                    }
                }
                For(pattern, source, body) => {
                    pattern.visit_expressions(visitor);
                    source.visit(visitor);
                    body.visit_expressions(visitor);
                }
                While(condition, body) => {
                    condition.visit(visitor);
                    body.visit_expressions(visitor);
                }
            }
        }
    }
}

impl ComputationStatement {
    pub fn depth(&self) -> usize {
        use ComputationStatementKind::*;
        match &self.kind {
            Let(binding, _) => binding.value.depth,
            LetAnd(bindings) => bindings
                .iter()
                .map(|binding| binding.value.depth)
                .max()
                .unwrap_or(0),
            Match(value, arms) => arms
                .iter()
                .map(|arm| {
                    arm.pattern
                        .depth
                        .max(arm.body.depth)
                        .max(arm.guard.as_ref().map_or(0, |guard| guard.depth))
                })
                .max()
                .unwrap_or(0)
                .max(value.depth),
            Do(value) | Operation(_, value) | Expression(value) => value.depth,
            If(condition, yes, no) => condition
                .depth
                .max(yes.depth)
                .max(no.as_ref().map_or(0, |branch| branch.depth)),
            For(pattern, source, body) => pattern.depth.max(source.depth).max(body.depth),
            While(source, body) => source.depth.max(body.depth),
        }
    }
}
