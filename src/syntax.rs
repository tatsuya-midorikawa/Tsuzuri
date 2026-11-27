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
    pub const E2028: Self = Self(2028);
    /// What CLI projects without a manifest and the embedded standard library are read with.
    /// Rust's source-only compatibility APIs retain E2026; explicit inputs choose their edition.
    pub const LATEST: Self = Self::E2028;
    /// What a manifest without `edition` means. It never changes, or a compiler update would
    /// give the manifests that already exist new reserved words.
    pub const MANIFEST_DEFAULT: Self = Self::E2026;
    pub const SUPPORTED: &'static [Self] = &[Self::E2026, Self::E2027, Self::E2028];
    /// What `tsuzuri new` writes into a new manifest (NC17 Phase 3).
    pub const NEW_PROJECT: Self = Self::E2028;

    pub fn borrows_parameters(self) -> bool {
        self >= Self::E2027
    }

    /// Edition 2028 unified range family (D-66 / NC17): inclusive bare `..` in slices, `<..`,
    /// `<..<`, `^k` from-end positions, and stepped bracket ranges.
    pub fn range_family(self) -> bool {
        self >= Self::E2028
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
    /// `<..`: a range that excludes its start and includes its end (NC17).
    LessDotDot,
    /// `<..<`: a range that excludes both endpoints (NC17).
    LessDotDotLess,
    At,
    Hash,
    /// `?`: a typed hole (G25).
    Question,
    Arrow,
    FatArrow,
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
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
    /// `extern class` declarations: Objective-C classes as types (M01).
    pub extern_classes: Vec<ExternClassDecl>,
    /// `extern category` declarations: more members of an Objective-C class (M02).
    pub extern_categories: Vec<ExternCategoryDecl>,
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
    /// Whether this module writes an anonymous union type `(A | B)` or a type test pattern `:? T`
    /// anywhere, bodies included (A24). The checker synthesizes the generic declarations behind
    /// anonymous unions only when some module of the program sets it.
    pub uses_anonymous_unions: bool,
    /// The `@cpu [...]` attributes of this module's functions (F08 Phase 3).
    pub cpu_attributes: Vec<CpuAttribute>,
    /// The effect attributes (`@no_alloc` ...) of this module's functions, in source order (A20).
    pub effect_attributes: Vec<EffectAttribute>,
    /// The `@trusted` marks of this module's extern declarations, in source order (E15).
    pub trusted: Vec<TrustedAttribute>,
    /// The `@count(pos, len)` attributes of this module's extern function declarations (W13).
    pub extern_counts: Vec<CountAttribute>,
    /// The `@c_layout` attributes of this module's record declarations (W21).
    pub c_layouts: Vec<CLayoutAttribute>,
    /// The `@link("name")` / `@link(framework = "name")` attributes of this module's extern declarations (W17 / M20).
    pub links: Vec<LinkAttribute>,
    /// The `@weak` attributes of this module's extern declarations (M17).
    pub weaks: Vec<WeakAttribute>,
    /// The `@last_error` attributes of this module's extern function declarations (W16).
    pub last_error: Vec<LastErrorAttribute>,
    /// The `@returns_retained` / `@returns_not_retained` attributes of this module's extern
    /// function declarations (M01). A member of an extern class carries its own.
    pub returns_retained: Vec<OwnershipAttribute>,
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
pub const CONTEXTUAL_KEYWORDS: [&str; 10] = [
    "finally",
    "is",
    "namespace",
    "of",
    "pure",
    "select",
    "try",
    "tzroutine",
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

/// The C length parameter kind chosen by `@count(position, length)` on an `extern` buffer (W13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CountLength {
    I32,
    I32U,
    I64,
    I64U,
    None,
}

impl CountLength {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "i32" => Some(Self::I32),
            "i32u" => Some(Self::I32U),
            "i64" => Some(Self::I64),
            "i64u" => Some(Self::I64U),
            "none" => Some(Self::None),
            _ => Option::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I32U => "i32u",
            Self::I64 => "i64",
            Self::I64U => "i64u",
            Self::None => "none",
        }
    }

    pub fn c_type(self) -> Option<&'static str> {
        match self {
            Self::I32 => Some("int32_t"),
            Self::I32U => Some("uint32_t"),
            Self::I64 => Some("int64_t"),
            Self::I64U => Some("uint64_t"),
            Self::None => Option::None,
        }
    }

    pub fn llvm_type(self) -> Option<&'static str> {
        match self {
            Self::I32 | Self::I32U => Some("i32"),
            Self::I64 | Self::I64U => Some("i64"),
            Self::None => Option::None,
        }
    }

    pub fn max_length(self) -> Option<u64> {
        match self {
            Self::I32 => Some(i32::MAX as u64),
            Self::I32U => Some(u32::MAX as u64),
            Self::I64 | Self::I64U | Self::None => Option::None,
        }
    }
}

/// `@count(position, length)` before an extern function declaration (W13).
#[derive(Clone, Debug)]
pub struct CountAttribute {
    pub target: Ident,
    pub position: u32,
    pub length: CountLength,
    pub span: Span,
}

