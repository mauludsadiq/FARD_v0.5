use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value as J};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let (run, want_version) = fard_v0_5_language_gate::cli::fardrun_cli::Cli::parse_compat();
    if want_version {
        println!("fard_runtime_version={}", env!("CARGO_PKG_VERSION"));
        println!("trace_format_version=0.1.0");
        println!("stdlib_root_cid=sha256:dev");
        return Ok(());
    }

    let program = run.program;
    let out_dir = run.out;
    let lockfile = run.lockfile;
    let registry_dir = run.registry;

    fs::create_dir_all(&out_dir).ok();

    let trace_path = out_dir.join("trace.ndjson");
    let result_path = out_dir.join("result.json");

    let mut tracer = Tracer::new(&out_dir, &trace_path)?;
    let mut loader = ModuleLoader::new(program.parent().unwrap_or(Path::new(".")));
    if let Some(rp) = registry_dir.clone() {
        loader.registry_dir = Some(rp);
    }

    if let Some(lockp) = lockfile {
        loader.lock = Some(Lockfile::load(&lockp)?);
    }

    let v = match loader.eval_main(&program, &mut tracer) {
        Ok(v) => v,
        Err(e) => {
            let msg0 = format!("{}", e);
            let code = msg0
                .split_whitespace()
                .find(|w| w.starts_with("ERROR_"))
                .unwrap_or("ERROR_RUNTIME")
                .to_string();
            let msg = if msg0.contains("ERROR_") {
                msg0
            } else {
                format!("ERROR_RUNTIME {}", msg0)
            };
            let mut em = Map::new();
            em.insert("code".to_string(), J::String(code.clone()));
            em.insert("message".to_string(), J::String(msg.clone()));
            if let Some(pe) = e.downcast_ref::<ParseError>() {
                // Stored spans are absolute offsets; G39 expects line-relative byte offsets.
                let mut bs = pe.span.byte_start;
                let mut be = pe.span.byte_end;
                let mut ln = pe.span.line;
                let mut cl = pe.span.col;

                if let Ok(src) = fs::read_to_string(&pe.span.file) {
                    let abs_s = bs.min(src.len());
                    let ls = src[..abs_s].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_s = abs_s.saturating_sub(ls);

                    let abs_e = be.min(src.len());
                    let le = src[..abs_e].rfind("\n").map(|i| i + 1).unwrap_or(0);
                    let rel_e = abs_e.saturating_sub(le);

                    bs = rel_s;
                    be = rel_e;
                    cl = rel_s + 1;
                    ln = src[..ls].bytes().filter(|b| *b == b"\n"[0]).count() + 1;
                }

                let mut sm = Map::new();
                sm.insert("file".to_string(), J::String(pe.span.file.clone()));
                sm.insert("byte_start".to_string(), J::Number((bs as u64).into()));
                sm.insert("byte_end".to_string(), J::Number((be as u64).into()));
                sm.insert("line".to_string(), J::Number((ln as u64).into()));
                sm.insert("col".to_string(), J::Number((cl as u64).into()));
                em.insert("span".to_string(), J::Object(sm));
            }
            fs::write(
                out_dir.join("error.json"),
                serde_json::to_vec(&J::Object(em))?,
            )?;
            tracer.error_event(&code, &msg).ok();
            bail!(msg);
        }
    };
    let j = v.to_json().context("final result must be jsonable")?;
    let mut root = Map::new();
    root.insert("result".to_string(), j);
    fs::write(&result_path, serde_json::to_vec(&J::Object(root))?)?;
    Ok(())
}

struct Tracer {
    w: fs::File,
    out_dir: PathBuf,
}
impl Tracer {
    fn new(out_dir: &Path, path: &Path) -> Result<Self> {
        fs::create_dir_all(out_dir).ok();
        fs::create_dir_all(out_dir.join("artifacts")).ok();
        let w = fs::File::create(path)?;
        Ok(Self {
            w,
            out_dir: out_dir.to_path_buf(),
        })
    }

