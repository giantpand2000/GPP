#!/usr/bin/env bash
# Build a double-clickable GPP.app for macOS, with video file associations.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

INSTALL=0
CREATE_ZIP=0
for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    --zip) CREATE_ZIP=1 ;;
    -h|--help)
      cat <<'EOF'
Usage: scripts/package-macos.sh [--install] [--zip]

Builds dist/GPP.app from a release binary, writes Info.plist so Finder
can Open With the playable video types, and compiles assets/app-icon.png
into AppIcon.icns.

  --install   Copy the app to /Applications/GPP.app
  --zip       Also create a versioned, architecture-specific zip archive
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
ARCH="$(uname -m)"
ZIP="$DIST/GPP-${VERSION}-macOS-${ARCH}.zip"
CHECKSUM="$ZIP.sha256"
CONTENTS="$APP/Contents"
ICON_SRC="$ROOT/assets/app-icon.png"
ALPHA_SCRIPT="$ROOT/scripts/png-with-alpha.swift"
PLIST_SRC="$ROOT/packaging/macos/Info.plist"

if [[ ! -f "$ICON_SRC" ]]; then
  echo "missing app icon: $ICON_SRC" >&2
  exit 1
fi
if [[ ! -f "$PLIST_SRC" ]]; then
  echo "missing Info.plist template: $PLIST_SRC" >&2
  exit 1
fi
if [[ ! -f "$ALPHA_SCRIPT" ]]; then
  echo "missing PNG conversion helper: $ALPHA_SCRIPT" >&2
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

# iconutil rejects opaque RGB PNGs. Normalize the source to sRGB RGBA first.
ALPHA_ICON="$ICONSET/app-icon-rgba.png"
swift -module-cache-path "$ICONSET/ModuleCache" "$ALPHA_SCRIPT" "$ICON_SRC" "$ALPHA_ICON"

# iconutil expects this exact set of names/sizes.
while IFS=' ' read -r name px; do
  sips -z "$px" "$px" "$ALPHA_ICON" --out "$ICONSET/AppIcon.iconset/${name}.png" >/dev/null
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

if ! iconutil -c icns "$ICONSET/AppIcon.iconset" -o "$CONTENTS/Resources/AppIcon.icns"; then
  echo "warning: iconutil rejected the iconset; falling back to tiff2icns" >&2
  mkdir -p "$ICONSET/AppIcon.tiffset"
  TIFFS=()
  for px in 16 32 64 128 256 512 1024; do
    tiff="$ICONSET/AppIcon.tiffset/icon-${px}.tiff"
    sips -z "$px" "$px" -s format tiff "$ALPHA_ICON" --out "$tiff" >/dev/null
    TIFFS+=("$tiff")
  done
  tiffutil -cat "${TIFFS[@]}" -out "$ICONSET/AppIcon.tiff" >/dev/null
  tiff2icns "$ICONSET/AppIcon.tiff" "$CONTENTS/Resources/AppIcon.icns"
fi

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

if [[ "$CREATE_ZIP" -eq 1 ]]; then
  echo "==> creating $ZIP"
  rm -f "$ZIP"
  # ditto preserves the bundle's resource forks, permissions, and symlinks.
  ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"
  (
    cd "$DIST"
    shasum -a 256 "$(basename "$ZIP")" > "$(basename "$CHECKSUM")"
  )
  echo "archive:    $ZIP"
  echo "checksum:   $CHECKSUM"
fi

if [[ "$INSTALL" -eq 1 ]]; then
  DEST="/Applications/GPP.app"
  echo "==> installing $DEST"
  rm -rf "$DEST"
  cp -R "$APP" "$DEST"
  echo "installed. Finder Open With should list GPP after a refresh or logout."
fi
