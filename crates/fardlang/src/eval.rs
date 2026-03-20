use crate::ast::{Block, Expr, FnDecl, Stmt, Pattern};

// Sentinel used to propagate err values through ? without unwinding past block boundaries
#[derive(Debug)]
struct TryPropagation(V);
impl std::fmt::Display for TryPropagation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "TryPropagation") }
}
impl std::error::Error for TryPropagation {}
use valuecore::base64url;


use valuecore::json::{JsonVal, from_str as json_from_str, to_string as json_to_string};
use chacha20poly1305::{XChaCha20Poly1305, KeyInit, aead::{Aead, Payload}};

#[derive(Debug, Clone)]
pub enum EvalVal {
    V(V),
    TailCall { name: String, args: Vec<V> },
    Closure { params: Vec<String>, body: Box<Block>, captured: Vec<(String, EvalVal)>, fns: std::collections::BTreeMap<String, FnDecl> },
}

impl EvalVal {
    pub fn into_v(self) -> Result<V> {
        match self {
            EvalVal::V(v) => Ok(v),
            EvalVal::TailCall { .. } => Err(anyhow!("ERROR_EVAL tail call is not a value")),
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
use valuecore::{Val as V, canon_cmp, canon_eq};
use valuecore::int::{i64_add, i64_sub, i64_mul, i64_div, i64_rem, i64_neg};

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
            max_depth: 10000,
        }
    }

    pub fn with_fns(fns: BTreeMap<String, FnDecl>) -> Self {
        Self {
            bindings: vec![],
            fns,
            aliases: BTreeMap::new(),
            declared_effects: BTreeSet::new(),
            depth: 0,
            max_depth: 10000,
        }
    }

    pub fn set_max_depth(&mut self, d: usize) { self.max_depth = d; }

