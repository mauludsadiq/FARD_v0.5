use crate::ast::{BinOp, Expr, MatchArm};

fn op_name(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Concat => "text_concat",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::Eq => "cmp.eq",
        BinOp::Lt => "cmp.lt",
        BinOp::Gt => "cmp.gt",
        BinOp::Le => "cmp.le",
        BinOp::Ge => "cmp.ge",
        BinOp::And => "bool.and",
        BinOp::Or => "bool.or",
    }
}

pub fn desugar_expr(e: Expr) -> Expr {
    match e {
        Expr::UnaryMinus(x) => {
            let x = desugar_expr(*x);
            Expr::Call {
                f: "neg".to_string(),
                args: vec![x],
            }
        }

        Expr::BinOp { op: BinOp::And, lhs, rhs } => {
            let lhs = desugar_expr(*lhs);
            let rhs = desugar_expr(*rhs);
            Expr::If {
                c: Box::new(lhs),
                t: Box::new(crate::ast::Block { stmts: vec![], tail: Some(Box::new(rhs)) }),
                e: Box::new(crate::ast::Block { stmts: vec![], tail: Some(Box::new(Expr::Bool(false))) }),
            }
        }
        Expr::BinOp { op: BinOp::Or, lhs, rhs } => {
            let lhs = desugar_expr(*lhs);
            let rhs = desugar_expr(*rhs);
            Expr::If {
                c: Box::new(lhs),
                t: Box::new(crate::ast::Block { stmts: vec![], tail: Some(Box::new(Expr::Bool(true))) }),
                e: Box::new(crate::ast::Block { stmts: vec![], tail: Some(Box::new(rhs)) }),
            }
        }
        Expr::BinOp { op, lhs, rhs } => {
            let lhs = desugar_expr(*lhs);
            let rhs = desugar_expr(*rhs);
            Expr::Call {
                f: op_name(&op).to_string(),
                args: vec![lhs, rhs],
            }
        }

        Expr::If { c, t, e } => Expr::If {
            c: Box::new(desugar_expr(*c)),
            t,
            e,
        },

        Expr::Call { f, args } => Expr::Call {
            f,
            args: args.into_iter().map(desugar_expr).collect(),
        },

        Expr::Match { scrut, arms } => Expr::Match {
            scrut: Box::new(desugar_expr(*scrut)),
            arms: arms
                .into_iter()
                .map(|a| MatchArm { pat: a.pat, body: desugar_expr(a.body) })
                .collect(),
        },

        Expr::List(xs) => Expr::List(xs.into_iter().map(desugar_expr).collect()),

        Expr::Lambda { params, body } => Expr::Lambda { params, body },

        Expr::TryExpr { inner } => Expr::TryExpr { inner: Box::new(desugar_expr(*inner)) },
        Expr::CallExpr { f, args } => Expr::CallExpr {
            f: Box::new(desugar_expr(*f)),
            args: args.into_iter().map(desugar_expr).collect(),
        },

        other => other,
    }
}
