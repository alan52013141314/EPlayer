use ringbuf::{
    traits::{Consumer, Split},
    HeapProd, HeapRb,
};
use rustfft::{num_complex::Complex, FftPlanner};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
pub const BANDS: usize = 48;
pub struct Shared {
    pub enabled: AtomicBool,
    pub analyses: AtomicU64,
    running: AtomicBool,
    pub bands: Mutex<[f32; BANDS]>,
}
pub struct Spectrum {
    pub shared: Arc<Shared>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Spectrum {
    pub fn new(rate: u32) -> (Self, HeapProd<f32>) {
        let (producer, mut consumer) = HeapRb::<f32>::new(8192).split();
        let shared = Arc::new(Shared {
            enabled: AtomicBool::new(false),
            analyses: AtomicU64::new(0),
            running: AtomicBool::new(true),
            bands: Mutex::new([0.0; BANDS]),
        });
        let data = shared.clone();
        let worker = thread::Builder::new()
            .name("spectrum".into())
            .spawn(move || {
                let mut planner = FftPlanner::new();
                let fft = planner.plan_fft_forward(2048);
                let mut buffer = vec![Complex::new(0.0, 0.0); 2048];
                let mut scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
                let mut history = [0.0f32; 2048];
                let mut cursor = 0;
                let mut was_active = false;
                while data.running.load(Ordering::Relaxed) {
                    let active = data.enabled.load(Ordering::Acquire);
                    let mut count = 0;
                    while let Some(sample) = consumer.try_pop() {
                        if active {
                            history[cursor] = sample;
                            cursor = (cursor + 1) % history.len();
                            count += 1;
                        }
                    }
                    if active && count > 0 {
                        for (i, value) in buffer.iter_mut().enumerate() {
                            let window =
                                0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / 2047.0).cos();
                            *value = Complex::new(history[(cursor + i) % 2048] * window, 0.0);
                        }
                        fft.process_with_scratch(&mut buffer, &mut scratch);
                        let bands = band_levels(&buffer, rate);
                        if data.enabled.load(Ordering::Acquire) {
                            if let Ok(mut output) = data.bands.lock() {
                                *output = bands;
                            }
                            data.analyses.fetch_add(1, Ordering::Relaxed);
                        }
                    } else if !active && was_active {
                        history.fill(0.0);
                        if let Ok(mut output) = data.bands.lock() {
                            output.fill(0.0);
                        }
                    }
                    was_active = active;
                    thread::sleep(Duration::from_millis(34));
                }
            })
            .expect("spectrum worker");
        (
            Self {
                shared,
                worker: Some(worker),
            },
            producer,
        )
    }
}
impl Drop for Spectrum {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
fn band_levels(fft: &[Complex<f32>], rate: u32) -> [f32; BANDS] {
    let max_frequency = 20000.0f32.min(rate as f32 * 0.5);
    let ratio = max_frequency / 30.0;
    std::array::from_fn(|band| {
        let lower = 30.0 * ratio.powf(band as f32 / BANDS as f32);
        let upper = 30.0 * ratio.powf((band + 1) as f32 / BANDS as f32);
        let a = ((lower * fft.len() as f32 / rate as f32).floor() as usize).max(1);
        let b = ((upper * fft.len() as f32 / rate as f32).ceil() as usize)
            .min(fft.len() / 2)
            .max(a + 1);
        let amplitude = fft[a..b]
            .iter()
            .map(|v| v.norm() * 4.0 / fft.len() as f32)
            .fold(0.0f32, f32::max);
        ((20.0 * amplitude.max(1.0e-5).log10() + 72.0) / 72.0).clamp(0.0, 1.0)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tone_appears_in_expected_frequency_band() {
        let n = 2048;
        let mut values = (0..n)
            .map(|i| {
                Complex::new(
                    (std::f32::consts::TAU * 440.0 * i as f32 / 48000.0).sin()
                        * 0.5
                        * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1) as f32).cos()),
                    0.0,
                )
            })
            .collect::<Vec<_>>();
        FftPlanner::new().plan_fft_forward(n).process(&mut values);
        let bands = band_levels(&values, 48000);
        let peak = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        let expected = ((440.0f32 / 30.0).ln() / (20000.0f32 / 30.0).ln() * BANDS as f32) as usize;
        assert!(peak.abs_diff(expected) <= 1, "{peak} vs {expected}");
    }
}
