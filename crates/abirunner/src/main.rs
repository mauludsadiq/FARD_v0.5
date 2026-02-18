use anyhow::{bail, Result};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let bundle = match args.next() {
        Some(a) => PathBuf::from(a),
        None => bail!("usage: abirun <bundle_dir>"),
    };
    abirunner::run_bundle_to_stdout(&bundle)
}
