# Implementation Plan: Neptune Feature Expansion

## Overview

Implement fourteen new features for Neptune across four themes: Library Intelligence, Playback Enhancement, UI/UX Enrichment, and System Integration. Tasks are ordered by dependency — foundational infrastructure (DB schema, Cargo/npm deps, type updates) comes first, then DSP/backend modules, then frontend components, then wiring and integration.

All Rust code goes in `src-tauri/src/`. All React code goes in `src/components/` and `src/store/`. New Tauri commands are registered in `src-tauri/src/lib.rs`.

## Tasks

- [x] 1. Add dependencies and run DB schema migrations
  - Add `notify = "6"`, `discord-rich-presence = "2"`, `rayon = "1"` to `[dependencies]` in `src-tauri/Cargo.toml`
  - Add `proptest = "1"` to `[dev-dependencies]` in `src-tauri/Cargo.toml` (if not already present)
  - Add `"@dnd-kit/core": "^6"`, `"@dnd-kit/sortable": "^8"`, `"fuse.js": "^7"` to `package.json` and run `npm install`
  - In `db.rs` `create_schema`, add migration logic: `ALTER TABLE tracks ADD COLUMN bpm REAL` (guarded by checking if column exists), and `CREATE TABLE IF NOT EXISTS queue (...)` and `CREATE TABLE IF NOT EXISTS queue_state (...)` as specified in the design
  - _Requirements: 2.9, 6.5, 7.6, 8.5, 10.3_

- [x] 2. Update `Track` type in Rust and TypeScript
  - In `src-tauri/src/types.rs`, add `pub bpm: Option<f32>` to the `Track` struct
  - Update all `query_map` closures in `db.rs` that construct `Track` to read the new `bpm` column
  - In `src/store/index.ts`, add `bpm: number | null` to the `Track` interface
  - _Requirements: 10.3_

- [x] 3. Implement `watcher.rs` — Folder Watching
  - Create `src-tauri/src/watcher.rs` with the `Watcher` struct wrapping `notify::RecommendedWatcher`
  - Implement `Watcher::start(app_handle)`: reads `root_directories` from DB, registers each with 500 ms debounce
  - Implement event handler: `Create`/`Rename(to)` → `metadata::extract_metadata` + `db::insert_track`/`update_track` + emit `library_changed`; `Remove`/`Rename(from)` → `db::mark_missing(true)` + emit `library_changed`; `Modify` → `metadata::extract_metadata` + `db::update_track` + emit `library_changed`; errors → `eprintln!` and continue
  - Implement `add_directory` and `remove_directory` methods
  - Register `Watcher` as Tauri managed state in `lib.rs`; add `add_watch_directory` and `remove_watch_directory` Tauri commands
  - Wire `scan_directory` command to call `watcher.add_directory` after persisting the root
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8_

- [x] 4. Implement `queue.rs` — Queue Manager
  - Create `src-tauri/src/queue.rs` with `QueueManager` and `QueueState` structs as specified in the design
  - Implement `new`, `load_from_db`, `add_to_end`, `add_next`, `remove`, `move_track`, `clear`, `shuffle_after_current`, `advance`, `state`, `persist`, and `emit_changed`
  - `persist` writes to the `queue` and `queue_state` tables; `emit_changed` emits the `queue_changed` event
  - Register `QueueManager` as Tauri managed state in `lib.rs`; add all queue Tauri commands: `queue_add`, `queue_add_next`, `queue_remove`, `queue_move`, `queue_clear`, `queue_shuffle`, `get_queue`
  - Wire the player loop to call `queue_manager.advance()` when a track ends
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9, 2.10, 2.11_

  - [ ]* 4.1 Write property test for queue append (Property 1)
    - **Property 1: Queue append places track at end**
    - **Validates: Requirements 2.2**

  - [ ]* 4.2 Write property test for queue play-next insertion (Property 2)
    - **Property 2: Queue play-next inserts after current position**
    - **Validates: Requirements 2.3**

  - [ ]* 4.3 Write property test for queue mutation element set (Property 3)
    - **Property 3: Queue mutation preserves element set**
    - **Validates: Requirements 2.4, 2.5**

  - [ ]* 4.4 Write property test for queue persistence round-trip (Property 4)
    - **Property 4: Queue persistence round-trip**
    - **Validates: Requirements 2.9**

  - [ ]* 4.5 Write property test for queue shuffle permutation (Property 5)
    - **Property 5: Queue shuffle is a permutation**
    - **Validates: Requirements 2.11**

