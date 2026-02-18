use valuecore::{enc, Value};

/// Effect := record([("kind", text(kind)), ("req", V), ("sat", unit|text(CIDstring))])
///
/// Canonical sort key:
///   K(e) = UTF8(kind) || 0x00 || ENC(req)
pub fn effect_key_bytes(effect: &Value) -> Vec<u8> {
    // effect is a record([["kind",...],["req",...],["sat",...]])
    let (kind, req) = extract_kind_req(effect);
    let mut out = Vec::<u8>::new();
    out.extend_from_slice(kind.as_bytes());
    out.push(0u8);
    out.extend_from_slice(&enc(req));
    out
}

pub fn canonicalize_effects(mut effects: Vec<Value>) -> Vec<Value> {
    effects.sort_by(|a, b| effect_key_bytes(a).cmp(&effect_key_bytes(b)));
    effects
}

fn extract_kind_req(effect: &Value) -> (&str, &Value) {
    match effect {
        Value::Record(kvs) => {
            let mut kind: Option<&str> = None;
            let mut req: Option<&Value> = None;
            for (k, v) in kvs.iter() {
                if k == "kind" {
                    if let Value::Text(s) = v {
                        kind = Some(s.as_str());
                    }
                } else if k == "req" {
                    req = Some(v);
                }
            }
            (kind.expect("effect.kind missing or not text"), req.expect("effect.req missing"))
        }
        _ => panic!("effect not record"),
    }
}
