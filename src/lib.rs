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
    /// Symlinks encountered during reverse mode — skipped intentionally.
    pub skipped_symlinks: Vec<PathBuf>,
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
        skipped_symlinks: Vec::new(),
        clean_status: CleanStatus::Skipped,
    };

    match options.mode {
        Mode::Apply => process_apply(
            &options.origin_dir,
            &options.origin_dir,
            options,
            &mut report,
        )?,
        Mode::Reverse => process_reverse_root(options, &mut report)?,
    }

    apply_clean_policy(options, &mut report)?;

    Ok(report)
}

fn validate_origin_dir(origin_dir: &Path) -> Result<(), DotSyncError> {
    if !origin_dir.exists() || !origin_dir.is_dir() {
        return Err(DotSyncError::InvalidOriginDir(origin_dir.to_path_buf()));
    }

    Ok(())
}

/// Apply mode: iterates `origin_dir` and copies each file to `destination_dir`.
fn process_apply(
    current_dir: &Path,
    base_dir: &Path,
    options: &SyncOptions,
    report: &mut SyncReport,
) -> Result<(), DotSyncError> {
    let entries = read_dir_entries(current_dir)?;

    for full_entry_path in entries {
        if full_entry_path.file_name() == Some(OsStr::new(".git")) {
            continue;
        }

        let relative_path =
            full_entry_path
                .strip_prefix(base_dir)
                .map_err(|_| DotSyncError::RelativePath {
                    path: full_entry_path.clone(),
                    base: base_dir.to_path_buf(),
                })?;

        if full_entry_path.is_dir() {
            process_apply(&full_entry_path, base_dir, options, report)?;
        } else {
            let destination = options.destination_dir.join(relative_path);
            copy_file(&full_entry_path, &destination, options.dry_run, report)?;
        }
    }

    Ok(())
}

/// Reverse mode: iterates `origin_dir` (the repo) and copies each entry from
/// `destination_dir` (the machine) back into `origin_dir`.
///
/// - Only entries that exist in `origin_dir` are considered — this is the
///   whitelist. If `.trunk` is not in the repo, it will never be touched.
/// - Directories found in `origin_dir` are copied **entirely** from
///   `destination_dir`, capturing new files the app may have added.
/// - If an entry does not exist in `destination_dir`, it is skipped.
/// - Symlinks are skipped and recorded in the report.
fn process_reverse_root(
    options: &SyncOptions,
    report: &mut SyncReport,
) -> Result<(), DotSyncError> {
    let entries = read_dir_entries(&options.origin_dir)?;

    for full_origin_path in entries {
        if full_origin_path.file_name() == Some(OsStr::new(".git")) {
            continue;
        }

        let relative_path = full_origin_path
            .strip_prefix(&options.origin_dir)
            .map_err(|_| DotSyncError::RelativePath {
                path: full_origin_path.clone(),
                base: options.origin_dir.clone(),
            })?;

        let source = options.destination_dir.join(relative_path);

        if !source.exists() && !source.is_symlink() {
            report.skipped_missing_files.push(relative_path.to_path_buf());
            continue;
        }

        if source.is_symlink() {
            report.skipped_symlinks.push(source);
            continue;
        }

        let dest = options.origin_dir.join(relative_path);

        if source.is_dir() {
            copy_dir_all(&source, &dest, options.dry_run, report)?;
        } else {
            copy_file(&source, &dest, options.dry_run, report)?;
        }
    }

    Ok(())
}