/// C struct layout parameters of `@c_layout` / `@c_layout(pack = N)` (W21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CLayout {
    pub pack: Option<u8>,
}

/// `@c_layout` / `@c_layout(pack = N)` before a record declaration (W21).
#[derive(Clone, Debug)]
pub struct CLayoutAttribute {
    pub target: Ident,
    pub pack: Option<u8>,
    pub span: Span,
}

/// Kind of `@link` input: a system library (`@link("name")`) or a macOS framework (`@link(framework = "name")`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkKind {
    Library,
    Framework,
}

/// `@link("name")` or `@link(framework = "name")` before an `extern` declaration with a link name (W17 / M20).
#[derive(Clone, Debug)]
pub struct LinkAttribute {
    pub target: Ident,
    pub kind: LinkKind,
    pub library: String,
    pub span: Span,
    pub name_span: Span,
}

/// `@weak` before an `extern` declaration with a link name (M17).
#[derive(Clone, Debug)]
pub struct WeakAttribute {
    pub target: Ident,
    pub span: Span,
}

/// `@last_error` before an `extern ... def` declaration (W16).
#[derive(Clone, Debug)]
pub struct LastErrorAttribute {
    pub target: Ident,
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
        self.tag_notes("@deprecated").next()
    }

    /// The value of the first `@complexity` line (G35), which may be empty.
    pub fn complexity(&self) -> Option<&str> {
        self.tag_notes("@complexity").next()
    }

    /// The value of the first `@since` line (G35), which may be empty.
    pub fn since(&self) -> Option<&str> {
        self.tag_notes("@since").next()
    }

    /// The value of every `@see` line (G35), in order.
    pub fn see(&self) -> Vec<&str> {
        self.tag_notes("@see").collect()
    }

    /// The rest of every line whose first word is exactly `tag` (leading blanks ignored; the tag
    /// ends the line or is followed by a space or a tab, so `@deprecatedX` is not `@deprecated`).
    fn tag_notes<'a>(&'a self, tag: &'static str) -> impl Iterator<Item = &'a str> + 'a {
        let text = if self.text.contains(tag) {
            self.text.as_str()
        } else {
            ""
        };
        text.lines().filter_map(move |line| {
            let rest = line.trim_start().strip_prefix(tag)?;
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

/// Direction of an `extern view def` memory copy (W24).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ViewDirection {
    Read,
    Write,
}

impl ViewDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// How an `extern "sym" view def` or `extern "sym" address def` accesses a C data symbol (M06).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataAccess {
    Read,
    Address,
}

impl DataAccess {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Address => "address",
        }
    }
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
    /// Declared as `extern view def` (W24) or `extern "sym" view def` (M06).
    pub view: bool,
    /// Declared as a C data symbol (`extern "sym" view def` or `extern "sym" address def`, M06).
    pub data: Option<DataAccess>,
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

impl LinkName {
    pub(crate) fn quote_str(text: &str) -> String {
        let mut out = String::with_capacity(text.len() + 2);
        out.push('"');
        for ch in text.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\0' => out.push_str("\\0"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// Renders `"symbol"` or `"module" "symbol"` in Tsuzuri string-literal syntax.
    pub fn quoted(&self) -> String {
        if let Some((module, _)) = &self.module {
            format!(
                "{} {}",
                Self::quote_str(module),
                Self::quote_str(&self.symbol)
            )
        } else {
            Self::quote_str(&self.symbol)
        }
    }
}

impl SignatureDecl {
    /// Header prefix for `docgen` and `api-diff`, preserving explicit link symbols on data declarations.
    pub fn extern_header_prefix(&self) -> std::borrow::Cow<'static, str> {
        match (self.data, self.link.as_deref(), self.view) {
            (Some(DataAccess::Address), Some(link), _) => {
                std::borrow::Cow::Owned(format!("extern {} address def", link.quoted()))
            }
            (Some(DataAccess::Address), None, _) => {
                std::borrow::Cow::Borrowed("extern address def")
            }
            (Some(DataAccess::Read), Some(link), _) => {
                std::borrow::Cow::Owned(format!("extern {} view def", link.quoted()))
            }
            (Some(DataAccess::Read), None, _) | (None, _, true) => {
                std::borrow::Cow::Borrowed("extern view def")
            }
            (None, _, false) => std::borrow::Cow::Borrowed("extern def"),
        }
    }
}

/// What an `extern type` or `extern class` declaration says about its values (E12 D3, W18, W19,
/// M01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandleKind {
    /// `extern type H`: moved, not capturable, Send, not Sync (E12 D3).
    Owned,
    /// `extern type copy H`: an identifier that the host does not hand over (W18).
    Copy,
    /// `extern type local H`: moved, bound to its OS thread, neither Send nor Sync (W19).
    Local,
    /// `extern class C` and `ObjC.Object`: one strong reference to an Objective-C object, released
    /// on drop and bound to its OS thread like `Local` (M01).
    ObjcLocal,
    /// `extern class sync C`: one strong reference to an Objective-C object that any thread may
    /// use, so the value is Send and Sync (M01).
    ObjcSync,
}

