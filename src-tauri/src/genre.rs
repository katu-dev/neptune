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

// ---------------------------------------------------------------------------
// Event payload
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct GenreReadyPayload {
    track_id: i64,
    genre: String,
}

// ---------------------------------------------------------------------------
// Genre enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Genre {
    Electronic,
    Rock,
    Classical,
    Jazz,
    HipHop,
    Ambient,
    Unknown,
}

impl Genre {
    pub fn as_str(&self) -> &'static str {
        match self {
            Genre::Electronic => "Electronic",
            Genre::Rock => "Rock",
            Genre::Classical => "Classical",
            Genre::Jazz => "Jazz",
            Genre::HipHop => "Hip-Hop",
            Genre::Ambient => "Ambient",
            Genre::Unknown => "Unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// AudioFeatures struct
// ---------------------------------------------------------------------------

pub struct AudioFeatures {
    spectral_centroid: f32,
    spectral_rolloff: f32,
    zero_crossing_rate: f32,
}

// ---------------------------------------------------------------------------
// GenreClassifier
// ---------------------------------------------------------------------------

pub struct GenreClassifier {
    pool: rayon::ThreadPool,
    app_handle: AppHandle,
}

impl GenreClassifier {
    pub fn new(app_handle: AppHandle) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("Failed to build genre thread pool");
        Self { pool, app_handle }
    }

    /// Schedule genre classification for a track. Runs in the background thread pool.
    pub fn schedule(&self, track_id: i64, path: String) {
        let app_handle = self.app_handle.clone();
        self.pool.spawn(move || {
            let genre = Self::classify(&path);
            let genre_str = genre.as_str().to_string();

            // Write result to DB only when genre IS NULL.
            if let Ok(conn) = db::init_db(&app_handle) {
                let _ = conn.execute(
                    "UPDATE tracks SET genre = ?1 WHERE id = ?2 AND genre IS NULL",
                    rusqlite::params![genre_str, track_id],
                );
            }

            // Emit event to frontend.
            let _ = app_handle.emit(
                "genre_ready",
                GenreReadyPayload {
                    track_id,
                    genre: genre_str,
                },
            );
        });
    }

    /// Decode audio, extract features, and classify.
    fn classify(path: &str) -> Genre {
        let (samples, sample_rate) = match Self::decode_mono_with_rate(path) {
            Ok(v) => v,
            Err(_) => return Genre::Unknown,
        };

        if samples.is_empty() || sample_rate == 0 {
            return Genre::Unknown;
        }

        let features = Self::extract_features(&samples, sample_rate);
        Self::rule_based_classify(&features)
    }

    /// Decode audio to mono f32 samples and return (samples, sample_rate).
    fn decode_mono_with_rate(path: &str) -> Result<(Vec<f32>, u32), AppError> {
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
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);

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

        Ok((mono_samples, sample_rate))
    }

    /// Extract audio features averaged over 1-second frames.
    ///
    /// - spectral_centroid: Σ(f * |X[f]|) / Σ|X[f]|
    /// - spectral_rolloff: frequency below which 85% of spectral energy is concentrated
    /// - zero_crossing_rate: (1/N) * Σ |sign(x[n]) - sign(x[n-1])| / 2
    pub fn extract_features(samples: &[f32], sample_rate: u32) -> AudioFeatures {
        let frame_size = sample_rate as usize; // 1-second frames
        if samples.is_empty() || frame_size == 0 {
            return AudioFeatures {
                spectral_centroid: 0.0,
                spectral_rolloff: 0.0,
                zero_crossing_rate: 0.0,
            };
        }

        let num_frames = (samples.len() + frame_size - 1) / frame_size;
        let mut centroid_sum = 0.0f64;
        let mut rolloff_sum = 0.0f64;
        let mut zcr_sum = 0.0f64;
        let mut valid_frames = 0usize;

        for frame_idx in 0..num_frames {
            let start = frame_idx * frame_size;
            let end = (start + frame_size).min(samples.len());
            let frame = &samples[start..end];

            if frame.len() < 2 {
                continue;
            }

            // --- Zero-crossing rate ---
            let zcr = compute_zcr(frame);

            // --- Spectral features via naive DFT magnitude spectrum ---
            // Use up to 4096 points for efficiency (subsample the frame if needed).
            let fft_n = frame.len().min(4096).next_power_of_two();
            let mag = compute_magnitude_spectrum(&frame[..fft_n.min(frame.len())], fft_n);

            let centroid = compute_spectral_centroid(&mag, sample_rate);
            let rolloff = compute_spectral_rolloff(&mag, sample_rate, 0.85);

            centroid_sum += centroid as f64;
            rolloff_sum += rolloff as f64;
            zcr_sum += zcr as f64;
            valid_frames += 1;
        }

        if valid_frames == 0 {
            return AudioFeatures {
                spectral_centroid: 0.0,
                spectral_rolloff: 0.0,
                zero_crossing_rate: 0.0,
            };
        }

        AudioFeatures {
            spectral_centroid: (centroid_sum / valid_frames as f64) as f32,
            spectral_rolloff: (rolloff_sum / valid_frames as f64) as f32,
            zero_crossing_rate: (zcr_sum / valid_frames as f64) as f32,
        }
    }

