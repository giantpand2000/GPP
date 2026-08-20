#!/usr/bin/env bash
# Build a double-clickable GPP.app for macOS, with video file associations.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

INSTALL=0
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/package-macos.sh [--install]

Builds dist/GPP.app from a release binary, writes Info.plist so Finder
can Open With the playable video types, and compiles assets/app-icon.png
into AppIcon.icns.

  --install   Copy the app to /Applications/GPP.app
EOF
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "this script only packages a Mac .app" >&2
  exit 1
fi

VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
if [[ -z "$VERSION" ]]; then
  echo "could not read version from Cargo.toml" >&2
  exit 1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/release/gpp"
DIST="$ROOT/dist"
APP="$DIST/GPP.app"
CONTENTS="$APP/Contents"
ICON_SRC="$ROOT/assets/app-icon.png"
PLIST_SRC="$ROOT/packaging/macos/Info.plist"

if [[ ! -f "$ICON_SRC" ]]; then
  echo "missing app icon: $ICON_SRC" >&2
  exit 1
fi
if [[ ! -f "$PLIST_SRC" ]]; then
  echo "missing Info.plist template: $PLIST_SRC" >&2
  exit 1
fi

echo "==> building gpp $VERSION (release)"
cargo build --release --locked --bin gpp

if [[ ! -x "$BIN" ]]; then
  echo "release binary not found at $BIN" >&2
  exit 1
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$BIN" "$CONTENTS/MacOS/gpp"
chmod +x "$CONTENTS/MacOS/gpp"
printf 'APPL????' > "$CONTENTS/PkgInfo"

sed "s/__VERSION__/${VERSION}/g" "$PLIST_SRC" > "$CONTENTS/Info.plist"

echo "==> compiling AppIcon.icns"
ICONSET="$(mktemp -d "${TMPDIR:-/tmp}/gpp-iconset.XXXXXX")"
cleanup() { rm -rf "$ICONSET"; }
trap cleanup EXIT
mkdir -p "$ICONSET/AppIcon.iconset"

# iconutil expects this exact set of names/sizes.
while IFS=' ' read -r name px; do
  sips -z "$px" "$px" "$ICON_SRC" --out "$ICONSET/AppIcon.iconset/${name}.png" >/dev/null
done <<'SIZES'
icon_16x16 16
icon_16x16@2x 32
icon_32x32 32
icon_32x32@2x 64
icon_128x128 128
icon_128x128@2x 256
icon_256x256 256
icon_256x256@2x 512
icon_512x512 512
icon_512x512@2x 1024
SIZES

iconutil -c icns "$ICONSET/AppIcon.iconset" -o "$CONTENTS/Resources/AppIcon.icns"

echo "==> ad-hoc codesign"
codesign --force --deep --sign - "$APP" >/dev/null

LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [[ -x "$LSREGISTER" ]]; then
  "$LSREGISTER" -f "$APP" >/dev/null 2>&1 || true
fi

if [[ ! -d /Library/Frameworks/GStreamer.framework ]]; then
  echo "warning: /Library/Frameworks/GStreamer.framework is missing." >&2
  echo "         GPP.app needs the official GStreamer runtime to play video." >&2
fi

echo
echo "built $APP"
echo "open with:  open \"$APP\""
echo "or:         open -a \"$APP\" /path/to/video.mp4"

if [[ "$INSTALL" -eq 1 ]]; then
  DEST="/Applications/GPP.app"
  echo "==> installing $DEST"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
  echo "installed. Finder Open With should list GPP after a refresh or logout."
fi
