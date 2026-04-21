pub struct Equalizer {
    bands: [BiquadFilter; 8],
    gains_db: [f32; 8],
    bypassed: bool,
    sample_rate: u32,
}

struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // delay state per channel (up to 2 channels)
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
}

impl BiquadFilter {
    fn new() -> Self {
        BiquadFilter {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: [0.0; 2],
            x2: [0.0; 2],
            y1: [0.0; 2],
            y2: [0.0; 2],
        }
    }

    fn process(&mut self, x: f32, ch: usize) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1[ch] + self.b2 * self.x2[ch]
            - self.a1 * self.y1[ch]
            - self.a2 * self.y2[ch];
        self.x2[ch] = self.x1[ch];
        self.x1[ch] = x;
        self.y2[ch] = self.y1[ch];
        self.y1[ch] = y;
        y
    }
}

impl Equalizer {
    pub const CENTER_FREQS: [f32; 8] =
        [60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 14000.0];

    pub fn new(sample_rate: u32) -> Self {
        let mut eq = Equalizer {
            bands: std::array::from_fn(|_| BiquadFilter::new()),
            gains_db: [0.0; 8],
            bypassed: false,
            sample_rate,
        };
        for band in 0..8 {
            eq.recompute_coefficients(band);
        }
        eq
    }

    pub fn set_gain(&mut self, band: usize, gain_db: f32) {
        if band < 8 {
            self.gains_db[band] = gain_db.clamp(-12.0, 12.0);
            self.recompute_coefficients(band);
        }
    }

    pub fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    pub fn process(&mut self, samples: &mut [f32], channels: usize) {
        if self.bypassed || channels == 0 {
            return;
        }
        let ch_count = channels.min(2);
        for (i, sample) in samples.iter_mut().enumerate() {
            let ch = i % ch_count;
            let mut s = *sample;
            for band in self.bands.iter_mut() {
                s = band.process(s, ch);
            }
            *sample = s.clamp(-1.0, 1.0);
        }
    }

    fn recompute_coefficients(&mut self, band: usize) {
        let gain_db = self.gains_db[band];
        let f0 = Self::CENTER_FREQS[band];
        let fs = self.sample_rate as f32;
        let q = 1.0_f32;

        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * w0.cos();
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * w0.cos();
        let a2 = 1.0 - alpha / a;

        self.bands[band].b0 = b0 / a0;
        self.bands[band].b1 = b1 / a0;
        self.bands[band].b2 = b2 / a0;
        self.bands[band].a1 = a1 / a0;
        self.bands[band].a2 = a2 / a0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Feature: neptune-feature-expansion, Property 9: EQ gain clamping
    // Validates: Requirements 6.9
    #[test]
    fn test_eq_gain_clamping_proptest() {
        use proptest::prelude::*;

        // -12 dBFS = 10^(-12/20) ≈ 0.251189
        const MAX_AMPLITUDE: f32 = 0.251_189;

        let result = proptest::test_runner::TestRunner::default().run(
            &(
                // 8 gain values, each in [-12.0, 12.0]
                proptest::array::uniform8(-12.0_f32..=12.0_f32),
                // audio sample amplitude in [-MAX_AMPLITUDE, MAX_AMPLITUDE]
                proptest::collection::vec(
                    -MAX_AMPLITUDE..=MAX_AMPLITUDE,
                    1..=256usize,
                ),
            ),
            |(gains, mut samples)| {
                let mut eq = Equalizer::new(48000);
                for (band, &gain) in gains.iter().enumerate() {
                    eq.set_gain(band, gain);
                }
                eq.process(&mut samples, 2);
                for &s in &samples {
                    prop_assert!(
                        s >= -1.0 && s <= 1.0,
                        "output sample {} is outside [-1.0, 1.0]",
                        s
                    );
                }
                Ok(())
            },
        );
        result.unwrap();
    }
}
