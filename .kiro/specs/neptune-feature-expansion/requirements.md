# Requirements Document

## Introduction

This document specifies requirements for a large feature expansion of Neptune, a Tauri + React + Rust desktop music player. The existing app provides library scanning, file exploration, search/filter, tags, playback, waveform, spectrogram, spectrum analyzer, metadata panel, and persistent state. The expansion adds fourteen new capabilities grouped into four themes: library intelligence, playback enhancement, UI/UX enrichment, and system integration.

## Glossary

- **Watcher**: The Rust background service that monitors registered library folders for filesystem changes using OS-level file-system events.
- **Queue**: The ordered list of tracks scheduled for sequential playback, distinct from the static library.
- **Queue_Manager**: The Rust/Zustand subsystem responsible for maintaining and mutating the Queue.
- **Now_Playing_View**: The full-screen React overlay that displays large cover art, track info, and playback controls for the currently playing track.
- **Keybind_Registry**: The Rust subsystem that registers and dispatches global keyboard shortcuts.
- **Command_Palette**: The Ctrl+K modal overlay that provides fuzzy-search access to navigation targets and actions.
- **Equalizer**: The 8-band parametric EQ DSP stage inserted into the audio pipeline between the decoder and the CPAL output stream.
- **Panner**: The DSP stage that applies a stereo pan offset to the decoded audio samples before output.
- **Crossfader**: The DSP stage that overlaps the tail of the current track with the head of the next track during track transitions.
- **Discovery_Feed**: The React panel that surfaces track recommendations based on similarity to the currently playing track.
- **BPM_Analyzer**: The Rust service that detects tempo from audio samples using autocorrelation and stores the result in the database.
- **Genre_Classifier**: The Rust service that derives a genre label from audio features (spectral centroid, rolloff, zero-crossing rate) and stores the result in the database.
- **Ambient_Background**: The React visual layer that renders a blurred, color-extracted version of the current track's cover art as the application background.
- **Discord_Presence**: The Rust service that updates the Discord Rich Presence status with the currently playing track's metadata.
- **Track**: An audio file record in the SQLite database, as defined in the existing `types.rs`.
- **Library**: The full set of indexed Tracks in the SQLite database.
- **Player**: The existing Rust background thread that decodes audio and drives CPAL output.

---

## Requirements

### Requirement 1: Folder Watching

**User Story:** As a music listener, I want Neptune to automatically detect when files are added, removed, or changed in my library folders, so that my library stays up to date without manual rescans.

#### Acceptance Criteria

1. WHEN the application starts, THE Watcher SHALL begin monitoring all directories listed in `root_directories` for filesystem events (create, delete, rename, modify).
2. WHEN a supported audio file is created or renamed into a watched directory, THE Watcher SHALL index the new file into the Library within 2 seconds of the event.
3. WHEN a file is deleted or renamed out of a watched directory, THE Watcher SHALL mark the corresponding Track as `missing = true` in the Library within 2 seconds of the event.
4. WHEN a watched audio file's content is modified on disk, THE Watcher SHALL re-extract its metadata and update the Track record in the Library within 5 seconds of the event.
5. WHEN a new root directory is added via `scan_directory`, THE Watcher SHALL add that directory to its watch list without requiring an application restart.
6. WHEN a root directory is removed via `remove_root_directory`, THE Watcher SHALL stop monitoring that directory without requiring an application restart.
7. WHEN a filesystem event triggers a Library update, THE Watcher SHALL emit a `library_changed` Tauri event so the frontend can refresh its track list.
8. IF the Watcher encounters a filesystem error while monitoring a directory, THEN THE Watcher SHALL log the error and continue monitoring the remaining directories.

---

### Requirement 2: Queue and Playlist Management

**User Story:** As a music listener, I want to build and manage a queue of tracks to play next, so that I can control the order of playback without navigating the file explorer between tracks.

#### Acceptance Criteria

