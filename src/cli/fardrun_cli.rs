use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "fardrun")]
#[command(disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Command>,

    #[arg(long, short = 'V', action = clap::ArgAction::SetTrue)]
    pub version: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Run(RunArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(long)]
    pub program: PathBuf,

    #[arg(long)]
    pub out: PathBuf,

    #[arg(long, alias = "lock")]
    pub lockfile: Option<PathBuf>,

    #[arg(long)]
    pub registry: Option<PathBuf>,
}

impl Cli {
    pub fn parse_compat() -> (RunArgs, bool) {
        let raw: Vec<String> = std::env::args().collect();
        let cli = Cli::parse_from(raw);

        if cli.version {
            let dummy = RunArgs {
                program: PathBuf::from("."),
                out: PathBuf::from("."),
                lockfile: None,
                registry: None,
            };
            return (dummy, true);
        }

        let run = match cli.cmd {
            Some(Command::Run(r)) => r,
            None => {
                let mut raw2: Vec<String> = std::env::args().collect();
                if raw2.len() >= 2 && raw2[1] != "run" && !raw2[1].starts_with("-") {
                    let p = raw2.remove(1);
                    raw2.insert(1, "run".to_string());
                    raw2.insert(2, "--program".to_string());
                    raw2.insert(3, p);
                }
                let cli2 = Cli::parse_from(raw2);
                match cli2.cmd {
                    Some(Command::Run(r)) => r,
                    _ => unreachable!(),
                }
            }
        };

        (run, false)
    }
}
