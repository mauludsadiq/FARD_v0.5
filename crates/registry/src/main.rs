use anyhow::{bail, Result};
use std::env;
use inherit_cert_crdt::{EffectKey, RunID, InheritCertState, InheritCertDelta};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("usage: registry <put|get|crdt-propose|crdt-state|crdt-merge|crdt-get> ...");
    }
    let cmd = args[1].as_str();
    match cmd {
        "put" => {
            if args.len() != 4 { bail!("usage: registry put <runid> <path>"); }
            let runid = &args[2];
            let path = &args[3];
            let b = std::fs::read(path)?;
            registry::put_bytes(runid, &b)?;
            println!("stored {}", runid);
            Ok(())
        }
        "get" => {
            if args.len() != 4 { bail!("usage: registry get <runid> <out_path>"); }
            let runid = &args[2];
            let out_path = &args[3];
            let b = registry::get_bytes(runid)?;
            std::fs::write(out_path, b)?;
            println!("retrieved {} -> {}", runid, out_path);
            Ok(())
        }
        "crdt-propose" => {
            // registry crdt-propose <effect_kind> <req_bytes_hex> <run_id>
            if args.len() != 5 { bail!("usage: registry crdt-propose <kind> <req_hex> <run_id>"); }
            let kind = &args[2];
            let req_hex = hex::decode(&args[3])
                .map_err(|e| anyhow::anyhow!("bad req hex: {}", e))?;
            let run_id = RunID::new(args[4].clone());
            if !run_id.is_valid() { bail!("invalid RunID: {}", run_id); }
            let key = EffectKey::from_kind_req(kind, &req_hex);
            registry::crdt_propose(key.clone(), run_id.clone())?;
            println!("proposed effect_key={} run_id={}", key, run_id);
            Ok(())
        }
        "crdt-state" => {
            // registry crdt-state  — print current CRDT state as JSON
            let state = registry::crdt_load()?;
            println!("{}", serde_json::to_string_pretty(&state.to_json())?);
            Ok(())
        }
        "crdt-merge" => {
            // registry crdt-merge <state.json>  — merge a remote state file
            if args.len() != 3 { bail!("usage: registry crdt-merge <state.json>"); }
            let bytes = std::fs::read(&args[2])?;
            let v: serde_json::Value = serde_json::from_slice(&bytes)?;
            let remote = InheritCertState::from_json(&v)
                .map_err(|e| anyhow::anyhow!("parse: {}", e))?;
            let merged = registry::crdt_merge_state(&remote)?;
            println!("merged — {} effect(s) now canonical", merged.len());
            Ok(())
        }
        "crdt-get" => {
            // registry crdt-get <effect_kind> <req_bytes_hex>
            if args.len() != 4 { bail!("usage: registry crdt-get <kind> <req_hex>"); }
            let kind = &args[2];
            let req_hex = hex::decode(&args[3])
                .map_err(|e| anyhow::anyhow!("bad req hex: {}", e))?;
            let key = EffectKey::from_kind_req(kind, &req_hex);
            match registry::crdt_get(&key)? {
                Some(run_id) => println!("{}", run_id),
                None => println!("not found"),
            }
            Ok(())
        }
        "crdt-delta" => {
            // registry crdt-delta <their_state.json> — print delta they need
            if args.len() != 3 { bail!("usage: registry crdt-delta <their_state.json>"); }
            let bytes = std::fs::read(&args[2])?;
            let v: serde_json::Value = serde_json::from_slice(&bytes)?;
            let their_state = InheritCertState::from_json(&v)
                .map_err(|e| anyhow::anyhow!("parse: {}", e))?;
            let delta = registry::crdt_delta_for(&their_state)?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "kind": "fard/inherit_cert_delta/v0.1",
                "updates": delta.updates.iter()
                    .map(|(k, r)| (k.as_str().to_string(), r.as_str().to_string()))
                    .collect::<std::collections::BTreeMap<_,_>>()
            }))?);
            Ok(())
        }
        _ => bail!("unknown cmd: {}", cmd),
    }
}
