use std::collections::{BTreeMap, BTreeSet};
use std::fs;

fn is_sha256(s: &str) -> bool {
  if !s.starts_with("sha256:") { return false; }
  let h = &s[7..];
  if h.len() != 64 { return false; }
  h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
  use sha2::Digest;
  let mut h = sha2::Sha256::new();
  h.update(bytes);
  let out = h.finalize();
  hex::encode(out)
}

fn canon_json(v: &serde_json::Value) -> Result<String, String> {
  fn canon_value(v: &serde_json::Value, out: &mut String) -> Result<(), String> {
    match v {
      serde_json::Value::Null => { out.push_str("null"); Ok(()) }
      serde_json::Value::Bool(b) => { out.push_str(if *b { "true" } else { "false" }); Ok(()) }
      serde_json::Value::Number(n) => {
        let s = n.to_string();
        if s.contains('+') { return Err("M5_CANON_NUM_PLUS".into()); }
        if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") { return Err("M5_CANON_NUM_LEADING_ZERO".into()); }
        if s.ends_with(".0") { return Err("M5_CANON_NUM_DOT0".into()); }
        out.push_str(&s);
        Ok(())
      }
      serde_json::Value::String(s) => {
        out.push_str(&serde_json::to_string(s).map_err(|_| "M5_CANON_STRING_FAIL")?);
        Ok(())
      }
      serde_json::Value::Array(a) => {
        out.push('[');
        for (i, x) in a.iter().enumerate() {
          if i > 0 { out.push(','); }
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
          if i > 0 { out.push(','); }
          out.push_str(&serde_json::to_string(k).map_err(|_| "M5_CANON_KEY_ESC_FAIL")?);
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

fn expect_only_keys(obj: &serde_json::Map<String, serde_json::Value>, allowed: &[&str]) -> Result<(), String> {
  let allow: BTreeSet<&str> = allowed.iter().copied().collect();
  for k in obj.keys() {
    if !allow.contains(k.as_str()) { return Err(format!("M5_EXTRA_KEY {}", k)); }
  }
  Ok(())
}

fn expect_str<'a>(obj: &'a serde_json::Map<String, serde_json::Value>, k: &str) -> Result<&'a str, String> {
  obj.get(k).and_then(|v| v.as_str()).ok_or_else(|| format!("M5_EXPECT_STRING {}", k))
}

fn expect_bool(obj: &serde_json::Map<String, serde_json::Value>, k: &str) -> Result<bool, String> {
  obj.get(k).and_then(|v| v.as_bool()).ok_or_else(|| format!("M5_EXPECT_BOOL {}", k))
}

fn expect_obj<'a>(obj: &'a serde_json::Map<String, serde_json::Value>, k: &str) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
  obj.get(k).and_then(|v| v.as_object()).ok_or_else(|| format!("M5_EXPECT_OBJECT {}", k))
}

pub fn verify_bundle_outdir(outdir: &str) -> Result<(), String> {
  let dig_p = format!("{}/digests.json", outdir);
  let dig_bytes = fs::read(&dig_p).map_err(|_| "M5_MISSING_digests.json".to_string())?;
  let dig_v: serde_json::Value = serde_json::from_slice(&dig_bytes).map_err(|_| "M5_DIGESTS_PARSE_FAIL".to_string())?;
  let dobj = dig_v.as_object().ok_or_else(|| "M5_DIGESTS_NOT_OBJECT".to_string())?;

  // frozen digests.json surface (verifier-authoritative)
  expect_only_keys(dobj, &[
    "files","ok","preimage_sha256","runtime_version","trace_format_version","stdlib_root_digest"
  ])?;

  let ok = expect_bool(dobj, "ok")?;
  let runtime_version = expect_str(dobj, "runtime_version")?;
  let trace_format_version = expect_str(dobj, "trace_format_version")?;
  let stdlib_root_digest = expect_str(dobj, "stdlib_root_digest")?;
  let preimage_sha256 = expect_str(dobj, "preimage_sha256")?;
  if !is_sha256(preimage_sha256) { return Err("M5_BAD_preimage_sha256".into()); }

  let files_obj = expect_obj(dobj, "files")?;

  // filesystem presence rules (bundle shape)
  let trace_p = format!("{}/trace.ndjson", outdir);
  let modg_p = format!("{}/module_graph.json", outdir);
  let res_p = format!("{}/result.json", outdir);
  let err_p = format!("{}/error.json", outdir);
  let ag_p = format!("{}/artifact_graph.json", outdir);

  let trace_exists = fs::metadata(&trace_p).is_ok();
  let modg_exists = fs::metadata(&modg_p).is_ok();
  let res_exists = fs::metadata(&res_p).is_ok();
  let err_exists = fs::metadata(&err_p).is_ok();
  let ag_exists = fs::metadata(&ag_p).is_ok();

  if !trace_exists { return Err("M5_MISSING_FILE trace.ndjson".into()); }
  if !modg_exists { return Err("M5_MISSING_FILE module_graph.json".into()); }

  if ok {
    if !res_exists { return Err("M5_OK_MISSING_result.json".into()); }
    if err_exists { return Err("M5_OK_FORBIDS_error.json".into()); }
  } else {
    if !err_exists { return Err("M5_ERR_MISSING_error.json".into()); }
    if res_exists { return Err("M5_ERR_FORBIDS_result.json".into()); }
  }

  // digest coverage = exactly the canonical outputs for this run
  let mut expected: BTreeSet<String> = BTreeSet::new();
  expected.insert("trace.ndjson".to_string());
  expected.insert("module_graph.json".to_string());
  if ok { expected.insert("result.json".to_string()); } else { expected.insert("error.json".to_string()); }
  if ag_exists { expected.insert("artifact_graph.json".to_string()); }

  // require EXACT match: no missing, no extras
  let mut seen: BTreeSet<String> = BTreeSet::new();
  for k in files_obj.keys() { seen.insert(k.to_string()); }

  for k in expected.iter() {
    if !seen.contains(k) { return Err(format!("M5_DIGESTS_MISSING_FILE {}", k)); }
  }
  for k in seen.iter() {
    if !expected.contains(k) { return Err(format!("M5_DIGESTS_EXTRA_FILE {}", k)); }
  }

  // verify each file hash matches digests.json.files
  for name in expected.iter() {
    let cid = files_obj.get(name)
      .and_then(|v| v.as_str())
      .ok_or_else(|| format!("M5_EXPECT_CID {}", name))?;
    if !is_sha256(cid) { return Err(format!("M5_BAD_FILE_CID {}", name)); }

    let p = format!("{}/{}", outdir, name);
    let bytes = fs::read(&p).map_err(|_| format!("M5_MISSING_FILE {}", name))?;
    let hex = sha256_hex(&bytes);
    let want = format!("sha256:{}", hex);
    if want != cid { return Err(format!("M5_FILE_HASH_MISMATCH {}", name)); }
  }

  // canonical preimage (unambiguous)
  let mut pre_files: BTreeMap<String, String> = BTreeMap::new();
  for k in expected.iter() {
    let cid = files_obj.get(k).and_then(|v| v.as_str()).unwrap();
    pre_files.insert(k.to_string(), cid.to_string());
  }

  let preimage = serde_json::json!({
    "files": pre_files,
    "ok": ok,
    "runtime_version": runtime_version,
    "stdlib_root_digest": stdlib_root_digest,
    "trace_format_version": trace_format_version
  });

  let canon = canon_json(&preimage)?;
  let pre_hex = sha256_hex(canon.as_bytes());
  let pre_cid = format!("sha256:{}", pre_hex);
  if pre_cid != preimage_sha256 { return Err("M5_PREIMAGE_HASH_MISMATCH".into()); }

  Ok(())
}