- [x] 5. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 6. Implement `eq.rs` — 8-Band Equalizer DSP
  - Create `src-tauri/src/eq.rs` with `Equalizer` and `BiquadFilter` structs
  - Implement `new(sample_rate)`: initializes 8 bands at the specified center frequencies with 0 dB gain
  - Implement `set_gain(band, gain_db)` and `recompute_coefficients(band)` using the Audio EQ Cookbook peaking EQ formula
  - Implement `set_bypassed(bypassed)` and `process(samples, channels)`: applies biquad difference equation per channel, clamps output to `[-1.0, 1.0]`; no-op when bypassed
  - Add `get_eq_gains`, `set_eq_gain`, `set_eq_bypassed`, `reset_eq` Tauri commands in `lib.rs`
  - Load persisted `eq_gains` from `app_state` on startup and apply before first audio output
  - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 6.8, 6.9_

  - [x] 6.1 Write property test for EQ gain clamping (Property 9)
    - **Property 9: EQ gain clamping**
    - **Validates: Requirements 6.9**

  - [ ]* 6.2 Write property test for EQ bypass (Property 10)
    - **Property 10: EQ bypass passes audio unmodified**
    - **Validates: Requirements 6.8**

  - [ ]* 6.3 Write property test for EQ gains persistence round-trip (Property 11)
    - **Property 11: EQ gains persistence round-trip**
    - **Validates: Requirements 6.5**

- [x] 7. Implement `panner.rs` — Stereo Panning DSP
  - Create `src-tauri/src/panner.rs` with the `Panner` struct
  - Implement `new()`, `set_pan(pan)`, `gains()` returning `(cos(t), sin(t))` where `t = (pan+1)*π/4`, and `process(samples, channels)` applying per-channel gain to interleaved stereo samples
  - Add `get_pan` and `set_pan` Tauri commands in `lib.rs`
  - Load persisted `pan_value` from `app_state` on startup
  - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7, 7.8_

  - [ ]* 7.1 Write property test for panner constant-power invariant (Property 12)
    - **Property 12: Panner constant-power invariant**
    - **Validates: Requirements 7.8**

  - [ ]* 7.2 Write property test for panner unity at center (Property 13)
    - **Property 13: Panner unity at center**
    - **Validates: Requirements 7.3**

  - [ ]* 7.3 Write property test for pan value persistence round-trip (Property 14)
    - **Property 14: Pan value persistence round-trip**
    - **Validates: Requirements 7.6**

- [x] 8. Implement `crossfade.rs` — Gapless Playback and Crossfade DSP
  - Create `src-tauri/src/crossfade.rs` with the `Crossfader` struct
  - Implement `new()`, `set_duration(secs)`, `set_gapless(enabled)`, `begin_crossfade(next_samples)`, `process(current, sample_rate, channels)` applying linear fade-out/fade-in mix, and `is_complete()`
  - Extend `player.rs` to pre-decode the next queue track when `position_secs >= duration_secs - crossfade_duration_secs`; call `crossfader.begin_crossfade` with the buffered samples; skip undecoded tracks via `queue_manager.advance()`
  - Add `set_crossfade_duration`, `set_gapless_enabled`, `get_crossfade_settings` Tauri commands
  - Load persisted `crossfade_secs` and `gapless_enabled` from `app_state` on startup
  - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6, 8.7, 8.8_

  - [ ]* 8.1 Write property test for crossfade duration persistence round-trip (Property 15)
    - **Property 15: Crossfade duration persistence round-trip**
    - **Validates: Requirements 8.5**

