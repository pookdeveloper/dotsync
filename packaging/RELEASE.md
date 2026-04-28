# dotsync release process

## Contract for `curl` installation

`scripts/install.sh` expects GitHub Releases to publish binary assets with these names:

- `dotsync-aarch64-apple-darwin.tar.gz`
- `dotsync-x86_64-apple-darwin.tar.gz`
- `dotsync-aarch64-unknown-linux-gnu.tar.gz`
- `dotsync-x86_64-unknown-linux-gnu.tar.gz`

Each `.tar.gz` file must contain the `dotsync` binary at the archive root.

Install from the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/pookdeveloper/dotsync/main/scripts/install.sh | sh
```

The installer defaults to `pookdeveloper/dotsync`. Override it with `DOTSYNC_REPO=owner/repo` when testing forks.

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/pookdeveloper/dotsync/main/scripts/install.sh \
  | DOTSYNC_VERSION=v0.1.0 sh
```

## Homebrew

The formula template is available at `packaging/homebrew/dotsync.rb`.

Options:

1. **Dedicated tap**: create a `homebrew-tap` repository, copy the formula to `Formula/dotsync.rb`, and publish with:

   ```bash
   brew tap OWNER/tap
   brew install dotsync
   ```

2. **Direct formula URL**: useful for testing, worse for end users because it does not provide a stable channel or tap history.

Real tradeoff: `curl | sh` is convenient for quick installs, but Homebrew gives updates, auditability, and clean uninstallation. For serious distribution, Homebrew should be the primary path; `curl` remains a fallback.
