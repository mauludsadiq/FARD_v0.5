use crate::ast::{Block, Expr, FnDecl, Stmt};
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use valuecore::v0::{canon_cmp, canon_eq, i64_add, i64_mul, i64_sub, V};

#[derive(Debug, Clone)]
pub struct Env {
    pub bindings: Vec<(String, V)>,
    pub fns: BTreeMap<String, FnDecl>,
}

impl Env {
    pub fn new() -> Self {
        Self { bindings: vec![], fns: BTreeMap::new() }
    }

    pub fn with_fns(fns: BTreeMap<String, FnDecl>) -> Self {
        Self { bindings: vec![], fns }
    }

    fn get(&self, name: &str) -> Option<V> {
        for (k, v) in self.bindings.iter().rev() {
            if k == name {
                return Some(v.clone());
            }
        }
        None
    }

    fn set(&mut self, name: String, v: V) {
        self.bindings.push((name, v));
    }
}

pub fn eval_block(block: &Block, env: &mut Env) -> Result<V> {
    for s in &block.stmts {
        match s {
            Stmt::Let { name, expr } => {
                let v = eval_expr(expr, env)?;
                env.set(name.clone(), v);
            }
            Stmt::Expr(e) => {
                let _ = eval_expr(e, env)?;
            }
        }
    }

    match &block.tail {
        Some(t) => eval_expr(t, env),
        None => Ok(V::Unit),
    }
}

pub fn eval_expr(expr: &Expr, env: &mut Env) -> Result<V> {
    match expr {
        Expr::Unit => Ok(V::Unit),
        Expr::Bool(b) => Ok(V::Bool(*b)),
        Expr::Int(s) => {
            let i = s.parse::<i64>().map_err(|_| anyhow!("ERROR_BADARG int parse"))?;
            Ok(V::Int(i))
        }
        Expr::Text(s) => Ok(V::Text(s.clone())),
        Expr::BytesHex(h) => Ok(V::Bytes(decode_hex(h)?)),
        Expr::Ident(x) => env
            .get(x)
            .ok_or_else(|| anyhow!("ERROR_EVAL unbound ident {}", x)),

        Expr::If { c, t, e } => {
            let cv = eval_expr(c, env)?;
            match cv {
                V::Bool(true) => eval_block(t, env),
                V::Bool(false) => eval_block(e, env),
                _ => Err(anyhow!("ERROR_BADARG if condition must be bool")),
            }
        }

        Expr::Call { f, args } => {
            // builtins first
            if is_builtin(f) {
                let mut vs = Vec::with_capacity(args.len());
                for a in args {
                    vs.push(eval_expr(a, env)?);
                }
                return eval_builtin(f, &vs);
            }

            // user fn call
            let decl = env
                .fns
                .get(f)
                .cloned()
                .ok_or_else(|| anyhow!("ERROR_EVAL unknown function {}", f))?;

            if decl.params.len() != args.len() {
                return Err(anyhow!("ERROR_BADARG wrong arity for {}", f));
            }

            // no closures: new child env, only params + all fns (for recursion)
            let mut child = Env::with_fns(env.fns.clone());

            for (i, param) in decl.params.iter().enumerate() {
                let name = param.0.clone(); // (String, Type)
                let v = eval_expr(&args[i], env)?;
                child.set(name, v);
            }

            eval_block(&decl.body, &mut child)
        }
    }
}

fn is_builtin(f: &str) -> bool {
    matches!(f, "add" | "sub" | "mul" | "div" | "rem" | "neg" | "eq" | "lt" | "not")
}

fn eval_builtin(f: &str, args: &[V]) -> Result<V> {
    match f {
        "add" => {
            let (a, b) = expect_i64_2(args)?;
            Ok(V::Int(i64_add(a, b)?))
        }
        "sub" => {
            let (a, b) = expect_i64_2(args)?;
            Ok(V::Int(i64_sub(a, b)?))
        }
        "mul" => {
            let (a, b) = expect_i64_2(args)?;
            Ok(V::Int(i64_mul(a, b)?))
        }
        "div" => {
            let (a, b) = expect_i64_2(args)?;
            Ok(V::Int(i64_div(a, b)?))
        }
        "rem" => {
            let (a, b) = expect_i64_2(args)?;
            Ok(V::Int(i64_rem(a, b)?))
        }
        "neg" => {
            let a = expect_i64_1(args)?;
            Ok(V::Int(i64_neg(a)?))
        }
        "eq" => {
            if args.len() != 2 {
                return Err(anyhow!("ERROR_BADARG eq arity"));
            }
            Ok(V::Bool(canon_eq(&args[0], &args[1])))
        }
        "lt" => {
            if args.len() != 2 {
                return Err(anyhow!("ERROR_BADARG lt arity"));
            }
            Ok(V::Bool(canon_cmp(&args[0], &args[1]).is_lt()))
        }
        "not" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG not arity"));
            }
            match &args[0] {
                V::Bool(b) => Ok(V::Bool(!*b)),
                _ => Err(anyhow!("ERROR_BADARG not expects bool")),
            }
        }
        _ => Err(anyhow!("ERROR_EVAL unknown builtin {}", f)),
    }
}

fn expect_i64_1(args: &[V]) -> Result<i64> {
    if args.len() != 1 {
        return Err(anyhow!("ERROR_BADARG arity"));
    }
    match args[0] {
        V::Int(i) => Ok(i),
        _ => Err(anyhow!("ERROR_BADARG expected int")),
    }
}

fn expect_i64_2(args: &[V]) -> Result<(i64, i64)> {
    if args.len() != 2 {
        return Err(anyhow!("ERROR_BADARG arity"));
    }
    let a = match args[0] {
        V::Int(i) => i,
        _ => return Err(anyhow!("ERROR_BADARG expected int")),
    };
    let b = match args[1] {
        V::Int(i) => i,
        _ => return Err(anyhow!("ERROR_BADARG expected int")),
    };
    Ok((a, b))
}

// Match Gate8 semantics: div by zero => ERROR_DIV_ZERO; MIN / -1 => ERROR_OVERFLOW; trunc toward zero.
fn i64_div(a: i64, b: i64) -> Result<i64> {
    if b == 0 {
        return Err(anyhow!("ERROR_DIV_ZERO i64_div"));
    }
    if a == i64::MIN && b == -1 {
        return Err(anyhow!("ERROR_OVERFLOW i64_div"));
    }
    Ok(a / b)
}

fn i64_rem(a: i64, b: i64) -> Result<i64> {
    if b == 0 {
        return Err(anyhow!("ERROR_DIV_ZERO i64_rem"));
    }
    if a == i64::MIN && b == -1 {
        return Err(anyhow!("ERROR_OVERFLOW i64_rem"));
    }
    Ok(a % b)
}

fn i64_neg(a: i64) -> Result<i64> {
    a.checked_neg().ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_neg"))
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let rest = if let Some(r) = s.strip_prefix("hex:") { r } else { s };
    if rest.len() % 2 != 0 {
        return Err(anyhow!("ERROR_BADARG hex length must be even"));
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);

    let to_n = |c: u8| -> Result<u8> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(anyhow!("ERROR_BADARG invalid hex char")),
        }
    };

    let mut i = 0usize;
    while i < bytes.len() {
        let hi = to_n(bytes[i])?;
        let lo = to_n(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}