- [ ] 9. Implement `bpm.rs` — Auto BPM Detection
  - Create `src-tauri/src/bpm.rs` with `BpmAnalyzer` using a `rayon::ThreadPool` of 2 threads
  - Implement `schedule(track_id, path)`: spawns a pool task that calls `analyze(path)`
  - Implement `analyze`: decode to mono f32 via Symphonia → `onset_strength` (512-sample hop RMS energy delta) → `autocorrelation_bpm` (lag search for [40, 250] BPM range) → round to 1 decimal → store NULL if out of range
  - On completion: `UPDATE tracks SET bpm = ? WHERE id = ?` and emit `bpm_ready` event
  - Add `analyze_bpm` Tauri command; wire scanner to call `bpm_analyzer.schedule` for newly indexed tracks with no BPM
  - _Requirements: 10.1, 10.2, 10.3, 10.4, 10.5, 10.6, 10.7, 10.8_

  - [ ]* 9.1 Write property test for BPM rounding (Property 19)
    - **Property 19: BPM rounding to one decimal place**
    - **Validates: Requirements 10.3**

  - [ ]* 9.2 Write property test for BPM range clamping to NULL (Property 20)
    - **Property 20: BPM range clamping to NULL**
    - **Validates: Requirements 10.6**

- [ ] 10. Implement `genre.rs` — Genre Detection
  - Create `src-tauri/src/genre.rs` with `GenreClassifier`, `Genre` enum, and `AudioFeatures` struct
  - Implement `schedule(track_id, path)`: spawns a pool task that calls `classify(path)`
  - Implement `extract_features`: decode audio, compute spectral centroid, spectral rolloff (85th percentile), and zero-crossing rate averaged over 1-second frames
  - Implement `rule_based_classify(features)` using the threshold table from the design
  - On completion: `UPDATE tracks SET genre = ? WHERE id = ? AND genre IS NULL` and emit `genre_ready` event
  - Add `analyze_genre` Tauri command; wire scanner to call `genre_classifier.schedule` for newly indexed tracks with no genre
  - _Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6, 11.7_

  - [ ]* 10.1 Write property test for genre classifier valid output (Property 21)
    - **Property 21: Genre classifier output is always a valid label**
    - **Validates: Requirements 11.3**

  - [ ]* 10.2 Write property test for genre classifier no-overwrite (Property 22)
    - **Property 22: Genre classifier does not overwrite existing genre**
    - **Validates: Requirements 11.4**

  - [ ]* 10.3 Write property test for audio feature extraction finite values (Property 23)
    - **Property 23: Audio feature extraction produces finite values**
    - **Validates: Requirements 11.2**

- [ ] 11. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 12. Implement `keybinds.rs` — Keyboard Shortcuts
  - Create `src-tauri/src/keybinds.rs` with `KeybindMap` and `KeybindRegistry` structs
  - Implement `KeybindMap::defaults()` with the 8 default bindings from the design; implement `get_action`, `set`, `has_conflict`
  - Implement `KeybindRegistry::new`, `load_from_db`, `save_to_db`, `dispatch`, `reset_to_defaults`
  - Register `KeybindRegistry` as Tauri managed state; add `get_keybinds`, `set_keybind`, `reset_keybinds`, `dispatch_keybind` Tauri commands
  - Load persisted keybinds from `app_state` on startup; fall back to defaults if absent
  - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8_

  - [ ]* 12.1 Write property test for keybind map persistence round-trip (Property 6)
    - **Property 6: Keybind map persistence round-trip**
    - **Validates: Requirements 4.3**

