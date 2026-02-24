use valuecore::json::{JsonVal, to_string_pretty};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ModuleGraph {
    pub kind: String,
    pub entry: String,
    pub modules: Vec<ModuleNode>,
}

#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub name: String,
    pub source: String,
}

impl ModuleGraph {
    pub fn single(entry: &str, name: &str, source_cid: &str) -> Self {
        Self {
            kind: "fard/module_graph/v0".to_string(),
            entry: entry.to_string(),
            modules: vec![ModuleNode {
                name: name.to_string(),
                source: source_cid.to_string(),
            }],
        }
    }

    pub fn to_json(&self) -> JsonVal {
        let modules: Vec<JsonVal> = self.modules.iter().map(|m| {
            let mut node = BTreeMap::new();
            node.insert("name".to_string(), JsonVal::Str(m.name.clone()));
            node.insert("source".to_string(), JsonVal::Str(m.source.clone()));
            JsonVal::Object(node)
        }).collect();
        let mut obj = BTreeMap::new();
        obj.insert("entry".to_string(), JsonVal::Str(self.entry.clone()));
        obj.insert("kind".to_string(), JsonVal::Str(self.kind.clone()));
        obj.insert("modules".to_string(), JsonVal::Array(modules));
        JsonVal::Object(obj)
    }

    pub fn to_json_pretty(&self) -> String {
        to_string_pretty(&self.to_json())
    }
}
