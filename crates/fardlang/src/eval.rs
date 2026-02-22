use crate::ast::{Block, Expr, FnDecl, Stmt, Pattern};

#[derive(Debug, Clone)]
pub enum EvalVal {
    V(V),
    Closure { params: Vec<String>, body: Box<Block>, captured: Vec<(String, EvalVal)>, fns: std::collections::BTreeMap<String, FnDecl> },
}

impl EvalVal {
    pub fn into_v(self) -> Result<V> {
        match self {
            EvalVal::V(v) => Ok(v),
            EvalVal::Closure { .. } => Err(anyhow!("ERROR_EVAL closure is not a value")),
        }
    }
}
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use valuecore::v0::{canon_cmp, canon_eq, i64_add, i64_mul, i64_sub, V};

#[derive(Debug, Clone)]
pub struct Env {
    pub bindings: Vec<(String, EvalVal)>,
    pub fns: BTreeMap<String, FnDecl>,
    pub depth: usize,
    pub max_depth: usize,
}

impl Env {
    pub fn new() -> Self {
        Self {
            bindings: vec![],
            fns: BTreeMap::new(),
            depth: 0,
            max_depth: 1024,
        }
    }

    pub fn with_fns(fns: BTreeMap<String, FnDecl>) -> Self {
        Self {
            bindings: vec![],
            fns,
            depth: 0,
            max_depth: 1024,
        }
    }

    fn get_eval(&self, name: &str) -> Option<EvalVal> {
        for (k, v) in self.bindings.iter().rev() {
            if k == name {
                return Some(v.clone());
            }
        }
        None
    }

    fn get(&self, name: &str) -> Option<V> {
        match self.get_eval(name)? {
            EvalVal::V(v) => Some(v),
            _ => None,
        }
    }

    fn set(&mut self, name: String, v: V) {
        self.bindings.push((name, EvalVal::V(v)));
    }

    fn set_eval(&mut self, name: String, v: EvalVal) {
        self.bindings.push((name, v));
    }
}

pub fn eval_block(block: &Block, env: &mut Env) -> Result<V> {
    eval_block_inner(block, env)
}

fn eval_block_inner(block: &Block, env: &mut Env) -> Result<V> {
    for s in &block.stmts {
        match s {
            Stmt::Let { name, expr } => {
                let ev = eval_expr(expr, env)?;
                match ev {
                    EvalVal::Closure { .. } => env.set_eval(name.clone(), ev),
                    EvalVal::V(v) => env.set(name.clone(), v),
                }
            }
            Stmt::Expr(e) => {
                let _ = eval_expr(e, env)?;
            }
        }
    }

    match &block.tail {
        Some(t) => eval_expr(t, env)?.into_v(),
        None => Ok(V::Unit),
    }
}

