//! fardcheck — best-effort type checker for FARD programs
//! Reports definite type errors in pure expressions.
//! Imports and builtins are typed as Dynamic (propagates without error).

use anyhow::{bail, Result};
use std::collections::HashMap;

// ── Types ────────────────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Int,
    Float,
    Bool,
    Str,
    Null,
    List(Box<Ty>),
    Rec(Vec<(String, Ty)>),
    Func(Vec<Ty>, Box<Ty>),
    Var(u32),       // unification variable
    Dynamic,        // unknown / from import — no errors propagate through this
}

/// A polymorphic type scheme: ∀(vars). ty
/// vars are the generalized type variables
#[derive(Clone, Debug)]
struct Scheme {
    vars: Vec<u32>,
    ty: Ty,
}

impl Scheme {
    fn mono(ty: Ty) -> Self { Scheme { vars: vec![], ty } }
}

impl Ty {
    fn display(&self) -> String {
        match self {
            Ty::Int => "Int".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Str => "Text".to_string(),
            Ty::Null => "Null".to_string(),
            Ty::Dynamic => "?".to_string(),
            Ty::Var(n) => format!("t{}", n),
            Ty::List(inner) => format!("List({})", inner.display()),
            Ty::Func(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| p.display()).collect();
                format!("({}) -> {}", ps.join(", "), ret.display())
            }
            Ty::Rec(fields) => {
                let fs: Vec<String> = fields.iter().map(|(k, v)| format!("{}: {}", k, v.display())).collect();
                format!("{{ {} }}", fs.join(", "))
            }
        }
    }

    fn free_vars(&self) -> Vec<u32> {
        match self {
            Ty::Var(n) => vec![*n],
            Ty::List(inner) => inner.free_vars(),
            Ty::Func(params, ret) => {
                let mut vs: Vec<u32> = params.iter().flat_map(|p| p.free_vars()).collect();
                vs.extend(ret.free_vars());
                vs.sort(); vs.dedup();
                vs
            }
            Ty::Rec(fields) => {
                let mut vs: Vec<u32> = fields.iter().flat_map(|(_, v)| v.free_vars()).collect();
                vs.sort(); vs.dedup();
                vs
            }
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug)]
struct TyError {
    msg: String,
    line: usize,
}

// ── Unification ───────────────────────────────────────────────────────────────
struct Subst(HashMap<u32, Ty>);

impl Subst {
    fn new() -> Self { Subst(HashMap::new()) }

    fn apply(&self, t: &Ty) -> Ty {
        match t {
            Ty::Var(n) => {
                if let Some(t2) = self.0.get(n) {
                    self.apply(t2)
                } else {
                    t.clone()
                }
            }
            Ty::List(inner) => Ty::List(Box::new(self.apply(inner))),
            Ty::Func(ps, r) => Ty::Func(
                ps.iter().map(|p| self.apply(p)).collect(),
                Box::new(self.apply(r)),
            ),
            Ty::Rec(fields) => Ty::Rec(
                fields.iter().map(|(k, v)| (k.clone(), self.apply(v))).collect(),
            ),
            other => other.clone(),
        }
    }

    fn bind(&mut self, n: u32, t: Ty) {
        self.0.insert(n, t);
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> Result<()> {
        let a = self.apply(a);
        let b = self.apply(b);
        match (&a, &b) {
            (Ty::Dynamic, _) | (_, Ty::Dynamic) => Ok(()),
            (Ty::Var(n), t) | (t, Ty::Var(n)) => {
                let n = *n;
                if !self.occurs(n, t) {
                    self.bind(n, t.clone());
                }
                Ok(())
            }
            (Ty::Int, Ty::Int) | (Ty::Float, Ty::Float) |
            (Ty::Bool, Ty::Bool) | (Ty::Str, Ty::Str) |
            (Ty::Null, Ty::Null) => Ok(()),
            (Ty::List(a), Ty::List(b)) => self.unify(a, b),
            (Ty::Func(ap, ar), Ty::Func(bp, br)) => {
                if ap.len() != bp.len() {
                    bail!("arity mismatch: expected {} params, got {}", ap.len(), bp.len());
                }
                for (a, b) in ap.iter().zip(bp.iter()) {
                    self.unify(a, b)?;
                }
                self.unify(ar, br)
            }
            _ => bail!("type mismatch: {} vs {}", a.display(), b.display()),
        }
    }

    fn occurs(&self, n: u32, t: &Ty) -> bool {
        match t {
            Ty::Var(m) => *m == n,
            Ty::List(inner) => self.occurs(n, inner),
            Ty::Func(ps, r) => ps.iter().any(|p| self.occurs(n, p)) || self.occurs(n, r),
            Ty::Rec(fs) => fs.iter().any(|(_, v)| self.occurs(n, v)),
            _ => false,
        }
    }
}

// ── Checker ───────────────────────────────────────────────────────────────────
struct Checker {
    subst: Subst,
    next_var: u32,
    errors: Vec<TyError>,
    env: Vec<HashMap<String, Scheme>>,
}

impl Checker {
    fn new() -> Self {
        Checker {
            subst: Subst::new(),
            next_var: 0,
            errors: Vec::new(),
            env: vec![HashMap::new()],
        }
    }