/// Recursively syncs `src_dir` into `dst_dir` using `rsync -a --no-links`.
///
/// `-a` (archive) is recursive and preserves permissions and timestamps.
/// `--no-links` skips symlinks — they are machine-local and meaningless in a repo.
/// The trailing slash on `src` makes rsync copy the contents of `src` directly
/// into `dst`, not nest `src` as a subdirectory inside `dst`.
fn copy_dir_all(
    src_dir: &Path,
    dst_dir: &Path,
    dry_run: bool,
    report: &mut SyncReport,
) -> Result<(), DotSyncError> {
    // report.copy_operations.push(CopyOperation {
    //     source: src_dir.to_path_buf(),
    //     destination: dst_dir.to_path_buf(),
    //     executed: !dry_run,
    // });

    if dry_run {
        return Ok(());
    }

    if let Some(parent) = dst_dir.parent() {
        fs::create_dir_all(parent).map_err(|source| DotSyncError::Io {
            context: format!("Could not create directory '{}'", parent.display()),
            source,
        })?;
    }

    // Trailing slash on src makes rsync copy the *contents* of src into dst,
    // not nest src itself inside dst (rsync semantics).
    let src_with_slash = format!("{}/", src_dir.display());

    let output = Command::new("rsync")
        .args([
            "-a",                      // archive: recursive + preserve permissions/times
            "--no-links",              // skip symlinks entirely
            "--out-format=Copied: %n", // print each transferred file
        ])
        .arg(&src_with_slash)
        .arg(dst_dir)
        .output()
        .map_err(|source| DotSyncError::Io {
            context: "Could not execute 'rsync'".to_string(),
            source,
        })?;

    // Print each file rsync transferred, prefixing with the dst base path
    // so the output is consistent with copy_file logs.
    let transferred = String::from_utf8_lossy(&output.stdout);
    for line in transferred.lines() {
        println!("{}/{}", dst_dir.display(), line.trim_start_matches("Copied: "));
    }

    if !output.status.success() {
        return Err(DotSyncError::CommandFailed {
            command: format!("rsync -a --no-links {src_with_slash} {}", dst_dir.display()),
            status: output
                .status
                .code()
                .map_or_else(|| "unknown".to_string(), |c| c.to_string()),
        });
    }

    Ok(())
}

fn read_dir_entries(dir: &Path) -> Result<Vec<PathBuf>, DotSyncError> {
    let entries = fs::read_dir(dir).map_err(|source| DotSyncError::Io {
        context: format!("Could not read directory '{}'", dir.display()),
        source,
    })?;

    entries
        .map(|entry| {
            entry
                .map(|e| e.path())
                .map_err(|source| DotSyncError::Io {
                    context: format!("Could not read an entry from '{}'", dir.display()),
                    source,
                })
        })
        .collect()
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

        print!("Copying '{}' to '{}'", source.display(), destination.display());
        fs::copy(source, destination).map_err(|source_error| DotSyncError::Io {
            context: format!(
                "Could not copy '{}' to '{}'",
                source.display(),
                destination.display()
            ),
            source: source_error,
        })?;
    }

    // report.copy_operations.push(CopyOperation {
    //     source: source.to_path_buf(),
    //     destination: destination.to_path_buf(),
    //     executed: !dry_run,
    // });

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

    // Preview what will be deleted before doing it, so the user can see
    // what files are about to be removed (e.g. newly captured files).
    let preview = Command::new("git")
        .args(["clean", "-ndX", "."])
        .current_dir(&options.origin_dir)
        .output()
        .map_err(|source| DotSyncError::Io {
            context: "Could not execute 'git clean -ndX .'".to_string(),
            source,
        })?;

    // let preview_stdout = String::from_utf8_lossy(&preview.stdout).to_string();
    // if !preview_stdout.is_empty() {
    //     println!("\nThe following git-ignored files will be removed (use --no-clean to keep them):");
    //     print!("{preview_stdout}");
    // }

    let output = Command::new("git")
        .args(["clean", "-fdX", "."])
        .current_dir(&options.origin_dir)
        .output()
        .map_err(|source| DotSyncError::Io {
            context: "Could not execute 'git clean -fdX .'".to_string(),
            source,
        })?;

    if !output.status.success() {
        // git clean exits with status 1 when it cannot remove some files due
        // to permission errors (e.g. read-only cache dirs). These are warnings,
        // not hard failures — print stderr and continue instead of aborting.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.lines().all(|l| l.starts_with("warning:")) {
            eprintln!("Warning: git clean could not remove some files (permission denied):");
            eprint!("{stderr}");
        } else {
            return Err(DotSyncError::CommandFailed {
                command: "git clean -fdX .".to_string(),
                status: output
                    .status
                    .code()
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string()),
            });
        }
    }

    report.clean_status = CleanStatus::Executed {
        directory: options.origin_dir.clone(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    Ok(())
}
