use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fardlang::check::check_module as check_module_lang;
use fardlang::eval::{eval_block, Env};
use fardlang::parse::parse_module as parse_module_lang;

mod ast;
mod canon;
mod frontend_v1;
mod lex;
mod modgraph;
mod parse;

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
                if i >= args.len() {
                    bail!("ERROR_BADARG missing value for --src");
                }
                src = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                if i >= args.len() {
                    bail!("ERROR_BADARG missing value for --out");
                }
                out = Some(PathBuf::from(&args[i]));
            }
            _ => bail!("ERROR_BADARG unknown arg {}", args[i]),
        }
        i += 1;
    }

    let src = src.ok_or_else(|| anyhow::anyhow!("ERROR_BADARG missing --src"))?;
    let out = out.ok_or_else(|| anyhow::anyhow!("ERROR_BADARG missing --out"))?;

    // 1) Read raw source (bytes)
    let raw = fs::read(&src).with_context(|| format!("ERROR_IO read {}", src.display()))?;

    // 2) Canonical module bytes:
    //    - Prefer v1 frontend (syntax-aware, module header required)
    //    - Fallback to legacy parse/canon for older "fn ..." sources (Gate5 frozen)
    let canon_bytes_vec: Vec<u8> = match frontend_v1::compile_v1_module_to_canon(&raw) {
        Ok(v) => {
            frontend_v1::ensure_min_entry_is_present(&v)?;
            v
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            if msg.contains("expected KwModule got KwFn") || msg.contains("expected KwModule") {
                // legacy sources may start with fn or other non-module leading tokens
                // v1 requires a module header, so we fallback to legacy canonizer

                let m = parse::parse_module(&raw).context("ERROR_PARSE fardc parse_module")?;
                let canon = canon::print_module(&m);
                canon.into_bytes()
            } else {
                return Err(e).context("ERROR_PARSE fardc v1 frontend");
            }
        }
    };
    let canon_bytes = canon_bytes_vec.as_slice();

    // 3) CID is sha256(canonical bytes)
    let hex = sha256_hex(canon_bytes);
    let source_cid = format!("sha256:{}", hex);

    // Layout: bundle/sources/<hex>.src (canonical bytes)
    let sources_dir = out.join("sources");
    fs::create_dir_all(&sources_dir)
        .with_context(|| format!("ERROR_IO mkdir {}", sources_dir.display()))?;
    let src_out = sources_dir.join(format!("{}.src", hex));
    fs::write(&src_out, canon_bytes)
        .with_context(|| format!("ERROR_IO write {}", src_out.display()))?;

    // Minimal bundle files remain identical semantics
    write_text(&out.join("input.json"), r#"{"t":"unit"}"#)?;
    write_text(&out.join("imports.json"), r#"{"t":"list","v":[]}"#)?;
    write_text(&out.join("effects.json"), r#"{"t":"list","v":[]}"#)?;
    fs::create_dir_all(out.join("facts")).ok();

    // result.json
    //
    // Policy:
    // - v1 sources (module header): parse/check/eval via fardlang, write computed result.json
    // - legacy sources (Gate5 frozen; start with `fn`): keep historical behavior (unit result.json)
    let result_bytes: Vec<u8> = match parse_module_lang(&raw) {
        Ok(m_lang) => {
            check_module_lang(&m_lang).context("ERROR_CHECK fardc fardlang check_module")?;

            let mut fns: BTreeMap<String, fardlang::ast::FnDecl> = BTreeMap::new();
            fardlang::eval::apply_imports(&mut env, &m_lang.imports);
            for d in &m_lang.fns {
                fns.insert(d.name.clone(), d.clone());
            }

            let main_decl = fns
                .get("main")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("ERROR_EVAL missing main"))?;

            let mut env = Env::with_fns(fns);
            let v = eval_block(&main_decl.body, &mut env).context("ERROR_EVAL fardc eval main")?;
            valuecore::v0::encode_json(&v)
        }
        Err(e) => {
            let msg = format!("{:#}", e);
            if msg.contains("expected KwModule") {
                valuecore::v0::encode_json(&valuecore::v0::V::Unit)
            } else {
                return Err(e).context("ERROR_PARSE fardc fardlang parse_module");
            }
        }
    };

    fs::write(out.join("result.json"), &result_bytes)
        .with_context(|| format!("ERROR_IO write {}", out.join("result.json").display()))?;

    // program.json: points to source CID
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
}}"#,
        source_cid
    );

    write_text(&out.join("program.json"), &program)?;

    // module_graph.json: canonical graph (not used by runner yet, but part of frontend contract)
    let mg = modgraph::ModuleGraph::single("main", "main", &source_cid);
    let mg_json = serde_json::to_string_pretty(&mg).unwrap();
    write_text(&out.join("module_graph.json"), &mg_json)?;

    // Compiler contract (v0): prints CID(canonical module bytes)
    println!("{}", source_cid);
    Ok(())
}
