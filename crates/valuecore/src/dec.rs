use anyhow::{anyhow, bail, Result};

use crate::canon_hex::parse_hex_lower;
use crate::canon_int::parse_int_string;
use crate::json::{self, JsonVal};
use crate::value::{Value, ValueTag};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct DecodeError {
    pub code: String,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for DecodeError {}

pub fn dec(bytes: &[u8]) -> std::result::Result<Value, DecodeError> {
    match dec_inner(bytes) {
        Ok(v) => Ok(v),
        Err(e) => Err(DecodeError { code: format!("{}", e) }),
    }
}

fn dec_inner(bytes: &[u8]) -> Result<Value> {
    let j = json::from_slice(bytes).map_err(|_| anyhow!("DECODE_BAD_JSON"))?;
    decode_value(&j)
}

fn as_object(j: &JsonVal) -> Option<&BTreeMap<String, JsonVal>> {
    match j { JsonVal::Object(m) => Some(m), _ => None }
}
fn as_str(j: &JsonVal) -> Option<&str> {
    match j { JsonVal::Str(s) => Some(s.as_str()), _ => None }
}
fn as_bool(j: &JsonVal) -> Option<bool> {
    match j { JsonVal::Bool(b) => Some(*b), _ => None }
}
fn as_array(j: &JsonVal) -> Option<&Vec<JsonVal>> {
    match j { JsonVal::Array(a) => Some(a), _ => None }
}

fn decode_value(j: &JsonVal) -> Result<Value> {
    let obj = as_object(j).ok_or_else(|| anyhow!("DECODE_NOT_OBJECT"))?;
    let t = obj.get("t").and_then(|x| as_str(x))
        .ok_or_else(|| anyhow!("DECODE_MISSING_T"))?;
    let tag = ValueTag::parse(t).ok_or_else(|| anyhow!("DECODE_UNKNOWN_T"))?;

    match tag {
        ValueTag::Unit => {
            if obj.len() != 1 { bail!("DECODE_EXTRA_KEYS"); }
            Ok(Value::Unit)
        }
        ValueTag::Bool => {
            require_keys(obj, &["t", "v"])?;
            let b = obj.get("v").and_then(|x| as_bool(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_BOOL"))?;
            Ok(Value::Bool(b))
        }
        ValueTag::Int => {
            require_keys(obj, &["t", "v"])?;
            let s = obj.get("v").and_then(|x| as_str(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_INT"))?;
            Ok(Value::Int(parse_int_string(s)?))
        }
        ValueTag::Bytes => {
            require_keys(obj, &["t", "v"])?;
            let s = obj.get("v").and_then(|x| as_str(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_HEX"))?;
            Ok(Value::Bytes(parse_hex_lower(s)?))
        }
        ValueTag::Text => {
            require_keys(obj, &["t", "v"])?;
            let s = obj.get("v").and_then(|x| as_str(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_TEXT"))?;
            Ok(Value::Text(s.to_string()))
        }
        ValueTag::List => {
            require_keys(obj, &["t", "v"])?;
            let arr = obj.get("v").and_then(|x| as_array(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_LIST"))?;
            let mut xs = Vec::with_capacity(arr.len());
            for it in arr { xs.push(decode_value(it)?); }
            Ok(Value::List(xs))
        }
        ValueTag::Record => {
            require_keys(obj, &["t", "v"])?;
            let arr = obj.get("v").and_then(|x| as_array(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_RECORD"))?;
            let mut kvs: Vec<(String, Value)> = Vec::with_capacity(arr.len());
            for pair in arr {
                let p = as_array(pair).ok_or_else(|| anyhow!("DECODE_BAD_RECORD"))?;
                if p.len() != 2 { bail!("DECODE_BAD_RECORD"); }
                let k = as_str(&p[0]).ok_or_else(|| anyhow!("DECODE_BAD_RECORD"))?.to_string();
                let v = decode_value(&p[1])?;
                kvs.push((k, v));
            }
            {
                use std::collections::HashSet;
                let mut seen = HashSet::<&str>::new();
                for (k, _) in kvs.iter() {
                    if !seen.insert(k.as_str()) { bail!("DECODE_DUP_KEY"); }
                }
            }
            kvs.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
            Ok(Value::record_checked_sorted(kvs))
        }
        ValueTag::Err => {
            require_keys(obj, &["t", "v"])?;
            let vobj = obj.get("v").and_then(|x| as_object(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_ERR"))?;
            require_keys(vobj, &["code", "data"])?;
            let code = vobj.get("code").and_then(|x| as_str(x))
                .ok_or_else(|| anyhow!("DECODE_BAD_ERR"))?;
            if code.is_empty() { bail!("DECODE_BAD_ERR"); }
            let data = decode_value(vobj.get("data").unwrap())?;
            Ok(Value::Err { code: code.to_string(), data: Box::new(data) })
        }
    }
}

fn require_keys(obj: &BTreeMap<String, JsonVal>, keys: &[&str]) -> Result<()> {
    if obj.len() != keys.len() { bail!("DECODE_BAD_KEYS"); }
    for k in keys {
        if !obj.contains_key(*k) { bail!("DECODE_BAD_KEYS"); }
    }
    Ok(())
}
