use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn write_text(path: &Path, s: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("ERROR_IO mkdir {}", dir.display()))?;
    }
    fs::write(path, s.as_bytes()).with_context(|| format!("ERROR_IO write {}", path.display()))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    if args.is_empty() {
        bail!("usage: fardc --src <main.fard> --out <bundle_dir>");
    }

    let mut src: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--src" => {
                i += 1;
                if i >= args.len() { bail!("ERROR_BADARG missing value for --src"); }
                src = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                if i >= args.len() { bail!("ERROR_BADARG missing value for --out"); }
                out = Some(PathBuf::from(&args[i]));
            }
            _ => bail!("ERROR_BADARG unknown arg {}", args[i]),
        }
        i += 1;
    }

    let src = src.ok_or_else(|| anyhow::anyhow!("ERROR_BADARG missing --src"))?;
    let out = out.ok_or_else(|| anyhow::anyhow!("ERROR_BADARG missing --out"))?;

    let src_bytes = fs::read(&src).with_context(|| format!("ERROR_IO read {}", src.display()))?;
    let hex = sha256_hex(&src_bytes);
    let source_cid = format!("sha256:{}", hex);
// Layout: bundle/sources/<hex>.src (exact bytes)
    let sources_dir = out.join("sources");
    fs::create_dir_all(&sources_dir).with_context(|| format!("ERROR_IO mkdir {}", sources_dir.display()))?;
    let src_out = sources_dir.join(format!("{}.src", hex));
    fs::write(&src_out, &src_bytes).with_context(|| format!("ERROR_IO write {}", src_out.display()))?;

    // Minimal bundle files
    // input.json: unit
    write_text(&out.join("input.json"), r#"{"t":"unit"}"#)?;

    // imports.json: empty list
    write_text(&out.join("imports.json"), r#"{"t":"list","v":[]}"#)?;

    // effects.json: empty list
    write_text(&out.join("effects.json"), r#"{"t":"list","v":[]}"#)?;

    // facts/: empty dir
    fs::create_dir_all(out.join("facts")).ok();

    // program.json: one module main pointing at sha256:<hex>
    // NOTE: this is ValueCore JSON (normal JSON object with t/v fields).
    let program = format!(
r#"{{
  "t": "record",
  "v": [
    ["entry", {{ "t": "text", "v": "main" }}],
    ["kind",  {{ "t": "text", "v": "fard/program/v0.1" }}],
    ["mods",  {{ "t": "list", "v": [
      {{ "t": "record", "v": [
        ["name",   {{ "t": "text", "v": "main" }}],
        ["source", {{ "t": "text", "v": "{}" }}]
      ]}}
    ]}}]
  ]
}}"#, source_cid);

    write_text(&out.join("program.json"), &program)?;

    // Compiler output: print source_cid
    println!("{}", source_cid);
    Ok(())
}
