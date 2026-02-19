use fardlang::ast::Expr;
use fardlang::canon::print_expr_public as print_expr;

#[test]
fn canon_list_spacing_is_stable() {
    let e = Expr::List(vec![
        Expr::Int("1".into()),
        Expr::Int("2".into()),
        Expr::Int("3".into()),
    ]);
    assert_eq!(print_expr(&e), "[1, 2, 3]");

    let nested = Expr::List(vec![
        Expr::List(vec![Expr::Unit, Expr::Bool(true)]),
        Expr::List(vec![Expr::Text("x".into())]),
    ]);
    assert_eq!(print_expr(&nested), "[[unit, true], [\"x\"]]");
}
