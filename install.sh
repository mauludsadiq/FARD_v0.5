#!/bin/sh
# FARD installer — detects platform and installs all binaries to /usr/local/bin
set -e

REPO="mauludsadiq/FARD"
BASE="https://github.com/${REPO}/releases/latest/download"
INSTALL_DIR="${FARD_INSTALL_DIR:-/usr/local/bin}"

# Detect platform
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64) SUFFIX="macos-aarch64" ;;
      x86_64) SUFFIX="macos-x86_64" ;;
      *) echo "Unsupported arch: $ARCH"; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64) SUFFIX="linux-x86_64" ;;
      *) echo "Unsupported arch: $ARCH"; exit 1 ;;
    esac
    ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

ARCHIVE="fard-${SUFFIX}.tar.gz"
URL="${BASE}/${ARCHIVE}"

echo "Installing FARD for ${OS}/${ARCH}..."
echo "Downloading ${URL}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP/$ARCHIVE"
tar xzf "$TMP/$ARCHIVE" -C "$TMP"

DIST="$TMP/fard-${SUFFIX}"
INSTALLED=0

for bin in "$DIST"/fard* "$DIST"/fard-lsp "$DIST"/fard-build; do
  [ -f "$bin" ] || continue
  name=$(basename "$bin")
  dest="$INSTALL_DIR/$name"
  if [ -w "$INSTALL_DIR" ]; then
    cp "$bin" "$dest"
    chmod +x "$dest"
  else
    sudo cp "$bin" "$dest"
    sudo chmod +x "$dest"
  fi
  INSTALLED=$((INSTALLED + 1))
done

echo ""
echo "Installed $INSTALLED FARD binaries to $INSTALL_DIR"
echo ""
fardrun --version 2>/dev/null && echo "fardrun ok" || echo "fardrun installed (restart shell to use)"
echo ""
echo "Get started:"
echo "  fardrun new my-project"
echo "  cd my-project"
echo "  fardrun run --program main.fard --out ./out"
