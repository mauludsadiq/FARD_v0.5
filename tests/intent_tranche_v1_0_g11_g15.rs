use std::collections::{BTreeSet, HashSet};

fn read_bytes(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("READ_FAIL path={} err={}", path, e))
}

fn json_parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("JSON_PARSE_FAIL err={}", e))
}

fn canon_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Null => serde_json::Value::Null,
        serde_json::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_json::Value::Number(n) => serde_json::Value::Number(n.clone()),
        serde_json::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(canon_value).collect()),
        serde_json::Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canon_value(&m[k]));
            }
            serde_json::Value::Object(out)
        }
    }
}

fn canon_compact_bytes(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&canon_value(v)).expect("CANON_SERIALIZE_FAIL")
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

fn is_canonical_module_path(s: &str) -> bool {
    if !s.starts_with("std/") { return false; }
    let tail = &s[4..];
    if tail.is_empty() { return false; }
    tail.chars().all(is_ident_char)
}

fn as_obj<'a>(v: &'a serde_json::Value, ctx: &str) -> &'a serde_json::Map<String, serde_json::Value> {
    v.as_object().unwrap_or_else(|| panic!("TYPE_FAIL expected=object ctx={}", ctx))
}

fn as_str<'a>(v: &'a serde_json::Value, ctx: &str) -> &'a str {
    v.as_str().unwrap_or_else(|| panic!("TYPE_FAIL expected=string ctx={}", ctx))
}

fn keys_set(obj: &serde_json::Map<String, serde_json::Value>) -> BTreeSet<String> {
    obj.keys().cloned().collect()
}

fn require_export_shape(path: &str, name: &str, v: &serde_json::Value) {
    let ctx = format!("export {}.{}", path, name);
    let o = as_obj(v, &ctx);
    let ks = keys_set(o);

    let k4: BTreeSet<String> = ["intent","pipe","return","status"].iter().map(|s| s.to_string()).collect();
    let k5: BTreeSet<String> = ["intent","pipe","return","status","notes"].iter().map(|s| s.to_string()).collect();

    if ks != k4 && ks != k5 {
        panic!("KEYSET_FAIL ctx={} keys={:?}", ctx, ks);
    }

    let intent = as_str(o.get("intent").unwrap(), &format!("{}.intent", ctx));
    let ret = as_str(o.get("return").unwrap(), &format!("{}.return", ctx));
    let pipe = as_str(o.get("pipe").unwrap(), &format!("{}.pipe", ctx));
    let status = as_str(o.get("status").unwrap(), &format!("{}.status", ctx));

    let intents: HashSet<&'static str> = ["construct","transform","query","effect"].into_iter().collect();
    let rets: HashSet<&'static str> = ["Value","Option","Result"].into_iter().collect();
    let pipes: HashSet<&'static str> = ["Stage","No"].into_iter().collect();
    let statuses: HashSet<&'static str> = ["implemented","planned"].into_iter().collect();

    if !intents.contains(intent) { panic!("ENUM_FAIL ctx={} field=intent val={}", ctx, intent); }
    if !rets.contains(ret) { panic!("ENUM_FAIL ctx={} field=return val={}", ctx, ret); }
    if !pipes.contains(pipe) { panic!("ENUM_FAIL ctx={} field=pipe val={}", ctx, pipe); }
    if !statuses.contains(status) { panic!("ENUM_FAIL ctx={} field=status val={}", ctx, status); }

    if pipe == "Stage" && intent == "construct" {
        panic!("STAGE_CONTRACT_FAIL ctx={} rule=construct_cannot_be_stage", ctx);
    }

    if let Some(n) = o.get("notes") {
        let _ = as_str(n, &format!("{}.notes", ctx));
    }
}

fn load_manifest() -> (Vec<u8>, serde_json::Value) {
    let path = "spec/stdlib_surface.v1_0.ontology.json";
    let bytes = read_bytes(path);
    let v = json_parse(&bytes);
    (bytes, v)
}

#[test]
fn g11_manifest_parseable_and_schema() {
    let (_bytes, v) = load_manifest();

    let top = as_obj(&v, "top");
    let top_keys: Vec<&String> = top.keys().collect();

    let want_top_keys = vec!["modules".to_string(), "schema".to_string()];
    let have_top_keys: Vec<String> = top_keys.iter().map(|s| (*s).clone()).collect();

    if have_top_keys != want_top_keys {
        panic!("TOP_KEY_ORDER_FAIL have={:?} want={:?}", have_top_keys, want_top_keys);
    }

    let schema = as_str(top.get("schema").unwrap(), "top.schema");
    if schema != "fard.stdlib_surface.ontology.v1_0" {
        panic!("SCHEMA_MISMATCH have={} want=fard.stdlib_surface.ontology.v1_0", schema);
    }

    let modules = as_obj(top.get("modules").unwrap(), "top.modules");
    if modules.is_empty() {
        panic!("MODULES_EMPTY");
    }
}

