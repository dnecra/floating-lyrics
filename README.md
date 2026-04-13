# Floating Lyrics

Windows desktop app for showing synced lyrics in a transparent, always-on-top floating window.

This repo is built with Rust + Tauri 2 and is designed around three runtime modes:

- `standalone`: runs against a local embedded lyrics server on `127.0.0.1:1312`
- `ytm`: runs against a local embedded YTM server on `127.0.0.1:1312`
- `serverless`: connects to a remote lyrics endpoint and exposes a small local control API on `127.0.0.1:32145`

## Features

- Transparent overlay window with no standard window chrome
- Tray-based controls for monitor selection and playback behavior
- Click-through support for non-interactive overlay mode
- Hover-hide behavior with automatic large-layout detection
- Mini-window mode for a regular movable window experience
- Per-mode persisted settings
- Standalone server bootstrap and update flow from GitHub releases
- Deep-link support for packaged builds

## Runtime Modes

### Standalone

The standalone build is the default desktop app variant.

- Connects to `http://127.0.0.1:1312/lyrics`
- Tray guide shortcut opens `http://localhost:1312/welcome` in the browser
- Downloads and manages the bundled lyrics server executable automatically
- Checks the latest server release from `dnecra/lyrics-server`

### YTM

The `ytm` build behaves like standalone, but targets the YTM executable on the same GitHub release.

- Connects to `http://127.0.0.1:1312/lyrics`
- Downloads and manages `lyrics-ytm-x64.exe` directly from the latest release assets
- Uses the product name `Floating Lyrics YTM`
- Checks the latest server release from `dnecra/lyrics-server`

### Serverless

The serverless build targets an externally hosted lyrics source.

- Tries `http://192.168.99.47/lyrics`
- Falls back to `http://192.168.0.101/lyrics`
- Runs a local helper API on `127.0.0.1:32145`
- Exposes `GET /floating-lyrics/status`
- Exposes `POST /floating-lyrics/toggle`

## Tech Stack

- Rust 2021
- Tauri 2
- WebView2 on Windows
- Win32 APIs for window behavior, transparency, and topmost handling

## Requirements

This project is Windows-oriented. For local development and packaging, install:

- Rust toolchain
- Tauri CLI
- Microsoft Visual Studio Build Tools with MSVC C++ toolchain
- WebView2 Runtime

## Development

The repo uses `tauri.cmd` to switch the active entrypoint by copying one of `src/standalone.rs`, `src/ytm.rs`, or `src/serverless.rs` into `src/main.rs` before running Tauri commands.

### Run standalone

```bat
tauri.cmd dev standalone
```

### Run ytm

```bat
tauri.cmd dev ytm
```

### Run serverless

```bat
tauri.cmd dev serverless
```

### Watch mode

```bat
tauri.cmd dev-watch standalone
tauri.cmd dev-watch ytm
tauri.cmd dev-watch serverless
```

If `cargo-watch` is not installed, the script falls back to normal `cargo tauri dev`.

## Build

### Default build

```bat
build.bat
```

Equivalent to:

```bat
tauri.cmd build standalone x64
```

### Other build targets

```bat
tauri.cmd build standalone x64
tauri.cmd build ytm x64
tauri.cmd build serverless x64
tauri.cmd build all x64
```

Packaged installers are copied into [`nsis`](./nsis).

## Tray Controls

The system tray menu currently includes:

- Local server IP shortcut
- Mini-window mode toggle
- Monitor selection
- Always on top
- Disable hide on hover
- Fancy animations
- Blur Effect
- Pause lyrics
- Open guide
- Restart app
- Quit

Standalone builds also surface server update status/actions in the tray.

## Settings

Settings are persisted per mode under the app data directory:

- `settings.normal.json`
- `settings.window.json`

Tracked settings include:

- selected monitor
- click-through
- always-on-top
- hover-hide preference
- animation toggle
- blur toggle
- mini-window bounds

## Repo Layout

- [`src`](./src): Rust app runtime and platform modules
- [`html`](./html): minimal web frontend entry
- [`scripts`](./scripts): injected browser-side behavior tweaks
- [`icons`](./icons): app icons
- [`windows`](./windows): NSIS installer hooks
- [`capabilities`](./capabilities): Tauri capability config

## Notes

- The app is tuned primarily for Windows behavior.
- `src/main.rs` is generated/swapped by `tauri.cmd`; avoid treating it as the canonical source of truth.
- The standalone updater expects a `.7z` asset with a GitHub-provided `sha256:` digest on the latest release.
- The `ytm` updater expects a `lyrics-ytm-x64.exe` asset with a GitHub-provided `sha256:` digest on the latest release.
