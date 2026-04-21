use serde::{Deserialize, Serialize};

/// A single audio file entry in the Library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub dir_path: String,
    pub filename: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_secs: Option<f64>,
    pub cover_art_path: Option<String>,
    pub missing: bool,
    pub bpm: Option<f32>,
}

/// A node in the directory tree shown in the File Explorer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirNode {
    pub path: String,
    pub name: String,
    pub children: Vec<DirNode>,
    pub tracks: Vec<Track>,
}

/// Downsampled amplitude envelope for waveform rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformData {
    pub track_id: i64,
    /// Peak amplitude per column (outer envelope spikes).
    pub samples_per_channel: Vec<f32>,
    /// RMS amplitude per column (inner body fill).
    pub rms_per_column: Vec<f32>,
    pub channels: u16,
    pub duration_secs: f64,
}

/// Short-Time Fourier Transform output for spectrogram rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrogramData {
    pub track_id: i64,
    /// [time_frame][freq_bin] magnitudes in dB.
    pub magnitudes: Vec<Vec<f32>>,
    pub fft_size: u32,
    pub hop_size: u32,
    pub sample_rate: u32,
    pub duration_secs: f64,
}

/// Persisted application state restored on next launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub last_track_id: Option<i64>,
    pub last_position_secs: f64,
    pub volume: f32,
    pub root_directories: Vec<String>,
}

/// A user-defined tag that can be assigned to tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String, // CSS hex color e.g. "#6366f1"
}

/// Summary returned after a library scan completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub total_found: u32,
    pub total_indexed: u32,
    pub total_updated: u32,
    pub total_missing: u32,
}

/// All errors that can be returned from Tauri commands.
#[derive(Debug, thiserror::Error, Serialize)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("Track not found: {0}")]
    TrackNotFound(i64),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}
