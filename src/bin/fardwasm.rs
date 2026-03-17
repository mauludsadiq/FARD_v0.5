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

// ── Value representation ──────────────────────────────────────────────────────
// All FARD values are i64 on the WASM stack.
// Tag in high 8 bits:
//   0x00 = Int   (i64 value in low 56 bits, sign-extended)
//   0x01 = Float (f64 bits stored separately; i64 = ptr to f64 in memory)
//   0x02 = Bool  (0 or 1 in low bits)
//   0x03 = Null
//   0x04 = Ptr to string: high 32 bits = ptr, low 32 bits = byte length
//   0x05 = Ptr to record: i64 = ptr to {n_fields, [key_ptr, key_len, val_i64]...}
//
// For simplicity in this implementation:
//   Int  -> raw i64 (no tag, pure numeric)
//   Bool -> i64 (0 or 1)
//   Null -> i64 0
//   Float -> f64 stored in memory, i64 = tagged ptr
//   String -> i64 = (ptr << 32 | len), string bytes in linear memory
//   Record -> i64 = ptr, layout in linear memory
//
// Memory layout:
//   0x0000 - 0x00FF: receipt buffer (256 bytes)
//   0x0100 - 0x01FF: result JSON buffer (256 bytes, for WASI output)
//   0x0200 - bump allocator start

// ── Codegen ───────────────────────────────────────────────────────────────────
struct Codegen {
    locals: Vec<HashMap<String, u32>>,
    local_count: u32,
    local_types: Vec<(u32, &'static str)>, // (local_idx, wasm_type)
    funcs: HashMap<String, usize>,
    func_defs: Vec<String>,
    global_defs: Vec<(String, i64)>,
    func_index: u32,
    // string literals interned at compile time
    string_pool: Vec<(String, u32)>, // (content, offset_in_data)
    string_data_offset: u32,
    needs_memory: bool,
    needs_bump_alloc: bool,
}

impl Codegen {
    fn new() -> Self {
        Codegen {
            locals: Vec::new(),
            local_count: 0,
            local_types: Vec::new(),
            funcs: HashMap::new(),
            func_defs: Vec::new(),
            global_defs: Vec::new(),
            func_index: 0,
            string_pool: Vec::new(),
            string_data_offset: 0x0300, // start after reserved areas
            needs_memory: false,
            needs_bump_alloc: false,
        }
    }

    fn push_scope(&mut self) { self.locals.push(HashMap::new()); }
    fn pop_scope(&mut self) { self.locals.pop(); }

