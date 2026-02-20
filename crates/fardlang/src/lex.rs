use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    PlusPlus,
    KwModule,
    KwImport,
    KwAs,
    KwPub,
    KwType,
    KwEffect,
    KwFn,
    KwUses,
    KwRun,
    KwLet,
    KwIf,
    KwElse,
    KwTrue,
    KwFalse,
    KwUnit,

    Ident(String),
    Text(String),     // "..."
    BytesHex(String), // b"..."
    Int(String),      // -?\d+

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    Lt,
    Gt,
    Colon,
    Comma,
    Dot,
    Eq,
    Pipe,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    EqEq,
    Le,
    Ge,
    AndAnd,
    OrOr,
    Eof,
}

pub struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    pub fn mark(&self) -> usize {
        self.i
    }
    pub fn reset(&mut self, m: usize) {
        self.i = m
    }

    pub fn new(bytes: &'a [u8]) -> Self {
        Self { s: bytes, i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        Some(b)
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.i += 1;
            }
            if self.peek() == Some(b'/') && self.s.get(self.i + 1) == Some(&b'/') {
                self.i += 2;
                while let Some(b) = self.peek() {
                    self.i += 1;
                    if b == b'\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }

    fn is_ident_start(b: u8) -> bool {
        matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_')
    }
    fn is_ident_cont(b: u8) -> bool {
        Self::is_ident_start(b) || matches!(b, b'0'..=b'9')
    }

    fn lex_ident(&mut self) -> Result<String> {
        let mut v = Vec::new();
        match self.peek() {
            Some(b) if Self::is_ident_start(b) => v.push(self.bump().unwrap()),
            _ => bail!("ERROR_PARSE expected ident"),
        }
        while let Some(b) = self.peek() {
            if Self::is_ident_cont(b) {
                v.push(self.bump().unwrap());
            } else {
                break;
            }
        }
        Ok(String::from_utf8(v).unwrap())
    }

    fn lex_int(&mut self) -> Result<String> {
        let mut v = Vec::new();
        let mut any = false;
        while let Some(b) = self.peek() {
            if matches!(b, b'0'..=b'9') {
                any = true;
                v.push(self.bump().unwrap());
            } else {
                break;
            }
        }
        if !any {
            bail!("ERROR_PARSE expected digits");
        }
        Ok(String::from_utf8(v).unwrap())
    }

    fn lex_text(&mut self) -> Result<String> {
        // assumes opening '"'
        self.bump();
        let mut out = String::new();
        while let Some(b) = self.bump() {
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = self
                        .bump()
                        .ok_or_else(|| anyhow::anyhow!("ERROR_PARSE bad escape"))?;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0c'),
                        _ => bail!("ERROR_PARSE unsupported escape"),
                    }
                }
                _ => out.push(char::from(b)),
            }
        }
        bail!("ERROR_PARSE unterminated string")
    }

    fn lex_bytes_hex(&mut self) -> Result<String> {
        // assumes leading b"
        self.bump(); // b
        if self.bump() != Some(b'"') {
            bail!("ERROR_PARSE expected b\"");
        }
        let mut v = Vec::new();
        while let Some(b) = self.bump() {
            if b == b'"' {
                break;
            }
            v.push(b);
        }
        Ok(String::from_utf8(v).unwrap())
    }

    pub fn next(&mut self) -> Result<Tok> {
        self.skip_ws_and_comments();
        match self.peek() {
            // lex_ops_dispatch_v1 begin
            Some(b'<') => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Tok::Le)
                } else {
                    Ok(Tok::Lt)
                }
            }
            Some(b'>') => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Tok::Ge)
                } else {
                    Ok(Tok::Gt)
                }
            }
            Some(b'+') => {
                self.bump();
                if self.peek() == Some(b'+') {
                    self.bump();
                    Ok(Tok::PlusPlus)
                } else {
                    Ok(Tok::Plus)
                }
            }
            Some(b'-') => {
                self.bump();
                Ok(Tok::Minus)
            }
            Some(b'*') => {
                self.bump();
                Ok(Tok::Star)
            }
            Some(b'/') => {
                self.bump();
                Ok(Tok::Slash)
            }
            Some(b'%') => {
                self.bump();
                Ok(Tok::Percent)
            }
            Some(b'=') => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Tok::EqEq)
                } else {
                    Ok(Tok::Eq)
                }
            }
            Some(b'&') => {
                self.bump();
                if self.peek() == Some(b'&') {
                    self.bump();
                    Ok(Tok::AndAnd)
                } else {
                    bail!("ERROR_PARSE expected &&");
                }
            }
            Some(b'|') => {
                self.bump();
                if self.peek() == Some(b'|') {
                    self.bump();
                    Ok(Tok::OrOr)
                } else {
                    Ok(Tok::Pipe)
                }
            }
            // lex_ops_dispatch_v1 end
            None => Ok(Tok::Eof),
            Some(b'(') => {
                self.bump();
                Ok(Tok::LParen)
            }
            Some(b')') => {
                self.bump();
                Ok(Tok::RParen)
            }
            Some(b'{') => {
                self.bump();
                Ok(Tok::LBrace)
            }
            Some(b'}') => {
                self.bump();
                Ok(Tok::RBrace)
            }
            Some(b'[') => {
                self.bump();
                Ok(Tok::LBrack)
            }
            Some(b']') => {
                self.bump();
                Ok(Tok::RBrack)
            }
            Some(b':') => {
                self.bump();
                Ok(Tok::Colon)
            }
            Some(b',') => {
                self.bump();
                Ok(Tok::Comma)
            }
            Some(b'.') => {
                self.bump();
                Ok(Tok::Dot)
            }
            Some(b'"') => Ok(Tok::Text(self.lex_text()?)),
            Some(b'0'..=b'9') => Ok(Tok::Int(self.lex_int()?)),

            Some(b'b') if self.s.get(self.i + 1) == Some(&b'"') => {
                Ok(Tok::BytesHex(self.lex_bytes_hex()?))
            }
            Some(b) if Self::is_ident_start(b) => {
                let id = self.lex_ident()?;
                Ok(match id.as_str() {
                    "module" => Tok::KwModule,
                    "import" => Tok::KwImport,
                    "as" => Tok::KwAs,
                    "pub" => Tok::KwPub,
                    "type" => Tok::KwType,
                    "effect" => Tok::KwEffect,
                    "fn" => Tok::KwFn,
                    "uses" => Tok::KwUses,
                    "Run" => Tok::KwRun,
                    "let" => Tok::KwLet,
                    "if" => Tok::KwIf,
                    "else" => Tok::KwElse,
                    "true" => Tok::KwTrue,
                    "false" => Tok::KwFalse,
                    "unit" => Tok::KwUnit,
                    _ => Tok::Ident(id),
                })
            }
            Some(b) => bail!("ERROR_PARSE unexpected byte {} at {}", b, self.i),
        }
    }
}
