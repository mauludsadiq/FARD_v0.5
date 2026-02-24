//! Native JSON parser and serializer. No external dependencies.
//! Replaces serde_json across the workspace.

use std::collections::BTreeMap;
use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum JsonVal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<JsonVal>),
    Object(BTreeMap<String, JsonVal>),
}

// ── Serializer ──────────────────────────────────────────────────────────────

pub fn to_string(v: &JsonVal) -> String {
    let mut out = String::new();
    write_val(v, &mut out);
    out
}

pub fn to_string_pretty(v: &JsonVal) -> String {
    let mut out = String::new();
    write_pretty(v, &mut out, 0);
    out
}

pub fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c if (c as u32) > 0x7f => {
                // ASCII-safe: escape non-ASCII for cross-platform hash stability
                for unit in c.encode_utf16(&mut [0u16; 2]).iter() {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn write_val(v: &JsonVal, out: &mut String) {
    match v {
        JsonVal::Null       => out.push_str("null"),
        JsonVal::Bool(b)    => out.push_str(if *b { "true" } else { "false" }),
        JsonVal::Int(n)     => out.push_str(&n.to_string()),
        JsonVal::Float(f)   => {
            if f.is_finite() {
                out.push_str(&format!("{}", f));
            } else {
                out.push_str("null");
            }
        }
        JsonVal::Str(s)     => out.push_str(&escape_string(s)),
        JsonVal::Array(a)   => {
            out.push('[');
            for (i, v) in a.iter().enumerate() {
                if i > 0 { out.push(','); }
                write_val(v, out);
            }
            out.push(']');
        }
        JsonVal::Object(m)  => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(&escape_string(k));
                out.push(':');
                write_val(v, out);
            }
            out.push('}');
        }
    }
}

fn write_pretty(v: &JsonVal, out: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    let inner  = "  ".repeat(depth + 1);
    match v {
        JsonVal::Array(a) if a.is_empty() => out.push_str("[]"),
        JsonVal::Object(m) if m.is_empty() => out.push_str("{}"),
        JsonVal::Array(a) => {
            out.push_str("[\n");
            for (i, v) in a.iter().enumerate() {
                out.push_str(&inner);
                write_pretty(v, out, depth + 1);
                if i + 1 < a.len() { out.push(','); }
                out.push('\n');
            }
            out.push_str(&indent);
            out.push(']');
        }
        JsonVal::Object(m) => {
            out.push_str("{\n");
            let pairs: Vec<_> = m.iter().collect();
            for (i, (k, v)) in pairs.iter().enumerate() {
                out.push_str(&inner);
                out.push_str(&escape_string(k));
                out.push_str(": ");
                write_pretty(v, out, depth + 1);
                if i + 1 < pairs.len() { out.push(','); }
                out.push('\n');
            }
            out.push_str(&indent);
            out.push('}');
        }
        v => write_val(v, out),
    }
}

// ── Parser ───────────────────────────────────────────────────────────────────

pub fn from_str(s: &str) -> Result<JsonVal> {
    let mut p = Parser { src: s.as_bytes(), pos: 0 };
    let v = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.src.len() {
        bail!("JSON_TRAILING_GARBAGE at {}", p.pos);
    }
    Ok(v)
}

