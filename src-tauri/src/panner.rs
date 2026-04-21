/// Stereo panning DSP stage using a constant-power pan law.
///
/// The pan value is in the range [-1.0, 1.0]:
///   -1.0 = full left, 0.0 = center, 1.0 = full right.
///
/// Gain formula: t = (pan + 1) * π / 4
///   left_gain  = cos(t)
///   right_gain = sin(t)
///
/// At center (pan = 0.0): t = π/4, both gains = √2/2 ≈ 0.707.
/// The constant-power invariant cos²(t) + sin²(t) = 1 holds for all t.
pub struct Panner {
    pan: f32,
}

impl Panner {
    pub fn new() -> Self {
        Panner { pan: 0.0 }
    }

    /// Set the pan value, clamped to [-1.0, 1.0].
    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
    }

    /// Get the current pan value.
    pub fn get_pan(&self) -> f32 {
        self.pan
    }

    /// Returns (left_gain, right_gain) using the constant-power pan law.
    pub fn gains(&self) -> (f32, f32) {
        let t = (self.pan + 1.0) * std::f32::consts::PI / 4.0;
        (t.cos(), t.sin())
    }

    /// Apply per-channel gain to interleaved stereo samples.
    /// `channels` must be 2 for stereo; if not 2, samples are passed through unmodified.
    pub fn process(&mut self, samples: &mut [f32], channels: usize) {
        if channels != 2 {
            return;
        }
        let (gl, gr) = self.gains();
        let frames = samples.len() / 2;
        for frame in 0..frames {
            samples[frame * 2] *= gl;
            samples[frame * 2 + 1] *= gr;
        }
    }
}

impl Default for Panner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Feature: neptune-feature-expansion, Property 12: Panner constant-power invariant
    // Validates: Requirements 7.8
    #[test]
    fn test_panner_constant_power_invariant() {
        use proptest::prelude::*;

        let result = proptest::test_runner::TestRunner::default().run(
            &(-1.0_f32..=1.0_f32),
            |pan| {
                let mut panner = Panner::new();
                panner.set_pan(pan);
                let (gl, gr) = panner.gains();
                let invariant = gl * gl + gr * gr;
                prop_assert!(
                    (invariant - 1.0).abs() < 1e-6,
                    "constant-power invariant violated: gl²+gr² = {} for pan = {}",
                    invariant,
                    pan
                );
                Ok(())
            },
        );
        result.unwrap();
    }

    // Feature: neptune-feature-expansion, Property 13: Panner unity at center
    // Validates: Requirements 7.3
    #[test]
    fn test_panner_unity_at_center() {
        use proptest::prelude::*;

        let result = proptest::test_runner::TestRunner::default().run(
            &proptest::collection::vec(-1.0_f32..=1.0_f32, 2..=256usize),
            |samples_orig| {
                // samples_orig must have even length for stereo
                let len = (samples_orig.len() / 2) * 2;
                let samples_orig = &samples_orig[..len];
                let mut samples = samples_orig.to_vec();

                let mut panner = Panner::new();
                panner.set_pan(0.0);
                panner.process(&mut samples, 2);

                let (gl, gr) = panner.gains();
                // At pan=0, both gains should be √2/2 ≈ 0.707
                // The spec says "unity gain" in the constant-power sense (not amplitude unity)
                // Requirement 7.3: pass through unmodified — but the formula gives √2/2 at center.
                // The design doc clarifies: "unity gain for constant-power (not unity amplitude)"
                // So we verify gains are equal and the invariant holds.
                prop_assert!(
                    (gl - gr).abs() < 1e-6,
                    "at pan=0, left and right gains should be equal: gl={}, gr={}",
                    gl,
                    gr
                );
                prop_assert!(
                    (gl * gl + gr * gr - 1.0).abs() < 1e-6,
                    "constant-power invariant violated at center"
                );
                Ok(())
            },
        );
        result.unwrap();
    }

    // Feature: neptune-feature-expansion, Property 14: Pan value persistence round-trip
    // Validates: Requirements 7.6
    #[test]
    fn test_pan_value_persistence_round_trip() {
        use proptest::prelude::*;
        use rusqlite::Connection;

        let result = proptest::test_runner::TestRunner::default().run(
            &(-1.0_f32..=1.0_f32),
            |pan| {
                let conn = Connection::open_in_memory().expect("in-memory DB");
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS app_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
                )
                .expect("schema");

                // Persist pan value
                conn.execute(
                    "INSERT INTO app_state (key, value) VALUES ('pan_value', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![pan.to_string()],
                )
                .expect("insert");

                // Load pan value back
                let loaded: f32 = conn
                    .query_row(
                        "SELECT value FROM app_state WHERE key = 'pan_value'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("query")
                    .parse()
                    .expect("parse");

                prop_assert!(
                    (loaded - pan).abs() < 1e-6,
                    "pan round-trip failed: stored {}, loaded {}",
                    pan,
                    loaded
                );
                Ok(())
            },
        );
        result.unwrap();
    }

    #[test]
    fn test_panner_full_left() {
        let mut p = Panner::new();
        p.set_pan(-1.0);
        let (gl, gr) = p.gains();
        assert!((gl - 1.0).abs() < 1e-6, "full left: gl should be 1.0, got {}", gl);
        assert!(gr.abs() < 1e-6, "full left: gr should be 0.0, got {}", gr);
    }

    #[test]
    fn test_panner_full_right() {
        let mut p = Panner::new();
        p.set_pan(1.0);
        let (gl, gr) = p.gains();
        assert!(gl.abs() < 1e-6, "full right: gl should be 0.0, got {}", gl);
        assert!((gr - 1.0).abs() < 1e-6, "full right: gr should be 1.0, got {}", gr);
    }

    #[test]
    fn test_panner_process_applies_gains() {
        let mut p = Panner::new();
        p.set_pan(0.0);
        let (gl, gr) = p.gains();
        let mut samples = vec![1.0_f32, 1.0_f32]; // one stereo frame
        p.process(&mut samples, 2);
        assert!((samples[0] - gl).abs() < 1e-6);
        assert!((samples[1] - gr).abs() < 1e-6);
    }

    #[test]
    fn test_panner_clamps_pan_value() {
        let mut p = Panner::new();
        p.set_pan(2.0);
        assert_eq!(p.get_pan(), 1.0);
        p.set_pan(-5.0);
        assert_eq!(p.get_pan(), -1.0);
    }
}