impl HandleKind {
    /// The modifier after `extern type`; an Objective-C class is not written as an `extern type`.
    pub fn keyword(self) -> Option<&'static str> {
        match self {
            Self::Owned | Self::ObjcLocal | Self::ObjcSync => None,
            Self::Copy => Some("copy"),
            Self::Local => Some("local"),
        }
    }

    /// A value that the host does not hand over: copied freely and never dropped (W18).
    pub fn copy(self) -> bool {
        match self {
            Self::Copy => true,
            Self::Owned | Self::Local | Self::ObjcLocal | Self::ObjcSync => false,
        }
    }

    /// A moved value: every kind but `copy`.
    pub fn owned(self) -> bool {
        !self.copy()
    }

    /// Bound to the OS thread that created it: neither Send nor Sync, and the code that touches
    /// it is `pinned` (W19, M01).
    pub fn thread_bound(self) -> bool {
        match self {
            Self::Local | Self::ObjcLocal => true,
            Self::Owned | Self::Copy | Self::ObjcSync => false,
        }
    }

    /// A strong reference to an Objective-C object, which a drop releases (M01).
    pub fn objc(self) -> bool {
        match self {
            Self::ObjcLocal | Self::ObjcSync => true,
            Self::Owned | Self::Copy | Self::Local => false,
        }
    }

    /// Whether several threads may share a borrow of the value.
    pub fn syncable(self) -> bool {
        match self {
            Self::Copy | Self::ObjcSync => true,
            Self::Owned | Self::Local | Self::ObjcLocal => false,
        }
    }
}

/// The built-in root of every `extern class` chain: Objective-C's `id` (M01).
pub const OBJC_OBJECT: &str = "ObjC.Object";
/// Objective-C's `Class`, a copy handle (M01).
pub const OBJC_CLASS: &str = "ObjC.Class";
/// Objective-C's `SEL`, a copy handle (M01).
pub const OBJC_SEL: &str = "ObjC.Sel";

/// `extern ["RuntimeName"] class [sync] Name : Base [{ members }]`: an Objective-C class as a
/// type (M01), with the methods that the program sends to it (M02).
#[derive(Clone, Debug)]
pub struct ExternClassDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    /// The class's name in the Objective-C runtime, when it differs from `name`.
    pub runtime_name: Option<(String, Span)>,
    /// The span of `sync`: the values are Send and Sync.
    pub sync: Option<Span>,
    pub name: Ident,
    pub base: TypeExpr,
    /// Whether a `{ ... }` body follows, which may be empty.
    pub body: bool,
    pub members: Vec<ObjcMember>,
    /// The span of `@trusted` before the declaration (E15).
    pub trusted: Option<Span>,
    /// The span of `@weak` before the declaration: the class may be missing at run time (M17).
    pub weak: Option<Span>,
}

impl ExternClassDecl {
    /// The name that the Objective-C runtime knows the class by.
    pub fn runtime_name(&self) -> &str {
        self.runtime_name
            .as_ref()
            .map_or(self.name.text.as_str(), |(name, _)| name.as_str())
    }

    pub fn header(&self) -> String {
        self.header_with(&self.name.text)
    }

    /// The declaration's first line with `name` in place of the class name.
    pub fn header_with(&self, name: &str) -> String {
        let mut header = String::from("extern ");
        if let Some((runtime, _)) = &self.runtime_name {
            header.push_str(&LinkName::quote_str(runtime));
            header.push(' ');
        }
        header.push_str("class ");
        if self.sync.is_some() {
            header.push_str("sync ");
        }
        header.push_str(name);
        header.push_str(" : ");
        header.push_str(&crate::docgen::type_text(&self.base));
        header
    }
}

/// `extern category Name : Class { members }`: a namespace of more members of an Objective-C
/// class (M02).
#[derive(Clone, Debug)]
pub struct ExternCategoryDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub name: Ident,
    pub class: TypeExpr,
    pub members: Vec<ObjcMember>,
    /// The span of `@trusted` before the declaration (E15).
    pub trusted: Option<Span>,
}

impl ExternCategoryDecl {
    pub fn header(&self) -> String {
        format!(
            "extern category {} : {}",
            self.name.text,
            crate::docgen::type_text(&self.class)
        )
    }
}

/// A method of an `extern class` or `extern category`: `[class] ["selector:"] name :: Type` (M02).
#[derive(Clone, Debug)]
pub struct ObjcMember {
    pub doc: Option<Documentation>,
    /// `@returns_retained` (`true`) or `@returns_not_retained` (`false`) and its span.
    pub returns: Option<(bool, Span)>,
    /// The span of a leading `class`: a class method.
    pub class_method: Option<Span>,
    /// The Objective-C selector when it is written; a member without arguments may leave it out.
    pub selector: Option<(Box<str>, Span)>,
    pub name: Ident,
    /// The parameters after the receiver; `unit` alone for a method without arguments.
    pub parameters: Vec<TypeExpr>,
    pub result: TypeExpr,
}

impl ObjcMember {
    /// The selector that the member sends: the written string, or the name.
    pub fn selector(&self) -> &str {
        self.selector
            .as_ref()
            .map_or(self.name.text.as_str(), |(selector, _)| selector)
    }

