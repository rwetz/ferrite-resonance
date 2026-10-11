//! Bounded local-file decoding shared with Prism. Original PCM stays unnormalized.
use std::{path::Path, sync::Arc};
use symphonia::core::{audio::SampleBuffer, errors::Error, io::MediaSourceStream, probe::Hint};

pub const MAX_FRAMES: usize = 32_000_000;
pub struct Track {
    pub name: String,
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
}
impl Track {
    pub fn seconds(&self) -> f64 {
        self.frames.len() as f64 / self.rate as f64
    }
}

pub fn load(path: &Path) -> Result<Arc<Track>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
        return Err("File exceeds the 256 MiB limit. Open a shorter bounce.".into());
    }
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .format(&hint, stream, &Default::default(), &Default::default())
        .map_err(|e| format!("Unsupported or damaged audio file: {e}"))?
        .format;
    let source = format
        .default_track()
        .ok_or("No audio track in this file")?;
    let id = source.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&source.codec_params, &Default::default())
        .map_err(|e| format!("Unsupported audio codec: {e}"))?;
    let mut rate = 0;
    let mut frames = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("Cannot read audio: {e}")),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|e| format!("Cannot decode audio: {e}"))?;
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        if channels == 0 || channels > 2 {
            return Err(
                "Use a mono or stereo bounce. Surround files are not supported yet.".into(),
            );
        }
        if rate != 0 && rate != spec.rate {
            return Err("The file changes sample rate mid-track.".into());
        }
        rate = spec.rate;
        if rate == 0 {
            return Err("Invalid sample rate".into());
        }
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        let added = buffer.samples().len() / channels;
        if frames.len() + added > MAX_FRAMES {
            return Err("Decoded audio exceeds 32 million frames (about 11 minutes at 48 kHz). Open a shorter bounce.".into());
        }
        for sample in buffer.samples().chunks_exact(channels) {
            let pair = [sample[0], sample[channels - 1]];
            if pair.iter().any(|v| !v.is_finite()) {
                return Err("The file contains non-finite audio samples.".into());
            }
            frames.push(pair);
        }
    }
    if frames.is_empty() {
        return Err("This file contains no decoded audio".into());
    }
    Ok(Arc::new(Track {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        rate,
        frames,
    }))
}

pub fn time(seconds: f64) -> String {
    let secs = seconds.max(0.) as u64;
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_stereo_pcm_without_normalizing_and_rejects_invalid_files() {
        let path = std::env::temp_dir().join(format!(
            "ferrite-pcm-{}-{}.wav",
            env!("CARGO_PKG_NAME"),
            std::process::id()
        ));
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(44u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(48000u32.to_le_bytes());
        bytes.extend(192000u32.to_le_bytes());
        bytes.extend(4u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(8u32.to_le_bytes());
        for value in [16384i16, -16384, 8192, -8192] {
            bytes.extend(value.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let track = load(&path).unwrap();
        assert_eq!(track.rate, 48000);
        assert_eq!(track.frames, [[0.5, -0.5], [0.25, -0.25]]);
        std::fs::write(&path, b"not a song").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
