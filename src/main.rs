use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use dotsync::{sync_dotfiles, CleanStatus, DotSyncError, Mode, SyncOptions, SyncReport};

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
    let report = sync_dotfiles(&options)?;

    // print_report(&report, &options);

    println!(
        "Files processed successfully with '{}'.",
        options.mode.as_str()
    );

    Ok(())
}

fn parse_arguments(args: impl IntoIterator<Item = String>) -> Result<SyncOptions, AppError> {
    let mut dry_run = false;
    let mut clean_ignored_files = true;
    let mut reverse_flag = false;
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
            "--no-clean" => clean_ignored_files = false,
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
        .with_clean_ignored_files(clean_ignored_files))
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

fn print_report(report: &SyncReport, options: &SyncOptions) {

    println!("Copying files from {} to {} \n", options.origin_dir.display(), options.destination_dir.display());

    for operation in &report.copy_operations {
        if operation.executed {
            println!(
                "Copied: {} -> {}",
                operation.source.display(),
                operation.destination.display()
            );
        } else {
            println!(
                "[dry-run] Would copy: {} -> {}",
                operation.source.display(),
                operation.destination.display()
            );
        }
    }

    for relative_path in &report.skipped_missing_files {
        eprintln!(
            "Warning: file {} was not found in {}. It will be skipped.",
            relative_path.display(),
            options.destination_dir.display()
        );
    }

    for symlink_path in &report.skipped_symlinks {
        eprintln!(
            "Warning: skipping symlink '{}'.",
            symlink_path.display()
        );
    }

    match &report.clean_status {
        CleanStatus::Planned { directory } => {
            println!(
                "[dry-run] Would run: git clean -fdX . in {}",
                directory.display()
            );
        }
        CleanStatus::Executed {
            directory,
            stdout,
            stderr,
        } => {
            println!("Running: git clean -fdX . in {} (This ignores copying files that are excluded) ", directory.display());
            print_command_output(stdout);
            print_command_output(stderr);
        }
        CleanStatus::Skipped => {
            if !options.clean_ignored_files {
                println!("Cleanup skipped by --no-clean.");
            }
        }
    }

    if report.dry_run {
        println!(
            "[dry-run] Summary: {} planned copies, {} skipped files, {} skipped symlinks.",
            report.planned_copies(),
            report.skipped_missing_files.len(),
            report.skipped_symlinks.len()
        );
    } else {
        println!(
            "Summary: {} executed copies, {} skipped files, {} skipped symlinks.",
            report.executed_copies(),
            report.skipped_missing_files.len(),
            report.skipped_symlinks.len()
        );
    }
}

fn print_command_output(output: &str) {
    if output.is_empty() {
        return;
    }

    print!("{output}");
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
    eprintln!("  no command          Apply from origin to destination.");
    eprintln!(
        "  --reverse           Copy from destination to origin following origin's structure."
    );
    eprintln!("  reverse             Equivalent to --reverse.");
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
    eprintln!("  -n, --dry-run       Show what would happen without copying or cleaning.");
    eprintln!("      --reverse       Enable reverse synchronization.");
    eprintln!("      --no-clean      Do not run git clean -fdX . at the end.");
    eprintln!("      --origin <path> Origin directory path.");
    eprintln!("      --destination <path>");
    eprintln!("                         Destination directory path.");
    eprintln!("      --dest <path>   Short alias for --destination.");
    eprintln!("  -h, --help          Show this help message.");
    eprintln!("      --version       Show version information.");
}
