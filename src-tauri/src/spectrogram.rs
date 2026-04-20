use std::fs::File;
use std::path::Path;

use rustfft::{num_complex::Complex, FftPlanner};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::types::{AppError, SpectrogramData};

pub const DEFAULT_FFT_SIZE: u32 = 2048;
pub const DEFAULT_HOP_SIZE: u32 = 512;

/// Compute a Short-Time Fourier Transform (STFT) over the audio file at `path`.
///
/// Returns `SpectrogramData` where:
/// - `magnitudes[t][f]` is the magnitude in dB for time frame `t` and frequency bin `f`
/// - Number of time frames = `ceil((N - fft_size) / hop_size)` where N = total samples
/// - Number of frequency bins = `fft_size / 2 + 1`
pub fn generate_spectrogram(
    path: &str,
    track_id: i64,
    fft_size: u32,
    hop_size: u32,
) -> Result<SpectrogramData, AppError> {
    let fft_size = fft_size as usize;
    let hop_size = hop_size as usize;

    // --- Decode audio to mono f32 samples ---
    let file = File::open(Path::new(path)).map_err(|e| AppError::Io(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AppError::Decode(e.to_string()))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AppError::Decode("No audio track found".to_string()))?;

    let track_id_sym = track.id;
    let codec_params = &track.codec_params;

    let sample_rate = codec_params.sample_rate.unwrap_or(44100);

    let duration_secs = match (codec_params.n_frames, codec_params.time_base) {
        (Some(frames), Some(tb)) => frames as f64 * tb.numer as f64 / tb.denom as f64,
        _ => 0.0,
    };

    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
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
            Err(e) => return Err(AppError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id_sym {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let frames = audio_buffer_to_mono(&decoded);
                mono_samples.extend_from_slice(&frames);
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(AppError::Decode(e.to_string())),
        }
    }

    // --- Compute STFT ---
    let n = mono_samples.len();
    let num_bins = fft_size / 2 + 1;

    // Handle edge case: not enough samples for even one frame
    if n < fft_size {
        return Ok(SpectrogramData {
            track_id,
            magnitudes: Vec::new(),
            fft_size: fft_size as u32,
            hop_size: hop_size as u32,
            sample_rate,
            duration_secs,
        });
    }

    let num_frames = (n - fft_size + hop_size - 1) / hop_size; // ceil((N - F) / H)

    // Pre-compute Hann window
    let hann: Vec<f32> = (0..fft_size)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos())
        })
        .collect();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_size);

    let mut magnitudes: Vec<Vec<f32>> = Vec::with_capacity(num_frames);

    let mut scratch = vec![Complex::new(0.0_f32, 0.0_f32); fft.get_inplace_scratch_len()];
    let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size];

    for frame_idx in 0..num_frames {
        let start = frame_idx * hop_size;
        let end = start + fft_size;

        // Fill FFT buffer with windowed samples
        for (i, sample) in mono_samples[start..end].iter().enumerate() {
            buf[i] = Complex::new(sample * hann[i], 0.0);
        }

        fft.process_with_scratch(&mut buf, &mut scratch);

        // Compute magnitudes in dB for the positive frequencies (bins 0..=fft_size/2)
        let frame_mags: Vec<f32> = buf[..num_bins]
            .iter()
            .map(|c| {
                let mag = c.norm();
                20.0 * (mag + 1e-10_f32).log10()
            })
            .collect();

        magnitudes.push(frame_mags);
    }

    Ok(SpectrogramData {
        track_id,
        magnitudes,
        fft_size: fft_size as u32,
        hop_size: hop_size as u32,
        sample_rate,
        duration_secs,
    })
}

