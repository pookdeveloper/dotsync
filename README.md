# dotsync

Rust CLI and library for managing dotfiles with a `stow`-style structure.

## Commands

### `dotsync config <destination_path>`

Set the repository destination path (where your dotfiles repo lives). This writes to `~/.config/dotsync/config.toml` so other commands know where to find your repo.

```bash
dotsync config ~/dotfiles
```

If `<destination_path>` is a directory, `dotsync` will also scaffold a `.dotsyncignore` file with a comment header if one does not already exist.

### `dotsync add <path>`

Copy a dotfile or directory from `$HOME` into the repo, preserving its path relative to `$HOME`.

```bash
# Copy ~/.config/nvim into ~/dotfiles/.config/nvim
dotsync add ~/.config/nvim

# Copy ~/.zshrc into ~/dotfiles/.zshrc
dotsync add ~/.zshrc
```

Files matching `.dotsyncignore` patterns are skipped automatically.

Options:

- `-n`, `--dry-run`: show what would happen without copying files.
- `-v`, `--verbose`: print each copied file (warnings always shown).

### `dotsync apply [<path>]`

Apply tracked dotfiles from the repo to `$HOME`.

```bash
# Apply everything
dotsync apply

# Apply only .config/nvim
dotsync apply .config/nvim
```

- Without `<path>`: copies every tracked file from the repo to `$HOME`.
- With `<path>`: applies only that subdirectory.

Files matching `.dotsyncignore` are skipped.

Options:

- `-n`, `--dry-run`: show what would happen without copying files.
- `-v`, `--verbose`: print each copied file.

### `dotsync readd [--dirs]`

Re-add tracked files from `$HOME` back into the repo. Useful when apps have updated their config files and you want to capture those changes.

```bash
# Re-add each tracked file individually
dotsync readd

# Re-add by parent directory, also capturing new files
dotsync readd --dirs
```

- Without `--dirs`: only the exact files already tracked in the repo are copied back.
- With `--dirs`: the parent directory of each tracked file is copied recursively, so new files created by the app are also captured.

Options:

- `--dirs`: group by directory instead of copying file by file.
- `-n`, `--dry-run`: show what would happen without copying files.
- `-v`, `--verbose`: print each copied file.

## `.dotsyncignore`

Patterns follow gitignore semantics:

- No slash → matches name at any depth (`.DS_Store`, `*.log`)
- With slash → relative to the repo root (`.config/nvim/sessions/`)
- Leading `/` → anchored to root (`/.zshrc.bak`)
- Prefix `!` → negate a previous pattern (`!important.log`)
- Lines starting with `#` are comments

## Global options

- `-h`, `--help`: show usage.
- `--version`: show version information.

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
