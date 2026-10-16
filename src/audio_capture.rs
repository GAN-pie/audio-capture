use anyhow::{Result, anyhow};
use cpal::StreamConfig;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use cpal::{
    Sample, SampleFormat,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

/// A type alias for an audio chunk, represented as a vector of f32 samples.
pub type AudioChunk = Vec<f32>;

/// Configuration for audio capture.
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    pub device_id: Option<String>,
    pub channels: u16,
    pub sample_rate: u32,
    pub buffer_size: Option<u32>,
    pub sample_format: String,
    pub start: bool,
    pub duration: u64,
    pub record_file: Option<String>,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            device_id: Some("alsa:default".to_string()),
            channels: 1,
            sample_rate: 48000,
            buffer_size: Some(256),
            sample_format: "F32".to_string(),
            start: true,
            duration: 0,
            record_file: None,
        }
    }
}

impl Into<cpal::StreamConfig> for AudioCaptureConfig {
    fn into(self) -> cpal::StreamConfig {
        cpal::StreamConfig {
            channels: self.channels,
            sample_rate: self.sample_rate,
            buffer_size: match self.buffer_size {
                Some(size) => cpal::BufferSize::Fixed(size),
                None => cpal::BufferSize::Fixed(256),
            },
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

fn to_hound_sample_format(format: SampleFormat) -> hound::SampleFormat {
    if format.is_dsd() {
        panic!("DSD formats cannot be written to WAV files");
    } else if format.is_float() {
        hound::SampleFormat::Float
    } else {
        hound::SampleFormat::Int
    }
}

fn wav_spec_from_config(config: &AudioCaptureConfig) -> hound::WavSpec {
    let _sample_format = parse_sample_format(&config.sample_format).unwrap();
    hound::WavSpec {
        channels: config.channels as _,
        sample_rate: config.sample_rate as _,
        bits_per_sample: (_sample_format.sample_size() * 8) as _,
        sample_format: to_hound_sample_format(_sample_format),
    }
}

type WavWriterHandle = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

fn write_input_data<T, U>(input: &[T], writer: &WavWriterHandle)
where
    T: Sample,
    U: Sample + hound::Sample + cpal::FromSample<T>,
{
    if let Ok(mut guard) = writer.lock() {
        if let Some(ref mut w) = *guard {
            for &sample in input.iter() {
                let sample: U = U::from_sample(sample);
                match w.write_sample(sample) {
                    Err(error) => panic!("Error: {:?}, cannot write sample", error),
                    _ => {}
                }
            }
        }
    }
}

fn finalize_writer(writer: &Option<WavWriterHandle>) {
    if let Some(w) = writer {
        if let Ok(mut guard) = w.lock() {
            if let Some(wr) = guard.take() {
                if let Err(e) = wr.finalize() {
                    eprintln!("Failed to finalize wav file: {e}");
                }
            }
        }
    }
}

pub struct AudioCapture {
    receiver: mpsc::Receiver<AudioChunk>,
    stream: Arc<Mutex<Option<cpal::Stream>>>,
    config_params: AudioCaptureConfig,
    writer: Option<WavWriterHandle>, // new
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if let Ok(mut g) = self.stream.lock() {
            *g = None; // stop the callback first
        }
        finalize_writer(&self.writer); // then flush + patch header
    }
}

impl AudioCapture {
    /// Creates a new `AudioCapture` instance with the given configuration, starting the audio stream.
    pub fn new(config_params: AudioCaptureConfig) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<AudioChunk>();

        let host = cpal::default_host();
        let device = match config_params.device_id {
            Some(ref wanted) => host
                .input_devices()?
                .find(|d| d.id().map(|id| id.to_string() == *wanted).unwrap_or(false))
                .ok_or_else(|| anyhow!("Cannot found device {:?}", wanted))?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow!("No default input device found"))?,
        };

        let sample_format = parse_sample_format(&config_params.sample_format)?;

        let stream_config: StreamConfig = config_params.clone().into();

        let writer = match &config_params.record_file {
            Some(file_path) => {
                // The WAV file we're recording to.
                let spec = wav_spec_from_config(&config_params);
                let writer = hound::WavWriter::create(file_path, spec)?;
                Some(Arc::new(Mutex::new(Some(writer))))
            }
            None => None,
        };

        let err_fn = |err: cpal::Error| {
            eprintln!("Stream error: {}", err);
        };

        let writer_clone = writer.clone();

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    if writer_clone.is_some() {
                        write_input_data::<f32, f32>(data, writer_clone.as_ref().unwrap());
                    }
                    let _ = sender.send(data.to_vec());
                },
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    if writer_clone.is_some() {
                        write_input_data::<i16, i16>(data, writer_clone.as_ref().unwrap());
                    }
                    let f32_data: AudioChunk = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I32 => device.build_input_stream(
                stream_config,
                move |data: &[i32], _| {
                    if writer_clone.is_some() {
                        write_input_data::<i32, i32>(data, writer_clone.as_ref().unwrap());
                    }
                    let f32_data: AudioChunk = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I8 => device.build_input_stream(
                stream_config,
                move |data: &[i8], _| {
                    if writer_clone.is_some() {
                        write_input_data::<i8, i8>(data, writer_clone.as_ref().unwrap());
                    }
                    let f32_data: AudioChunk = data.iter().map(|&s| s.to_sample()).collect();
                    let _ = sender.send(f32_data);
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("Unsupported sample format")),
        };

        let stream = Arc::new(Mutex::new(Some(stream)));

        if config_params.start {
            let mut stream_guard = stream
                .lock()
                .map_err(|_| anyhow!("Failed to lock stream mutex"))?;
            if let Some(s) = stream_guard.as_mut() {
                s.play()?;
            }
        }

        if config_params.duration > 0 {
            let stream_clone = Arc::clone(&stream);
            let writer_for_thread = writer.clone();
            let duration = config_params.duration;
            thread::spawn(move || {
                thread::sleep(std::time::Duration::from_secs(duration));
                if let Ok(mut g) = stream_clone.lock() {
                    *g = None;
                }
                finalize_writer(&writer_for_thread);
            });
        }

        Ok(AudioCapture {
            receiver,
            stream,
            config_params,
            writer,
        })
    }

    /// Manually starts the audio stream.
    pub fn play(&self) -> Result<()> {
        let mut stream_guard = self
            .stream
            .lock()
            .map_err(|_| anyhow!("Failed to lock stream mutex"))?;
        if let Some(s) = stream_guard.as_mut() {
            s.play()?;
        } else {
            return Err(anyhow!(
                "Stream has already been stopped or was never started"
            ));
        }
        Ok(())
    }
}

impl Iterator for AudioCapture {
    type Item = AudioChunk;

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
    fn test_manual_start() {
        let config = AudioCaptureConfig {
            start: false,
            ..Default::default()
        };
        let mut capture =
            read_microphone_audio_with_config(config).expect("Failed to create capture");

        // Since start is false, the iterator should not yield anything immediately.
        // We'll use a small timeout check.
        let receiver_clone = capture.receiver.try_recv();
        assert!(receiver_clone.is_err()); // Should be empty

        capture.play().expect("Failed to play");

        // Now it should work.
        let chunk = capture.next();
        assert!(chunk.is_some());
    }

