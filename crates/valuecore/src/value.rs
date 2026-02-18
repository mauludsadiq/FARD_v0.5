use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(BigInt),
    Bytes(Vec<u8>),
    Text(String),
    List(Vec<Value>),
    // Invariant for Value::Record:
    // - keys strictly unique
    // - entries sorted by UTF-8 byte order of key
    Record(Vec<(String, Value)>),
    Err { code: String, data: Box<Value> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueTag {
    Unit,
    Bool,
    Int,
    Bytes,
    Text,
    List,
    Record,
    Err,
}

impl ValueTag {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueTag::Unit => "unit",
            ValueTag::Bool => "bool",
            ValueTag::Int => "int",
            ValueTag::Bytes => "bytes",
            ValueTag::Text => "text",
            ValueTag::List => "list",
            ValueTag::Record => "record",
            ValueTag::Err => "err",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unit" => Some(ValueTag::Unit),
            "bool" => Some(ValueTag::Bool),
            "int" => Some(ValueTag::Int),
            "bytes" => Some(ValueTag::Bytes),
            "text" => Some(ValueTag::Text),
            "list" => Some(ValueTag::List),
            "record" => Some(ValueTag::Record),
            "err" => Some(ValueTag::Err),
            _ => None,
        }
    }
}

impl Value {
    pub fn unit() -> Self {
        Value::Unit
    }
    pub fn bool_(b: bool) -> Self {
        Value::Bool(b)
    }
    pub fn int(i: BigInt) -> Self {
        Value::Int(i)
    }
    pub fn text<S: Into<String>>(s: S) -> Self {
        Value::Text(s.into())
    }
    pub fn bytes(b: Vec<u8>) -> Self {
        Value::Bytes(b)
    }
    pub fn list(xs: Vec<Value>) -> Self {
        Value::List(xs)
    }

    // Duplicate keys rule (constructor totality):
    // If construction attempted with duplicate keys, return:
    // err("ERROR_DUP_KEY", record([("key",text(k)),("value",unit)]))
    // where k is the first duplicated key encountered in source order.
    pub fn record(kvs: Vec<(String, Value)>) -> Self {
        use std::collections::HashSet;

        let mut seen: HashSet<&str> = HashSet::new();
        for (k, _v) in kvs.iter() {
            if !seen.insert(k.as_str()) {
                return Value::Err {
                    code: "ERROR_DUP_KEY".to_string(),
                    data: Box::new(Value::Record(vec![
                        ("key".to_string(), Value::Text(k.clone())),
                        ("value".to_string(), Value::Unit),
                    ])),
                };
            }
        }

        let mut out = kvs;
        out.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
        Value::Record(out)
    }

    // Internal helper for decoder after strict validation + sorting.
    pub(crate) fn record_checked_sorted(kvs_sorted_unique: Vec<(String, Value)>) -> Self {
        Value::Record(kvs_sorted_unique)
    }

    pub fn err<S: Into<String>>(code: S, data: Value) -> Self {
        Value::Err {
            code: code.into(),
            data: Box::new(data),
        }
    }
}