1. THE Queue_Manager SHALL maintain an ordered list of Track IDs representing the playback queue.
2. WHEN the user adds a track to the queue, THE Queue_Manager SHALL append the Track ID to the end of the queue.
3. WHEN the user adds a track to play next, THE Queue_Manager SHALL insert the Track ID immediately after the currently playing position in the queue.
4. WHEN the user removes a track from the queue, THE Queue_Manager SHALL remove that entry without affecting other queue positions.
5. WHEN the user reorders queue entries via drag-and-drop, THE Queue_Manager SHALL update the queue order to reflect the new positions.
6. WHEN the current track finishes and the queue is non-empty, THE Player SHALL automatically begin playing the next track in the queue.
7. WHEN the current track finishes and the queue is empty, THE Player SHALL stop playback.
8. WHEN the user clears the queue, THE Queue_Manager SHALL remove all entries from the queue.
9. THE Queue_Manager SHALL persist the queue contents to the database so that the queue is restored on application relaunch.
10. WHEN the queue changes, THE Queue_Manager SHALL emit a `queue_changed` Tauri event so the frontend can re-render the queue panel.
11. THE Queue_Manager SHALL expose a shuffle action that randomises the order of all tracks after the current playing position.

---

### Requirement 3: Now Playing View

**User Story:** As a music listener, I want a full-screen focused view of the currently playing track, so that I can see large cover art and track information clearly while listening.

#### Acceptance Criteria

1. WHEN the user activates the Now Playing View, THE Now_Playing_View SHALL render as a full-screen overlay above all other UI panels.
2. THE Now_Playing_View SHALL display the cover art of the currently playing track at a minimum rendered size of 300 × 300 logical pixels.
3. THE Now_Playing_View SHALL display the track title, artist, album, and duration of the currently playing track.
4. THE Now_Playing_View SHALL include a seek slider, play/pause button, previous-track button, and next-track button that function identically to the existing PlaybackControls component.
5. WHEN no track is playing, THE Now_Playing_View SHALL display a placeholder cover art image and empty metadata fields.
6. WHEN the currently playing track changes, THE Now_Playing_View SHALL update all displayed metadata and cover art within one render cycle.
7. WHEN the user dismisses the Now Playing View, THE Now_Playing_View SHALL close and return focus to the previously active panel.
8. WHERE the Ambient_Background feature is enabled, THE Now_Playing_View SHALL render the Ambient_Background behind its content.

---

### Requirement 4: Keyboard Shortcuts and Keybind Customization

**User Story:** As a power user, I want to control playback and navigate Neptune using keyboard shortcuts, and to remap those shortcuts to keys I prefer, so that I can operate the app efficiently without touching the mouse.

#### Acceptance Criteria

1. THE Keybind_Registry SHALL provide default global shortcuts: Space (play/pause), Right Arrow (next track), Left Arrow (previous track), Up Arrow (volume up by 5%), Down Arrow (volume down by 5%), F (seek forward 10 s), B (seek backward 10 s), and Ctrl+K (open Command Palette).
2. WHEN a registered shortcut key combination is pressed while the Neptune window is focused, THE Keybind_Registry SHALL dispatch the corresponding action within 50 ms.
3. THE Keybind_Registry SHALL store the current keybind mapping in the database under the `keybinds` app_state key as a JSON object.
4. WHEN the application starts, THE Keybind_Registry SHALL load the persisted keybind mapping from the database and register all shortcuts.
5. WHEN the user opens the Keybinds settings page and assigns a new key combination to an action, THE Keybind_Registry SHALL update the mapping in memory and persist it to the database.
6. IF the user assigns a key combination that is already bound to a different action, THEN THE Keybind_Registry SHALL display a conflict warning and require the user to confirm the reassignment before overwriting the existing binding.
7. WHEN the user resets keybinds to defaults on the settings page, THE Keybind_Registry SHALL restore the default mapping and persist it to the database.
8. THE Keybind_Registry SHALL support modifier keys (Ctrl, Alt, Shift, Meta) in combination with any printable key or function key.

---

### Requirement 5: Command Palette

**User Story:** As a power user, I want a Ctrl+K command palette that lets me quickly navigate to any track, folder, or action by typing a few characters, so that I can find and trigger things without using the mouse.

#### Acceptance Criteria

