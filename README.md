# GPP

GPU-accelerated video player written in Rust, using [GPUI](https://www.gpui.rs/) for the UI and [GStreamer](https://gstreamer.freedesktop.org/) (via `gpui-video-player`) for playback.

## Features

- Local files and HTTP(S) streams
- Drag a file or folder onto the window (folders are scanned for videos)
- YouTube-style overlay: SVG icons, red progress bar, hover-to-scrub knob, expanding volume slider
- Play / pause, skip 5s, volume, mute, loop, playback speed
- Playlist next
- Embedded and sidecar subtitle tracks (SRT / ASS / VTT), rendered with libass via GStreamer `assrender`, cycled with `C` or the CC button
- Fullscreen
- Auto-hiding controls while a video is playing
- Global settings overlay for autoplay, volume, speed, and subtitle defaults

## Requirements

- Rust 1.85+ (edition 2024)
- **Xcode** (GPUI compiles Metal shaders; Command Line Tools alone is not enough). If `xcrun metal` is missing, install Xcode and run `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`. Alternatively set `gpui = { version = "0.2.2", features = ["macos-blade"] }` in `Cargo.toml`.
- GStreamer 1.14+ with the base / good plugins (bad / libav recommended for extra codecs)

### macOS

Install the official runtime from [gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/#macos) so this path exists:

```text
/Library/Frameworks/GStreamer.framework
```

The project includes stub pkg-config files so a runtime-only framework install can still link. If you already have GStreamer from Homebrew, that works too:

```bash
brew install gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
```

### Linux

```bash
# Debian / Ubuntu
sudo apt install \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav

# Fedora
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel \
  gstreamer1-plugins-good gstreamer1-plugins-bad-free
```

## Run

```bash
cargo run --release
cargo run --release -- /path/to/movie.mp4
cargo run --release -- https://example.com/stream.m3u8
```

## Package (macOS)

```bash
./scripts/package-macos.sh
open dist/GPP.app
# or install for Finder “Open With”:
./scripts/package-macos.sh --install
```

This builds `dist/GPP.app`, compiles `assets/app-icon.png` into `AppIcon.icns`, and registers the playable video extensions (mp4, mkv, webm, mov, and the rest of the list in `src/util.rs`). GPP still uses the system GStreamer runtime at `/Library/Frameworks/GStreamer.framework`.

## Keyboard

| Key | Action |
| --- | --- |
| Space / K | Play / pause |
| ← / J | Seek back 5s |
| → / L | Seek forward 5s |
| Shift+← / Shift+→ | Seek 15s |
| ↑ / ↓ | Volume |
| M | Mute |
| R | Loop |
| S | Cycle speed |
| F | Fullscreen |
| N / P | Next / previous |
| Home / 0 | Restart |
| ⌘O / Ctrl+O | Open file |
| Double-click video | Fullscreen |

## Architecture

- `src/player.rs` — GPUI view, controls, playlist
- `src/theme.rs` — colors
- `gpui-video-player` — GStreamer `playbin` + NV12 frames rendered by GPUI (`CVPixelBuffer` on macOS)
