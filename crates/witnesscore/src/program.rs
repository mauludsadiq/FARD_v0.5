use anyhow::Result;
use valuecore::Value;

/// ProgramIdentity :=
/// record([
///   ("kind",  text("fard/program/v0.1")),
///   ("entry", text(EntryModuleName)),
///   ("mods",  list(ModEntry))   // sorted by name
/// ])
///
/// ModEntry := record([("name", text(ModuleName)), ("source", text(CID(module_bytes)))])
pub fn mod_entry_v0_1(name: &str, source_cid: &str) -> Value {
    Value::record(vec![
        ("name".to_string(), Value::text(name)),
        ("source".to_string(), Value::text(source_cid)),
    ])
}

pub fn program_identity_v0_1(entry: &str, mut mods: Vec<Value>) -> Result<Value> {
    // mods sorted by ModuleName (UTF-8 byte order) on the "name" field.
    mods.sort_by(|a, b| mod_name(a).as_bytes().cmp(mod_name(b).as_bytes()));
    Ok(Value::record(vec![
        ("entry".to_string(), Value::text(entry)),
        ("kind".to_string(), Value::text("fard/program/v0.1")),
        ("mods".to_string(), Value::list(mods)),
    ]))
}

fn mod_name(v: &Value) -> &str {
    match v {
        Value::Record(kvs) => {
            for (k, x) in kvs.iter() {
                if k == "name" {
                    if let Value::Text(s) = x {
                        return s.as_str();
                    }
                }
            }
            panic!("mod entry missing name");
        }
        _ => panic!("mod entry not record"),
    }
}
