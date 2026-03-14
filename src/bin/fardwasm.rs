//! fardwasm — compile FARD pure expressions to WebAssembly Text (WAT)
//! Supports: integers, booleans, arithmetic, let, fn, if/then/else, call
//! Dynamic values (imports, strings) emit unreachable traps.

use std::collections::HashMap;
use std::fmt::Write;

// ── Lexer (shared with fardcheck) ────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Int(i64), Float(f64), Bool(bool), Str(String), Null,
    Ident(String),
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Dot, Comma, Colon, Eq, Arrow, FatArrow,
    Plus, Minus, Star, Slash, Percent,
    EqEq, BangEq, Lt, Gt, LtEq, GtEq,
    AmpAmp, PipePipe, Bang,
    Let, In, Fn, If, Then, Else, Import, As, Match, While, Return, Test,
    Eof,
}

struct Lexer { chars: Vec<char>, pos: usize }

impl Lexer {
    fn new(src: &str) -> Self { Lexer { chars: src.chars().collect(), pos: 0 } }
    fn peek(&self) -> char { self.chars.get(self.pos).copied().unwrap_or('\0') }
    fn advance(&mut self) -> char { let c = self.peek(); self.pos += 1; c }
    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                ' '|'\t'|'\r'|'\n' => { self.advance(); }
                '/' if self.chars.get(self.pos+1) == Some(&'/') => {
                    while self.peek() != '\n' && self.peek() != '\0' { self.advance(); }
                }
                _ => break,
            }
        }
    }
    fn next_tok(&mut self) -> Token {
        self.skip_ws();
        let c = self.peek();
        if c == '\0' { return Token::Eof; }
        self.advance();
        match c {
            '(' => Token::LParen, ')' => Token::RParen,
            '{' => Token::LBrace, '}' => Token::RBrace,
            '[' => Token::LBracket, ']' => Token::RBracket,
            '.' => Token::Dot, ',' => Token::Comma, ':' => Token::Colon,
            '+' => Token::Plus, '*' => Token::Star, '%' => Token::Percent,
            '-' => if self.peek()=='>'{self.advance();Token::Arrow}else{Token::Minus},
            '/' => Token::Slash,
            '=' => if self.peek()=='='{self.advance();Token::EqEq}
                   else if self.peek()=='>'{self.advance();Token::FatArrow}
                   else{Token::Eq},
            '!' => if self.peek()=='='{self.advance();Token::BangEq}else{Token::Bang},
            '<' => if self.peek()=='='{self.advance();Token::LtEq}else{Token::Lt},
            '>' => if self.peek()=='='{self.advance();Token::GtEq}else{Token::Gt},
            '&' => { if self.peek()=='&'{self.advance();} Token::AmpAmp },
            '|' => { if self.peek()=='|'{self.advance();} Token::PipePipe },
            '"' => {
                while self.peek()!='"' && self.peek()!='\0' {
                    let ch = self.advance();
                    if ch=='\\' { self.advance(); }
                    else if ch=='$' && self.peek()=='{' {
                        self.advance();
                        let mut d=1;
                        while d>0 && self.peek()!='\0' {
                            match self.advance() { '{'=>d+=1, '}'=>d-=1, _=>{} }
                        }
                    }
                }
                self.advance();
                Token::Str(String::new())
            }
            '`' => { while self.peek()!='`'&&self.peek()!='\0'{self.advance();} self.advance(); Token::Str(String::new()) }
            _ if c.is_ascii_digit() => {
                let mut s = String::from(c);
                while self.peek().is_ascii_digit() { s.push(self.advance()); }
                if self.peek()=='.' && self.chars.get(self.pos+1).map(|c|c.is_ascii_digit()).unwrap_or(false) {
                    s.push(self.advance());
                    while self.peek().is_ascii_digit() { s.push(self.advance()); }
                    Token::Float(s.parse().unwrap_or(0.0))
                } else { Token::Int(s.parse().unwrap_or(0)) }
            }
            _ if c.is_alphabetic()||c=='_' => {
                let mut s = String::from(c);
                while self.peek().is_alphanumeric()||self.peek()=='_' { s.push(self.advance()); }
                match s.as_str() {
                    "let"=>Token::Let,"in"=>Token::In,"fn"=>Token::Fn,
                    "if"=>Token::If,"then"=>Token::Then,"else"=>Token::Else,
                    "true"=>Token::Bool(true),"false"=>Token::Bool(false),
                    "null"=>Token::Null,"import"=>Token::Import,"as"=>Token::As,
                    "match"=>Token::Match,"while"=>Token::While,
                    "return"=>Token::Return,"test"=>Token::Test,
                    _=>Token::Ident(s),
                }
            }
            _ => Token::Ident(String::from(c)),
        }
    }
    fn tokenize(&mut self) -> Vec<Token> {
        let mut v = Vec::new();
        loop { let t = self.next_tok(); let e = t==Token::Eof; v.push(t); if e{break;} }
        v
    }
}

