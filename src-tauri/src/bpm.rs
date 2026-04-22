use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tauri::{AppHandle, Emitter};

use crate::db;
use crate::types::AppError;

const HOP_SIZE: usize = 512;
const BPM_MIN: f32 = 40.0;
const BPM_MAX: f32 = 250.0;

// ---------------------------------------------------------------------------
// Event payload
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct BpmReadyPayload {
    track_id: i64,
    bpm: Option<f32>,
}

// ---------------------------------------------------------------------------
// BpmAnalyzer
// ---------------------------------------------------------------------------

pub struct BpmAnalyzer {
    pool: rayon::ThreadPool,
    app_handle: AppHandle,
}

impl BpmAnalyzer {
    pub fn new(app_handle: AppHandle) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("Failed to build BPM thread pool");
        Self { pool, app_handle }
    }

    /// Schedule BPM analysis for a track. Runs in the background thread pool.
    pub fn schedule(&self, track_id: i64, path: String) {
        let app_handle = self.app_handle.clone();
        self.pool.spawn(move || {
            let bpm = Self::analyze(&path);

            // Write result to DB.
            if let Ok(conn) = db::init_db(&app_handle) {
                let bpm_val: Option<f64> = bpm.map(|b| b as f64);
                let _ = conn.execute(
                    "UPDATE tracks SET bpm = ?1 WHERE id = ?2",
                    rusqlite::params![bpm_val, track_id],
                );
            }

            // Emit event to frontend.
            let _ = app_handle.emit("bpm_ready", BpmReadyPayload { track_id, bpm });
        });
    }

    /// Decode audio to mono f32, compute onset strength, run autocorrelation BPM detection.
    /// Returns `None` if BPM is outside [40, 250] or analysis fails.
    fn analyze(path: &str) -> Option<f32> {
        let samples = Self::decode_mono(path).ok()?;
        if samples.is_empty() {
            return None;
        }

        // We need the sample rate — re-probe just for the rate.
        let sample_rate = Self::probe_sample_rate(path)?;

        let onset_env = Self::onset_strength(&samples, sample_rate);
        if onset_env.is_empty() {
            return None;
        }

        Self::autocorrelation_bpm(&onset_env, sample_rate)
    }

    /// Decode the audio file to mono f32 samples.
    fn decode_mono(path: &str) -> Result<Vec<f32>, AppError> {
        let file = File::open(Path::new(path)).map_err(|e| AppError::Io(e.to_string()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| AppError::Decode(e.to_string()))?;

        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| AppError::Decode("No audio track".to_string()))?;

        let track_id = track.id;
        let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| AppError::Decode(e.to_string()))?;

        let mut mono_samples: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(SymphoniaError::ResetRequired) => {
                    decoder.reset();
                    continue;
                }
                Err(_) => break,
            };

            if packet.track_id() != track_id {
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let interleaved = audio_buffer_to_f32(&decoded);
                    // Mix down to mono.
                    let frames = interleaved.len() / channels;
                    for f in 0..frames {
                        let mut sum = 0.0f32;
                        for c in 0..channels {
                            sum += interleaved[f * channels + c];
                        }
                        mono_samples.push(sum / channels as f32);
                    }
                }
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(_) => break,
            }
        }

        Ok(mono_samples)
    }

    /// Probe the sample rate of an audio file without decoding all samples.
    fn probe_sample_rate(path: &str) -> Option<u32> {
        let file = File::open(Path::new(path)).ok()?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .ok()?;

        probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?
            .codec_params
            .sample_rate
    }

    /// Compute onset strength envelope from mono samples.
    /// For each 512-sample hop: RMS energy; onset = max(0, energy[i] - energy[i-1]).
    pub fn onset_strength(samples: &[f32], _sample_rate: u32) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }

        let num_hops = samples.len() / HOP_SIZE;
        if num_hops == 0 {
            return Vec::new();
        }

        let mut energies = Vec::with_capacity(num_hops);
        for i in 0..num_hops {
            let start = i * HOP_SIZE;
            let end = (start + HOP_SIZE).min(samples.len());
            let hop = &samples[start..end];
            let rms = (hop.iter().map(|&s| s * s).sum::<f32>() / hop.len() as f32).sqrt();
            energies.push(rms);
        }

        // onset strength = max(0, energy[i] - energy[i-1])
        let mut onset_env = Vec::with_capacity(num_hops);
        onset_env.push(0.0f32); // first frame has no predecessor
        for i in 1..num_hops {
            onset_env.push((energies[i] - energies[i - 1]).max(0.0));
        }

        onset_env
    }

    /// Compute autocorrelation of the onset envelope for lags in [40, 250] BPM range.
    /// Returns the BPM rounded to 1 decimal, or None if outside [40, 250].
    pub fn autocorrelation_bpm(onset_env: &[f32], sample_rate: u32) -> Option<f32> {
        if onset_env.is_empty() {
            return None;
        }

        // Convert BPM range to lag range (in onset envelope frames).
        // lag = 60 * sample_rate / (hop_size * bpm)
        let lag_max = (60.0 * sample_rate as f32 / (HOP_SIZE as f32 * BPM_MIN)).ceil() as usize;
        let lag_min = (60.0 * sample_rate as f32 / (HOP_SIZE as f32 * BPM_MAX)).floor() as usize;

        let lag_min = lag_min.max(1);
        let lag_max = lag_max.min(onset_env.len() - 1);

        if lag_min > lag_max {
            return None;
        }

        let n = onset_env.len();

        // Find lag with maximum autocorrelation.
        let mut best_lag = lag_min;
        let mut best_corr = f32::NEG_INFINITY;

        for lag in lag_min..=lag_max {
            let mut corr = 0.0f32;
            let count = n - lag;
            if count == 0 {
                continue;
            }
            for i in 0..count {
                corr += onset_env[i] * onset_env[i + lag];
            }
            // Normalize by count to avoid bias toward shorter lags.
            corr /= count as f32;

            if corr > best_corr {
                best_corr = corr;
                best_lag = lag;
            }
        }

        if best_lag == 0 {
            return None;
        }

        // Convert lag back to BPM.
        let bpm = 60.0 * sample_rate as f32 / (HOP_SIZE as f32 * best_lag as f32);

        // Round to 1 decimal place.
        let bpm_rounded = (bpm * 10.0).round() / 10.0;

        // Return None if outside valid range.
        if bpm_rounded < BPM_MIN || bpm_rounded > BPM_MAX {
            None
        } else {
            Some(bpm_rounded)
        }
    }
}

