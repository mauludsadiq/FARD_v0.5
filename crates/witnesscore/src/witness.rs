use anyhow::Result;
use valuecore::{vdig, Value};

use crate::effects::canonicalize_effects;

/// Trace := record([("kind", text("fard/trace/v0.1")), ("cid", unit|text(CIDstring))])
pub fn trace_v0_1(cid: Value) -> Value {
    // caller must supply Value::Unit or Value::Text("sha256:...")
    Value::record(vec![
        ("cid".to_string(), cid),
        ("kind".to_string(), Value::text("fard/trace/v0.1")),
    ])
}

/// ImportUse := record([("run", text(RunID)), ("result", text(VDIG(import_value)))])
pub fn import_use_v0(runid: &str, imported_value: &Value) -> Value {
    Value::record(vec![
        ("result".to_string(), Value::text(vdig(imported_value))),
        ("run".to_string(), Value::text(runid)),
    ])
}

/// imports list MUST be sorted by RunID (UTF-8 byte order).
pub fn import_uses_sorted(mut uses: Vec<Value>) -> Vec<Value> {
    uses.sort_by(|a, b| import_run(a).as_bytes().cmp(import_run(b).as_bytes()));
    uses
}

fn import_run(v: &Value) -> &str {
    match v {
        Value::Record(kvs) => {
            for (k, x) in kvs.iter() {
                if k == "run" {
                    if let Value::Text(s) = x {
                        return s.as_str();
                    }
                }
            }
            panic!("import use missing run");
        }
        _ => panic!("import use not record"),
    }
}

/// Witness v0.1 (always emit the 7-key form, even when empty):
///
/// record([
///  ("effects", list(Effect)),
///  ("imports", list(ImportUse)),
///  ("input",   text(VDIG(I))),
///  ("kind",    text("fard/witness/v0.1")),
///  ("program", ProgramIdentity),
///  ("result",  V),
///  ("trace",   Trace)
/// ])
pub fn witness_v0_1(
    program_identity: Value,
    input_value: &Value,
    effects: Vec<Value>,
    imports: Vec<Value>,
    result: Value,
    trace: Value,
) -> Result<Value> {
    let effects = canonicalize_effects(effects);
    let imports = import_uses_sorted(imports);

    Ok(Value::record(vec![
        ("effects".to_string(), Value::list(effects)),
        ("imports".to_string(), Value::list(imports)),
        ("input".to_string(), Value::text(vdig(input_value))),
        ("kind".to_string(), Value::text("fard/witness/v0.1")),
        ("program".to_string(), program_identity),
        ("result".to_string(), result),
        ("trace".to_string(), trace),
    ]))
}