// ── AST ───────────────────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
enum Expr {
    Int(i64), Float(f64), Bool(bool), Null,
    Var(String),
    List(Vec<Expr>),
    Rec(Vec<(String, Expr)>),
    Get(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Fn(Vec<String>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Bin(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Block(Vec<(String, Expr)>, Box<Expr>),
    Trap, // unreachable — for strings, imports, etc.
}

#[derive(Debug)]
enum TopItem {
    Let(String, Expr),
    Fn(String, Vec<String>, Expr),
}

// ── Parser ────────────────────────────────────────────────────────────────────
struct Parser { toks: Vec<Token>, pos: usize }

impl Parser {
    fn new(toks: Vec<Token>) -> Self { Parser { toks, pos: 0 } }
    fn peek(&self) -> &Token { self.toks.get(self.pos).unwrap_or(&Token::Eof) }
    fn eat(&mut self) -> Token {
        let t = self.toks.get(self.pos).cloned().unwrap_or(Token::Eof);
        if self.pos < self.toks.len() { self.pos += 1; }
        t
    }
    fn expect(&mut self, t: &Token) { if self.peek() == t { self.eat(); } }

    fn parse_program(&mut self) -> Vec<TopItem> {
        let mut items = Vec::new();
        loop {
            match self.peek().clone() {
                Token::Eof => break,
                Token::Import => { self.eat(); self.skip_import(); }
                Token::Test => { self.eat(); self.skip_test(); }
                Token::Let => {
                    self.eat();
                    if let Some(i) = self.parse_top_let() { items.push(i); }
                }
                Token::Fn => {
                    self.eat();
                    if let Some(i) = self.parse_top_fn() { items.push(i); }
                }
                _ => { self.eat(); }
            }
        }
        items
    }

    fn skip_import(&mut self) {
        while !matches!(self.peek(), Token::As|Token::Eof) { self.eat(); }
        if self.peek()==&Token::As { self.eat(); self.eat(); }
    }

    fn skip_test(&mut self) {
        self.eat(); // label string
        self.expect(&Token::LBrace);
        let mut depth = 1;
        while depth > 0 && self.peek() != &Token::Eof {
            match self.eat() { Token::LBrace => depth+=1, Token::RBrace => depth-=1, _=>{} }
        }
    }

    fn parse_top_let(&mut self) -> Option<TopItem> {
        let name = match self.eat() { Token::Ident(s) => s, _ => return None };
        self.expect(&Token::Eq);
        let expr = self.parse_expr();
        Some(TopItem::Let(name, expr))
    }

    fn parse_top_fn(&mut self) -> Option<TopItem> {
        let name = match self.eat() { Token::Ident(s) => s, _ => return None };
        self.expect(&Token::LParen);
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::RParen|Token::Eof) {
            if let Token::Ident(p) = self.eat() { params.push(p); }
            if self.peek()==&Token::Comma { self.eat(); }
        }
        self.expect(&Token::RParen);
        self.expect(&Token::LBrace);
        let body = self.parse_block_expr();
        self.expect(&Token::RBrace);
        Some(TopItem::Fn(name, params, body))
    }

    fn parse_block_expr(&mut self) -> Expr {
        let mut bindings = Vec::new();
        loop {
            if self.peek() == &Token::Let {
                self.eat();
                let name = match self.eat() { Token::Ident(s) => s, _ => "_".to_string() };
                // skip pattern vars until =
                while !matches!(self.peek(), Token::Eq|Token::Eof) { self.eat(); }
                self.eat(); // =
                let val = self.parse_expr();
                if self.peek()==&Token::In { self.eat(); }
                bindings.push((name, val));
            } else { break; }
        }
        let tail = self.parse_expr();
        if bindings.is_empty() { tail } else { Expr::Block(bindings, Box::new(tail)) }
    }

    fn parse_expr(&mut self) -> Expr { self.parse_or() }

    fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_and();
        while self.peek()==&Token::PipePipe {
            self.eat(); let rhs = self.parse_and();
            lhs = Expr::Bin("||".into(), Box::new(lhs), Box::new(rhs));
        }
        lhs
    }
    fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_cmp();
        while self.peek()==&Token::AmpAmp {
            self.eat(); let rhs = self.parse_cmp();
            lhs = Expr::Bin("&&".into(), Box::new(lhs), Box::new(rhs));
        }
        lhs
    }
    fn parse_cmp(&mut self) -> Expr {
        let mut lhs = self.parse_add();
        loop {
            let op = match self.peek() {
                Token::EqEq=>"==", Token::BangEq=>"!=",
                Token::Lt=>"<", Token::Gt=>">",
                Token::LtEq=>"<=", Token::GtEq=>">=",
                _=>break,
            }.to_string();
            self.eat(); let rhs = self.parse_add();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }
    fn parse_add(&mut self) -> Expr {
        let mut lhs = self.parse_mul();
        loop {
            let op = match self.peek() { Token::Plus=>"+", Token::Minus=>"-", _=>break }.to_string();
            self.eat(); let rhs = self.parse_mul();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }
    fn parse_mul(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.peek() { Token::Star=>"*", Token::Slash=>"/", Token::Percent=>"%", _=>break }.to_string();
            self.eat(); let rhs = self.parse_unary();
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }
    fn parse_unary(&mut self) -> Expr {
        if self.peek()==&Token::Bang { self.eat(); return Expr::Unary("!".into(), Box::new(self.parse_unary())); }
        if self.peek()==&Token::Minus { self.eat(); return Expr::Unary("-".into(), Box::new(self.parse_unary())); }
        self.parse_postfix()
    }
    fn parse_postfix(&mut self) -> Expr {
        let mut e = self.parse_atom();
        loop {
            match self.peek().clone() {
                Token::Dot => {
                    self.eat();
                    if let Token::Ident(f) = self.eat() {
                        if self.peek()==&Token::LParen {
                            self.eat();
                            let args = self.parse_args();
                            self.eat();
                            let getter = Expr::Get(Box::new(e), f);
                            e = Expr::Call(Box::new(getter), args);
                        } else { e = Expr::Get(Box::new(e), f); }
                    }
                }
                Token::LBracket => {
                    self.eat(); let idx = self.parse_expr(); self.expect(&Token::RBracket);
                    e = Expr::Index(Box::new(e), Box::new(idx));
                }
                Token::LParen => {
                    self.eat(); let args = self.parse_args(); self.eat();
                    e = Expr::Call(Box::new(e), args);
                }
                _ => break,
            }
        }
        e
    }
    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen|Token::Eof) {
            // skip named arg labels
            if matches!(self.peek(), Token::Ident(_)) {
                let np = self.pos+1;
                if np < self.toks.len() && self.toks[np]==Token::Colon {
                    self.eat(); self.eat();
                }
            }
            args.push(self.parse_expr());
            if self.peek()==&Token::Comma { self.eat(); }
        }
        args
    }
    fn parse_atom(&mut self) -> Expr {
        match self.peek().clone() {
            Token::Int(n) => { self.eat(); Expr::Int(n) }
            Token::Float(f) => { self.eat(); Expr::Float(f) }
            Token::Bool(b) => { self.eat(); Expr::Bool(b) }
            Token::Str(_) => { self.eat(); Expr::Trap } // strings → trap
            Token::Null => { self.eat(); Expr::Null }
            Token::Ident(s) => { self.eat(); Expr::Var(s) }
            Token::LParen => {
                self.eat(); let e = self.parse_expr(); self.expect(&Token::RParen); e
            }
            Token::LBracket => {
                self.eat();
                let mut items = Vec::new();
                while !matches!(self.peek(), Token::RBracket|Token::Eof) {
                    items.push(self.parse_expr());
                    if self.peek()==&Token::Comma { self.eat(); }
                }
                self.eat();
                Expr::List(items)
            }
            Token::LBrace => {
                self.eat();
                let mut fields = Vec::new();
                while !matches!(self.peek(), Token::RBrace|Token::Eof) {
                    if let Token::Ident(k) = self.peek().clone() {
                        let k = k.clone(); self.eat();
                        if self.peek()==&Token::Colon {
                            self.eat(); let v = self.parse_expr(); fields.push((k, v));
                            if self.peek()==&Token::Comma { self.eat(); }
                            continue;
                        }
                    }
                    self.eat();
                }
                self.eat();
                Expr::Rec(fields)
            }
            Token::Fn => {
                self.eat(); self.eat(); // fn (
                let mut params = Vec::new();
                while !matches!(self.peek(), Token::RParen|Token::Eof) {
                    if let Token::Ident(p) = self.eat() { params.push(p); }
                    if self.peek()==&Token::Comma { self.eat(); }
                }
                self.eat(); self.eat(); // ) {
                let body = self.parse_block_expr();
                self.eat(); // }
                Expr::Fn(params, Box::new(body))
            }
            Token::If => {
                self.eat();
                let cond = self.parse_expr();
                if self.peek()==&Token::Then { self.eat(); }
                let then = self.parse_expr();
                if self.peek()==&Token::Else { self.eat(); }
                let else_ = self.parse_expr();
                Expr::If(Box::new(cond), Box::new(then), Box::new(else_))
            }
            Token::Let => {
                self.eat();
                let name = match self.eat() { Token::Ident(s)=>s, _=>"_".into() };
                while !matches!(self.peek(), Token::Eq|Token::Eof) { self.eat(); }
                self.eat();
                let val = self.parse_expr();
                if self.peek()==&Token::In { self.eat(); }
                let body = self.parse_expr();
                Expr::Let(name, Box::new(val), Box::new(body))
            }
            _ => { self.eat(); Expr::Null }
        }
    }
}

