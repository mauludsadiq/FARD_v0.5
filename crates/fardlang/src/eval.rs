use crate::ast::{Block, Expr, FnDecl, Stmt, Pattern};

// Sentinel used to propagate err values through ? without unwinding past block boundaries
#[derive(Debug)]
struct TryPropagation(V);
impl std::fmt::Display for TryPropagation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "TryPropagation") }
}
impl std::error::Error for TryPropagation {}
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Sha256, Digest};
use hkdf::Hkdf;
use chacha20poly1305::{XChaCha20Poly1305, KeyInit, aead::{Aead, Payload}};
use p256::ecdsa::{VerifyingKey, signature::Verifier, DerSignature};

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

// Thread-local effect handler pointer — set by fardcli before eval, cleared after
use std::cell::RefCell;
thread_local! {
    static EFFECT_HANDLER: RefCell<Option<*mut dyn crate::effects::EffectHandler>> = RefCell::new(None);
}

pub fn with_effect_handler<H: crate::effects::EffectHandler + 'static>(handler: &mut H, f: impl FnOnce()) {
    let ptr = handler as *mut dyn crate::effects::EffectHandler;
    EFFECT_HANDLER.with(|cell| *cell.borrow_mut() = Some(ptr));
    f();
    EFFECT_HANDLER.with(|cell| *cell.borrow_mut() = None);
}
use std::collections::{BTreeMap, BTreeSet};
use valuecore::v0::{canon_cmp, canon_eq, i64_add, i64_mul, i64_sub, V};

#[derive(Debug, Clone)]
pub struct Env {
    pub bindings: Vec<(String, EvalVal)>,
    pub fns: BTreeMap<String, FnDecl>,
    pub aliases: BTreeMap<String, String>, // "list.len" -> "list_len"
    pub declared_effects: BTreeSet<String>,
    pub depth: usize,
    pub max_depth: usize,
}

impl Env {
    pub fn new() -> Self {
        Self {
            bindings: vec![],
            fns: BTreeMap::new(),
            aliases: BTreeMap::new(),
            declared_effects: BTreeSet::new(),
            depth: 0,
            max_depth: 200,
        }
    }

