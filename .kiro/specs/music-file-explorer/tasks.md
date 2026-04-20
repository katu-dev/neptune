# Implementation Plan: music-file-explorer - neptune

## Overview

Implement the Neptune Tauri desktop app incrementally: Rust backend first (DB, scanner, metadata, player, visualizations), then React/TypeScript frontend (layout, file explorer, metadata panel, playback controls, waveform, spectrogram). Each phase wires into the previous one so the app is always in a runnable state.

## Tasks

- [x] 1. Project scaffolding and core types
  - Run `cargo tauri init` / `npm create tauri-app` to generate the Tauri + React/TypeScript project skeleton
  - Add Rust dependencies to `Cargo.toml`: `tauri`, `tokio`, `rusqlite`, `lofty`, `symphonia` (all codecs), `cpal`, `proptest`, `thiserror`, `serde`, `walkdir`
  - Add frontend dependencies: `@fontsource/inter`, `zustand`, `react-window`
  - Define all Rust types in `src-tauri/src/types.rs`: `Track`, `DirNode`, `WaveformData`, `SpectrogramData`, `AppState`, `ScanResult`, `AppError`
  - _Requirements: 1.1, 2.1, 4.4, 5.1, 6.1_

- [x] 2. Database layer
  - [x] 2.1 Implement `src-tauri/src/db.rs` with SQLite initialization, schema creation (`tracks`, `app_state` tables), and CRUD functions
    - On first run, create the DB file in the app data directory
    - Implement `insert_track`, `update_track`, `get_all_tracks`, `mark_missing`, `get_app_state`, `save_app_state`, `reset_library`
    - _Requirements: 7.1, 7.2, 7.4, 7.5_

  - [ ]* 2.2 Write property test for library persistence round trip
    - **Property 16: Library persistence round trip**
    - **Validates: Requirements 7.2**

  - [ ]* 2.3 Write property test for app state persistence round trip
    - **Property 18: App state persistence round trip**
    - **Validates: Requirements 7.4**

- [x] 3. Scanner and indexer
  - [x] 3.1 Implement `src-tauri/src/scanner.rs` using `walkdir` + `tokio::spawn_blocking`
    - Filter files by Supported_Format extensions (mp3, flac, aac, ogg, wav, aiff, opus)
    - Emit `scan_progress` Tauri events during traversal
    - Call `MetadataExtractor` for each discovered file, then upsert via `db`
    - Mark tracks missing when files no longer exist on disk
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7_

  - [ ]* 3.2 Write property test for scanner file filtering
    - **Property 1: Scanner returns exactly supported-format files**
    - **Validates: Requirements 1.1, 1.4**

  - [ ]* 3.3 Write property test for scan-then-query round trip
    - **Property 2: Scan-then-query round trip**
    - **Validates: Requirements 1.3**

  - [ ]* 3.4 Write property test for missing file detection
    - **Property 3: Missing files are marked, present files are not**
    - **Validates: Requirements 1.5**

  - [ ]* 3.5 Write property test for rescan idempotency
    - **Property 4: Rescan is idempotent — no duplicates, new files added**
    - **Validates: Requirements 1.6**

  - [ ]* 3.6 Write property test for additive root directory
    - **Property 17: Adding a root directory is additive**
    - **Validates: Requirements 7.3**

- [x] 4. Metadata extractor
  - [x] 4.1 Implement `src-tauri/src/metadata.rs` using `lofty`
    - Extract title, artist, album, album_artist, year, genre, track_number, disc_number, duration
    - Extract embedded cover art and write to app cache dir; store path in track record
    - Return `null` for absent fields without failing
    - _Requirements: 2.1, 2.2, 2.3, 2.5_

  - [ ]* 4.2 Write property test for metadata extraction with partial tags
    - **Property 5: Metadata extraction round trip with partial tags**
    - **Validates: Requirements 2.1, 2.3**

  - [ ]* 4.3 Write property test for cover art extraction
    - **Property 6: Cover art round trip**
    - **Validates: Requirements 2.2**

  - [ ]* 4.4 Write property test for tag format support
    - **Property 7: Tag format support**
    - **Validates: Requirements 2.5**