    fn define_local(&mut self, name: &str, ty: &'static str) -> u32 {
        let idx = self.local_count;
        self.local_count += 1;
        self.local_types.push((idx, ty));
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

    fn intern_string(&mut self, s: &str) -> u32 {
        // Return offset of string in data segment
        for (content, offset) in &self.string_pool {
            if content == s { return *offset; }
        }
        let offset = self.string_data_offset;
        self.string_pool.push((s.to_string(), offset));
        self.string_data_offset += s.len() as u32 + 1; // +1 for null terminator
        self.needs_memory = true;
        offset
    }

    fn compile_expr(&mut self, e: &Expr, out: &mut String) {
        match e {
            Expr::Int(n) => { let _ = write!(out, "    i64.const {}\n", n); }
            Expr::Float(f) => {
                // Store float as f64 on stack, boxed via local
                let _ = write!(out, "    f64.const {}\n    i64.reinterpret_f64\n", f);
            }
            Expr::Bool(b) => { let _ = write!(out, "    i64.const {}\n", if *b { 1 } else { 0 }); }
            Expr::Null => { let _ = write!(out, "    i64.const 0\n"); }
            Expr::Trap => { let _ = write!(out, "    unreachable\n"); }

            Expr::Var(name) => {
                if let Some(idx) = self.lookup_local(name) {
                    let _ = write!(out, "    local.get $l{}\n", idx);
                } else if self.funcs.contains_key(name) {
                    let _ = write!(out, "    i64.const 0 ;; func ref {}\n", name);
                } else {
                    let _ = write!(out, "    global.get $g_{}\n", name);
                }
            }

            Expr::Let(name, val, body) => {
                let idx = self.define_local(name, "i64");
                self.compile_expr(val, out);
                let _ = write!(out, "    local.set $l{}\n", idx);
                self.compile_expr(body, out);
            }

            Expr::Block(bindings, tail) => {
                self.push_scope();
                for (name, val) in bindings {
                    let idx = self.define_local(name, "i64");
                    self.compile_expr(val, out);
                    let _ = write!(out, "    local.set $l{}\n", idx);
                }
                self.compile_expr(tail, out);
                self.pop_scope();
            }

            Expr::If(cond, then, else_) => {
                self.compile_expr(cond, out);
                let _ = write!(out, "    i32.wrap_i64\n    (if (result i64)\n      (then\n");
                self.compile_expr(then, out);
                let _ = write!(out, "      )\n      (else\n");
                self.compile_expr(else_, out);
                let _ = write!(out, "      )\n    )\n");
            }

            Expr::Bin(op, lhs, rhs) => {
                // String concatenation
                if op == "+" {
                    // Could be string or int — emit int path, strings need runtime check
                    // For now: compile both, use i64.add (strings would need special handling)
                }
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
                if let Expr::Var(name) = f.as_ref() {
                    if self.funcs.contains_key(name) {
                        for arg in args { self.compile_expr(arg, out); }
                        let _ = write!(out, "    call $f_{}\n", name);
                        return;
                    }
                }
                // Method call: math.sqrt, str.len, etc.
                if let Expr::Get(base, method) = f.as_ref() {
                    if let Expr::Var(module) = base.as_ref() {
                        match (module.as_str(), method.as_str()) {
                            ("math", "sqrt") if args.len() == 1 => {
                                self.compile_expr(&args[0], out);
                                let _ = write!(out, "    f64.reinterpret_i64\n    f64.sqrt\n    i64.reinterpret_f64\n");
                                return;
                            }
                            ("math", "abs") if args.len() == 1 => {
                                self.compile_expr(&args[0], out);
                                let _ = write!(out, "    i64.const 0\n    i64.lt_s\n    (if (result i64) (then i64.const 0 local.get $l0 i64.sub) (else local.get $l0))\n");
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                // Unsupported call — trap
                for arg in args { self.compile_expr(arg, out); }
                let _ = write!(out, "    unreachable ;; unsupported call\n");
            }

            Expr::Fn(params, body) => {
                let fname = format!("lambda_{}", self.func_index);
                self.func_index += 1;
                self.funcs.insert(fname.clone(), params.len());
                let fdef = self.compile_func(&fname, params, body);
                self.func_defs.push(fdef);
                let _ = write!(out, "    i64.const 0 ;; lambda {}\n", fname);
            }

            Expr::Get(_, _) | Expr::Index(_, _) => {
                let _ = write!(out, "    unreachable ;; field/index access\n");
            }

            Expr::List(items) => {
                // Allocate list in linear memory: [n_items, item0, item1, ...]
                // Returns i64 ptr
                self.needs_memory = true;
                self.needs_bump_alloc = true;
                let n = items.len();
                // bump_alloc((n+1) * 8) -> ptr
                let _ = write!(out, "    i64.const {}\n    call $bump_alloc\n", (n + 1) * 8);
                let ptr_local = self.define_local("__list_ptr", "i64");
                let _ = write!(out, "    local.tee $l{}\n", ptr_local);
                // store length
                let _ = write!(out, "    i32.wrap_i64\n    i64.const {}\n    i64.store\n", n);
                // store items
                for (i, item) in items.iter().enumerate() {
                    let _ = write!(out, "    local.get $l{}\n    i32.wrap_i64\n    i32.const {}\n    i32.add\n", ptr_local, (i + 1) * 8);
                    self.compile_expr(item, out);
                    let _ = write!(out, "    i64.store\n");
                }
                let _ = write!(out, "    local.get $l{}\n", ptr_local);
            }

            Expr::Rec(fields) => {
                // Allocate record: [n_fields, [val0, val1, ...]]
                // Key names encoded at compile time in data section
                self.needs_memory = true;
                self.needs_bump_alloc = true;
                let n = fields.len();
                let _ = write!(out, "    i64.const {}\n    call $bump_alloc\n", (n + 1) * 8);
                let ptr_local = self.define_local("__rec_ptr", "i64");
                let _ = write!(out, "    local.tee $l{}\n", ptr_local);
                let _ = write!(out, "    i32.wrap_i64\n    i64.const {}\n    i64.store\n", n);
                for (i, (_key, val)) in fields.iter().enumerate() {
                    let _ = write!(out, "    local.get $l{}\n    i32.wrap_i64\n    i32.const {}\n    i32.add\n", ptr_local, (i + 1) * 8);
                    self.compile_expr(val, out);
                    let _ = write!(out, "    i64.store\n");
                }
                let _ = write!(out, "    local.get $l{}\n", ptr_local);
            }
        }
    }

    fn compile_func(&mut self, name: &str, params: &[String], body: &Expr) -> String {
        self.local_count = 0;
        self.locals.clear();
        self.local_types.clear();
        self.push_scope();

        // Register params with sentinel high bits
        for (i, p) in params.iter().enumerate() {
            if let Some(top) = self.locals.last_mut() {
                top.insert(p.clone(), 0xFFFF0000u32 | i as u32);
            }
        }
        self.local_count = params.len() as u32;

        let mut body_out = String::new();
        self.compile_expr(body, &mut body_out);
        self.pop_scope();

        let param_decls: String = params.iter()
            .map(|p| format!(" (param ${} i64)", p))
            .collect();

        // Collect non-param locals
        let local_decls: String = self.local_types.iter()
            .filter(|(idx, _)| *idx >= params.len() as u32)
            .map(|(idx, ty)| format!("    (local $l{} {})\n", idx, ty))
            .collect();

        // Fix local references: replace $lN where N < n_params with $param_name
        let mut fixed_body = body_out.clone();
        for (i, p) in params.iter().enumerate() {
            let sentinel = format!("$l{}", 0xFFFF0000u32 | i as u32);
            fixed_body = fixed_body.replace(&sentinel, &format!("${}", p));
        }

        format!(
            "  (func $f_{name}{param_decls} (result i64)\n{local_decls}{body}  )\n",
            name = name,
            param_decls = param_decls,
            local_decls = local_decls,
            body = fixed_body,
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
    let mut main_expr: Option<Expr> = None;
    let mut top_globals: Vec<(String, i64)> = Vec::new();

    for item in items {
        match item {
            TopItem::Fn(name, params, body) => {
                let fdef = cg.compile_func(&name, &params, &body);
                cg.func_defs.push(fdef);
                exports.push(name.clone());
            }
            TopItem::Let(name, expr) => {
                match &expr {
                    Expr::Int(n) => { top_globals.push((name, *n)); }
                    Expr::Bool(b) => { top_globals.push((name, if *b { 1 } else { 0 })); }
                    _ => {
                        // Last non-trivial let becomes the main expression
                        main_expr = Some(expr);
                    }
                }
            }
        }
    }

    // Emit WAT
    let mut wat = String::new();
    let _ = write!(wat, "(module\n");

    // WASI imports MUST come before memory and other definitions
    if target == "wasi" {
        let _ = write!(wat, "  (import \"wasi_snapshot_preview1\" \"fd_write\" (func $fd_write (param i32 i32 i32 i32) (result i32)))\n");
        let _ = write!(wat, "  (import \"wasi_snapshot_preview1\" \"proc_exit\" (func $proc_exit (param i32)))\n");
    }

    // Memory
    let needs_mem = cg.needs_memory || main_expr.is_some();
    if needs_mem || target == "wasi" {
        let _ = write!(wat, "  (memory (export \"memory\") 2)\n");
    }

    // Bump allocator global
    if cg.needs_bump_alloc || target == "wasi" {
        let _ = write!(wat, "  (global $heap_ptr (mut i32) (i32.const 0x0400))\n");
    }

    // Globals
    for (name, val) in &top_globals {
        let _ = write!(wat, "  (global $g_{} i64 (i64.const {}))\n", name, val);
    }

    // String data section
    if !cg.string_pool.is_empty() {
        for (content, offset) in &cg.string_pool {
            let escaped: String = content.chars().map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c.to_string()
                } else {
                    format!("\\{:02x}", c as u8)
                }
            }).collect();
            let _ = write!(wat, "  (data (i32.const {}) \"{}\\00\")\n", offset, escaped);
        }
    }

    // Bump allocator function
    if cg.needs_bump_alloc || target == "wasi" {
        let _ = write!(wat, r#"  (func $bump_alloc (param $size i64) (result i64)
    global.get $heap_ptr
    i64.extend_i32_u
    global.get $heap_ptr
    local.get $size
    i32.wrap_i64
    i32.add
    global.set $heap_ptr
  )
"#);
    }

    // i64_to_str helper (writes decimal i64 to memory, returns ptr+len as i64)
    if target == "wasi" {
        let _ = write!(wat, r#"  (func $i64_to_str (param $n i64) (result i32 i32)
    (local $ptr i32)
    (local $end i32)
    (local $neg i32)
    i32.const 0x0118
    local.set $end
    i32.const 0x0118
    local.set $ptr
    ;; handle zero
    local.get $n
    i64.const 0
    i64.eq
    (if
      (then
        local.get $ptr
        i32.const 1
        i32.sub
        local.tee $ptr
        i32.const 48
        i32.store8
        local.get $ptr
        local.get $end
        local.get $ptr
        i32.sub
        return
      )
    )
    ;; handle negative
    local.get $n
    i64.const 0
    i64.lt_s
    local.tee $neg
    (if
      (then
        i64.const 0
        local.get $n
        i64.sub
        local.set $n
      )
    )
    ;; digits
    (block $break
      (loop $loop
        local.get $n
        i64.const 0
        i64.gt_s
        i32.eqz
        br_if $break
        local.get $ptr
        i32.const 1
        i32.sub
        local.tee $ptr
        local.get $n
        i64.const 10
        i64.rem_s
        i32.wrap_i64
        i32.const 48
        i32.add
        i32.store8
        local.get $n
        i64.const 10
        i64.div_s
        local.set $n
        br $loop
      )
    )
    local.get $neg
    (if
      (then
        local.get $ptr
        i32.const 1
        i32.sub
        local.tee $ptr
        i32.const 45
        i32.store8
      )
    )
    local.get $ptr
    local.get $end
    local.get $ptr
    i32.sub
  )
"#);

        // fd_write helper: print string at (ptr, len) to stdout
        let _ = write!(wat, r#"  (func $print_i64 (param $n i64)
    (local $ptr i32)
    (local $len i32)
    (local $iov i32)
    local.get $n
    call $i64_to_str
    local.set $len
    local.set $ptr
    ;; iovec at 0x00F0: [ptr, len]
    i32.const 0x00F0
    local.set $iov
    local.get $iov
    local.get $ptr
    i32.store
    local.get $iov
    i32.const 4
    i32.add
    local.get $len
    i32.store
    i32.const 1  ;; stdout fd
    local.get $iov
    i32.const 1  ;; 1 iovec
    i32.const 0x00F8  ;; nwritten ptr
    call $fd_write
    drop
  )
"#);

        // _start: evaluate main expr, print result, exit 0
        let main_body = if let Some(ref me) = main_expr {
            let mut mb = String::new();
            cg.compile_expr(me, &mut mb);
            // collect locals needed
            let local_decls: String = cg.local_types.iter()
                .map(|(idx, ty)| format!("    (local $l{} {})\n", idx, ty))
                .collect();
            format!("{}{}    call $print_i64\n", local_decls, mb)
        } else if !exports.is_empty() {
            // call first exported fn with no args as demo
            format!("    i64.const 0\n    call $f_{}\n    call $print_i64\n", exports[0])
        } else {
            "    i64.const 0\n    call $print_i64\n".to_string()
        };

        let _ = write!(wat, "  (func $_start\n{}    i32.const 0\n    call $proc_exit\n  )\n", main_body);
        let _ = write!(wat, "  (export \"_start\" (func $_start))\n");
    }

    // Compiled functions
    for fdef in &cg.func_defs {
        let _ = write!(wat, "{}", fdef);
    }

    // Function exports
    for name in &exports {
        let _ = write!(wat, "  (export \"{}\" (func $f_{}))\n", name, name);
    }

    let _ = write!(wat, ")\n");

    if target == "wasi" {
        let tmp_wat = format!("{}.tmp.wat", out_path);
        std::fs::write(&tmp_wat, &wat).unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
        let status = std::process::Command::new("wat2wasm")
            .arg(&tmp_wat).arg("-o").arg(&out_path)
            .status();
        let _ = std::fs::remove_file(&tmp_wat);
        match status {
            Ok(s) if s.success() => {
                println!("wrote {} (wasm, WASI)", out_path);
                println!("{} function(s) exported", exports.len());
                if !exports.is_empty() { println!("exports: {}", exports.join(", ")); }
                println!("run with: wasmtime {}", out_path);
            }
            Ok(_) => { eprintln!("wat2wasm failed"); std::process::exit(1); }
            Err(e) => { eprintln!("wat2wasm not found: {e}\nInstall: brew install wabt"); std::process::exit(1); }
        }
    } else {
        std::fs::write(&out_path, &wat).unwrap_or_else(|e| { eprintln!("error: {e}"); std::process::exit(1); });
        println!("wrote {}", out_path);
        println!("{} function(s) exported", exports.len());
        if !exports.is_empty() { println!("exports: {}", exports.join(", ")); }
    }
}
