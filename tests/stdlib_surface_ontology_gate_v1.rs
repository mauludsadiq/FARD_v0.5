#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde::Deserialize;

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

#[derive(Debug, Clone, Copy)]
struct Sig {
    arity_min: usize,
    value_first: bool,
}

/// Test-local “Rust builtin signature table” keyed by fully-qualified surface name:
///     "<module>::<export>"
///
/// This is deliberately ontology-facing:
/// - It does NOT encode runtime behavior.
/// - It encodes only the *signature shape contract* needed for Stage enforcement.
fn builtin_sig_table_v1() -> BTreeMap<&'static str, Sig> {
    let mut m: BTreeMap<&'static str, Sig> = BTreeMap::new();

    // Helpers
    let vf1 = Sig { arity_min: 1, value_first: true };
    let vf2 = Sig { arity_min: 2, value_first: true };
    let vf3 = Sig { arity_min: 3, value_first: true };

    // ----------------
    // std/result
    // ----------------
    m.insert("std/result::isOk", vf1);
    m.insert("std/result::isErr", vf1);
    m.insert("std/result::map", vf2);        // (r, f)
    m.insert("std/result::mapErr", vf2);     // (r, f)
    m.insert("std/result::andThen", vf2);    // (r, f)
    m.insert("std/result::orElse", vf2);     // (r, f)
    m.insert("std/result::unwrapOr", vf2);   // (r, default)
    m.insert("std/result::unwrapOrElse", vf2); // (r, f)
    m.insert("std/result::toOption", vf1);   // (r)
    m.insert("std/result::fromOption", vf2); // (opt, err)

    // ----------------
    // std/option
    // ----------------
    m.insert("std/option::isSome", vf1);
    m.insert("std/option::isNone", vf1);
    m.insert("std/option::map", vf2);        // (o, f)
    m.insert("std/option::andThen", vf2);    // (o, f)
    m.insert("std/option::unwrapOr", vf2);   // (o, default)
    m.insert("std/option::unwrapOrElse", vf2);// (o, f)
    m.insert("std/option::toResult", vf2);   // (o, err)
    m.insert("std/option::fromNullable", vf1); // (x)
    m.insert("std/option::toNullable", vf1); // (o)

    // ----------------
    // std/null
    // ----------------
    m.insert("std/null::isNull", vf1);
    m.insert("std/null::coalesce", vf2);       // (x, y)
    m.insert("std/null::guardNotNull", vf2);   // (x, err)

    // ----------------
    // std/bool
    // ----------------
    m.insert("std/bool::not", vf1);

    // ----------------
    // std/int
    // ----------------
    m.insert("std/int::abs", vf1);
    m.insert("std/int::clamp", vf3);         // (n, lo, hi)
    m.insert("std/int::toString", vf1);
    m.insert("std/int::parse", vf1);         // (s)

    // ----------------
    // std/num
    // ----------------
    m.insert("std/num::round", vf1);
    m.insert("std/num::floor", vf1);
    m.insert("std/num::ceil", vf1);
    m.insert("std/num::sqrt", vf1);
    m.insert("std/num::log", vf1);
    m.insert("std/num::exp", vf1);

    // ----------------
    // std/str
    // ----------------
    m.insert("std/str::len", vf1);
    m.insert("std/str::trim", vf1);
    m.insert("std/str::toLower", vf1);
    m.insert("std/str::toUpper", vf1);
    m.insert("std/str::split", vf2);         // (s, sep)
    m.insert("std/str::join", vf2);          // (parts, sep)  <-- Stage contract
    m.insert("std/str::replace", vf3);       // (s, from, to)
    m.insert("std/str::contains", vf2);      // (s, sub)
    m.insert("std/str::startsWith", vf2);    // (s, prefix)
    m.insert("std/str::endsWith", vf2);      // (s, suffix)
    m.insert("std/str::slice", vf3);         // (s, lo, hi)
    m.insert("std/str::padLeft", vf3);       // (s, n, ch)
    m.insert("std/str::padRight", vf3);      // (s, n, ch)

    // ----------------
    // std/list
    // ----------------
    m.insert("std/list::len", vf1);
    m.insert("std/list::isEmpty", vf1);
    m.insert("std/list::push", vf2);         // (xs, x)
    m.insert("std/list::map", vf2);          // (xs, f)
    m.insert("std/list::filter", vf2);       // (xs, pred)
    m.insert("std/list::flatMap", vf2);      // (xs, f)
    m.insert("std/list::fold", Sig { arity_min: 3, value_first: true }); // (xs, init, f)
    m.insert("std/list::sum", vf1);
    m.insert("std/list::min", vf1);
    m.insert("std/list::max", vf1);
    m.insert("std/list::take", vf2);         // (xs, n)
    m.insert("std/list::drop", vf2);         // (xs, n)
    m.insert("std/list::slice", vf3);        // (xs, lo, hi)
    m.insert("std/list::enumerate", vf1);
    m.insert("std/list::groupBy", vf2);      // (xs, keyFn)
    m.insert("std/list::sort", vf1);
    m.insert("std/list::sortBy", vf2);       // (xs, keyFn)
    m.insert("std/list::stableSortBy", vf2); // (xs, keyFn)
    m.insert("std/list::unique", vf1);
    m.insert("std/list::uniqueBy", vf2);     // (xs, keyFn)
    m.insert("std/list::chunk", vf2);        // (xs, n)

    // ----------------
    // std/rec
    // ----------------
    m.insert("std/rec::keys", vf1);
    m.insert("std/rec::values", vf1);
    m.insert("std/rec::has", vf2);           // (r, key)
    m.insert("std/rec::get", vf2);           // (r, key)
    m.insert("std/rec::getOr", Sig { arity_min: 3, value_first: true }); // (r, key, default)
    m.insert("std/rec::getOrErr", Sig { arity_min: 3, value_first: true }); // (r, key, err)
    m.insert("std/rec::set", Sig { arity_min: 3, value_first: true });   // (r, key, value)
    m.insert("std/rec::remove", vf2);        // (r, key)
    m.insert("std/rec::select", vf2);        // (r, keys)
    m.insert("std/rec::rename", vf2);        // (r, mapping)
    m.insert("std/rec::update", vf3);        // (r, key, f)

    // ----------------
    // std/json
    // ----------------
    m.insert("std/json::decode", vf1);       // (text)
    m.insert("std/json::encode", vf1);       // (value)
    m.insert("std/json::parse", vf1);
    m.insert("std/json::stringify", vf1);
    m.insert("std/json::pretty", vf1);
    m.insert("std/json::pathGet", vf2);      // (value, path)
    m.insert("std/json::pathSet", vf3);      // (value, path, newValue)

    // ----------------
    // std/csv
    // ----------------
    m.insert("std/csv::parse", vf1);
    m.insert("std/csv::encode", vf1);
    m.insert("std/csv::withHeader", vf1);
    m.insert("std/csv::toRecords", vf1);

    // ----------------
    // std/bytes
    // ----------------
    m.insert("std/bytes::len", vf1);
    m.insert("std/bytes::slice", vf3);       // (b, lo, hi)
    m.insert("std/bytes::toHex", vf1);
    m.insert("std/bytes::fromHex", vf1);

    // ----------------
    // std/hash
    // ----------------
    m.insert("std/hash::sha256", vf1);
    m.insert("std/hash::sha256Text", vf1);
    m.insert("std/hash::toHex", vf1);

    // ----------------
    // std/path
    // ----------------
    m.insert("std/path::normalize", vf1);
    m.insert("std/path::dir", vf1);
    m.insert("std/path::base", vf1);
    m.insert("std/path::ext", vf1);
    m.insert("std/path::isAbs", vf1);
    m.insert("std/path::joinAll", vf2);      // (parts, sep?) – ontology-level: (parts, ...) value-first

    // ----------------
    // std/fs
    // ----------------
    m.insert("std/fs::read", vf1);
    m.insert("std/fs::open", vf1);
    m.insert("std/fs::create", vf1);
    m.insert("std/fs::close", vf1);
    m.insert("std/fs::readAll", vf1);
    m.insert("std/fs::writeAll", vf2);       // (handle, data)
    m.insert("std/fs::exists", vf1);
    m.insert("std/fs::listDir", vf1);

    // ----------------
    // std/http
    // ----------------
    m.insert("std/http::get", vf1);
    m.insert("std/http::post", vf2);         // (url, body)
    m.insert("std/http::request", vf1);
    m.insert("std/http::okOr", Sig { arity_min: 3, value_first: true }); // (resp, allowed, err)

    // ----------------
    // std/time
    // ----------------
    m.insert("std/time::parse", vf1);
    m.insert("std/time::format", vf1);
    m.insert("std/time::add", vf2);          // (t, dur)
    m.insert("std/time::sub", vf2);          // (t, dur)

    // ----------------
    // std/trace
    // ----------------
    m.insert("std/trace::info", vf1);
    m.insert("std/trace::warn", vf1);
    m.insert("std/trace::error", vf1);

    // ----------------
    // std/artifact
    // ----------------
    m.insert("std/artifact::import", vf1);
    // ----------------
    // std/schema
    // ----------------
    m.insert("std/schema::check", Sig { arity_min: 2, value_first: true }); // (value, schema)


    m
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

    // C) every Stage export must be value-first per Rust signature table
    let sigs = builtin_sig_table_v1();

    for m in &ont.modules {
        for e in &m.exports {
            if e.pipe != "Stage" {
                continue;
            }

            let fq = format!("{}::{}", m.name, e.name);

            let sig = sigs.get(fq.as_str()).unwrap_or_else(|| {
                panic!(
                    "missing signature table entry for Stage export: {} (add to builtin_sig_table_v1)",
                    fq
                )
            });

            assert!(
                sig.arity_min >= 1,
                "Stage export must accept at least 1 arg: {} arity_min={}",
                fq,
                sig.arity_min
            );

            assert!(
                sig.value_first,
                "Stage export must be value-first: {}",
                fq
            );
        }
    }
}
