use crate::ast::*;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct CheckEnv {
    pub effects: BTreeSet<String>,
}

pub fn check_module(m: &Module) -> Result<()> {
    let mut effects = BTreeSet::new();
    for e in &m.effects {
        effects.insert(e.name.clone());
    }
    let env = CheckEnv { effects };

    for f in &m.fns {
        check_fn(&env, f)?;
    }
    Ok(())
}

fn check_fn(env: &CheckEnv, f: &FnDecl) -> Result<()> {
    let allowed: BTreeSet<String> = f.uses.iter().cloned().collect();
    let mut vars: BTreeMap<String, ()> = BTreeMap::new();
    for (p, _) in &f.params {
        vars.insert(p.clone(), ());
    }

    check_block(env, &allowed, &mut vars, &f.body)?;
    Ok(())
}

fn check_block(
    env: &CheckEnv,
    allowed: &BTreeSet<String>,
    vars: &mut BTreeMap<String, ()>,
    b: &Block,
) -> Result<()> {
    for s in &b.stmts {
        match s {
            Stmt::Let { name, expr } => {
                check_expr(env, allowed, vars, expr)?;
                vars.insert(name.clone(), ());
            }
            Stmt::Expr(e) => check_expr(env, allowed, vars, e)?,
        }
    }
    if let Some(t) = &b.tail {
        check_expr(env, allowed, vars, t)?;
    }
    Ok(())
}

fn check_expr(
    env: &CheckEnv,
    allowed: &BTreeSet<String>,
    vars: &BTreeMap<String, ()>,
    e: &Expr,
) -> Result<()> {
    match e {
        // check_operator_close_v1 begin
        Expr::UnaryMinus(x) => {
            let e2 = crate::desugar::desugar_expr(Expr::UnaryMinus(x.clone()));
            return check_expr(env, allowed, vars, &e2);
        }

        Expr::BinOp { op, lhs, rhs } => {
            let e2 = crate::desugar::desugar_expr(Expr::BinOp {
                op: op.clone(),
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            });
            return check_expr(env, allowed, vars, &e2);
        }
        // check_operator_close_v1 end
        Expr::Unit
        | Expr::Bool(_)
        | Expr::Int(_)
        | Expr::Text(_)
        | Expr::BytesHex(_)
        | Expr::List(_) => Ok(()),
        Expr::Ident(x) => {
            if vars.contains_key(x) {
                Ok(())
            } else {
                Ok(())
            } // could be global fn; resolved in lowering phase
        }
        Expr::Call { f, args } => {
            for a in args {
                check_expr(env, allowed, vars, a)?;
            }
            if env.effects.contains(f) && !allowed.contains(f) {
                bail!("ERROR_EFFECT_NOT_ALLOWED {} not in uses[]", f);
            }
            Ok(())
        }
        Expr::If { c, t, e } => {
            check_expr(env, allowed, vars, c)?;
            let mut v1 = vars.clone();
            let mut v2 = vars.clone();
            check_block(env, allowed, &mut v1, t)?;
            check_block(env, allowed, &mut v2, e)?;
            Ok(())
        }
    }
}
