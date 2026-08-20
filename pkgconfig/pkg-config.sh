#!/bin/sh
# Wrapper used by Cargo to locate GStreamer.
#
# macOS official installer ships a runtime framework without pkgconfig files.
# We inject stub .pc files from this directory so gstreamer-sys can link.
# Linux / Homebrew installs keep using the real pkg-config on PATH.

set -e

HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
FRAMEWORK="/Library/Frameworks/GStreamer.framework/Versions/1.0"
FRAMEWORK_PKG="$FRAMEWORK/bin/pkg-config"

if [ -x "$FRAMEWORK_PKG" ]; then
    PKG_CONFIG_PATH="$HERE${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export PKG_CONFIG_PATH
    exec "$FRAMEWORK_PKG" "$@"
fi

if command -v pkg-config >/dev/null 2>&1; then
    exec pkg-config "$@"
fi

if command -v pkgconf >/dev/null 2>&1; then
    exec pkgconf "$@"
fi

echo "pkg-config not found. Install GStreamer development files." >&2
exit 1
