use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Fn,
    Ident(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    Unit,
    Eof,
}

pub struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
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
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.i += 1;
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

    pub fn next(&mut self) -> Result<Tok> {
        self.skip_ws();
        match self.peek() {
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
            Some(b) if Self::is_ident_start(b) => {
                let id = self.lex_ident()?;
                Ok(match id.as_str() {
                    "fn" => Tok::Fn,
                    "unit" => Tok::Unit,
                    _ => Tok::Ident(id),
                })
            }
            Some(b) => bail!("ERROR_PARSE unexpected byte {} at {}", b, self.i),
        }
    }
}