    pub fn with_fns_and_depth(fns: BTreeMap<String, FnDecl>, max_depth: usize) -> Self {
        Self {
            bindings: vec![],
            fns,
            aliases: BTreeMap::new(),
            declared_effects: BTreeSet::new(),
            depth: 0,
            max_depth,
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

    #[allow(dead_code)]
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
        ("crypto",&["sha256","hkdf_sha256","xchacha20poly1305_seal","xchacha20poly1305_open"]),
        ("encode",&["base64url_encode","base64url_decode","json_parse","json_emit"]),
        ("float",&["from_int","to_int","from_text","to_text","add","sub","mul","div","exp","ln","sqrt","pow","abs","neg","lt","gt","le","ge","eq","nan","inf","is_nan","is_finite","floor","ceil","round"]),
        ("linalg",&["dot","norm","zeros","eye","matvec","matmul","transpose","eigh","vec_add","vec_sub","vec_scale","mat_add","mat_scale","vec_exp","vec_log","vec_sum","vec_max","vec_mul","vec_relu","vec_relu_grad","softmax","softmax_grad","cross_entropy","outer","mat_mul_vec_grad","vec_scalar_add","mat_row_sum"]),
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
                    EvalVal::TailCall { .. } => {}
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
            Ok(EvalVal::V(V::err("ERROR_MATCH_NO_ARM")))
        }
        Expr::RecordLit(fs) => {
            let mut kvs: Vec<(String, V)> = vec![];
            for (k, v) in fs.iter() {
                let vv = eval_expr(v, env)?.into_v()?;
                kvs.push((k.clone(), vv));
            }
            Ok(EvalVal::V(V::record(kvs)))
        }
        Expr::FieldGet { base, field } => {
            let b = eval_expr(base, env)?.into_v()?;
            match b {
                V::Record(kvs) => {
                    let nb = V::record(kvs);
                    match nb {
                        V::Record(xs) => {
                            for (k, v) in xs {
                                if k == *field {
                                    return Ok(EvalVal::V(v));
                                }
                            }
                            Ok(EvalVal::V(V::err_data(&format!("ERROR_OOB record missing field {}", field), V::Unit)))
                        }
                        _ => unreachable!("normalize(Map) must return Map"),
                    }
                }
                _ => Ok(EvalVal::V(V::err("ERROR_BADARG field access expects record"))),
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
                    Err(e) => V::err_data(&e.to_string(), V::Unit),
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
                    return Ok(EvalVal::V(V::err("ERROR_EVAL_DEPTH recursion limit exceeded")));
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
                return Ok(EvalVal::V(V::err("ERROR_EVAL_DEPTH recursion limit exceeded")));
            }

            // no closures: new child env, only params + all fns (for recursion)
            let mut child = Env::with_fns(env.fns.clone());
            child.depth = env.depth + 1;
            child.max_depth = env.max_depth;

            for (i, param) in decl.params.iter().enumerate() {
                let name = param.0.clone(); // (String, Type)
                let ty = &param.1;
                let ev = eval_expr(&args[i], env)?;
                // runtime type enforcement for non-Value annotations
                if let EvalVal::V(ref v) = ev {
                    if !type_check(ty, v) {
                        return Ok(EvalVal::V(V::err(&format!(
                            "ERROR_TYPE {}:{} expected {} got {}",
                            f, name, type_name(ty), v.type_name()
                        ))));
                    }
                }
                child.set_eval(name, ev);
            }

            // TCO: just call eval_block directly; depth already checked above
            eval_block(&decl.body, &mut child).map(EvalVal::V)
        }

        Expr::TryExpr { inner } => {
            let v = eval_expr(inner, env)?;
            match v {
                // err value: propagate up via sentinel
                EvalVal::V(V::Err { code: ref e, .. }) => {
                    return Err(anyhow::Error::new(TryPropagation(V::err(e))));
                }
                // {tag:"ok", val:v} record: unwrap to v
                EvalVal::V(V::Record(ref kvs)) => {
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
                        return Ok(EvalVal::V(V::err("ERROR_EVAL_DEPTH recursion limit exceeded")));
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
        Pattern::Ok(inner) => {
            // ok(x) matches {tag:"ok", val:v} or V::Ok(v)
            match v {
                V::Record(kvs) => {
                    let tag = kvs.iter().find(|(k,_)| k == "tag").map(|(_,v)| v);
                    let val = kvs.iter().find(|(k,_)| k == "val").map(|(_,v)| v);
                    matches!(tag, Some(V::Text(t)) if t == "ok")
                        && val.map_or(false, |v| pat_matches(inner, v))
                }
                _ => false,
            }
        }
        Pattern::Err(inner) => {
            match v {
                V::Record(kvs) => {
                    let tag = kvs.iter().find(|(k,_)| k == "tag").map(|(_,v)| v);
                    let val = kvs.iter().find(|(k,_)| k == "val").map(|(_,v)| v);
                    matches!(tag, Some(V::Text(t)) if t == "err")
                        && val.map_or(false, |v| pat_matches(inner, v))
                }
                V::Err { .. } => pat_matches(inner, &V::Text("_err_".into())),
                _ => false,
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
        Pattern::Ok(inner) => {
            match v {
                V::Record(kvs) => {
                    if let Some((_, val)) = kvs.iter().find(|(k,_)| k == "val") {
                        pat_binds(inner, val)
                    } else { vec![] }
                }
                _ => vec![],
            }
        }
        Pattern::Err(inner) => {
            match v {
                V::Record(kvs) => {
                    if let Some((_, val)) = kvs.iter().find(|(k,_)| k == "val") {
                        pat_binds(inner, val)
                    } else { vec![] }
                }
                V::Err { code: e, .. } => pat_binds(inner, &V::Text(e.clone())),
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

fn type_check(ty: &crate::ast::Type, v: &V) -> bool {
    use crate::ast::Type;
    match ty {
        Type::Value => true,  // Value = any type, no check
        Type::Unit => matches!(v, V::Unit),
        Type::Bool => matches!(v, V::Bool(_)),
        Type::Int => matches!(v, V::Int(_)),
        Type::Text => matches!(v, V::Text(_)),
        Type::Bytes => matches!(v, V::Bytes(_)),
        Type::List(_) => matches!(v, V::List(_)),
        Type::Map(_, _) => matches!(v, V::Record(_)),
        Type::Named { name, .. } => match name.as_str() {
            "Value" => true,
            "Int" => matches!(v, V::Int(_)),
            "Text" => matches!(v, V::Text(_)),
            "Bool" => matches!(v, V::Bool(_)),
            "Bytes" => matches!(v, V::Bytes(_)),
            "List" => matches!(v, V::List(_)),
            "Map" => matches!(v, V::Record(_)),
            "Unit" => matches!(v, V::Unit),
            _ => true, // unknown named type — pass through
        },
    }
}

fn type_name(ty: &crate::ast::Type) -> &'static str {
    use crate::ast::Type;
    match ty {
        Type::Value => "Value",
        Type::Unit => "Unit",
        Type::Bool => "Bool",
        Type::Int => "Int",
        Type::Text => "Text",
        Type::Bytes => "Bytes",
        Type::List(_) => "List",
        Type::Map(_, _) => "Record",
        Type::Named { name, .. } => match name.as_str() {
            "Int" => "Int", "Text" => "Text", "Bool" => "Bool",
            "Bytes" => "Bytes", "List" => "List", "Map" => "Map",
            "Unit" => "Unit", "Value" => "Value",
            other => Box::leak(other.to_string().into_boxed_str()),
        },
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
            | "ok"
            | "err"
            | "float_from_int"
            | "float_to_int"
            | "float_from_text"
            | "float_to_text"
            | "float_add"
            | "float_sub"
            | "float_mul"
            | "float_div"
            | "float_exp"
            | "float_ln"
            | "float_sqrt"
            | "float_pow"
            | "float_abs"
            | "float_neg"
            | "float_lt"
            | "float_gt"
            | "float_le"
            | "float_ge"
            | "float_eq"
            | "float_nan"
            | "float_inf"
            | "float_is_nan"
            | "float_is_finite"
            | "float_floor"
            | "float_ceil"
            | "float_round"
            | "linalg_dot"
            | "linalg_norm"
            | "linalg_zeros"
            | "linalg_eye"
            | "linalg_matvec"
            | "linalg_matmul"
            | "linalg_transpose"
            | "linalg_eigh"
            | "linalg_vec_add"
            | "linalg_vec_sub"
            | "linalg_vec_scale"
            | "linalg_mat_add"
            | "linalg_mat_scale"
            | "linalg_vec_exp"
            | "linalg_vec_log"
            | "linalg_vec_sum"
            | "linalg_vec_max"
            | "linalg_vec_mul"
            | "linalg_vec_relu"
            | "linalg_vec_relu_grad"
            | "linalg_softmax"
            | "linalg_softmax_grad"
            | "linalg_cross_entropy"
            | "linalg_outer"
            | "linalg_mat_mul_vec_grad"
            | "linalg_vec_scalar_add"
            | "linalg_mat_row_sum"
    )
}

fn eval_builtin(f: &str, args: &[V]) -> Result<V> {
    if f.starts_with("float_") { return eval_float_builtin(f, args); }
    if f.starts_with("linalg_") { return eval_linalg_builtin(f, args); }
    match f {
        "add" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_add(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
        }
        "sub" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_sub(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
        }
        "mul" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_mul(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
        }
        "div" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_div(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
        }
        "rem" => {
            let (a, b) = match expect_i64_2(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_rem(a, b) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
        }
        "neg" => {
            let a = match expect_i64_1(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            match i64_neg(a) { Ok(n) => Ok(V::Int(n)), Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)) }
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
                _ => Ok(V::err("ERROR_BADARG gt expects (int, int)")),
            }
        }
        "le" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Bool(a <= b)),
                _ => Ok(V::err("ERROR_BADARG le expects (int, int)")),
            }
        }
        "ge" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Int(a)), Some(V::Int(b))) => Ok(V::Bool(a >= b)),
                _ => Ok(V::err("ERROR_BADARG ge expects (int, int)")),
            }
        }
        "not" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG not arity"));
            }
            match &args[0] {
                V::Bool(b) => Ok(V::Bool(!*b)),
                _ => Ok(V::err("ERROR_BADARG not expects bool")),
            }
        }
        "list_len" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG list_len arity"));
            }
            match &args[0] {
                V::List(xs) => Ok(V::Int(xs.len() as i64)),
                _ => Ok(V::err("ERROR_BADARG list_len expects list")),
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
                return Ok(V::err("ERROR_OOB list_get"));
            }
            let u = idx as usize;
            match &args[0] {
                V::List(xs) => Ok(xs
                    .get(u)
                    .cloned()
                    .unwrap_or_else(|| V::err("ERROR_OOB list_get"))),
                _ => Ok(V::err("ERROR_BADARG list_get expects list")),
            }
        }
        "map_get" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Record(kvs)), Some(V::Text(k))) => {
                    let norm = V::record(kvs.clone());
                    if let V::Record(nkvs) = norm {
                        let found = nkvs.into_iter().find(|(ek, _)| ek == k).map(|(_, v)| v);
                        Ok(found.unwrap_or_else(|| V::err_data(&format!("ERROR_OOB map_get missing key {}", k), V::Unit)))
                    } else {
                        Ok(V::err("ERROR_BADARG map_get expects record"))
                    }
                }
                _ => Ok(V::err("ERROR_BADARG map_get expects (record, text)")),
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
                _ => Ok(V::err("ERROR_BADARG text_concat expects text")),
            }
        }
        "int_to_text" => {
            if args.len() != 1 {
                return Err(anyhow!("ERROR_BADARG int_to_text arity"));
            }
            match &args[0] {
                V::Int(i) => Ok(V::Text(i.to_string())),
                _ => Ok(V::err("ERROR_BADARG int_to_text expects int")),
            }
        }
        "text_len" => {
            match args.get(0) {
                Some(V::Text(s)) => Ok(V::Int(s.chars().count() as i64)),
                _ => Ok(V::err("ERROR_BADARG text_len expects text")),
            }
        }
        "text_contains" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.contains(pat.as_str()))),
                _ => Ok(V::err("ERROR_BADARG text_contains expects (text, text)")),
            }
        }
        "text_starts_with" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(pat))) => Ok(V::Bool(s.starts_with(pat.as_str()))),
                _ => Ok(V::err("ERROR_BADARG text_starts_with expects (text, text)")),
            }
        }
        "text_split" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Text(s)), Some(V::Text(sep))) => {
                    let parts: Vec<V> = s.split(sep.as_str()).map(|p| V::Text(p.to_string())).collect();
                    Ok(V::List(parts))
                }
                _ => Ok(V::err("ERROR_BADARG text_split expects (text, text)")),
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
            Ok(V::record(vec![]))
        }
        "map_set" => {
            match (args.get(0), args.get(1), args.get(2)) {
                (Some(V::Record(kvs)), Some(V::Text(k)), Some(v)) => {
                    let mut out = kvs.clone();
                    out.retain(|(ek, _)| ek != k);
                    out.push((k.clone(), v.clone()));
                    Ok(V::record(out))
                }
                _ => Err(anyhow!("ERROR_BADARG map_set expects (record, text, value)")),
            }
        }
        "map_has" => {
            match (args.get(0), args.get(1)) {
                (Some(V::Record(kvs)), Some(V::Text(k))) => {
                    let norm = V::record(kvs.clone());
                    if let V::Record(nkvs) = norm {
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
                Some(V::Record(kvs)) => {
                    let norm = V::record(kvs.clone());
                    if let V::Record(nkvs) = norm {
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
                (Some(V::Record(kvs)), Some(V::Text(k))) => {
                    let mut out = kvs.clone();
                    out.retain(|(ek, _)| ek != k);
                    Ok(V::record(out))
                }
                _ => Err(anyhow!("ERROR_BADARG map_delete expects (record, text)")),
            }
        }
        "base64url_encode" => {
            match args.get(0) {
                Some(V::Bytes(b)) => Ok(V::Text(base64url::encode(b))),
                _ => Err(anyhow!("ERROR_BADARG base64url_encode expects bytes")),
            }
        }
        "base64url_decode" => {
            match args.get(0) {
                Some(V::Text(s)) => {
                    match base64url::decode(s.as_bytes()) {
                        Ok(b) => Ok(V::Bytes(b)),
                        Err(e) => Ok(V::err_data(&format!("ERROR_BADARG base64url_decode: {}", e), V::Unit)),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG base64url_decode expects text")),
            }
        }
        "json_parse" => {
            match args.get(0) {
                Some(V::Text(s)) => {
                    match json_from_str(s) {
                        Ok(jv) => json_to_v(&jv),
                        Err(e) => Ok(V::err_data(&format!("ERROR_BADARG json_parse: {}", e), V::Unit)),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG json_parse expects text")),
            }
        }
        "json_emit" => {
            match args.get(0) {
                Some(v) => {
                    match v_to_json(v).map(|jv| json_to_string(&jv)) {
                        Ok(s) => Ok(V::Text(s)),
                        Err(e) => Ok(V::err_data(&format!("ERROR_EVAL json_emit: {}", e), V::Unit)),
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG json_emit expects value")),
            }
        }
        "sha256" => {
            match args.get(0) {
                Some(V::Bytes(b)) => {
                    let hash = valuecore::Sha256::digest(b);
                    Ok(V::Bytes(hash.to_vec()))
                }
                _ => Err(anyhow!("ERROR_BADARG sha256 expects bytes")),
            }
        }
        "hkdf_sha256" => {
            match (args.get(0), args.get(1), args.get(2), args.get(3)) {
                (Some(V::Bytes(ikm)), Some(V::Bytes(salt)), Some(V::Bytes(info)), Some(V::Int(len))) => {


                    match valuecore::hkdf_sha256(salt, ikm, info, *len as usize) {
                        Ok(out) => Ok(V::Bytes(out)),
                        Err(e) => Ok(V::err_data(&e.to_string(), V::Unit)),
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
                        Err(_) => Ok(V::err("ERROR_BADARG xchacha20poly1305_seal: key must be 32 bytes")),
                        Ok(cipher) => {
                            let n = XNonce::from_slice(nonce);
                            match cipher.encrypt(n, Payload { msg: pt, aad }) {
                                Ok(ct) => Ok(V::Bytes(ct)),
                                Err(_) => Ok(V::err("ERROR_EVAL xchacha20poly1305_seal: encryption failed")),
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
                        Err(_) => Ok(V::err("ERROR_BADARG xchacha20poly1305_open: key must be 32 bytes")),
                        Ok(cipher) => {
                            let n = XNonce::from_slice(nonce);
                            match cipher.decrypt(n, Payload { msg: ct, aad }) {
                                Ok(pt) => Ok(V::Bytes(pt)),
                                Err(_) => Ok(V::err("ERROR_EVAL xchacha20poly1305_open: decryption failed")),
                            }
                        }
                    }
                }
                _ => Err(anyhow!("ERROR_BADARG xchacha20poly1305_open expects (bytes, bytes, bytes, bytes)")),
            }
        }


        "ok" => {
            match args.get(0) {
                Some(v) => Ok(V::record(vec![
                    ("tag".to_string(), V::Text("ok".to_string())),
                    ("val".to_string(), v.clone()),
                ])),
                _ => Ok(V::err("ERROR_BADARG ok expects 1 argument")),
            }
        }
        "err" => {
            match args.get(0) {
                Some(V::Text(code)) => Ok(V::err(code)),
                Some(v) => Ok(V::err_data(&format!("{:?}", v), V::Unit)),
                _ => Ok(V::err("ERROR_BADARG err expects 1 argument")),
            }
        }
        _ => Err(anyhow!("ERROR_EVAL unknown builtin {}", f)),
    }
}

fn json_to_v(j: &JsonVal) -> Result<V> {
    match j {
        JsonVal::Null => Ok(V::Unit),
        JsonVal::Bool(b) => Ok(V::Bool(*b)),
        JsonVal::Float(f) => Ok(V::Bytes(f.to_le_bytes().to_vec())),
        JsonVal::Int(n) => Ok(V::Int(*n)),
        JsonVal::Str(s) => Ok(V::Text(s.clone())),
        JsonVal::Array(xs) => {
            let vs: Result<Vec<V>> = xs.iter().map(json_to_v).collect();
            Ok(V::List(vs?))
        }
        JsonVal::Object(m) => {
            let kvs: Result<Vec<(String, V)>> = m.iter()
                .map(|(k, v)| json_to_v(v).map(|vv| (k.clone(), vv)))
                .collect();
            Ok(V::record(kvs?))
        }
    }
}

fn v_to_json(v: &V) -> Result<JsonVal> {
    match v {
        V::Unit => Ok(JsonVal::Null),
        V::Bool(b) => Ok(JsonVal::Bool(*b)),
        V::Int(i) => Ok(JsonVal::Int(*i)),
        V::Text(s) => Ok(JsonVal::Str(s.clone())),
        V::Bytes(b) => Ok(JsonVal::Str(valuecore::base64url::encode(b))),
        V::List(xs) => {
            let vs: Result<Vec<JsonVal>> = xs.iter().map(v_to_json).collect();
            Ok(JsonVal::Array(vs?))
        }
        V::Record(kvs) => {
            let mut m = std::collections::BTreeMap::new();
            for (k, v) in kvs {
                m.insert(k.clone(), v_to_json(v)?);
            }
            Ok(JsonVal::Object(m))
        }
        V::Float(f) => Ok(JsonVal::Float(*f)),
        V::Err { code: e, .. } => Ok(JsonVal::Str(format!("error:{}", e))),

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


// ── float helpers ─────────────────────────────────────────────────────────────

fn f64_to_bytes(f: f64) -> V {
    V::Bytes(f.to_le_bytes().to_vec())
}

fn bytes_to_f64(v: &V) -> Result<f64> {
    match v {
        V::Bytes(b) if b.len() == 8 => {
            let arr: [u8; 8] = b.as_slice().try_into().unwrap();
            Ok(f64::from_le_bytes(arr))
        }
        V::Int(n) => {
            // allow passing plain ints as floats for convenience
            let s = n.to_string();
            Ok(s.parse::<f64>().unwrap_or(f64::NAN))
        }
        _ => Err(anyhow!("ERROR_BADARG expected float (8-byte Bytes), got {:?}", v)),
    }
}

fn expect_f64_1(args: &[V]) -> Result<f64> {
    if args.len() != 1 { return Err(anyhow!("ERROR_ARITY expected 1 arg")); }
    bytes_to_f64(&args[0])
}

fn expect_f64_2(args: &[V]) -> Result<(f64, f64)> {
    if args.len() != 2 { return Err(anyhow!("ERROR_ARITY expected 2 args")); }
    Ok((bytes_to_f64(&args[0])?, bytes_to_f64(&args[1])?))
}

fn eval_float_builtin(f: &str, args: &[V]) -> Result<V> {
    match f {
        "float_from_int" => {
            match args.first() {
                Some(V::Int(n)) => {
                    let s = n.to_string();
                    let v: f64 = s.parse().unwrap_or(f64::NAN);
                    Ok(f64_to_bytes(v))
                }
                _ => Ok(V::err("ERROR_BADARG float_from_int expects int")),
            }
        }
        "float_to_int" => {
            let f = match expect_f64_1(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            Ok(V::Int(f as i64))
        }
        "float_from_text" => {
            match args.first() {
                Some(V::Text(s)) => match s.parse::<f64>() {
                    Ok(v) => Ok(f64_to_bytes(v)),
                    Err(_) => Ok(V::err_data(&format!("ERROR_PARSE cannot parse float: {}", s), V::Unit)),
                },
                _ => Ok(V::err("ERROR_BADARG float_from_text expects text")),
            }
        }
        "float_to_text" => {
            let f = match expect_f64_1(args) { Ok(v) => v, Err(e) => return Ok(V::err_data(&e.to_string(), V::Unit)) };
            Ok(V::Text(format!("{}", f)))
        }
        "float_add" => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a+b)) }
        "float_sub" => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a-b)) }
        "float_mul" => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a*b)) }
        "float_div" => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a/b)) }
        "float_exp"  => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.exp())) }
        "float_ln"   => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.ln())) }
        "float_sqrt" => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.sqrt())) }
        "float_abs"  => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.abs())) }
        "float_neg"  => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(-a)) }
        "float_floor"=> { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.floor())) }
        "float_ceil" => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.ceil())) }
        "float_round"=> { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.round())) }
        "float_pow"  => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(f64_to_bytes(a.powf(b))) }
        "float_lt"   => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a<b)) }
        "float_gt"   => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a>b)) }
        "float_le"   => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a<=b)) }
        "float_ge"   => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a>=b)) }
        "float_eq"   => { let (a,b) = match expect_f64_2(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a==b)) }
        "float_nan"  => Ok(f64_to_bytes(f64::NAN)),
        "float_inf"  => Ok(f64_to_bytes(f64::INFINITY)),
        "float_is_nan"    => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a.is_nan())) }
        "float_is_finite" => { let a = match expect_f64_1(args) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) }; Ok(V::Bool(a.is_finite())) }
        _ => Err(anyhow!("ERROR_EVAL unknown float builtin: {}", f)),
    }
}

