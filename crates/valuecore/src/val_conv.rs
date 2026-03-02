//! Conversion between Val (runtime) and Value (wire).
//!
//! Val::Float(f64) -> Value::Bytes(8-byte LE) at wire boundary.
//! Val::Int(i64)   -> Value::Int(BigInt::from(i)) at wire boundary.
//! Val::Record     -> Value::record (sorted, canonical).
//! Val::Err{code,data} -> Value::Err{code,data} (recursive).

use anyhow::{anyhow, Result};
use num_bigint::BigInt;
use crate::val::Val;
use crate::value::Value;

/// Convert a runtime Val to a wire Value.
/// Val::Float is encoded as 8-byte little-endian IEEE 754.
pub fn val_to_value(v: &Val) -> Value {
    match v {
        Val::Unit => Value::Unit,
        Val::Bool(b) => Value::Bool(*b),
        Val::Int(i) => Value::Int(BigInt::from(*i)),
        Val::Float(f) => Value::Bytes(f.to_le_bytes().to_vec()),
        Val::Text(s) => Value::Text(s.clone()),
        Val::Bytes(b) => Value::Bytes(b.clone()),
        Val::List(xs) => Value::List(xs.iter().map(val_to_value).collect()),
        Val::Record(kvs) => {
            // kvs already sorted by Val::record() invariant
            Value::record(kvs.iter().map(|(k, v)| (k.clone(), val_to_value(v))).collect())
        }
        Val::Err { code, data } => Value::Err {
            code: code.clone(),
            data: Box::new(val_to_value(data)),
        },
    }
}

/// Convert a wire Value to a runtime Val.
/// Value::Bytes of exactly 8 bytes is treated as Val::Bytes, not Float —
/// Float is a runtime-only type, not recoverable from wire without schema.
/// Value::Int is narrowed to i64; values outside range return Val::Err.
pub fn value_to_val(v: &Value) -> Result<Val> {
    match v {
        Value::Unit => Ok(Val::Unit),
        Value::Bool(b) => Ok(Val::Bool(*b)),
        Value::Int(z) => {
            use num_traits::ToPrimitive;
            z.to_i64()
                .map(Val::Int)
                .ok_or_else(|| anyhow!("ERROR_OVERFLOW value_to_val: Int {} out of i64 range", z))
        }
        Value::Text(s) => Ok(Val::Text(s.clone())),
        Value::Bytes(b) => Ok(Val::Bytes(b.clone())),
        Value::List(xs) => {
            let vs: Result<Vec<Val>> = xs.iter().map(value_to_val).collect();
            Ok(Val::List(vs?))
        }
        Value::Record(kvs) => {
            let pairs: Result<Vec<(String, Val)>> = kvs
                .iter()
                .map(|(k, v)| value_to_val(v).map(|vv| (k.clone(), vv)))
                .collect();
            // Value::Record is already sorted and unique by invariant
            Ok(Val::Record(pairs?))
        }
        Value::Err { code, data } => Ok(Val::Err {
            code: code.clone(),
            data: Box::new(value_to_val(data)?),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: &Val) {
        let wire = val_to_value(v);
        let back = value_to_val(&wire).expect("value_to_val failed");
        // Float round-trips as Bytes, so only check non-float
        if !matches!(v, Val::Float(_)) {
            assert_eq!(&back, v, "roundtrip failed for {:?}", v);
        }
    }

    #[test]
    fn roundtrip_scalars() {
        roundtrip(&Val::Unit);
        roundtrip(&Val::Bool(true));
        roundtrip(&Val::Bool(false));
        roundtrip(&Val::Int(0));
        roundtrip(&Val::Int(i64::MAX));
        roundtrip(&Val::Int(i64::MIN));
        roundtrip(&Val::Text("hello".to_string()));
        roundtrip(&Val::Bytes(vec![1, 2, 3]));
    }

    #[test]
    fn roundtrip_list() {
        roundtrip(&Val::List(vec![Val::Int(1), Val::Bool(false)]));
    }

    #[test]
    fn roundtrip_record() {
        let r = Val::record(vec![
            ("b".to_string(), Val::Int(2)),
            ("a".to_string(), Val::Int(1)),
        ]);
        roundtrip(&r);
    }

    #[test]
    fn roundtrip_err() {
        let e = Val::err_data("ERROR_FOO", Val::Text("detail".to_string()));
        roundtrip(&e);
    }

    #[test]
    fn float_encodes_as_bytes() {
        let f = Val::Float(1.5_f64);
        let wire = val_to_value(&f);
        assert!(matches!(wire, Value::Bytes(_)));
        if let Value::Bytes(b) = wire {
            assert_eq!(b, 1.5_f64.to_le_bytes().to_vec());
        }
    }

    #[test]
    fn int_overflow_is_error() {
        let big = Value::Int(BigInt::from(i64::MAX) + BigInt::from(1u64));
        let e = value_to_val(&big).unwrap_err();
        assert!(e.to_string().contains("ERROR_OVERFLOW"), "{}", e);
    }
}

/// Convert Val to v0::V for wire serialization in the v0 receipt format.
/// This is the sole crossing point from runtime Val to the v0 wire boundary.
pub fn val_to_v0(v: &Val) -> crate::v0::V {
    use crate::v0::V;
    match v {
        Val::Unit    => V::Unit,
        Val::Bool(b) => V::Bool(*b),
        Val::Int(i)  => V::Int(*i),
        Val::Float(f) => V::Bytes(f.to_le_bytes().to_vec()),
        Val::Text(s) => V::Text(s.clone()),
        Val::Bytes(b) => V::Bytes(b.clone()),
        Val::List(xs) => V::List(xs.iter().map(val_to_v0).collect()),
        Val::Record(kvs) => V::Map(kvs.iter().map(|(k, v)| (k.clone(), val_to_v0(v))).collect()),
        Val::Err { code, .. } => V::Err(code.clone()),
    }
}
