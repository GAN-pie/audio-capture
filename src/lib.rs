use std::sync::mpsc;
use anyhow::{anyhow, Result};

use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, SampleFormat, Sample};

/// A type alias for an audio frame, represented as a vector of f32 samples.
pub type AudioFrame = Vec<f32>;

/// A struct that captures audio from the default input device and provides it via an Iterator.
pub struct AudioCapture {
    receiver: mpsc::Receiver<AudioFrame>,
    _stream: cpal::Stream,
}

impl AudioCapture {
    /// Creates a new `AudioCapture` instance, starting the audio stream.
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<AudioFrame>();

        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))?;

        let config = device.default_input_config()
            .map_err(|e| anyhow!("Failed to get default input config: {}", e))?;

        println!("Default input/output config: {config:?}");

        let err_fn = |err: cpal::Error| {
            eprintln!("Stream error: {}", err);
        };

        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                let config: cpal::StreamConfig = config.into();
                device.build_input_stream(
                    config,
                    move |data: &[f32], _| {
                        let _ = sender.send(data.to_vec());
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let config: cpal::StreamConfig = config.into();
                device.build_input_stream(
                    config,
                    move |data: &[i16], _| {
                        let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                        let _ = sender.send(f32_data);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I32 => {
                let config: cpal::StreamConfig = config.into();
                device.build_input_stream(
                    config,
                    move |data: &[i32], _| {
                        let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                        let _ = sender.send(f32_data);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I8 => {
                let config: cpal::StreamConfig = config.into();
                device.build_input_stream(
                    config,
                    move |data: &[i8], _| {
                        let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                        let _ = sender.send(f32_data);
                    },
                    err_fn,
                    None,
                )?
            }
            _ => return Err(anyhow!("Unsupported sample format")),
        };

        stream.play()?;

        Ok(AudioCapture {
            receiver,
            _stream: stream,
        })
    }
}

impl Iterator for AudioCapture {
    type Item = AudioFrame;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}



pub fn read_microphone_audio() -> Result<AudioCapture> {
    AudioCapture::new()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audiocapture_iterator() {
        // Test that we can create an iterator
        let iterator = read_microphone_audio();
        assert!(iterator.is_ok());
    }

    #[test]
    fn test_audiocapture_frame_count() {
        let mut iterator = read_microphone_audio().expect("Failed to start audio capture");
        let start = std::time::Instant::now();
        let mut total_frames = 0;
        while start.elapsed().as_millis() < 1000 {
            if let Some(frame) = iterator.next() {
                total_frames += frame.len();
            }
        }
        // Assuming 48kHz stereo, we expect 48000 frames (samples) in 1 second.
        let expected_frames = 48000 * 2;
        let margin = 5000; // Larger margin for CI/varying environments
        assert!(
            total_frames >= (expected_frames - margin) && total_frames <= (expected_frames + margin),
            "Expected approximately {} frames, but got {}", expected_frames, total_frames
        );
    }
}
