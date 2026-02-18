use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleGraph {
    pub kind: String,
    pub entry: String,
    pub modules: Vec<ModuleNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
