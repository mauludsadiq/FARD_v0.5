use crate::ast::{BinOp, Expr};

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

        Expr::List(xs) => Expr::List(xs.into_iter().map(desugar_expr).collect()),

        other => other,
    }
}
