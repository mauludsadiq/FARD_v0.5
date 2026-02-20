#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: ModPath,
    pub imports: Vec<ImportDecl>,
    pub fact_imports: Vec<FactImportDecl>,
    pub effects: Vec<EffectDecl>,
    pub types: Vec<TypeDecl>,
    pub fns: Vec<FnDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModPath(pub Vec<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDecl {
    pub path: ModPath,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactImportDecl {
    pub name: String,
    pub run_id: String, // must include "sha256:" prefix
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDecl {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: TypeBody,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBody {
    Record(Vec<(String, Type)>),
    Sum(Vec<Variant>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub ret: Option<Type>,
    pub uses: Vec<String>,
    pub body: Block,
    pub is_pub: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let { name: String, expr: Expr },
    Expr(Expr),
}

// canon_binop_ast_v1 begin
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}
// canon_binop_ast_v1 end
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Unit,
    Bool(bool),
    Int(String),      // keep as source string; validation later
    Text(String),     // decoded content
    BytesHex(String), // lower/upper accepted; normalized later
    List(Vec<Expr>),
    Ident(String),
    UnaryMinus(Box<Expr>),
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        f: String,
        args: Vec<Expr>,
    },
    If {
        c: Box<Expr>,
        t: Box<Block>,
        e: Box<Block>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Bool,
    Int,
    Bytes,
    Text,
    Value,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Named { name: String, args: Vec<Type> },
}
