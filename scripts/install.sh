#!/usr/bin/env sh
set -eu

DEFAULT_REPO="pookdeveloper/dotsync"
REPO="${DOTSYNC_REPO:-$DEFAULT_REPO}"
VERSION="${DOTSYNC_VERSION:-latest}"
INSTALL_DIR="${DOTSYNC_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) os_part="apple-darwin" ;;
  Linux) os_part="unknown-linux-gnu" ;;
  *)
    echo "Unsupported operating system: $os" >&2
    exit 1
    ;;
esac

case "$arch" in
  x86_64 | amd64) arch_part="x86_64" ;;
  arm64 | aarch64) arch_part="aarch64" ;;
  *)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

target="$arch_part-$os_part"
asset="dotsync-$target.tar.gz"

if [ "$VERSION" = "latest" ]; then
  url="https://github.com/$REPO/releases/latest/download/$asset"
else
  url="https://github.com/$REPO/releases/download/$VERSION/$asset"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

mkdir -p "$INSTALL_DIR"

echo "Downloading $url"
curl -fsSL "$url" -o "$tmp_dir/$asset"
tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
install -m 0755 "$tmp_dir/dotsync" "$INSTALL_DIR/dotsync"

echo "dotsync installed at $INSTALL_DIR/dotsync"
echo "Make sure $INSTALL_DIR is in your PATH."
