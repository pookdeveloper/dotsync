# GitHub Release Process for Dotsync

This document outlines the steps to create a new release for the `dotsync` project and publish it on GitHub.

## Steps to Create and Publish a Release with GitHub CLI

### 1. Ensure Your Code is Ready for Release
   - Verify all changes are committed and pushed to the `main` branch.

### 2. Build the Project
   - Compile the project in release mode to generate the binary:
     ```bash
     cargo build --release
     ```

### 3. Package the Binary
   - Package the `dotsync` binary into a tar.gz file for distribution:
     ```bash
     tar -czvf dotsync-vX.Y.Z.tar.gz -C target/release dotsync
     ```
     Replace `X.Y.Z` with the desired version, e.g., `v0.1.0`.

### 4. Create Git Tag
   - Create and push a new Git tag for the release:
     ```bash
     git tag -a vX.Y.Z -m "Release vX.Y.Z"
     git push origin vX.Y.Z
     ```

### 5. Publish the GitHub Release
   - Use `gh` to create the release:
     ```bash
     gh release create vX.Y.Z dotsync-vX.Y.Z.tar.gz \
       --title "Release vX.Y.Z" \
       --notes "Release notes here."
     ```
     Replace `X.Y.Z` with the version number and update the release notes.

### Example for v0.1.0
```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
cargo build --release
tar -czvf dotsync-v0.1.0.tar.gz -C target/release dotsync
gh release create v0.1.0 dotsync-v0.1.0.tar.gz \
  --title "Release v0.1.0" \
  --notes "Initial release of dotsync. Includes CLI and library for applying and reversing dotfile sync."
```

This process ensures a smooth release with the correct binary attached to the release notes. For any issues, verify the GitHub CLI (`gh`) is authenticated and the repository has write permissions enabled.