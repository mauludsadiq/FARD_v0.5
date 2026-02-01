use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about = "Verify that the tests/gate tree exists for FARD v0.5")]
struct Args {
    #[arg(long, default_value = "tests/gate")]
    gate_dir: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let spec = args.gate_dir.join("gates.json");
    if !spec.is_file() {
        anyhow::bail!("missing {:?}", spec);
    }
    let programs = args.gate_dir.join("programs");
    if !programs.is_dir() {
        anyhow::bail!("missing {:?}", programs);
    }
    println!("OK: found gate spec and programs under {:?}", args.gate_dir);
    Ok(())
}
