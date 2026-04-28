use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

/// Synchronization direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Applies files from origin to destination.
    Apply,
    /// Syncs in reverse: from destination back to origin.
    Reverse,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Reverse => "reverse",
        }
    }
}

impl FromStr for Mode {
    type Err = DotSyncError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "apply" => Ok(Self::Apply),
            "reverse" => Ok(Self::Reverse),
            // Legacy aliases from earlier CLI iterations.
            "backup" | "capture" => Ok(Self::Reverse),
            _ => Err(DotSyncError::InvalidMode(value.to_string())),
        }
    }
}

/// Public synchronization options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOptions {
    pub mode: Mode,
    pub origin_dir: PathBuf,
    pub destination_dir: PathBuf,
    pub dry_run: bool,
    pub clean_ignored_files: bool,
}

impl SyncOptions {
    pub fn new(
        mode: Mode,
        origin_dir: impl Into<PathBuf>,
        destination_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            mode,
            origin_dir: origin_dir.into(),
            destination_dir: destination_dir.into(),
            dry_run: false,
            clean_ignored_files: true,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_clean_ignored_files(mut self, clean_ignored_files: bool) -> Self {
        self.clean_ignored_files = clean_ignored_files;
        self
    }
}

/// A planned or executed copy operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyOperation {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub executed: bool,
}

/// Git ignored-file cleanup status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanStatus {
    Planned {
        directory: PathBuf,
    },
    Executed {
        directory: PathBuf,
        stdout: String,
        stderr: String,
    },
    Skipped,
}

/// Accumulated synchronization result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub dry_run: bool,
    pub copy_operations: Vec<CopyOperation>,
    pub skipped_missing_files: Vec<PathBuf>,
    pub clean_status: CleanStatus,
}

impl SyncReport {
    pub fn executed_copies(&self) -> usize {
        self.copy_operations
            .iter()
            .filter(|operation| operation.executed)
            .count()
    }

    pub fn planned_copies(&self) -> usize {
        self.copy_operations
            .iter()
            .filter(|operation| !operation.executed)
            .count()
    }
}

#[derive(Debug)]
pub enum DotSyncError {
    InvalidMode(String),
    InvalidOriginDir(PathBuf),
    RelativePath { path: PathBuf, base: PathBuf },
    Io { context: String, source: io::Error },
    CommandFailed { command: String, status: String },
}

impl fmt::Display for DotSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMode(mode) => {
                write!(
                    f,
                    "Error: command '{mode}' must be 'reverse' or a valid alias."
                )
            }
            Self::InvalidOriginDir(path) => {
                write!(
                    f,
                    "Error: origin directory '{}' does not exist.",
                    path.display()
                )
            }
            Self::RelativePath { path, base } => write!(
                f,
                "Could not compute relative path for '{}' from base '{}'",
                path.display(),
                base.display()
            ),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::CommandFailed { command, status } => {
                write!(f, "Command '{command}' failed with status {status}")
            }
        }
    }
}

impl std::error::Error for DotSyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Synchronizes dotfiles according to the provided options.
///
/// In `dry_run`, it does not create directories, copy files, or run `git clean`.
pub fn sync_dotfiles(options: &SyncOptions) -> Result<SyncReport, DotSyncError> {
    validate_origin_dir(&options.origin_dir)?;

    let mut report = SyncReport {
        dry_run: options.dry_run,
        copy_operations: Vec::new(),
        skipped_missing_files: Vec::new(),
        clean_status: CleanStatus::Skipped,
    };

    process_directory(
        &options.origin_dir,
        &options.origin_dir,
        options,
        &mut report,
    )?;
    apply_clean_policy(options, &mut report)?;

    Ok(report)
}

fn validate_origin_dir(origin_dir: &Path) -> Result<(), DotSyncError> {
    if !origin_dir.exists() || !origin_dir.is_dir() {
        return Err(DotSyncError::InvalidOriginDir(origin_dir.to_path_buf()));
    }

    Ok(())
}

fn process_directory(
    current_dir: &Path,
    base_dir: &Path,
    options: &SyncOptions,
    report: &mut SyncReport,
) -> Result<(), DotSyncError> {
    let entries = fs::read_dir(current_dir).map_err(|source| DotSyncError::Io {
        context: format!("Could not read directory '{}'", current_dir.display()),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| DotSyncError::Io {
            context: format!("Could not read an entry from '{}'", current_dir.display()),
            source,
        })?;

        if entry.file_name() == OsStr::new(".git") {
            continue;
        }

        let full_entry_path = entry.path();
        let relative_path =
            full_entry_path
                .strip_prefix(base_dir)
                .map_err(|_| DotSyncError::RelativePath {
                    path: full_entry_path.clone(),
                    base: base_dir.to_path_buf(),
                })?;

        let file_type = entry.file_type().map_err(|source| DotSyncError::Io {
            context: format!(
                "Could not determine file type for '{}'",
                full_entry_path.display()
            ),
            source,
        })?;

        if file_type.is_dir() {
            process_directory(&full_entry_path, base_dir, options, report)?;
            continue;
        }

        match options.mode {
            Mode::Apply => {
                let destination = options.destination_dir.join(relative_path);
                copy_file(&full_entry_path, &destination, options.dry_run, report)?;
            }
            Mode::Reverse => {
                let source = options.destination_dir.join(relative_path);

                if source.exists() {
                    let destination = options.origin_dir.join(relative_path);
                    copy_file(&source, &destination, options.dry_run, report)?;
                } else {
                    report
                        .skipped_missing_files
                        .push(relative_path.to_path_buf());
                }
            }
        }
    }

    Ok(())
}

fn copy_file(
    source: &Path,
    destination: &Path,
    dry_run: bool,
    report: &mut SyncReport,
) -> Result<(), DotSyncError> {
    if !dry_run {
        if let Some(destination_dir) = destination.parent() {
            fs::create_dir_all(destination_dir).map_err(|source_error| DotSyncError::Io {
                context: format!("Could not create directory '{}'", destination_dir.display()),
                source: source_error,
            })?;
        }

        fs::copy(source, destination).map_err(|source_error| DotSyncError::Io {
            context: format!(
                "Could not copy '{}' to '{}'",
                source.display(),
                destination.display()
            ),
            source: source_error,
        })?;
    }

    report.copy_operations.push(CopyOperation {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        executed: !dry_run,
    });

    Ok(())
}

fn apply_clean_policy(options: &SyncOptions, report: &mut SyncReport) -> Result<(), DotSyncError> {
    if !options.clean_ignored_files {
        report.clean_status = CleanStatus::Skipped;
        return Ok(());
    }

    if options.dry_run {
        report.clean_status = CleanStatus::Planned {
            directory: options.origin_dir.clone(),
        };
        return Ok(());
    }

    let output = Command::new("git")
        .args(["clean", "-fdX", "."])
        .current_dir(&options.origin_dir)
        .output()
        .map_err(|source| DotSyncError::Io {
            context: "Could not execute 'git clean -fdX .'".to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(DotSyncError::CommandFailed {
            command: "git clean -fdX .".to_string(),
            status: output
                .status
                .code()
                .map_or_else(|| "unknown".to_string(), |code| code.to_string()),
        });
    }

    report.clean_status = CleanStatus::Executed {
        directory: options.origin_dir.clone(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    Ok(())
}
