//! Unified runtime value type for the FARD evaluator.
//!
//! This type is the single runtime representation used by all evaluators.
//! It maps to/from `valuecore::Value` at the wire boundary only.
//!
//! Variants:
//!   Unit, Bool, Int, Float, Text, Bytes, List, Record — data values
//!   Err { code, data } — structured error, matches wire format
//!
//! Not serializable directly: use val_to_value() + enc() for wire output.

use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<Val>),
    /// Keys are sorted by UTF-8 byte order, unique. Use Val::record() to construct.
    Record(Vec<(String, Val)>),
    /// Structured error. code is an ERROR_* string, data is arbitrary.
    Err { code: String, data: Box<Val> },
}

impl Val {
    /// Construct a Record, sorting keys. Duplicate keys produce Val::Err.
    pub fn record(mut kvs: Vec<(String, Val)>) -> Val {
        kvs.sort_by(|a, b| a.0.cmp(&b.0));
        // check for duplicates after sort
        for i in 1..kvs.len() {
            if kvs[i].0 == kvs[i - 1].0 {
                return Val::Err {
                    code: "ERROR_DUP_KEY".to_string(),
                    data: Box::new(Val::Text(kvs[i].0.clone())),
                };
            }
        }
        Val::Record(kvs)
    }

    /// Construct a simple error with Unit data.
    pub fn err(code: &str) -> Val {
        Val::Err {
            code: code.to_string(),
            data: Box::new(Val::Unit),
        }
    }

    /// Construct an error with attached data.
    pub fn err_data(code: &str, data: Val) -> Val {
        Val::Err {
            code: code.to_string(),
            data: Box::new(data),
        }
    }

    /// Return true if this is any Err variant.
    pub fn is_err(&self) -> bool {
        matches!(self, Val::Err { .. })
    }

    /// Get record field by name. Returns None if not a Record or key missing.
    pub fn get_field(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Record(kvs) => kvs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Type name string for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Val::Unit => "Unit",
            Val::Bool(_) => "Bool",
            Val::Int(_) => "Int",
            Val::Float(_) => "Float",
            Val::Text(_) => "Text",
            Val::Bytes(_) => "Bytes",
            Val::List(_) => "List",
            Val::Record(_) => "Record",
            Val::Err { .. } => "Err",
        }
    }
}

/// Tag rank for canonical ordering (matches v0 order, extended).
fn tag_rank(v: &Val) -> u8 {
    match v {
        Val::Unit => 0,
        Val::Bool(_) => 1,
        Val::Int(_) => 2,
        Val::Float(_) => 3,
        Val::Text(_) => 4,
        Val::Bytes(_) => 5,
        Val::List(_) => 6,
        Val::Record(_) => 7,
        Val::Err { .. } => 8,
    }
}

/// Canonical total order over Val. Records are compared after key-sorting.
pub fn canon_cmp(a: &Val, b: &Val) -> Ordering {
    let ra = tag_rank(a);
    let rb = tag_rank(b);
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Val::Unit, Val::Unit) => Ordering::Equal,
        (Val::Bool(x), Val::Bool(y)) => x.cmp(y),
        (Val::Int(x), Val::Int(y)) => x.cmp(y),
        (Val::Float(x), Val::Float(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Val::Text(x), Val::Text(y)) => x.cmp(y),
        (Val::Bytes(x), Val::Bytes(y)) => x.cmp(y),
        (Val::List(xs), Val::List(ys)) => {
            let n = xs.len().min(ys.len());
            for i in 0..n {
                let c = canon_cmp(&xs[i], &ys[i]);
                if c != Ordering::Equal { return c; }
            }
            xs.len().cmp(&ys.len())
        }
        (Val::Record(xs), Val::Record(ys)) => {
            // both already sorted by Val::record()
            let n = xs.len().min(ys.len());
            for i in 0..n {
                let kc = xs[i].0.cmp(&ys[i].0);
                if kc != Ordering::Equal { return kc; }
                let vc = canon_cmp(&xs[i].1, &ys[i].1);
                if vc != Ordering::Equal { return vc; }
            }
            xs.len().cmp(&ys.len())
        }
        (Val::Err { code: cx, data: dx }, Val::Err { code: cy, data: dy }) => {
            let c = cx.cmp(cy);
            if c != Ordering::Equal { return c; }
            canon_cmp(dx, dy)
        }
        _ => Ordering::Equal,
    }
}

/// Canonical equality: two Vals are equal if canon_cmp returns Equal.
pub fn canon_eq(a: &Val, b: &Val) -> bool {
    canon_cmp(a, b) == Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sorts_keys() {
        let r = Val::record(vec![
            ("b".to_string(), Val::Int(2)),
            ("a".to_string(), Val::Int(1)),
        ]);
        match r {
            Val::Record(kvs) => {
                assert_eq!(kvs[0].0, "a");
                assert_eq!(kvs[1].0, "b");
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn record_dup_key_yields_err() {
        let r = Val::record(vec![
            ("a".to_string(), Val::Int(1)),
            ("a".to_string(), Val::Int(2)),
        ]);
        assert!(r.is_err());
    }

    #[test]
    fn canon_cmp_tag_order() {
        let vals = vec![
            Val::Unit,
            Val::Bool(false),
            Val::Int(0),
            Val::Float(0.0),
            Val::Text(String::new()),
            Val::Bytes(vec![]),
            Val::List(vec![]),
            Val::record(vec![]),
            Val::err("E"),
        ];
        for i in 0..vals.len() {
            for j in 0..vals.len() {
                let c = canon_cmp(&vals[i], &vals[j]);
                if i < j { assert_eq!(c, Ordering::Less, "i={} j={}", i, j); }
                if i == j { assert_eq!(c, Ordering::Equal, "i={} j={}", i, j); }
                if i > j { assert_eq!(c, Ordering::Greater, "i={} j={}", i, j); }
            }
        }
    }

    #[test]
    fn canon_eq_record_order_insensitive() {
        let a = Val::record(vec![
            ("b".to_string(), Val::Int(2)),
            ("a".to_string(), Val::Int(1)),
        ]);
        let b = Val::record(vec![
            ("a".to_string(), Val::Int(1)),
            ("b".to_string(), Val::Int(2)),
        ]);
        assert!(canon_eq(&a, &b));
    }

    #[test]
    fn type_names() {
        assert_eq!(Val::Unit.type_name(), "Unit");
        assert_eq!(Val::Int(0).type_name(), "Int");
        assert_eq!(Val::Float(0.0).type_name(), "Float");
        assert_eq!(Val::err("E").type_name(), "Err");
    }

    #[test]
    fn get_field() {
        let r = Val::record(vec![("x".to_string(), Val::Int(42))]);
        assert_eq!(r.get_field("x"), Some(&Val::Int(42)));
        assert_eq!(r.get_field("y"), None);
    }
}
