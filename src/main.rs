use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use dotsync::{add_dotfile, init_ignore_file, readd_dotfiles, sync_dotfiles, DotSyncError, Mode, SyncOptions};

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
    Sync(SyncOptions),
    Config(PathBuf),
    Add { source: PathBuf, dry_run: bool, verbose: bool },
    Readd { dirs: bool, dry_run: bool, verbose: bool },
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
        CliCommand::Sync(options) => {
            println!(
                "Syncing from {} to {}\n",
                options.origin_dir.display(),
                options.destination_dir.display()
            );
            sync_dotfiles(&options)?;
            println!("\nDone. Mode: '{}'.", options.mode.as_str());
        }
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
        CliCommand::Readd { dirs, dry_run, verbose } => {
            let repo = require_destination()?;
            let home = require_home()?;
            println!("Re-adding dotfiles from {} to {}\n", home.display(), repo.display());
            readd_dotfiles(&repo, &home, dirs, dry_run, verbose)?;
            if !dry_run {
                println!("\nDone.");
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
        Some("config") => parse_config_command(&args),
        Some("add") => parse_add_command(&args),
        Some("readd") => parse_readd_command(&args),
        _ => parse_sync_command(args).map(CliCommand::Sync),
    }
}

fn parse_config_command(args: &[String]) -> Result<CliCommand, AppError> {
    let mut positional: Vec<&str> = Vec::new();
    let mut seen_command = false;

    for arg in args {
        if arg == "config" && !seen_command {
            seen_command = true;
            continue;
        }
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
    let mut seen_command = false;

    for arg in args {
        if arg == "add" && !seen_command {
            seen_command = true;
            continue;
        }
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
    let mut seen_command = false;

    for arg in args {
        if arg == "readd" && !seen_command {
            seen_command = true;
            continue;
        }
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

fn parse_sync_command(args: Vec<String>) -> Result<SyncOptions, AppError> {
    let mut dry_run = false;
    let mut verbose = false;
    let mut reverse_flag = false;
    let mut reverse_only_files = false;
    let mut command = None;
    let mut origin_dir = None;
    let mut destination_dir = None;
    let mut positional = Vec::new();

    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-n" | "--dry-run" => dry_run = true,
            "-v" | "--verbose" => verbose = true,
            "--reverse" => reverse_flag = true,
            "--reverse-only-files" => {
                reverse_flag = true;
                reverse_only_files = true;
            }
            "--origin" => {
                origin_dir = Some(next_option_value(&mut args, arg.as_str())?);
            }
            "--destination" | "--dest" => {
                destination_dir = Some(next_option_value(&mut args, arg.as_str())?);
            }
            value if value.starts_with("--origin=") => {
                origin_dir = Some(value["--origin=".len()..].to_string());
            }
            value if value.starts_with("--destination=") => {
                destination_dir = Some(value["--destination=".len()..].to_string());
            }
            value if value.starts_with("--dest=") => {
                destination_dir = Some(value["--dest=".len()..].to_string());
            }
            value if value.starts_with('-') => {
                return Err(AppError::Usage(format!("Unknown option: {value}")))
            }
            _ if command.is_none() && is_mode_alias(&arg) => command = Some(arg),
            _ => positional.push(arg),
        }
    }

    let mode = resolve_mode(command, reverse_flag)?;
    let (origin_dir, destination_dir) =
        resolve_directories(origin_dir, destination_dir, positional)?;

    Ok(SyncOptions::new(mode, origin_dir, destination_dir)
        .with_dry_run(dry_run)
        .with_verbose(verbose)
        .with_reverse_only_files(reverse_only_files))
}

fn is_mode_alias(value: &str) -> bool {
    matches!(value, "reverse")
}

fn resolve_mode(command: Option<String>, reverse_flag: bool) -> Result<Mode, AppError> {
    let command_mode = match command {
        Some(command) => Some(Mode::from_str(&command)?),
        None => None,
    };

    match (command_mode, reverse_flag) {
        (Some(mode), _) => Ok(mode),
        (None, true) => Ok(Mode::Reverse),
        (None, false) => Ok(Mode::Apply),
    }
}

fn next_option_value(
    args: &mut impl Iterator<Item = String>,
    option_name: &str,
) -> Result<String, AppError> {
    args.next()
        .ok_or_else(|| AppError::Usage(format!("Option {option_name} requires a path value.")))
}

fn resolve_directories(
    origin_dir: Option<String>,
    destination_dir: Option<String>,
    positional: Vec<String>,
) -> Result<(PathBuf, PathBuf), AppError> {
    match (origin_dir, destination_dir, positional.as_slice()) {
        (Some(o), Some(d), []) => Ok((absolute_path(o)?, absolute_path(d)?)),
        (None, None, [o, d]) => Ok((absolute_path(o)?, absolute_path(d)?)),
        (Some(o), None, []) => {
            let dest = config::read_destination().ok_or_else(|| {
                AppError::Usage(
                    "Missing --destination. No destination configured; run 'dotsync config <path>' first.".to_string(),
                )
            })?;
            Ok((absolute_path(o)?, dest))
        }
        (None, None, [o]) => {
            let dest = config::read_destination().ok_or_else(|| {
                AppError::Usage(
                    "Missing destination. No destination configured; run 'dotsync config <path>' first.".to_string(),
                )
            })?;
            Ok((absolute_path(o)?, dest))
        }
        (Some(_), Some(_), _) => Err(AppError::Usage(
            "Do not mix --origin/--destination with positional paths. Choose one form."
                .to_string(),
        )),
        (Some(_), None, _) => Err(AppError::Usage(
            "Missing --destination <path>.".to_string(),
        )),
        (None, Some(_), _) => Err(AppError::Usage("Missing --origin <path>.".to_string())),
        (None, None, _) => Err(AppError::Usage(
            "Invalid usage: expected --origin <origin_dir> --destination <destination_dir>, or two positional paths."
                .to_string(),
        )),
    }
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

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  dotsync config <destination_path>");
    eprintln!("  dotsync add [-n] [-v] <path>");
    eprintln!("  dotsync readd [-n] [-v] [--dirs]");
    eprintln!("  dotsync [options] --origin <origin_dir> [--destination <destination_dir>]");
    eprintln!(
        "  dotsync [options] --reverse --origin <origin_dir> [--destination <destination_dir>]"
    );
    eprintln!("  dotsync [options] reverse <origin_dir> [<destination_dir>]");
    eprintln!("  dotsync [options] <origin_dir> [<destination_dir>]");
    eprintln!();
    eprintln!("Configuration:");
    eprintln!("  config <path>            Set default destination (~/.config/dotsync/config.toml).");
    eprintln!();
    eprintln!("Dotfile management:");
    eprintln!("  add <path>               Copy a dotfile/dir from $HOME into the repo.");
    eprintln!("                           Path is preserved relative to $HOME.");
    eprintln!("  readd                    Re-add every tracked file from $HOME into the repo.");
    eprintln!("      --dirs               Group by directory instead of copying file by file.");
    eprintln!("                           Under .config: caps at .config/<app>, never .config itself.");
    eprintln!("                           Elsewhere: copies the immediate parent directory.");
    eprintln!();
    eprintln!("Sync direction:");
    eprintln!("  no command               Apply from origin to destination.");
    eprintln!("  --reverse                Copy from destination to origin (full dir sync).");
    eprintln!("  --reverse-only-files     Copy from destination to origin (existing files only).");
    eprintln!("  reverse                  Subcommand equivalent to --reverse.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -n, --dry-run            Show what would happen without copying files.");
    eprintln!("  -v, --verbose            Print each copied file (warnings always shown).");
    eprintln!("      --origin <path>      Origin directory path.");
    eprintln!("      --destination <path> Destination directory path (overrides config).");
    eprintln!("      --dest <path>        Short alias for --destination.");
    eprintln!("  -h, --help               Show this help message.");
    eprintln!("      --version            Show version information.");
    eprintln!();
    eprintln!("Note: --destination is optional when 'dotsync config' has been set.");
}