pub fn from_slice(b: &[u8]) -> Result<JsonVal> {
    match std::str::from_utf8(b) {
        Ok(s) => from_str(s),
        Err(e) => bail!("JSON_INVALID_UTF8: {}", e),
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Result<u8> {
        match self.src.get(self.pos) {
            Some(&b) => { self.pos += 1; Ok(b) }
            None => bail!("JSON_UNEXPECTED_EOF"),
        }
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, b: u8) -> Result<()> {
        let got = self.next_byte()?;
        if got != b { bail!("JSON_EXPECTED {:?} got {:?}", b as char, got as char); }
        Ok(())
    }

    fn parse_value(&mut self) -> Result<JsonVal> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => { self.consume_literal(b"null")?; Ok(JsonVal::Null) }
            Some(b't') => { self.consume_literal(b"true")?; Ok(JsonVal::Bool(true)) }
            Some(b'f') => { self.consume_literal(b"false")?; Ok(JsonVal::Bool(false)) }
            Some(b'"') => Ok(JsonVal::Str(self.parse_string()?)),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(b) => bail!("JSON_UNEXPECTED_BYTE {:?} at {}", b as char, self.pos),
            None => bail!("JSON_UNEXPECTED_EOF"),
        }
    }

    fn consume_literal(&mut self, lit: &[u8]) -> Result<()> {
        for &b in lit {
            let got = self.next_byte()?;
            if got != b { bail!("JSON_BAD_LITERAL"); }
        }
        Ok(())
    }

    fn parse_string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let mut s = String::new();
        loop {
            let b = self.next_byte()?;
            match b {
                b'"' => break,
                b'\\' => {
                    let esc = self.next_byte()?;
                    match esc {
                        b'"'  => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/'  => s.push('/'),
                        b'n'  => s.push('\n'),
                        b'r'  => s.push('\r'),
                        b't'  => s.push('\t'),
                        b'b'  => s.push('\x08'),
                        b'f'  => s.push('\x0c'),
                        b'u'  => {
                            let u = self.parse_hex4()?;
                            if (0xD800..=0xDBFF).contains(&u) {
                                // surrogate pair
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let u2 = self.parse_hex4()?;
                                let cp = 0x10000 + ((u as u32 - 0xD800) << 10)
                                       + (u2 as u32 - 0xDC00);
                                s.push(char::from_u32(cp)
                                    .ok_or_else(|| anyhow::anyhow!("JSON_BAD_SURROGATE"))?);
                            } else {
                                s.push(char::from_u32(u as u32)
                                    .ok_or_else(|| anyhow::anyhow!("JSON_BAD_UNICODE"))?);
                            }
                        }
                        _ => bail!("JSON_BAD_ESCAPE {:?}", esc as char),
                    }
                }
                b if b < 0x20 => bail!("JSON_CONTROL_CHAR {}", b),
                b => {
                    // UTF-8 passthrough
                    s.push(b as char);
                    // handle multi-byte sequences
                    if b >= 0x80 {
                        s.pop();
                        let mut buf = vec![b];
                        let extra = if b >= 0xF0 { 3 }
                                    else if b >= 0xE0 { 2 }
                                    else { 1 };
                        for _ in 0..extra {
                            buf.push(self.next_byte()?);
                        }
                        let ch = std::str::from_utf8(&buf)
                            .map_err(|_| anyhow::anyhow!("JSON_BAD_UTF8"))?
                            .chars().next()
                            .ok_or_else(|| anyhow::anyhow!("JSON_EMPTY_UTF8"))?;
                        s.push(ch);
                    }
                }
            }
        }
        Ok(s)
    }

    fn parse_hex4(&mut self) -> Result<u16> {
        let mut v = 0u16;
        for _ in 0..4 {
            let b = self.next_byte()?;
            let d = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => bail!("JSON_BAD_HEX {:?}", b as char),
            };
            v = (v << 4) | d as u16;
        }
        Ok(v)
    }

    fn parse_number(&mut self) -> Result<JsonVal> {
        let start = self.pos;
        if self.peek() == Some(b'-') { self.pos += 1; }
        while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
        let is_float = matches!(self.peek(), Some(b'.') | Some(b'e') | Some(b'E'));
        if is_float {
            if self.peek() == Some(b'.') {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
            }
            if matches!(self.peek(), Some(b'e') | Some(b'E')) {
                self.pos += 1;
                if matches!(self.peek(), Some(b'+') | Some(b'-')) { self.pos += 1; }
                while matches!(self.peek(), Some(b'0'..=b'9')) { self.pos += 1; }
            }
            let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
            let f: f64 = s.parse().map_err(|_| anyhow::anyhow!("JSON_BAD_FLOAT {}", s))?;
            Ok(JsonVal::Float(f))
        } else {
            let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
            let i: i64 = s.parse().map_err(|_| anyhow::anyhow!("JSON_BAD_INT {}", s))?;
            Ok(JsonVal::Int(i))
        }
    }

    fn parse_array(&mut self) -> Result<JsonVal> {
        self.expect(b'[')?;
        self.skip_ws();
        let mut arr = Vec::new();
        if self.peek() == Some(b']') { self.pos += 1; return Ok(JsonVal::Array(arr)); }
        loop {
            arr.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b']') => { self.pos += 1; break; }
                Some(b',') => { self.pos += 1; }
                _ => bail!("JSON_EXPECTED ] or ,"),
            }
        }
        Ok(JsonVal::Array(arr))
    }

    fn parse_object(&mut self) -> Result<JsonVal> {
        self.expect(b'{')?;
        self.skip_ws();
        let mut map = BTreeMap::new();
        if self.peek() == Some(b'}') { self.pos += 1; return Ok(JsonVal::Object(map)); }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek() {
                Some(b'}') => { self.pos += 1; break; }
                Some(b',') => { self.pos += 1; }
                _ => bail!("JSON_EXPECTED }} or ,"),
            }
        }
        Ok(JsonVal::Object(map))
    }
}


impl JsonVal {
    pub fn get(&self, key: &str) -> Option<&JsonVal> {
        match self { JsonVal::Object(m) => m.get(key), _ => None }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self { JsonVal::Str(s) => Some(s.as_str()), _ => None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self { JsonVal::Bool(b) => Some(*b), _ => None }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self { JsonVal::Int(n) => Some(*n), _ => None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonVal::Float(f) => Some(*f),
            JsonVal::Int(n) => Some(*n as f64),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Vec<JsonVal>> {
        match self { JsonVal::Array(a) => Some(a), _ => None }
    }
    pub fn as_object(&self) -> Option<&std::collections::BTreeMap<String, JsonVal>> {
        match self { JsonVal::Object(m) => Some(m), _ => None }
    }
    pub fn is_null(&self) -> bool { matches!(self, JsonVal::Null) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_primitives() {
        for s in &["null", "true", "false", "42", "-7", "3.14"] {
            let v = from_str(s).unwrap();
            let out = to_string(&v);
            assert_eq!(out, *s, "roundtrip failed for {}", s);
        }
    }

    #[test]
    fn test_string_escape() {
        let v = from_str(r#""hello\nworld""#).unwrap();
        assert_eq!(v, JsonVal::Str("hello\nworld".into()));
        let out = to_string(&v);
        assert!(out.contains("\\n"));
    }

    #[test]
    fn test_object() {
        let v = from_str(r#"{"a":1,"b":true}"#).unwrap();
        match &v {
            JsonVal::Object(m) => {
                assert_eq!(m["a"], JsonVal::Int(1));
                assert_eq!(m["b"], JsonVal::Bool(true));
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn test_nested() {
        let s = r#"{"x":[1,2,{"y":null}]}"#;
        let v = from_str(s).unwrap();
        let out = to_string(&v);
        let v2 = from_str(&out).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn test_unicode_escape() {
        let v = from_str(r#""\u0041""#).unwrap();
        assert_eq!(v, JsonVal::Str("A".into()));
    }
}
