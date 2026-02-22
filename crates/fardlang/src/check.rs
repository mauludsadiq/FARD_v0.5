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
    // std.io and std.http auto-declare effects
    let io_fns = ["read_file","write_file","clock_now","random_bytes"];
    let http_fns = ["http_get"];
    for imp in &m.imports {
        let parts = &imp.path.0;
        if parts.len() == 2 && parts[0] == "std" {
            match parts[1].as_str() {
                "io"   => { for f in &io_fns   { effects.insert(f.to_string()); } }
                "http" => { for f in &http_fns { effects.insert(f.to_string()); } }
                _ => {}
            }
        }
    }
    // build alias map from uses[] entries: io.read_file -> read_file
    let alias_table = crate::eval::std_aliases();
    let env = CheckEnv { effects };

    for f in &m.fns {
        check_fn(&env, &alias_table, f)?;
    }
    Ok(())
}

fn check_fn(env: &CheckEnv, alias_table: &std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>, f: &FnDecl) -> Result<()> {
    // expand uses[] aliases: io.read_file -> read_file
    let mut allowed: BTreeSet<String> = BTreeSet::new();
    for u in &f.uses {
        allowed.insert(u.clone());
        // try to resolve as alias.fn_name
        if let Some(dot) = u.find('.') {
            let mod_part = &u[..dot];
            let fn_part  = &u[dot+1..];
            if let Some(mod_map) = alias_table.get(mod_part) {
                if let Some(resolved) = mod_map.get(fn_part) {
                    allowed.insert(resolved.clone());
                }
            }
        }
    }
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

        Expr::RecordLit(fields) => {
            for (_k, v) in fields.iter() {
                check_expr(env, allowed, vars, v)?;
            }
            Ok(())
        }
        Expr::FieldGet { base, field: _field } => {
                check_expr(env, allowed, vars, base)?;
                Ok(())
            }
            Expr::Lambda { params, body } => {
                let mut child_vars = vars.clone();
                for p in params { child_vars.insert(p.clone(), ()); }
                check_block(env, allowed, &mut child_vars, body)?;
                Ok(())
            }
            Expr::TryExpr { inner } => return check_expr(env, allowed, vars, inner),
            Expr::CallExpr { f, args } => {
                check_expr(env, allowed, vars, f)?;
                for a in args { check_expr(env, allowed, vars, a)?; }
                Ok(())
            }
            Expr::Match { scrut, arms } => {
                check_expr(env, allowed, vars, scrut)?;
                for a in arms {
                    check_expr(env, allowed, vars, &a.body)?;
                }
                Ok(())
            }
        }
}
