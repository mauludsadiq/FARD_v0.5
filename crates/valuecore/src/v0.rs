use anyhow::{anyhow, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum V {
    Unit,
    Bool(bool),
    Int(i64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<V>),
    Map(Vec<(String, V)>), // canonical encoding sorts by key
    Ok(Box<V>),
    Err(String),
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn push_json_string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(br#"\""#),
            '\\' => out.extend_from_slice(br#"\\"#),
            '\n' => out.extend_from_slice(br#"\n"#),
            '\r' => out.extend_from_slice(br#"\r"#),
            '\t' => out.extend_from_slice(br#"\t"#),
            c if (c as u32) < 0x20 => {
                let u = c as u32;
                let esc = format!("\\u{:04x}", u);
                out.extend_from_slice(esc.as_bytes());
            }
            c => {
                let mut buf = [0u8; 4];
                let n = c.encode_utf8(&mut buf).len();
                out.extend_from_slice(&buf[..n]);
            }
        }
    }
    out.push(b'"');
}


fn encode_into(out: &mut Vec<u8>, v: &V) {
    match v {
        V::Unit => {
            out.extend_from_slice(br#"{"t":"unit"}"#);
        }
        V::Bool(b) => {
            out.extend_from_slice(br#"{"t":"bool","v":"#);
            if *b { out.extend_from_slice(b"true"); } else { out.extend_from_slice(b"false"); }
            out.push(b'}');
        }
        V::Int(i) => {
            out.extend_from_slice(br#"{"t":"int","v":"#);
            out.extend_from_slice(i.to_string().as_bytes());
            out.push(b'}');
        }
        V::Text(s) => {
            out.extend_from_slice(br#"{"t":"text","v":"#);
            push_json_string(out, s);
            out.push(b'}');
        }
        V::Bytes(bs) => {
            out.extend_from_slice(br#"{"t":"bytes","v":"#);
            let h = format!("hex:{}", hex_lower(bs));
            push_json_string(out, &h);
            out.push(b'}');
        }
        V::List(xs) => {
            out.extend_from_slice(br#"{"t":"list","v":["#);
            for (i, x) in xs.iter().enumerate() {
                if i != 0 { out.push(b','); }
                encode_into(out, x);
            }
            out.extend_from_slice(b"]}");
        }
        V::Map(kvs) => {
            let mut kvs2 = kvs.clone();
            kvs2.sort_by(|a, b| a.0.cmp(&b.0));

            out.extend_from_slice(br#"{"t":"map","v":["#);
            for (i, (k, val)) in kvs2.iter().enumerate() {
                if i != 0 { out.push(b','); }
                out.push(b'[');
                push_json_string(out, k);
                out.push(b',');
                encode_into(out, val);
                out.push(b']');
            }
            out.extend_from_slice(b"]}");
        }
        V::Ok(x) => {
            out.extend_from_slice(br#"{"t":"ok","v":"#);
            encode_into(out, x);
            out.push(b'}');
        }
        V::Err(e) => {
            out.extend_from_slice(br#"{"t":"err","e":"#);
            push_json_string(out, e);
            out.push(b'}');
        }
    }
}

pub fn encode_json(v: &V) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(&mut out, v);
    out
}

fn expect_obj<'a>(j: &'a serde_json::Value) -> Result<&'a serde_json::Map<String, serde_json::Value>> {
    j.as_object().ok_or_else(|| anyhow!("ERROR_JSON expected object"))
}

fn expect_str<'a>(j: &'a serde_json::Value) -> Result<&'a str> {
    j.as_str().ok_or_else(|| anyhow!("ERROR_JSON expected string"))
}

fn expect_i64(j: &serde_json::Value) -> Result<i64> {
    j.as_i64().ok_or_else(|| anyhow!("ERROR_JSON expected int"))
}

fn expect_bool(j: &serde_json::Value) -> Result<bool> {
    j.as_bool().ok_or_else(|| anyhow!("ERROR_JSON expected bool"))
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>> {
    let rest = s.strip_prefix("hex:").ok_or_else(|| anyhow!("ERROR_JSON bytes must be hex:..."))?;
    if rest.len() % 2 != 0 {
        return Err(anyhow!("ERROR_JSON hex length must be even"));
    }
    let mut out = Vec::with_capacity(rest.len() / 2);
    let bytes = rest.as_bytes();
    let to_n = |c: u8| -> Result<u8> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(anyhow!("ERROR_JSON invalid hex char")),
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

pub fn decode_json(bytes: &[u8]) -> Result<V> {
    let j: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| anyhow!("ERROR_JSON {}", e))?;
    decode_value(&j)
}

fn decode_value(j: &serde_json::Value) -> Result<V> {
    let obj = expect_obj(j)?;
    let t = obj.get("t").ok_or_else(|| anyhow!("ERROR_JSON missing t"))?;
    let t = expect_str(t)?;

    match t {
        "unit" => Ok(V::Unit),
        "bool" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            Ok(V::Bool(expect_bool(v)?))
        }
        "int" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            Ok(V::Int(expect_i64(v)?))
        }
        "text" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            Ok(V::Text(expect_str(v)?.to_string()))
        }
        "bytes" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            Ok(V::Bytes(parse_hex_bytes(expect_str(v)?)?))
        }
        "list" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            let arr = v.as_array().ok_or_else(|| anyhow!("ERROR_JSON list v must be array"))?;
            let mut out = Vec::with_capacity(arr.len());
            for x in arr {
                out.push(decode_value(x)?);
            }
            Ok(V::List(out))
        }
        "map" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            let arr = v.as_array().ok_or_else(|| anyhow!("ERROR_JSON map v must be array"))?;
            let mut out = Vec::with_capacity(arr.len());
            for pair in arr {
                let p = pair.as_array().ok_or_else(|| anyhow!("ERROR_JSON map pair must be array"))?;
                if p.len() != 2 {
                    return Err(anyhow!("ERROR_JSON map pair len must be 2"));
                }
                let k = expect_str(&p[0])?.to_string();
                let val = decode_value(&p[1])?;
                out.push((k, val));
            }
            out.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(V::Map(out))
        }
        "ok" => {
            let v = obj.get("v").ok_or_else(|| anyhow!("ERROR_JSON missing v"))?;
            Ok(V::Ok(Box::new(decode_value(v)?)))
        }
        "err" => {
            let e = obj.get("e").ok_or_else(|| anyhow!("ERROR_JSON missing e"))?;
            Ok(V::Err(expect_str(e)?.to_string()))
        }
        _ => Err(anyhow!("ERROR_JSON unknown t {}", t)),
    }
}

pub fn i64_add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_add"))
}
pub fn i64_sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_sub"))
}
pub fn i64_mul(a: i64, b: i64) -> Result<i64> {
    a.checked_mul(b).ok_or_else(|| anyhow!("ERROR_OVERFLOW i64_mul"))
}