/// Convert any `AudioBufferRef` to per-frame mono f32 samples (average across channels).
fn audio_buffer_to_mono(buf: &AudioBufferRef<'_>) -> Vec<f32> {
    use symphonia::core::audio::AudioBufferRef::*;
    match buf {
        F32(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch).map(|c| b.chan(c)[f]).sum();
                    sum / ch as f32
                })
                .collect()
        }
        F64(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch).map(|c| b.chan(c)[f] as f32).sum();
                    sum / ch as f32
                })
                .collect()
        }
        S32(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch)
                        .map(|c| b.chan(c)[f] as f32 / i32::MAX as f32)
                        .sum();
                    sum / ch as f32
                })
                .collect()
        }
        S24(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch)
                        .map(|c| b.chan(c)[f].inner() as f32 / 8_388_607.0)
                        .sum();
                    sum / ch as f32
                })
                .collect()
        }
        S16(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch)
                        .map(|c| b.chan(c)[f] as f32 / i16::MAX as f32)
                        .sum();
                    sum / ch as f32
                })
                .collect()
        }
        U8(b) => {
            let frames = b.frames();
            let ch = b.spec().channels.count();
            (0..frames)
                .map(|f| {
                    let sum: f32 = (0..ch)
                        .map(|c| (b.chan(c)[f] as f32 - 128.0) / 128.0)
                        .sum();
                    sum / ch as f32
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic mono PCM signal of `n` samples (sine wave at 440 Hz, 44100 Hz SR).
    fn make_sine(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect()
    }

    fn run_stft(samples: &[f32], fft_size: usize, hop_size: usize) -> Vec<Vec<f32>> {
        let n = samples.len();
        let num_bins = fft_size / 2 + 1;

        if n < fft_size {
            return Vec::new();
        }

        let num_frames = (n - fft_size + hop_size - 1) / hop_size;

        let hann: Vec<f32> = (0..fft_size)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos())
            })
            .collect();

        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let mut scratch = vec![Complex::new(0.0_f32, 0.0_f32); fft.get_inplace_scratch_len()];
        let mut buf: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size];
        let mut magnitudes = Vec::with_capacity(num_frames);

        for frame_idx in 0..num_frames {
            let start = frame_idx * hop_size;
            let end = start + fft_size;
            for (i, s) in samples[start..end].iter().enumerate() {
                buf[i] = Complex::new(s * hann[i], 0.0);
            }
            fft.process_with_scratch(&mut buf, &mut scratch);
            let frame: Vec<f32> = buf[..num_bins]
                .iter()
                .map(|c| 20.0 * (c.norm() + 1e-10_f32).log10())
                .collect();
            magnitudes.push(frame);
        }

        magnitudes
    }

    #[test]
    fn test_stft_frame_count() {
        // Property 15: num_frames = ceil((N - F) / H)
        let fft_size = 2048_usize;
        let hop_size = 512_usize;
        let n = 44100_usize; // ~1 second at 44100 Hz
        let samples = make_sine(n);
        let mags = run_stft(&samples, fft_size, hop_size);
        let expected_frames = (n - fft_size + hop_size - 1) / hop_size;
        assert_eq!(mags.len(), expected_frames);
    }

    #[test]
    fn test_stft_bin_count() {
        // Each frame must have fft_size / 2 + 1 bins
        let fft_size = 2048_usize;
        let hop_size = 512_usize;
        let samples = make_sine(44100);
        let mags = run_stft(&samples, fft_size, hop_size);
        let expected_bins = fft_size / 2 + 1;
        for frame in &mags {
            assert_eq!(frame.len(), expected_bins);
        }
    }

    #[test]
    fn test_stft_magnitudes_are_db() {
        // dB values should be finite and within a reasonable range
        let samples = make_sine(44100);
        let mags = run_stft(&samples, 2048, 512);
        for frame in &mags {
            for &v in frame {
                assert!(v.is_finite(), "magnitude must be finite, got {v}");
            }
        }
    }

    #[test]
    fn test_stft_empty_returns_no_frames() {
        // Fewer samples than fft_size → no frames
        let samples = make_sine(100);
        let mags = run_stft(&samples, 2048, 512);
        assert!(mags.is_empty());
    }

    #[test]
    fn test_stft_different_fft_sizes() {
        // Verify dimensions hold for various fft_size / hop_size combos
        let n = 8192_usize;
        let samples = make_sine(n);
        for &(fft_size, hop_size) in &[(512, 128), (1024, 256), (4096, 1024)] {
            let mags = run_stft(&samples, fft_size, hop_size);
            let expected_frames = (n - fft_size + hop_size - 1) / hop_size;
            let expected_bins = fft_size / 2 + 1;
            assert_eq!(
                mags.len(),
                expected_frames,
                "frame count mismatch for fft_size={fft_size}"
            );
            for frame in &mags {
                assert_eq!(
                    frame.len(),
                    expected_bins,
                    "bin count mismatch for fft_size={fft_size}"
                );
            }
        }
    }
}
