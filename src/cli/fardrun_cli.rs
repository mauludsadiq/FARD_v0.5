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
    Repl,
    Test(TestArgs),
    Publish(PublishArgs),
    Install(InstallArgs),
    New(NewArgs),
    Search(SearchArgs),
    Notebook(NotebookArgs),
}

#[derive(Args, Debug)]
pub struct NewArgs {
    /// Project name
    pub name: String,

    /// Template: minimal, server, ci (default: minimal)
    #[arg(long, default_value = "minimal")]
    pub template: String,
}

#[derive(Args, Debug)]
pub struct TestArgs {
    #[arg(long)]
    pub program: PathBuf,

    #[arg(long, default_value_t = false)]
    pub json: bool,
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

    #[arg(long, default_value_t = false)]
    pub enforce_lockfile: bool,

    #[arg(long, default_value_t = false)]
    pub no_trace: bool,

    #[arg(long, default_value_t = false)]
    pub strict_types: bool,

    /// Program arguments passed after --
    #[arg(last = true)]
    pub program_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct PublishArgs {
    #[arg(long)]
    pub package: PathBuf,

    #[arg(long)]
    pub token: String,

    #[arg(long, default_value = "mauludsadiq/FARD")]
    pub repo: String,
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    #[arg(long)]
    pub dep: Option<String>,
    #[arg(long, default_value = "fard.toml")]
    pub manifest: PathBuf,
    #[arg(long)]
    pub registry: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct NotebookArgs {
    /// Input notebook file (.fardnb.md)
    #[arg(long, default_value = "notebook.fardnb.md")]
    pub input: std::path::PathBuf,
    /// Output file (default: overwrites input with results)
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
    /// Directory for cell outputs
    #[arg(long, default_value = "./notebook_out")]
    pub out_dir: String,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query (package name or keyword)
    pub query: Option<String>,
}

impl Cli {
    pub fn parse_compat() -> (RunArgs, bool, bool, Option<TestArgs>, Option<PublishArgs>, Option<InstallArgs>, Option<NewArgs>) {
        use std::ffi::OsString;
        let mut argv: Vec<OsString> = std::env::args_os().collect();
        if argv.len() >= 2 {
            let has_legacy = argv.iter().any(|a| {
                let s = a.to_string_lossy();
                s == "--program"
                    || s == "--lock"
                    || s == "--lockfile"
                    || s == "--registry"
                    || s == "--out"
                    || s == "--trace"
                    || s == "--result"
                    || s == "--stdin"
                    || s == "--enforce-lockfile"
            });
            let first_is_flag = argv
                .get(1)
                .map(|a| a.to_string_lossy().starts_with("-"))
                .unwrap_or(false);
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
                enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
            };
            return (dummy, true, false, None, None, None, None);
        }

        let want_repl = matches!(cli.cmd, Some(Command::Repl));
        let run = match cli.cmd {
            Some(Command::Run(r)) => r,
            Some(Command::Test(t)) => {
                let dummy = RunArgs {
                    program: t.program.clone(),
                    out: PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, Some(t), None, None, None);
            }
            Some(Command::Publish(p)) => {
                let dummy = RunArgs {
                    program: p.package.clone(),
                    out: PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, None, Some(p), None, None);
            }
            Some(Command::Install(i)) => {
                let dummy = RunArgs {
                    program: i.manifest.clone(),
                    out: PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, None, None, Some(i), None);
            }
            Some(Command::New(n)) => {
                let dummy = RunArgs {
                    program: PathBuf::from("."),
                    out: PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, None, None, None, Some(n));
            }
            Some(Command::Notebook(_)) => {
                // Handled directly in fardrun.rs
                let dummy = RunArgs {
                    program: std::path::PathBuf::from("."),
                    out: std::path::PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, None, None, None, None);
            }
            Some(Command::Search(s)) => {
                let query = s.query.unwrap_or_default();
                // Print search results and exit
                // Store search query in env for fardrun.rs to handle
                std::env::set_var("FARD_SEARCH_QUERY", &query);
                std::env::set_var("FARD_SEARCH_MODE", "1");
                let dummy = RunArgs {
                    program: PathBuf::from("."),
                    out: PathBuf::from("."),
                    lockfile: None,
                    registry: None,
                    enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                };
                return (dummy, false, false, None, None, None, None);
            }
            Some(Command::Repl) | None => {
                if want_repl {
                    let dummy = RunArgs {
                        program: PathBuf::from("."),
                        out: PathBuf::from("."),
                        lockfile: None,
                        registry: None,
                        enforce_lockfile: false,
                    no_trace: false,
                    strict_types: false,
                    program_args: vec![],
                    };
                    return (dummy, false, true, None, None, None, None);
                }
                eprintln!("usage: fardrun run --program <file.fard> --out <dir>");
                eprintln!("       fardrun test --program <file.fard>");
                eprintln!("       fardrun repl");
                eprintln!("       fardrun --version");
                std::process::exit(0);
            }
        };

        (run, false, false, None, None, None, None)
    }
}

impl Cli {
    pub fn parse_compat_notebook() -> Option<Command> {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(|s| s.as_str()) == Some("notebook") {
            let mut input = std::path::PathBuf::from("notebook.fardnb.md");
            let mut output: Option<std::path::PathBuf> = None;
            let mut out_dir = "./notebook_out".to_string();
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--input" => { i += 1; if i < args.len() { input = std::path::PathBuf::from(&args[i]); } }
                    "--output" => { i += 1; if i < args.len() { output = Some(std::path::PathBuf::from(&args[i])); } }
                    "--out-dir" => { i += 1; if i < args.len() { out_dir = args[i].clone(); } }
                    _ => {}
                }
                i += 1;
            }
            Some(Command::Notebook(NotebookArgs { input, output, out_dir }))
        } else {
            None
        }
    }
}
