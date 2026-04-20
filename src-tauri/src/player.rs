use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use rustfft::{FftPlanner, num_complex::Complex};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use tauri::{AppHandle, Emitter};

use crate::types::AppError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaybackPositionPayload {
    pub position_secs: f64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaybackStatePayload {
    pub state: PlayerState,
    pub track_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Internal control messages
// ---------------------------------------------------------------------------

enum ControlMsg {
    Play {
        path: String,
        track_id: i64,
        start_pos: f64,
    },
    Pause,
    Resume,
    Stop,
    Seek(f64),
    SetVolume(f32),
}

// ---------------------------------------------------------------------------
// Player handle (public API)
// ---------------------------------------------------------------------------

/// Cheap-to-clone handle that sends commands to the background player thread.
#[derive(Clone)]
pub struct PlayerHandle {
    tx: std::sync::mpsc::SyncSender<ControlMsg>,
}

impl PlayerHandle {
    pub fn play_track(&self, path: String, track_id: i64, start_pos: f64) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::Play {
                path,
                track_id,
                start_pos,
            })
            .map_err(|e| AppError::Decode(e.to_string()))
    }

    pub fn pause(&self) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::Pause)
            .map_err(|e| AppError::Decode(e.to_string()))
    }

    pub fn resume(&self) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::Resume)
            .map_err(|e| AppError::Decode(e.to_string()))
    }

    pub fn stop(&self) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::Stop)
            .map_err(|e| AppError::Decode(e.to_string()))
    }

    pub fn seek(&self, position_secs: f64) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::Seek(position_secs))
            .map_err(|e| AppError::Decode(e.to_string()))
    }

    pub fn set_volume(&self, level: f32) -> Result<(), AppError> {
        self.tx
            .send(ControlMsg::SetVolume(level))
            .map_err(|e| AppError::Decode(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Player struct (managed Tauri state)
// ---------------------------------------------------------------------------

pub struct Player {
    handle: PlayerHandle,
    state: Arc<Mutex<PlayerState>>,
    current_track_id: Arc<Mutex<Option<i64>>>,
}

impl Player {
    /// Spawn the background decode/playback thread and return a `Player`.
    pub fn new(app_handle: AppHandle) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<ControlMsg>(32);
        let state = Arc::new(Mutex::new(PlayerState::Stopped));
        let current_track_id: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));

        let state_clone = Arc::clone(&state);
        let track_id_clone = Arc::clone(&current_track_id);

        std::thread::spawn(move || {
            player_loop(rx, app_handle, state_clone, track_id_clone);
        });

        Player {
            handle: PlayerHandle { tx },
            state,
            current_track_id,
        }
    }

    pub fn handle(&self) -> &PlayerHandle {
        &self.handle
    }

    pub fn state(&self) -> PlayerState {
        self.state.lock().unwrap().clone()
    }

    pub fn current_track_id(&self) -> Option<i64> {
        *self.current_track_id.lock().unwrap()
    }
}

// ---------------------------------------------------------------------------
// Background player loop
// ---------------------------------------------------------------------------

