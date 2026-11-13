//! Arvore sintatica do stage 0.

use crate::source::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Fn(FnItem),
    Const(ConstItem),
    Struct(StructItem),
    Impl(ImplItem),
    Enum(EnumItem),
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: Ident,
    pub fields: Vec<Type>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EnumItem {
    pub name: Ident,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Arm {
    pub pat: Pat,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum PatKind {
    /// `_`
    Wild,
    /// um nome novo: `n`
    Bind(Ident),
    /// numero, texto, caractere, `真` / `偽` (ou `-5`)
    Lit(Box<Expr>),
    /// `1..=5`
    Range {
        lo: Box<Expr>,
        hi: Box<Expr>,
        inclusive: bool,
    },
    /// `形::円(r)`
    Variant {
        ty: Ident,
        name: Ident,
        fields: Vec<Pat>,
    },
    /// `a | b`
    Or(Vec<Pat>),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: Ident,
    pub ty: Type,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StructItem {
    pub name: Ident,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImplItem {
    pub ty: Ident,
    pub fns: Vec<FnItem>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// `自分`
    Value,
    /// `&自分`
    Ref,
    /// `&可変 自分`
    RefMut,
}

#[derive(Debug, Clone, Copy)]
pub struct SelfParam {
    pub kind: SelfKind,
    /// `可変 自分`
    pub mutable: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// Um tipo: um nome (`整32`, `点`, `自身`), opcionalmente atras de `&` / `&可変`.
#[derive(Debug, Clone)]
pub struct Type {
    pub name: String,
    pub span: Span,
    /// `Some(false)` = `&T`, `Some(true)` = `&可変 T`.
    pub reference: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FnItem {
    pub name: Ident,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ConstItem {
    pub name: Ident,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Ultima expressao sem `;`: o valor do bloco.
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let {
        mutable: bool,
        name: Ident,
        ty: Option<Type>,
        init: Expr,
    },
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Expr(Expr),
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(u128),
    Float(String),
    Str(String),
    Char(char),
    Bool(bool),
    Path(Ident),
    Paren(Box<Expr>),
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Match {
        scrut: Box<Expr>,
        arms: Vec<Arm>,
    },
    Field {
        base: Box<Expr>,
        name: Ident,
    },
    /// `点::原点`
    AssocPath {
        ty: Ident,
        name: Ident,
    },
    StructLit {
        name: Ident,
        fields: Vec<(Ident, Expr)>,
    },
    /// `&x` / `&可変 x`
    Ref {
        mutable: bool,
        expr: Box<Expr>,
    },
    MethodCall {
        recv: Box<Expr>,
        name: Ident,
        args: Vec<Expr>,
    },
    MacroCall {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
    },
    If {
        cond: Box<Expr>,
        then: Block,
        /// Outro `If` (para `他もし`) ou um `Block`.
        else_: Option<Box<Expr>>,
    },
    While {
        cond: Box<Expr>,
        body: Block,
    },
    Loop {
        body: Block,
    },
    For {
        var: Ident,
        iter: Box<Expr>,
        body: Block,
    },
    Block(Block),
}
