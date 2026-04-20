use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::types::{AppError, WaveformData};

/// Seconds of audio decoded before emitting a progressive chunk event.
const CHUNK_SECS: f64 = 20.0;

/// Decode an audio file, emitting `waveform_chunk` events every ~20 s of decoded audio
/// so the frontend can render progressively. The final complete `WaveformData` is returned
/// (and the caller emits `waveform_ready`).
///
/// Each `waveform_chunk` payload is a `WaveformChunk` with the columns decoded so far,
/// expressed as a fraction of the total `width` proportional to how much of the track
/// has been decoded.
pub fn generate_waveform<F>(
    path: &str,
    width: usize,
    track_id: i64,
    emit_chunk: F,
) -> Result<WaveformData, AppError>
where
    F: Fn(WaveformChunk),
{
    if width == 0 {
        return Ok(WaveformData {
            track_id,
            samples_per_channel: Vec::new(),
            rms_per_column: Vec::new(),
            channels: 0,
            duration_secs: 0.0,
        });
    }

    let file = File::open(Path::new(path)).map_err(|e| AppError::Io(e.to_string()))?;
    let mss  = MediaSourceStream::new(Box::new(file), Default::default());

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
        .ok_or_else(|| AppError::Decode("No audio track found".to_string()))?;

    let track_id_sym = track.id;
    let codec_params = &track.codec_params;

    let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);

    let duration_secs = match (codec_params.n_frames, codec_params.time_base) {
        (Some(frames), Some(tb)) => frames as f64 * tb.numer as f64 / tb.denom as f64,
        _ => 0.0,
    };

    let sample_rate = codec_params.sample_rate.unwrap_or(44100) as f64;

    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| AppError::Decode(e.to_string()))?;

    let mut all_samples: Vec<f32> = Vec::new();
    // How many raw samples correspond to one chunk window
    let chunk_samples = (CHUNK_SECS * sample_rate) as usize;
    let mut last_chunk_at: usize = 0; // raw sample count at last emit

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => { decoder.reset(); continue; }
            Err(e) => return Err(AppError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id_sym { continue; }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                all_samples.extend_from_slice(&audio_buffer_to_mono_peaks(&decoded));

                // Emit a chunk every CHUNK_SECS of decoded audio
                if all_samples.len() >= last_chunk_at + chunk_samples {
                    last_chunk_at = all_samples.len();

                    // How many columns does this decoded portion represent?
                    // We know total duration, so we can proportion the columns.
                    let decoded_secs = all_samples.len() as f64 / sample_rate;
                    let cols_ready = if duration_secs > 0.0 {
                        ((decoded_secs / duration_secs) * width as f64).round() as usize
                    } else {
                        width
                    };
                    let cols_ready = cols_ready.min(width);

                    if cols_ready > 0 {
                        let peak = downsample_peaks(&all_samples, cols_ready);
                        let rms  = downsample_rms(&all_samples, cols_ready);
                        emit_chunk(WaveformChunk {
                            track_id,
                            peak,
                            rms,
                            cols_ready,
                            total_cols: width,
                            duration_secs,
                        });
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(AppError::Decode(e.to_string())),
        }
    }

    let samples_per_channel = downsample_peaks(&all_samples, width);
    let rms_per_column      = downsample_rms(&all_samples, width);

    Ok(WaveformData {
        track_id,
        samples_per_channel,
        rms_per_column,
        channels,
        duration_secs,
    })
}

/// Payload emitted for each progressive chunk.
#[derive(serde::Serialize, Clone)]
pub struct WaveformChunk {
    pub track_id:     i64,
    pub peak:         Vec<f32>,
    pub rms:          Vec<f32>,
    /// Number of columns populated in peak/rms (= peak.len()).
    pub cols_ready:   usize,
    /// Total columns that will exist when fully loaded.
    pub total_cols:   usize,
    pub duration_secs: f64,
}

// ─── Audio helpers ────────────────────────────────────────────────────────────

/// Convert any `AudioBufferRef` to per-frame peak amplitudes (max abs across channels).
fn audio_buffer_to_mono_peaks(buf: &AudioBufferRef<'_>) -> Vec<f32> {
    use symphonia::core::audio::AudioBufferRef::*;
    macro_rules! extract {
        ($b:expr, $scale:expr) => {{
            let frames = $b.frames();
            let ch = $b.spec().channels.count();
            (0..frames)
                .map(|f| (0..ch).map(|c| ($b.chan(c)[f] as f32 * $scale).abs()).fold(0.0_f32, f32::max))
                .collect()
        }};
    }
    match buf {
        F32(b) => extract!(b, 1.0),
        F64(b) => extract!(b, 1.0),
        S32(b) => extract!(b, 1.0 / i32::MAX as f32),
        S24(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| (0..ch).map(|c| (b.chan(c)[f].inner() as f32 / 8_388_607.0).abs()).fold(0.0_f32, f32::max))
                .collect()
        }
        S16(b) => extract!(b, 1.0 / i16::MAX as f32),
        U8(b)  => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| (0..ch).map(|c| ((b.chan(c)[f] as f32 - 128.0) / 128.0).abs()).fold(0.0_f32, f32::max))
                .collect()
        }
        _ => Vec::new(),
    }
}

// ─── Downsamplers ─────────────────────────────────────────────────────────────

pub fn downsample_peaks(samples: &[f32], width: usize) -> Vec<f32> {
    if width == 0 { return Vec::new(); }
    let total = samples.len();
    if total == 0 { return vec![0.0; width]; }
    (0..width).map(|i| {
        let start = (i * total) / width;
        let end   = ((i + 1) * total) / width;
        if start >= end { samples.get(start).copied().unwrap_or(0.0) }
        else { samples[start..end].iter().copied().fold(0.0_f32, f32::max) }
    }).collect()
}

pub fn downsample_rms(samples: &[f32], width: usize) -> Vec<f32> {
    if width == 0 { return Vec::new(); }
    let total = samples.len();
    if total == 0 { return vec![0.0; width]; }
    (0..width).map(|i| {
        let start = (i * total) / width;
        let end   = ((i + 1) * total) / width;
        if start >= end { samples.get(start).copied().unwrap_or(0.0) }
        else {
            let b = &samples[start..end];
            (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt()
        }
    }).collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downsample_exact_length() {
        let s: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        assert_eq!(downsample_peaks(&s, 100).len(), 100);
    }

    #[test]
    fn test_downsample_empty_pads_zeros() {
        let r = downsample_peaks(&[], 50);
        assert_eq!(r.len(), 50);
        assert!(r.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_downsample_width_zero() {
        assert!(downsample_peaks(&vec![0.5_f32; 100], 0).is_empty());
    }

    #[test]
    fn test_downsample_more_pixels_than_samples() {
        let s: Vec<f32> = (0..10).map(|i| i as f32 / 10.0).collect();
        assert_eq!(downsample_peaks(&s, 100).len(), 100);
    }

    #[test]
    fn test_downsample_peak_is_max() {
        let mut s = vec![0.5_f32; 1000];
        s[500] = 0.9;
        let r = downsample_peaks(&s, 10);
        assert!((r.iter().copied().fold(0.0_f32, f32::max) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn test_rms_lower_than_peak() {
        let s = vec![0.0_f32, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0];
        assert!(downsample_rms(&s, 1)[0] < downsample_peaks(&s, 1)[0]);
    }
}
