# dotsync

Rust CLI and library for synchronizing dotfiles with a `stow`-style structure.

## CLI usage

Applying from origin to destination is the default behavior:

```bash
dotsync <origin_dir> <destination_dir> [options]
```

To synchronize in reverse, from destination back to origin:

```bash
dotsync <origin_dir> <destination_dir> --reverse [options]
```

### What each mode does

#### Apply (default)

Copies files from `origin` (your repo) to `destination` (your machine), file by file.
Only the files that exist in the repo are touched — nothing else on your machine is affected.

```bash
# Files inside ~/.config
dotsync $HOME/dotfiles/config/.config/ ~/.config

# Dotfiles at the root of $HOME (.zshrc, .gitconfig, etc.)
dotsync $HOME/dotfiles/dot/ ~/
```

#### Reverse

Copies files and folders from `destination` (your machine) back to `origin` (your repo),
so you can capture changes made by apps and commit them.

- **Files** in `origin` are copied individually from `destination`.
- **Folders** in `origin` are synced entirely from `destination` using `rsync`, capturing
  any new files the app may have created inside them.
- Entries not present in `origin` are never touched — your repo is always the whitelist.
- Symlinks are skipped (they are machine-local and meaningless in a repo).

```bash
dotsync $HOME/dotfiles/config/.config/ ~/.config --reverse
```

Direction:

- No command: copies from `origin` to `destination`.
- `--reverse`: copies from `destination` to `origin`, following the structure of `origin`.

For dotfiles, usually:

- `origin`: your dotfiles repository, for example `$HOME/dotfiles`.
- `destination`: the system/home directory you sync against, for example `$HOME`.

Options:

- `-n`, `--dry-run`: shows what would happen without copying files.
- `-v`, `--verbose`: shows detailed output of operations.
- `--reverse`: enables reverse mode.
- `--reverse-only-files`: enables reverse mode but only copies files, skipping folders.
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