// ── WAT Code Generator ────────────────────────────────────────────────────────
struct Codegen {
    // function bodies being built
    out: String,
    // local variable index map for current function
    locals: Vec<HashMap<String, u32>>,
    local_count: u32,
    // top-level function table: name -> (param_count)
    funcs: HashMap<String, usize>,
    // global let bindings (compiled as i64 globals)
    globals: Vec<(String, Expr)>,
    // list of compiled func WAT strings
    func_defs: Vec<String>,
    // list of (global_name, init_value) for simple constant globals
    global_defs: Vec<(String, i64)>,
    func_index: u32,
}

impl Codegen {
    fn new() -> Self {
        Codegen {
            out: String::new(),
            locals: Vec::new(),
            local_count: 0,
            funcs: HashMap::new(),
            globals: Vec::new(),
            func_defs: Vec::new(),
            global_defs: Vec::new(),
            func_index: 0,
        }
    }

    fn push_scope(&mut self) { self.locals.push(HashMap::new()); }
    fn pop_scope(&mut self) { self.locals.pop(); }

    fn define_local(&mut self, name: &str) -> u32 {
        let idx = self.local_count;
        self.local_count += 1;
        if let Some(top) = self.locals.last_mut() {
            top.insert(name.to_string(), idx);
        }
        idx
    }

