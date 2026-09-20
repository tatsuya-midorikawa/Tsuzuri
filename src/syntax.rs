use crate::diagnostic::Span;

pub const MAX_NESTING: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Integer(String),
    Float(String),
    String(String),
    Fn,
    Export,
    Record,
    Let,
    Mut,
    As,
    If,
    Else,
    True,
    False,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Arrow,
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
    pub records: Vec<RecordDecl>,
    pub functions: Vec<FunctionDecl>,
    pub entry: Option<Expr>,
}

#[derive(Debug)]
pub struct RecordDecl {
    pub name: Ident,
    pub fields: Vec<Parameter>,
}

#[derive(Debug)]
pub struct FunctionDecl {
    pub name: Ident,
    pub exported: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpr,
    pub body: Expr,
}

#[derive(Debug)]
pub struct Parameter {
    pub name: Ident,
    pub ty: TypeExpr,
    pub mutable: bool,
}

#[derive(Debug)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypeExprKind {
    Named(String),
    Array(Box<TypeExpr>, usize),
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

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub depth: usize,
}

#[derive(Debug)]
pub enum ExprKind {
    Integer(u128, Option<String>),
    Float(String, Option<String>),
    String(String),
    Bool(bool),
    Unit,
    Name(Ident),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
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
    Field(Box<Expr>, Ident),
    Index(Box<Expr>, Box<Expr>),
    Borrow(Box<Expr>, bool),
    Dereference(Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, TypeExpr),
}

#[derive(Debug)]
pub struct Binding {
    pub name: Ident,
    pub mutable: bool,
    pub annotation: Option<TypeExpr>,
    pub value: Expr,
}