    pub fn with_fns(fns: BTreeMap<String, FnDecl>) -> Self {
        Self {
            bindings: vec![],
            fns,
            aliases: BTreeMap::new(),
            declared_effects: BTreeSet::new(),
            depth: 0,
            max_depth: 200,
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

// Static std namespace table
pub fn std_aliases() -> BTreeMap<String, BTreeMap<String, String>> {
    let mut m: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let modules: &[(&str, &[&str])] = &[
        ("list", &["len","append","concat","reverse","contains","slice"]),
        ("text", &["len","contains","starts_with","split","trim","slice","replace","join"]),
        ("bytes", &["len","concat","slice","eq","from_text"]),
        ("map",   &["new","set","get","has","keys","delete"]),
        ("crypto",&["sha256","hkdf_sha256","xchacha20poly1305_seal","xchacha20poly1305_open","rsa_verify_pkcs1_sha256","ecdsa_p256_verify"]),
        ("encode",&["base64url_encode","base64url_decode","json_parse","json_emit"]),
        ("result",&["ok","err"]),
        ("io",    &["read_file","write_file","clock_now","random_bytes"]),
        ("http",  &["http_get"]),
    ];
    for (mod_name, fns) in modules {
        let mut mod_map = BTreeMap::new();
        for fn_name in *fns {
            // list.len -> list_len, encode.json_parse -> json_parse (already prefixed)
            let builtin = match *mod_name {
                "encode" | "crypto" | "result" | "io" | "http" => fn_name.to_string(),
                _ => format!("{}_{}", mod_name, fn_name),
            };
            mod_map.insert(fn_name.to_string(), builtin);
        }
        m.insert(mod_name.to_string(), mod_map);
    }
    m
}

pub fn apply_imports(env: &mut Env, imports: &[crate::ast::ImportDecl]) {
    let table = std_aliases();
    for imp in imports {
        let parts = &imp.path.0;
        // expect path like ["std", "list"] or ["std", "text"]
        if parts.len() == 2 && parts[0] == "std" {
            let mod_name = &parts[1];
            let alias = imp.alias.as_deref().unwrap_or(mod_name.as_str());
            if let Some(mod_map) = table.get(mod_name.as_str()) {
                for (fn_name, builtin) in mod_map {
                    // alias.fn_name -> builtin
                    env.aliases.insert(format!("{}.{}", alias, fn_name), builtin.clone());
                }
            }
        }
    }
    // auto-declare io/http effects into env.declared_effects
    let effect_mods = [
        ("io",   &["read_file","write_file","clock_now","random_bytes"] as &[&str]),
        ("http", &["http_get"]),
    ];
    for imp in imports {
        let parts = &imp.path.0;
        if parts.len() == 2 && parts[0] == "std" {
            let mod_name = parts[1].as_str();
            for (em, fns) in &effect_mods {
                if mod_name == *em {
                    for fn_name in *fns {
                        env.declared_effects.insert(fn_name.to_string());
                    }
                }
            }
        }
    }
}

pub fn eval_block(block: &Block, env: &mut Env) -> Result<V> {
    eval_block_inner(block, env)
}

fn eval_block_inner(block: &Block, env: &mut Env) -> Result<V> {
    match eval_block_inner_fallible(block, env) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(tp) = e.downcast_ref::<TryPropagation>() {
                Ok(tp.0.clone())
            } else {
                Err(e)
            }
        }
    }
}

fn eval_block_inner_fallible(block: &Block, env: &mut Env) -> Result<V> {
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
            Ok(EvalVal::V(V::Err("ERROR_MATCH_NO_ARM".into())))
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
                            Ok(EvalVal::V(V::Err(format!("ERROR_OOB record missing field {}", field))))
                        }
                        _ => unreachable!("normalize(Map) must return Map"),
                    }
                }
                _ => Ok(EvalVal::V(V::Err("ERROR_BADARG field access expects record".into()))),
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
            // resolve namespace alias: list.len -> list_ln
            // also handle method-call desugaring: f="ok", args=[Ident("result"), ...] -> f="result.ok"
            // reconstruct namespace: result.ok(42) -> f="ok", args=[Ident("result"),42] -> "result.ok"
            let f_reconstructed;
            let args_tail;
            let (f, args): (&str, &[Expr]) = if let Some(Expr::Ident(ns)) = args.first() {
                let candidate = format!("{}.{}", ns, f);
                if env.aliases.contains_key(candidate.as_str()) {
                    // builtin alias: list.len -> list_len
                    f_reconstructed = candidate;
                    args_tail = &args[1..];
                    (f_reconstructed.as_str(), args_tail)
                } else if env.fns.contains_key(candidate.as_str()) {
                    // pure-FARD stdlib fn: list.map, list.filter etc.
                    f_reconstructed = candidate;
                    args_tail = &args[1..];
                    (f_reconstructed.as_str(), args_tail)
                } else {
                    (f.as_str(), args.as_slice())
                }
            } else {
                (f.as_str(), args.as_slice())
            };
            let f_resolved;
            let f: &str = if let Some(resolved) = env.aliases.get(f) {
                f_resolved = resolved.clone();
                f_resolved.as_str()
            } else {
                f
            };
            // effect dispatch — before builtins
            if env.declared_effects.contains(f) {
                let mut vs = Vec::with_capacity(args.len());
                for a in args {
                    vs.push(eval_expr(a, env)?.into_v()?);
                }
                let result = EFFECT_HANDLER.with(|cell| -> Result<V> {
                    let opt = *cell.borrow();
                    let ptr = opt.ok_or_else(|| anyhow::anyhow!("ERROR_EFFECT no handler"))?;
                    unsafe { (*ptr).call(f, &vs) }
                });
                return Ok(EvalVal::V(match result {
                    Ok(v) => v,
                    Err(e) => V::Err(e.to_string()),
                }));
            }

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
                    return Ok(EvalVal::V(V::Err("ERROR_EVAL_DEPTH recursion limit exceeded".into())));
                }
                let mut child = Env::with_fns(fns);
                child.bindings = captured;
                child.depth = env.depth + 1;
                child.max_depth = env.max_depth;
                for (i, p) in params.iter().enumerate() {
                    let ev = eval_expr(&args[i], env)?;
                    child.set_eval(p.clone(), ev);
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
                return Ok(EvalVal::V(V::Err("ERROR_EVAL_DEPTH recursion limit exceeded".into())));
            }

            // no closures: new child env, only params + all fns (for recursion)
            let mut child = Env::with_fns(env.fns.clone());
            child.depth = env.depth + 1;
            child.max_depth = env.max_depth;

            for (i, param) in decl.params.iter().enumerate() {
                let name = param.0.clone(); // (String, Type)
                let ev = eval_expr(&args[i], env)?;
                child.set_eval(name, ev);
            }

            eval_block(&decl.body, &mut child).map(EvalVal::V)
        }

        Expr::TryExpr { inner } => {
            let v = eval_expr(inner, env)?;
            match v {
                // err value: propagate up via sentinel
                EvalVal::V(V::Err(ref e)) => {
                    return Err(anyhow::Error::new(TryPropagation(V::Err(e.clone()))));
                }
                // {tag:"ok", val:v} record: unwrap to v
                EvalVal::V(V::Map(ref kvs)) => {
                    let tag = kvs.iter().find(|(k, _)| k == "tag").map(|(_, v)| v.clone());
                    let val = kvs.iter().find(|(k, _)| k == "val").map(|(_, v)| v.clone());
                    match (tag, val) {
                        (Some(V::Text(ref t)), Some(ref v2)) if t == "ok" => Ok(EvalVal::V(v2.clone())),
                        _ => Ok(v),
                    }
                }
                other => Ok(other),
            }
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
                        return Ok(EvalVal::V(V::Err("ERROR_EVAL_DEPTH recursion limit exceeded".into())));
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
            | "gt"
            | "le"
            | "ge"
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
            | "bytes_len"
            | "bytes_concat"
            | "bytes_slice"
            | "bytes_eq"
            | "bytes_from_text"
            | "map_new"
            | "map_set"
            | "map_has"
            | "map_keys"
            | "map_delete"
            | "base64url_encode"
            | "base64url_decode"
            | "json_parse"
            | "json_emit"
            | "sha256"
            | "hkdf_sha256"
            | "xchacha20poly1305_seal"
            | "xchacha20poly1305_open"
            | "rsa_verify_pkcs1_sha256"
            | "ecdsa_p256_verify"
            | "ok"
            | "err"
    )
}

