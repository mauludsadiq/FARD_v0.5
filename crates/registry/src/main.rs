use anyhow::{bail, Result};
use std::env;

fn main() -> Result<()> {
    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("usage: registry <put|get> ...");
    }
    let cmd = args[1].as_str();
    match cmd {
        "put" => {
            if args.len() != 4 {
                bail!("usage: registry put <runid> <path>");
            }
            let runid = &args[2];
            let path = &args[3];
            let b = std::fs::read(path)?;
            registry::put_bytes(runid, &b)?;
            Ok(())
        }
        "get" => {
            if args.len() != 4 {
                bail!("usage: registry get <runid> <out_path>");
            }
            let runid = &args[2];
            let out_path = &args[3];
            let b = registry::get_bytes(runid)?;
            std::fs::write(out_path, b)?;
            Ok(())
        }
        _ => bail!("unknown cmd {}", cmd),
    }
}
