//! Calibrated sample-domain measurements and a Hann-windowed stereo FFT.
//! These are sample peaks and RMS, not true-peak or BS.1770 LUFS measurements.
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;
pub const FFT_SIZE: usize = 2048;
pub const BINS: usize = 64;

pub struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    input: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    power: Vec<f32>,
}
impl Spectrum {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self {
            fft,
            input: vec![Complex::default(); FFT_SIZE],
            scratch,
            power: vec![0.; FFT_SIZE / 2 + 1],
        }
    }
    /// Stereo channel powers are combined, so opposite-phase content survives.
    pub fn amplitudes(&mut self, frames: &[[f32; 2]], rate: u32) -> Vec<f32> {
        self.power.fill(0.);
        for channel in 0..2 {
            for (i, v) in self.input.iter_mut().enumerate() {
                let hann = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT_SIZE as f32).cos();
                *v = Complex::new(frames.get(i).map_or(0., |f| f[channel]) * hann, 0.);
            }
            self.fft
                .process_with_scratch(&mut self.input, &mut self.scratch);
            for (p, v) in self.power.iter_mut().zip(&self.input) {
                *p += v.norm_sqr() * 0.5;
            }
        }
        let high = (rate as f32 / 2.).clamp(31., 20000.);
        (0..BINS)
            .map(|i| {
                let low = 30. * (high / 30.).powf(i as f32 / BINS as f32);
                let upper = 30. * (high / 30.).powf((i + 1) as f32 / BINS as f32);
                let begin = (low * FFT_SIZE as f32 / rate.max(1) as f32).floor() as usize;
                let end = ((upper * FFT_SIZE as f32 / rate.max(1) as f32).ceil() as usize)
                    .max(begin + 1)
                    .min(self.power.len());
                self.power[begin.min(end)..end]
                    .iter()
                    .copied()
                    .fold(0., f32::max)
                    .sqrt()
                    * 4.
                    / FFT_SIZE as f32
            })
            .collect()
    }
}
pub fn bars(amplitudes: &[f32]) -> Vec<f32> {
    amplitudes
        .iter()
        .map(|v| ((20. * v.max(1e-8).log10() + 80.) / 80.).clamp(0., 1.))
        .collect()
}
pub fn db(value: f64) -> String {
    if value <= 0. {
        "-inf".into()
    } else {
        format!("{:.1}", 20. * value.log10())
    }
}
#[derive(Debug)]
pub struct Levels {
    pub peak: [f64; 2],
    pub rms: [f64; 2],
    pub correlation: Option<f64>,
    pub clipped: usize,
}
pub fn levels(frames: &[[f32; 2]]) -> Levels {
    let mut peak = [0f64; 2];
    let mut energy = [0f64; 2];
    let mut cross = 0.;
    let mut clipped = 0;
    for frame in frames {
        for ch in 0..2 {
            let value = frame[ch] as f64;
            peak[ch] = peak[ch].max(value.abs());
            energy[ch] += value * value;
            clipped += usize::from(value.abs() >= 1.);
        }
        cross += frame[0] as f64 * frame[1] as f64;
    }
    let denominator = (energy[0] * energy[1]).sqrt();
    Levels {
        peak,
        rms: energy.map(|e| (e / frames.len().max(1) as f64).sqrt()),
        correlation: (denominator > 1e-15).then(|| (cross / denominator).clamp(-1., 1.)),
        clipped,
    }
}
pub struct Report {
    pub levels: Levels,
    pub spectrum: Vec<f32>,
    pub waveform: Vec<[f32; 2]>,
    pub windows: usize,
}
pub fn analyze(track: &crate::track::Track) -> Report {
    let count = track.frames.len();
    let windows = count.div_ceil(FFT_SIZE).min(600);
    let mut fft = Spectrum::new();
    let mut power = vec![0.; BINS];
    for i in 0..windows {
        let start = if windows == 1 {
            0
        } else {
            i * count.saturating_sub(FFT_SIZE) / (windows - 1)
        };
        for (p, v) in power
            .iter_mut()
            .zip(fft.amplitudes(&track.frames[start..], track.rate))
        {
            *p += v * v;
        }
    }
    let spectrum = power
        .into_iter()
        .map(|v| (v / windows.max(1) as f32).sqrt())
        .collect();
    let waveform = track
        .frames
        .chunks(count.div_ceil(512).max(1))
        .map(|chunk| {
            let mut range = [0f32; 2];
            for frame in chunk {
                for v in frame {
                    range[0] = range[0].min(*v);
                    range[1] = range[1].max(*v);
                }
            }
            range
        })
        .collect();
    Report {
        levels: levels(&track.frames),
        spectrum,
        waveform,
        windows,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_and_level_measurements_handle_silence_and_antiphase() {
        let silence = levels(&[[0.; 2]; 128]);
        assert!(silence.correlation.is_none());
        assert_eq!(db(0.), "-inf");
        let signal: Vec<_> = (0..FFT_SIZE)
            .map(|i| {
                let v = 0.5 * (std::f32::consts::TAU * 64. * i as f32 / FFT_SIZE as f32).sin();
                [v, -v]
            })
            .collect();
        let measured = levels(&signal);
        assert!((measured.peak[0] - 0.5).abs() < 1e-6);
        assert!((measured.rms[0] - 0.5 / 2f64.sqrt()).abs() < 1e-5);
        assert!(measured.correlation.unwrap() < -0.9999);
        let mut fft = Spectrum::new();
        let a = fft.amplitudes(&signal, 48000);
        let strongest = a.iter().copied().fold(0., f32::max);
        assert!((strongest - 0.5).abs() < 0.01, "{strongest}");
        assert_eq!(levels(&[[1.1, -1.2]]).clipped, 2);
    }
}
