# Neptune

A cross-platform desktop music file explorer and player built with Tauri, React, and Rust.

## Features

- **Library scanning** — add local folders and Neptune scans them recursively for audio files
- **File explorer** — browse your library in a folder tree with keyboard navigation
- **Search & filter** — filter tracks by title, artist, or album; filter by tags
- **Tags** — create color-coded tags, assign them to tracks, and filter your library by tag
- **Playback** — play, pause, seek, and control volume
- **Waveform view** — visual waveform display for the current track
- **Spectrogram** — static and live spectrogram visualization
- **Spectrum analyzer** — real-time frequency spectrum with VU meter
- **Metadata panel** — view track metadata and cover art; assign/remove tags
- **Persistent state** — last played track, volume, and library folders are restored on relaunch

## Tech Stack

| Layer | Technology |
|---|---|
| UI | React 18, TypeScript, Zustand |
| Desktop shell | Tauri 2 |
| Audio decoding | Symphonia |
| Metadata reading | Lofty |
| Database | SQLite (rusqlite, bundled) |
| Audio output | CPAL |
| FFT | RustFFT |

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (stable toolchain)
- Tauri prerequisites for your OS — see the [Tauri docs](https://tauri.app/start/prerequisites/)

## Getting Started

```bash
# Install JS dependencies
npm install

# Run in development mode (hot-reload)
npm run tauri dev

# Build a release bundle
npm run tauri build
```

The compiled app will be in `src-tauri/target/release/bundle/`.

## Project Structure

```
├── src/                  # React frontend
│   ├── components/       # UI components
│   ├── store/            # Zustand store
│   └── styles/           # Global CSS and design tokens
└── src-tauri/            # Rust backend
    └── src/
        ├── commands.rs   # Tauri command handlers
        ├── db.rs         # SQLite schema and queries
        ├── player.rs     # Audio playback
        ├── scanner.rs    # Directory scanner
        ├── metadata.rs   # Tag/metadata reading
        ├── waveform.rs   # Waveform generation
        └── spectrogram.rs# Spectrogram computation
```