    /// The member as its declaration writes it, on one line.
    pub fn header(&self) -> String {
        let mut header = String::new();
        if let Some((retained, _)) = self.returns {
            header.push_str(if retained {
                "@returns_retained "
            } else {
                "@returns_not_retained "
            });
        }
        if self.class_method.is_some() {
            header.push_str("class ");
        }
        if let Some((selector, _)) = &self.selector {
            header.push_str(&LinkName::quote_str(selector));
            header.push(' ');
        }
        header.push_str(&self.name.text);
        header.push_str(" :: ");
        for parameter in &self.parameters {
            header.push_str(&crate::docgen::type_text(parameter));
            header.push_str(" -> ");
        }
        header.push_str(&crate::docgen::type_text(&self.result));
        header
    }
}

/// `@returns_retained` or `@returns_not_retained` before an `extern def` (M01): whether the
/// function hands over a +1 reference of its Objective-C result.
#[derive(Clone, Debug)]
pub struct OwnershipAttribute {
    pub target: Ident,
    pub retained: bool,
    pub span: Span,
}

/// `extern type [copy] Name`: an opaque host handle.
#[derive(Clone, Debug)]
pub struct ExternTypeDecl {
    pub doc: Option<Documentation>,
    pub visibility: Visibility,
    pub kind: HandleKind,
    pub kind_span: Option<Span>,
    pub name: Ident,
}

impl ExternTypeDecl {
    pub fn header(&self) -> String {
        self.header_with(&self.name.text)
    }

