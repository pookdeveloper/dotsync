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
    /// When true, reverse mode copies only the exact files present in `origin_dir`,
    /// using rsync `--existing` so no new files are created in the repo.
    pub reverse_only_files: bool,
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
            reverse_only_files: false,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_reverse_only_files(mut self, reverse_only_files: bool) -> Self {
        self.reverse_only_files = reverse_only_files;
        self
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
/// In `dry_run`, it does not create directories or copy files.
pub fn sync_dotfiles(options: &SyncOptions) -> Result<(), DotSyncError> {
    validate_origin_dir(&options.origin_dir)?;

    match options.mode {
        Mode::Apply => process_apply(&options.origin_dir, &options.origin_dir, options)?,
        Mode::Reverse => process_reverse_root(options)?,
    }

    Ok(())
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
            process_apply(&full_entry_path, base_dir, options)?;
        } else {
            let destination = options.destination_dir.join(relative_path);
            copy_file(&full_entry_path, &destination, options.dry_run)?;
        }
    }

    Ok(())
}

/// Reverse mode: iterates `origin_dir` (the repo) and copies each entry from
/// `destination_dir` (the machine) back into `origin_dir`.
///
/// - Only entries that exist in `origin_dir` are considered — the repo is the whitelist.
/// - Directories: if `reverse_only_files` is set, rsync uses `--existing` to update
///   only files already in the repo; otherwise full sync captures new files too.
/// - Entries missing from `destination_dir` or that are symlinks are skipped.
fn process_reverse_root(options: &SyncOptions) -> Result<(), DotSyncError> {
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
            eprintln!(
                "Warning: '{}' not found in destination, skipping.",
                relative_path.display()
            );
            continue;
        }

        if source.is_symlink() {
            eprintln!(
                "Warning: skipping symlink '{}'.",
                source.display()
            );
            continue;
        }

        let dest = options.origin_dir.join(relative_path);

        if source.is_dir() {
            copy_dir_all(&source, &dest, options.dry_run, options.reverse_only_files)?;
        } else {
            copy_file(&source, &dest, options.dry_run)?;
        }
    }

    Ok(())
}

/// Recursively syncs `src_dir` into `dst_dir` using `rsync -a --no-links`.
///
/// `-a` (archive) is recursive and preserves permissions and timestamps.
/// `--no-links` skips symlinks — they are machine-local and meaningless in a repo.
/// `only_existing` adds `--existing` so rsync never creates new files in `dst_dir`.
fn copy_dir_all(
    src_dir: &Path,
    dst_dir: &Path,
    dry_run: bool,
    only_existing: bool,
) -> Result<(), DotSyncError> {
    if dry_run {
        println!(
            "[dry-run] Would rsync: {} -> {}",
            src_dir.display(),
            dst_dir.display()
        );
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

    let mut args = vec![
        "-a",                      // archive: recursive + preserve permissions/times
        "--no-links",              // skip symlinks entirely
        "--out-format=Copied: %n", // print each transferred file
    ];

    if only_existing {
        args.push("--existing"); // never create new files in dst
    }

    let output = Command::new("rsync")
        .args(&args)
        .arg(&src_with_slash)
        .arg(dst_dir)
        .output()
        .map_err(|source| DotSyncError::Io {
            context: "Could not execute 'rsync'".to_string(),
            source,
        })?;

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

fn copy_file(source: &Path, destination: &Path, dry_run: bool) -> Result<(), DotSyncError> {
    if dry_run {
        println!(
            "[dry-run] Would copy: {} -> {}",
            source.display(),
            destination.display()
        );
        return Ok(());
    }

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

    println!("Copied: {} -> {}", source.display(), destination.display());

    Ok(())
}