- [ ] 13. Implement `discord.rs` — Discord Rich Presence
  - Create `src-tauri/src/discord.rs` with the `DiscordPresence` struct
  - Implement `new()`, `set_enabled(enabled)`, `update_playing(title, artist, start_timestamp)`, `update_paused()`, `clear()`, `try_connect()`, `schedule_reconnect(app_handle)`
  - `try_connect` returns `false` silently when Discord is not running; all methods are no-ops when not connected
  - `schedule_reconnect` spawns a thread that retries every 30 seconds; cancelled when `set_enabled(false)` is called
  - Register `DiscordPresence` as Tauri managed state; add `set_discord_enabled` and `get_discord_enabled` Tauri commands
  - Extend `player.rs` `emit_state_changed` path to call `discord.update_playing/paused/clear` based on playback state
  - _Requirements: 14.1, 14.2, 14.3, 14.4, 14.5, 14.6, 14.7, 14.8_

- [ ] 14. Wire audio pipeline in `player.rs`
  - Instantiate `Equalizer`, `Panner`, and `Crossfader` inside the player background thread
  - After each decoded packet, call `eq.process`, then `panner.process`, then `crossfader.process` in sequence before writing to the CPAL ring buffer
  - Pass `Equalizer` and `Panner` handles to the Tauri command handlers so `set_eq_gain` and `set_pan` can mutate them from the command thread (use `Arc<Mutex<_>>`)
  - _Requirements: 6.1, 7.1, 8.1_

- [ ] 15. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 16. Extend Zustand store with new slices
  - In `src/store/index.ts`, add slices for: `queue` (`queueTrackIds`, `currentQueueIndex`), `eq` (`eqGains`, `eqBypassed`), `pan` (`panValue`), `crossfade` (`crossfadeSecs`, `gaplessEnabled`), `keybinds` (`keybindMap`), `discovery` (`recommendations`), `ambientBg` (`enabled`, `currentArtUrl`), `discordPresence` (`enabled`), and `nowPlayingOpen` with `openNowPlaying`/`closeNowPlaying` actions
  - Register event listeners for `queue_changed`, `bpm_ready`, `genre_ready`, and `keybind_action` in `initEventListeners`
  - Register `library_changed` listener to call `get_library` and refresh `tracks`
  - Add a `window` `keydown` listener that builds a combo string and calls `invoke('dispatch_keybind', { combo })`
  - _Requirements: 1.7, 2.10, 4.2, 10.5, 11.5_

- [ ] 17. Implement `AmbientBackground.tsx`
  - Create `src/components/AmbientBackground.tsx` as a `position: fixed; inset: 0; z-index: -1` div
  - Render two overlapping `<img>` elements for cross-fade: `prevArtUrl` fades out, `currentArtUrl` fades in over 600 ms using CSS `opacity` transition
  - Apply `filter: blur(40px) brightness(0.4)` and `will-change: opacity` to both images
  - Use `convertFileSrc(track.cover_art_path)` for the image URL; fall back to `background-color: #0f0f0f` when no cover art
  - Render nothing when `ambientBgEnabled` is `false` in the store
  - _Requirements: 12.1, 12.2, 12.3, 12.4, 12.5, 12.6, 12.7_

- [ ] 18. Implement `NowPlayingView.tsx`
  - Create `src/components/NowPlayingView.tsx` as a `position: fixed` full-screen overlay
  - Render `<AmbientBackground />` (when enabled), a close button, cover art `<img>` (min 300×300), track title/artist/album/duration, and the existing `PlaybackControls` component
  - Show a placeholder SVG and empty metadata when `currentTrack` is null
  - Subscribe to `selectedTrackId` and `playbackState` from the store; update all metadata and cover art on track change
  - Add a `useDroppable` zone for drag-and-drop: on drop call `queue_add_next(track_id)` and `play_track(track_id)`
  - Mount the component in `App.tsx`, conditionally shown when `nowPlayingOpen` is `true`
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8_

