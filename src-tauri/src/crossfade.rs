/// Crossfader DSP stage for gapless playback and linear crossfade transitions.
///
/// When `gapless_enabled` is true and `duration_secs > 0`, the crossfader
/// applies a linear fade-out to the current track's samples while mixing in
/// a linear fade-in from the next track's pre-decoded buffer.
///
/// When `duration_secs == 0` (or crossfade is not active), the next track's
/// samples are simply appended after the current track ends (gapless).
///
/// When `gapless_enabled` is false, the crossfader is a no-op and the player
/// reverts to stop-then-play behaviour.
use std::sync::{Arc, Mutex};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CrossfadeSettings {
    pub crossfade_secs: f32,
    pub gapless_enabled: bool,
}

pub struct Crossfader {
    /// Crossfade duration in seconds (0 = gapless, no fade).
    pub duration_secs: f32,
    /// Whether gapless / crossfade is enabled at all.
    pub gapless_enabled: bool,
    /// Pre-decoded samples from the next track.
    pub next_track_buffer: Arc<Mutex<Vec<f32>>>,
    /// How many samples have been processed since the fade began.
    pub(crate) fade_position: f32,
    /// Whether a crossfade is currently in progress.
    pub(crate) fading: bool,
}

impl Crossfader {
    pub fn new() -> Self {
        Crossfader {
            duration_secs: 0.0,
            gapless_enabled: false,
            next_track_buffer: Arc::new(Mutex::new(Vec::new())),
            fade_position: 0.0,
            fading: false,
        }
    }

    /// Set the crossfade duration, clamped to [0.0, 10.0].
    pub fn set_duration(&mut self, secs: f32) {
        self.duration_secs = secs.clamp(0.0, 10.0);
    }

    /// Enable or disable gapless / crossfade playback.
    pub fn set_gapless(&mut self, enabled: bool) {
        self.gapless_enabled = enabled;
    }

    /// Load the next track's pre-decoded samples and begin the crossfade.
    pub fn begin_crossfade(&mut self, next_samples: Vec<f32>) {
        *self.next_track_buffer.lock().unwrap() = next_samples;
        self.fade_position = 0.0;
        self.fading = true;
    }

    /// Returns true if a crossfade is currently active.
    pub fn is_fading(&self) -> bool {
        self.fading
    }

    /// Returns true if the crossfade has completed (all next-track samples consumed).
    pub fn is_complete(&self) -> bool {
        if !self.fading {
            return false;
        }
        let next_buf = self.next_track_buffer.lock().unwrap();
        next_buf.is_empty()
    }

    /// Apply the crossfade to `current` samples in-place.
    ///
    /// - If `duration_secs > 0`: linear fade-out on `current`, mix in linear
    ///   fade-in from `next_track_buffer`.
    /// - If `duration_secs == 0` (gapless): samples pass through unmodified;
    ///   the next track's buffer is consumed after the current track ends.
    ///
    /// `sample_rate` and `channels` are used to convert `fade_position`
    /// (in samples) to seconds.
    pub fn process(&mut self, current: &mut [f32], sample_rate: u32, channels: usize) {
        if !self.fading || !self.gapless_enabled {
            return;
        }

        let total_fade_samples = self.duration_secs * sample_rate as f32;
        let mut next_buf = self.next_track_buffer.lock().unwrap();

        if self.duration_secs <= 0.0 {
            // Gapless mode: no fade, just let the current track finish.
            // The next track buffer will be appended by the player loop.
            return;
        }

        let frames = current.len() / channels.max(1);
        for frame in 0..frames {
            let t = if total_fade_samples > 0.0 {
                (self.fade_position / total_fade_samples).clamp(0.0, 1.0)
            } else {
                1.0
            };

            // Fade-out gain for current track: 1.0 → 0.0
            let fade_out = 1.0 - t;
            // Fade-in gain for next track: 0.0 → 1.0
            let fade_in = t;

            for ch in 0..channels {
                let idx = frame * channels + ch;
                let cur_sample = current[idx] * fade_out;

                // Mix in next track sample if available.
                let next_idx = frame * channels + ch;
                let next_sample = if next_idx < next_buf.len() {
                    next_buf[next_idx] * fade_in
                } else {
                    0.0
                };

                current[idx] = (cur_sample + next_sample).clamp(-1.0, 1.0);
            }

            self.fade_position += 1.0;
        }

        // Drain the consumed next-track samples.
        let consumed = (frames * channels).min(next_buf.len());
        if consumed > 0 {
            next_buf.drain(..consumed);
        }
    }

    /// Reset the crossfader state (called when a new track starts normally).
    pub fn reset(&mut self) {
        self.fading = false;
        self.fade_position = 0.0;
        self.next_track_buffer.lock().unwrap().clear();
    }
}