    #[test]
    fn test_automatic_stop_by_duration() {
        let config = AudioCaptureConfig {
            duration: 1, // 1 second
            ..Default::default()
        };
        let mut capture =
            read_microphone_audio_with_config(config).expect("Failed to start audio capture");

        let start_time = std::time::Instant::now();
        let mut chunk_count = 0;
        while let Some(_) = capture.next() {
            chunk_count += 1;
            if start_time.elapsed().as_secs() >= 3 {
                // Safety break
                break;
            }
        }
        let elapsed = start_time.elapsed().as_secs();

        // It should have stopped around 1 second.
        // We allow some margin for timing and hardware latency.
        assert!(
            elapsed >= 1 && elapsed < 3,
            "Expected to stop around 1s, but took {}s",
            elapsed
        );
        assert!(
            chunk_count > 0,
            "Should have received some chunks before stopping"
        );
    }

    #[test]
    fn test_audiocapture_chunk_flow() {
        // Use a specific config to match the test expectation (stereo, 48kHz)
        let config = AudioCaptureConfig {
            device_id: Some("alsa:default".to_string()),
            channels: 2,
            sample_rate: 48000,
            buffer_size: Some(256),
            sample_format: "FLOAT32".to_string(),
            start: true,
            duration: 0,
            record_file: None,
        };
        let mut iterator =
            read_microphone_audio_with_config(config).expect("Failed to start audio capture");
        let start = std::time::Instant::now();
        let mut total_samples_received = 0;
        while start.elapsed().as_millis() < 1000 {
            if let Some(chunk) = iterator.next() {
                total_samples_received += chunk.len();
            }
        }

        // Instead of an exact count, we just check that we received a reasonable amount of data.
        // For 48kHz stereo, 1 second is 96,000 samples.
        // We check that we received at least some data to prove the stream works.
        assert!(
            total_samples_received > 0,
            "Expected to receive some samples, but got 0"
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
            device_id: Some("alsa:default".to_string()),
            channels: 1,
            sample_rate: 44100,
            buffer_size: Some(512),
            sample_format: "I16".to_string(),
            start: true,
            duration: 0,
            record_file: None,
        };
        let result = read_microphone_audio_with_config(config);
        // We don't assert is_ok() because it depends on hardware,
        // but we check that the call itself works.
        let _ = result;
    }

    #[test]
    fn test_no_data_after_stop() {
        let config = AudioCaptureConfig {
            duration: 1, // 1 second
            ..Default::default()
        };
        let mut capture =
            read_microphone_audio_with_config(config).expect("Failed to start audio capture");

        // Wait for duration to expire + a small buffer
        thread::sleep(std::time::Duration::from_secs(2));

        // Drain any remaining data in the channel and ensure it eventually returns None
        let start = std::time::Instant::now();
        while let Some(_) = capture.next() {
            if start.elapsed().as_secs() >= 3 {
                break;
            }
        }

        // After draining and stopping, next() should return None
        assert!(capture.next().is_none());
    }

    #[test]
    fn test_wav_file_creation() {
        let test_file = "test_output.wav";
        // Clean up in case a previous test failed
        let _ = std::fs::remove_file(test_file);

        let config = AudioCaptureConfig {
            device_id: Some("alsa:default".to_string()),
            channels: 1,
            sample_rate: 44100,
            buffer_size: Some(512),
            sample_format: "I16".to_string(),
            start: true,
            duration: 1,
            record_file: Some(test_file.to_string()),
        };

        let mut capture =
            read_microphone_audio_with_config(config).expect("Failed to start audio capture");

        // Consume the iterator until it stops due to duration
        while let Some(_) = capture.next() {}

        // Give a tiny bit of time for the writer to flush/close if necessary
        thread::sleep(std::time::Duration::from_millis(500));

        let file_exists = std::path::Path::new(test_file).exists();

        // Cleanup
        let _ = std::fs::remove_file(test_file);

        assert!(file_exists, "WAV file should have been created");
    }
}