- [ ] 19. Implement `QueuePanel.tsx` with drag-and-drop
  - Create `src/components/QueuePanel.tsx` that renders the queue from the `queue` store slice
  - Wrap the list in `<DndContext>` and `<SortableContext>`; each row is a `<SortableItem>` using `useSortable`
  - On `onDragEnd`, call `invoke('queue_move', { from, to })`; on remove button click, call `invoke('queue_remove', { index })`; on "Play Next" button click, call `invoke('queue_add_next', { trackId })`
  - Add `useDroppable` to accept drops from `FileExplorer`; on drop call `invoke('queue_add', { trackId })` or `invoke('queue_add_next', { trackId })` based on drop position
  - Render a `DragOverlay` with a semi-transparent ghost row; render a highlighted drop indicator `<div>` between items using `isOver`
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 13.1, 13.2, 13.3, 13.4, 13.6, 13.7_

- [ ] 20. Add `useDraggable` to `FileExplorer` track rows
  - In the existing `FileExplorer` component, wrap each track row with `useDraggable` from `@dnd-kit/core`, passing `{ id: track.id, data: { trackId: track.id } }`
  - Render a `DragOverlay` at the `App.tsx` level (shared with `QueuePanel`) to show the ghost row during drag
  - _Requirements: 13.3, 13.4, 13.5, 13.6_

- [ ] 21. Implement `EqualizerPanel.tsx`
  - Create `src/components/EqualizerPanel.tsx` with 8 vertical `<input type="range" min="-12" max="12" step="0.5">` sliders, one per band
  - Each slider label shows the center frequency (60 Hz, 170 Hz, 310 Hz, 600 Hz, 1 kHz, 3 kHz, 6 kHz, 14 kHz)
  - On slider change, call `invoke('set_eq_gain', { band, gainDb })` and update the `eq.eqGains` store slice
  - Add a "Bypass" toggle that calls `invoke('set_eq_bypassed', { bypassed })`; add a "Reset" button that calls `invoke('reset_eq')` and sets all sliders to 0
  - _Requirements: 6.2, 6.3, 6.4, 6.7, 6.8_

- [ ] 22. Implement `PannerControl.tsx`
  - Create `src/components/PannerControl.tsx` with a horizontal `<input type="range" min="-1" max="1" step="0.01">` slider
  - Show "L" and "R" labels at the ends; add a center reset button that sets pan to 0
  - On slider change, call `invoke('set_pan', { value })` and update the `pan.panValue` store slice
  - _Requirements: 7.2, 7.3, 7.4, 7.5_

- [ ] 23. Implement `CrossfadeSettings.tsx`
  - Create `src/components/CrossfadeSettings.tsx` with a gapless toggle and a crossfade duration slider (`min="0.5" max="10" step="0.5"`)
  - On toggle change, call `invoke('set_gapless_enabled', { enabled })`; on slider change, call `invoke('set_crossfade_duration', { secs })`
  - _Requirements: 8.3, 8.4, 8.8_

- [ ] 24. Implement `KeybindSettings.tsx`
  - Create `src/components/KeybindSettings.tsx` rendered inside `SettingsPanel`
  - Render a row per action showing action name, current key combo, and a "Record" button
  - Clicking "Record" enters capture mode: the next `keydown` event is captured and sent to `invoke('set_keybind', { action, combo })`
  - If the backend returns a `ConflictError`, display a confirmation dialog with "Confirm" / "Cancel"; on confirm, call `set_keybind` again with a force flag or re-invoke after user confirmation
  - On "Reset to Defaults" button click, call `invoke('reset_keybinds')` and refresh the store
  - _Requirements: 4.5, 4.6, 4.7, 4.8_