- [x] 5. Path normalization utility
  - [x] 5.1 Implement `src-tauri/src/utils.rs` with a `normalize_path` function that converts any path string to use the OS-native separator
    - _Requirements: 8.3_

  - [ ]* 5.2 Write property test for path normalization
    - **Property 19: Path normalization uses OS-native separator**
    - **Validates: Requirements 8.3**

- [x] 6. Tauri commands — library and metadata
  - Implement `scan_directory`, `get_library`, `get_directory_tree`, `get_track_metadata`, `get_cover_art`, `reset_library` in `src-tauri/src/commands.rs`
  - Wire commands into `tauri::Builder` in `main.rs`
  - Use `tauri::AppHandle` to emit `scan_progress` events from the scanner
  - _Requirements: 1.1, 1.2, 1.3, 2.1, 2.2, 3.1, 3.2, 7.3_

- [x] 7. Checkpoint — backend library pipeline
  - Ensure all Rust tests pass (`cargo test`)
  - Verify `scan_directory` → DB → `get_library` round trip works end-to-end via a manual Tauri invoke
  - Ask the user if questions arise.

- [x] 8. Audio player
  - [x] 8.1 Implement `src-tauri/src/player.rs` using `symphonia` for decoding and `cpal` for output
    - Manage player state machine: stopped → playing → paused → stopped
    - Emit `playback_position` events (≤500ms interval) and `playback_state_changed` events
    - Support seek by flushing decode buffer and repositioning
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.7_

  - [x] 8.2 Implement Tauri commands: `play_track`, `pause`, `stop`, `seek`, `set_volume`, `play_next`, `play_previous`
    - Return `AppError::Decode` for undecodable files without crashing
    - Use `cpal` default device on all platforms
    - _Requirements: 4.1, 4.3, 4.5, 4.6, 8.2_

  - [ ]* 8.3 Write property test for playback format support
    - **Property 11: Playback succeeds for all supported formats**
    - **Validates: Requirements 4.4**

  - [ ]* 8.4 Write property test for decode error isolation
    - **Property 12: Decode errors are isolated**
    - **Validates: Requirements 4.5**

- [x] 9. Waveform generator
  - [x] 9.1 Implement `src-tauri/src/waveform.rs` using `symphonia` to decode audio and downsample to peak amplitudes
    - Accept a `width` parameter (pixel columns); return `samples_per_channel` of exactly that length
    - Emit `waveform_ready` event when done; implement `get_waveform` Tauri command
    - _Requirements: 5.1, 5.5_

  - [ ]* 9.2 Write property test for waveform output length
    - **Property 13: Waveform output length matches requested width**
    - **Validates: Requirements 5.1**

- [x] 10. Spectrogram generator
  - [x] 10.1 Implement `src-tauri/src/spectrogram.rs` using `symphonia` + FFT (e.g., `rustfft`)
    - Compute STFT with configurable `fft_size` (default 2048) and `hop_size` (default 512)
    - Return magnitudes in dB; dimensions must be `ceil((N-F)/H)` frames × `F/2+1` bins
    - Emit `spectrogram_ready` event; implement `get_spectrogram` Tauri command
    - _Requirements: 6.1, 6.5_

  - [ ]* 10.2 Write property test for spectrogram output dimensions
    - **Property 15: Spectrogram output dimensions match FFT parameters**
    - **Validates: Requirements 6.1**

- [x] 11. Checkpoint — full backend
  - Ensure all Rust tests pass (`cargo test`)
  - Ask the user if questions arise.

- [x] 12. Design tokens and global styles
  - Create `src/styles/tokens.css` with all CSS custom properties from the design (colors, typography)
  - Load `@fontsource/inter` in `main.tsx`
  - Create `src/styles/global.css` with base resets and font-family assignment
  - _Requirements: (visual design system)_

- [x] 13. App shell layout
  - Implement `src/components/Layout.tsx`: three-column layout (Sidebar 220px, FileExplorer flex-grow, MetadataPanel 280px), WaveformBar (120px), PlaybackBar (64px), TitleBar (40px)
  - Implement `src/components/Sidebar.tsx`: logo.svg + "Music Explorer" label, navigation items with active indicator
  - Implement `src/components/TitleBar.tsx`: search input placeholder and settings icon
  - _Requirements: 3.1, 8.4_