    pub fn header_with(&self, name: &str) -> String {
        match self.kind.keyword() {
            Some(modifier) => format!("extern type {modifier} {name}"),
            None => format!("extern type {name}"),
        }
    }
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
    /// Extra type arguments for builtin 3-argument indexing classes (`Index<Self, Key, Output>`
    /// and `IndexMut<Self, Key, Value>`); empty for all single-parameter classes.
    pub extra_args: Vec<TypeExpr>,
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
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Reference(Box<TypeExpr>, bool),
    Own(Box<TypeExpr>),
    Mut(Box<TypeExpr>),
    Pure(Box<TypeExpr>),
    /// `dyn Shapes.Shape` or `dyn (Shape, Send)`: an owned value of some type with instances of
    /// the classes, dispatched at run time (A14). Boxed like `Apply` so `TypeExpr` does not grow.
    Dyn(Box<DynTypeExpr>),
    /// `(A | B | ...)`: an anonymous union of two or more member types, in the order written
    /// (A24). The checker flattens, checks and orders the members. Boxed so `TypeExpr` does not grow.
    AnonymousUnion(Box<[TypeExpr]>),
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

impl BinaryOp {
    pub fn compound_symbol(self) -> &'static str {
        match self {
            Self::Add => "+=",
            Self::Subtract => "-=",
            Self::Multiply => "*=",
            Self::Divide => "/=",
            Self::Remainder => "%=",
            _ => unreachable!("only + - * / % have compound assignment"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub depth: usize,
}

impl Expr {
    /// Visits the expression and its subexpressions, outermost first, for rewriting: the visitor
    /// returns whether to descend into the (possibly replaced) expression. It reaches the same
    /// expressions as [`Expr::visit`], those in patterns and computation bodies included.
    pub(crate) fn visit_mut(&mut self, visitor: &mut impl FnMut(&mut Expr) -> bool) {
        if !visitor(self) {
            return;
        }
        use ExprKind::*;
        match &mut self.kind {
            Unary(_, value)
            | Lambda(_, value)
            | Routine(value)
            | ComputationBoundary(value)
            | Checked(value)
            | NewLiteral(value)
            | Field(value, _)
            | Borrow(value, ..)
            | Dereference(value, _)
            | Cast(value, _) => value.visit_mut(visitor),
            Binary(_, left, right)
            | Index(left, right)
            | Assign(left, right)
            | CompoundAssign {
                target: left,
                value: right,
                ..
            }
            | NewArray(_, left, right)
            | NewList(_, left, right) => {
                left.visit_mut(visitor);
                right.visit_mut(visitor);
            }
            Call(callee, arguments) => {
                callee.visit_mut(visitor);
                for argument in arguments {
                    argument.visit_mut(visitor);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.visit_mut(visitor);
                then_branch.visit_mut(visitor);
                else_branch.visit_mut(visitor);
            }
            IfLet(if_let) => {
                if_let.pattern.visit_expressions_mut(visitor);
                if_let.value.visit_mut(visitor);
                if let Some(guard) = &mut if_let.guard {
                    guard.visit_mut(visitor);
                }
                if_let.then_branch.visit_mut(visitor);
                if_let.else_branch.visit_mut(visitor);
            }
            While { condition, body } => {
                condition.visit_mut(visitor);
                body.visit_mut(visitor);
            }
            For {
                pattern,
                source,
                body,
            } => {
                pattern.visit_expressions_mut(visitor);
                source.visit_mut(visitor);
                body.visit_mut(visitor);
            }
            Range {
                start,
                step,
                finish,
                ..
            } => {
                start.visit_mut(visitor);
                if let Some(step) = step {
                    step.visit_mut(visitor);
                }
                finish.visit_mut(visitor);
            }
            Match { value, arms, .. } => {
                value.visit_mut(visitor);
                for arm in arms {
                    arm.pattern.visit_expressions_mut(visitor);
                    if let Some(guard) = &mut arm.guard {
                        guard.visit_mut(visitor);
                    }
                    arm.body.visit_mut(visitor);
                }
            }
            Select(arms) => {
                for arm in arms {
                    for expression in arm.expressions_mut() {
                        expression.visit_mut(visitor);
                    }
                }
            }
            Try(handled) => {
                handled.body.visit_mut(visitor);
                for arm in &mut handled.arms {
                    arm.pattern.visit_expressions_mut(visitor);
                    if let Some(guard) = &mut arm.guard {
                        guard.visit_mut(visitor);
                    }
                    arm.body.visit_mut(visitor);
                }
                if let Some(finally) = &mut handled.finally {
                    finally.visit_mut(visitor);
                }
            }
            Block { bindings, result } => {
                for binding in bindings {
                    if let Some((pattern, _)) = binding.pattern.as_deref_mut() {
                        pattern.visit_expressions_mut(visitor);
                    }
                    binding.value.visit_mut(visitor);
                }
                result.visit_mut(visitor);
            }
            Record { fields, .. } => {
                for (_, value) in fields {
                    value.visit_mut(visitor);
                }
            }
            RecordUpdate { base, fields } => {
                base.visit_mut(visitor);
                for (_, value) in fields {
                    value.visit_mut(visitor);
                }
            }
            Array(values) | List(values) | Tuple(values) => {
                for value in values {
                    value.visit_mut(visitor);
                }
            }
            Slice { value, range, .. } => {
                value.visit_mut(visitor);
                for bound in range
                    .start
                    .iter_mut()
                    .chain(range.end.iter_mut())
                    .chain(range.step.iter_mut())
                {
                    bound.visit_mut(visitor);
                }
            }
            MultiIndex { value, keys, .. } => {
                value.visit_mut(visitor);
                for key in keys {
                    match key {
                        IndexComponent::Expr(expr) => expr.visit_mut(visitor),
                        IndexComponent::Range(range) => {
                            for bound in range
                                .start
                                .iter_mut()
                                .chain(range.end.iter_mut())
                                .chain(range.step.iter_mut())
                            {
                                bound.visit_mut(visitor);
                            }
                        }
                        IndexComponent::Whole(_) => {}
                    }
                }
            }
            FromEnd(offset) => offset.visit_mut(visitor),
            Computation(_, body) => body.visit_expressions_mut(visitor),
            Interpolated(interpolation) => {
                for hole in &mut interpolation.holes {
                    hole.value.visit_mut(visitor);
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
            | Hole(_)
            | Name(_)
            | QualifiedFunction(_)
            | TypeFunction(..)
            | DynDispatch { .. } => {}
        }
    }

    pub(crate) fn visit(&self, visitor: &mut impl FnMut(&Expr)) {
        visitor(self);
        use ExprKind::*;
        match &self.kind {
            Unary(_, value)
            | Lambda(_, value)
            | Routine(value)
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
            | CompoundAssign {
                target: left,
                value: right,
                ..
            }
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
            IfLet(if_let) => {
                if_let.pattern.visit_expressions(visitor);
                if_let.value.visit(visitor);
                if let Some(guard) = &if_let.guard {
                    guard.visit(visitor);
                }
                if_let.then_branch.visit(visitor);
                if_let.else_branch.visit(visitor);
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
            Select(arms) => {
                for arm in arms {
                    for expression in arm.expressions() {
                        expression.visit(visitor);
                    }
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
                    if let Some((pattern, _)) = binding.pattern.as_deref() {
                        pattern.visit_expressions(visitor);
                    }
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
            Slice { value, range, .. } => {
                value.visit(visitor);
                for bound in range.start.iter().chain(&range.end).chain(&range.step) {
                    bound.visit(visitor);
                }
            }
            MultiIndex { value, keys, .. } => {
                value.visit(visitor);
                for key in keys {
                    match key {
                        IndexComponent::Expr(expr) => expr.visit(visitor),
                        IndexComponent::Range(range) => {
                            for bound in range.start.iter().chain(&range.end).chain(&range.step) {
                                bound.visit(visitor);
                            }
                        }
                        IndexComponent::Whole(_) => {}
                    }
                }
            }
            FromEnd(offset) => offset.visit(visitor),
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
            | Hole(_)
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
    /// `?` or `?name`: a typed hole (G25), always an error (`E1030`) that reports the type expected
    /// at its position. Not the hole of an interpolated string (`InterpolationHole`).
    Hole(Option<Ident>),
    Name(Ident),
    QualifiedFunction(Ident),
    TypeFunction(Box<Ident>, Box<Ident>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Lambda(Vec<(Ident, bool)>, Box<Expr>),
    /// `tzroutine { body }`: spawns the block as a new tzroutine and returns `()` (TZ02).
    Routine(Box<Expr>),
    /// `select | recv x from ch -> e | send v to ch -> e | default -> e` (TZ04): waits for one of
    /// the arms' channel operations.
    Select(Vec<SelectArm>),
    Computation(Ident, Box<ComputationBlock>),
    ComputationBoundary(Box<Expr>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    /// `if let <pattern> = <value> [when <guard>] then <then_branch> [else <else_branch>]` (NS06).
    IfLet(Box<IfLetExpr>),
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
        /// Which endpoints are open/closed, settled by the parser from the source's edition (NC17).
        ends: RangeEnds,
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
    /// `ref xs[a..b]`, `ref mut xs[a..b]`, or an unborrowed window target `t[a..b] += u` (C08 / NC17 / NC20).
    Slice {
        value: Box<Expr>,
        range: Box<RangeKey>,
        mutable: Option<bool>,
    },
    /// `base[c1, ..., cn]` with two or more components (NC18). `borrow` is `Some(false)` for
    /// `ref`, `Some(true)` for `ref mut`, as `Slice::mutable` is for one-component slices.
    MultiIndex {
        value: Box<Expr>,
        keys: Box<[IndexComponent]>,
        borrow: Option<bool>,
    },
    /// `^k`: position `n - 1 - k` from the end; only at the start of an index component or slice bound (NC17).
    FromEnd(Box<Expr>),
    Borrow(Box<Expr>, bool, Notation),
    Dereference(Box<Expr>, Notation),
    Assign(Box<Expr>, Box<Expr>),
    /// `target += value` (`+=`, `-=`, `*=`, `/=`, `%=`, NC20).
    CompoundAssign {
        op: BinaryOp,
        op_span: Span,
        target: Box<Expr>,
        value: Box<Expr>,
    },
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

/// An `if let` / `elif let` conditional pattern binding (NS06).
#[derive(Clone, Debug)]
pub struct IfLetExpr {
    pub pattern: Pattern,
    pub value: Expr,
    pub guard: Option<Expr>,
    pub then_branch: Expr,
    pub else_branch: Expr,
    pub has_else: bool,
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
    /// Edition 2026/2027 bare `..` slice with an end bound (`W1013`, enabled by default, fix `..<`).
    LegacyBareSliceRange,
    /// Edition 2028+ bare `..` slice with an end bound, noted as `W1013` on manifest-less sources.
    BareSliceRangeMeaningChanged,
}

impl NoncanonicalKind {
    /// Whether `edition` emits a warning for this kind (`W1007` under `--warn canonical`, or
    /// `W1013` by default in Phase 2+).
    pub fn warned_in(self, edition: Edition) -> bool {
        match self {
            Self::BareRange
            | Self::ShortSuffix
            | Self::SymbolicBorrow
            | Self::SymbolicDeref
            | Self::SymbolicReferenceType
            | Self::LegacyLambda => edition == Edition::E2026,
            Self::LegacyBareSliceRange => !edition.range_family(),
            Self::BareSliceRangeMeaningChanged => edition.range_family(),
        }
    }

    /// Whether `edition` rejects this kind with `E0004`.
    pub fn rejected_in(self, edition: Edition) -> bool {
        match self {
            Self::BareRange => edition == Edition::E2027,
            Self::ShortSuffix
            | Self::SymbolicBorrow
            | Self::SymbolicDeref
            | Self::SymbolicReferenceType
            | Self::LegacyLambda => edition >= Edition::E2027,
            Self::LegacyBareSliceRange | Self::BareSliceRangeMeaningChanged => false,
        }
    }

    /// The warning code associated with this kind when warned (`"W1007"` or `"W1013"`).
    pub fn code(self) -> &'static str {
        match self {
            Self::LegacyBareSliceRange | Self::BareSliceRangeMeaningChanged => "W1013",
            _ => "W1007",
        }
    }
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

/// The spelling of a range operator during parsing only; never stored in the AST (NC17 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RangeOp {
    Bare,
    Closed,
    Open,
    StartOpen,
    BothOpen,
}

/// Which endpoints of a range are open/closed, settled by the parser from the source's edition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RangeEnds {
    pub start_open: bool,
    pub end_closed: bool,
}

/// One range inside brackets; shared by single-axis slices and multi-axis keys (NC17 / NC18).
#[derive(Clone, Debug)]
pub struct RangeKey {
    pub start: Option<Box<Expr>>,
    pub end: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,
    pub ends: RangeEnds,
}

/// One bracket component parsed by `Parser::index_component` (NC17 / NC18).
#[derive(Clone, Debug)]
pub enum IndexComponent {
    Expr(Expr),
    Range(Box<RangeKey>),
    Whole(Span),
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
    /// `:? T` or `:? T as x`: whether an anonymous union holds its member `T`, binding the member's
    /// value to `x` (A24). The binder `_` stays an `Ident`; the checker treats it as a wildcard.
    /// The span is the test's own extent, from its `:?` to the type or the binder, where the
    /// checker reports what is wrong with the test (A25); the pattern's span also covers the
    /// parentheses that group it, as the span of any other pattern does.
    TypeTest(Box<TypeExpr>, Option<Ident>, Span),
}

impl Pattern {
    fn visit_expressions_mut(&mut self, visitor: &mut impl FnMut(&mut Expr) -> bool) {
        use PatternKind::*;
        match &mut self.kind {
            Literal(value) | Argument(value) => value.visit_mut(visitor),
            Apply(_, patterns) | Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    pattern.visit_expressions_mut(visitor);
                }
            }
            Record(_, fields) => {
                for (_, pattern) in fields {
                    pattern.visit_expressions_mut(visitor);
                }
            }
            Cons(left, right) | Or(left, right) | And(left, right) => {
                left.visit_expressions_mut(visitor);
                right.visit_expressions_mut(visitor);
            }
            As(pattern, _) | Annotated(pattern, _) => pattern.visit_expressions_mut(visitor),
            Wildcard | Binding(_) | TypeTest(..) => {}
        }
    }

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
            Wildcard | Binding(_) | TypeTest(..) => {}
        }
    }

    pub fn bound_names(&self, out: &mut Vec<Ident>) {
        use PatternKind::*;
        match &self.kind {
            Wildcard | Argument(_) | Literal(_) => {}
            Binding(name) => {
                if !name.text.as_bytes()[0].is_ascii_uppercase() && !name.text.contains(['.', ':'])
                {
                    out.push(name.clone());
                }
            }
            Apply(_, patterns) | Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    pattern.bound_names(out);
                }
            }
            Record(_, fields) => {
                for (_, pattern) in fields {
                    pattern.bound_names(out);
                }
            }
            Cons(left, right) | And(left, right) => {
                left.bound_names(out);
                right.bound_names(out);
            }
            Or(left, _) => left.bound_names(out),
            As(pattern, name) => {
                pattern.bound_names(out);
                if name.text != "_" {
                    out.push(name.clone());
                }
            }
            Annotated(pattern, _) => pattern.bound_names(out),
            TypeTest(_, Some(binder), _) if binder.text != "_" => out.push(binder.clone()),
            TypeTest(..) => {}
        }
    }
}

/// Where a `match` comes from. Explicit matches, function guards, and user destructuring bindings
/// check coverage; compiler-generated internal matches do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchOrigin {
    /// `match value with ...`.
    Explicit,
    /// The clauses of `fn f x | pattern -> ...`.
    FunctionGuard,
    /// A destructuring lambda parameter.
    LambdaDestructuring,
    /// A destructuring `let!`, `and!`, or `for` inside a computation expression.
    ComputationDestructuring,
    /// A destructuring `for ... in` loop.
    ForDestructuring,
    /// An immutable `let <pattern> = ...` destructuring binding.
    LetDestructuring,
    /// A conditional `if let <pattern> = ...` / `elif let <pattern> = ...` binding (NS06).
    IfLet,
    /// A compiler-synthesized match (`for` channel/seq step or `select` arm dispatch).
    Internal,
}

impl MatchOrigin {
    /// Whether the checker inspects coverage at compile time.
    pub fn checks_coverage(self) -> bool {
        self != Self::Internal
    }

    /// The user-facing description of a single-pattern destructuring site, or `None` for multi-arm matches.
    pub fn binding_place_name(self) -> Option<&'static str> {
        match self {
            Self::LambdaDestructuring => Some("a lambda parameter"),
            Self::ComputationDestructuring => Some("a computation binding"),
            Self::ForDestructuring => Some("a 'for' binding"),
            Self::LetDestructuring => Some("a 'let' binding"),
            Self::Explicit | Self::FunctionGuard | Self::IfLet | Self::Internal => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// One arm of a `select` (TZ04): its head, the optional `when` guard, and the body.
#[derive(Clone, Debug)]
pub struct SelectArm {
    pub head: SelectHead,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum SelectHead {
    /// `recv name from channel`; the name is `_` to drop what was received.
    Recv {
        name: Ident,
        channel: Box<Expr>,
    },
    /// `send value to channel`.
    Send {
        value: Box<Expr>,
        channel: Box<Expr>,
    },
    Default,
}

impl SelectArm {
    /// The expressions of the arm in evaluation order: the guard, the channel, the value, the body.
    pub fn expressions(&self) -> Vec<&Expr> {
        let mut found: Vec<&Expr> = self.guard.iter().collect();
        match &self.head {
            SelectHead::Recv { channel, .. } => found.push(channel),
            SelectHead::Send { value, channel } => {
                found.push(channel);
                found.push(value);
            }
            SelectHead::Default => {}
        }
        found.push(&self.body);
        found
    }

    pub fn expressions_mut(&mut self) -> Vec<&mut Expr> {
        let mut found: Vec<&mut Expr> = self.guard.iter_mut().collect();
        match &mut self.head {
            SelectHead::Recv { channel, .. } => found.push(channel),
            SelectHead::Send { value, channel } => {
                found.push(channel);
                found.push(value);
            }
            SelectHead::Default => {}
        }
        found.push(&mut self.body);
        found
    }
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
    pub pattern: Option<Box<(Pattern, MatchOrigin)>>,
    pub mutable: bool,
    /// A `use` binding: a `let` whose value type must implement `Drop` (B07).
    pub using: bool,
    pub recursion: Recursion,
    pub annotation: Option<TypeExpr>,
    pub value: Expr,
}

impl Binding {
    pub fn bound_names(&self, out: &mut Vec<Ident>) {
        if let Some((pattern, _)) = self.pattern.as_deref() {
            pattern.bound_names(out);
        } else if self.name.provenance == Provenance::User && self.name.text != "_" {
            out.push(self.name.clone());
        }
    }
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
    IfLet(Box<ComputationIfLet>),
    For(Box<Pattern>, Expr, ComputationBlock),
    While(Expr, ComputationBlock),
    Expression(Expr),
}

/// An `if let` / `elif let` statement inside a computation block (NS06).
#[derive(Clone, Debug)]
pub struct ComputationIfLet {
    pub pattern: Box<Pattern>,
    pub value: Expr,
    pub guard: Option<Expr>,
    pub then_branch: ComputationBlock,
    pub else_branch: Option<ComputationBlock>,
}

#[derive(Clone, Debug)]
pub struct ComputationMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: ComputationBlock,
    pub span: Span,
}

impl ComputationBlock {
    fn visit_expressions_mut(&mut self, visitor: &mut impl FnMut(&mut Expr) -> bool) {
        use ComputationStatementKind::*;
        for statement in &mut self.statements {
            match &mut statement.kind {
                Let(binding, _) => {
                    if let Some((pattern, _)) = binding.pattern.as_deref_mut() {
                        pattern.visit_expressions_mut(visitor);
                    }
                    binding.value.visit_mut(visitor);
                }
                LetAnd(bindings) => {
                    for binding in bindings {
                        if let Some((pattern, _)) = binding.pattern.as_deref_mut() {
                            pattern.visit_expressions_mut(visitor);
                        }
                        binding.value.visit_mut(visitor);
                    }
                }
                Do(value) | Operation(_, value) | Expression(value) => value.visit_mut(visitor),
                Match(value, arms) => {
                    value.visit_mut(visitor);
                    for arm in arms {
                        arm.pattern.visit_expressions_mut(visitor);
                        if let Some(guard) = &mut arm.guard {
                            guard.visit_mut(visitor);
                        }
                        arm.body.visit_expressions_mut(visitor);
                    }
                }
                If(condition, yes, no) => {
                    condition.visit_mut(visitor);
                    yes.visit_expressions_mut(visitor);
                    if let Some(no) = no {
                        no.visit_expressions_mut(visitor);
                    }
                }
                IfLet(if_let) => {
                    if_let.pattern.visit_expressions_mut(visitor);
                    if_let.value.visit_mut(visitor);
                    if let Some(guard) = &mut if_let.guard {
                        guard.visit_mut(visitor);
                    }
                    if_let.then_branch.visit_expressions_mut(visitor);
                    if let Some(no) = &mut if_let.else_branch {
                        no.visit_expressions_mut(visitor);
                    }
                }
                For(pattern, source, body) => {
                    pattern.visit_expressions_mut(visitor);
                    source.visit_mut(visitor);
                    body.visit_expressions_mut(visitor);
                }
                While(condition, body) => {
                    condition.visit_mut(visitor);
                    body.visit_expressions_mut(visitor);
                }
            }
        }
    }

    fn visit_expressions(&self, visitor: &mut impl FnMut(&Expr)) {
        use ComputationStatementKind::*;
        for statement in &self.statements {
            match &statement.kind {
                Let(binding, _) => {
                    if let Some((pattern, _)) = binding.pattern.as_deref() {
                        pattern.visit_expressions(visitor);
                    }
                    binding.value.visit(visitor);
                }
                LetAnd(bindings) => {
                    for binding in bindings {
                        if let Some((pattern, _)) = binding.pattern.as_deref() {
                            pattern.visit_expressions(visitor);
                        }
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
                IfLet(if_let) => {
                    if_let.pattern.visit_expressions(visitor);
                    if_let.value.visit(visitor);
                    if let Some(guard) = &if_let.guard {
                        guard.visit(visitor);
                    }
                    if_let.then_branch.visit_expressions(visitor);
                    if let Some(no) = &if_let.else_branch {
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
            Let(binding, _) => binding
                .pattern
                .as_ref()
                .map_or(0, |pair| pair.0.depth)
                .max(binding.value.depth),
            LetAnd(bindings) => bindings
                .iter()
                .map(|binding| {
                    binding
                        .pattern
                        .as_ref()
                        .map_or(0, |pair| pair.0.depth)
                        .max(binding.value.depth)
                })
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
            IfLet(if_let) => if_let
                .pattern
                .depth
                .max(if_let.value.depth)
                .max(if_let.guard.as_ref().map_or(0, |guard| guard.depth))
                .max(if_let.then_branch.depth)
                .max(if_let.else_branch.as_ref().map_or(0, |branch| branch.depth)),
            For(pattern, source, body) => pattern.depth.max(source.depth).max(body.depth),
            While(source, body) => source.depth.max(body.depth),
        }
    }
}
