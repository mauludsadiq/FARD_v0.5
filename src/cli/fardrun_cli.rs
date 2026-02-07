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
            use std::ffi::OsString;
    let mut argv: Vec<OsString> = std::env::args_os().collect();
    if argv.len() >= 2 {
        let has_legacy = argv.iter().any(|a| {
            let s = a.to_string_lossy();
            s == "--program" || s == "--lock" || s == "--lockfile" || s == "--registry" || s == "--out" || s == "--trace" || s == "--result" || s == "--stdin"
        });
        let first_is_flag = argv.get(1).map(|a| a.to_string_lossy().starts_with("-")).unwrap_or(false);
        if has_legacy && first_is_flag {
            argv.insert(1, OsString::from("run"));
        }
    }
      let cli = Cli::parse_from(argv.clone());

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
                let cli2 = Cli::parse_from(argv);
                match cli2.cmd {
                    Some(Command::Run(r)) => r,
                    _ => unreachable!(),
                }
            }
        };

        (run, false)
    }
}