fn eval_builtin(f: &str, args: &[V]) -> Result<V> {
    match f {
        "add" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_add(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
        }
        "sub" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_sub(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
        }
        "mul" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_mul(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
        }
        "div" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_div(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
        }
        "rem" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_rem(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
        }
        "neg" => {
            let a = match expect_i64_1(args) { Ok(v) => v, Err(e) => return Ok(V::Err(e.to_string())) };
            match i64_neg(a) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::Err(e.to_string())) }
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
        "gt" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Bool(a > b)),
                _ => Ok(V::Err("ERROR_BADARG gt expects (int, int)".into())),
            }
        }
        "le" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Bool(a <= b)),
                _ => Ok(V::Err("ERROR_BADARG le expects (int, int)".into())),
            }
        }
        "ge" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Bool(a >= b)),
                _ => Ok(V::Err("ERROR_BADARG ge expects (int, int)".into())),
            }
        }
        "not" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG not arity"));
            }
            match &args[0] {
                V::Bool(b) => Ok(V::Bool(!*b)),
                _ => Ok(V::Err("ERROR_BADARG not expects bool".into())),
            }
        }
        "list_len" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG list_len arity"));
            }
            match &args[0] {
                V::List(xs) => Ok(V::Int(xs.len() as i64)),
                _ => Ok(V::Err("ERROR_BADARG list_len expects list".into())),
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
                return Ok(V::Err("ERROR_OOB list_get".into()));
            }
            let u = idx as usize;
            match &args[0] {
                V::List(xs) => Ok(xs
                    .get(u)
                    .cloned()
                    .unwrap_or_else(|| V::Err("ERROR_OOB list_get".into()))),
                _ => Ok(V::Err("ERROR_BADARG list_get expects list".into())),
            }
        }
        "map_get" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Map(kvs)), Some(V::Text(k))) => {
                    let norm = valuecore::v0::normalize(&V::Map(kvs.clone()));
                    if let V::Map(nkvs) = norm {
                        let found = nkvs.into_iter().find(|(ek, _)| ek == k).map(|(_, v)| v);
                        Ok(found.unwrap_or_else(|| V::Err(format!("ERROR_OOB map_get missing key {}", k))))
                    } else {
                        Ok(V::Err("ERROR_BADARG map_get expects record".into()))
                    }
                }
                _ => Ok(V::Err("ERROR_BADARG map_get expects (record, text)".into())),
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
                _ => Ok(V::Err("ERROR_BADARG text_concat expects text".into())),
            }
        }
        "int_to_text" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG int_to_text arity"));
            }
            match &args[0] {
                V::Int(i) => Ok(V::Text(i.to_string())),
                _ => Ok(V::Err("ERROR_BADARG int_to_text expects int".into())),
            }
        }
        "text_len" => {
            match args.get(0) {
                Some(V::Text(s)) => Ok(V::Int(s.chars().count() as i64)),
                _ => Ok(V::Err("ERROR_BADARG text_len expects text".into())),
            }
        }
        "text_contains" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.contains(pat.as_str()))),
                _ => Ok(V::Err("ERROR_BADARG text_contains expects (text, text)".into())),
            }
        }
        "text_starts_with" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.starts_with(pat.as_str()))),
                _ => Ok(V::Err("ERROR_BADARG text_starts_with expects (text, text)".into())),
            }
        }
        "text_split" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(sep))) => {
                    let parts: Vec<V> = s.split(sep.as_str()).map(|p| V::Text(p.to_string())).collect();
                    Ok(V::List(parts))
                }
                _ => Ok(V::Err("ERROR_BADARG text_split expects (text, text)".into())),
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
        "bytes_len" => {
            match args.get(0) {
                Some(V::Bytes(b)) => Ok(V::Int(b.len() as i64)),
                _ => Err(anyhow!("ERROR_BADARG bytes_len expects bytes")),
            }
        }
        "bytes_concat" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Bytes(a)), Some(V::Bytes(b))) => {
                    let mut out = a.clone();
                    out.extend_from_slice(b);
                    Ok(V::Bytes(out))
                }
                _ => Err(anyhow!("ERROR_BADARG bytes_concat expects (bytes, bytes)")),
            }
        }
        "bytes_slice" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::Bytes(b)), Some(V::Int(start)), Some(V::Int(end))) => {
                    let len = b.len() as i64;
                    let s = (*start).max(0).min(len) as usize;
                    let e = (*end).max(0).min(len) as usize;
                    let e = e.max(s);
                    Ok(V::Bytes(b[s..e].to_vec()))
                }
                _ => Err(anyhow!("ERROR_BADARG bytes_slice expects (bytes, int, int)")),
            }
        }
        "bytes_eq" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Bytes(a)), Some(V::Bytes(b))) => Ok(V::Bool(a == b)),
                _ => Err(anyhow!("ERROR_BADARG bytes_eq expects (bytes, bytes)")),
            }
        }
        "bytes_from_text" => {
            match args.get(0) {
                Some(V::Text(s)) => Ok(V::Bytes(s.as_bytes().to_vec())),
                _ => Err(anyhow!("ERROR_BADARG bytes_from_text expects text")),
            }
        }
        "map_new" => {
            Ok(valuecore::v0::normalize(&V::Map(vec![])))
        }
        "map_set" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::Map(kvs)), Some(V::Text(k)), Some(v)) => {
                    let mut out = kvs.clone();
                    out.retain(|(ek, _)| ek != k);
                    out.push((k.clone(), v.clone()));
                    Ok(valuecore::v0::normalize(&V::Map(out)))
                }
                _ => Err(anyhow!("ERROR_BADARG map_set expects (record, text, value)")),
            }
        }
        "map_has" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Map(kvs)), Some(V::Text(k))) => {
                    let norm = valuecore::v0::normalize(&V::Map(kvs.clone()));
                    if let V::Map(nkvs) = norm {
                        Ok(V::Bool(nkvs.iter().any(|(ek, _)| ek == k)))
                    } else {
                        Ok(V::Bool(false))
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG map_has expects (record, text)")),
            }
        }
        "map_keys" => {
            match args.get(0) {
                Some(V::Map(kvs)) => {
                    let norm = valuecore::v0::normalize(&V::Map(kvs.clone()));
                    if let V::Map(nkvs) = norm {
                        Ok(V::List(nkvs.into_iter().map(|(k, _)| V::Text(k)).collect()))
                    } else {
                        Ok(V::List(vec![]))
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG map_keys expects record")),
            }
        }
        "map_delete" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Map(kvs)), Some(V::Text(k))) => {
                    let mut out = kvs.clone();
                    out.retain(|(ek, _)| ek != k);
                    Ok(valuecore::v0::normalize(&V::Map(out)))
                }
                _ => Err(anyhow!("ERROR_BADARG map_delete expects (record, text)")),
            }
        }
        "base64url_encode" => {
            match args.get(0) {
                Some(V::Bytes(b)) => Ok(V::Text(URL_SAFE_NO_PAD.encode(b))),
                _ => Err(anyhow!("ERROR_BADARG base64url_encode expects bytes")),
            }
        }
        "base64url_decode" => {
            match args.get(0) {
                Some(V::Text(s)) => {
                    match URL_SAFE_NO_PAD.decode(s.as_bytes()) {
                        Ok(b) => Ok(V::Bytes(b)),
                        Err(e) => Ok(V::Err(format!("ERROR_BADARG base64url_decode: {}", e))),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG base64url_decode expects text")),
            }
        }
        "json_parse" => {
            match args.get(0) {
                Some(V::Text(s)) => {
                    match serde_json::from_str::<serde_json::Value>(s) {
                        Ok(jv) => json_to_v(&jv),
                        Err(e) => Ok(V::Err(format!("ERROR_BADARG json_parse: {}", e))),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG json_parse expects text")),
            }
        }
        "json_emit" => {
            match args.get(0) {
                Some(v) => {
                    match v_to_json(v).and_then(|jv| serde_json::to_string(&jv).map_err(|e| anyhow!("json_emit: {}", e))) {
                        Ok(s) => Ok(V::Text(s)),
                        Err(e) => Ok(V::Err(format!("ERROR_EVAL json_emit: {}", e))),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG json_emit expects value")),
            }
        }
        "sha256" => {
            match args.get(0) {
                Some(V::Bytes(b)) => {
                    let hash = Sha256::digest(b);
                    Ok(V::Bytes(hash.to_vec()))
                }
                _ => Err(anyhow!("ERROR_BADARG sha256 expects bytes")),
            }
        }
        "hkdf_sha256" => {
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(ikm)), Some(V::Bytes(salt)), Some(V::Bytes(info)), Some(V::Int(len))) => {
                    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
                    let mut out = vec![0u8; *len as usize];
                    match hk.expand(info, &mut out) {
                        Ok(_) => Ok(V::Bytes(out)),
                        Err(_) => Ok(V::Err("ERROR_BADARG hkdf_sha256: invalid length".into())),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG hkdf_sha256 expects (bytes, bytes, bytes, int)")),
            }
        }
        "xchacha20poly1305_seal" => {
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(key)), Some(V::Bytes(nonce)), Some(V::Bytes(aad)), Some(V::Bytes(pt))) => {
                    use chacha20poly1305::XNonce;
                    match XChaCha20Poly1305::new_from_slice(key) {
                        Err(_) => Ok(V::Err("ERROR_BADARG xchacha20poly1305_seal: key must be 32 bytes".into())),
                        Ok(cipher) => {
                            let n = XNonce::from_slice(nonce);
                            match cipher.encrypt(n, Payload { msg: pt, aad }) {
                                Ok(ct) => Ok(V::Bytes(ct)),
                                Err(_) => Ok(V::Err("ERROR_EVAL xchacha20poly1305_seal: encryption failed".into())),
                            }
                        }
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG xchacha20poly1305_seal expects (bytes, bytes, bytes, bytes)")),
            }
        }
        "xchacha20poly1305_open" => {
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(key)), Some(V::Bytes(nonce)), Some(V::Bytes(aad)), Some(V::Bytes(ct))) => {
                    use chacha20poly1305::XNonce;
                    match XChaCha20Poly1305::new_from_slice(key) {
                        Err(_) => Ok(V::Err("ERROR_BADARG xchacha20poly1305_open: key must be 32 bytes".into())),
                        Ok(cipher) => {
                            let n = XNonce::from_slice(nonce);
                            match cipher.decrypt(n, Payload { msg: ct, aad }) {
                                Ok(pt) => Ok(V::Bytes(pt)),
                                Err(_) => Ok(V::Err("ERROR_EVAL xchacha20poly1305_open: decryption failed".into())),
                            }
                        }
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG xchacha20poly1305_open expects (bytes, bytes, bytes, bytes)")),
            }
        }
        "rsa_verify_pkcs1_sha256" => {
            use rsa::{RsaPublicKey, pkcs1v15::{VerifyingKey as RsaVerifyingKey, Signature as RsaSig}, signature::Verifier as RsaVerifier};
            use rsa::BigUint;
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(msg)), Some(V::Bytes(sig)), Some(V::Bytes(n)), Some(V::Bytes(e))) => {
                    let pub_key = RsaPublicKey::new(
                        BigUint::from_bytes_be(n),
                        BigUint::from_bytes_be(e),
                    ).map_err(|e| anyhow!("ERROR_BADARG rsa_verify_pkcs1_sha256: invalid key: {}", e))?;
                    let vk: RsaVerifyingKey<Sha256> = RsaVerifyingKey::new(pub_key);
                    let signature = RsaSig::try_from(sig.as_slice())
                        .map_err(|e| anyhow!("ERROR_BADARG rsa_verify_pkcs1_sha256: invalid sig: {}", e))?;
                    Ok(V::Bool(vk.verify(msg, &signature).is_ok()))
                }
                _ => Err(anyhow!("ERROR_BADARG rsa_verify_pkcs1_sha256 expects (bytes, bytes, bytes, bytes)")),
            }
        }
        "ecdsa_p256_verify" => {
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(msg)), Some(V::Bytes(sig)), Some(V::Bytes(x)), Some(V::Bytes(y))) => {
                    use p256::EncodedPoint;
                    let point = EncodedPoint::from_affine_coordinates(
                        x.as_slice().into(),
                        y.as_slice().into(),
                        false,
                    );
                    let vk = VerifyingKey::from_encoded_point(&point)
                        .map_err(|e| anyhow!("ERROR_BADARG ecdsa_p256_verify: invalid key: {}", e))?;
                    let signature = DerSignature::try_from(sig.as_slice())
                        .map_err(|e| anyhow!("ERROR_BADARG ecdsa_p256_verify: invalid sig: {}", e))?;
                    Ok(V::Bool(vk.verify(msg, &signature).is_ok()))
                }
                _ => Err(anyhow!("ERROR_BADARG ecdsa_p256_verify expects (bytes, bytes, bytes, bytes)")),
            }
        }
        "ok" => {
            match args.get(0) {
                Some(v) => Ok(valuecore::v0::normalize(&V::Map(vec![
                    ("tag".to_string(), V::Text("ok".to_string())),
                    ("val".to_string(), v.clone()),
                ]))),
                _ => Ok(V::Err("ERROR_BADARG ok expects 1 argument".into())),
            }
        }
        "err" => {
            match args.get(0) {
                Some(V::Text(code)) => Ok(V::Err(code.clone())),
                Some(v) => Ok(V::Err(format!("{:?}", v))),
                _ => Ok(V::Err("ERROR_BADARG err expects 1 argument".into())),
            }
        }
        _ => Err(anyhow!("ERROR_EVAL unknown builtin {}", f)),
    }
}