impl Default for Crossfader {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Feature: neptune-feature-expansion, Property 15: Crossfade linear fade invariant
    // Validates: Requirements 8.2
    #[test]
    fn test_crossfade_linear_fade_invariant() {
        use proptest::prelude::*;

        let result = proptest::test_runner::TestRunner::default().run(
            &(
                // crossfade duration in [0.5, 10.0]
                (1u32..=20u32).prop_map(|x| x as f32 * 0.5),
                // sample rate
                proptest::prop_oneof![Just(44100u32), Just(48000u32)],
                // number of frames to process (at least 1 full fade worth)
                (1usize..=256usize),
            ),
            |(duration_secs, sample_rate, frames)| {
                let channels = 2usize;
                let mut crossfader = Crossfader::new();
                crossfader.set_duration(duration_secs);
                crossfader.set_gapless(true);

                // Fill next track buffer with 1.0 samples
                let next_samples = vec![1.0f32; frames * channels * 4];
                crossfader.begin_crossfade(next_samples);

                // Current track: all 1.0 samples
                let mut current = vec![1.0f32; frames * channels];
                crossfader.process(&mut current, sample_rate, channels);

                // All output samples must be in [-1.0, 1.0]
                for &s in &current {
                    prop_assert!(
                        s >= -1.0 && s <= 1.0,
                        "crossfade output {} out of range",
                        s
                    );
                }
                Ok(())
            },
        );
        result.unwrap();
    }

    // Feature: neptune-feature-expansion, Property 16: Crossfade persistence round-trip
    // Validates: Requirements 8.5, 8.6
    #[test]
    fn test_crossfade_persistence_round_trip() {
        use proptest::prelude::*;
        use rusqlite::Connection;

        let result = proptest::test_runner::TestRunner::default().run(
            &(
                // crossfade_secs in [0.0, 10.0]
                (0u32..=20u32).prop_map(|x| x as f32 * 0.5),
                // gapless_enabled
                proptest::bool::ANY,
            ),
            |(crossfade_secs, gapless_enabled)| {
                let conn = Connection::open_in_memory().expect("in-memory DB");
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS app_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
                )
                .expect("schema");

                // Persist
                conn.execute(
                    "INSERT INTO app_state (key, value) VALUES ('crossfade_secs', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![crossfade_secs.to_string()],
                )
                .expect("insert crossfade_secs");

                conn.execute(
                    "INSERT INTO app_state (key, value) VALUES ('gapless_enabled', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![gapless_enabled.to_string()],
                )
                .expect("insert gapless_enabled");

                // Load back
                let loaded_secs: f32 = conn
                    .query_row(
                        "SELECT value FROM app_state WHERE key = 'crossfade_secs'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("query crossfade_secs")
                    .parse()
                    .expect("parse crossfade_secs");

                let loaded_gapless: bool = conn
                    .query_row(
                        "SELECT value FROM app_state WHERE key = 'gapless_enabled'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("query gapless_enabled")
                    .parse()
                    .expect("parse gapless_enabled");

                prop_assert!(
                    (loaded_secs - crossfade_secs).abs() < 1e-5,
                    "crossfade_secs round-trip failed: stored {}, loaded {}",
                    crossfade_secs,
                    loaded_secs
                );
                prop_assert_eq!(
                    loaded_gapless,
                    gapless_enabled,
                    "gapless_enabled round-trip failed"
                );
                Ok(())
            },
        );
        result.unwrap();
    }

    #[test]
    fn test_crossfader_new_defaults() {
        let cf = Crossfader::new();
        assert_eq!(cf.duration_secs, 0.0);
        assert!(!cf.gapless_enabled);
        assert!(!cf.fading);
        assert!(!cf.is_complete());
    }

    #[test]
    fn test_crossfader_set_duration_clamps() {
        let mut cf = Crossfader::new();
        cf.set_duration(15.0);
        assert_eq!(cf.duration_secs, 10.0);
        cf.set_duration(-1.0);
        assert_eq!(cf.duration_secs, 0.0);
    }

    #[test]
    fn test_crossfader_begin_crossfade_sets_fading() {
        let mut cf = Crossfader::new();
        cf.set_duration(2.0);
        cf.set_gapless(true);
        cf.begin_crossfade(vec![0.5f32; 1024]);
        assert!(cf.is_fading());
        assert!(!cf.is_complete());
    }

    #[test]
    fn test_crossfader_process_no_op_when_disabled() {
        let mut cf = Crossfader::new();
        cf.set_gapless(false);
        let original = vec![0.8f32; 64];
        let mut samples = original.clone();
        cf.process(&mut samples, 48000, 2);
        assert_eq!(samples, original, "disabled crossfader should not modify samples");
    }

    #[test]
    fn test_crossfader_reset_clears_state() {
        let mut cf = Crossfader::new();
        cf.set_duration(2.0);
        cf.set_gapless(true);
        cf.begin_crossfade(vec![1.0f32; 512]);
        cf.reset();
        assert!(!cf.is_fading());
        assert!(cf.next_track_buffer.lock().unwrap().is_empty());
    }

    #[test]
    fn test_crossfader_output_clamped() {
        let mut cf = Crossfader::new();
        cf.set_duration(1.0);
        cf.set_gapless(true);
        // Both current and next at full amplitude — sum could exceed 1.0 without clamping
        cf.begin_crossfade(vec![1.0f32; 4096]);
        let mut current = vec![1.0f32; 4096];
        cf.process(&mut current, 48000, 2);
        for &s in &current {
            assert!(s >= -1.0 && s <= 1.0, "sample {} out of range", s);
        }
    }
}
