use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
pub const FREQUENCIES: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EqSettings {
    pub enabled: bool,
    pub gains: [f32; 10],
    pub preamp: f32,
}
impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            gains: [0.0; 10],
            preamp: 0.0,
        }
    }
}
pub struct EqControls {
    enabled: AtomicBool,
    gains: [AtomicU32; 10],
    preamp: AtomicU32,
}
impl Default for EqControls {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            gains: std::array::from_fn(|_| AtomicU32::new(0.0f32.to_bits())),
            preamp: AtomicU32::new(0.0f32.to_bits()),
        }
    }
}
impl EqControls {
    pub fn set(&self, settings: EqSettings) {
        self.enabled.store(settings.enabled, Ordering::Relaxed);
        for (cell, gain) in self.gains.iter().zip(settings.gains) {
            cell.store(
                if gain.is_finite() {
                    gain.clamp(-12.0, 12.0)
                } else {
                    0.0
                }
                .to_bits(),
                Ordering::Relaxed,
            );
        }
        self.preamp.store(
            if settings.preamp.is_finite() {
                settings.preamp.clamp(-24.0, 0.0)
            } else {
                0.0
            }
            .to_bits(),
            Ordering::Relaxed,
        );
    }
}
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}
impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}
impl Biquad {
    fn peak(frequency: f32, gain: f32, rate: f32) -> Self {
        let a = 10.0f32.powf(gain / 40.0);
        let omega = std::f32::consts::TAU * frequency.min(rate * 0.45) / rate;
        let alpha = omega.sin() / (2.0 * 1.4);
        let cos = omega.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
    fn process(&self, input: f32, state: &mut [f32; 2]) -> f32 {
        let out = self.b0 * input + state[0];
        state[0] = self.b1 * input - self.a1 * out + state[1];
        state[1] = self.b2 * input - self.a2 * out;
        if state[0].abs() < 1.0e-20 {
            state[0] = 0.0;
        }
        if state[1].abs() < 1.0e-20 {
            state[1] = 0.0;
        }
        out
    }
}
pub struct Equalizer {
    filters: [Biquad; 10],
    states: [[[f32; 2]; 10]; 2],
    gains: [f32; 10],
    preamp: f32,
    mix: f32,
    target_mix: f32,
    rate: f32,
}
impl Equalizer {
    pub fn new(rate: u32) -> Self {
        Self {
            filters: [Biquad::default(); 10],
            states: [[[0.0; 2]; 10]; 2],
            gains: [0.0; 10],
            preamp: 1.0,
            mix: 0.0,
            target_mix: 0.0,
            rate: rate as f32,
        }
    }
    pub fn prepare(&mut self, controls: &EqControls, frames: usize) {
        self.target_mix = if controls.enabled.load(Ordering::Relaxed) {
            1.0
        } else {
            0.0
        };
        if self.target_mix == 0.0 && self.mix == 0.0 {
            return;
        }
        let smoothing = 1.0 - (-(frames as f32) / (self.rate * 0.02)).exp();
        for (i, gain) in self.gains.iter_mut().enumerate() {
            let target = f32::from_bits(controls.gains[i].load(Ordering::Relaxed));
            *gain += (target - *gain) * smoothing;
            self.filters[i] = Biquad::peak(FREQUENCIES[i], *gain, self.rate);
        }
        let target = 10.0f32.powf(f32::from_bits(controls.preamp.load(Ordering::Relaxed)) / 20.0);
        self.preamp += (target - self.preamp) * smoothing;
    }
    pub fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        if self.target_mix == 0.0 && self.mix == 0.0 {
            return frame;
        }
        self.mix += (self.target_mix - self.mix) * 0.002;
        if self.target_mix == 0.0 && self.mix < 0.00001 {
            self.mix = 0.0;
            self.states = [[[0.0; 2]; 10]; 2];
            return frame;
        }
        std::array::from_fn(|channel| {
            let mut wet = frame[channel] * self.preamp;
            for (i, filter) in self.filters.iter().enumerate() {
                wet = filter.process(wet, &mut self.states[channel][i]);
            }
            let out = frame[channel] + (wet - frame[channel]) * self.mix;
            if out.is_finite() {
                out
            } else {
                0.0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bypass_is_bit_exact() {
        let mut eq = Equalizer::new(48000);
        eq.prepare(&EqControls::default(), 512);
        for i in 0..10_000 {
            let sample = [(i as f32).sin() * 0.5, -0.125];
            assert_eq!(eq.process(sample), sample);
        }
    }
    #[test]
    fn one_kilohertz_band_boost_matches_six_db() {
        let controls = EqControls::default();
        let mut settings = EqSettings {
            enabled: true,
            ..Default::default()
        };
        settings.gains[5] = 6.0;
        controls.set(settings);
        let mut eq = Equalizer::new(48000);
        let (mut dry, mut wet) = (0.0f64, 0.0f64);
        for i in 0..96000 {
            if i % 480 == 0 {
                eq.prepare(&controls, 480);
            }
            let input = (std::f32::consts::TAU * 1000.0 * i as f32 / 48000.0).sin() * 0.1;
            let out = eq.process([input; 2])[0];
            if i >= 48000 {
                dry += (input * input) as f64;
                wet += (out * out) as f64;
            }
        }
        let db = 10.0 * (wet / dry).log10();
        assert!((db - 6.0).abs() < 0.15, "{db}");
    }
}
