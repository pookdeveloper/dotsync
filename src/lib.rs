use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

mod ignore;
use ignore::IgnoreRules;

/// Public synchronization options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOptions {
    pub origin_dir: PathBuf,
    pub destination_dir: PathBuf,
    pub dry_run: bool,
    /// When true, logs every copied file. Warnings and errors are always shown.
    pub verbose: bool,
    /// When set, `.dotsyncignore` is loaded from this directory instead of `origin_dir`.
    /// Needed for scoped applies where `origin_dir` is a subdirectory of the repo.
    pub ignore_root: Option<PathBuf>,
}

impl SyncOptions {
    pub fn new(
        origin_dir: impl Into<PathBuf>,
        destination_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            origin_dir: origin_dir.into(),
            destination_dir: destination_dir.into(),
            dry_run: false,
            verbose: false,
            ignore_root: None,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn with_ignore_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.ignore_root = Some(root.into());
        self
    }
}

#[derive(Debug)]
pub enum DotSyncError {
    InvalidOriginDir(PathBuf),
    NotUnderHome { source: PathBuf, home: PathBuf },
    RelativePath { path: PathBuf, base: PathBuf },
    Io { context: String, source: io::Error },
    CommandFailed { command: String, status: String },
}

impl fmt::Display for DotSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOriginDir(path) => {
                write!(
                    f,
                    "Error: origin directory '{}' does not exist.",
                    path.display()
                )
            }
            Self::NotUnderHome { source, home } => write!(
                f,
                "Error: '{}' is not under HOME '{}'.",
                source.display(),
                home.display()
            ),
            Self::RelativePath { path, base } => write!(
                f,
                "Could not compute relative path for '{}' from base '{}'",
                path.display(),
                base.display()
            ),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::CommandFailed { command, status } => {
                write!(f, "Command failed with exit code {status}: {command}")
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

/// Creates a `.dotsyncignore` file in `repo_dir` with a comment header if one does not
/// already exist. Called automatically by `dotsync config` to scaffold the file.
pub fn init_ignore_file(repo_dir: &Path) -> Result<bool, DotSyncError> {
    let path = repo_dir.join(".dotsyncignore");
    if path.exists() {
        return Ok(false);
    }

    let content = "\
# .dotsyncignore — paths that dotsync will never copy or apply.
# Patterns follow gitignore semantics:
#   - No slash → matches name at any depth        (.DS_Store, *.log)
#   - With slash → relative to the repo root      (.config/nvim/sessions/)
#   - Leading /  → anchored to root               (/.zshrc.bak)
#   - Prefix !   → negate a previous pattern      (!important.log)
#   - Lines starting with # are comments
";

    fs::write(&path, content).map_err(|source| DotSyncError::Io {
        context: format!("Could not create '{}'", path.display()),
        source,
    })?;

    Ok(true)
}

/// Copies a single dotfile or directory from its live location into the repo,
/// preserving its path relative to `home_dir`.
///
/// `dotsync add ~/.config/nvim` with `home_dir=$HOME` and `repo_dir=~/dotfiles`
/// copies to `~/dotfiles/.config/nvim`.
pub fn add_dotfile(
    source: &Path,
    home_dir: &Path,
    repo_dir: &Path,
    dry_run: bool,
    verbose: bool,
) -> Result<(), DotSyncError> {
    let relative = source
        .strip_prefix(home_dir)
        .map_err(|_| DotSyncError::NotUnderHome {
            source: source.to_path_buf(),
            home: home_dir.to_path_buf(),
        })?;

    if !source.exists() {
        return Err(DotSyncError::InvalidOriginDir(source.to_path_buf()));
    }

    let rules = IgnoreRules::load(repo_dir);
    if rules.is_ignored(relative) {
        if verbose {
            println!("Ignored: {}", source.display());
        }
        return Ok(());
    }

    if source.is_dir() {
        add_dotfile_dir(source, home_dir, repo_dir, &rules, dry_run, verbose)
    } else {
        copy_file(source, &repo_dir.join(relative), dry_run, verbose)
    }
}

fn add_dotfile_dir(
    source_dir: &Path,
    home_dir: &Path,
    repo_dir: &Path,
    rules: &IgnoreRules,
    dry_run: bool,
    verbose: bool,
) -> Result<(), DotSyncError> {
    for entry in read_dir_entries(source_dir)? {
        if entry.is_symlink() {
            eprintln!("Warning: skipping symlink '{}'.", entry.display());
            continue;
        }

        let relative = entry
            .strip_prefix(home_dir)
            .map_err(|_| DotSyncError::RelativePath {
                path: entry.clone(),
                base: home_dir.to_path_buf(),
            })?;

        if rules.is_ignored(relative) {
            if verbose {
                println!("Ignored: {}", entry.display());
            }
            continue;
        }

        if entry.is_dir() {
            add_dotfile_dir(&entry, home_dir, repo_dir, rules, dry_run, verbose)?;
        } else {
            copy_file(&entry, &repo_dir.join(relative), dry_run, verbose)?;
        }
    }
    Ok(())
}

/// Re-adds tracked dotfiles from `home_dir` back into `repo_dir`.
///
/// When `dirs` is false, each tracked file is copied individually.
/// When `dirs` is true, the parent directory of each tracked file is copied
/// recursively, which also captures new files in those directories.
pub fn readd_dotfiles(
    repo_dir: &Path,
    home_dir: &Path,
    dirs: bool,
    dry_run: bool,
    verbose: bool,
) -> Result<(), DotSyncError> {
    validate_origin_dir(repo_dir)?;

    let rules = IgnoreRules::load(repo_dir);
    let units = collect_readd_units(repo_dir, dirs, &rules)?;

    for unit in &units {
        let source = home_dir.join(unit);
        let target = repo_dir.join(unit);

        if source.is_symlink() {
            eprintln!("Warning: skipping symlink '{}'.", source.display());
            continue;
        }

        if !source.exists() {
            eprintln!("Warning: '{}' not found, skipping.", source.display());
            continue;
        }

        if source.is_dir() {
            copy_dir_all(&source, &target, dry_run, false, verbose)?;
        } else {
            copy_file(&source, &target, dry_run, verbose)?;
        }
    }

    Ok(())
}

fn collect_readd_units(
    repo_dir: &Path,
    dirs: bool,
    rules: &IgnoreRules,
) -> Result<BTreeSet<PathBuf>, DotSyncError> {
    let mut units = BTreeSet::new();
    collect_leaf_files(repo_dir, repo_dir, dirs, rules, &mut units)?;
    Ok(units)
}

fn collect_leaf_files(
    current: &Path,
    repo_dir: &Path,
    dirs: bool,
    rules: &IgnoreRules,
    units: &mut BTreeSet<PathBuf>,
) -> Result<(), DotSyncError> {
    for entry in read_dir_entries(current)? {
        let name = entry.file_name();
        if name == Some(OsStr::new(".git")) || name == Some(OsStr::new(".dotsyncignore")) {
            continue;
        }

        let relative =
            entry
                .strip_prefix(repo_dir)
                .map_err(|_| DotSyncError::RelativePath {
                    path: entry.clone(),
                    base: repo_dir.to_path_buf(),
                })?;

        if rules.is_ignored(relative) {
            continue;
        }

        if entry.is_dir() {
            collect_leaf_files(&entry, repo_dir, dirs, rules, units)?;
        } else {
            let unit = if dirs {
                effective_dir_unit(relative)
            } else {
                relative.to_path_buf()
            };
            units.insert(unit);
        }
    }

    Ok(())
}

/// Determines the copy unit for a file path when `--dirs` is active.
///
/// Rules:
/// - 1 component (file directly in HOME): keep the file.
/// - Under `.config/`: cap at `.config/<app>`, never `.config/` itself.
/// - Anywhere else: use the immediate parent directory.
fn effective_dir_unit(relative: &Path) -> PathBuf {
    let components: Vec<_> = relative.components().collect();

    match components.as_slice() {
        [_] => relative.to_path_buf(),
        [first, second, ..] if first.as_os_str() == ".config" => {
            Path::new(first.as_os_str()).join(second.as_os_str())
        }
        _ => relative.parent().unwrap_or(relative).to_path_buf(),
    }
}

/// Synchronizes dotfiles from `origin_dir` (repo) to `destination_dir` (home).
pub fn sync_dotfiles(options: &SyncOptions) -> Result<(), DotSyncError> {
    validate_origin_dir(&options.origin_dir)?;

    let rules_dir = options.ignore_root.as_deref().unwrap_or(&options.origin_dir);
    let rules = IgnoreRules::load(rules_dir);

    process_apply(&options.origin_dir, &options.origin_dir, options, &rules)
}

fn is_internal_entry(path: &Path) -> bool {
    matches!(
        path.file_name(),
        Some(n) if n == OsStr::new(".git") || n == OsStr::new(".dotsyncignore")
    )
}

fn validate_origin_dir(origin_dir: &Path) -> Result<(), DotSyncError> {
    if !origin_dir.exists() || !origin_dir.is_dir() {
        return Err(DotSyncError::InvalidOriginDir(origin_dir.to_path_buf()));
    }
    Ok(())
}

fn process_apply(
    current_dir: &Path,
    base_dir: &Path,
    options: &SyncOptions,
    rules: &IgnoreRules,
) -> Result<(), DotSyncError> {
    for full_entry_path in read_dir_entries(current_dir)? {
        if is_internal_entry(&full_entry_path) {
            continue;
        }

        let relative_path =
            full_entry_path
                .strip_prefix(base_dir)
                .map_err(|_| DotSyncError::RelativePath {
                    path: full_entry_path.clone(),
                    base: base_dir.to_path_buf(),
                })?;

        if rules.is_ignored(relative_path) {
            continue;
        }

        if full_entry_path.is_dir() {
            process_apply(&full_entry_path, base_dir, options, rules)?;
        } else {
            let destination = options.destination_dir.join(relative_path);
            copy_file(&full_entry_path, &destination, options.dry_run, options.verbose)?;
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
    verbose: bool,
) -> Result<(), DotSyncError> {
    if dry_run {
        if verbose {
            println!(
                "[dry-run] Would rsync: {} -> {}",
                src_dir.display(),
                dst_dir.display()
            );
        }
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
    if verbose {
        for line in transferred.lines() {
            println!("{}/{}", dst_dir.display(), line.trim_start_matches("Copied: "));
        }
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

fn copy_file(source: &Path, destination: &Path, dry_run: bool, verbose: bool) -> Result<(), DotSyncError> {
    if dry_run {
        if verbose {
            println!(
                "[dry-run] Would copy: {} -> {}",
                source.display(),
                destination.display()
            );
        }
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

    if verbose {
        println!("Copied: {} -> {}", source.display(), destination.display());
    }

    Ok(())
}
