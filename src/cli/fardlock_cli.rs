use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "fardlock", disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
    #[arg(short = 'V', long = "version")]
    pub version: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Lock {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

impl Cli {
    pub fn parse_compat() -> (Command, bool) {
        let cli = Cli::parse();
        (cli.cmd, cli.version)
    }
}
