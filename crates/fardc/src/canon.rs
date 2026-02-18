use crate::ast::{Expr, Module};

/// Canonical printer: deterministic, stable bytes.
/// IMPORTANT: For the Gate5 fixture, this must emit exactly: `fn main() { unit }`
pub fn print_module(m: &Module) -> String {
    // v0: single-module, many functions; join with '\n' for stability
    let mut out = String::new();
    for (i, f) in m.funcs.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str("fn ");
        out.push_str(&f.name);
        out.push_str("() { ");
        out.push_str(&print_expr(&f.body));
        out.push_str(" }");
    }
    out
}

fn print_expr(e: &Expr) -> &'static str {
    match e {
        Expr::Unit => "unit",
    }
}
