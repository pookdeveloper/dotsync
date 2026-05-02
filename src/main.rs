use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use dotsync::{
    add_dotfile, diff_dotfiles, init_ignore_file, readd_dotfiles, status_dotfiles, sync_dotfiles,
    DotSyncError, FileStatus, SyncOptions,
};

mod config;

#[derive(Debug)]
enum AppError {
    Usage(String),
    DotSync(DotSyncError),
    Help,
    Version,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::DotSync(error) => write!(f, "{error}"),
            Self::Help | Self::Version => Ok(()),
        }
    }
}

impl From<DotSyncError> for AppError {
    fn from(error: DotSyncError) -> Self {
        Self::DotSync(error)
    }
}

enum CliCommand {
    Config(PathBuf),
    Add { source: PathBuf, dry_run: bool, verbose: bool },
    Apply { path: Option<PathBuf>, dry_run: bool, verbose: bool },
    Readd { dirs: bool, dry_run: bool, verbose: bool },
    Status,
    Diff,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(AppError::Help) => {
            print_usage();
        }
        Err(AppError::Version) => {
            println!("dotsync {}", env!("CARGO_PKG_VERSION"));
        }
        Err(error) => {
            eprintln!("{error}");
            print_usage();
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), AppError> {
    match parse_arguments(env::args().skip(1))? {
        CliCommand::Config(destination) => {
            config::write_destination(&destination).map_err(AppError::Usage)?;
            println!("Destination set to: {}", destination.display());
            if destination.is_dir() {
                match init_ignore_file(&destination)? {
                    true => println!("Created: {}/.dotsyncignore", destination.display()),
                    false => {}
                }
            }
        }
        CliCommand::Add { source, dry_run, verbose } => {
            let repo = require_destination()?;
            let home = require_home()?;
            println!("Adding {} to {}\n", source.display(), repo.display());
            add_dotfile(&source, &home, &repo, dry_run, verbose)?;
            if !dry_run {
                println!("\nDone.");
            }
        }
        CliCommand::Apply { path, dry_run, verbose } => {
            let repo = require_destination()?;
            let home = require_home()?;

            let (origin, destination) = match &path {
                None => (repo.clone(), home.clone()),
                Some(p) => {
                    let origin = repo.join(p);
                    if !origin.exists() {
                        return Err(AppError::Usage(format!(
                            "'{}' not found in repo '{}'.",
                            p.display(),
                            repo.display()
                        )));
                    }
                    (origin, home.join(p))
                }
            };

            println!("Applying {} to {}\n", origin.display(), destination.display());

            let options = SyncOptions::new(&origin, &destination)
                .with_dry_run(dry_run)
                .with_verbose(verbose)
                .with_ignore_root(&repo);

            sync_dotfiles(&options)?;
            if !dry_run {
                println!("\nDone.");
            }
        }
        CliCommand::Readd { dirs, dry_run, verbose } => {
            let repo = require_destination()?;
            let home = require_home()?;
            println!("Re-adding dotfiles from {} to {}\n", home.display(), repo.display());
            readd_dotfiles(&repo, &home, dirs, dry_run, verbose)?;
            if !dry_run {
                println!("\nDone.");
            }
        }
        CliCommand::Status => {
            let repo = require_destination()?;
            let home = require_home()?;
            let statuses = status_dotfiles(&repo, &home)?;
            if statuses.is_empty() {
                println!("All tracked files are in sync.");
            } else {
                for s in &statuses {
                    match s {
                        FileStatus::Modified(p) => println!("M  {}", p.display()),
                        FileStatus::Missing(p) => println!("?  {}", p.display()),
                    }
                }
                let modified = statuses.iter().filter(|s| matches!(s, FileStatus::Modified(_))).count();
                let missing = statuses.iter().filter(|s| matches!(s, FileStatus::Missing(_))).count();
                println!();
                let mut parts: Vec<String> = Vec::new();
                if modified > 0 {
                    parts.push(format!("{modified} modified"));
                }
                if missing > 0 {
                    parts.push(format!("{missing} not applied"));
                }
                println!("{}", parts.join(", "));
            }
        }
        CliCommand::Diff => {
            let repo = require_destination()?;
            let home = require_home()?;
            if !diff_dotfiles(&repo, &home)? {
                println!("All tracked files are in sync.");
            }
        }
    }

    Ok(())
}

fn require_destination() -> Result<PathBuf, AppError> {
    config::read_destination().ok_or_else(|| {
        AppError::Usage(
            "No destination configured. Run 'dotsync config <path>' first.".to_string(),
        )
    })
}

fn require_home() -> Result<PathBuf, AppError> {
    config::home_dir()
        .ok_or_else(|| AppError::Usage("HOME environment variable is not set.".to_string()))
}

fn parse_arguments(args: impl IntoIterator<Item = String>) -> Result<CliCommand, AppError> {
    let args: Vec<String> = args.into_iter().collect();

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" => return Err(AppError::Help),
            "--version" => return Err(AppError::Version),
            _ => {}
        }
    }

    let first_positional = args.iter().find(|a| !a.starts_with('-')).map(String::as_str);

    match first_positional {
        Some("config") => parse_config_command(&skip_command(&args, "config")),
        Some("add") => parse_add_command(&skip_command(&args, "add")),
        Some("apply") => parse_apply_command(&skip_command(&args, "apply")),
        Some("readd") => parse_readd_command(&skip_command(&args, "readd")),
        Some("status") => parse_no_arg_command(&skip_command(&args, "status"), "status", CliCommand::Status),
        Some("diff") => parse_no_arg_command(&skip_command(&args, "diff"), "diff", CliCommand::Diff),
        _ => Err(AppError::Usage(
            "Unknown command. See --help for usage.".to_string(),
        )),
    }
}

