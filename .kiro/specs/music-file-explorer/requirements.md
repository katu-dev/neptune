# Requirements Document

## Introduction

A cross-platform desktop application for exploring, organizing, and playing music files. Built with Tauri (Rust backend) and a TypeScript frontend, the application provides native-performance library scanning, metadata extraction, audio playback, and visual representations of audio content (waveforms, spectrograms). It targets users with large local music collections who need fast, reliable access to their files with rich metadata and audio visualization.

## Glossary

- **App**: The music-file-explorer desktop application as a whole.
- **Library**: The indexed collection of audio files discovered from one or more root directories.
- **Scanner**: The Rust backend component responsible for recursively walking directories and discovering audio files.
- **Indexer**: The Rust backend component responsible for persisting Library metadata to a local database.
- **Metadata_Extractor**: The Rust backend component (using `lofty`) that reads ID3/Vorbis/MP4/FLAC tags from audio files.
- **Waveform_Generator**: The Rust backend component (using `symphonia`) that decodes audio and computes per-channel amplitude data for display.
- **Spectrogram_Generator**: The Rust backend component that computes frequency-domain data (FFT) from decoded audio samples.
- **Player**: The Rust backend component (using `cpal`) responsible for audio decoding and playback.
- **File_Explorer**: The frontend UI component that renders the directory tree and file list.
- **Metadata_Panel**: The frontend UI component that displays tags and properties for a selected track.
- **Waveform_View**: The frontend UI component that renders the waveform visualization for a selected track.
- **Spectrogram_View**: The frontend UI component that renders the spectrogram visualization for a selected track.
- **Track**: A single audio file entry in the Library, including its path, metadata, and computed visual data.
- **Supported_Format**: Any audio format decodable by `symphonia`: MP3, FLAC, AAC, OGG Vorbis, WAV, AIFF, Opus.
- **Tag**: A metadata field embedded in an audio file (e.g., title, artist, album, year, genre, track number, cover art).

---

## Requirements

### Requirement 1: Library Scanning

**User Story:** As a music listener, I want to scan a folder on my computer so that all my audio files are discovered and added to my Library.

#### Acceptance Criteria

1. WHEN the user selects a root directory, THE Scanner SHALL recursively traverse all subdirectories and identify files with Supported_Format extensions.
2. WHEN a scan is initiated, THE App SHALL display a progress indicator showing the number of files discovered and the current directory being scanned.
3. WHEN the scan completes, THE Indexer SHALL persist all discovered Track paths and metadata to a local database.
4. IF a file is not a Supported_Format, THEN THE Scanner SHALL skip the file without interrupting the scan.
5. IF a previously indexed Track's file no longer exists on disk, THEN THE Indexer SHALL mark the Track as missing in the Library.
6. WHEN a rescan is triggered on an already-indexed directory, THE Scanner SHALL update existing Track entries and add newly discovered files without duplicating existing entries.
7. THE Scanner SHALL complete a scan of 10,000 audio files within 60 seconds on a standard SSD.

---

### Requirement 2: Metadata Extraction

**User Story:** As a music listener, I want to see rich tag information for each track so that I can identify and organize my music.

#### Acceptance Criteria

1. WHEN a Track is discovered during scanning, THE Metadata_Extractor SHALL read all available Tags from the file including title, artist, album, album artist, year, genre, track number, disc number, and duration.
2. WHEN a Track contains embedded cover art, THE Metadata_Extractor SHALL extract the cover art as image data and store a reference in the Library.
3. IF a Tag field is absent from a file, THEN THE Metadata_Extractor SHALL store a null value for that field without failing extraction.
4. WHEN a user selects a Track in the File_Explorer, THE Metadata_Panel SHALL display all extracted Tags for that Track within 100ms of selection.
5. THE Metadata_Extractor SHALL support tag formats: ID3v1, ID3v2, Vorbis Comment, MP4/iTunes atoms, and FLAC metadata blocks.

---

### Requirement 3: File Explorer Navigation

**User Story:** As a music listener, I want to browse my Library using a familiar file-tree interface so that I can navigate my music collection by folder structure.

#### Acceptance Criteria

1. THE File_Explorer SHALL render the indexed directory tree with expandable/collapsible folder nodes.
2. WHEN the user expands a folder node, THE File_Explorer SHALL display all child folders and Tracks within that folder.
3. WHEN the user selects a Track in the File_Explorer, THE App SHALL load and display that Track's metadata in the Metadata_Panel.
4. THE File_Explorer SHALL support keyboard navigation using arrow keys to move between nodes and Enter to select a Track.
5. WHEN the Library contains more than 1,000 visible items in a single folder, THE File_Explorer SHALL use virtualized rendering to maintain scroll performance above 60 frames per second.
6. THE File_Explorer SHALL provide a search input that filters visible Tracks by title, artist, or album in real time as the user types.
7. WHEN the search input contains a query, THE File_Explorer SHALL display only Tracks whose title, artist, or album contains the query string (case-insensitive).