    fn fresh(&mut self) -> Ty {
        let n = self.next_var;
        self.next_var += 1;
        Ty::Var(n)
    }

    fn push(&mut self) { self.env.push(HashMap::new()); }
    fn pop(&mut self) { self.env.pop(); }

    fn define(&mut self, name: &str, ty: Ty) {
        self.define_scheme(name, Scheme::mono(ty));
    }

    fn define_scheme(&mut self, name: &str, scheme: Scheme) {
        if let Some(top) = self.env.last_mut() {
            top.insert(name.to_string(), scheme);
        }
    }

    fn lookup(&mut self, name: &str) -> Ty {
        let scheme = self.env.iter().rev()
            .find_map(|scope| scope.get(name))
            .cloned();
        match scheme {
            Some(s) => self.instantiate_scheme(&s),
            None => Ty::Dynamic,
        }
    }

    fn instantiate_scheme(&mut self, scheme: &Scheme) -> Ty {
        if scheme.vars.is_empty() {
            return scheme.ty.clone();
        }
        let mut inst_subst = Subst::new();
        for &v in &scheme.vars {
            let fresh = self.fresh();
            inst_subst.bind(v, fresh);
        }
        inst_subst.apply(&scheme.ty)
    }

    fn generalize(&self, ty: &Ty) -> Scheme {
        // Collect free vars in ty that are NOT free in the env
        let ty_vars = self.subst.apply(ty).free_vars();
        let env_vars = self.env_free_vars();
        let generalized: Vec<u32> = ty_vars.into_iter()
            .filter(|v| !env_vars.contains(v))
            .collect();
        Scheme { vars: generalized, ty: self.subst.apply(ty) }
    }

    fn env_free_vars(&self) -> Vec<u32> {
        let mut vars = Vec::new();
        for scope in &self.env {
            for scheme in scope.values() {
                let applied = self.subst.apply(&scheme.ty);
                vars.extend(applied.free_vars());
            }
        }
        vars.sort(); vars.dedup();
        vars
    }

    fn err(&mut self, msg: String, line: usize) {
        self.errors.push(TyError { msg, line });
    }

    fn unify_or_err(&mut self, a: &Ty, b: &Ty, line: usize, ctx: &str) {
        let a = self.subst.apply(a);
        let b = self.subst.apply(b);
        if let Err(e) = self.subst.unify(&a, &b) {
            self.err(format!("{ctx}: {e}"), line);
        }
    }
}

// ── Simple line-tracking tokenizer ───────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
enum Token {
    // literals
    Int(i64), Float(f64), Bool(bool), Str(String), Null,
    // identifiers / keywords
    Ident(String),
    // punctuation
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Dot, Comma, Colon, Eq, Arrow, FatArrow,
    Plus, Minus, Star, Slash, Percent,
    EqEq, BangEq, Lt, Gt, LtEq, GtEq,
    AmpAmp, PipePipe, Bang,
    // keywords
    Let, In, Fn, If, Then, Else, Import, As, Match, While, Return, Test,
    // misc
    Eof,
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer { chars: src.chars().collect(), pos: 0, line: 1 }
    }