fn player_loop(
    rx: std::sync::mpsc::Receiver<ControlMsg>,
    app_handle: AppHandle,
    state: Arc<Mutex<PlayerState>>,
    current_track_id: Arc<Mutex<Option<i64>>>,
) {
    let audio_buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(48000 * 2)));
    let volume: Arc<Mutex<f32>> = Arc::new(Mutex::new(1.0));
    let stream_active: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let mut _stream: Option<Stream> = None;
    let mut decode_ctx: Option<DecodeContext> = None;

    // Wall-clock anchor for smooth position interpolation in the frontend.
    // We store the last *confirmed* decode position + the instant it was set.
    // The frontend uses performance.now() to interpolate between events.
    let position_secs: Arc<Mutex<f64>> = Arc::new(Mutex::new(0.0));
    let duration_secs: Arc<Mutex<f64>> = Arc::new(Mutex::new(0.0));
    let play_started_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let mut last_spectrum_emit = Instant::now();

    // Spawn a dedicated position-emit thread that fires at ~60fps.
    {
        let position_secs = Arc::clone(&position_secs);
        let duration_secs = Arc::clone(&duration_secs);
        let play_started_at = Arc::clone(&play_started_at);
        let state = Arc::clone(&state);
        let app_handle = app_handle.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(16)); // ~60fps
                if *state.lock().unwrap() != PlayerState::Playing {
                    continue;
                }
                let base_pos = *position_secs.lock().unwrap();
                let dur = *duration_secs.lock().unwrap();
                // Interpolate: add elapsed time since last decode position update.
                let elapsed = play_started_at
                    .lock()
                    .unwrap()
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0);
                let pos = (base_pos + elapsed).min(dur);
                let _ = app_handle.emit(
                    "playback_position",
                    PlaybackPositionPayload {
                        position_secs: pos,
                        duration_secs: dur,
                    },
                );
            }
        });
    }

    loop {
        let msg = if decode_ctx.is_some() && *state.lock().unwrap() == PlayerState::Playing {
            rx.try_recv().ok().map(Some).unwrap_or(None)
        } else {
            rx.recv().ok().map(Some).unwrap_or(None)
        };

        if let Some(msg) = msg {
            match msg {
                ControlMsg::Play { path, track_id, start_pos } => {
                    *stream_active.lock().unwrap() = false;
                    _stream = None;
                    decode_ctx = None;
                    audio_buf.lock().unwrap().clear();

                    match open_decode_context(&path, start_pos) {
                        Ok(ctx) => {
                            let dur = ctx.duration_secs;
                            *duration_secs.lock().unwrap() = dur;
                            *position_secs.lock().unwrap() = start_pos;
                            *play_started_at.lock().unwrap() = Some(Instant::now());
                            *current_track_id.lock().unwrap() = Some(track_id);

                            match build_cpal_stream(
                                ctx.sample_rate,
                                ctx.channels,
                                Arc::clone(&audio_buf),
                                Arc::clone(&volume),
                                Arc::clone(&stream_active),
                            ) {
                                Ok(stream) => {
                                    *stream_active.lock().unwrap() = true;
                                    let _ = stream.play();
                                    _stream = Some(stream);
                                    decode_ctx = Some(ctx);
                                    *state.lock().unwrap() = PlayerState::Playing;
                                    emit_state_changed(&app_handle, PlayerState::Playing, Some(track_id));
                                }
                                Err(e) => {
                                    *current_track_id.lock().unwrap() = None;
                                    *play_started_at.lock().unwrap() = None;
                                    *state.lock().unwrap() = PlayerState::Stopped;
                                    emit_state_changed(&app_handle, PlayerState::Stopped, None);
                                    eprintln!("cpal stream error: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            *current_track_id.lock().unwrap() = None;
                            *play_started_at.lock().unwrap() = None;
                            *state.lock().unwrap() = PlayerState::Stopped;
                            emit_state_changed(&app_handle, PlayerState::Stopped, None);
                            eprintln!("Decode error: {e}");
                        }
                    }
                }

                ControlMsg::Pause => {
                    if *state.lock().unwrap() == PlayerState::Playing {
                        // Snapshot interpolated position before pausing.
                        let elapsed = play_started_at.lock().unwrap()
                            .map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
                        *position_secs.lock().unwrap() += elapsed;
                        *play_started_at.lock().unwrap() = None;

                        *stream_active.lock().unwrap() = false;
                        if let Some(s) = &_stream { let _ = s.pause(); }
                        *state.lock().unwrap() = PlayerState::Paused;
                        let tid = *current_track_id.lock().unwrap();
                        emit_state_changed(&app_handle, PlayerState::Paused, tid);
                    }
                }

                ControlMsg::Resume => {
                    if *state.lock().unwrap() == PlayerState::Paused {
                        *play_started_at.lock().unwrap() = Some(Instant::now());
                        *stream_active.lock().unwrap() = true;
                        if let Some(s) = &_stream { let _ = s.play(); }
                        *state.lock().unwrap() = PlayerState::Playing;
                        let tid = *current_track_id.lock().unwrap();
                        emit_state_changed(&app_handle, PlayerState::Playing, tid);
                    }
                }

                ControlMsg::Stop => {
                    *stream_active.lock().unwrap() = false;
                    _stream = None;
                    decode_ctx = None;
                    audio_buf.lock().unwrap().clear();
                    *position_secs.lock().unwrap() = 0.0;
                    *play_started_at.lock().unwrap() = None;
                    *current_track_id.lock().unwrap() = None;
                    *state.lock().unwrap() = PlayerState::Stopped;
                    emit_state_changed(&app_handle, PlayerState::Stopped, None);
                }

                ControlMsg::Seek(pos) => {
                    if let Some(ctx) = &mut decode_ctx {
                        audio_buf.lock().unwrap().clear();
                        let seek_time = SeekTo::Time { time: Time::from(pos), track_id: None };
                        if ctx.format.seek(SeekMode::Accurate, seek_time).is_ok() {
                            ctx.decoder.reset();
                            *position_secs.lock().unwrap() = pos;
                            *play_started_at.lock().unwrap() = Some(Instant::now());
                            let dur = *duration_secs.lock().unwrap();
                            let tid = *current_track_id.lock().unwrap();
                            let _ = app_handle.emit("playback_position", PlaybackPositionPayload {
                                position_secs: pos,
                                duration_secs: dur,
                            });
                            let st = state.lock().unwrap().clone();
                            emit_state_changed(&app_handle, st, tid);
                        }
                    }
                }

                ControlMsg::SetVolume(v) => {
                    *volume.lock().unwrap() = v.clamp(0.0, 1.0);
                }
            }
        }

        // Feed the audio buffer while playing.
        if *state.lock().unwrap() == PlayerState::Playing {
            if let Some(ctx) = &mut decode_ctx {
                let buf_len = audio_buf.lock().unwrap().len();
                let target = ctx.channels as usize * 8192;

                if buf_len < target {
                    match decode_next_packet(ctx) {
                        Ok(Some(samples)) => {
                            // Update the decode position anchor.
                            let frames = samples.len() / ctx.channels as usize;
                            let delta = frames as f64 / ctx.sample_rate as f64;
                            {
                                let mut pos = position_secs.lock().unwrap();
                                *pos += delta;
                                // Reset the wall-clock anchor so interpolation
                                // stays accurate after each decode burst.
                                *play_started_at.lock().unwrap() = Some(Instant::now());
                            }

                            // Emit real FFT spectrum + per-channel RMS at ~60fps
                            if last_spectrum_emit.elapsed() >= Duration::from_millis(16) {
                                let bins = compute_spectrum(&samples, ctx.channels as usize);
                                let (rms_l, rms_r) = compute_rms(&samples, ctx.channels as usize);
                                let _ = app_handle.emit("spectrum_data", SpectrumDataPayload { bins, rms_l, rms_r });
                                last_spectrum_emit = Instant::now();
                            }

                            audio_buf.lock().unwrap().extend_from_slice(&samples);
                        }
                        Ok(None) => {
                            let buf_empty = audio_buf.lock().unwrap().is_empty();
                            if buf_empty {
                                *stream_active.lock().unwrap() = false;
                                _stream = None;
                                decode_ctx = None;
                                *position_secs.lock().unwrap() = 0.0;
                                *play_started_at.lock().unwrap() = None;
                                *current_track_id.lock().unwrap() = None;
                                *state.lock().unwrap() = PlayerState::Stopped;
                                emit_state_changed(&app_handle, PlayerState::Stopped, None);
                            }
                        }
                        Err(e) => {
                            eprintln!("Decode error during playback: {e}");
                            *stream_active.lock().unwrap() = false;
                            _stream = None;
                            decode_ctx = None;
                            *play_started_at.lock().unwrap() = None;
                            *current_track_id.lock().unwrap() = None;
                            *state.lock().unwrap() = PlayerState::Stopped;
                            emit_state_changed(&app_handle, PlayerState::Stopped, None);
                        }
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FFT spectrum emission
// ---------------------------------------------------------------------------

const FFT_SIZE: usize = 2048;
const SPECTRUM_BINS: usize = 128; // bins sent to frontend

/// Compute a Hann-windowed FFT on interleaved f32 samples (mixed to mono),
/// convert magnitudes to dB, downsample to SPECTRUM_BINS, and return as Vec<f32>.
fn compute_spectrum(samples: &[f32], channels: usize) -> Vec<f32> {
    if samples.is_empty() || channels == 0 {
        return vec![0.0; SPECTRUM_BINS];
    }

    // Mix to mono and take up to FFT_SIZE frames
    let frames = (samples.len() / channels).min(FFT_SIZE);
    let mut mono: Vec<Complex<f32>> = Vec::with_capacity(FFT_SIZE);

    for f in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += samples[f * channels + c];
        }
        // Hann window
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * f as f32 / (FFT_SIZE - 1) as f32).cos());
        mono.push(Complex { re: (sum / channels as f32) * w, im: 0.0 });
    }

    // Zero-pad to FFT_SIZE
    while mono.len() < FFT_SIZE {
        mono.push(Complex { re: 0.0, im: 0.0 });
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    fft.process(&mut mono);

    // Magnitude in dB for the positive half (FFT_SIZE/2 bins)
    let half = FFT_SIZE / 2;
    let scale = 1.0 / FFT_SIZE as f32;

    // Downsample to SPECTRUM_BINS using logarithmic mapping
    let mut out = vec![0.0f32; SPECTRUM_BINS];
    let log_min = (1.0f32).ln();
    let log_max = (half as f32).ln();

    for i in 0..SPECTRUM_BINS {
        let t = i as f32 / (SPECTRUM_BINS - 1) as f32;
        let log_idx = (log_min + t * (log_max - log_min)).exp();
        let lo = (log_idx.floor() as usize).min(half - 1);
        let hi = (lo + 1).min(half - 1);
        let frac = log_idx - log_idx.floor();

        let mag_lo = (mono[lo].norm() * scale).max(1e-10);
        let mag_hi = (mono[hi].norm() * scale).max(1e-10);
        let mag = mag_lo * (1.0 - frac) + mag_hi * frac;

        // Convert to dB, normalise to [0, 1] range (roughly -90 dB to 0 dB)
        let db = 20.0 * mag.log10();
        out[i] = ((db + 90.0) / 90.0).clamp(0.0, 1.0);
    }

    out
}

#[derive(serde::Serialize, Clone)]
struct SpectrumDataPayload {
    bins: Vec<f32>, // SPECTRUM_BINS normalised values 0–1
    rms_l: f32,     // RMS of left channel (or mono), 0–1
    rms_r: f32,     // RMS of right channel (or mono if mono), 0–1
}

/// Compute per-channel RMS from interleaved samples, normalised to [0, 1].
/// Returns (rms_l, rms_r). For mono, both are the same value.
fn compute_rms(samples: &[f32], channels: usize) -> (f32, f32) {
    if samples.is_empty() || channels == 0 {
        return (0.0, 0.0);
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return (0.0, 0.0);
    }

    let ch_l = 0usize;
    let ch_r = if channels >= 2 { 1 } else { 0 };

    let mut sum_l = 0.0f64;
    let mut sum_r = 0.0f64;
    for f in 0..frames {
        let sl = samples[f * channels + ch_l] as f64;
        let sr = samples[f * channels + ch_r] as f64;
        sum_l += sl * sl;
        sum_r += sr * sr;
    }

    let rms_l = (sum_l / frames as f64).sqrt() as f32;
    let rms_r = (sum_r / frames as f64).sqrt() as f32;

    // Normalise: RMS of a full-scale sine is ~0.707; scale so that maps to ~1.0
    (rms_l.min(1.0) * 1.414, rms_r.min(1.0) * 1.414)
}

struct DecodeContext {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
    duration_secs: f64,
}

fn open_decode_context(path: &str, start_pos: f64) -> Result<DecodeContext, AppError> {
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

    let format = probed.format;

    // Pick the first audio track.
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AppError::Decode("No audio track found".to_string()))?;

    let track_id = track.id;
    let codec_params = &track.codec_params;

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| AppError::Decode("Unknown sample rate".to_string()))?;

    let channels = codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    // Compute duration.
    let duration_secs = match (codec_params.n_frames, codec_params.time_base) {
        (Some(frames), Some(tb)) => frames as f64 * tb.numer as f64 / tb.denom as f64,
        _ => 0.0,
    };

    let decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| AppError::Decode(e.to_string()))?;

    let mut ctx = DecodeContext {
        format,
        decoder,
        track_id,
        sample_rate,
        channels,
        duration_secs,
    };

    // Seek to start position if non-zero.
    if start_pos > 0.0 {
        let seek_time = SeekTo::Time {
            time: Time::from(start_pos),
            track_id: None,
        };
        let _ = ctx.format.seek(SeekMode::Accurate, seek_time);
    }

    Ok(ctx)
}

/// Decode the next packet and return interleaved f32 samples, or `None` at EOF.
fn decode_next_packet(ctx: &mut DecodeContext) -> Result<Option<Vec<f32>>, AppError> {
    loop {
        let packet = match ctx.format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return Ok(None);
            }
            Err(SymphoniaError::ResetRequired) => {
                ctx.decoder.reset();
                continue;
            }
            Err(e) => return Err(AppError::Decode(e.to_string())),
        };

        if packet.track_id() != ctx.track_id {
            continue;
        }

        match ctx.decoder.decode(&packet) {
            Ok(decoded) => {
                let samples = audio_buffer_to_f32(&decoded);
                return Ok(Some(samples));
            }
            Err(SymphoniaError::DecodeError(_)) => {
                // Skip corrupt packets.
                continue;
            }
            Err(e) => return Err(AppError::Decode(e.to_string())),
        }
    }
}

