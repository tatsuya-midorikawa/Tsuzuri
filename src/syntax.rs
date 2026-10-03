use crate::diagnostic::Span;

pub const MAX_NESTING: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

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

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    DocComment(String),
    TypeVariable(String),
    Integer(String),
    Float(String),
    String(StringLiteral),
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
    Class,
    Instance,
    Deriving,
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
    Semicolon,
    Comma,
    Dot,
    DotDot,
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
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
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
    pub entry: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct TestDecl {
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
}

impl DeriveClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "Eq",
            Self::Ord => "Ord",
            Self::Display => "Display",
            Self::Hash => "Hash",
            Self::Default => "Default",
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnionCaseDecl {
    pub name: Ident,
    pub payload: Option<TypeExpr>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub doc: Option<Documentation>,
    pub name: Ident,
    pub regions: Vec<Ident>,
    pub recursion: Option<String>,
    pub visibility: Visibility,
    pub exported: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct SignatureDecl {
    pub doc: Option<Documentation>,
    pub name: Ident,
    pub regions: Vec<Ident>,
    pub recursion: Option<String>,
    pub visibility: Visibility,
    pub exported: bool,
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
    Function(Ident),
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
    List(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Task(Box<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Reference(Box<TypeExpr>, bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    BitNot,
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
            Slice { value, start, end } => {
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
            Integer(..) | Float(..) | String(_) | Char(_) | Utf8Char(_) | Bool(_) | Unit
            | Break | Continue | Name(_) | QualifiedFunction(_) | TypeFunction(..) => {}
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Integer(u128, Option<String>),
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
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
        origin: MatchOrigin,
    },
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
    Slice {
        value: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Borrow(Box<Expr>, bool, Notation),
    Dereference(Box<Expr>, Notation),
    Assign(Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, TypeExpr),
    /// `$"a{x}b"`: `texts` has one more element than `holes`.
    Interpolated(Box<Interpolation>),
}

#[derive(Clone, Debug)]
pub struct Interpolation {
    pub texts: Vec<StringLiteral>,
    pub holes: Vec<InterpolationHole>,
}

#[derive(Clone, Debug)]
pub struct InterpolationHole {
    pub value: Expr,
    pub spec: Option<FormatSpec>,
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

#[derive(Clone, Debug)]
pub struct Binding {
    pub name: Ident,
    pub mutable: bool,
    /// A `use` binding: a `let` whose value type must implement `Drop` (B07).
    pub using: bool,
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