    fn peek(&self) -> char { self.chars.get(self.pos).copied().unwrap_or('\0') }
    fn advance(&mut self) -> char {
        let c = self.peek();
        self.pos += 1;
        if c == '\n' { self.line += 1; }
        c
    }

    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                ' ' | '\t' | '\r' | '\n' => { self.advance(); }
                '/' if self.chars.get(self.pos+1) == Some(&'/') => {
                    while self.peek() != '\n' && self.peek() != '\0' { self.advance(); }
                }
                _ => break,
            }
        }
    }

    fn next_tok(&mut self) -> (Token, usize) {
        self.skip_ws();
        let line = self.line;
        let c = self.peek();
        if c == '\0' { return (Token::Eof, line); }
        self.advance();
        let tok = match c {
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '.' => Token::Dot,
            ',' => Token::Comma,
            ':' => Token::Colon,
            '+' => Token::Plus,
            '*' => Token::Star,
            '%' => Token::Percent,
            '-' => if self.peek() == '>' { self.advance(); Token::Arrow } else { Token::Minus },
            '/' => Token::Slash,
            '=' => if self.peek() == '=' { self.advance(); Token::EqEq }
                   else if self.peek() == '>' { self.advance(); Token::FatArrow }
                   else { Token::Eq },
            '!' => if self.peek() == '=' { self.advance(); Token::BangEq } else { Token::Bang },
            '<' => if self.peek() == '=' { self.advance(); Token::LtEq } else { Token::Lt },
            '>' => if self.peek() == '=' { self.advance(); Token::GtEq } else { Token::Gt },
            '&' => { if self.peek() == '&' { self.advance(); } Token::AmpAmp },
            '|' => { if self.peek() == '|' { self.advance(); } Token::PipePipe },
            '"' => {
                let mut s = String::new();
                while self.peek() != '"' && self.peek() != '\0' {
                    let ch = self.advance();
                    if ch == '\\' { self.advance(); } // skip escape
                    else if ch == '$' && self.peek() == '{' {
                        // skip interpolation
                        self.advance();
                        let mut depth = 1;
                        while depth > 0 && self.peek() != '\0' {
                            match self.advance() {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                _ => {}
                            }
                        }
                    } else {
                        s.push(ch);
                    }
                }
                self.advance(); // closing "
                Token::Str(s)
            }
            '`' => {
                while self.peek() != '`' && self.peek() != '\0' { self.advance(); }
                self.advance();
                Token::Str(String::new())
            }
            _ if c.is_ascii_digit() => {
                let mut s = String::from(c);
                while self.peek().is_ascii_digit() { s.push(self.advance()); }
                if self.peek() == '.' && self.chars.get(self.pos+1).map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    s.push(self.advance());
                    while self.peek().is_ascii_digit() { s.push(self.advance()); }
                    Token::Float(s.parse().unwrap_or(0.0))
                } else {
                    Token::Int(s.parse().unwrap_or(0))
                }
            }
            _ if c.is_alphabetic() || c == '_' => {
                let mut s = String::from(c);
                while self.peek().is_alphanumeric() || self.peek() == '_' { s.push(self.advance()); }
                match s.as_str() {
                    "let" => Token::Let,
                    "in" => Token::In,
                    "fn" => Token::Fn,
                    "if" => Token::If,
                    "then" => Token::Then,
                    "else" => Token::Else,
                    "true" => Token::Bool(true),
                    "false" => Token::Bool(false),
                    "null" => Token::Null,
                    "import" => Token::Import,
                    "as" => Token::As,
                    "match" => Token::Match,
                    "while" => Token::While,
                    "return" => Token::Return,
                    "test" => Token::Test,
                    _ => Token::Ident(s),
                }
            }
            _ => Token::Ident(String::from(c)),
        };
        (tok, line)
    }

    fn tokenize(&mut self) -> Vec<(Token, usize)> {
        let mut toks = Vec::new();
        loop {
            let t = self.next_tok();
            let eof = t.0 == Token::Eof;
            toks.push(t);
            if eof { break; }
        }
        toks
    }
}

// ── Parser (produces typed AST nodes inline) ─────────────────────────────────
struct Parser {
    toks: Vec<(Token, usize)>,
    pos: usize,
}

#[derive(Clone, Debug)]
enum Expr {
    Int(i64, usize),
    Float(f64, usize),
    Bool(bool, usize),
    Str(usize),
    Null(usize),
    Var(String, usize),
    List(Vec<Expr>, usize),
    Rec(Vec<(String, Expr)>, usize),
    Get(Box<Expr>, String, usize),
    Index(Box<Expr>, Box<Expr>, usize),
    Call(Box<Expr>, Vec<Expr>, usize),
    Fn(Vec<String>, Box<Expr>, usize),
    Let(String, Box<Expr>, Box<Expr>, usize),
    If(Box<Expr>, Box<Expr>, Box<Expr>, usize),
    Bin(String, Box<Expr>, Box<Expr>, usize),
    Unary(String, Box<Expr>, usize),
    Block(Vec<(String, Expr)>, Box<Expr>, usize), // let chain ending in expr
}