- [x] 14. Zustand store and Tauri event listeners
  - Create `src/store/index.ts` with Zustand store holding: `tracks`, `selectedTrackId`, `playbackState`, `playbackPosition`, `volume`, `searchQuery`
  - Register Tauri event listeners for `scan_progress`, `playback_position`, `playback_state_changed`, `waveform_ready`, `spectrogram_ready`
  - _Requirements: 1.2, 4.2, 5.3_

- [x] 15. File Explorer component
  - [x] 15.1 Implement `src/components/FileExplorer.tsx`
    - Render directory tree with expandable/collapsible folder nodes using `react-window` for virtualization
    - Keyboard navigation: arrow keys move focus, Enter selects track
    - On track selection invoke `get_track_metadata` and update store
    - Show missing-track indicator (warning icon + muted text) for `missing = true` tracks
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

  - [x] 15.2 Implement search filtering in FileExplorer
    - Filter visible tracks by title/artist/album against `searchQuery` from store (case-insensitive, client-side)
    - _Requirements: 3.6, 3.7_

  - [ ]* 15.3 Write property test for search filter correctness
    - **Property 10: Search filter is complete and case-insensitive**
    - **Validates: Requirements 3.6, 3.7**

  - [ ]* 15.4 Write property test for folder expansion completeness
    - **Property 8: Folder expansion shows all children**
    - **Validates: Requirements 3.2**

  - [ ]* 15.5 Write property test for track selection metadata correctness
    - **Property 9: Track selection loads correct metadata**
    - **Validates: Requirements 3.3**

- [x] 16. Metadata Panel component
  - Implement `src/components/MetadataPanel.tsx`
  - Display cover art (from `get_cover_art`), all tag fields, file path in monospace
  - Show within 100ms of track selection (invoke on selection event)
  - _Requirements: 2.1, 2.2, 2.4_

- [x] 17. Playback Controls component
  - Implement `src/components/PlaybackControls.tsx`
  - Play/pause/stop buttons invoke corresponding Tauri commands
  - Seek slider: position from `playback_position` events; click/drag invokes `seek`
  - Volume slider invokes `set_volume`
  - Skip next/previous buttons invoke `play_next` / `play_previous`
  - Display current position and duration formatted as `mm:ss`
  - _Requirements: 4.1, 4.2, 4.3, 4.6_

- [x] 18. Waveform View component
  - Implement `src/components/WaveformView.tsx`
  - On track selection call `get_waveform`; render amplitude bars on HTML Canvas using design color tokens
  - Show loading indicator while computing
  - Advance playback cursor via `playback_position` events
  - Click handler: compute normalized position, invoke `seek(p * duration_secs)`
  - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.6_

  - [ ]* 18.1 Write property test for click-to-seek position mapping (waveform)
    - **Property 14: Click-to-seek position mapping**
    - **Validates: Requirements 5.4, 6.4**

- [x] 19. Spectrogram View component
  - Implement `src/components/SpectrogramView.tsx`
  - On activation call `get_spectrogram`; render time-frequency heatmap on Canvas using inferno color map
  - Show loading indicator while computing
  - Click handler: compute normalized position, invoke `seek(p * duration_secs)`
  - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6_

- [x] 20. Error handling and DB corruption modal
  - Implement toast notification component for `AppError::Io` and `AppError::UnsupportedFormat`
  - Implement modal for `AppError::Database` with "Reset Library" / "Cancel" actions (calls `reset_library`)
  - Show inline error badge on track rows for `AppError::Decode`
  - _Requirements: 4.5, 7.5_

- [x] 21. App state persistence and restore on launch
  - On app startup: call `get_app_state`, restore `last_track_id`, `last_position_secs`, `volume` in store
  - On app close / track change: call `save_app_state`
  - Implement native directory picker via Tauri dialog plugin for "Add folder" action
  - _Requirements: 7.4, 8.4_

- [x] 22. Final checkpoint — full integration
  - Ensure all Rust tests pass (`cargo test`)
  - Ensure all frontend tests pass
  - Verify the full user flow: add folder → scan → browse → select track → play → seek via waveform → spectrogram view
  - Ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Property tests use `proptest` in Rust; audio-processing properties use synthetic PCM buffers for speed
- Each property test file must include a comment: `// Feature: Neptune, Property N: <title>`
- Checkpoints ensure the app is always in a runnable state before moving to the next phase