/// Convert any `AudioBufferRef` to interleaved f32 samples.
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
// cpal stream builder
// ---------------------------------------------------------------------------

fn build_cpal_stream(
    sample_rate: u32,
    channels: u16,
    audio_buf: Arc<Mutex<Vec<f32>>>,
    volume: Arc<Mutex<f32>>,
    active: Arc<Mutex<bool>>,
) -> Result<Stream, AppError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AppError::Decode("No audio output device found".to_string()))?;

    // Always use the device's native config to avoid sample-rate mismatch
    // (which causes sped-up / slowed-down audio).
    let default_config = device
        .default_output_config()
        .map_err(|e| AppError::Decode(e.to_string()))?;

    let device_rate = default_config.sample_rate().0;
    let device_channels = default_config.channels() as u16;

    let stream_config = cpal::StreamConfig {
        channels: device_channels,
        sample_rate: cpal::SampleRate(device_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // Resample ratio: how many source frames per output frame.
    let resample_ratio = sample_rate as f64 / device_rate as f64;
    let src_channels = channels as usize;
    let dst_channels = device_channels as usize;

    // Fractional position in the source stream for linear resampling.
    let resample_pos: Arc<Mutex<f64>> = Arc::new(Mutex::new(0.0));

    let stream = device
        .build_output_stream(
            &stream_config,
            move |data: &mut [f32], _| {
                if !*active.lock().unwrap() {
                    for s in data.iter_mut() { *s = 0.0; }
                    return;
                }
                let vol = *volume.lock().unwrap();
                let mut buf = audio_buf.lock().unwrap();
                let mut pos = resample_pos.lock().unwrap();

                let output_frames = data.len() / dst_channels;

                for out_frame in 0..output_frames {
                    // Source frame index (fractional)
                    let src_frac = *pos;
                    let src_lo = src_frac.floor() as usize;
                    let src_hi = src_lo + 1;
                    let t = src_frac - src_lo as f64;

                    for dst_ch in 0..dst_channels {
                        // Map destination channel to source channel
                        let src_ch = if src_channels == 1 { 0 } else { dst_ch.min(src_channels - 1) };

                        let idx_lo = src_lo * src_channels + src_ch;
                        let idx_hi = src_hi * src_channels + src_ch;

                        let s_lo = buf.get(idx_lo).copied().unwrap_or(0.0);
                        let s_hi = buf.get(idx_hi).copied().unwrap_or(s_lo);
                        let sample = s_lo + (s_hi - s_lo) * t as f32;

                        data[out_frame * dst_channels + dst_ch] = sample * vol;
                    }

                    *pos += resample_ratio;
                }

                // Drain consumed source frames
                let consumed = pos.floor() as usize;
                let drain = (consumed * src_channels).min(buf.len());
                if drain > 0 {
                    buf.drain(..drain);
                    *pos -= consumed as f64;
                }
            },
            |e| eprintln!("cpal stream error: {e}"),
            None,
        )
        .map_err(|e| AppError::Decode(e.to_string()))?;

    Ok(stream)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_state_changed(app_handle: &AppHandle, state: PlayerState, track_id: Option<i64>) {
    let _ = app_handle.emit(
        "playback_state_changed",
        PlaybackStatePayload { state, track_id },
    );
}