    /// Rule-based genre classifier using threshold table from the design.
    ///
    /// | Genre      | Centroid      | Rolloff        | ZCR          |
    /// |------------|---------------|----------------|--------------|
    /// | Electronic | > 3000 Hz     | > 8000 Hz      | any          |
    /// | Rock       | 1500–4000 Hz  | 4000–10000 Hz  | > 0.08       |
    /// | Classical  | < 2000 Hz     | < 5000 Hz      | < 0.05       |
    /// | Jazz       | 1000–3000 Hz  | 3000–7000 Hz   | 0.04–0.10    |
    /// | Hip-Hop    | < 2500 Hz     | < 6000 Hz      | < 0.07       |
    /// | Ambient    | < 1500 Hz     | < 4000 Hz      | < 0.03       |
    /// | Unknown    | (fallback)    |                |              |
    pub fn rule_based_classify(features: &AudioFeatures) -> Genre {
        let c = features.spectral_centroid;
        let r = features.spectral_rolloff;
        let z = features.zero_crossing_rate;

        // Electronic: centroid > 3000 AND rolloff > 8000
        if c > 3000.0 && r > 8000.0 {
            return Genre::Electronic;
        }

        // Ambient: centroid < 1500 AND rolloff < 4000 AND zcr < 0.03
        if c < 1500.0 && r < 4000.0 && z < 0.03 {
            return Genre::Ambient;
        }

        // Classical: centroid < 2000 AND rolloff < 5000 AND zcr < 0.05
        if c < 2000.0 && r < 5000.0 && z < 0.05 {
            return Genre::Classical;
        }

        // Rock: centroid 1500–4000 AND rolloff 4000–10000 AND zcr > 0.08
        if (1500.0..=4000.0).contains(&c) && (4000.0..=10000.0).contains(&r) && z > 0.08 {
            return Genre::Rock;
        }

        // Jazz: centroid 1000–3000 AND rolloff 3000–7000 AND zcr 0.04–0.10
        if (1000.0..=3000.0).contains(&c) && (3000.0..=7000.0).contains(&r) && (0.04..=0.10).contains(&z) {
            return Genre::Jazz;
        }

        // Hip-Hop: centroid < 2500 AND rolloff < 6000 AND zcr < 0.07
        if c < 2500.0 && r < 6000.0 && z < 0.07 {
            return Genre::HipHop;
        }

        Genre::Unknown
    }
}

// ---------------------------------------------------------------------------
// DSP helpers
// ---------------------------------------------------------------------------

/// Compute zero-crossing rate: (1/N) * Σ |sign(x[n]) - sign(x[n-1])| / 2
fn compute_zcr(frame: &[f32]) -> f32 {
    if frame.len() < 2 {
        return 0.0;
    }
    let crossings: f32 = frame
        .windows(2)
        .map(|w| {
            let s0 = w[0].signum();
            let s1 = w[1].signum();
            (s1 - s0).abs() / 2.0
        })
        .sum();
    crossings / frame.len() as f32
}

