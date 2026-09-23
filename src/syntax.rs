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
    TypeVariable(String),
    Integer(String),
    Float(String),
    String(String),
    Fn,
    Fx,
    Def,
    Rec,
    And,
    Export,
    Record,
    Class,
    Instance,
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
    Arrow,
    FatArrow,
    Equal,
    Plus,
    Minus,
    Star,
    Slash,
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

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub text: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct Program {
    pub source_kind: Option<SourceKind>,
    pub records: Vec<RecordDecl>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
    pub instances: Vec<InstanceDecl>,
    pub active_patterns: Vec<ActivePattern>,
    pub entry: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct ActivePattern {
    pub name: Ident,
    pub function: String,
    pub partial: bool,
}

#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub name: Ident,
    pub fields: Vec<Parameter>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: Ident,
    pub recursion: Option<String>,
    pub exported: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct SignatureDecl {
    pub name: Ident,
    pub recursion: Option<String>,
    pub exported: bool,
    pub parameters: Vec<TypeExpr>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
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
    pub class: Ident,
    pub ty: TypeExpr,
}

#[derive(Debug)]
pub struct ClassDecl {
    pub name: Ident,
    pub variable: Ident,
    pub methods: Vec<SignatureDecl>,
}

#[derive(Debug)]
pub struct InstanceDecl {
    pub class: Ident,
    pub ty: TypeExpr,
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
    Constrained(Box<Ident>, Box<TypeExpr>),
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

#[derive(Clone, Debug)]
pub enum ExprKind {
    Integer(u128, Option<String>),
    Float(String, Option<String>),
    String(String),
    Bool(bool),
    Unit,
    Name(Ident),
    QualifiedFunction(Ident),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Lambda(Vec<(Ident, bool)>, Box<Expr>),
    Task(Box<Expr>),
    TaskRun(Box<Expr>),
    Computation(Ident, Box<ComputationBlock>),
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
    },
    Block {
        bindings: Vec<Binding>,
        result: Box<Expr>,
    },
    Record {
        name: Ident,
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
    Borrow(Box<Expr>, bool, Notation),
    Dereference(Box<Expr>, Notation),
    Assign(Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, TypeExpr),
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
    Do(Expr),
    Operation(&'static str, Expr),
    If(Expr, ComputationBlock, Option<ComputationBlock>),
    For(Box<Pattern>, Expr, ComputationBlock),
    While(Expr, ComputationBlock),
    Expression(Expr),
}

impl ComputationStatement {
    pub fn depth(&self) -> usize {
        use ComputationStatementKind::*;
        match &self.kind {
            Let(binding, _) => binding.value.depth,
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