impl Parser {
    fn new(toks: Vec<(Token, usize)>) -> Self { Parser { toks, pos: 0 } }

    fn peek(&self) -> &Token { &self.toks[self.pos.min(self.toks.len()-1)].0 }
    fn line(&self) -> usize { self.toks[self.pos.min(self.toks.len()-1)].1 }

    fn eat(&mut self) -> &Token {
        let p = self.pos;
        if self.pos < self.toks.len() - 1 { self.pos += 1; }
        &self.toks[p].0
    }

    fn expect(&mut self, t: &Token) -> Result<()> {
        if self.peek() == t { self.eat(); Ok(()) }
        else { bail!("expected {:?}, got {:?} at line {}", t, self.peek(), self.line()) }
    }

    fn parse_program(&mut self) -> Vec<TopItem> {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                Token::Eof => break,
                Token::Import => { self.eat(); self.skip_import(); }
                Token::Test => { self.eat(); items.push(self.parse_test()); }
                Token::Let => {
                    self.eat();
                    if let Ok(item) = self.parse_top_let() {
                        items.push(item);
                    }
                }
                Token::Fn => {
                    self.eat();
                    if let Ok(item) = self.parse_top_fn() {
                        items.push(item);
                    }
                }
                _ => { self.eat(); } // skip unknown top-level tokens
            }
        }
        items
    }

    fn skip_import(&mut self) {
        // import("...") as name  — skip to past 'as name'
        while !matches!(self.peek(), Token::As | Token::Eof) { self.eat(); }
        if self.peek() == &Token::As { self.eat(); self.eat(); } // as <name>
    }

    fn parse_test(&mut self) -> TopItem {
        let line = self.line();
        // test "label" { expr }
        let label = match self.eat().clone() {
            Token::Str(s) => s,
            _ => String::new(),
        };
        let _ = self.expect(&Token::LBrace);
        let expr = self.parse_expr();
        let _ = self.expect(&Token::RBrace);
        TopItem::Test(label, expr, line)
    }

    fn parse_top_let(&mut self) -> Result<TopItem> {
        let line = self.line();
        let name = match self.eat().clone() {
            Token::Ident(s) => s,
            t => bail!("expected ident, got {:?}", t),
        };
        self.expect(&Token::Eq)?;
        let expr = self.parse_expr();
        Ok(TopItem::Let(name, expr, line))
    }

    fn parse_top_fn(&mut self) -> Result<TopItem> {
        let line = self.line();
        let name = match self.eat().clone() {
            Token::Ident(s) => s,
            t => bail!("expected ident, got {:?}", t),
        };
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        while self.peek() != &Token::RParen && self.peek() != &Token::Eof {
            if let Token::Ident(p) = self.eat().clone() { params.push(p); }
            if self.peek() == &Token::Comma { self.eat(); }
        }
        self.expect(&Token::RParen)?;
        self.expect(&Token::LBrace)?;
        let body = self.parse_block_expr();
        self.expect(&Token::RBrace)?;
        Ok(TopItem::Fn(name, params, body, line))
    }

    fn parse_block_expr(&mut self) -> Expr {
        // sequence of let-bindings ending in an expression
        let line = self.line();
        let mut bindings: Vec<(String, Expr)> = Vec::new();
        loop {
            if self.peek() == &Token::Let {
                self.eat();
                let name = match self.eat().clone() {
                    Token::Ident(s) => s,
                    _ => "_".to_string(),
                };
                // skip optional pattern, expect =
                while !matches!(self.peek(), Token::Eq | Token::Eof) { self.eat(); }
                self.eat(); // =
                let val = self.parse_expr();
                // optional 'in'
                if self.peek() == &Token::In { self.eat(); }
                bindings.push((name, val));
            } else {
                break;
            }
        }
        let tail = self.parse_expr();
        if bindings.is_empty() {
            tail
        } else {
            Expr::Block(bindings, Box::new(tail), line)
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_and();
        while self.peek() == &Token::PipePipe {
            let line = self.line();
            self.eat();
            let rhs = self.parse_and();
            lhs = Expr::Bin("||".to_string(), Box::new(lhs), Box::new(rhs), line);
        }
        lhs
    }

    fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_cmp();
        while self.peek() == &Token::AmpAmp {
            let line = self.line();
            self.eat();
            let rhs = self.parse_cmp();
            lhs = Expr::Bin("&&".to_string(), Box::new(lhs), Box::new(rhs), line);
        }
        lhs
    }

    fn parse_cmp(&mut self) -> Expr {
        let mut lhs = self.parse_add();
        loop {
            let op = match self.peek() {
                Token::EqEq => "==",
                Token::BangEq => "!=",
                Token::Lt => "<",
                Token::Gt => ">",
                Token::LtEq => "<=",
                Token::GtEq => ">=",
                _ => break,
            }.to_string();
            let line = self.line();
            self.eat();
            let rhs = self.parse_add();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), line);
        }
        lhs
    }

    fn parse_add(&mut self) -> Expr {
        let mut lhs = self.parse_mul();
        loop {
            let op = match self.peek() {
                Token::Plus => "+",
                Token::Minus => "-",
                _ => break,
            }.to_string();
            let line = self.line();
            self.eat();
            let rhs = self.parse_mul();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), line);
        }
        lhs
    }

    fn parse_mul(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.peek() {
                Token::Star => "*",
                Token::Slash => "/",
                Token::Percent => "%",
                _ => break,
            }.to_string();
            let line = self.line();
            self.eat();
            let rhs = self.parse_unary();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs), line);
        }
        lhs
    }

    fn parse_unary(&mut self) -> Expr {
        let line = self.line();
        if self.peek() == &Token::Bang {
            self.eat();
            return Expr::Unary("!".to_string(), Box::new(self.parse_unary()), line);
        }
        if self.peek() == &Token::Minus {
            self.eat();
            return Expr::Unary("-".to_string(), Box::new(self.parse_unary()), line);
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut e = self.parse_atom();
        loop {
            let line = self.line();
            match self.peek() {
                Token::Dot => {
                    self.eat();
                    if let Token::Ident(f) = self.eat().clone() {
                        if self.peek() == &Token::LParen {
                            self.eat();
                            let args = self.parse_args();
                            self.eat(); // )
                            // method call: e.f(args) -> Call(Get(e, f), args)
                            let getter = Expr::Get(Box::new(e), f, line);
                            e = Expr::Call(Box::new(getter), args, line);
                        } else {
                            e = Expr::Get(Box::new(e), f, line);
                        }
                    }
                }
                Token::LBracket => {
                    self.eat();
                    let idx = self.parse_expr();
                    let _ = self.expect(&Token::RBracket);
                    e = Expr::Index(Box::new(e), Box::new(idx), line);
                }
                Token::LParen => {
                    self.eat();
                    let args = self.parse_args();
                    self.eat(); // )
                    e = Expr::Call(Box::new(e), args, line);
                }
                _ => break,
            }
        }
        e
    }

    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            // skip named arg label: "name: value" -> just parse value
            if matches!(self.peek(), Token::Ident(_)) {
                let next_pos = self.pos + 1;
                if next_pos < self.toks.len() && self.toks[next_pos].0 == Token::Colon {
                    self.eat(); // ident
                    self.eat(); // :
                }
            }
            args.push(self.parse_expr());
            if self.peek() == &Token::Comma { self.eat(); }
        }
        args
    }

    fn parse_atom(&mut self) -> Expr {
        let line = self.line();
        match self.peek().clone() {
            Token::Int(n) => { self.eat(); Expr::Int(n, line) }
            Token::Float(f) => { self.eat(); Expr::Float(f, line) }
            Token::Bool(b) => { self.eat(); Expr::Bool(b, line) }
            Token::Str(_) => { self.eat(); Expr::Str(line) }
            Token::Null => { self.eat(); Expr::Null(line) }
            Token::Ident(s) => { self.eat(); Expr::Var(s, line) }
            Token::LParen => {
                self.eat();
                let e = self.parse_expr();
                let _ = self.expect(&Token::RParen);
                e
            }
            Token::LBracket => {
                self.eat();
                let mut items = Vec::new();
                while !matches!(self.peek(), Token::RBracket | Token::Eof) {
                    items.push(self.parse_expr());
                    if self.peek() == &Token::Comma { self.eat(); }
                }
                self.eat(); // ]
                Expr::List(items, line)
            }
            Token::LBrace => {
                self.eat();
                // Could be record literal or block
                // Heuristic: if next is ident followed by colon => record
                let mut fields = Vec::new();
                while !matches!(self.peek(), Token::RBrace | Token::Eof) {
                    if let Token::Ident(k) = self.peek().clone() {
                        let k = k.clone();
                        self.eat();
                        if self.peek() == &Token::Colon {
                            self.eat();
                            let v = self.parse_expr();
                            fields.push((k, v));
                            if self.peek() == &Token::Comma { self.eat(); }
                            continue;
                        }
                    }
                    // Not a record — skip
                    self.eat();
                }
                self.eat(); // }
                Expr::Rec(fields, line)
            }
            Token::Fn => {
                self.eat();
                self.eat(); // (
                let mut params = Vec::new();
                while !matches!(self.peek(), Token::RParen | Token::Eof) {
                    if let Token::Ident(p) = self.eat().clone() { params.push(p); }
                    if self.peek() == &Token::Comma { self.eat(); }
                }
                self.eat(); // )
                self.eat(); // {
                let body = self.parse_block_expr();
                self.eat(); // }
                Expr::Fn(params, Box::new(body), line)
            }
            Token::If => {
                self.eat();
                let cond = self.parse_expr();
                // optional 'then'
                if self.peek() == &Token::Then { self.eat(); }
                let then = self.parse_expr();
                // optional 'else'
                if self.peek() == &Token::Else { self.eat(); }
                let else_ = self.parse_expr();
                Expr::If(Box::new(cond), Box::new(then), Box::new(else_), line)
            }
            Token::Let => {
                self.eat();
                let name = match self.eat().clone() {
                    Token::Ident(s) => s,
                    _ => "_".to_string(),
                };
                while !matches!(self.peek(), Token::Eq | Token::Eof) { self.eat(); }
                self.eat(); // =
                let val = self.parse_expr();
                if self.peek() == &Token::In { self.eat(); }
                let body = self.parse_expr();
                Expr::Let(name, Box::new(val), Box::new(body), line)
            }
            _ => { Expr::Null(line) }  // don't consume unknown tokens
        }
    }
}