/// Compute the one-sided magnitude spectrum via naive DFT with Hann windowing.
/// Returns magnitudes for bins 0..=fft_n/2.
fn compute_magnitude_spectrum(frame: &[f32], fft_n: usize) -> Vec<f32> {
    // Pad or truncate to fft_n
    let n = fft_n;
    let num_bins = n / 2 + 1;
    let mut mag = vec![0.0f32; num_bins];

    // Naive DFT — O(N²) but acceptable for short frames (≤4096 samples)
    // and rule-based classification where precision > speed isn't critical.
    // Apply Hann window to reduce spectral leakage.
    use std::f64::consts::PI;
    let frame_len = frame.len().min(n);

    // Precompute Hann window coefficients
    let window: Vec<f64> = (0..frame_len)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (frame_len - 1).max(1) as f64).cos()))
        .collect();

    for k in 0..num_bins {
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        for (i, &s) in frame[..frame_len].iter().enumerate() {
            let windowed = s as f64 * window[i];
            let angle = 2.0 * PI * k as f64 * i as f64 / n as f64;
            re += windowed * angle.cos();
            im -= windowed * angle.sin();
        }
        mag[k] = (re * re + im * im).sqrt() as f32;
    }

    mag
}

/// Compute spectral centroid: Σ(freq[k] * mag[k]) / Σmag[k]
fn compute_spectral_centroid(mag: &[f32], sample_rate: u32) -> f32 {
    let n = (mag.len() - 1) * 2; // fft_n = 2 * (num_bins - 1)
    let freq_resolution = sample_rate as f32 / n as f32;

    let mut weighted_sum = 0.0f64;
    let mut mag_sum = 0.0f64;

    for (k, &m) in mag.iter().enumerate() {
        let freq = k as f32 * freq_resolution;
        weighted_sum += freq as f64 * m as f64;
        mag_sum += m as f64;
    }

    if mag_sum < 1e-10 {
        return 0.0;
    }

    (weighted_sum / mag_sum) as f32
}

/// Compute spectral rolloff: frequency below which `threshold` fraction of energy is concentrated.
fn compute_spectral_rolloff(mag: &[f32], sample_rate: u32, threshold: f32) -> f32 {
    let n = (mag.len() - 1) * 2;
    let freq_resolution = sample_rate as f32 / n as f32;

    let total_energy: f64 = mag.iter().map(|&m| (m * m) as f64).sum();
    if total_energy < 1e-20 {
        return 0.0;
    }

    let target = total_energy * threshold as f64;
    let mut cumulative = 0.0f64;

    for (k, &m) in mag.iter().enumerate() {
        cumulative += (m * m) as f64;
        if cumulative >= target {
            return k as f32 * freq_resolution;
        }
    }

    // Fallback: return Nyquist
    (mag.len() - 1) as f32 * freq_resolution
}