1. WHEN the user presses the Command Palette shortcut (default Ctrl+K), THE Command_Palette SHALL open as a modal overlay centered on the screen.
2. THE Command_Palette SHALL accept free-text input and display matching results within 100 ms of each keystroke.
3. THE Command_Palette SHALL search across track titles, artist names, album names, folder paths, and registered action names.
4. THE Command_Palette SHALL display a maximum of 20 results at a time, ranked by fuzzy-match score descending.
5. WHEN the user selects a track result, THE Command_Palette SHALL close and begin playing that track.
6. WHEN the user selects a folder result, THE Command_Palette SHALL close and navigate the File Explorer to that folder.
7. WHEN the user selects an action result, THE Command_Palette SHALL close and execute that action.
8. WHEN the user presses Escape or clicks outside the Command_Palette, THE Command_Palette SHALL close without performing any action.
9. THE Command_Palette SHALL support keyboard navigation: Up/Down arrows to move between results, Enter to confirm selection.
10. FOR ALL query strings of length ≥ 1, THE Command_Palette SHALL return results that are a superset of the results returned for any extension of that query string (monotonic filtering property).

---

### Requirement 6: 8-Band Equalizer

**User Story:** As an audiophile, I want an 8-band equalizer that I can adjust per band, so that I can shape the frequency response of my music to my preference.

#### Acceptance Criteria

1. THE Equalizer SHALL process audio samples in the Player pipeline between the decoder output and the CPAL output buffer.
2. THE Equalizer SHALL provide exactly 8 frequency bands with center frequencies at 60 Hz, 170 Hz, 310 Hz, 600 Hz, 1 kHz, 3 kHz, 6 kHz, and 14 kHz.
3. WHEN the user adjusts a band's gain slider, THE Equalizer SHALL apply the new gain value to the audio pipeline within one audio buffer period (≤ 50 ms at 48 kHz / 2048-sample buffer).
4. THE Equalizer SHALL support a per-band gain range of −12 dB to +12 dB in 0.5 dB steps.
5. THE Equalizer SHALL persist the current band gains to the database under the `eq_gains` app_state key as a JSON array of 8 floats.
6. WHEN the application starts, THE Equalizer SHALL load the persisted band gains from the database and apply them before the first audio sample is output.
7. WHEN the user resets the EQ to flat, THE Equalizer SHALL set all band gains to 0 dB and persist the change.
8. WHERE the Equalizer is bypassed by the user, THE Equalizer SHALL pass audio samples through unmodified.
9. FOR ALL valid gain configurations, THE Equalizer SHALL not introduce clipping on audio samples whose pre-EQ amplitude is within −12 dBFS (i.e., the output SHALL be clamped to [−1.0, 1.0]).

---

### Requirement 7: Panning Control

**User Story:** As a listener, I want a stereo panning control so that I can adjust the left/right balance of the audio output.

#### Acceptance Criteria

1. THE Panner SHALL process stereo audio samples in the Player pipeline after the Equalizer stage and before the CPAL output buffer.
2. THE Panner SHALL accept a pan value in the range [−1.0, 1.0], where −1.0 is full left, 0.0 is center, and 1.0 is full right.
3. WHEN the pan value is 0.0, THE Panner SHALL pass left and right channel samples through unmodified (unity gain on both channels).
4. WHEN the pan value is non-zero, THE Panner SHALL apply a constant-power pan law: left gain = cos((pan + 1) × π / 4), right gain = sin((pan + 1) × π / 4).
5. WHEN the user adjusts the pan slider, THE Panner SHALL apply the new pan value within one audio buffer period (≤ 50 ms).
6. THE Panner SHALL persist the current pan value to the database under the `pan_value` app_state key.
7. WHEN the application starts, THE Panner SHALL load the persisted pan value from the database and apply it before the first audio sample is output.
8. FOR ALL pan values p in [−1.0, 1.0], THE Panner SHALL preserve the total perceived loudness such that left_gain² + right_gain² = 1.0 (constant-power invariant).

---

### Requirement 8: Gapless Playback and Crossfade

**User Story:** As a music listener, I want seamless transitions between tracks, with an optional crossfade, so that albums and playlists flow without jarring silences or abrupt cuts.

#### Acceptance Criteria