#[test]
fn g12_paths_canonical_and_canonical_json_bytes() {
    let (bytes, v) = load_manifest();
    let canon = canon_compact_bytes(&v);

    let mut want = canon.clone();
    want.push(b'\n');

    if bytes != want {
        let have_len = bytes.len();
        let want_len = want.len();
        panic!("CANON_BYTES_MISMATCH have_len={} want_len={} (normalize via: jq -cS . file > file.tmp && mv file.tmp file)", have_len, want_len);
    }

    let top = as_obj(&v, "top");
    let modules = as_obj(top.get("modules").unwrap(), "top.modules");

    let mut prev: Option<&str> = None;
    for k in modules.keys() {
        if !is_canonical_module_path(k) {
            panic!("MODULE_PATH_FAIL path={} rule=std_lower_ident", k);
        }
        if let Some(p) = prev {
            if p > k.as_str() {
                panic!("MODULE_KEY_ORDER_FAIL prev={} curr={}", p, k);
            }
        }
        prev = Some(k.as_str());
    }
}

#[test]
fn g13_exports_shapes_valid_and_fq_unique() {
    let (_bytes, v) = load_manifest();
    let top = as_obj(&v, "top");
    let modules = as_obj(top.get("modules").unwrap(), "top.modules");

    let mut fq_seen: HashSet<String> = HashSet::new();

    for (mname, mval) in modules {
        let mctx = format!("module {}", mname);
        let mo = as_obj(mval, &mctx);

        let mks = keys_set(mo);
        let want: BTreeSet<String> = ["exports"].iter().map(|s| s.to_string()).collect();
        if mks != want {
            panic!("MODULE_KEYSET_FAIL ctx={} keys={:?}", mctx, mks);
        }

        let exports = as_obj(mo.get("exports").unwrap(), &format!("{}.exports", mctx));

        let mut prev: Option<&str> = None;
        for (ename, eval) in exports {
            if let Some(p) = prev {
                if p > ename.as_str() {
                    panic!("EXPORT_KEY_ORDER_FAIL module={} prev={} curr={}", mname, p, ename);
                }
            }
            prev = Some(ename.as_str());

            require_export_shape(mname, ename, eval);

            let fq = format!("{}.{}", mname, ename);
            if !fq_seen.insert(fq.clone()) {
                panic!("FQ_EXPORT_DUPLICATE fq={}", fq);
            }
        }
    }
}

#[test]
fn g14_stage_value_first_contract() {
    let (_bytes, v) = load_manifest();
    let top = as_obj(&v, "top");
    let modules = as_obj(top.get("modules").unwrap(), "top.modules");

    for (mname, mval) in modules {
        let mo = as_obj(mval, &format!("module {}", mname));
        let exports = as_obj(mo.get("exports").unwrap(), &format!("module {} exports", mname));
        for (ename, eval) in exports {
            let ectx = format!("{}.{}", mname, ename);
            let eo = as_obj(eval, &format!("export {}", ectx));
            let pipe = as_str(eo.get("pipe").unwrap(), &format!("export {} pipe", ectx));
            if pipe == "Stage" {
                let intent = as_str(eo.get("intent").unwrap(), &format!("export {} intent", ectx));
                if intent == "construct" {
                    panic!("STAGE_CONTRACT_FAIL export={} rule=construct_cannot_be_stage", ectx);
                }
            }
        }
    }
}

fn require_module_exports_present(
    modules: &serde_json::Map<String, serde_json::Value>,
    module: &str,
    exports: &[&str],
) {
    let mv = modules.get(module).unwrap_or_else(|| panic!("MIN_SURFACE_MISSING_MODULE {}", module));
    let mo = as_obj(mv, &format!("min_surface {}", module));
    let ex = as_obj(mo.get("exports").unwrap(), &format!("min_surface {} exports", module));

    for e in exports {
        if !ex.contains_key(*e) {
            panic!("MIN_SURFACE_MISSING_EXPORT module={} export={}", module, e);
        }
    }
}

#[test]
fn g15_minimum_1_0_surface_present() {
    let (_bytes, v) = load_manifest();
    let top = as_obj(&v, "top");
    let modules = as_obj(top.get("modules").unwrap(), "top.modules");

    require_module_exports_present(modules, "std/result", &["Ok","Err","andThen"]);
    require_module_exports_present(modules, "std/list", &["len","hist_int","sort_by_int_key"]);
    require_module_exports_present(modules, "std/str", &["len","concat"]);
    require_module_exports_present(modules, "std/json", &["encode","decode"]);
    require_module_exports_present(modules, "std/map", &["get","set","keys","values","has"]);
    require_module_exports_present(modules, "std/grow", &["append","merge"]);
    require_module_exports_present(modules, "std/flow", &["id","tap"]);
}
