#!/usr/bin/env bash
# kiro-cc-plugins installer
# Usage: curl -fsSL https://raw.githubusercontent.com/vokako/kiro-cc-plugins/main/install.sh | bash
set -e

REPO="vokako/kiro-cc-plugins"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS-$ARCH" in
  Darwin-arm64)  TARGET="aarch64-apple-darwin" ;;
  Darwin-x86_64) TARGET="x86_64-apple-darwin" ;;
  Linux-x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  *) echo "Unsupported platform: $OS-$ARCH" >&2; exit 1 ;;
esac

# Resolve latest tag if VERSION not set
VERSION="${VERSION:-$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)}"
if [ -z "$VERSION" ]; then
  echo "Failed to detect latest version" >&2; exit 1
fi

URL="https://github.com/$REPO/releases/download/$VERSION/kiro-cc-plugins-$TARGET.tar.gz"
echo "Downloading $URL"

mkdir -p "$INSTALL_DIR"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" | tar xz -C "$TMP"
install -m 755 "$TMP/kiro-cc-plugins" "$INSTALL_DIR/kiro-cc-plugins"

echo ""
echo "✓ Installed kiro-cc-plugins $VERSION to $INSTALL_DIR/kiro-cc-plugins"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "⚠  $INSTALL_DIR is not in your PATH. Add this to your shell profile:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac

echo ""
echo "Run: kiro-cc-plugins --help"