    fn emit(&mut self, v: &J) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("emit".to_string()));
        m.insert("v".to_string(), v.clone());
        let line = serde_json::to_string(&J::Object(m))?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    fn grow_node(&mut self, v: &Val) -> Result<()> {
        let j = v.to_json().context("grow_node must be jsonable")?;
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("grow_node".to_string()));
        m.insert("v".to_string(), j);
        let line = serde_json::to_string(&J::Object(m))?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    fn artifact_in(&mut self, path: &str, cid: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("artifact_in".to_string()));
        m.insert("path".to_string(), J::String(path.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        let line = serde_json::to_string(&J::Object(m))?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    fn artifact_out(&mut self, name: &str, cid: &str, bytes: &[u8]) -> Result<()> {
        let outp = self.out_dir.join("artifacts").join(name);
        fs::write(&outp, bytes)?;
        {
            let cid_path = if let Some(ext) = outp.extension().and_then(|e| e.to_str()) {
                outp.with_extension(format!("{ext}.cid"))
            } else {
                outp.with_extension("cid")
            };
            fs::write(&cid_path, format!("{}\n", cid))?;
        }
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("artifact_out".to_string()));
        m.insert("name".to_string(), J::String(name.to_string()));
        m.insert("cid".to_string(), J::String(cid.to_string()));
        let line = serde_json::to_string(&J::Object(m))?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    fn error_event(&mut self, code: &str, message: &str) -> Result<()> {
        let mut m = Map::new();
        m.insert("t".to_string(), J::String("error".to_string()));
        m.insert("code".to_string(), J::String(code.to_string()));
        m.insert("message".to_string(), J::String(message.to_string()));
        let line = serde_json::to_string(&J::Object(m))?;
        self.w.write_all(line.as_bytes())?;
        self.w.write_all(b"\n")?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Tok {
    Kw(String),
    Ident(String),
    Num(i64),
    Str(String),
    Sym(String),
    Eof,
}

#[derive(Clone, Debug)]
struct SpanPos {
    byte_start: usize,
    byte_end: usize,
    line: usize,
    col: usize,
}


fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
#[derive(Clone, Debug)]
struct ErrorSpan {
    file: String,
    byte_start: usize,
    byte_end: usize,
    line: usize,
    col: usize,
}

#[derive(Debug)]
struct ParseError {
    span: ErrorSpan,
    message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

fn line_col_at(src: &str, byte_pos: usize) -> (usize, usize) {
    let mut line: usize = 1;
    let mut col: usize = 1;
    let mut i: usize = 0;
    for ch in src.chars() {
        if i >= byte_pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
        i += 1;
    }
    (line, col)
}


struct Lex {
    s: Vec<char>,
    i: usize,
}
impl Lex {
    fn new(src: &str) -> Self {
        Self {
            s: src.chars().collect(),
            i: 0,
        }
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == "#".chars().next().unwrap() {
                while let Some(d) = self.bump() {
                    if d == "\n".chars().next().unwrap() { break; }
                }
                continue;
            }
            if c.is_whitespace() {
                self.i += 1;
                continue;
            }
            if c == '/' && self.s.get(self.i + 1) == Some(&'/') {
                while let Some(d) = self.peek() {
                    self.i += 1;
                    if d == '\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }
    fn next(&mut self) -> Result<Tok> {
        self.skip_ws();
        let Some(c) = self.peek() else {
            return Ok(Tok::Eof);
        };

        if is_ident_start(c) {
            let mut t = String::new();
            t.push(self.bump().unwrap());
            while let Some(d) = self.peek() {
                if is_ident_cont(d) {
                    t.push(self.bump().unwrap());
                } else {
                    break;
                }
            }
            {
                let id = t;
                let kws = [
                    "let", "in", "fn", "if", "then", "else", "import", "as", "export", "true",
                    "false", "null",
                ];
                if kws.contains(&id.as_str()) {
                    return Ok(Tok::Kw(id));
                }
                return Ok(Tok::Ident(id));
            }
        }

        if c.is_ascii_digit() {
            let mut n: i64 = 0;
            while let Some(d) = self.peek() {
                if d.is_ascii_digit() {
                    n = n * 10 + (d as i64 - '0' as i64);
                    self.i += 1;
                } else {
                    break;
                }
            }
            return Ok(Tok::Num(n));
        }

        if c == '"' {
            self.bump();
            let mut t = String::new();
            while let Some(d) = self.bump() {
                if d == '"' {
                    break;
                }
                if d == '\\' {
                    let e = self.bump().ok_or_else(|| anyhow!("bad escape"))?;
                    match e {
                        'n' => t.push('\n'),
                        't' => t.push('\t'),
                        '"' => t.push('"'),
                        '\\' => t.push('\\'),
                        _ => bail!("bad escape: \\{e}"),
                    }
                } else {
                    t.push(d);
                }
            }
            return Ok(Tok::Str(t));
        }

        let two = if self.i + 1 < self.s.len() {
            let mut t = String::new();
            t.push(self.s[self.i]);
            t.push(self.s[self.i + 1]);
            Some(t)
        } else {
            None
        };

        for op in ["==", "<=", ">=", "&&", "||", "->"] {
            if two.as_deref() == Some(op) {
                self.i += 2;
                return Ok(Tok::Sym(op.to_string()));
            }
        }

        let one = self.bump().unwrap();
        let sym = match one {
            '(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | '.' | '+' | '-' | '*' | '/' | '='
            | '<' | '>' => one.to_string(),
            _ => bail!("unexpected char: {one}"),
        };
        Ok(Tok::Sym(sym))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    Int,
    String,
    Bool,
    Unit,
    List(Box<Type>),
    Rec(Vec<(String, Type)>),
    Func(Vec<Type>, Box<Type>),
    Var(String),
    Named(String, Vec<Type>),
    Dynamic,
}

#[derive(Clone, Debug)]
enum Expr {
    Let(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Fn(Vec<String>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Get(Box<Expr>, String),
    List(Vec<Expr>),
    Rec(Vec<(String, Expr)>),
    Var(String),
    Int(i64),
    Bool(bool),
    Str(String),
    Null,
    Bin(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
}

#[derive(Clone, Debug)]
enum Item {
    Import(String, String),
    Let(String, Expr),
    Fn(String, Vec<(String, Option<Type>)>, Option<Type>, Expr),
    Export(Vec<String>),
    Expr(Expr),
}

struct Parser {
    toks: Vec<Tok>,
    spans: Vec<(usize, usize)>,
    file: String,
    src: String,
    i: usize,
}
impl Parser {
    fn from_src(src: &str, file: &str) -> Result<Self> {
        let mut lx = Lex::new(src);
        let mut toks = Vec::new();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        loop {
            // IMPORTANT: spans must begin at the first token byte, not at preceding whitespace/comments
            lx.skip_ws();
            let byte_start = lx.i;
            let t = lx.next()?;
            let byte_end = lx.i;
            let done = matches!(t, Tok::Eof);
            toks.push(t);
            spans.push((byte_start, byte_end));
            if done {
                break;
            }
        }
        Ok(Self {
            toks,
            spans,
            file: file.to_string(),
            src: src.to_string(),
            i: 0,
        })
    }
    fn peek(&self) -> &Tok {
        self.toks.get(self.i).unwrap_or(&Tok::Eof)
    }
    fn bump(&mut self) -> Tok {
        let t = self.peek().clone();
        self.i += 1;
        t
    }

    fn cur_span(&self) -> ErrorSpan {
        // Many parse errors are reported after we already advanced `i`
        // (e.g. via bump / expect_*). Use the previous token span when possible.
        let idx = if self.i > 0 { self.i - 1 } else { 0 };

        let (byte_start, byte_end) = self
            .spans
            .get(idx)
            .cloned()
            .unwrap_or((0usize, 0usize));

        let (line, col) = line_col_at(&self.src, byte_start);

        ErrorSpan {
            file: self.file.clone(),
            byte_start,
            byte_end,
            line,
            col,
        }
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if x == s) && {
            self.i += 1;
            true
        }
    }
    fn expect_sym(&mut self, s: &str) -> Result<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            bail!("ERROR_PARSE expected symbol {s:?}")
        }
    }
    fn eat_kw(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Tok::Kw(x) if x == s)
            || matches!(self.peek(), Tok::Ident(x) if x == s)
        {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect_kw(&mut self, s: &str) -> Result<()> {
        if self.eat_kw(s) {
            Ok(())
        } else {
            bail!("ERROR_PARSE expected keyword {s}")
        }
    }
    fn expect_ident(&mut self) -> Result<String> {
        match self.bump() {
            Tok::Ident(x) => Ok(x),
            _ => bail!("ERROR_PARSE expected identifier"),
        }
    }

    fn parse_type(&mut self) -> Result<Type> {
        match self.peek() {
            Tok::Kw(x) | Tok::Ident(x) if x == "Int" => {
                self.i += 1;
                Ok(Type::Int)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "String" => {
                self.i += 1;
                Ok(Type::String)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Bool" => {
                self.i += 1;
                Ok(Type::Bool)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Unit" => {
                self.i += 1;
                Ok(Type::Unit)
            }
            Tok::Kw(x) | Tok::Ident(x) if x == "Dynamic" => {
                self.i += 1;
                Ok(Type::Dynamic)
            }

            Tok::Kw(name) | Tok::Ident(name) => {
                let name = name.clone();
                self.i += 1;

                if name == "List" {
                    self.expect_sym("<")?;
                    let inner = self.parse_type()?;
                    self.expect_sym(">")?;
                    return Ok(Type::List(Box::new(inner)));
                }

                if name == "Rec" {
                    self.expect_sym("{")?;
                    let mut fields: Vec<(String, Type)> = Vec::new();
                    if !self.eat_sym("}") {
                        loop {
                            let k = self.expect_ident()?;
                            self.expect_sym(":")?;
                            let t = self.parse_type()?;
                            fields.push((k, t));
                            if self.eat_sym("}") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    return Ok(Type::Rec(fields));
                }

                if name == "Func" {
                    self.expect_sym("(")?;
                    let mut args: Vec<Type> = Vec::new();
                    if !self.eat_sym(")") {
                        loop {
                            let a = self.parse_type()?;
                            args.push(a);
                            if self.eat_sym(")") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    self.expect_sym("->")?;
                    let ret = self.parse_type()?;
                    return Ok(Type::Func(args, Box::new(ret)));
                }

                if self.eat_sym("<") {
                    let mut args: Vec<Type> = Vec::new();
                    if !self.eat_sym(">") {
                        loop {
                            let a = self.parse_type()?;
                            args.push(a);
                            if self.eat_sym(">") {
                                break;
                            }
                            self.expect_sym(",")?;
                        }
                    }
                    return Ok(Type::Named(name, args));
                }

                Ok(Type::Named(name, Vec::new()))
            }

            Tok::Sym(s) if s == "(" => {
                self.i += 1;
                let t = self.parse_type()?;
                self.expect_sym(")")?;
                Ok(t)
            }

            _ => bail!("ERROR_PARSE expected type"),
        }
    }

    fn parse_type_annotation(&mut self) -> Result<Option<Type>> {
        if self.eat_sym(":") {
            Ok(Some(self.parse_type()?))
        } else {
            Ok(None)
        }
    }

    fn parse_module(&mut self) -> Result<Vec<Item>> {
        let mut items = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            if self.eat_kw("import") {
                self.expect_sym("(")?;
                let p = match self.bump() {
                    Tok::Str(s) => s,
                    _ => bail!("ERROR_PARSE import() requires string"),
                };
                self.expect_sym(")")?;
                self.expect_kw("as")?;
                let alias = self.expect_ident()?;
                items.push(Item::Import(p, alias));
                continue;
            }
            if self.eat_kw("export") {
                self.expect_sym("{")?;
                let mut names = Vec::new();
                loop {
                    let n = self.expect_ident()?;
                    names.push(n);
                    if self.eat_sym("}") {
                        break;
                    }
                    self.expect_sym(",")?;
                    if self.eat_sym("}") {
                        break;
                    }
                }
                items.push(Item::Export(names));
                continue;
            }
            if self.eat_kw("fn") {
                let name = self.expect_ident()?;
                self.expect_sym("(")?;
                let mut params: Vec<(String, Option<Type>)> = Vec::new();
                if !self.eat_sym(")") {
                    loop {
                        let p = self.expect_ident()?;
                        let ann = if self.eat_sym(":") {
                            Some(self.parse_type()?)
                        } else {
                            None
                        };
                        params.push((p, ann));
                        if self.eat_sym(")") {
                            break;
                        }
                        self.expect_sym(",")?;
                    }
                }
                let ret: Option<Type> = if self.eat_sym("->") {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                self.expect_sym("{")?;
                let body = self.parse_expr()?;
                self.expect_sym("}")?;
                items.push(Item::Fn(name, params, ret, body));
                continue;
            }

            if matches!(self.peek(), Tok::Kw(s) if s == "let") {
                let __save = self.i;
                if let Ok(e) = self.parse_expr() {
                    items.push(Item::Expr(e));
                    break;
                }
                self.i = __save;

                self.expect_kw("let")?;
                let name = self.expect_ident()?;
                self.expect_sym("=")?;
                let rhs = self.parse_expr()?;
                items.push(Item::Let(name, rhs));
                continue;
            }
            let e = self.parse_expr()?;
            items.push(Item::Expr(e));
            break;
        }
        Ok(items)
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        if self.eat_kw("let") {
            let name = self.expect_ident()?;
            self.expect_sym("=")?;
            let e1 = self.parse_expr()?;
            self.expect_kw("in")?;
            let e2 = self.parse_expr()?;
            return Ok(Expr::Let(name, Box::new(e1), Box::new(e2)));
        }
        if self.eat_kw("if") {
            let c = self.parse_expr()?;
            self.expect_kw("then")?;
            let t = self.parse_expr()?;
            self.expect_kw("else")?;
            let f = self.parse_expr()?;
            return Ok(Expr::If(Box::new(c), Box::new(t), Box::new(f)));
        }
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut e = self.parse_and()?;
        while self.eat_sym("||") {
            let r = self.parse_and()?;
            e = Expr::Bin("||".to_string(), Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_and(&mut self) -> Result<Expr> {
        let mut e = self.parse_eq()?;
        while self.eat_sym("&&") {
            let r = self.parse_eq()?;
            e = Expr::Bin("&&".to_string(), Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_eq(&mut self) -> Result<Expr> {
        let mut e = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(x) if x == "==" || x == "<" || x == ">" || x == "<=" || x == ">=" => {
                    x.clone()
                }
                _ => break,
            };
            self.bump();
            let r = self.parse_add()?;
            e = Expr::Bin(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }
    fn parse_add(&mut self) -> Result<Expr> {
        let mut e = self.parse_mul()?;
        loop {
            if self.eat_sym("+") {
                let r = self.parse_mul()?;
                e = Expr::Bin("+".to_string(), Box::new(e), Box::new(r));
            } else if self.eat_sym("-") {
                let r = self.parse_mul()?;
                e = Expr::Bin("-".to_string(), Box::new(e), Box::new(r));
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn parse_mul(&mut self) -> Result<Expr> {
        let mut e = self.parse_unary()?;
        loop {
            if self.eat_sym("*") {
                let r = self.parse_unary()?;
                e = Expr::Bin("*".to_string(), Box::new(e), Box::new(r));
            } else if self.eat_sym("/") {
                let r = self.parse_unary()?;
                e = Expr::Bin("/".to_string(), Box::new(e), Box::new(r));
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn parse_unary(&mut self) -> Result<Expr> {
        if self.eat_sym("-") {
            let e = self.parse_unary()?;
            return Ok(Expr::Unary("-".to_string(), Box::new(e)));
        }
        self.parse_postfix()
    }
    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            if self.eat_sym(".") {
                let n = self.expect_ident()?;
                e = Expr::Get(Box::new(e), n);
                continue;
            }
            if self.eat_sym("(") {
                let mut args = Vec::new();
                if !self.eat_sym(")") {
                    loop {
                        let a = self.parse_expr()?;
                        args.push(a);
                        if self.eat_sym(")") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym(")") {
                            break;
                        }
                    }
                }
                e = Expr::Call(Box::new(e), args);
                continue;
            }
            break;
        }
        Ok(e)
    }
    fn parse_primary(&mut self) -> Result<Expr> {
        if self.eat_kw("fn") {
            self.expect_sym("(")?;
            let mut params = Vec::new();
            if !self.eat_sym(")") {
                loop {
                    let p = self.expect_ident()?;
                    params.push(p);
                    if self.eat_sym(")") {
                        break;
                    }
                    self.expect_sym(",")?;
                    if self.eat_sym(")") {
                        break;
                    }
                }
            }
            self.expect_sym("{")?;
            let body = self.parse_expr()?;
            self.expect_sym("}")?;
            return Ok(Expr::Fn(params, Box::new(body)));
        }

        match self.bump() {
            Tok::Num(n) => Ok(Expr::Int(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Kw(s) if s == "true" => Ok(Expr::Bool(true)),
            Tok::Kw(s) if s == "false" => Ok(Expr::Bool(false)),
            Tok::Kw(s) if s == "null" => Ok(Expr::Null),
            Tok::Ident(s) => Ok(Expr::Var(s)),
            Tok::Sym(s) if s == "(" => {
                let e = self.parse_expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            Tok::Sym(s) if s == "[" => {
                let mut xs = Vec::new();
                if !self.eat_sym("]") {
                    loop {
                        xs.push(self.parse_expr()?);
                        if self.eat_sym("]") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("]") {
                            break;
                        }
                    }
                }
                Ok(Expr::List(xs))
            }
            Tok::Sym(s) if s == "{" => {
                let mut kvs = Vec::new();
                if !self.eat_sym("}") {
                    loop {
                        let k = match self.bump() {
                            Tok::Ident(x) => x,
                            Tok::Kw(x) => x,
                            Tok::Str(x) => x,
                            _ => bail!("record key must be ident or string"),
                        };
                        self.expect_sym(":")?;
                        let v = self.parse_expr()?;
                        kvs.push((k, v));
                        if self.eat_sym("}") {
                            break;
                        }
                        self.expect_sym(",")?;
                        if self.eat_sym("}") {
                            break;
                        }
                    }
                }
                Ok(Expr::Rec(kvs))
            }
            other => {
                return Err(anyhow!(ParseError {
                    span: self.cur_span(),
                    message: format!("ERROR_PARSE unexpected token: {other:?}"),
                }));
            },
        }
    }
}

#[derive(Clone, Debug)]
enum Val {
    Int(i64),
    Bool(bool),
    Str(String),
    Null,
    List(Vec<Val>),
    Rec(BTreeMap<String, Val>),
    Func(Func),
    Builtin(Builtin),
}
#[derive(Clone, Debug)]
struct Func {
    params: Vec<String>,
    body: Expr,
    env: Env,
}
#[derive(Clone, Debug)]
enum Builtin {
    ResultOk,
    ResultAndThen,
    ListGet,
    ListSortByIntKey,
    GrowUnfoldTree,
    ImportArtifact,
    EmitArtifact,

    Emit,
    Len,
    SortInt,
    DedupeSortedInt,
    HistInt,
    Unfold,
    FlowPipe,
      StrLen,
      StrConcat,
      MapGet,
      MapSet,
      JsonEncode,
      JsonDecode,
}

#[derive(Clone, Debug)]
struct Env {
    parent: Option<Box<Env>>,
    vars: HashMap<String, Val>,
}
impl Env {
    fn new() -> Self {
        Self {
            parent: None,
            vars: HashMap::new(),
        }
    }
    fn child(&self) -> Self {
        Self {
            parent: Some(Box::new(self.clone())),
            vars: HashMap::new(),
        }
    }
    fn set(&mut self, k: String, v: Val) {
        self.vars.insert(k, v);
    }
    fn get(&self, k: &str) -> Option<Val> {
        if let Some(v) = self.vars.get(k) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.get(k))
    }
}

impl Val {
    fn to_json(&self) -> Option<J> {
        match self {
            Val::Int(n) => Some(J::Number((*n).into())),
            Val::Bool(b) => Some(J::Bool(*b)),
            Val::Str(s) => Some(J::String(s.clone())),
            Val::Null => Some(J::Null),
            Val::List(xs) => Some(J::Array(
                xs.iter().map(|x| x.to_json()).collect::<Option<Vec<_>>>()?,
            )),
            Val::Rec(m) => {
                let mut obj = Map::new();
                for (k, v) in m.iter() {
                    obj.insert(k.clone(), v.to_json()?);
                }
                Some(J::Object(obj))
            }
            Val::Func(_) | Val::Builtin(_) => None,
        }
    }
}

fn val_from_json(j: &J) -> Result<Val> {
    match j {
        J::Null => Ok(Val::Null),
        J::Bool(b) => Ok(Val::Bool(*b)),
        J::Number(n) => {
            let i = n.as_i64().ok_or_else(|| anyhow!("ERROR_RUNTIME json number not i64"))?;
            Ok(Val::Int(i))
        }
        J::String(s) => Ok(Val::Str(s.clone())),
        J::Array(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.push(val_from_json(x)?);
            }
            Ok(Val::List(out))
        }
        J::Object(m) => {
            let mut out = BTreeMap::new();
            for (k, v) in m.iter() {
                out.insert(k.clone(), val_from_json(v)?);
            }
            Ok(Val::Rec(out))
        }
    }
}

fn eval(e: &Expr, env: &mut Env, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
    match e {
        Expr::Int(n) => Ok(Val::Int(*n)),
        Expr::Bool(b) => Ok(Val::Bool(*b)),
        Expr::Str(s) => Ok(Val::Str(s.clone())),
        Expr::Null => Ok(Val::Null),
        Expr::Var(n) => env.get(n).ok_or_else(|| anyhow!("unbound var: {n}")),
        Expr::List(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.push(eval(x, env, tracer, loader)?);
            }
            Ok(Val::List(out))
        }
        Expr::Rec(kvs) => {
            let mut m = BTreeMap::new();
            for (k, v) in kvs {
                m.insert(k.clone(), eval(v, env, tracer, loader)?);
            }
            Ok(Val::Rec(m))
        }
        Expr::Get(obj, k) => {
            let o = eval(obj, env, tracer, loader)?;
            match o {
                Val::Rec(m) => m
                    .get(k)
                    .cloned()
                    .ok_or_else(|| anyhow!("EXPORT_MISSING missing field {k}")),
                _ => bail!("field access on non-record"),
            }
        }
        Expr::Let(name, e1, e2) => {
            let v1 = eval(e1, env, tracer, loader)?;
            let mut child = env.child();
            child.set(name.clone(), v1);
            eval(e2, &mut child, tracer, loader)
        }
        Expr::If(c, t, f) => {
            let cv = eval(c, env, tracer, loader)?;
            match cv {
                Val::Bool(true) => eval(t, env, tracer, loader),
                Val::Bool(false) => eval(f, env, tracer, loader),
                _ => bail!("if cond must be bool"),
            }
        }
        Expr::Fn(params, body) => Ok(Val::Func(Func {
            params: params.clone(),
            body: (*body.clone()),
            env: env.clone(),
        })),
        Expr::Unary(op, a) => {
            let v = eval(a, env, tracer, loader)?;
            match (op.as_str(), v) {
                ("-", Val::Int(n)) => Ok(Val::Int(-n)),
                _ => bail!("bad unary op"),
            }
        }
        Expr::Bin(op, a, b) => {
            let x = eval(a, env, tracer, loader)?;
            let y = eval(b, env, tracer, loader)?;
            match (op.as_str(), x, y) {
                ("+", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l + r)),
                ("-", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l - r)),
                ("*", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l * r)),
                ("/", Val::Int(l), Val::Int(r)) => Ok(Val::Int(l / r)),
                ("==", l, r) => Ok(Val::Bool(val_eq(&l, &r))),
                ("&&", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l && r)),
                ("||", Val::Bool(l), Val::Bool(r)) => Ok(Val::Bool(l || r)),
                ("<", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l < r)),
                (">", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l > r)),
                ("<=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l <= r)),
                (">=", Val::Int(l), Val::Int(r)) => Ok(Val::Bool(l >= r)),
                _ => bail!("bad binop {op}"),
            }
        }
        Expr::Call(f, args) => {
            let fv = eval(f, env, tracer, loader)?;
            let mut av = Vec::new();
            for a in args {
                av.push(eval(a, env, tracer, loader)?);
            }
            call(fv, av, tracer, loader)
        }
    }
}

fn val_eq(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Int(x), Val::Int(y)) => x == y,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        (Val::Str(x), Val::Str(y)) => x == y,
        (Val::Null, Val::Null) => true,
        (Val::List(xs), Val::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| val_eq(x, y))
        }
        (Val::Rec(xm), Val::Rec(ym)) => {
            xm.len() == ym.len()
                && xm
                    .iter()
                    .all(|(k, xv)| ym.get(k).map(|yv| val_eq(xv, yv)).unwrap_or(false))
        }
        _ => false,
    }
}

fn call(f: Val, args: Vec<Val>, tracer: &mut Tracer, loader: &mut ModuleLoader) -> Result<Val> {
    match f {
        Val::Builtin(b) => call_builtin(b, args, tracer, loader),
        Val::Func(fun) => {
            if fun.params.len() != args.len() {
                bail!("arity mismatch");
            }
            let mut e = fun.env.child();
            for (p, a) in fun.params.iter().zip(args.into_iter()) {
                e.set(p.clone(), a);
            }
            eval(&fun.body, &mut e, tracer, loader)
        }
        _ => bail!("call on non-function"),
    }
}

fn call_builtin(
    b: Builtin,
    args: Vec<Val>,
    tracer: &mut Tracer,
    loader: &mut ModuleLoader,
) -> Result<Val> {
    match b {
        Builtin::ResultOk => {
            if args.len() != 1 {
                bail!("ERROR_BADARG result.ok expects 1 arg");
            }
            let mut m = BTreeMap::new();
            m.insert("ok".to_string(), args[0].clone());
            Ok(Val::Rec(m))
        }

        Builtin::ResultAndThen => {
            if args.len() != 2 {
                bail!("ERROR_BADARG result.andThen expects 2 args");
            }
            let r = args[0].clone();
            let f = args[1].clone();

            let m = match r {
                Val::Rec(m) => m,
                _ => bail!("ERROR_BADARG result.andThen arg0 must be record"),
            };

            if let Some(v) = m.get("ok").cloned() {
                call(f, vec![v], tracer, loader)
            } else {
                Ok(Val::Rec(m))
            }
        }

        Builtin::FlowPipe => {
            if args.len() != 2 {
                bail!("ERROR_BADARG flow.pipe expects 2 args");
            }
            let mut acc = args[0].clone();
            let fs = match &args[1] {
                Val::List(xs) => xs.clone(),
                _ => bail!("ERROR_BADARG flow.pipe arg1 must be list"),
            };
            for f in fs {
                acc = call(f, vec![acc], tracer, loader)?;
            }
            Ok(acc)
        }
        Builtin::StrLen => {
            if args.len() != 1 { bail!("ERROR_RUNTIME arity"); }
            match &args[0] {
                Val::Str(s) => Ok(Val::Int(s.len() as i64)),
                _ => bail!("ERROR_RUNTIME type"),
            }
        }

        Builtin::StrConcat => {
            if args.len() != 2 { bail!("ERROR_RUNTIME arity"); }
            let a = match &args[0] { Val::Str(s) => s, _ => bail!("ERROR_RUNTIME type"), };
            let b = match &args[1] { Val::Str(s) => s, _ => bail!("ERROR_RUNTIME type"), };
            Ok(Val::Str(format!("{}{}", a, b)))
        }

        Builtin::MapGet => {
            if args.len() != 2 { bail!("ERROR_RUNTIME arity"); }
            let m = match &args[0] { Val::Rec(mm) => mm, _ => bail!("ERROR_RUNTIME type"), };
            let k = match &args[1] { Val::Str(s) => s, _ => bail!("ERROR_RUNTIME type"), };
            Ok(m.get(k).cloned().unwrap_or(Val::Null))
        }

        Builtin::MapSet => {
            if args.len() != 3 { bail!("ERROR_RUNTIME arity"); }
            let m = match &args[0] { Val::Rec(mm) => mm, _ => bail!("ERROR_RUNTIME type"), };
            let k = match &args[1] { Val::Str(s) => s, _ => bail!("ERROR_RUNTIME type"), };
            let v = args[2].clone();
            let mut out = m.clone();
            out.insert(k.clone(), v);
            Ok(Val::Rec(out))
        }

        Builtin::JsonEncode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME arity"); }
            let j = args[0].to_json().ok_or_else(|| anyhow!("ERROR_RUNTIME json encode non-jsonable"))?;
            Ok(Val::Str(serde_json::to_string(&j)?))
        }

        Builtin::JsonDecode => {
            if args.len() != 1 { bail!("ERROR_RUNTIME arity"); }
            let s = match &args[0] { Val::Str(ss) => ss, _ => bail!("ERROR_RUNTIME type"), };
            let j: J = serde_json::from_str(s)?;
            val_from_json(&j)
        }

        Builtin::ListGet => {
            if args.len() != 2 {
                bail!("ERROR_BADARG list.get expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG list.get arg0 must be list"),
            };
            let i = match &args[1] {
                Val::Int(n) => *n,
                _ => bail!("ERROR_BADARG list.get arg1 must be int"),
            };
            if i < 0 {
                bail!("ERROR_OOB list index out of bounds");
            }
            let iu = i as usize;
            if iu >= xs.len() {
                bail!("ERROR_OOB list index out of bounds");
            }
            return Ok(xs[iu].clone());
        }

        Builtin::ListSortByIntKey => {
            if args.len() != 2 {
                bail!("ERROR_BADARG sort_by_int_key expects 2 args");
            }
            let xs = match &args[0] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG sort_by_int_key arg0 must be list"),
            };
            let mut keyed: Vec<(i64, usize, Val)> = Vec::new();
            for (idx, it) in xs.into_iter().enumerate() {
                let k = match &it {
                    Val::Rec(m) => match m.get("k") {
                        Some(Val::Int(n)) => *n,
                        _ => bail!("ERROR_BADARG sort_by_int_key expects rec.k int"),
                    },
                    _ => bail!("ERROR_BADARG sort_by_int_key expects records"),
                };
                keyed.push((k, idx, it));
            }
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let out: Vec<Val> = keyed.into_iter().map(|t| t.2).collect();
            return Ok(Val::List(out));
        }

        Builtin::GrowUnfoldTree => {
            if args.len() < 2 {
                bail!("ERROR_BADARG unfold_tree expects at least 2 args");
            }
            let seed = args[0].clone();
            let depth = match &args[1] {
                Val::Rec(m) => match m.get("depth") {
                    Some(Val::Int(n)) => *n,
                    _ => 2,
                },
                _ => 2,
            };
            let mut q: std::collections::VecDeque<(Val, i64)> = std::collections::VecDeque::new();
            q.push_back((seed.clone(), 0));
            while let Some((node, d)) = q.pop_front() {
                tracer.grow_node(&node)?;
                if d >= depth {
                    continue;
                }
                let n = match &node {
                    Val::Rec(m) => match m.get("n") {
                        Some(Val::Int(x)) => *x,
                        _ => 0,
                    },
                    _ => 0,
                };
                let c1 = Val::Rec({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 1));
                    m
                });
                let c2 = Val::Rec({
                    let mut m = BTreeMap::new();
                    m.insert("n".to_string(), Val::Int(n + 2));
                    m
                });
                q.push_back((c1, d + 1));
                q.push_back((c2, d + 1));
            }
            return Ok(Val::Null);
        }

        Builtin::ImportArtifact => {
            if args.len() != 1 {
                bail!("ERROR_BADARG import_artifact expects 1 arg");
            }
            let p = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG import_artifact arg must be string"),
            };
            let bytes =
                fs::read(&p).with_context(|| format!("ERROR_IO cannot read artifact: {p}"))?;
            let cid = sha256_bytes(&bytes);
            tracer.artifact_in(&p, &cid)?;
            let out: Vec<Val> = bytes.into_iter().map(|b| Val::Int(b as i64)).collect();
            return Ok(Val::List(out));
        }

        Builtin::EmitArtifact => {
            if args.len() != 2 {
                bail!("ERROR_BADARG emit_artifact expects 2 args");
            }
            let name = match &args[0] {
                Val::Str(s) => s.clone(),
                _ => bail!("ERROR_BADARG emit_artifact name must be string"),
            };
            let xs = match &args[1] {
                Val::List(v) => v.clone(),
                _ => bail!("ERROR_BADARG emit_artifact bytes must be list"),
            };
            let mut bytes: Vec<u8> = Vec::with_capacity(xs.len());
            for v in xs {
                let n = match v {
                    Val::Int(i) => i,
                    _ => bail!("ERROR_BADARG emit_artifact bytes must be ints"),
                };
                if n < 0 || n > 255 {
                    bail!("ERROR_BADARG emit_artifact byte out of range");
                }
                bytes.push(n as u8);
            }
            let cid = sha256_bytes(&bytes);
            tracer.artifact_out(&name, &cid, &bytes)?;
            return Ok(Val::Null);
        }
        Builtin::Emit => {
            if args.len() != 1 {
                bail!("emit arity");
            }
            let j = args[0]
                .to_json()
                .ok_or_else(|| anyhow!("emit arg must be jsonable"))?;
            tracer.emit(&j)?;
            Ok(Val::Null)
        }
        Builtin::Len => {
            if args.len() != 1 {
                bail!("len arity");
            }
            match &args[0] {
                Val::List(xs) => Ok(Val::Int(xs.len() as i64)),
                _ => bail!("len expects list"),
            }
        }
        Builtin::SortInt => {
            if args.len() != 1 {
                bail!("sort_int arity");
            }
            let mut xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("sort_int expects list"),
            };
            let mut ns: Vec<i64> = Vec::new();
            for v in xs.drain(..) {
                match v {
                    Val::Int(n) => ns.push(n),
                    _ => bail!("sort_int expects ints"),
                }
            }
            insertion_sort(&mut ns);
            Ok(Val::List(ns.into_iter().map(Val::Int).collect()))
        }
        Builtin::DedupeSortedInt => {
            if args.len() != 1 {
                bail!("dedupe_sorted_int arity");
            }
            let xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("dedupe_sorted_int expects list"),
            };
            let mut out: Vec<i64> = Vec::new();
            let mut last: Option<i64> = None;
            for v in xs {
                let n = match v {
                    Val::Int(n) => n,
                    _ => bail!("dedupe_sorted_int expects ints"),
                };
                if last.map(|x| x == n).unwrap_or(false) {
                    continue;
                }
                last = Some(n);
                out.push(n);
            }
            Ok(Val::List(out.into_iter().map(Val::Int).collect()))
        }
        Builtin::HistInt => {
            if args.len() != 1 {
                bail!("hist_int arity");
            }
            let xs = match args[0].clone() {
                Val::List(v) => v,
                _ => bail!("hist_int expects list"),
            };

            let mut m: BTreeMap<i64, i64> = BTreeMap::new();
            for v in xs {
                let n = match v {
                    Val::Int(n) => n,
                    _ => bail!("hist_int expects ints"),
                };
                *m.entry(n).or_insert(0) += 1;
            }

            let mut out_list: Vec<Val> = Vec::new();
            for (v, c) in m {
                let mut rec = BTreeMap::new();
                rec.insert("v".to_string(), Val::Int(v));
                rec.insert("count".to_string(), Val::Int(c));
                out_list.push(Val::Rec(rec));
            }
            Ok(Val::List(out_list))
        }
        Builtin::Unfold => {
            if args.len() != 3 {
                bail!("unfold arity");
            }
            let mut seed = args[0].clone();
            let fuel = match &args[1] {
                Val::Int(n) => *n,
                Val::Rec(m) => {
                    if let Some(Val::Int(n)) = m.get("fuel") {
                        *n
                    } else if let Some(Val::Int(n)) = m.get("steps") {
                        *n
                    } else {
                        bail!("unfold opts must include fuel:int or steps:int");
                    }
                }
                _ => bail!("unfold opts must be record or int"),
            };
            let step = args[2].clone();

            let mut out = Vec::new();
            let mut k = 0i64;
            while k < fuel {
                let r = call(step.clone(), vec![seed.clone()], tracer, loader)?;
                match r {
                    Val::Null => break,
                    Val::Rec(m) => {
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Rec(mm)
                        } else {
                            bail!("unfold step missing seed/i");
                        };
                        let val = m
                            .get("value")
                            .or_else(|| m.get("out"))
                            .cloned()
                            .ok_or_else(|| anyhow!("unfold step missing value/out"))?;
                        out.push(val);
                        seed = next_seed;
                    }
                    Val::List(xs) => {
                        if xs.is_empty() {
                            break;
                        }
                        let m = match &xs[0] {
                            Val::Rec(m) => m,
                            _ => bail!("unfold step list must contain record"),
                        };
                        let next_seed = if let Some(v) = m.get("seed").cloned() {
                            v
                        } else if let Some(v) = m.get("i").cloned() {
                            let mut mm = BTreeMap::new();
                            mm.insert("i".to_string(), v);
                            Val::Rec(mm)
                        } else {
                            bail!("unfold step missing seed/i");
                        };
                        let val = m
                            .get("value")
                            .or_else(|| m.get("out"))
                            .cloned()
                            .ok_or_else(|| anyhow!("unfold step missing value/out"))?;
                        out.push(val);
                        seed = next_seed;
                    }
                    _ => bail!("unfold step must return record, list, or null"),
                }
                k += 1;
            }
            Ok(Val::List(out))
        }
    }
}

fn insertion_sort(xs: &mut [i64]) {
    for i in 1..xs.len() {
        let key = xs[i];
        let mut j = i;
        while j > 0 && xs[j - 1] > key {
            xs[j] = xs[j - 1];
            j -= 1;
        }
        xs[j] = key;
    }
}

struct Lockfile {
    modules: HashMap<String, String>,
}
impl Lockfile {
    fn load(p: &Path) -> Result<Self> {
        let bytes = match fs::read(p) {
            Ok(b) => b,
            Err(e) => {
                // If the program has chdir’d (e.g., into --out), a relative lock path
                // like "fard.lock.json" will fail. Retry against the shell’s original PWD.
                if e.kind() == std::io::ErrorKind::NotFound && !p.is_absolute() {
                    if let Ok(pwd) = std::env::var("PWD") {
                        let alt = std::path::Path::new(&pwd).join(p);
                        fs::read(&alt).with_context(|| {
                            format!(
                                "missing lock file: {} (also tried {})",
                                p.display(),
                                alt.display()
                            )
                        })?
                    } else {
                        return Err(e)
                            .with_context(|| format!("missing lock file: {}", p.display()));
                    }
                } else {
                    return Err(e).with_context(|| format!("missing lock file: {}", p.display()));
                }
            }
        };
        let v: J = serde_json::from_slice(&bytes)?;
        let mut modules: HashMap<String, String> = HashMap::new();

        // Accept either:
        //  (A) object map:
        //      "modules": { "spec": "sha256:..." }
        //      "modules": { "spec": { "digest": "sha256:..." } }
        //  (B) array of entries (fixture format):
        //      "modules": [ { "spec": "...", "digest": "...", "path": "..." }, ... ]
        if let Some(ms) = v.get("modules").and_then(|x| x.as_object()) {
            for (k, vv) in ms {
                let dig = if let Some(s) = vv.as_str() {
                    s.to_string()
                } else {
                    vv.get("digest")
                        .and_then(|x| x.as_str())
                        .ok_or_else(|| {
                            anyhow!(
                                "ERROR_LOCK modules values must be string. expected string or object with digest key"
                            )
                        })?
                        .to_string()
                };
                if dig.is_empty() {
                    bail!("ERROR_LOCK modules digest empty");
                }
                modules.insert(k.clone(), dig);
            }
        } else if let Some(arr) = v.get("modules").and_then(|x| x.as_array()) {
            for it in arr {
                let spec = it
                    .get("spec")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing spec"))?;
                let dig = it
                    .get("digest")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("ERROR_LOCK modules array entry missing digest"))?;
                if dig.is_empty() {
                    bail!("ERROR_LOCK modules digest empty");
                }
                modules.insert(spec.to_string(), dig.to_string());
            }
        }

        Ok(Self { modules })
    }
    fn expected(&self, k: &str) -> Option<&str> {
        self.modules.get(k).map(|s| s.as_str())
    }
}

struct ModuleLoader {
    root_dir: PathBuf,
    registry_dir: Option<PathBuf>,
    cache: HashMap<String, BTreeMap<String, Val>>,
    stack: Vec<String>,
    lock: Option<Lockfile>,
}
impl ModuleLoader {
    fn new(root: &Path) -> Self {
        Self {
            root_dir: root.to_path_buf(),
            registry_dir: None,
            cache: HashMap::new(),
            stack: Vec::new(),
            lock: None,
        }
    }

    fn eval_main(&mut self, main_path: &Path, tracer: &mut Tracer) -> Result<Val> {
        let src = fs::read_to_string(main_path)
            .with_context(|| format!("missing main program file: {}", main_path.display()))?;
          let file = main_path.to_string_lossy().to_string();
          let mut p = Parser::from_src(&src, &file)?;
        let items = p.parse_module()?;
        let mut env = base_env();
        let here_dir = main_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.root_dir.clone());
          let v = self.eval_items(items, &mut env, tracer, &here_dir)?;
          let mut mg = Map::new();
          mg.insert(
              "nodes".to_string(),
              J::Array(vec![J::String(main_path.to_string_lossy().to_string())]),
          );
          fs::write(
              tracer.out_dir.join("module_graph.json"),
              serde_json::to_vec(&J::Object(mg))?,
          )?;
          Ok(v)
    }

    fn eval_items(
        &mut self,
        items: Vec<Item>,
        env: &mut Env,
        tracer: &mut Tracer,
        here: &Path,
    ) -> Result<Val> {
        let mut exports: Option<Vec<String>> = None;
        let mut last: Val = Val::Null;

        for it in items {
            match it {
                Item::Import(path, alias) => {
                    let ex = self.load_module(&path, here, tracer)?;
                    env.set(alias, Val::Rec(ex));
                }
                Item::Let(name, rhs) => {
                    let v = eval(&rhs, env, tracer, self)?;
                    env.set(name, v);
                }
                Item::Fn(name, params, _ret, body) => {
                    let f = Val::Func(Func {
                        params: params.into_iter().map(|(n, _)| n).collect(),
                        body,
                        env: env.clone(),
                    });
                    env.set(name, f);
                }
                Item::Export(ns) => exports = Some(ns),
                Item::Expr(e) => last = eval(&e, env, tracer, self)?,
            }
        }

        if let Some(ns) = exports {
            let mut out = BTreeMap::new();
            for n in ns {
                let v = env
                    .get(&n)
                    .ok_or_else(|| anyhow!("export missing name: {n}"))?;
                out.insert(n, v);
            }
            return Ok(Val::Rec(out));
        }

        Ok(last)
    }

    fn load_module(
        &mut self,
        name: &str,
        here: &Path,
        tracer: &mut Tracer,
    ) -> Result<BTreeMap<String, Val>> {
        if let Some(c) = self.cache.get(name) {
            return Ok(c.clone());
        }

        if self.stack.contains(&name.to_string()) {
            bail!("IMPORT_CYCLE cycle detected in imports at {name}");
        }
        self.stack.push(name.to_string());

        let exports = if name.starts_with("std/") {
            let ex = self.builtin_std(name)?;
            self.check_lock(name, &self.builtin_digest(name))?;
            ex
        } else if name.starts_with("pkg:") || name.starts_with("pkg/") {
            // pkg imports require a lock (determinism), and require an explicit registry root.
            if self.lock.is_none() {
                eprintln!("IMPORT_PKG_REQUIRES_LOCK");
                bail!("ERROR_LOCK missing lock for pkg import: {name}");
            }

            let reg = self
                .registry_dir
                .as_ref()
                .ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;

            let spec = if let Some(s) = name.strip_prefix("pkg:") {
                s
            } else if let Some(s) = name.strip_prefix("pkg/") {
                s
            } else {
                name
            };

            let (pkg, rest) = spec
                .split_once("@")
                .ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;
            let (ver, mod_id) = rest
                .split_once("/")
                .ok_or_else(|| anyhow!("ERROR_RUNTIME bad pkg import: {name}"))?;

            let base = reg.join("pkgs").join(pkg).join(ver);

              let pkg_json_path = base.join("package.json");

              // Prefer explicit entrypoints when package.json exists; otherwise fall back to "<mod_id>.fard".
              let rel: String = if let Ok(pkg_json_bytes) = fs::read(&pkg_json_path) {
                  let pkg_json: J = serde_json::from_slice(&pkg_json_bytes)
                      .with_context(|| format!("bad json: {}", pkg_json_path.display()))?;

                  let entrypoints = pkg_json
                      .get("entrypoints")
                      .and_then(|x| x.as_object())
                      .ok_or_else(|| anyhow!("ERROR_RUNTIME package.json missing entrypoints"))?;

                  entrypoints
                      .get(mod_id)
                      .and_then(|x| x.as_str())
                      .ok_or_else(|| anyhow!("ERROR_RUNTIME missing entrypoint {mod_id} in package.json"))?
                      .to_string()
              } else {
                  format!("{mod_id}.fard")
              };

              let path = base.join("files").join(&rel);
            let src = fs::read_to_string(&path)
                .with_context(|| format!("missing module file: {}", path.display()))?;

            self.check_lock(name, &file_digest(&path)?)?;

            let file = path.to_string_lossy().to_string();
            let mut p = Parser::from_src(&src, &file)?;
            let items = p.parse_module()?;
            let mut env = base_env();
            let v = self.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
            match v {
                Val::Rec(m) => m,
                _ => bail!("module must export a record"),
            }
        } else if name.starts_with("registry/") {
            let reg = self
                .registry_dir
                .as_ref()
                .ok_or_else(|| anyhow!("ERROR_REGISTRY missing --registry"))?;
            let rest = name.strip_prefix("registry/").unwrap_or(name);

            let path = reg.join(format!("{rest}.fard"));
            let src = fs::read_to_string(&path)
                .with_context(|| { if path.to_string_lossy().contains("/pkg/") { eprintln!("IMPORT_PKG_REQUIRES_LOCK"); } format!("missing module file: {}", path.display()) })?;

            self.check_lock(name, &file_digest(&path)?)?;

          let file = path.to_string_lossy().to_string();
          let mut p = Parser::from_src(&src, &file)?;
            let items = p.parse_module()?;
            let mut env = base_env();
            let v = self.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
            match v {
                Val::Rec(m) => m,
                _ => bail!("module must export a record"),
            }
        } else {
            let base: &Path = if name.starts_with("lib/") {
                self.root_dir.as_path()
            } else {
                here
            };

            let path = base.join(format!("{name}.fard"));
            let src = fs::read_to_string(&path)
                .with_context(|| { if path.to_string_lossy().contains("/pkg/") { eprintln!("IMPORT_PKG_REQUIRES_LOCK"); } format!("missing module file: {}", path.display()) })?;

            self.check_lock(name, &file_digest(&path)?)?;

          let file = path.to_string_lossy().to_string();
          let mut p = Parser::from_src(&src, &file)?;
            let items = p.parse_module()?;
            let mut env = base_env();
            let v = self.eval_items(items, &mut env, tracer, path.parent().unwrap_or(here))?;
            match v {
                Val::Rec(m) => m,
                _ => bail!("module must export a record"),
            }
        };
        self.stack.pop();
        self.cache.insert(name.to_string(), exports.clone());
        Ok(exports)
    }

    fn check_lock(&self, module: &str, got: &str) -> Result<()> {
        if let Some(lk) = &self.lock {
            if let Some(exp) = lk.expected(module) {
                if exp == "sha256:0000000000000000000000000000000000000000000000000000000000000000" {
                    // wildcard digest: lock is required, but digest is intentionally unset
                } else if exp != got {
                    bail!("LOCK_MISMATCH lock mismatch for module {module}: expected {exp}, got {got}");
                }
            }
        }
        Ok(())
    }

    fn builtin_std(&self, name: &str) -> Result<BTreeMap<String, Val>> {
        match name {
            "std/list" => {
                let mut m = BTreeMap::new();
                m.insert("get".to_string(), Val::Builtin(Builtin::ListGet));
                m.insert(
                    "sort_by_int_key".to_string(),
                    Val::Builtin(Builtin::ListSortByIntKey),
                );
                m.insert("sort_int".to_string(), Val::Builtin(Builtin::SortInt));
                m.insert(
                    "dedupe_sorted_int".to_string(),
                    Val::Builtin(Builtin::DedupeSortedInt),
                );
                m.insert("hist_int".to_string(), Val::Builtin(Builtin::HistInt));
                Ok(m)
            }

            "std/result" => {
                let mut m = BTreeMap::new();
                m.insert("ok".to_string(), Val::Builtin(Builtin::ResultOk));
                m.insert("andThen".to_string(), Val::Builtin(Builtin::ResultAndThen));
                Ok(m)
            }

            "std/grow" => {
                let mut m = BTreeMap::new();
                m.insert(
                    "unfold_tree".to_string(),
                    Val::Builtin(Builtin::GrowUnfoldTree),
                );
                m.insert("unfold".to_string(), Val::Builtin(Builtin::Unfold));
                Ok(m)
            }
              "std/flow" => {
                  let mut m = BTreeMap::new();
                  m.insert("pipe".to_string(), Val::Builtin(Builtin::FlowPipe));
                  Ok(m)
              }

              "std/str" => {
                  let mut m = BTreeMap::new();
                  m.insert("len".to_string(), Val::Builtin(Builtin::StrLen));
                  m.insert("concat".to_string(), Val::Builtin(Builtin::StrConcat));
                  Ok(m)
              }

              "std/map" => {
                  let mut m = BTreeMap::new();
                  m.insert("get".to_string(), Val::Builtin(Builtin::MapGet));
                  m.insert("set".to_string(), Val::Builtin(Builtin::MapSet));
                  Ok(m)
              }

              "std/json" => {
                  let mut m = BTreeMap::new();
                  m.insert("encode".to_string(), Val::Builtin(Builtin::JsonEncode));
                  m.insert("decode".to_string(), Val::Builtin(Builtin::JsonDecode));
                  Ok(m)
              }
            _ => bail!("unknown std module: {name}"),
        }
    }

    fn builtin_digest(&self, name: &str) -> String {
        let mut h = Sha256::new();
        h.update(format!("builtin:{name}:v0.5").as_bytes());
        format!("sha256:{:x}", h.finalize())
    }
}

fn base_env() -> Env {
    let mut e = Env::new();
    e.set("emit".to_string(), Val::Builtin(Builtin::Emit));
    e.set("len".to_string(), Val::Builtin(Builtin::Len));
    e.set(
        "import_artifact".to_string(),
        Val::Builtin(Builtin::ImportArtifact),
    );
    e.set(
        "emit_artifact".to_string(),
        Val::Builtin(Builtin::EmitArtifact),
    );
    e.set(
        "import_artifact".to_string(),
        Val::Builtin(Builtin::ImportArtifact),
    );
    e.set(
        "emit_artifact".to_string(),
        Val::Builtin(Builtin::EmitArtifact),
    );
    e
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

fn file_digest(p: &Path) -> Result<String> {
    let b = fs::read(p)?;
    let mut h = Sha256::new();
    h.update(&b);
    Ok(format!("sha256:{:x}", h.finalize()))
}
