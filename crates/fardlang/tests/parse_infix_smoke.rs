use fardlang::parse::parse_module;

#[test]
fn parse_infix_produces_nodes_smoke() {
    let src = r#"
module m

pub fn main() : int {
  // infix coverage that must construct BinOp/UnaryMinus
  let a = 1 + 2 * 3
  let b = -1 + 2
  let c = 1 < 2 && 3 <= 4 || 5 == 6
  a
}
"#;

    let m = parse_module(src.as_bytes()).unwrap();
    let dbg = format!("{m:#?}");

    // Be tolerant to Debug formatting differences.
    // We want evidence of operator nodes existing in the AST.
    let has_binop = dbg.contains("BinOp") || dbg.contains("Expr::BinOp") || dbg.contains("op:");
    let has_unary = dbg.contains("UnaryMinus") || dbg.contains("Expr::UnaryMinus");

    assert!(
        has_binop,
        "expected BinOp-like nodes in AST debug output, got:\n{dbg}"
    );
    assert!(
        has_unary,
        "expected UnaryMinus-like nodes in AST debug output, got:\n{dbg}"
    );
}
