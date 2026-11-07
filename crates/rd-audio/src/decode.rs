//! High-speed audio extraction and decoding to 32-bit float PCM at 48 kHz stereo.

use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

/// Extract and decode the audio stream of a media file into 48 kHz stereo 32-bit floating-point PCM.
pub fn decode_audio_to_pcm(path: impl AsRef<Path>) -> Result<Vec<f32>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut child = Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(path)
        .arg("-vn")
        .arg("-f")
        .arg("f32le")
        .arg("-ar")
        .arg("48000")
        .arg("-ac")
        .arg("2")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to spawn ffmpeg audio decode for {}", path.display()))?;

    let mut stdout = child.stdout.take().context("Failed to open stdout pipe")?;
    let mut raw_bytes = Vec::new();
    stdout.read_to_end(&mut raw_bytes)?;
    let _ = child.wait();

    if raw_bytes.is_empty() {
        return Ok(Vec::new());
    }

    let sample_count = raw_bytes.len() / 4;
    let mut pcm = Vec::with_capacity(sample_count);
    for chunk in raw_bytes.as_chunks::<4>().0 {
        pcm.push(f32::from_le_bytes(*chunk));
    }

    log::info!(
        "Extracted {} audio samples ({:.2} s) from {}",
        pcm.len(),
        pcm.len() as f64 / (48000.0 * 2.0),
        path.display()
    );

    Ok(pcm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_audio_from_demo() {
        let demo = PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4");
        if !demo.exists() {
            return;
        }

        let pcm = decode_audio_to_pcm(&demo).expect("Failed to extract audio");
        assert!(!pcm.is_empty());
        // 48000 Hz * 2 channels * 32 seconds = ~3,072,000 samples
        assert!(pcm.len() > 100_000);
    }
}
