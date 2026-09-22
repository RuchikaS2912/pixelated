#!/bin/sh
# Walking Reminder installer.
#
# Usage:
#   curl -fsSL https://your-domain.com/install.sh | sh
#
# Safer (inspect first):
#   curl -fsSL https://your-domain.com/install.sh -o install.sh
#   less install.sh && sh install.sh
#
# Steps: detect OS + arch -> download release artifact over HTTPS ->
# verify SHA256 -> install binary -> health check. The binary is
# self-contained: default character assets are embedded and extracted
# to ~/.walking-reminder/characters on first run.
#
# Overrides:
#   REPO=owner/walking-reminder  GitHub repo to download from
#   VERSION=v0.1.0               specific release tag (default: latest)
#   INSTALL_DIR=~/.local/bin     install destination

set -eu

REPO="${REPO:-RuchikaS2912/pixelated}"
VERSION="${VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

log()  { printf '\033[32m✓\033[0m %s\n' "$1"; }
warn() { printf '\033[33m!\033[0m %s\n' "$1"; }
die()  { printf '\033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }

# ---------------------------------------------------------------- OS/arch
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Darwin) OS="macos" ;;
  Linux)  OS="linux" ;;
  *) die "unsupported OS: $OS (macOS and Linux are supported; Windows: use the release .zip)" ;;
esac
case "$ARCH" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64|amd64)  ARCH="x64" ;;
  *) die "unsupported architecture: $ARCH" ;;
esac
log "detected $OS/$ARCH"

# --------------------------------------------------------------- download
if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  die "need curl or wget to download"
fi

if [ "$VERSION" = "latest" ]; then
  VERSION="$($FETCH "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
  [ -n "$VERSION" ] || die "could not determine latest release (repo $REPO)"
fi
log "release $VERSION"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

BASE="${BASE:-https://github.com/$REPO/releases/download/$VERSION}"
case "$OS" in
  macos) ASSET="walking-reminder-$VERSION-$OS-$ARCH.tar.gz" ;;
  linux) ASSET="walking-reminder-$VERSION-$OS-$ARCH.tar.gz" ;;
esac

$FETCH -o "$TMP/$ASSET" "$BASE/$ASSET" || die "download failed: $BASE/$ASSET"
$FETCH -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS" || die "checksums download failed"

# --------------------------------------------------------------- verify
if command -v shasum >/dev/null 2>&1; then
  SHA_CMD="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then
  SHA_CMD="sha256sum"
else
  die "no sha256 tool found (install shasum or sha256sum)"
fi
want="$(grep " $ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}')"
[ -n "$want" ] || die "no checksum entry for $ASSET"
got="$($SHA_CMD "$TMP/$ASSET" | awk '{print $1}')"
if [ "$want" != "$got" ]; then
  die "checksum mismatch!\n  expected $want\n  got      $got"
fi
log "checksum verified"

# ---------------------------------------------------------------- install
tar -xzf "$TMP/$ASSET" -C "$TMP" --strip-components=1
BIN="$TMP/walking-reminder"
[ -f "$BIN" ] || die "archive did not contain the walking-reminder binary"

mkdir -p "$INSTALL_DIR"
if cp "$BIN" "$INSTALL_DIR/walking-reminder" 2>/dev/null; then
  :
else
  # Destination not user-writable: try /usr/local/bin with sudo.
  warn "$INSTALL_DIR not writable; trying /usr/local/bin with sudo"
  sudo mkdir -p /usr/local/bin
  sudo cp "$BIN" /usr/local/bin/walking-reminder
  INSTALL_DIR="/usr/local/bin"
fi
chmod +x "$INSTALL_DIR/walking-reminder"
log "installed to $INSTALL_DIR/walking-reminder"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) warn "$INSTALL_DIR is not on your PATH; add it with:
    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
esac

# ----------------------------------------------------------- health check
WR="$INSTALL_DIR/walking-reminder"
"$WR" --version || die "health check failed"
log "health check passed: $("$WR" --version | head -1)"

printf '\n\033[1mWalking Reminder is installed!\033[0m\n\n'
cat <<'EOF'
  walking-reminder add "Drink water" --every 1h
  walking-reminder test        # watch the character walk across your screen
  walking-reminder start       # start the background daemon
  walking-reminder enable      # start automatically at login
EOF