1. WHEN gapless playback is enabled and the current track ends, THE Player SHALL begin decoding and buffering the next queue track before the current track's audio buffer is exhausted, such that no silence gap exceeding 10 ms is introduced between tracks.
2. WHEN crossfade is enabled and the current track has fewer than `crossfade_duration_secs` seconds remaining, THE Crossfader SHALL begin fading out the current track and fading in the next track simultaneously.
3. THE Crossfader SHALL support a configurable crossfade duration in the range [0.5, 10.0] seconds in 0.5-second steps.
4. WHEN crossfade duration is set to 0, THE Crossfader SHALL behave identically to gapless playback (no fade, no gap).
5. THE Crossfader SHALL persist the crossfade duration to the database under the `crossfade_secs` app_state key.
6. WHEN the application starts, THE Crossfader SHALL load the persisted crossfade duration and apply it to subsequent track transitions.
7. IF the next queue track cannot be decoded (missing file, unsupported format), THEN THE Player SHALL skip that track, log the error, and attempt to play the track after it in the queue.
8. WHERE gapless playback is disabled by the user, THE Player SHALL revert to the existing stop-then-play behavior between tracks.

---

### Requirement 9: Discovery / "What to Play Next" Feed

**User Story:** As a music listener, I want Neptune to suggest tracks similar to what I'm currently playing, based on BPM, genre, and tags, so that I can discover music in my library that fits my current mood.

#### Acceptance Criteria

1. WHEN the user opens the Discovery Feed, THE Discovery_Feed SHALL display up to 20 track recommendations ranked by similarity score to the currently playing track.
2. THE Discovery_Feed SHALL compute similarity using a weighted combination of: BPM proximity (weight 0.4), genre match (weight 0.3), and shared tag count (weight 0.3).
3. WHEN the currently playing track changes, THE Discovery_Feed SHALL recompute and refresh its recommendations within 500 ms.
4. WHEN the user clicks a recommended track, THE Discovery_Feed SHALL add that track to the front of the queue and begin playing it.
5. IF the currently playing track has no BPM, genre, or tags, THEN THE Discovery_Feed SHALL fall back to recommending tracks from the same album artist, then the same album, then random tracks.
6. THE Discovery_Feed SHALL exclude the currently playing track from its recommendations.
7. THE Discovery_Feed SHALL exclude tracks marked as `missing = true` from its recommendations.

---

### Requirement 10: Auto BPM Detection

**User Story:** As a music listener, I want Neptune to automatically detect and store the BPM of each track, so that I can use BPM for filtering, sorting, and discovery.

#### Acceptance Criteria

1. WHEN a track is indexed into the Library and has no stored BPM, THE BPM_Analyzer SHALL schedule that track for BPM analysis.
2. THE BPM_Analyzer SHALL detect tempo using onset-strength autocorrelation on the audio samples decoded by Symphonia.
3. THE BPM_Analyzer SHALL store the detected BPM as a floating-point value in a `bpm` column on the `tracks` table, rounded to one decimal place.
4. THE BPM_Analyzer SHALL run analysis tasks in a background thread pool so that BPM detection does not block the Player or the UI thread.
5. WHEN BPM analysis completes for a track, THE BPM_Analyzer SHALL emit a `bpm_ready` Tauri event with the track ID and detected BPM value.
6. THE BPM_Analyzer SHALL detect BPM values in the range [40, 250] BPM; values outside this range SHALL be stored as NULL.
7. IF a track's audio cannot be decoded during BPM analysis, THEN THE BPM_Analyzer SHALL store NULL for that track's BPM and log the error.
8. WHEN the user triggers a manual BPM re-analysis for a track, THE BPM_Analyzer SHALL overwrite the existing BPM value with the newly computed result.

---

### Requirement 11: Genre Detection from Audio Analysis

**User Story:** As a music listener, I want Neptune to detect the genre of tracks from their audio content, not just their metadata tags, so that tracks without genre tags are still categorised.

#### Acceptance Criteria

1. WHEN a track is indexed and has no genre stored in its metadata, THE Genre_Classifier SHALL schedule that track for audio-based genre classification.
2. THE Genre_Classifier SHALL extract the following audio features from the decoded samples: spectral centroid, spectral rolloff (85th percentile), and zero-crossing rate, each averaged over 1-second analysis frames.
3. THE Genre_Classifier SHALL map the extracted features to one of the following genre labels using a rule-based classifier: Electronic, Rock, Classical, Jazz, Hip-Hop, Ambient, or Unknown.
4. THE Genre_Classifier SHALL store the detected genre in the existing `genre` column of the `tracks` table only when the column is currently NULL.
5. WHEN genre classification completes for a track, THE Genre_Classifier SHALL emit a `genre_ready` Tauri event with the track ID and detected genre label.
6. THE Genre_Classifier SHALL run in the same background thread pool as the BPM_Analyzer so that classification does not block the Player or the UI thread.
7. IF a track's audio cannot be decoded during genre classification, THEN THE Genre_Classifier SHALL leave the `genre` column as NULL and log the error.

