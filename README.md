# GPP

GPU-accelerated video player written in Rust, using [GPUI](https://www.gpui.rs/) for the UI and [GStreamer](https://gstreamer.freedesktop.org/) (via `gpui-video-player`) for playback.

GPP currently targets macOS for packaged desktop releases. The Rust application
can also be built on Linux when the required GStreamer development packages are
available.

## Features

- Local files and HTTP(S) streams
- Drag a file or folder onto the window (folders are scanned for videos)
- YouTube-style overlay: SVG icons, red progress bar, hover-to-scrub knob, expanding volume slider
- Play / pause, skip 5s, volume, mute, loop, playback speed
- Playlist next
- Embedded and sidecar subtitle tracks (SRT / ASS / VTT), rendered with libass via GStreamer `assrender`, cycled with `C` or the CC button
- Danmaku (Bilibili XML / JSON) drawn with system text so emoji works, sitting above the video but below GPUI toasts and out of the subtitle band; toggle with `D`
- Fullscreen
- Auto-hiding controls while a video is playing
- Global settings overlay for autoplay, volume, speed, and subtitle defaults

## Requirements

- Rust 1.85+ (edition 2024)
- **Xcode** (GPUI compiles Metal shaders; Command Line Tools alone is not
  enough). Select it with
  `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`. If
  `xcrun metal --version` reports a missing Metal Toolchain, run
  `xcodebuild -downloadComponent MetalToolchain`.
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

## Install

Download the macOS zip from the repository's Releases page, extract it, and move
`GPP.app` to `Applications`. The published app is ad-hoc signed rather than Apple
notarized, so macOS may require you to control-click the app and choose **Open**
the first time.

Install the official GStreamer runtime from
[gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/#macos)
before launching GPP. The runtime is shared and is not embedded in the app zip.

## Run from source

```bash
cargo run --release
cargo run --release -- /path/to/movie.mp4
cargo run --release -- https://example.com/stream.m3u8
```

## Package (macOS)

```bash
./scripts/package-macos.sh --zip
open dist/GPP.app
# or install for Finder “Open With”:
./scripts/package-macos.sh --install
```

This builds `dist/GPP.app` and a versioned
`dist/GPP-<version>-macOS-<architecture>.zip`, compiles `assets/app-icon.png`
into `AppIcon.icns`, and registers the playable video extensions (mp4, mkv,
webm, mov, and the rest of the list in `src/util.rs`). GPP uses the shared
GStreamer runtime at `/Library/Frameworks/GStreamer.framework`. A matching
`.sha256` checksum is generated beside the archive.

## Continuous integration and releases

GitHub Actions checks formatting, runs Clippy and the test suite, builds the
macOS app for Apple Silicon and Intel, and uploads both zips on pushes and pull
requests. Pushing a tag that matches the Cargo version (for example, `v0.1.0`)
also publishes those archives as a GitHub Release. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the release checklist.

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
| C | Cycle subtitles |
| D | Toggle danmaku |
| F | Fullscreen |
| N / P | Next / previous |
| Home / 0 | Restart |
| ⌘O / Ctrl+O | Open file |
| Double-click video | Fullscreen |

## Architecture

- `src/player.rs` — GPUI view, controls, playlist
- `src/theme.rs` — colors
- `gpui-video-player` — GStreamer `playbin` + NV12 frames rendered by GPUI (`CVPixelBuffer` on macOS)

## License

The GPP application is dual-licensed under either
[Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
The bundled `crates/gpui-video-player` crate retains its original MIT license.