- [ ] 25. Implement `CommandPalette.tsx`
  - Create `src/components/CommandPalette.tsx` as a modal overlay
  - Build a Fuse.js index from all tracks, folder paths, and registered action names; rebuild when `tracks` changes in the store
  - On each keystroke, run `fuse.search(query).slice(0, 20)` and render results in a list
  - Implement keyboard navigation: `ArrowUp`/`ArrowDown` moves `selectedIndex`; `Enter` confirms; `Escape` or outside click closes without action
  - Selection handlers: `track` → `invoke('play_track', { trackId })` + close; `folder` → `setActiveFolder(path)` + close; `action` → call store action + close
  - Mount in `App.tsx`; open when `keybind_action` event fires with `command_palette`
  - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.7, 5.8, 5.9, 5.10_

- [ ] 26. Implement `DiscoveryFeed.tsx`
  - Create `src/components/DiscoveryFeed.tsx` that calls `invoke('get_recommendations', { trackId: selectedTrackId })` when `selectedTrackId` changes
  - Render up to 20 track rows with title, artist, and a "Play Next" button
  - On "Play Next" click, call `invoke('queue_add_next', { trackId })` and `invoke('play_track', { trackId })`
  - Show a loading state while fetching; show an empty state when no recommendations are available
  - _Requirements: 9.1, 9.3, 9.4, 9.5, 9.6, 9.7_

- [ ] 27. Implement `get_recommendations` Tauri command
  - In `src-tauri/src/commands.rs` (or a new `discovery.rs`), implement the `get_recommendations` async command
  - Load current track and all non-missing tracks from DB; for each candidate compute BPM score, genre score, and tag score; compute `similarity = 0.4*bpm + 0.3*genre + 0.3*tag`; sort descending; return top 20
  - Implement fallback: when all scores are 0, return tracks by same `album_artist`, then same `album`, then random
  - Exclude the current track and tracks with `missing = true`
  - Register the command in `lib.rs`
  - _Requirements: 9.1, 9.2, 9.5, 9.6, 9.7_

  - [ ]* 27.1 Write property test for discovery feed excludes current and missing (Property 16)
    - **Property 16: Discovery feed excludes current and missing tracks**
    - **Validates: Requirements 9.6, 9.7**

  - [ ]* 27.2 Write property test for discovery feed result count and ordering (Property 17)
    - **Property 17: Discovery feed result count and ordering**
    - **Validates: Requirements 9.1**

  - [ ]* 27.3 Write property test for discovery similarity formula correctness (Property 18)
    - **Property 18: Discovery similarity formula correctness**
    - **Validates: Requirements 9.2**

- [ ] 28. Checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 29. Wire all new modules into `lib.rs`
  - Declare all new modules (`mod watcher`, `mod queue`, `mod eq`, `mod panner`, `mod crossfade`, `mod bpm`, `mod genre`, `mod discord`, `mod keybinds`) in `src-tauri/src/lib.rs`
  - In the Tauri `Builder`, register all new Tauri commands in `.invoke_handler(tauri::generate_handler![...])`
  - In the `setup` closure, initialize and register all managed state: `Watcher`, `QueueManager`, `KeybindRegistry`, `DiscordPresence`; load persisted state for EQ, panner, crossfade, and keybinds from DB
  - Call `bpm_analyzer.schedule` and `genre_classifier.schedule` for any tracks in the library that are missing BPM or genre at startup
  - _Requirements: 1.1, 4.4, 6.6, 7.7, 8.6_

- [ ] 30. Final checkpoint — Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Property tests use `proptest = "1"` in `[dev-dependencies]`; each test is tagged with `// Feature: neptune-feature-expansion, Property N: <text>`
- All property tests live in `#[cfg(test)]` modules within their respective source files
- Checkpoints ensure incremental validation after each major subsystem is complete
- The audio pipeline wiring (task 14) must happen after EQ, panner, and crossfade are implemented (tasks 6–8)
- The store extension (task 16) should be done before implementing React components (tasks 17–26) so components can subscribe to the correct slices
