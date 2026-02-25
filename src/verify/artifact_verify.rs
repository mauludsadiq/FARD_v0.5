use std::collections::BTreeSet;
use valuecore::json::{JsonVal, from_slice, from_str as json_from_str, escape_string};
use std::fs;

fn is_sha256(s: &str) -> bool {
    if !s.starts_with("sha256:") {
        return false;
    }
    let h = &s[7..];
    if h.len() != 64 {
        return false;
    }
    h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn cid_hex(s: &str) -> Result<&str, String> {
    if !is_sha256(s) {
        return Err("M3_BAD_CID".into());
    }
    Ok(&s[7..])
}

fn canon_json(v: &JsonVal) -> Result<String, String> {
    fn canon_value(v: &JsonVal, out: &mut String) -> Result<(), String> {
        match v {
            JsonVal::Null => {
                out.push_str("null");
                Ok(())
            }
            JsonVal::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            JsonVal::Float(f) => {
                let s = format!("{}", f);
                if s.contains('+') { return Err("CANON_NUM_PLUS".into()); }
                if s.ends_with(".0") { return Err("CANON_NUM_DOT0".into()); }
                out.push_str(&s);
                Ok(())
            }
            JsonVal::Int(n) => {
                let s = n.to_string();
                if s.contains('+') {
                    return Err("M3_CANON_NUM_PLUS".into());
                }
                if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") {
                    return Err("M3_CANON_NUM_LEADING_ZERO".into());
                }
                if s.ends_with(".0") {
                    return Err("M3_CANON_NUM_DOT0".into());
                }
                out.push_str(&s);
                Ok(())
            }
            JsonVal::Str(s) => {
                out.push_str(&escape_string(s));
                Ok(())
            }
            JsonVal::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    canon_value(x, out)?;
                }
                out.push(']');
                Ok(())
            }
            JsonVal::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort_by(|a, b| {
                    if a.as_str() == "t" && b.as_str() != "t" {
                        return std::cmp::Ordering::Less;
                    }
                    if a.as_str() != "t" && b.as_str() == "t" {
                        return std::cmp::Ordering::Greater;
                    }
                    a.cmp(b)
                });
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&escape_string(k));
                    out.push(':');
                    canon_value(&m[*k], out)?;
                }
                out.push('}');
                Ok(())
            }
        }
    }
    let mut out = String::new();
    canon_value(v, &mut out)?;
    Ok(out)
}

fn expect_only_keys(
    obj: &std::collections::BTreeMap<String, JsonVal>,
    allowed: &[&str],
) -> Result<(), String> {
    let allow: BTreeSet<&str> = allowed.iter().copied().collect();
    for k in obj.keys() {
        if !allow.contains(k.as_str()) {
            return Err(format!("M3_EXTRA_KEY {}", k));
        }
    }
    Ok(())
}

fn expect_str<'a>(
    obj: &'a std::collections::BTreeMap<String, JsonVal>,
    k: &str,
) -> Result<&'a str, String> {
    obj.get(k)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("M3_EXPECT_STRING {}", k))
}

pub fn verify_artifact_outdir(outdir: &str) -> Result<(), String> {
    let digests_p = format!("{}/digests.json", outdir);
    let digests_bytes = fs::read(&digests_p).map_err(|_| "M3_MISSING_digests.json".to_string())?;
    let digests_v: JsonVal =
        from_slice(&digests_bytes).map_err(|_| "M3_DIGESTS_PARSE_FAIL".to_string())?;
    let files_obj = digests_v
        .get("files")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "M3_DIGESTS_MISSING_files".to_string())?;
    if !files_obj.contains_key("artifact_graph.json") {
        return Err("M3_DIGESTS_MISSING_artifact_graph".into());
    }

    let graph_p = format!("{}/artifact_graph.json", outdir);
    let graph_bytes =
        fs::read(&graph_p).map_err(|_| "M3_MISSING_artifact_graph.json".to_string())?;
    let graph_s = std::str::from_utf8(&graph_bytes).map_err(|_| "M3_GRAPH_NOT_UTF8".to_string())?;
    if graph_s.contains('\r') {
        return Err("M3_GRAPH_HAS_CR".into());
    }
    if graph_s.ends_with(' ') || graph_s.ends_with('\t') {
        return Err("M3_GRAPH_TRAILING_SPACE".into());
    }

    if !graph_s.ends_with("\n") {
        return Err("M3_GRAPH_MISSING_FINAL_NL".into());
    }
    if graph_s.ends_with("\n\n") {
        return Err("M3_GRAPH_EXTRA_FINAL_NL".into());
    }
    let trimmed = graph_s.strip_suffix("\n").unwrap();
    let graph_v: JsonVal =
        from_slice(&graph_bytes).map_err(|_| "M3_GRAPH_PARSE_FAIL".to_string())?;
    let canon = canon_json(&graph_v)?;
    if canon != trimmed {
        return Err("M3_GRAPH_CANON_MISMATCH".into());
    }

    let gobj = graph_v
        .as_object()
        .ok_or_else(|| "M3_GRAPH_NOT_OBJECT".to_string())?;
    expect_only_keys(gobj, &["edges", "nodes", "v"])?;
    let vver = gobj
        .get("v")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "M3_GRAPH_MISSING_v".to_string())?;
    if vver != "0.1.0" {
        return Err("M3_GRAPH_BAD_v".into());
    }

    let nodes = gobj
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "M3_GRAPH_MISSING_nodes".to_string())?;
    let edges = gobj
        .get("edges")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "M3_GRAPH_MISSING_edges".to_string())?;

    let mut node_cids: BTreeSet<String> = BTreeSet::new();

    for n in nodes {
        let no = n
            .as_object()
            .ok_or_else(|| "M3_NODE_NOT_OBJECT".to_string())?;
        expect_only_keys(no, &["cid", "name", "role"])?;
        let cid = expect_str(no, "cid")?;
        if !is_sha256(cid) {
            return Err("M3_NODE_BAD_CID".into());
        }
        let _name = expect_str(no, "name")?;
        let role = expect_str(no, "role")?;
        if !matches!(role, "in" | "out") {
            return Err("M3_NODE_BAD_ROLE".into());
        }
        if !node_cids.insert(cid.to_string()) {
            return Err("M3_NODE_DUP_CID".into());
        }
    }

    for e in edges {
        let eo = e
            .as_object()
            .ok_or_else(|| "M3_EDGE_NOT_OBJECT".to_string())?;
        expect_only_keys(eo, &["from", "kind", "to"])?;
        let from = expect_str(eo, "from")?;
        let to = expect_str(eo, "to")?;
        let kind = expect_str(eo, "kind")?;
        if kind != "used_by" {
            return Err("M3_EDGE_BAD_KIND".into());
        }
        if !node_cids.contains(from) {
            return Err("M3_EDGE_FROM_UNKNOWN".into());
        }
        if !node_cids.contains(to) {
            return Err("M3_EDGE_TO_UNKNOWN".into());
        }
    }

    for cid in node_cids.iter() {
        let hex = cid_hex(cid)?;
        let p = format!("{}/artifacts/{}.bin", outdir, hex);
        if fs::metadata(&p).is_err() {
            return Err("M3_MISSING_ARTIFACT_BYTES".into());
        }
    }

    Ok(())
}
