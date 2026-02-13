use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use serde_json::Value as J;
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn read_json(path: &str) -> J {
    let b = fs::read(path).expect("READ_JSON_BYTES");
    serde_json::from_slice(&b).expect("READ_JSON_PARSE")
}

fn canon_json(v: &J) -> anyhow::Result<String> {
    use anyhow::Context;

    fn canon_value(v: &J, out: &mut String) -> anyhow::Result<()> {
        match v {
            J::Null => {
                out.push_str("null");
                Ok(())
            }
            J::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(())
            }
            J::Number(n) => {
                let s = n.to_string();
                if s.contains('+') {
                    anyhow::bail!("M5_CANON_NUM_PLUS");
                }
                if s.starts_with('0') && s.len() > 1 && !s.starts_with("0.") {
                    anyhow::bail!("M5_CANON_NUM_LEADING_ZERO");
                }
                if s.ends_with(".0") {
                    anyhow::bail!("M5_CANON_NUM_DOT0");
                }
                out.push_str(&s);
                Ok(())
            }
            J::String(s) => {
                out.push_str(&serde_json::to_string(s).context("M5_CANON_STRING_FAIL")?);
                Ok(())
            }
            J::Array(a) => {
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
            J::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k).context("M5_CANON_KEY_ESC_FAIL")?);
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

fn run_fardrun_err(program: &str, outdir: &str) {
    let _ = fs::remove_dir_all(outdir);

    let st = Command::new("cargo")
        .args([
            "run",
            "-q",
            "--bin",
            "fardrun",
            "--",
            "run",
            "--program",
            program,
            "--out",
            outdir,
        ])
        .status()
        .expect("SPAWN_FARDRUN");

    assert!(!st.success(), "FARDRUN_EXPECTED_ERR");
    assert!(fs::metadata(outdir).is_ok(), "OUTDIR_MISSING_ON_ERR");
    assert!(
        fs::metadata(format!("{}/digests.json", outdir)).is_ok(),
        "DIGESTS_MISSING_ON_ERR"
    );
}

#[test]
fn m5_digests_self_verify_err_exact() {
    let outdir = "out/m5_digest_verify_err_exact";
    let program = "spec/tmp/m5_digest_verify_err_exact.fard";

    run_fardrun_err(program, outdir);

    let dig = read_json(&format!("{}/digests.json", outdir));
    let dobj = dig.as_object().expect("DIGESTS_NOT_OBJECT");

    let ok = dobj
        .get("ok")
        .expect("M5_MISSING_ok")
        .as_bool()
        .expect("M5_ok_NOT_BOOL");
    assert!(!ok, "EXPECTED_ERR_RUN");

    let runtime_version = dobj
        .get("runtime_version")
        .expect("M5_MISSING_runtime_version")
        .as_str()
        .expect("M5_runtime_version_NOT_STR")
        .to_string();

    let trace_format_version = dobj
        .get("trace_format_version")
        .expect("M5_MISSING_trace_format_version")
        .as_str()
        .expect("M5_trace_format_version_NOT_STR")
        .to_string();

    let stdlib_root_digest = dobj
        .get("stdlib_root_digest")
        .expect("M5_MISSING_stdlib_root_digest")
        .as_str()
        .expect("M5_stdlib_root_digest_NOT_STR")
        .to_string();

    let preimage_sha256 = dobj
        .get("preimage_sha256")
        .expect("M5_MISSING_preimage_sha256")
        .as_str()
        .expect("M5_preimage_sha256_NOT_STR")
        .to_string();

    let files_obj = dobj
        .get("files")
        .expect("M5_MISSING_files")
        .as_object()
        .expect("M5_files_NOT_OBJECT");

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in files_obj.iter() {
        let h = v.as_str().expect("M5_file_hash_NOT_STR").to_string();
        files.insert(k.clone(), h);
    }

    let preimage = serde_json::json!({
      "files": files,
      "ok": ok,
      "runtime_version": runtime_version,
      "stdlib_root_digest": stdlib_root_digest,
      "trace_format_version": trace_format_version
    });

    let canon = canon_json(&preimage).expect("M5_CANON_JSON_FAIL");
    let expected = format!("sha256:{}", sha256_hex(canon.as_bytes()));
    assert_eq!(
        expected, preimage_sha256,
        "EXPECTED_M5_PREIMAGE_SHA256_MATCH"
    );
}