fn json_to_v(j: &serde_json::Value) -> Result<V> {
    match j {
        serde_json::Value::Null => Ok(V::Unit),
        serde_json::Value::Bool(b) => Ok(V::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(V::Int(i))
            } else {
                Ok(V::Err("ERROR_BADARG json_parse: non-integer number".into()))
            }
        }
        serde_json::Value::String(s) => Ok(V::Text(s.clone())),
        serde_json::Value::Array(xs) => {
            let vs: Result<Vec<V>> = xs.iter().map(json_to_v).collect();
            Ok(V::List(vs?))
        }
        serde_json::Value::Object(m) => {
            let kvs: Result<Vec<(String, V)>> = m.iter()
                .map(|(k, v)| json_to_v(v).map(|vv| (k.clone(), vv)))
                .collect();
            Ok(valuecore::v0::normalize(&V::Map(kvs?)))
        }
    }
}

fn v_to_json(v: &V) -> Result<serde_json::Value> {
    match v {
        V::Unit => Ok(serde_json::Value::Null),
        V::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        V::Int(i) => Ok(serde_json::Value::Number((*i).into())),
        V::Text(s) => Ok(serde_json::Value::String(s.clone())),
        V::Bytes(b) => Ok(serde_json::Value::String(URL_SAFE_NO_PAD.encode(b))),
        V::List(xs) => {
            let vs: Result<Vec<serde_json::Value>> = xs.iter().map(v_to_json).collect();
            Ok(serde_json::Value::Array(vs?))
        }
        V::Map(kvs) => {
            let mut m = serde_json::Map::new();
            for (k, v) in kvs {
                m.insert(k.clone(), v_to_json(v)?);
            }
            Ok(serde_json::Value::Object(m))
        }
        V::Err(e) => Ok(serde_json::Value::String(format!("error:{}", e))),
        V::Ok(v) => v_to_json(v),
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
