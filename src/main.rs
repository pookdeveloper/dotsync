use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use dotsync::{sync_dotfiles, DotSyncError, Mode, SyncOptions};

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
    let options = parse_arguments(env::args().skip(1))?;

    println!(
        "Syncing from {} to {}\n",
        options.origin_dir.display(),
        options.destination_dir.display()
    );

    sync_dotfiles(&options)?;

    println!(
        "\nDone. Mode: '{}'.",
        options.mode.as_str()
    );

    Ok(())
}

fn parse_arguments(args: impl IntoIterator<Item = String>) -> Result<SyncOptions, AppError> {
    let mut dry_run = false;
    let mut reverse_flag = false;
    let mut reverse_only_files = false;
    let mut command = None;
    let mut origin_dir = None;
    let mut destination_dir = None;
    let mut positional = Vec::new();

    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Err(AppError::Help),
            "--version" => return Err(AppError::Version),
            "-n" | "--dry-run" => dry_run = true,
            "--reverse" | "--backup" => reverse_flag = true,
            "--reverse-only-files" => {
                reverse_flag = true;
                reverse_only_files = true;
            }
            "--origin" | "--repo" | "--dotfiles" | "--dotfiles-dir" => {
                origin_dir = Some(next_option_value(&mut args, arg.as_str())?);
            }
            "--destination" | "--dest" | "--home" | "--home-dir" => {
                destination_dir = Some(next_option_value(&mut args, arg.as_str())?);
            }
            value if value.starts_with("--origin=") => {
                origin_dir = Some(value["--origin=".len()..].to_string());
            }
            value if value.starts_with("--repo=") => {
                origin_dir = Some(value["--repo=".len()..].to_string());
            }
            value if value.starts_with("--dotfiles=") => {
                origin_dir = Some(value["--dotfiles=".len()..].to_string());
            }
            value if value.starts_with("--dotfiles-dir=") => {
                origin_dir = Some(value["--dotfiles-dir=".len()..].to_string());
            }
            value if value.starts_with("--destination=") => {
                destination_dir = Some(value["--destination=".len()..].to_string());
            }
            value if value.starts_with("--dest=") => {
                destination_dir = Some(value["--dest=".len()..].to_string());
            }
            value if value.starts_with("--home=") => {
                destination_dir = Some(value["--home=".len()..].to_string());
            }
            value if value.starts_with("--home-dir=") => {
                destination_dir = Some(value["--home-dir=".len()..].to_string());
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
        .with_reverse_only_files(reverse_only_files))
}

fn is_mode_alias(value: &str) -> bool {
    matches!(value, "apply" | "reverse" | "backup" | "capture")
}

fn resolve_mode(command: Option<String>, reverse_flag: bool) -> Result<Mode, AppError> {
    let command_mode = match command {
        Some(command) => Some(Mode::from_str(&command)?),
        None => None,
    };

    match (command_mode, reverse_flag) {
        (Some(Mode::Apply), true) => Err(AppError::Usage(
            "Do not combine the 'apply' alias with --reverse/--backup. Choose one direction."
                .to_string(),
        )),
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
        (Some(origin_dir), Some(destination_dir), []) => {
            Ok((absolute_path(origin_dir)?, absolute_path(destination_dir)?))
        }
        (None, None, [origin_dir, destination_dir]) => {
            Ok((absolute_path(origin_dir)?, absolute_path(destination_dir)?))
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
    eprintln!("  dotsync [options] --origin <origin_dir> --destination <destination_dir>");
    eprintln!(
        "  dotsync [options] --reverse --origin <origin_dir> --destination <destination_dir>"
    );
    eprintln!("  dotsync [options] reverse <origin_dir> <destination_dir>");
    eprintln!("  dotsync [options] <origin_dir> <destination_dir>");
    eprintln!();
    eprintln!("Direction:");
    eprintln!("  no command               Apply from origin to destination.");
    eprintln!("  --reverse                Copy from destination to origin (full dir sync).");
    eprintln!("  --reverse-only-files     Copy from destination to origin (existing files only).");
    eprintln!("  reverse                  Equivalent to --reverse.");
    eprintln!();
    eprintln!("Legacy aliases:");
    eprintln!("  apply               Explicit alias for the default mode.");
    eprintln!("  backup              Legacy alias for reverse.");
    eprintln!("  capture             Legacy alias for reverse.");
    eprintln!("  --backup            Legacy alias for --reverse.");
    eprintln!("  --repo              Legacy alias for --origin.");
    eprintln!("  --home              Legacy alias for --destination.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -n, --dry-run            Show what would happen without copying files.");
    eprintln!("      --reverse            Enable reverse synchronization.");
    eprintln!("      --reverse-only-files Enable reverse sync for existing files only.");
    eprintln!("      --origin <path>      Origin directory path.");
    eprintln!("      --destination <path> Destination directory path.");
    eprintln!("      --dest <path>        Short alias for --destination.");
    eprintln!("  -h, --help               Show this help message.");
    eprintln!("      --version            Show version information.");
}
