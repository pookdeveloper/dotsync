# dotsync

Rust CLI and library for synchronizing dotfiles with a `stow`-style structure.

## CLI usage

Applying from origin to destination is the default behavior:

```bash
dotsync [options] --origin <origin_dir> --destination <destination_dir>
```

To synchronize in reverse, from destination back to origin:

```bash
dotsync [options] --reverse --origin <origin_dir> --destination <destination_dir>
```

### What the reverse mode does

The **reverse mode** synchronizes files from `destination` back to `origin` (your dotfiles repository).

- Only iterates files already present in `origin`, so nothing unexpected is pulled from `destination`.
- Uses the same diff logic as the default mode — no copies unless the content actually differs.

Positional paths are also supported:

```bash
# Default apply mode
dotsync [options] <origin_dir> <destination_dir>

# Explicit reverse mode
dotsync [options] reverse <origin_dir> <destination_dir>
```

Direction:

- No command: copies from `origin` to `destination`.
- `--reverse` or `reverse`: copies from `destination` to `origin`, following the structure of `origin`.

For dotfiles, usually:

- `origin`: your dotfiles repository, for example `$HOME/dotfiles`.
- `destination`: the system/home directory you sync against, for example `$HOME`.

Legacy aliases:

- `apply`: explicit alias for the default mode.
- `backup`: legacy alias for `reverse`.
- `capture`: legacy alias for `reverse`.
- `--backup`: legacy alias for `--reverse`.
- `--repo`: legacy alias for `--origin`.
- `--home`: legacy alias for `--destination`.

Options:

- `-n`, `--dry-run`: shows what would happen without copying files or running cleanup.
- `--reverse`: enables reverse mode.
- `--no-clean`: avoids running `git clean -fdX .` at the end.
- `--origin <path>`: origin directory path.
- `--destination <path>`: destination directory path.
- `--dest <path>`: short alias for `--destination`.
- `-h`, `--help`: shows help.
- `--version`: shows the version.

Examples:

```bash
# Simulate applying from origin to destination
dotsync --dry-run --origin $HOME/dotfiles --destination $HOME

# Apply from origin to destination
dotsync --origin $HOME/dotfiles --destination $HOME

# Simulate reverse sync from destination to origin
dotsync --dry-run --reverse --origin $HOME/dotfiles --destination $HOME

# Reverse sync from destination to origin
dotsync --reverse --origin $HOME/dotfiles --destination $HOME

# Alternative reverse form
dotsync reverse $HOME/dotfiles $HOME
```

Notes:

- `.git` directories are skipped.
- By default, `git clean -fdX .` runs inside the origin directory at the end.
- In `--dry-run`, no directories are created, no files are copied, and `git clean` is not executed.

## Installation with curl

`scripts/install.sh` installs binaries published in GitHub Releases. The repository must publish assets according to the contract documented in `packaging/RELEASE.md`.

```bash
curl -fsSL https://raw.githubusercontent.com/pookdeveloper/dotsync/main/scripts/install.sh | sh
```

Advanced users can override the GitHub repository with `DOTSYNC_REPO=owner/repo` when testing forks.

To install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/pookdeveloper/dotsync/main/scripts/install.sh \
  | DOTSYNC_VERSION=v0.1.0 sh
```

## Homebrew

You can install `dotsync` using Homebrew in a few steps if the formula has been published to a tap:

```bash
brew tap pookdeveloper/tap
brew install dotsync
```
