//! File playback. Audio-device sample rate is handled by linear interpolation.
//! UI analysis reads source PCM before volume, so muting does not hide the signal.
use crate::track::Track;
use cpal::{
    FromSample, SampleFormat, SizedSample,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
const NO_SEEK: u64 = u64::MAX;
pub struct Player {
    pub track: Mutex<Option<Arc<Track>>>,
    pub playing: AtomicBool,
    pub looping: AtomicBool,
    pub position: AtomicU64,
    pub volume: AtomicU32,
    seek: AtomicU64,
    revision: AtomicU64,
    pub error: Mutex<Option<String>>,
}
impl Player {
    pub fn new() -> Arc<Self> {
        let player = Arc::new(Self {
            track: Mutex::new(None),
            playing: AtomicBool::new(false),
            looping: AtomicBool::new(false),
            position: AtomicU64::new(0),
            volume: AtomicU32::new(0.5f32.to_bits()),
            seek: AtomicU64::new(NO_SEEK),
            revision: AtomicU64::new(0),
            error: Mutex::new(None),
        });
        let shared = player.clone();
        std::thread::spawn(move || match open(shared.clone()) {
            Ok(_stream) => loop {
                std::thread::park();
            },
            Err(e) => *shared.error.lock().unwrap() = Some(e),
        });
        player
    }
    pub fn load(&self, track: Arc<Track>) {
        self.playing.store(false, Ordering::Relaxed);
        // Pair the source change with its revision under one short lock.
        let mut held = self.track.lock().unwrap();
        *held = Some(track);
        self.seek.store(NO_SEEK, Ordering::Relaxed);
        self.position.store(0, Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Release);
    }
    pub fn seek(&self, frame: u64) {
        self.seek.store(frame, Ordering::Release);
    }
    pub fn toggle(&self) {
        if let Some(t) = self.track.lock().unwrap().as_ref() {
            if self.position.load(Ordering::Relaxed) >= t.frames.len() as u64 {
                self.seek(0);
            }
            self.playing.fetch_xor(true, Ordering::Relaxed);
        }
    }
    pub fn volume(&self, value: f32) {
        self.volume
            .store(value.clamp(0., 1.).to_bits(), Ordering::Relaxed);
    }
}
fn open(player: Arc<Player>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("No audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, player),
        SampleFormat::I16 => build::<i16>(&device, &config, player),
        SampleFormat::U16 => build::<u16>(&device, &config, player),
        SampleFormat::I32 => build::<i32>(&device, &config, player),
        format => Err(format!("Unsupported output format: {format}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    player: Arc<Player>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let output_rate = config.sample_rate as f64;
    let mut cursor = 0f64;
    let mut revision = 0;
    let errors = player.clone();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let (track, current_revision) = {
                    let held = player.track.lock().unwrap();
                    (held.clone(), player.revision.load(Ordering::Acquire))
                };
                if current_revision != revision {
                    cursor = 0.;
                    revision = current_revision;
                }
                let seek = player.seek.swap(NO_SEEK, Ordering::AcqRel);
                if seek != NO_SEEK {
                    cursor = track
                        .as_ref()
                        .map_or(0., |t| seek.min(t.frames.len() as u64) as f64);
                }
                let volume = f32::from_bits(player.volume.load(Ordering::Relaxed));
                let mut playing = player.playing.load(Ordering::Relaxed);
                for frame in data.chunks_mut(channels) {
                    let mut out = [0.; 2];
                    if let Some(t) = &track {
                        if cursor >= t.frames.len() as f64 {
                            if player.looping.load(Ordering::Relaxed) {
                                cursor %= t.frames.len() as f64;
                            } else {
                                playing = false;
                                player.playing.store(false, Ordering::Relaxed);
                                cursor = t.frames.len() as f64;
                            }
                        }
                        if playing {
                            out = sample(t, cursor);
                            cursor += t.rate as f64 / output_rate;
                        }
                    }
                    for (ch, value) in frame.iter_mut().enumerate() {
                        let v = if channels == 1 {
                            (out[0] + out[1]) * 0.5
                        } else if ch < 2 {
                            out[ch]
                        } else {
                            0.
                        };
                        *value = T::from_sample((v * volume).clamp(-1., 1.));
                    }
                }
                player.position.store(cursor as u64, Ordering::Relaxed);
            },
            move |error| {
                *errors.error.lock().unwrap() = Some(format!("Audio device: {error}"));
                errors.playing.store(false, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| e.to_string())
}
fn sample(track: &Track, cursor: f64) -> [f32; 2] {
    let i = cursor as usize;
    let a = track.frames[i.min(track.frames.len() - 1)];
    let b = track.frames[(i + 1).min(track.frames.len() - 1)];
    let f = (cursor - i as f64) as f32;
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampling_interpolates_channels_and_clamps_the_last_frame() {
        let t = Track {
            name: "test".into(),
            rate: 44100,
            frames: vec![[0., 1.], [1., -1.]],
        };
        assert_eq!(sample(&t, 0.5), [0.5, 0.]);
        assert_eq!(sample(&t, 1.8), [1., -1.]);
    }
}