---

### Requirement 4: Audio Playback

**User Story:** As a music listener, I want to play audio files directly in the application so that I can preview and listen to my music without switching to another player.

#### Acceptance Criteria

1. WHEN the user activates a Track (double-click or Enter key), THE Player SHALL begin decoding and playing the audio file.
2. WHILE a Track is playing, THE App SHALL display the current playback position and total duration, updated at a maximum interval of 500ms.
3. WHEN the user seeks to a position in the track, THE Player SHALL resume playback from the requested position within 200ms.
4. THE Player SHALL support playback of all Supported_Formats.
5. IF a Track file cannot be decoded, THEN THE Player SHALL display an error message identifying the Track and the reason for failure without terminating the application.
6. THE App SHALL provide playback controls: play, pause, stop, seek, volume adjustment, and skip to next/previous Track in the current folder.
7. WHILE a Track is playing, THE Player SHALL maintain uninterrupted audio output when the user navigates the File_Explorer.

---

### Requirement 5: Waveform Visualization

**User Story:** As a music listener, I want to see a waveform of the currently selected track so that I can visually understand the audio's dynamic structure and navigate by waveform.

#### Acceptance Criteria

1. WHEN a Track is selected, THE Waveform_Generator SHALL decode the full audio file and compute a downsampled amplitude envelope (one peak value per pixel-width segment) for display.
2. WHEN waveform data is ready, THE Waveform_View SHALL render the amplitude envelope as a symmetric waveform graphic within 2 seconds of Track selection for files up to 10 minutes in duration.
3. WHILE a Track is playing, THE Waveform_View SHALL display a playback position cursor that advances in real time.
4. WHEN the user clicks a position on the Waveform_View, THE Player SHALL seek to the corresponding time offset in the Track.
5. THE Waveform_Generator SHALL compute waveform data for a 3-minute audio file within 1 second on a standard desktop CPU.
6. IF waveform computation is in progress, THEN THE Waveform_View SHALL display a loading indicator until computation completes.

---

### Requirement 6: Spectrogram Visualization

**User Story:** As a music listener, I want to see a spectrogram of the currently selected track so that I can analyze the frequency content of my audio files.

#### Acceptance Criteria

1. WHEN the user activates the Spectrogram_View for a Track, THE Spectrogram_Generator SHALL compute a Short-Time Fourier Transform (STFT) over the decoded audio samples using a configurable FFT window size (default: 2048 samples) and hop size (default: 512 samples).
2. WHEN spectrogram data is ready, THE Spectrogram_View SHALL render a time-frequency heatmap where the x-axis represents time, the y-axis represents frequency (0 Hz to Nyquist), and color intensity represents magnitude in decibels.
3. THE Spectrogram_View SHALL use a perceptually uniform color map (e.g., viridis or inferno) to represent magnitude values.
4. WHEN the user clicks a position on the Spectrogram_View, THE Player SHALL seek to the corresponding time offset in the Track.
5. THE Spectrogram_Generator SHALL complete computation for a 3-minute audio file within 3 seconds on a standard desktop CPU.
6. IF spectrogram computation is in progress, THEN THE Spectrogram_View SHALL display a loading indicator until computation completes.

---

### Requirement 7: Library Persistence and State

**User Story:** As a music listener, I want my Library to be saved between sessions so that I do not need to rescan my music folders every time I open the application.

#### Acceptance Criteria

1. THE Indexer SHALL persist the Library to a local SQLite database stored in the application's data directory.
2. WHEN the App starts, THE App SHALL load the previously persisted Library from the database and display it in the File_Explorer without requiring a rescan.
3. WHEN the user adds a new root directory, THE Indexer SHALL append newly discovered Tracks to the existing Library without clearing previously indexed entries.
4. THE App SHALL persist the last selected Track, playback position, and volume level, and restore these values on next launch.
5. IF the database file is corrupted or unreadable, THEN THE App SHALL display an error message and offer to reset the Library to an empty state.

---

### Requirement 8: Cross-Platform Support

**User Story:** As a music listener, I want the application to run on Windows, macOS, and Linux so that I can use it regardless of my operating system.

#### Acceptance Criteria

1. THE App SHALL build and run on Windows 10+, macOS 12+, and Ubuntu 22.04+ without platform-specific feature degradation.
2. THE Player SHALL use the platform's default audio output device via `cpal` on all supported operating systems.
3. THE App SHALL resolve file paths using the operating system's native path separator and encoding.
4. WHERE the operating system provides a native file picker dialog, THE App SHALL use it when the user selects a root directory to scan.