---

### Requirement 12: Ambient Background

**User Story:** As a music listener, I want the album cover art of the currently playing track to be used as a blurred, dynamic background behind the Neptune UI, so that the app feels visually immersive.

#### Acceptance Criteria

1. WHEN a track begins playing and cover art is available, THE Ambient_Background SHALL render a blurred version of the cover art as the full-window background layer behind all UI panels.
2. THE Ambient_Background SHALL apply a CSS blur of at least 40 px and a darkening overlay of at least 60% opacity to ensure UI text remains legible.
3. WHEN the currently playing track changes and the new track has different cover art, THE Ambient_Background SHALL cross-fade from the previous background to the new one over 600 ms.
4. WHEN the currently playing track has no cover art, THE Ambient_Background SHALL display a solid dark fallback color (#0f0f0f) instead of an image.
5. WHEN playback stops, THE Ambient_Background SHALL transition to the fallback color over 600 ms.
6. WHERE the Ambient_Background is disabled by the user in settings, THE Ambient_Background SHALL not render any background image and SHALL use the application's default background color.
7. THE Ambient_Background SHALL not cause the main thread frame rate to drop below 30 fps during transitions, as measured by the browser's `requestAnimationFrame` callback interval.

---

### Requirement 13: Drag-and-Drop Track Reordering

**User Story:** As a music listener, I want to reorder tracks in the queue and in the file explorer by dragging and dropping them, so that I can quickly rearrange what plays next.

#### Acceptance Criteria

1. WHEN the user begins dragging a track row in the Queue panel, THE Queue_Manager SHALL enter drag mode and display a visual insertion indicator between queue entries.
2. WHEN the user drops a dragged track row onto a new position in the Queue panel, THE Queue_Manager SHALL move the track to that position and update the queue order.
3. WHEN the user begins dragging a track row in the File Explorer, THE Queue_Manager SHALL treat the drag as an "add to queue" gesture.
4. WHEN the user drops a dragged File Explorer track onto the Queue panel, THE Queue_Manager SHALL insert the track at the drop position in the queue.
5. WHEN the user drops a dragged File Explorer track onto the Now Playing View, THE Queue_Manager SHALL insert the track at the front of the queue and begin playing it immediately.
6. THE drag-and-drop interaction SHALL provide visual feedback (ghost image of the dragged row, highlighted drop target) throughout the drag gesture.
7. IF the user releases the drag outside any valid drop target, THE Queue_Manager SHALL cancel the drag and leave the queue unchanged.

---

### Requirement 14: Discord Rich Presence

**User Story:** As a social music listener, I want Neptune to show my currently playing track in my Discord status, so that my friends can see what I'm listening to.

#### Acceptance Criteria

1. WHEN a track begins playing, THE Discord_Presence SHALL update the Discord Rich Presence status to display the track title and artist name within 2 seconds.
2. THE Discord_Presence SHALL display the elapsed playback time as a Discord "elapsed" timestamp in the Rich Presence activity.
3. WHEN playback is paused, THE Discord_Presence SHALL update the Rich Presence status to show "Paused" and remove the elapsed timestamp.
4. WHEN playback stops, THE Discord_Presence SHALL clear the Rich Presence activity entirely.
5. WHEN the currently playing track changes, THE Discord_Presence SHALL update the Rich Presence status within 2 seconds of the track change event.
6. WHERE Discord is not running on the host machine, THE Discord_Presence SHALL silently fail to connect and SHALL not display any error to the user.
7. WHERE the user disables Discord Rich Presence in settings, THE Discord_Presence SHALL clear any active Rich Presence activity and SHALL not attempt to reconnect until the setting is re-enabled.
8. IF the Discord IPC connection is lost while a track is playing, THEN THE Discord_Presence SHALL attempt to reconnect at 30-second intervals without blocking the Player or the UI thread.