    fn lookup_local(&self, name: &str) -> Option<u32> {
        for scope in self.locals.iter().rev() {
            if let Some(idx) = scope.get(name) { return Some(*idx); }
        }
        None
    }

    fn compile_expr(&mut self, e: &Expr, out: &mut String) {
        match e {
            Expr::Int(n) => { let _ = write!(out, "    i64.const {}\n", n); }
            Expr::Float(f) => { let _ = write!(out, "    f64.const {}\n", f); }
            Expr::Bool(b) => { let _ = write!(out, "    i64.const {}\n", if *b { 1 } else { 0 }); }
            Expr::Null => { let _ = write!(out, "    i64.const 0\n"); }
            Expr::Trap => { let _ = write!(out, "    unreachable\n"); }

            Expr::Var(name) => {
                if let Some(idx) = self.lookup_local(name) {
                    let wat = if idx >= 0xFFFF0000u32 { name.clone() } else { format!("{}", idx) };
                    let _ = write!(out, "    local.get ${}\n", wat);
                } else if self.funcs.contains_key(name) {
                    // function reference — not directly representable as i64
                    // emit a sentinel; actual call site handles it
                    let _ = write!(out, "    i64.const 0 ;; func ref {}\n", name);
                } else {
                    // global or import — emit global.get if it exists, else trap
                    let _ = write!(out, "    global.get $g_{}\n", name);
                }
            }

            Expr::Let(name, val, body) => {
                // compile val, store to local, compile body
                let idx = self.define_local(name);
                let _ = write!(out, "    (local ${}  i64)\n", idx);
                self.compile_expr(val, out);
                let _ = write!(out, "    local.set ${}\n", idx);
                self.compile_expr(body, out);
            }

            Expr::Block(bindings, tail) => {
                self.push_scope();
                for (name, val) in bindings {
                    let idx = self.define_local(name);
                    let _ = write!(out, "    (local ${}  i64)\n", idx);
                    self.compile_expr(val, out);
                    let _ = write!(out, "    local.set ${}\n", idx);
                }
                self.compile_expr(tail, out);
                self.pop_scope();
            }

            Expr::If(cond, then, else_) => {
                self.compile_expr(cond, out);
                // WAT if needs i32; comparisons give i32, booleans (i64) need wrap+ne
                let _ = write!(out, "    i32.wrap_i64\n    (if (result i64)\n      (then\n");
                self.compile_expr(then, out);
                let _ = write!(out, "      )\n      (else\n");
                self.compile_expr(else_, out);
                let _ = write!(out, "      )\n    )\n");
            }

            Expr::Bin(op, lhs, rhs) => {
                self.compile_expr(lhs, out);
                self.compile_expr(rhs, out);
                let instr = match op.as_str() {
                    "+"  => "i64.add",
                    "-"  => "i64.sub",
                    "*"  => "i64.mul",
                    "/"  => "i64.div_s",
                    "%"  => "i64.rem_s",
                    "==" => "i64.eq",
                    "!=" => "i64.ne",
                    "<"  => "i64.lt_s",
                    ">"  => "i64.gt_s",
                    "<=" => "i64.le_s",
                    ">=" => "i64.ge_s",
                    "&&" => "i64.and",
                    "||" => "i64.or",
                    _    => "unreachable",
                };
                let _ = write!(out, "    {}\n", instr);
                // comparisons return i32; extend to i64 so stack is always i64
                match op.as_str() {
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        let _ = write!(out, "    i64.extend_i32_s\n");
                    }
                    _ => {}
                }
            }

            Expr::Unary(op, inner) => {
                match op.as_str() {
                    "!" => {
                        self.compile_expr(inner, out);
                        let _ = write!(out, "    i64.const 1\n    i64.xor\n");
                    }
                    "-" => {
                        let _ = write!(out, "    i64.const 0\n");
                        self.compile_expr(inner, out);
                        let _ = write!(out, "    i64.sub\n");
                    }
                    _ => { self.compile_expr(inner, out); }
                }
            }

            Expr::Call(f, args) => {
                // Direct named call
                if let Expr::Var(name) = f.as_ref() {
                    if self.funcs.contains_key(name) {
                        for arg in args { self.compile_expr(arg, out); }
                        let _ = write!(out, "    call $f_{}\n", name);
                        return;
                    }
                }
                // Method call on module (e.g. math.add) → trap
                if let Expr::Get(_, _) = f.as_ref() {
                    let _ = write!(out, "    unreachable ;; import call\n");
                    return;
                }
                // Anonymous fn call — compile args, then trap (no closures in WAT)
                for arg in args { self.compile_expr(arg, out); }
                let _ = write!(out, "    unreachable ;; indirect call\n");
            }

            Expr::Fn(params, body) => {
                // Inline lambda — compile as a nested func, return trap at call site
                // (closures not supported in basic WAT)
                let fname = format!("lambda_{}", self.func_index);
                self.func_index += 1;
                self.funcs.insert(fname.clone(), params.len());
                let fdef = self.compile_func(&fname, params, body);
                self.func_defs.push(fdef);
                // The lambda expr itself has no i64 representation — push 0
                let _ = write!(out, "    i64.const 0 ;; lambda ref {}\n", fname);
            }

            Expr::Get(_, _) | Expr::Index(_, _) | Expr::List(_) | Expr::Rec(_) => {
                // Not representable as i64 — trap
                let _ = write!(out, "    unreachable ;; not supported in WASM target\n");
            }
        }
    }

    fn compile_func(&mut self, name: &str, params: &[String], body: &Expr) -> String {
        self.local_count = 0;
        self.locals.clear();
        self.push_scope();

        let mut body_out = String::new();

        // declare params as locals
        let param_decls: String = params.iter().map(|p| {
            format!(" (param ${} i64)", p)
        }).collect();

        // use actual param names in WAT; store sentinel 0xFFFF0000|i to distinguish from locals
        for (i, p) in params.iter().enumerate() {
            if let Some(top) = self.locals.last_mut() {
                top.insert(p.clone(), 0xFFFF0000u32 | i as u32);
            }
        }
        self.local_count = params.len() as u32;

        self.compile_expr(body, &mut body_out);
        self.pop_scope();

        // Hoist (local ...) declarations to top of func body
        let mut local_decls = String::new();
        let mut instrs = String::new();
        for line in body_out.lines() {
            let t = line.trim();
            if t.starts_with("(local ") {
                let _ = writeln!(local_decls, "    {}", t);
            } else {
                let _ = writeln!(instrs, "{}", line);
            }
        }

        format!(
            "  (func $f_{name}{param_decls} (result i64)\n{local_decls}{instrs}  )\n",
            name = name,
            param_decls = param_decls,
            local_decls = local_decls,
            instrs = instrs,
        )
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.iter().skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
        .or_else(|| args.windows(2).find(|w| w[0]=="--program").map(|w| w[1].clone()));

    let path = match path {
        Some(p) => p,
        None => { eprintln!("usage: fardwasm [--program] <file.fard> [--out <file.wat>] [--target wat|wasi]"); std::process::exit(1); }
    };

    let target = args.windows(2)
        .find(|w| w[0]=="--target")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "wat".to_string());

    let out_path = args.windows(2)
        .find(|w| w[0]=="--out")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| {
            let ext = if target == "wasi" { ".wasm" } else { ".wat" };
            path.replace(".fard", ext)
        });

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
    };

    let mut lx = Lexer::new(&src);
    let toks = lx.tokenize();
    let mut parser = Parser::new(toks);
    let items = parser.parse_program();

    let mut cg = Codegen::new();

    // First pass: register all top-level fn names
    for item in &items {
        if let TopItem::Fn(name, params, _) = item {
            cg.funcs.insert(name.clone(), params.len());
        }
    }

    // Second pass: compile
    let mut exports = Vec::new();
    for item in items {
        match item {
            TopItem::Fn(name, params, body) => {
                let fdef = cg.compile_func(&name, &params, &body);
                cg.func_defs.push(fdef);
                exports.push(name.clone());
            }
            TopItem::Let(name, expr) => {
                // Simple constant globals only
                if let Expr::Int(n) = expr {
                    cg.global_defs.push((name, n));
                } else if let Expr::Bool(b) = expr {
                    cg.global_defs.push((name, if b { 1 } else { 0 }));
                }
                // Complex lets — skip for now
            }
        }
    }

    // Emit WAT
    let mut wat = String::new();
    let _ = write!(wat, "(module\n");

    // globals
    for (name, val) in &cg.global_defs {
        let _ = write!(wat, "  (global $g_{} i64 (i64.const {}))\n", name, val);
    }

    // lambda funcs (defined during compilation)
    for fdef in &cg.func_defs {
        let _ = write!(wat, "{}", fdef);
    }

    // exports
    for name in &exports {
        let _ = write!(wat, "  (export \"{}\" (func $f_{}))\n", name, name);
    }

    let _ = write!(wat, ")\n");

    if target == "wasi" {
        // Assemble WAT to WASM using wat2wasm, then report
        let tmp_wat = format!("{}.tmp.wat", out_path);
        match std::fs::write(&tmp_wat, &wat) {
            Ok(_) => {}
            Err(e) => { eprintln!("error writing {}: {}", tmp_wat, e); std::process::exit(1); }
        }
        let status = std::process::Command::new("wat2wasm")
            .arg(&tmp_wat).arg("-o").arg(&out_path)
            .status();
        let _ = std::fs::remove_file(&tmp_wat);
        match status {
            Ok(s) if s.success() => {
                println!("wrote {} (wasm)", out_path);
                println!("{} function(s) exported", exports.len());
                if !exports.is_empty() {
                    println!("exports: {}", exports.join(", "));
                }
            }
            Ok(_) => { eprintln!("wat2wasm failed"); std::process::exit(1); }
            Err(e) => { eprintln!("wat2wasm not found: {e}
Install with: brew install wabt"); std::process::exit(1); }
        }
    } else {
        match std::fs::write(&out_path, &wat) {
            Ok(_) => {
                println!("wrote {}", out_path);
                println!("{} function(s) exported", exports.len());
                if !exports.is_empty() {
                    println!("exports: {}", exports.join(", "));
                }
            }
            Err(e) => { eprintln!("error writing {out_path}: {e}"); std::process::exit(1); }
        }
    }
}