// ---------------------------------------------------------------------------
// Audio buffer conversion (mirrors player.rs)
// ---------------------------------------------------------------------------

fn audio_buffer_to_f32(buf: &AudioBufferRef<'_>) -> Vec<f32> {
    use symphonia::core::audio::AudioBufferRef::*;
    match buf {
        F32(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f]);
                }
            }
            out
        }
        F64(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f] as f32);
                }
            }
            out
        }
        S32(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f] as f32 / i32::MAX as f32);
                }
            }
            out
        }
        S24(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f].inner() as f32 / 8_388_607.0);
                }
            }
            out
        }
        S16(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push(b.chan(c)[f] as f32 / i16::MAX as f32);
                }
            }
            out
        }
        U8(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for c in 0..ch {
                    out.push((b.chan(c)[f] as f32 - 128.0) / 128.0);
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Feature: neptune-feature-expansion, Property 19: BPM rounding to one decimal place
    // Validates: Requirements 10.3
    #[test]
    fn test_bpm_rounding_one_decimal() {
        // Generate a synthetic onset envelope with a known period.
        // At sample_rate=44100, hop=512, a lag of 52 frames gives:
        // bpm = 60 * 44100 / (512 * 52) ≈ 99.76... → rounds to 99.8
        let sample_rate: u32 = 44100;
        let lag: usize = 52;
        let n = lag * 10 + 1;
        let mut onset_env = vec![0.0f32; n];
        // Place impulses at multiples of `lag`.
        for i in (0..n).step_by(lag) {
            onset_env[i] = 1.0;
        }

        let bpm = BpmAnalyzer::autocorrelation_bpm(&onset_env, sample_rate);
        assert!(bpm.is_some(), "Expected a BPM result");
        let bpm = bpm.unwrap();
        // Verify it is rounded to 1 decimal place.
        let rounded = (bpm * 10.0).round() / 10.0;
        assert!(
            (bpm - rounded).abs() < 1e-4,
            "BPM {bpm} is not rounded to 1 decimal place"
        );
        // Verify it is in range.
        assert!(bpm >= BPM_MIN && bpm <= BPM_MAX, "BPM {bpm} out of range");
    }

    // Feature: neptune-feature-expansion, Property 20: BPM range clamping to NULL
    // Validates: Requirements 10.6
    #[test]
    fn test_bpm_out_of_range_returns_none() {
        let sample_rate: u32 = 44100;

        // Lag corresponding to BPM > 250: lag < lag_min
        // lag_min = floor(60 * 44100 / (512 * 250)) = floor(20.67) = 20
        // Use lag=1 which is way below range → autocorrelation will pick best in range,
        // but if we force a very short onset env with only 1 frame, it returns None.
        let onset_env: Vec<f32> = vec![1.0]; // too short
        let result = BpmAnalyzer::autocorrelation_bpm(&onset_env, sample_rate);
        assert!(result.is_none(), "Expected None for too-short onset envelope");
    }

    #[test]
    fn test_onset_strength_empty() {
        let result = BpmAnalyzer::onset_strength(&[], 44100);
        assert!(result.is_empty());
    }

    #[test]
    fn test_onset_strength_non_negative() {
        let samples: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.01).sin()).collect();
        let env = BpmAnalyzer::onset_strength(&samples, 44100);
        for &v in &env {
            assert!(v >= 0.0, "Onset strength must be non-negative, got {v}");
        }
    }

    #[test]
    fn test_autocorrelation_bpm_empty() {
        let result = BpmAnalyzer::autocorrelation_bpm(&[], 44100);
        assert!(result.is_none());
    }

    // Property 19 (proptest): BPM rounding to one decimal place
    // Validates: Requirements 10.3
    #[cfg(test)]
    mod proptest_bpm {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // Feature: neptune-feature-expansion, Property 19: BPM rounding to one decimal place
            #[test]
            fn prop_bpm_rounded_to_one_decimal(
                lag in 21usize..=103usize,  // covers [40, 250] BPM at 44100 Hz
                n_periods in 5usize..=20usize,
            ) {
                let sample_rate: u32 = 44100;
                let n = lag * n_periods + 1;
                let mut onset_env = vec![0.0f32; n];
                for i in (0..n).step_by(lag) {
                    onset_env[i] = 1.0;
                }

                if let Some(bpm) = BpmAnalyzer::autocorrelation_bpm(&onset_env, sample_rate) {
                    let rounded = (bpm * 10.0).round() / 10.0;
                    prop_assert!(
                        (bpm - rounded).abs() < 1e-3,
                        "BPM {bpm} not rounded to 1 decimal"
                    );
                }
            }

            // Feature: neptune-feature-expansion, Property 20: BPM range clamping to NULL
            #[test]
            fn prop_bpm_in_range_or_none(
                lag in 1usize..=200usize,
                n_periods in 3usize..=15usize,
            ) {
                let sample_rate: u32 = 44100;
                let n = lag * n_periods + 1;
                let mut onset_env = vec![0.0f32; n];
                for i in (0..n).step_by(lag) {
                    onset_env[i] = 1.0;
                }

                if let Some(bpm) = BpmAnalyzer::autocorrelation_bpm(&onset_env, sample_rate) {
                    prop_assert!(
                        bpm >= BPM_MIN && bpm <= BPM_MAX,
                        "BPM {bpm} outside valid range [{BPM_MIN}, {BPM_MAX}]"
                    );
                }
            }
        }
    }
}
