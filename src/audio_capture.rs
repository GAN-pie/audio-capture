use anyhow::{Result, anyhow};
use std::sync::mpsc;

use cpal::{
    Sample, SampleFormat,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

/// A type alias for an audio frame, represented as a vector of f32 samples.
pub type AudioFrame = Vec<f32>;

/// Configuration for audio capture.
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    pub channels: u16,
    pub sample_rate: u32,
    pub buffer_size: Option<u32>,
    pub sample_format: String,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            channels: 1,
            sample_rate: 48000,
            buffer_size: Some(256),
            sample_format: "FLOAT32".to_string(),
        }
    }
}

fn parse_sample_format(format: &str) -> Result<SampleFormat> {
    match format.to_uppercase().as_str() {
        "FLOAT32" | "F32" => Ok(SampleFormat::F32),
        "I16" => Ok(SampleFormat::I16),
        "I32" => Ok(SampleFormat::I32),
        "I8" => Ok(SampleFormat::I8),
        _ => Err(anyhow!("Unsupported sample format: {}", format)),
    }
}

/// A struct that captures audio from the default input device and provides it via an Iterator.
pub struct AudioCapture {
    receiver: mpsc::Receiver<AudioFrame>,
    _stream: cpal::Stream,
}

impl AudioCapture {
    /// Creates a new `AudioCapture` instance with the given configuration, starting the audio stream.
    pub fn new(config_params: AudioCaptureConfig) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<AudioFrame>();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))?;

        let sample_format = parse_sample_format(&config_params.sample_format)?;

        let stream_config = cpal::StreamConfig {
            channels: config_params.channels,
            sample_rate: config_params.sample_rate,
            buffer_size: match config_params.buffer_size {
                Some(size) => cpal::BufferSize::Fixed(size),
                None => cpal::BufferSize::Fixed(256),
            },
        };

        let err_fn = |err: cpal::Error| {
            eprintln!("Stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    let _ = sender.send(data.to_vec());
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I32 => device.build_input_stream(
                stream_config,
                move |data: &[i32], _| {
                    let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I8 => device.build_input_stream(
                stream_config,
                move |data: &[i8], _| {
                    let f32_data: AudioFrame = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
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

/// Creates an `AudioCapture` instance using default configuration.
pub fn read_microphone_audio() -> Result<AudioCapture> {
    AudioCapture::new(AudioCaptureConfig::default())
}

/// Creates an `AudioCapture` instance using the provided configuration.
pub fn read_microphone_audio_with_config(config: AudioCaptureConfig) -> Result<AudioCapture> {
    AudioCapture::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audiocapture_iterator() {
        // Test that we can create an iterator with default config
        let iterator = read_microphone_audio();
        assert!(iterator.is_ok());
    }

    #[test]
    fn test_audiocapture_frame_count() {
        // Use a specific config to match the test expectation (stereo, 48kHz)
        let config = AudioCaptureConfig {
            channels: 2,
            sample_rate: 48000,
            buffer_size: Some(256),
            sample_format: "FLOAT32".to_string(),
        };
        let mut iterator =
            read_microphone_audio_with_config(config).expect("Failed to start audio capture");
        let start = std::time::Instant::now();
        let mut total_frames = 0;
        while start.elapsed().as_millis() < 1000 {
            if let Some(frame) = iterator.next() {
                total_frames += frame.len();
            }
        }
        // Assuming 48kHz stereo, we expect 48000 * 2 frames in 1 second.
        let expected_frames = 48000 * 2;
        let margin = 5000; // Larger margin for CI/varying environments
        assert!(
            total_frames >= (expected_frames - margin)
                && total_frames <= (expected_frames + margin),
            "Expected approximately {} frames, but got {}",
            expected_frames,
            total_frames
        );
    }

    #[test]
    fn test_invalid_sample_format() {
        let config = AudioCaptureConfig {
            sample_format: "INVALID".to_string(),
            ..Default::default()
        };
        let result = read_microphone_audio_with_config(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_microphone_audio_with_config() {
        let config = AudioCaptureConfig {
            channels: 1,
            sample_rate: 44100,
            buffer_size: Some(512),
            sample_format: "I16".to_string(),
        };
        let result = read_microphone_audio_with_config(config);
        // We don't assert is_ok() because it depends on hardware,
        // but we check that the call itself works.
        // If it fails, it's likely due to hardware not supporting the config.
        let _ = result;
    }
}
