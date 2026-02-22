pub mod ast;
pub mod canon;
pub mod check;
pub mod lex;
pub mod parse;

pub mod desugar;
pub use ast::*;
pub use parse::parse_module;

pub mod eval;
pub mod effects;