pub fn eval_expr(expr: &Expr, env: &mut Env) -> Result<EvalVal> {
    match expr {
        Expr::Match { scrut, arms } => {
            let sv = eval_expr(scrut, env)?.into_v()?;
            for a in arms {
                if pat_matches(&a.pat, &sv) {
                    let mut child = env.clone();
                    for (k, vv) in pat_binds(&a.pat, &sv) {
                        child.set(k, vv);
                    }
                    return eval_expr(&a.body, &mut child);
                }
            }
            Err(anyhow!("ERROR_MATCH_NO_ARM"))
        }
        Expr::RecordLit(fs) => {
            let mut kvs: Vec<(String, V)> = vec![];
            for (k, v) in fs.iter() {
                let vv = eval_expr(v, env)?.into_v()?;
                kvs.push((k.clone(), vv));
            }
            Ok(EvalVal::V(valuecore::v0::normalize(&V::Map(kvs))))
        }
        Expr::FieldGet { base, field } => {
            let b = eval_expr(base, env)?.into_v()?;
            match b {
                V::Map(kvs) => {
                    let nb = valuecore::v0::normalize(&V::Map(kvs));
                    match nb {
                        V::Map(xs) => {
                            for (k, v) in xs {
                                if k == *field {
                                    return Ok(EvalVal::V(v));
                                }
                            }
                            Err(anyhow!("ERROR_OOB record missing field {}", field))
                        }
                        _ => unreachable!("normalize(Map) must return Map"),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG field access expects record")),
            }
        }
        // eval_operator_close_v1 begin
        Expr::UnaryMinus(x) => {
            let e2 = crate::desugar::desugar_expr(Expr::UnaryMinus(x.clone()));
            return eval_expr(&e2, env);
        }

        Expr::BinOp { op, lhs, rhs } => {
            let e2 = crate::desugar::desugar_expr(Expr::BinOp {
                op: op.clone(),
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            });
            return eval_expr(&e2, env);
        }
        // eval_operator_close_v1 end
        Expr::Unit => Ok(EvalVal::V(V::Unit)),
        Expr::Bool(b) => Ok(EvalVal::V(V::Bool(*b))),
        Expr::Int(s) => {
            let i = s
                .parse::<i64>()
                .map_err(|_| anyhow!("ERROR_BADARG int parse"))?;
            Ok(EvalVal::V(V::Int(i)))
        }
        Expr::Text(s) => Ok(EvalVal::V(V::Text(s.clone()))),
        Expr::BytesHex(h) => Ok(EvalVal::V(V::Bytes(decode_hex(h)?))),
        Expr::List(items) => {
            let mut vs = Vec::with_capacity(items.len());
            for it in items {
                vs.push(eval_expr(it, env)?.into_v()?);
            }
            Ok(EvalVal::V(V::List(vs)))
        }
        Expr::Ident(x) => {
            if let Some(ev) = env.get_eval(x) {
                Ok(ev)
            } else {
                Err(anyhow!("ERROR_EVAL unbound ident {}", x))
            }
        }

        Expr::If { c, t, e } => {
            let cv = eval_expr(c, env)?.into_v()?;
            match cv {
                V::Bool(true) => eval_block(t, env).map(EvalVal::V),
                V::Bool(false) => eval_block(e, env).map(EvalVal::V),
                _ => Err(anyhow!("ERROR_BADARG if condition must be bool")),
            }
        }

        Expr::Call { f, args } => {
            // builtins first
            if is_builtin(f) {
                let mut vs = Vec::with_capacity(args.len());
                for a in args {
                    vs.push(eval_expr(a, env)?.into_v()?);
                }
                return eval_builtin(f, &vs).map(EvalVal::V);
            }

            // check if name is a closure in bindings
            if let Some(EvalVal::Closure { params, body, captured, fns }) = env.get_eval(f) {
                if params.len() != args.len() {
                    return Err(anyhow!("ERROR_BADARG wrong arity for closure {}", f));
                }
                if env.depth >= env.max_depth {
                    return Err(anyhow!("ERROR_EVAL_DEPTH recursion limit exceeded"));
                }
                let mut child = Env::with_fns(fns);
                child.bindings = captured;
                child.depth = env.depth + 1;
                child.max_depth = env.max_depth;
                for (i, p) in params.iter().enumerate() {
                    let v = eval_expr(&args[i], env)?.into_v()?;
                    child.set(p.clone(), v);
                }
                return eval_block(&body, &mut child).map(EvalVal::V);
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

            if env.depth >= env.max_depth {
                return Err(anyhow!("ERROR_EVAL_DEPTH recursion limit exceeded"));
            }

            // no closures: new child env, only params + all fns (for recursion)
            let mut child = Env::with_fns(env.fns.clone());
            child.depth = env.depth + 1;
            child.max_depth = env.max_depth;

            for (i, param) in decl.params.iter().enumerate() {
                let name = param.0.clone(); // (String, Type)
                let v = eval_expr(&args[i], env)?.into_v()?;
                child.set(name, v);
            }

            eval_block(&decl.body, &mut child).map(EvalVal::V)
        }

        Expr::Lambda { params, body } => {
            Ok(EvalVal::Closure {
                params: params.clone(),
                body: body.clone(),
                captured: env.bindings.clone(),
                fns: env.fns.clone(),
            })
        }

        Expr::CallExpr { f, args } => {
            // Evaluate f to get a closure from env
            if let Expr::Ident(name) = f.as_ref() {
                if let Some(EvalVal::Closure { params, body, captured, fns }) = env.get_eval(name) {
                    if params.len() != args.len() {
                        return Err(anyhow!("ERROR_BADARG wrong arity for closure"));
                    }
                    if env.depth >= env.max_depth {
                        return Err(anyhow!("ERROR_EVAL_DEPTH recursion limit exceeded"));
                    }
                    let mut child = Env::with_fns(fns);
                    child.bindings = captured;
                    child.depth = env.depth + 1;
                    child.max_depth = env.max_depth;
                    for (i, p) in params.iter().enumerate() {
                        let v = eval_expr(&args[i], env)?.into_v()?;
                        child.set(p.clone(), v);
                    }
                    return eval_block(&body, &mut child).map(EvalVal::V);
                }
            }
            Err(anyhow!("ERROR_EVAL not a closure"))
        }
    }
}


fn pat_matches(p: &Pattern, v: &V) -> bool {
    match p {
        Pattern::Wild => true,
        Pattern::Unit => matches!(v, V::Unit),
        Pattern::Bool(b) => matches!(v, V::Bool(x) if x == b),
        Pattern::Int(z) => {
            if let Ok(i) = z.parse::<i64>() {
                matches!(v, V::Int(x) if *x == i)
            } else {
                false
            }
        }
        Pattern::Text(s) => matches!(v, V::Text(x) if x == s),
        Pattern::BytesHex(h) => {
            if let Ok(bs) = decode_hex(h) {
                matches!(v, V::Bytes(x) if *x == bs)
            } else {
                false
            }
        }
        Pattern::List(pats) => {
            if let V::List(vs) = v {
                pats.len() == vs.len() && pats.iter().zip(vs.iter()).all(|(p, v)| pat_matches(p, v))
            } else {
                false
            }
        }
        Pattern::Ident(_) => true,
    }
}

fn pat_binds(p: &Pattern, v: &V) -> Vec<(String, V)> {
    match p {
        Pattern::Ident(name) if name != "_" => vec![(name.clone(), v.clone())],
        Pattern::List(pats) => {
            if let V::List(vs) = v {
                pats.iter().zip(vs.iter()).flat_map(|(p, v)| pat_binds(p, v)).collect()
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

fn is_builtin(f: &str) -> bool {
    matches!(
        f,
        "add"
            | "sub"
            | "mul"
            | "div"
            | "rem"
            | "neg"
            | "eq"
            | "lt"
            | "not"
            | "list_len"
            | "list_get"
            | "text_concat"
            | "map_get"
            | "int_to_text"
            | "text_len"
            | "text_contains"
            | "text_starts_with"
            | "text_split"
            | "text_trim"
            | "text_slice"
            | "text_replace"
            | "text_join"
            | "list_append"
            | "list_concat"
            | "list_reverse"
            | "list_contains"
            | "list_slice"
    )
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
        "list_len" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG list_len arity"));
            }
            match &args[0] {
                V::List(xs) => Ok(V::Int(xs.len() as i64)),
                _ => Err(anyhow!("ERROR_BADARG list_len expects list")),
            }
        }
        "list_get" => {
            if args.len() != 2 {
                return Err(anyhow!("ERROR_BADARG list_get arity"));
            }
            let idx = match &args[1] {
                V::Int(i) => *i,
                _ => return Err(anyhow!("ERROR_BADARG list_get expects int index")),
            };
            if idx < 0 {
                return Err(anyhow!("ERROR_OOB list_get"));
            }
            let u = idx as usize;
            match &args[0] {
                V::List(xs) => xs
                    .get(u)
                    .cloned()
                    .ok_or_else(|| anyhow!("ERROR_OOB list_get")),
                _ => Err(anyhow!("ERROR_BADARG list_get expects list")),
            }
        }
        "text_concat" => {
            if args.len() != 2 {
                return Err(anyhow!("ERROR_BADARG text_concat arity"));
            }
            match (&args[0], &args[1]) {
                (V::Text(a), V::Text(b)) => {
                    let mut out = String::with_capacity(a.len() + b.len());
                    out.push_str(a);
                    out.push_str(b);
                    Ok(V::Text(out))
                }
                _ => Err(anyhow!("ERROR_BADARG text_concat expects text")),
            }
        }
        "int_to_text" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG int_to_text arity"));
            }
            match &args[0] {
                V::Int(i) => Ok(V::Text(i.to_string())),
                _ => Err(anyhow!("ERROR_BADARG int_to_text expects int")),
            }
        }
        "text_len" => {
            match args.get(0) {
                Some(V::Text(s)) => Ok(V::Int(s.chars().count() as i64)),
                _ => Err(anyhow!("ERROR_BADARG text_len expects text")),
            }
        }
        "text_contains" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.contains(pat.as_str()))),
                _ => Err(anyhow!("ERROR_BADARG text_contains expects (text, text)")),
            }
        }
        "text_starts_with" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.starts_with(pat.as_str()))),
                _ => Err(anyhow!("ERROR_BADARG text_starts_with expects (text, text)")),
            }
        }
        "text_split" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(sep))) => {
                    let parts: Vec<V> = s.split(sep.as_str()).map(|p| V::Text(p.to_string())).collect();
                    Ok(V::List(parts))
                }
                _ => Err(anyhow!("ERROR_BADARG text_split expects (text, text)")),
            }
        }
        "text_trim" => {
            match args.get(0) {
                Some(V::Text(s)) => Ok(V::Text(s.trim().to_string())),
                _ => Err(anyhow!("ERROR_BADARG text_trim expects text")),
            }
        }
        "text_slice" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::Text(s)), Some(V::Int(start)), Some(V::Int(end))) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let s = (*start).max(0).min(len) as usize;
                    let e = (*end).max(0).min(len) as usize;
                    let e = e.max(s);
                    Ok(V::Text(chars[s..e].iter().collect()))
                }
                _ => Err(anyhow!("ERROR_BADARG text_slice expects (text, int, int)")),
            }
        }
        "text_replace" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::Text(s)), Some(V::Text(from)), Some(V::Text(to))) => {
                    Ok(V::Text(s.replace(from.as_str(), to.as_str())))
                }
                _ => Err(anyhow!("ERROR_BADARG text_replace expects (text, text, text)")),
            }
        }
        "text_join" => {
            match (args.get(0), args.get(1)) {
                (Some(V::List(items)), Some(V::Text(sep))) => {
                    let mut parts = Vec::new();
                    for item in items {
                        match item {
                            V::Text(t) => parts.push(t.clone()),
                            _ => return Err(anyhow!("ERROR_BADARG text_join list must contain text")),
                        }
                    }
                    Ok(V::Text(parts.join(sep.as_str())))
                }
                _ => Err(anyhow!("ERROR_BADARG text_join expects (list, text)")),
            }
        }
        "list_append" => {
            match (args.get(0), args.get(1)) {
                (Some(V::List(xs)), Some(v)) => {
                    let mut out = xs.clone();
                    out.push(v.clone());
                    Ok(V::List(out))
                }
                _ => Err(anyhow!("ERROR_BADARG list_append expects (list, value)")),
            }
        }
        "list_concat" => {
            match (args.get(0), args.get(1)) {
                (Some(V::List(a)), Some(V::List(b))) => {
                    let mut out = a.clone();
                    out.extend(b.iter().cloned());
                    Ok(V::List(out))
                }
                _ => Err(anyhow!("ERROR_BADARG list_concat expects (list, list)")),
            }
        }
        "list_reverse" => {
            match args.get(0) {
                Some(V::List(xs)) => {
                    let mut out = xs.clone();
                    out.reverse();
                    Ok(V::List(out))
                }
                _ => Err(anyhow!("ERROR_BADARG list_reverse expects list")),
            }
        }
        "list_contains" => {
            match (args.get(0), args.get(1)) {
                (Some(V::List(xs)), Some(v)) => {
                    Ok(V::Bool(xs.iter().any(|x| canon_eq(x, v))))
                }
                _ => Err(anyhow!("ERROR_BADARG list_contains expects (list, value)")),
            }
        }
        "list_slice" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::List(xs)), Some(V::Int(start)), Some(V::Int(end))) => {
                    let len = xs.len() as i64;
                    let s = (*start).max(0).min(len) as usize;
                    let e = (*end).max(0).min(len) as usize;
                    let e = e.max(s);
                    Ok(V::List(xs[s..e].to_vec()))
                }
                _ => Err(anyhow!("ERROR_BADARG list_slice expects (list, int, int)")),
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
    a.checked_neg()
        .ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_neg"))
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let rest = if let Some(r) = s.strip_prefix("hex:") {
        r
    } else {
        s
    };
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
