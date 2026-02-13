use std::collections::BTreeSet;
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

fn canon_line(v: &serde_json::Value) -> Result<String, String> {
    fn canon_value(v: &serde_json::Value, out: &mut String) -> Result<(), String> {
        match v {
            serde_json::Value::Null => {
                out.push_str("null");
                Ok(())
            }
            serde_json::Value::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            serde_json::Value::Number(n) => {
                let s = n.to_string();
                if s.contains("+") {
                    return Err("CANON_NUM_PLUS".into());
                }
                if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") {
                    return Err("CANON_NUM_LEADING_ZERO".into());
                }
                if s.ends_with(".0") {
                    return Err("CANON_NUM_DOT0".into());
                }
                out.push_str(&s);
                Ok(())
            }
            serde_json::Value::String(s) => {
                out.push_str(&serde_json::to_string(s).map_err(|_| "CANON_STRING_FAIL")?);
                Ok(())
            }
            serde_json::Value::Array(a) => {
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
            serde_json::Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k).map_err(|_| "CANON_KEY_ESC_FAIL")?);
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
    obj: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<(), String> {
    let allow: BTreeSet<&str> = allowed.iter().copied().collect();
    for k in obj.keys() {
        if !allow.contains(k.as_str()) {
            return Err(format!("TRACE_EXTRA_KEY {}", k));
        }
    }
    Ok(())
}

fn expect_str<'a>(
    obj: &'a serde_json::Map<String, serde_json::Value>,
    k: &str,
) -> Result<&'a str, String> {
    obj.get(k)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("TRACE_EXPECT_STRING {}", k))
}

fn expect_t(obj: &serde_json::Map<String, serde_json::Value>) -> Result<&str, String> {
    expect_str(obj, "t")
}

pub fn verify_trace_outdir(outdir: &str) -> Result<(), String> {
    let digests_p = format!("{}/digests.json", outdir);
    let trace_p = format!("{}/trace.ndjson", outdir);
    let digests_bytes = fs::read(&digests_p).map_err(|_| "M2_MISSING_digests.json".to_string())?;
    let digests_v: serde_json::Value =
        serde_json::from_slice(&digests_bytes).map_err(|_| "M2_DIGESTS_PARSE_FAIL".to_string())?;
    let ok = digests_v
        .get("ok")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "M2_DIGESTS_MISSING_ok".to_string())?;

    let trace_bytes = fs::read(&trace_p).map_err(|_| "M2_MISSING_trace.ndjson".to_string())?;
    let trace_str =
        std::str::from_utf8(&trace_bytes).map_err(|_| "M2_TRACE_NOT_UTF8".to_string())?;

    let allowed_t: BTreeSet<&str> = [
        "module_resolve",
        "module_graph",
        "artifact_in",
        "artifact_out",
        "error",
    ]
    .into_iter()
    .collect();

    let mut saw_non_module_resolve = false;
    let mut module_graph_count = 0usize;
    let mut error_count = 0usize;
    let mut last_t: Option<String> = None;

    for (idx, raw_line) in trace_str.split('\n').enumerate() {
        if raw_line.is_empty() {
            if idx + 1 == trace_str.split('\n').count() {
                break;
            }
            return Err("M2_EMPTY_LINE".into());
        }
        if raw_line.ends_with(' ') || raw_line.ends_with('\t') || raw_line.contains("\r") {
            return Err("M2_TRAILING_SPACE_OR_CR".into());
        }

        let v: serde_json::Value =
            serde_json::from_str(raw_line).map_err(|_| "M2_TRACE_LINE_PARSE_FAIL".to_string())?;
        let obj = v
            .as_object()
            .ok_or_else(|| "M2_TRACE_LINE_NOT_OBJECT".to_string())?;

        let t = expect_t(obj)?;
        if !allowed_t.contains(t) {
            return Err(format!("M2_BAD_EVENT_TAG {}", t));
        }

        let canon = canon_line(&v)?;
        if canon != raw_line {
            return Err("M2_CANON_MISMATCH".into());
        }

        match t {
            "module_resolve" => {
                expect_only_keys(obj, &["cid", "kind", "name", "t"])?;
                let cid = expect_str(obj, "cid")?;
                if !is_sha256(cid) {
                    return Err("M2_BAD_CID".into());
                }
                let kind = expect_str(obj, "kind")?;
                let _name = expect_str(obj, "name")?;
                if !matches!(kind, "std" | "rel" | "abs" | "vendor") {
                    return Err("M2_BAD_KIND".into());
                }
                if saw_non_module_resolve {
                    return Err("M2_ORDER_MODULE_RESOLVE_PREFIX".into());
                }
            }
            "module_graph" => {
                expect_only_keys(obj, &["cid", "t"])?;
                let cid = expect_str(obj, "cid")?;
                if !is_sha256(cid) {
                    return Err("M2_BAD_CID".into());
                }
                module_graph_count += 1;
                saw_non_module_resolve = true;
            }
            "artifact_in" | "artifact_out" => {
                expect_only_keys(obj, &["cid", "name", "t"])?;
                let cid = expect_str(obj, "cid")?;
                if !is_sha256(cid) {
                    return Err("M2_BAD_CID".into());
                }
                let _name = expect_str(obj, "name")?;
                saw_non_module_resolve = true;
            }
            "error" => {
                expect_only_keys(obj, &["code", "message", "t"])?;
                let _code = expect_str(obj, "code")?;
                let _msg = expect_str(obj, "message")?;
                error_count += 1;
                saw_non_module_resolve = true;
            }
            _ => return Err("M2_UNREACHABLE".into()),
        }

        last_t = Some(t.to_string());
    }

    if module_graph_count != 1 {
        return Err("M2_MODULE_GRAPH_NOT_ONCE".into());
    }

    if error_count > 0 {
        if error_count != 1 {
            return Err("M2_ERROR_NOT_ONCE".into());
        }
        if last_t.as_deref() != Some("error") {
            return Err("M2_ERROR_NOT_LAST".into());
        }
    }

    let result_p = format!("{}/result.json", outdir);
    let error_p = format!("{}/error.json", outdir);

    let has_result = fs::metadata(&result_p).is_ok();
    let has_error = fs::metadata(&error_p).is_ok();

    if ok {
        if error_count != 0 {
            return Err("M2_OK_MUST_HAVE_NO_ERROR_EVENT".into());
        }
        if !has_result {
            return Err("M2_OK_MUST_HAVE_result.json".into());
        }
        if has_error {
            return Err("M2_OK_MUST_NOT_HAVE_error.json".into());
        }
    } else {
        if error_count != 1 {
            return Err("M2_FAIL_MUST_HAVE_ONE_ERROR_EVENT".into());
        }
        if has_result {
            return Err("M2_FAIL_MUST_NOT_HAVE_result.json".into());
        }
        if !has_error {
            return Err("M2_FAIL_MUST_HAVE_error.json".into());
        }
    }

    Ok(())
}
