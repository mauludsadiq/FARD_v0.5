#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub funcs: Vec<Func>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Unit,
}
