#!/bin/sh
# Dribble installer.
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
# to ~/.dribble/characters on first run.
#
# Overrides:
#   REPO=owner/dribble  GitHub repo to download from
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
  macos) ASSET="dribble-$VERSION-$OS-$ARCH.tar.gz" ;;
  linux) ASSET="dribble-$VERSION-$OS-$ARCH.tar.gz" ;;
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
BIN="$TMP/dribble"
[ -f "$BIN" ] || die "archive did not contain the dribble binary"

# Stop any running instance first: replacing a binary that is currently
# executing poisons its code-signature validation on macOS (SIGKILLs).
pkill -f "dribble __daemon" 2>/dev/null || true
pkill -f "dribble __pet" 2>/dev/null || true
sleep 1

mkdir -p "$INSTALL_DIR"
rm -f "$INSTALL_DIR/dribble"
if cp "$BIN" "$INSTALL_DIR/dribble" 2>/dev/null; then
  :
else
  # Destination not user-writable: try /usr/local/bin with sudo.
  warn "$INSTALL_DIR not writable; trying /usr/local/bin with sudo"
  sudo mkdir -p /usr/local/bin
  sudo rm -f /usr/local/bin/dribble
  sudo cp "$BIN" /usr/local/bin/dribble
  INSTALL_DIR="/usr/local/bin"
fi
chmod +x "$INSTALL_DIR/dribble"
if command -v codesign >/dev/null 2>&1; then
  codesign -s - --force "$INSTALL_DIR/dribble" >/dev/null 2>&1 || true
fi
log "installed to $INSTALL_DIR/dribble"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) warn "$INSTALL_DIR is not on your PATH; add it with:
    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
esac

# ----------------------------------------------------------- health check
WR="$INSTALL_DIR/dribble"
"$WR" --version || die "health check failed"
# doctor validates the install and extracts the bundled character assets
"$WR" doctor >/dev/null 2>&1 || true
log "health check passed: $("$WR" --version | head -1)"

# ---------------------------------------------------- macOS: install as app
# Double-clickable "Dribble" in /Applications (agent app: starts
# the daemon quietly, no Dock icon). Spotlight finds it too.
if [ "$OS" = "macos" ]; then
  APP_PARENT="${APP_DIR:-/Applications}"
  [ -d "$APP_PARENT" ] && [ -w "$APP_PARENT" ] || APP_PARENT="$HOME/Applications"
  mkdir -p "$APP_PARENT" 2>/dev/null || APP_PARENT="$HOME/Applications"
  APP="$APP_PARENT/Dribble.app"
  rm -rf "$APP"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

  cat > "$APP/Contents/MacOS/WalkingReminder" <<LAUNCHER
#!/bin/sh
exec "$INSTALL_DIR/dribble" start
LAUNCHER
  chmod +x "$APP/Contents/MacOS/WalkingReminder"

  cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Dribble</string>
    <key>CFBundleDisplayName</key>
    <string>Dribble</string>
    <key>CFBundleIdentifier</key>
    <string>com.dribble.app</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>WalkingReminder</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
PLIST

  # Icon from the installed character's sprite (active character first,
  # then the bundled default). Skipped gracefully if tools are missing.
  SPRITE=""
  for cand in \
    "$HOME/.dribble/characters/$(python3 -c "import json;print(json.load(open('$HOME/.dribble/config.json')).get('character',''))" 2>/dev/null)/walk_01.png" \
    "$HOME/.dribble/characters/footballer/walk_01.png"; do
    [ -f "$cand" ] && SPRITE="$cand" && break
  done
  if [ -n "$SPRITE" ] && command -v iconutil >/dev/null 2>&1 && command -v sips >/dev/null 2>&1; then
    # 1024-quality icon: nearest-neighbor upscale keeps pixel art crisp.
    MASTER="$(mktemp -d)/master.png"
    python3 - "$SPRITE" "$MASTER" <<'PYEOF'
import struct, zlib, sys
src, dst = sys.argv[1], sys.argv[2]
data = open(src, 'rb').read()
pos = 8; idat = b''
while pos < len(data):
    ln = struct.unpack('>I', data[pos:pos+4])[0]; typ = data[pos+4:pos+8]
    if typ == b'IHDR': w, h = struct.unpack('>II', data[pos+8:pos+16])
    elif typ == b'IDAT': idat += data[pos+8:pos+8+ln]
    pos += ln + 12
raw = zlib.decompress(idat); stride = w * 4
prev = bytearray(stride); out = bytearray(); i = 0
for y in range(h):
    f = raw[i]; i += 1; line = bytearray(raw[i:i+stride]); i += stride
    if f == 1:
        for x in range(4, stride): line[x] = (line[x] + line[x-4]) & 255
    elif f == 2:
        for x in range(stride): line[x] = (line[x] + prev[x]) & 255
    elif f == 3:
        for x in range(stride): line[x] = (line[x] + ((line[x-4] if x >= 4 else 0) + prev[x]) // 2) & 255
    elif f == 4:
        for x in range(stride):
            a = line[x-4] if x >= 4 else 0; b = prev[x]; c = prev[x-4] if x >= 4 else 0
            p = a + b - c; pa, pb, pc = abs(p-a), abs(p-b), abs(p-c)
            pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
            line[x] = (line[x] + pr) & 255
    out += line; prev = line
S = 5; W, H = w * S, h * S
big = bytearray(W * H * 4)
for y in range(H):
    sy = y // S
    for x in range(W):
        so = (sy * w + x // S) * 4; do = (y * W + x) * 4
        big[do:do+4] = out[so:so+4]
rawo = bytearray()
for y in range(H):
    rawo.append(0); rawo += big[y*W*4:(y+1)*W*4]
def chunk(t, d):
    return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
png = (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', W, H, 8, 6, 0, 0, 0))
       + chunk(b'IDAT', zlib.compress(bytes(rawo), 9)) + chunk(b'IEND', b''))
open(dst, 'wb').write(png)
PYEOF
    ICONSET="$(mktemp -d)/icon.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 128 256 512; do
      sips -z "$size" "$size" "$MASTER" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null 2>&1
      sips -z $((size*2)) $((size*2)) "$MASTER" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null 2>&1
    done
    sips -z 1024 1024 "$MASTER" --out "$ICONSET/icon_1024x1024.png" >/dev/null 2>&1
    if iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns" >/dev/null 2>&1; then
      plutil -insert CFBundleIconFile -string "AppIcon" "$APP/Contents/Info.plist" >/dev/null 2>&1 || true
    fi
  fi
  log "app installed: $APP (double-click or Spotlight: \"Dribble\")"
fi

printf '\n\033[1mDribble is installed!\033[0m\n\n'
cat <<'EOF'
  dribble add "Drink water" --every 1h
  dribble test        # watch the character walk across your screen
  dribble start       # start the background daemon
  dribble enable      # start automatically at login
EOF
