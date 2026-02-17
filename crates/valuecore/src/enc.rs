use anyhow::Result;
use num_bigint::BigInt;

use crate::canon_hex::emit_hex_lower;
use crate::canon_int::emit_int_string;
use crate::canon_str::emit_json_string;
use crate::value::Value;

pub fn enc(v: &Value) -> Vec<u8> {
    let mut out = Vec::<u8>::new();
    // infallible by construction; string escaping currently returns Result for future-proofing
    enc_value(&mut out, v).expect("enc_value failed");
    out
}

fn enc_value(out: &mut Vec<u8>, v: &Value) -> Result<()> {
    match v {
        Value::Unit => {
            out.extend_from_slice(br#"{"t":"unit"}"#);
            Ok(())
        }
        Value::Bool(b) => {
            out.extend_from_slice(br#"{"t":"bool","v":"#);
            out.extend_from_slice(if *b { b"true" } else { b"false" });
            out.push(b'}');
            Ok(())
        }
        Value::Int(z) => enc_int(out, z),
        Value::Bytes(b) => {
            out.extend_from_slice(br#"{"t":"bytes","v":"#);
            out.push(b'"');
            emit_hex_lower(out, b);
            out.push(b'"');
            out.push(b'}');
            Ok(())
        }
        Value::Text(s) => {
            out.extend_from_slice(br#"{"t":"text","v":"#);
            emit_json_string(out, s)?;
            out.push(b'}');
            Ok(())
        }
        Value::List(xs) => {
            out.extend_from_slice(br#"{"t":"list","v":["#);
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                enc_value(out, x)?;
            }
            out.extend_from_slice(br#"]}"#);
            Ok(())
        }
        Value::Record(kvs) => {
            // kvs must be sorted and unique by Value invariant.
            out.extend_from_slice(br#"{"t":"record","v":["#);
            for (i, (k, val)) in kvs.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.push(b'[');
                emit_json_string(out, k)?;
                out.push(b',');
                enc_value(out, val)?;
                out.push(b']');
            }
            out.extend_from_slice(br#"]}"#);
            Ok(())
        }
        Value::Err { code, data } => {
            // {"t":"err","v":{"code":"<CODE>","data":<ENC_JSON(data)>}}
            out.extend_from_slice(br#"{"t":"err","v":{"code":"#);
            emit_json_string(out, code)?;
            out.extend_from_slice(br#","data":"#);
            enc_value(out, data)?;
            out.extend_from_slice(br#"}}"#);
            Ok(())
        }
    }
}

fn enc_int(out: &mut Vec<u8>, z: &BigInt) -> Result<()> {
    out.extend_from_slice(br#"{"t":"int","v":"#);
    out.push(b'"');
    emit_int_string(out, z);
    out.push(b'"');
    out.push(b'}');
    Ok(())
}