// ── linalg helpers ────────────────────────────────────────────────────────────

fn v_to_vec(v: &V) -> Result<Vec<f64>> {
    match v {
        V::List(items) => items.iter().map(bytes_to_f64).collect(),
        _ => Err(anyhow!("ERROR_BADARG expected list of floats")),
    }
}

fn vec_to_v(v: &[f64]) -> V {
    V::List(v.iter().map(|&x| f64_to_bytes(x)).collect())
}

fn v_to_mat(v: &V) -> Result<Vec<Vec<f64>>> {
    match v {
        V::List(rows) => rows.iter().map(v_to_vec).collect(),
        _ => Err(anyhow!("ERROR_BADARG expected list of list of floats")),
    }
}

fn mat_to_v(m: &[Vec<f64>]) -> V {
    V::List(m.iter().map(|r| vec_to_v(r)).collect())
}

fn eval_linalg_builtin(f: &str, args: &[V]) -> Result<V> {
    match f {
        "linalg_zeros" => {
            match args.first() {
                Some(V::Int(n)) => {
                    let s = n.to_string();
                    let n: usize = s.parse().map_err(|_| anyhow!("ERROR_BADARG linalg_zeros expects non-negative int"))?;
                    Ok(vec_to_v(&vec![0.0f64; n]))
                }
                _ => Ok(V::err("ERROR_BADARG linalg_zeros expects int")),
            }
        }
        "linalg_eye" => {
            match args.first() {
                Some(V::Int(n)) => {
                    let s = n.to_string();
                    let n: usize = s.parse().map_err(|_| anyhow!("ERROR_BADARG linalg_eye expects non-negative int"))?;
                    let m: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| if i==j {1.0} else {0.0}).collect()).collect();
                    Ok(mat_to_v(&m))
                }
                _ => Ok(V::err("ERROR_BADARG linalg_eye expects int")),
            }
        }
        "linalg_dot" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_dot expects 2 args")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            if a.len() != b.len() { return Ok(V::err("ERROR_BADARG linalg_dot length mismatch")); }
            Ok(f64_to_bytes(a.iter().zip(b.iter()).map(|(x,y)| x*y).sum()))
        }
        "linalg_norm" => {
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            Ok(f64_to_bytes(a.iter().map(|x| x*x).sum::<f64>().sqrt()))
        }
        "linalg_vec_add" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            if a.len() != b.len() { return Ok(V::err("ERROR_BADARG length mismatch")); }
            Ok(vec_to_v(&a.iter().zip(b.iter()).map(|(x,y)| x+y).collect::<Vec<_>>()))
        }
        "linalg_vec_sub" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            if a.len() != b.len() { return Ok(V::err("ERROR_BADARG length mismatch")); }
            Ok(vec_to_v(&a.iter().zip(b.iter()).map(|(x,y)| x-y).collect::<Vec<_>>()))
        }
        "linalg_vec_scale" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let s = match bytes_to_f64(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            Ok(vec_to_v(&a.iter().map(|x| x*s).collect::<Vec<_>>()))
        }
        "linalg_transpose" => {
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            if m.is_empty() { return Ok(mat_to_v(&[])); }
            let rows = m.len(); let cols = m[0].len();
            let t: Vec<Vec<f64>> = (0..cols).map(|j| (0..rows).map(|i| m[i][j]).collect()).collect();
            Ok(mat_to_v(&t))
        }
        "linalg_matvec" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_matvec expects 2 args")); }
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let x = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let result: Vec<f64> = m.iter().map(|row| {
                row.iter().zip(x.iter()).map(|(a,b)| a*b).sum()
            }).collect();
            Ok(vec_to_v(&result))
        }
        "linalg_matmul" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_matmul expects 2 args")); }
            let a = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let b = match v_to_mat(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            if a.is_empty() || b.is_empty() { return Ok(mat_to_v(&[])); }
            let m = a.len(); let k = a[0].len(); let n = b[0].len();
            let c: Vec<Vec<f64>> = (0..m).map(|i|
                (0..n).map(|j|
                    (0..k).map(|l| a[i][l]*b[l][j]).sum()
                ).collect()
            ).collect();
            Ok(mat_to_v(&c))
        }
        "linalg_mat_add" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY")); }
            let a = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let b = match v_to_mat(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let c: Vec<Vec<f64>> = a.iter().zip(b.iter()).map(|(ra,rb)|
                ra.iter().zip(rb.iter()).map(|(x,y)| x+y).collect()
            ).collect();
            Ok(mat_to_v(&c))
        }
        "linalg_mat_scale" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY")); }
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let s = match bytes_to_f64(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            Ok(mat_to_v(&m.iter().map(|r| r.iter().map(|x| x*s).collect()).collect::<Vec<_>>()))
        }
        "linalg_eigh" => {
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err_data(&e.to_string(), V::Unit)) };
            let n = m.len();
            if n == 0 { return Ok(V::Record(vec![
                ("vals".into(), V::List(vec![])),
                ("vecs".into(), V::List(vec![])),
            ])); }
            let flat: Vec<f64> = m.iter().flat_map(|r| r.iter().cloned()).collect();
            let (eigenvalues, eigenvecs) = valuecore::linalg::eigh(&flat, n);
            Ok(V::Record(vec![
                ("vals".into(), vec_to_v(&eigenvalues)),
                ("vecs".into(), mat_to_v(&eigenvecs)),
            ]))
        }
        "linalg_vec_exp" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_exp expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.exp()).collect::<Vec<_>>()))
        }
        "linalg_vec_log" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_log expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.ln()).collect::<Vec<_>>()))
        }
        "linalg_vec_sum" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_sum expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(V::Float(v.iter().sum()))
        }
        "linalg_vec_max" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_max expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if v.is_empty() { return Ok(V::err("ERROR_BADARG linalg_vec_max empty")); }
            Ok(V::Float(v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
        }
        "linalg_vec_mul" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_mul expects 2 args")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if a.len() != b.len() { return Ok(V::err("ERROR_BADARG linalg_vec_mul length mismatch")); }
            Ok(vec_to_v(&a.iter().zip(b.iter()).map(|(x,y)| x*y).collect::<Vec<_>>()))
        }
        "linalg_vec_relu" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_relu expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.max(0.0)).collect::<Vec<_>>()))
        }
        "linalg_vec_relu_grad" => {
            // gradient of relu: 1 if x > 0 else 0, multiplied by upstream grad
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_relu_grad expects 2 args")); }
            let x  = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let dv = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if x.len() != dv.len() { return Ok(V::err("ERROR_BADARG linalg_vec_relu_grad length mismatch")); }
            Ok(vec_to_v(&x.iter().zip(dv.iter()).map(|(xi,di)| if *xi > 0.0 { *di } else { 0.0 }).collect::<Vec<_>>()))
        }
        "linalg_softmax" => {
            // numerically stable softmax over a vector
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_softmax expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if v.is_empty() { return Ok(vec_to_v(&[])); }
            let max = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let exps: Vec<f64> = v.iter().map(|x| (x - max).exp()).collect();
            let sum: f64 = exps.iter().sum();
            Ok(vec_to_v(&exps.iter().map(|x| x / sum).collect::<Vec<_>>()))
        }
        "linalg_softmax_grad" => {
            // gradient of softmax cross-entropy: s - one_hot(y)
            // args: softmax output s [n], true class index y (as int string)
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_softmax_grad expects 2 args")); }
            let s = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let y = match &args[1] {
                V::Int(i) => *i as usize,
                V::Text(s) => s.parse::<usize>().unwrap_or(0),
                _ => return Ok(V::err("ERROR_BADARG linalg_softmax_grad expects int label")),
            };
            let mut grad = s.clone();
            if y < grad.len() { grad[y] -= 1.0; }
            Ok(vec_to_v(&grad))
        }
        "linalg_cross_entropy" => {
            // cross entropy loss: -log(s[y])
            // args: softmax output s [n], true class index y
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_cross_entropy expects 2 args")); }
            let s = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let y = match &args[1] {
                V::Int(i) => *i as usize,
                V::Text(s) => s.parse::<usize>().unwrap_or(0),
                _ => return Ok(V::err("ERROR_BADARG linalg_cross_entropy expects int label")),
            };
            if y >= s.len() { return Ok(V::err("ERROR_BADARG linalg_cross_entropy label out of range")); }
            let loss = -(s[y].max(1e-15).ln());
            Ok(V::Float(loss))
        }
        "linalg_outer" => {
            // outer product: a [m] x b [n] -> mat [m][n]
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_outer expects 2 args")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let mat: Vec<Vec<f64>> = a.iter().map(|ai| b.iter().map(|bj| ai*bj).collect()).collect();
            Ok(mat_to_v(&mat))
        }
        "linalg_mat_mul_vec_grad" => {
            // gradient of y=Wx wrt W: outer(grad_y, x)
            // gradient of y=Wx wrt x: W^T @ grad_y
            // args: W [m,n], x [n], grad_y [m]
            // returns: {dW: mat[m,n], dx: vec[n]}
            if args.len() != 3 { return Ok(V::err("ERROR_ARITY linalg_mat_mul_vec_grad expects 3 args")); }
            let w    = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let x    = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let dout = match v_to_vec(&args[2]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            // dW = outer(dout, x)
            let dw: Vec<Vec<f64>> = dout.iter().map(|di| x.iter().map(|xj| di*xj).collect()).collect();
            // dx = W^T @ dout
            let m = w.len(); let n = if m>0 { w[0].len() } else { 0 };
            let mut dx = vec![0.0f64; n];
            for i in 0..m { for j in 0..n { dx[j] += w[i][j] * dout[i]; } }
            Ok(V::Record(vec![
                ("dW".into(), mat_to_v(&dw)),
                ("dx".into(), vec_to_v(&dx)),
            ]))
        }
        "linalg_vec_scalar_add" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_scalar_add expects 2 args")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let s = match &args[1] { V::Float(f)=>*f, V::Int(i)=>*i as f64, _=>return Ok(V::err("ERROR_BADARG scalar")) };
            Ok(vec_to_v(&v.iter().map(|x| x+s).collect::<Vec<_>>()))
        }
        "linalg_mat_row_sum" => {
            // sum each row of a matrix -> vec of row sums
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_mat_row_sum expects 1 arg")); }
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&m.iter().map(|row| row.iter().sum()).collect::<Vec<_>>()))
        }
        "linalg_vec_exp" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_exp expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.exp()).collect::<Vec<_>>()))
        }
        "linalg_vec_log" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_log expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.ln()).collect::<Vec<_>>()))
        }
        "linalg_vec_sum" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_sum expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(V::Float(v.iter().sum()))
        }
        "linalg_vec_max" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_max expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if v.is_empty() { return Ok(V::err("ERROR_BADARG linalg_vec_max empty")); }
            Ok(V::Float(v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)))
        }
        "linalg_vec_mul" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_mul expects 2 args")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if a.len() != b.len() { return Ok(V::err("ERROR_BADARG linalg_vec_mul length mismatch")); }
            Ok(vec_to_v(&a.iter().zip(b.iter()).map(|(x,y)| x*y).collect::<Vec<_>>()))
        }
        "linalg_vec_relu" => {
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_vec_relu expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&v.iter().map(|x| x.max(0.0)).collect::<Vec<_>>()))
        }
        "linalg_vec_relu_grad" => {
            // gradient of relu: 1 if x > 0 else 0, multiplied by upstream grad
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_relu_grad expects 2 args")); }
            let x  = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let dv = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if x.len() != dv.len() { return Ok(V::err("ERROR_BADARG linalg_vec_relu_grad length mismatch")); }
            Ok(vec_to_v(&x.iter().zip(dv.iter()).map(|(xi,di)| if *xi > 0.0 { *di } else { 0.0 }).collect::<Vec<_>>()))
        }
        "linalg_softmax" => {
            // numerically stable softmax over a vector
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_softmax expects 1 arg")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            if v.is_empty() { return Ok(vec_to_v(&[])); }
            let max = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let exps: Vec<f64> = v.iter().map(|x| (x - max).exp()).collect();
            let sum: f64 = exps.iter().sum();
            Ok(vec_to_v(&exps.iter().map(|x| x / sum).collect::<Vec<_>>()))
        }
        "linalg_softmax_grad" => {
            // gradient of softmax cross-entropy: s - one_hot(y)
            // args: softmax output s [n], true class index y (as int string)
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_softmax_grad expects 2 args")); }
            let s = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let y = match &args[1] {
                V::Int(i) => *i as usize,
                V::Text(s) => s.parse::<usize>().unwrap_or(0),
                _ => return Ok(V::err("ERROR_BADARG linalg_softmax_grad expects int label")),
            };
            let mut grad = s.clone();
            if y < grad.len() { grad[y] -= 1.0; }
            Ok(vec_to_v(&grad))
        }
        "linalg_cross_entropy" => {
            // cross entropy loss: -log(s[y])
            // args: softmax output s [n], true class index y
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_cross_entropy expects 2 args")); }
            let s = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let y = match &args[1] {
                V::Int(i) => *i as usize,
                V::Text(s) => s.parse::<usize>().unwrap_or(0),
                _ => return Ok(V::err("ERROR_BADARG linalg_cross_entropy expects int label")),
            };
            if y >= s.len() { return Ok(V::err("ERROR_BADARG linalg_cross_entropy label out of range")); }
            let loss = -(s[y].max(1e-15).ln());
            Ok(V::Float(loss))
        }
        "linalg_outer" => {
            // outer product: a [m] x b [n] -> mat [m][n]
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_outer expects 2 args")); }
            let a = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let b = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let mat: Vec<Vec<f64>> = a.iter().map(|ai| b.iter().map(|bj| ai*bj).collect()).collect();
            Ok(mat_to_v(&mat))
        }
        "linalg_mat_mul_vec_grad" => {
            // gradient of y=Wx wrt W: outer(grad_y, x)
            // gradient of y=Wx wrt x: W^T @ grad_y
            // args: W [m,n], x [n], grad_y [m]
            // returns: {dW: mat[m,n], dx: vec[n]}
            if args.len() != 3 { return Ok(V::err("ERROR_ARITY linalg_mat_mul_vec_grad expects 3 args")); }
            let w    = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let x    = match v_to_vec(&args[1]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let dout = match v_to_vec(&args[2]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            // dW = outer(dout, x)
            let dw: Vec<Vec<f64>> = dout.iter().map(|di| x.iter().map(|xj| di*xj).collect()).collect();
            // dx = W^T @ dout
            let m = w.len(); let n = if m>0 { w[0].len() } else { 0 };
            let mut dx = vec![0.0f64; n];
            for i in 0..m { for j in 0..n { dx[j] += w[i][j] * dout[i]; } }
            Ok(V::Record(vec![
                ("dW".into(), mat_to_v(&dw)),
                ("dx".into(), vec_to_v(&dx)),
            ]))
        }
        "linalg_vec_scalar_add" => {
            if args.len() != 2 { return Ok(V::err("ERROR_ARITY linalg_vec_scalar_add expects 2 args")); }
            let v = match v_to_vec(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            let s = match &args[1] { V::Float(f)=>*f, V::Int(i)=>*i as f64, _=>return Ok(V::err("ERROR_BADARG scalar")) };
            Ok(vec_to_v(&v.iter().map(|x| x+s).collect::<Vec<_>>()))
        }
        "linalg_mat_row_sum" => {
            // sum each row of a matrix -> vec of row sums
            if args.len() != 1 { return Ok(V::err("ERROR_ARITY linalg_mat_row_sum expects 1 arg")); }
            let m = match v_to_mat(&args[0]) { Ok(v)=>v, Err(e)=>return Ok(V::err(&e.to_string())) };
            Ok(vec_to_v(&m.iter().map(|row| row.iter().sum()).collect::<Vec<_>>()))
        }
        _ => Err(anyhow!("ERROR_EVAL unknown linalg builtin: {}", f)),
    }
}