#[derive(Debug)]
enum TopItem {
    Let(String, Expr, usize),
    Fn(String, Vec<String>, Expr, usize),
    Test(String, Expr, usize),
}

// ── Type inference ────────────────────────────────────────────────────────────
impl Checker {
    fn infer(&mut self, e: &Expr) -> Ty {
        match e {
            Expr::Int(_, _) => Ty::Int,
            Expr::Float(_, _) => Ty::Float,
            Expr::Bool(_, _) => Ty::Bool,
            Expr::Str(_) => Ty::Str,
            Expr::Null(_) => Ty::Null,

            Expr::Var(name, _) => self.lookup(name),

            Expr::List(items, line) => {
                let elem = self.fresh();
                let mut heterogeneous = false;
                for item in items {
                    let t = self.infer(item);
                    if !heterogeneous {
                        let ea = self.subst.apply(&elem);
                        let ta = self.subst.apply(&t);
                        if self.subst.unify(&ea, &ta).is_err() {
                            heterogeneous = true; // mixed types — treat as Dynamic list
                        }
                    }
                }
                let elem_ty = if heterogeneous { Ty::Dynamic } else { self.subst.apply(&elem) };
                Ty::List(Box::new(elem_ty))
            }

            Expr::Rec(fields, _) => {
                let mut ftys = Vec::new();
                for (k, v) in fields {
                    let t = self.infer(v);
                    ftys.push((k.clone(), t));
                }
                Ty::Rec(ftys)
            }

            Expr::Get(base, field, line) => {
                let bt = self.infer(base);
                let bt = self.subst.apply(&bt);
                match &bt {
                    Ty::Dynamic => Ty::Dynamic,
                    Ty::Rec(fields) => {
                        if let Some((_, ft)) = fields.iter().find(|(k, _)| k == field) {
                            ft.clone()
                        } else {
                            // field not found in known record — warn but return Dynamic
                            self.err(format!("no field '{}' in record", field), *line);
                            Ty::Dynamic
                        }
                    }
                    _ => Ty::Dynamic, // could be module access
                }
            }

            Expr::Index(base, idx, line) => {
                let bt = self.infer(base);
                let it = self.infer(idx);
                let bt = self.subst.apply(&bt);
                match &bt {
                    Ty::List(elem) => {
                        let it = self.subst.apply(&it);
                        if !matches!(it, Ty::Int | Ty::Dynamic) {
                            self.err("list index must be Int".to_string(), *line);
                        }
                        *elem.clone()
                    }
                    Ty::Dynamic => Ty::Dynamic,
                    _ => {
                        Ty::Dynamic
                    }
                }
            }

            Expr::Call(f, args, line) => {
                let ft = self.infer(f);
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
                let ft = self.subst.apply(&ft);
                match ft {
                    Ty::Dynamic => Ty::Dynamic,
                    Ty::Func(param_tys, ret) => {
                        if param_tys.len() != arg_tys.len() {
                            self.err(
                                format!("arity error: expected {} args, got {}", param_tys.len(), arg_tys.len()),
                                *line,
                            );
                        } else {
                            for (p, a) in param_tys.iter().zip(arg_tys.iter()) {
                                self.unify_or_err(p, a, *line, "argument type mismatch");
                            }
                        }
                        *ret
                    }
                    other => {
                        // calling a non-function non-dynamic — definite error
                        if !matches!(other, Ty::Var(_)) {
                            self.err(format!("calling non-function: {}", other.display()), *line);
                        }
                        Ty::Dynamic
                    }
                }
            }

            Expr::Fn(params, body, _) => {
                self.push();
                let param_tys: Vec<Ty> = params.iter().map(|p| {
                    let t = self.fresh();
                    self.define(p, t.clone());
                    t
                }).collect();
                let ret = self.infer(body);
                self.pop();
                Ty::Func(param_tys, Box::new(ret))
            }

            Expr::Let(name, val, body, _) => {
                let vt = self.infer(val);
                self.define(name, vt);
                self.infer(body)
            }

            Expr::Block(bindings, tail, _) => {
                self.push();
                for (name, val) in bindings {
                    let vt = self.infer(val);
                    self.define(name, vt);
                }
                let t = self.infer(tail);
                self.pop();
                t
            }

            Expr::If(cond, then, else_, line) => {
                let ct = self.infer(cond);
                let ct = self.subst.apply(&ct);
                if !matches!(ct, Ty::Bool | Ty::Dynamic | Ty::Var(_)) {
                    self.err(format!("if condition must be Bool, got {}", ct.display()), *line);
                }
                let tt = self.infer(then);
                let et = self.infer(else_);
                // branches should unify but we allow mismatch (dynamic)
                let tt = self.subst.apply(&tt);
                let et = self.subst.apply(&et);
                if self.subst.unify(&tt, &et).is_ok() {
                    self.subst.apply(&tt)
                } else {
                    Ty::Dynamic
                }
            }

            Expr::Bin(op, lhs, rhs, line) => {
                let lt = self.infer(lhs);
                let rt = self.infer(rhs);
                let lt = self.subst.apply(&lt);
                let rt = self.subst.apply(&rt);
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" => {
                        // Error only on definitively non-numeric types
                        let non_numeric = |t: &Ty| matches!(t,
                            Ty::Bool | Ty::Null |
                            Ty::List(_) | Ty::Rec(_) | Ty::Func(_, _)
                        );
                        // Text + Text is valid (string concat)
                        let text_concat = op == "+" && matches!((&lt, &rt),
                            (Ty::Str, Ty::Str) | (Ty::Str, Ty::Dynamic) |
                            (Ty::Dynamic, Ty::Str));
                        if text_concat {
                            return Ty::Str;
                        }
                        if non_numeric(&lt) || non_numeric(&rt) {
                            self.err(
                                format!("operator '{}' requires numeric operands, got {} and {}", op, lt.display(), rt.display()),
                                *line,
                            );
                            return Ty::Dynamic;
                        }
                        match (&lt, &rt) {
                            (Ty::Dynamic, _) | (_, Ty::Dynamic) => Ty::Dynamic,
                            (Ty::Int, Ty::Int) => Ty::Int,
                            (Ty::Float, Ty::Float) => Ty::Float,
                            (Ty::Int, Ty::Float) | (Ty::Float, Ty::Int) => Ty::Float,
                            // Var operands — unify together, return same type
                            (Ty::Var(_), _) | (_, Ty::Var(_)) => {
                                self.subst.unify(&lt, &rt).ok();
                                self.subst.apply(&lt)
                            }
                            _ => Ty::Dynamic,
                        }
                    }
                    "&&" | "||" => {
                        match (&lt, &rt) {
                            (Ty::Dynamic, _) | (_, Ty::Dynamic) => Ty::Bool,
                            (Ty::Bool, Ty::Bool) => Ty::Bool,
                            (Ty::Var(_), _) | (_, Ty::Var(_)) => Ty::Bool,
                            _ => {
                                self.err(
                                    format!("operator '{}' requires Bool operands, got {} and {}", op, lt.display(), rt.display()),
                                    *line,
                                );
                                Ty::Bool
                            }
                        }
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => Ty::Bool,
                    _ => Ty::Dynamic,
                }
            }

            Expr::Unary(op, inner, line) => {
                let t = self.infer(inner);
                let t = self.subst.apply(&t);
                match op.as_str() {
                    "!" => {
                        match &t {
                            Ty::Bool | Ty::Dynamic => {}
                            Ty::Var(_) => { self.subst.unify(&t, &Ty::Bool).ok(); }
                            _ => { self.err(format!("'!' requires Bool, got {}", t.display()), *line); }
                        }
                        Ty::Bool
                    }
                    "-" => {
                        match t {
                            Ty::Dynamic | Ty::Var(_) => Ty::Dynamic,
                            Ty::Int => Ty::Int,
                            Ty::Float => Ty::Float,
                            other => {
                                self.err(format!("unary '-' requires numeric, got {:?}", other), *line);
                                Ty::Dynamic
                            }
                        }
                    }
                    _ => Ty::Dynamic,
                }
            }
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────
fn count_fn_lines(body: &str) -> usize {
    body.lines().filter(|l| !l.trim().is_empty()).count()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let hex_haiku = args.iter().any(|a| a == "--hex-haiku");
    let program = args.iter().skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .or_else(|| {
            // --program <path>
            args.windows(2).find(|w| w[0] == "--program").map(|w| w[1].clone())
        });

    let path = match program {
        Some(p) => p,
        None => {
            eprintln!("usage: fardcheck [--program] <file.fard>");
            std::process::exit(1);
        }
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => { eprintln!("error reading {path}: {e}"); std::process::exit(1); }
    };

    let mut lx = Lexer::new(&src);
    let toks = lx.tokenize();
    let mut parser = Parser::new(toks);
    let items = parser.parse_program();

    let mut checker = Checker::new();

    // Pre-define imports as Dynamic (they'll shadow nothing meaningful)
    // Actually imports define module aliases which are looked up as Dynamic vars

    for item in &items {
        match item {
            TopItem::Let(name, expr, _line) => {
                let t = checker.infer(expr);
                checker.define(name, t);
            }
            TopItem::Fn(name, params, body, _line) => {
                // Pre-register with a fresh var for recursive self-reference
                let self_var = checker.fresh();
                checker.define(name, self_var.clone());
                checker.push();
                let param_tys: Vec<Ty> = params.iter().map(|p| {
                    let t = checker.fresh();
                    checker.define(p, t.clone());
                    t
                }).collect();
                let ret = checker.infer(body);
                checker.pop();
                let ft = Ty::Func(param_tys, Box::new(ret));
                // Unify self-reference var with actual fn type
                checker.subst.unify(&self_var, &ft).ok();
                let ft_applied = checker.subst.apply(&ft);
                // Use monomorphic type (no generalization)
                // Generalization requires type classes for numeric operators
                checker.define(name, ft_applied);
            }
            TopItem::Test(label, expr, line) => {
                let t = checker.infer(expr);
                let t = checker.subst.apply(&t);
                if !matches!(t, Ty::Bool | Ty::Dynamic | Ty::Var(_)) {
                    checker.err(
                        format!("test '{}' body must return Bool, got {}", label, t.display()),
                        *line,
                    );
                }
            }
        }
    }

    // Hex-Haiku check
    let mut hh_warnings = 0;
    if hex_haiku {
        for item in &items {
            if let TopItem::Fn(name, _, body, line) = item {
                // Count semantic lines: number of let-bindings + 1 (return expr)
                let line_count = match body {
                    Expr::Block(bindings, _, _) => bindings.len() + 1,
                    _ => 1,
                };
                if line_count > 6 {
                    eprintln!("HEX_HAIKU line {}: fn '{}' is {} semantic lines (max 6)", line, name, line_count);
                    hh_warnings += 1;
                } else {
                    println!("✓ fn '{}': {} line(s)", name, line_count);
                }
            }
        }
        if hh_warnings > 0 {
            eprintln!("{} hex-haiku violation(s)", hh_warnings);
        } else {
            println!("hex-haiku ok — all functions ≤6 lines");
        }
    }

    if checker.errors.is_empty() && hh_warnings == 0 {
        println!("ok — {} items checked, 0 errors", items.len());
        std::process::exit(0);
    } else if !checker.errors.is_empty() {
        for e in &checker.errors {
            eprintln!("TYPE ERROR line {}: {}", e.line, e.msg);
        }
        eprintln!("{} error(s)", checker.errors.len());
        std::process::exit(1);
    } else {
        std::process::exit(1);
    }
}
