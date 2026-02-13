use std::collections::BTreeMap;
use std::fs;

use serde_json::Value as J;
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
  let mut h = Sha256::new();
  h.update(bytes);
  format!("{:x}", h.finalize())
}

fn canon_json(v: &J) -> Result<String, String> {
  fn canon_value(v: &J, out: &mut String) -> Result<(), String> {
    match v {
      J::Null => { out.push_str("null"); Ok(()) }
      J::Bool(b) => { out.push_str(if *b { "true" } else { "false" }); Ok(()) }
      J::Number(n) => {
        let s = n.to_string();
        if s.contains('+') { return Err("M5_CANON_NUM_PLUS".into()); }
        if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") { return Err("M5_CANON_NUM_LEADING_ZERO".into()); }
        if s.ends_with(".0") { return Err("M5_CANON_NUM_DOT0".into()); }
        out.push_str(&s);
        Ok(())
      }
      J::String(s) => {
        out.push_str(&serde_json::to_string(s).map_err(|_| "M5_CANON_STRING_FAIL".to_string())?);
        Ok(())
      }
      J::Array(a) => {
        out.push('[');
        for (i, x) in a.iter().enumerate() {
          if i > 0 { out.push(','); }
          canon_value(x, out)?;
        }
        out.push(']');
        Ok(())
      }
      J::Object(m) => {
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        out.push('{');
        for (i, k) in keys.iter().enumerate() {
          if i > 0 { out.push(','); }
          out.push_str(&serde_json::to_string(k).map_err(|_| "M5_CANON_KEY_ESC_FAIL".to_string())?);
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

fn read_json(path: &str) -> J {
  let b = fs::read(path).unwrap();
  serde_json::from_slice(&b).unwrap()
}

#[test]
fn m5_preimage_canonicalization_matches_verifier() {
  let outdir = "out/m5_ok_bundle";

  // digests.json is produced by fardrun; this test asserts the preimage
  // hash is consistent with the canonical-json preimage definition.
  let dig = read_json(&format!("{}/digests.json", outdir));
  let dobj = dig.as_object().unwrap();

  let ok = dobj.get("ok").unwrap().as_bool().unwrap();
  let runtime_version = dobj.get("runtime_version").unwrap().as_str().unwrap().to_string();
  let trace_format_version = dobj.get("trace_format_version").unwrap().as_str().unwrap().to_string();
  let stdlib_root_digest = dobj.get("stdlib_root_digest").unwrap().as_str().unwrap().to_string();
  let preimage_sha256 = dobj.get("preimage_sha256").unwrap().as_str().unwrap().to_string();

  let files_obj = dobj.get("files").unwrap().as_object().unwrap();

  // Freeze key order for files (BTreeMap) to match canonicalization assumptions.
  let mut files: BTreeMap<String, String> = BTreeMap::new();
  for (k, v) in files_obj.iter() {
    files.insert(k.clone(), v.as_str().unwrap().to_string());
  }

  let preimage = serde_json::json!({
    "files": files,
    "ok": ok,
    "runtime_version": runtime_version,
    "stdlib_root_digest": stdlib_root_digest,
    "trace_format_version": trace_format_version
  });

  let canon = canon_json(&preimage).unwrap();
  let expected = format!("sha256:{}", sha256_hex(canon.as_bytes()));

  assert_eq!(expected, preimage_sha256, "EXPECTED_M5_PREIMAGE_SHA256_MATCH");
}
