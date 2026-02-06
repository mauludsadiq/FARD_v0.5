#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde::Deserialize;


use fard_v0_5_language_gate::builtin_sig_table_v1::builtin_sig_table_v1;
use fard_v0_5_language_gate::builtin_pipe_v1::stage_allowlist_v1;
#[derive(Debug, Deserialize)]
struct Ontology {
    schema: String,
    generated_at: String,
    annotations: Annotations,
    modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
struct Annotations {
    intent_class: Vec<String>,
    return_meaning: Vec<String>,
    pipeline_eligibility: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Module {
    name: String,   // e.g. "std/list"
    family: String,
    exports: Vec<Export>,
}

#[derive(Debug, Deserialize)]
struct Export {
    name: String,   // e.g. "map"
    #[serde(rename = "intent")]
    intent_class: String,
    #[serde(rename = "return")]
    return_meaning: String,
    pipe: String,   // "Stage" | "No"
    #[serde(default)]
    notes: Option<String>,
}
#[test]
fn stdlib_surface_ontology_gate_v1() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/spec/stdlib_surface_tables.v1_0.ontology.json", root);

    let bytes = fs::read(&path).expect("read stdlib ontology json");
    let s = String::from_utf8(bytes).expect("utf8");

    assert!(!s.contains("\"Yes\""), "ontology json must not contain pipe tag \"Yes\" anywhere");

    let ont: Ontology = serde_json::from_str(&s).expect("parse ontology json");

    assert_eq!(ont.schema, "fard.stdlib.surface_tables.v1.0.ontology");

    // A) annotation keys must be exactly Stage|No (order irrelevant)
    let keys: BTreeSet<String> = ont.annotations.pipeline_eligibility.keys().cloned().collect();
    let expected: BTreeSet<String> = ["Stage", "No"].into_iter().map(|x| x.to_string()).collect();
    assert_eq!(keys, expected, "pipeline_eligibility keys must be exactly Stage|No");

    // B) every export .pipe must be Stage|No
    for m in &ont.modules {
        for e in &m.exports {
            assert!(
                e.pipe == "Stage" || e.pipe == "No",
                "illegal pipe tag: module={} export={} pipe={}",
                m.name,
                e.name,
                e.pipe
            );
        }
    }

    // C) Stage/value-first is derived from runtime tables:
    //    - builtin_sig_table_v1(): signature shape truth
    //    - stage_allowlist_v1(): Stage policy (pipe map)
    let sigs = builtin_sig_table_v1();
    let stage = stage_allowlist_v1();

    // C0) allowlist policy must only name real, value-first builtins
    for fq in stage.iter() {
        let sig = sigs.get(*fq).unwrap_or_else(|| panic!("Stage allowlist entry missing from sig table: {}", fq));
        assert!(sig.arity_min >= 1, "Stage allowlist entry must accept >=1 arg: {} arity_min={}", fq, sig.arity_min);
        assert!(sig.value_first, "Stage allowlist entry must be value-first per sig table: {}", fq);
    }

    // C1) ontology Stage set must exactly equal the runtime Stage allowlist
    let mut stage_from_ont: BTreeSet<String> = BTreeSet::new();
    for m in &ont.modules {
        for e in &m.exports {
            if e.pipe == "Stage" {
                stage_from_ont.insert(format!("{}::{}", m.name, e.name));
            }
        }
    }

    let stage_from_runtime: BTreeSet<String> = stage.iter().map(|x| x.to_string()).collect();

    // C1a) runtime Stage allowlist must not name exports absent from the ontology surface
    let mut all_exports: BTreeSet<String> = BTreeSet::new();
    for m in &ont.modules {
        for e in &m.exports {
            all_exports.insert(format!("{}::{}", m.name, e.name));
        }
    }
    for fq in stage_from_runtime.iter() {
        assert!(
            all_exports.contains(fq),
            "Stage allowlist names non-existent ontology export: {}",
            fq
        );
    }

    assert_eq!(stage_from_ont, stage_from_runtime, "ontology Stage set must equal runtime stage_allowlist_v1");

    // C2) every ontology Stage export must be value-first according to the runtime sig table
    for fq in stage_from_runtime.iter() {
        let sig = sigs.get(fq.as_str()).unwrap_or_else(|| {
            panic!("missing signature table entry for Stage export: {} (add to builtin_sig_table_v1)", fq)
        });
        assert!(sig.arity_min >= 1, "Stage export must accept at least 1 arg: {} arity_min={}", fq, sig.arity_min);
        assert!(sig.value_first, "Stage export must be value-first: {}", fq);
    }
}