// ---------------------------------------------------------------------------
// Audio buffer conversion (mirrors bpm.rs)
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

    // Helper: build AudioFeatures directly
    fn features(centroid: f32, rolloff: f32, zcr: f32) -> AudioFeatures {
        AudioFeatures {
            spectral_centroid: centroid,
            spectral_rolloff: rolloff,
            zero_crossing_rate: zcr,
        }
    }

    // --- rule_based_classify tests ---

    #[test]
    fn test_classify_electronic() {
        // centroid > 3000, rolloff > 8000
        let g = GenreClassifier::rule_based_classify(&features(3500.0, 9000.0, 0.05));
        assert_eq!(g, Genre::Electronic);
    }

    #[test]
    fn test_classify_ambient() {
        // centroid < 1500, rolloff < 4000, zcr < 0.03
        let g = GenreClassifier::rule_based_classify(&features(1000.0, 3000.0, 0.01));
        assert_eq!(g, Genre::Ambient);
    }

    #[test]
    fn test_classify_classical() {
        // centroid < 2000, rolloff < 5000, zcr < 0.05
        let g = GenreClassifier::rule_based_classify(&features(1800.0, 4500.0, 0.03));
        assert_eq!(g, Genre::Classical);
    }

    #[test]
    fn test_classify_rock() {
        // centroid 1500–4000, rolloff 4000–10000, zcr > 0.08
        let g = GenreClassifier::rule_based_classify(&features(2500.0, 7000.0, 0.10));
        assert_eq!(g, Genre::Rock);
    }

    #[test]
    fn test_classify_jazz() {
        // centroid 1000–3000, rolloff 3000–7000, zcr 0.04–0.10
        let g = GenreClassifier::rule_based_classify(&features(2000.0, 5000.0, 0.07));
        assert_eq!(g, Genre::Jazz);
    }

    #[test]
    fn test_classify_hiphop() {
        // centroid < 2500, rolloff < 6000, zcr < 0.07
        let g = GenreClassifier::rule_based_classify(&features(2000.0, 5000.0, 0.05));
        // Note: Jazz also matches 2000/5000/0.05 (zcr 0.04–0.10), so Jazz wins first.
        // Use zcr outside jazz range to hit HipHop.
        let g2 = GenreClassifier::rule_based_classify(&features(2000.0, 5000.0, 0.02));
        // zcr=0.02 < 0.03 → Ambient check: centroid 2000 >= 1500 → not Ambient
        // Classical: centroid 2000 < 2000? No (not strictly less). → not Classical
        // Rock: zcr 0.02 not > 0.08 → not Rock
        // Jazz: zcr 0.02 not in 0.04–0.10 → not Jazz
        // HipHop: centroid 2000 < 2500, rolloff 5000 < 6000, zcr 0.02 < 0.07 → HipHop
        assert_eq!(g2, Genre::HipHop);
        let _ = g; // suppress unused warning
    }

    #[test]
    fn test_classify_unknown() {
        // No rule matches: high centroid, low rolloff, mid zcr
        let g = GenreClassifier::rule_based_classify(&features(5000.0, 2000.0, 0.05));
        assert_eq!(g, Genre::Unknown);
    }

    // --- extract_features tests ---

    #[test]
    fn test_extract_features_empty() {
        let f = GenreClassifier::extract_features(&[], 44100);
        assert_eq!(f.spectral_centroid, 0.0);
        assert_eq!(f.spectral_rolloff, 0.0);
        assert_eq!(f.zero_crossing_rate, 0.0);
    }

    #[test]
    fn test_extract_features_sine_wave() {
        // 440 Hz sine at 44100 Hz sample rate, 1 second
        let sample_rate: u32 = 44100;
        let samples: Vec<f32> = (0..sample_rate as usize)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        let f = GenreClassifier::extract_features(&samples, sample_rate);

        // Spectral centroid should be near 440 Hz for a pure sine
        assert!(
            f.spectral_centroid > 100.0 && f.spectral_centroid < 2000.0,
            "Centroid {} not near 440 Hz",
            f.spectral_centroid
        );
        // ZCR for 440 Hz sine: ~2*440/44100 ≈ 0.02
        assert!(
            f.zero_crossing_rate > 0.0 && f.zero_crossing_rate < 0.1,
            "ZCR {} out of expected range",
            f.zero_crossing_rate
        );
    }

    #[test]
    fn test_zcr_constant_signal() {
        // Constant positive signal → no zero crossings
        let samples = vec![1.0f32; 1000];
        let zcr = compute_zcr(&samples);
        assert_eq!(zcr, 0.0);
    }

    #[test]
    fn test_zcr_alternating_signal() {
        // Alternating +1/-1 → maximum zero crossings
        let samples: Vec<f32> = (0..1000).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let zcr = compute_zcr(&samples);
        // Every adjacent pair crosses zero → (999 crossings) / 1000 ≈ 0.999
        assert!(zcr > 0.9, "ZCR {} should be near 1.0 for alternating signal", zcr);
    }

    #[test]
    fn test_genre_as_str() {
        assert_eq!(Genre::Electronic.as_str(), "Electronic");
        assert_eq!(Genre::Rock.as_str(), "Rock");
        assert_eq!(Genre::Classical.as_str(), "Classical");
        assert_eq!(Genre::Jazz.as_str(), "Jazz");
        assert_eq!(Genre::HipHop.as_str(), "Hip-Hop");
        assert_eq!(Genre::Ambient.as_str(), "Ambient");
        assert_eq!(Genre::Unknown.as_str(), "Unknown");
    }
}