fn skip_command(args: &[String], cmd: &str) -> Vec<String> {
    let mut skipped = false;
    args.iter()
        .filter(|a| {
            if !skipped && a.as_str() == cmd {
                skipped = true;
                false
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

fn parse_apply_command(args: &[String]) -> Result<CliCommand, AppError> {
    let mut dry_run = false;
    let mut verbose = false;
    let mut path: Option<PathBuf> = None;

    for arg in args {
        match arg.as_str() {
            "-n" | "--dry-run" => dry_run = true,
            "-v" | "--verbose" => verbose = true,
            value if value.starts_with('-') => {
                return Err(AppError::Usage(format!("Unknown option: {value}")))
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            other => {
                return Err(AppError::Usage(format!(
                    "'apply' takes at most one path argument, got extra: {other}"
                )))
            }
        }
    }

    Ok(CliCommand::Apply { path, dry_run, verbose })
}

fn parse_config_command(args: &[String]) -> Result<CliCommand, AppError> {
    let mut positional: Vec<&str> = Vec::new();

    for arg in args {
        if !arg.starts_with('-') {
            positional.push(arg.as_str());
        }
    }

    match positional.as_slice() {
        [path] => Ok(CliCommand::Config(absolute_path(path)?)),
        [] => Err(AppError::Usage(
            "Usage: dotsync config <destination_path>".to_string(),
        )),
        _ => Err(AppError::Usage(
            "'config' takes exactly one path argument.".to_string(),
        )),
    }
}

fn parse_add_command(args: &[String]) -> Result<CliCommand, AppError> {
    let mut dry_run = false;
    let mut verbose = false;
    let mut positional: Vec<&str> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-n" | "--dry-run" => dry_run = true,
            "-v" | "--verbose" => verbose = true,
            value if value.starts_with('-') => {
                return Err(AppError::Usage(format!("Unknown option: {value}")))
            }
            _ => positional.push(arg.as_str()),
        }
    }

    match positional.as_slice() {
        [path] => Ok(CliCommand::Add {
            source: absolute_path(path)?,
            dry_run,
            verbose,
        }),
        [] => Err(AppError::Usage("Usage: dotsync add <path>".to_string())),
        _ => Err(AppError::Usage(
            "'add' takes exactly one path argument.".to_string(),
        )),
    }
}

fn parse_readd_command(args: &[String]) -> Result<CliCommand, AppError> {
    let mut dirs = false;
    let mut dry_run = false;
    let mut verbose = false;

    for arg in args {
        match arg.as_str() {
            "--dirs" => dirs = true,
            "-n" | "--dry-run" => dry_run = true,
            "-v" | "--verbose" => verbose = true,
            value if value.starts_with('-') => {
                return Err(AppError::Usage(format!("Unknown option: {value}")))
            }
            other => {
                return Err(AppError::Usage(format!(
                    "'readd' takes no positional arguments, got: {other}"
                )))
            }
        }
    }

    Ok(CliCommand::Readd { dirs, dry_run, verbose })
}

fn absolute_path(value: impl AsRef<Path>) -> Result<PathBuf, AppError> {
    let path = value.as_ref();

    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()
            .map_err(|source| {
                AppError::DotSync(DotSyncError::Io {
                    context: "Could not get current directory".to_string(),
                    source,
                })
            })?
            .join(path))
    }
}

fn parse_no_arg_command(args: &[String], name: &str, cmd: CliCommand) -> Result<CliCommand, AppError> {
    for arg in args {
        if arg.starts_with('-') {
            return Err(AppError::Usage(format!("Unknown option: {arg}")));
        }
        return Err(AppError::Usage(format!("'{name}' takes no arguments, got: {arg}")));
    }
    Ok(cmd)
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  dotsync config <destination_path>");
    eprintln!("  dotsync add [-n] [-v] <path>");
    eprintln!("  dotsync apply [-n] [-v] [<path>]");
    eprintln!("  dotsync status");
    eprintln!("  dotsync diff");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  config <path>            Set the repository destination path.");
    eprintln!("                           Writes to ~/.config/dotsync/config.toml");
    eprintln!("  add <path>               Copy a dotfile/dir from $HOME into the repo.");
    eprintln!("                           Path is preserved relative to $HOME.");
    eprintln!("  apply [<path>]           Apply repo to $HOME (uses configured repo).");
    eprintln!("                           With <path>: applies only that subdirectory.");
    eprintln!("  readd [-n] [-v] [--dirs] Re-add tracked files from $HOME into the repo.");
    eprintln!("                           --dirs: copy entire parent dirs to catch new files.");
    eprintln!("  status                   Show which tracked files differ from $HOME.");
    eprintln!("                           M = modified, ? = not yet applied to $HOME.");
    eprintln!("  diff                     Show unified diff between $HOME and the repo.");
    eprintln!("                           Use before apply to review what will change.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -n, --dry-run            Show what would happen without copying files.");
    eprintln!("  -v, --verbose            Print each copied file (warnings always shown).");
    eprintln!("  -h, --help               Show this help message.");
    eprintln!("      --version            Show version information.");
}
