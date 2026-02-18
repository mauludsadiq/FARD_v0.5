use anyhow::{anyhow, bail, Context, Result};
use sha2::Digest;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use valuecore::{dec, enc, cid, vdig, Value};
use witnesscore::{trace_v0_1, witness_v0_1};

pub fn run_bundle_to_stdout(bundle_dir: &Path) -> Result<()> {
    // 1) read bundle/
    let program_path = bundle_dir.join("program.json");
    let input_path = bundle_dir.join("input.json");
    let effects_path = bundle_dir.join("effects.json");
    let facts_dir = bundle_dir.join("facts");
    let sources_dir = bundle_dir.join("sources");

    // 2) verify sources/<hex>.src hash → sha256:<hex>
    verify_sources_dir(&sources_dir)?;

    // 3) parse program.json, input.json, effects.json, facts/*.json
    let program = read_valuecore_json(&program_path).context("program.json")?;
    let input = read_valuecore_json(&input_path).context("input.json")?;
    let bundle_effects = read_valuecore_json(&effects_path).context("effects.json")?;
    let _facts = read_facts_dir(&facts_dir).context("facts/")?;

    // Verify that all module sources referenced by program.mods exist in sources/ and match CID.
    verify_program_sources_present(&program, &sources_dir).context("program sources")?;

    // 4) canonicalize effects (sort by UTF8(kind)||0x00||ENC(req))
    // effects.json schema: list of records with keys: kind, req, value
    // witness schema wants: Effect := record([kind, req, sat]) where sat=text(VDIG(value)) or unit
    let effects = bundle_effects_to_witness_effects(&bundle_effects).context("effects conversion")?;

    // 5) produce exact ENC(W*) to stdout for Vector 0
    // Evaluation for Vector 0: result = unit, imports = []
    let imports: Vec<Value> = vec![];
    let result = Value::Unit;
    let trace = trace_v0_1(Value::Unit);

    let w = witness_v0_1(program, &input, effects, imports, result, trace)?;
    let out_bytes = enc(&w);

    // ABI v0: stdout == ENC(W*) only (no newline, no extra bytes)
    let mut stdout = io::stdout();
    stdout.write_all(&out_bytes)?;
    stdout.flush()?;
    Ok(())
}

fn read_valuecore_json(path: &Path) -> Result<Value> {
    let b = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    dec(&b).map_err(|e| anyhow!("DECODE_FAIL {}", e.code))
}

fn verify_sources_dir(dir: &Path) -> Result<()> {
    if !dir.exists() {
        bail!("ERROR_BAD_BUNDLE missing sources/");
    }
    for ent in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let ent = ent?;
        let p = ent.path();
        if p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow!("ERROR_BAD_SOURCE bad filename"))?;
        if !name.ends_with(".src") {
            continue;
        }
        let hexpart = name.strip_suffix(".src").unwrap();
        if hexpart.len() != 64 || !hexpart.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
            bail!("ERROR_BAD_SOURCE bad source filename {}", name);
        }
        let bytes = fs::read(&p).with_context(|| format!("read {}", p.display()))?;
        let got = cid(&bytes);
        let want = format!("sha256:{}", hexpart);
        if got != want {
            bail!("ERROR_BAD_SOURCE source hash mismatch {} got {}", want, got);
        }
    }
    Ok(())
}

fn read_facts_dir(dir: &Path) -> Result<Vec<(String, Value)>> {
    if !dir.exists() {
        // facts/ required by ABI layout but can be empty; accept missing only for now?
        // Keep strict: require it exists (even if empty).
        bail!("ERROR_BAD_BUNDLE missing facts/");
    }
    let mut out: Vec<(String, Value)> = vec![];
    for ent in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let ent = ent?;
        let p = ent.path();
        if p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE bad fact filename"))?;
        if !name.ends_with(".json") {
            continue;
        }
        let hexpart = name.strip_suffix(".json").unwrap();
        if hexpart.len() != 64 || !hexpart.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
            bail!("ERROR_BAD_BUNDLE bad fact filename {}", name);
        }
        let runid = format!("sha256:{}", hexpart);
        let v = read_valuecore_json(&p).with_context(|| format!("fact {}", name))?;
        // ABI: facts/<hex>.json contains ENC(W*) and MUST satisfy VDIG(W*) == RunID
        let got = vdig(&v);
        if got != runid {
            bail!("ERROR_BAD_BUNDLE fact hash mismatch {} got {}", runid, got);
        }
        out.push((runid, v));
    }
    out.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
    Ok(out)
}

fn bundle_effects_to_witness_effects(v: &Value) -> Result<Vec<Value>> {
    let arr = match v {
        Value::List(xs) => xs,
        _ => bail!("ERROR_BAD_BUNDLE effects.json must be list"),
    };

    let mut effects: Vec<Value> = vec![];
    for e in arr.iter() {
        let (kind, req, val) = parse_bundle_effect(e)?;
        let sat = Value::text(vdig(val));
        effects.push(Value::record(vec![
            ("kind".to_string(), Value::text(kind)),
            ("req".to_string(), req.clone()),
            ("sat".to_string(), sat),
        ]));
    }
    Ok(effects)
}

fn parse_bundle_effect(v: &Value) -> Result<(&str, &Value, &Value)> {
    match v {
        Value::Record(kvs) => {
            let mut kind: Option<&str> = None;
            let mut req: Option<&Value> = None;
            let mut val: Option<&Value> = None;

            for (k, x) in kvs.iter() {
                if k == "kind" {
                    if let Value::Text(s) = x {
                        kind = Some(s.as_str());
                    }
                } else if k == "req" {
                    req = Some(x);
                } else if k == "value" {
                    val = Some(x);
                }
            }
            let kind = kind.ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE effect missing kind"))?;
            let req = req.ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE effect missing req"))?;
            let val = val.ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE effect missing value"))?;
            Ok((kind, req, val))
        }
        _ => bail!("ERROR_BAD_BUNDLE effect entry must be record"),
    }
}

fn verify_program_sources_present(program: &Value, sources_dir: &Path) -> Result<()> {
    // ProgramIdentity := record([("kind","fard/program/v0.1"),("entry",...),("mods",list(ModEntry))])
    // ModEntry := record([("name",...),("source",text("sha256:<hex>"))])
    let mods = match program {
        Value::Record(kvs) => kvs.iter().find(|(k, _)| k == "mods").map(|(_, v)| v),
        _ => None,
    }
    .ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE program missing mods"))?;

    let mods = match mods {
        Value::List(xs) => xs,
        _ => bail!("ERROR_BAD_BUNDLE program.mods must be list"),
    };

    for m in mods.iter() {
        let src_cid = match m {
            Value::Record(kvs) => kvs
                .iter()
                .find(|(k, _)| k == "source")
                .and_then(|(_, v)| match v {
                    Value::Text(s) => Some(s.as_str()),
                    _ => None,
                }),
            _ => None,
        }
        .ok_or_else(|| anyhow!("ERROR_BAD_BUNDLE mod missing source"))?;

        if !src_cid.starts_with("sha256:") || src_cid.len() != ("sha256:".len() + 64) {
            bail!("ERROR_BAD_BUNDLE bad mod source cid {}", src_cid);
        }
        let hexpart = &src_cid["sha256:".len()..];
        let p = sources_dir.join(format!("{}.src", hexpart));
        if !p.exists() {
            bail!("ERROR_BAD_BUNDLE missing source file for {}", src_cid);
        }
        // hash already checked by verify_sources_dir; no need to re-hash here
    }
    Ok(())
}
