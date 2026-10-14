//! Records a WAV file using the AudioCapture component.
//!
//! The input data is recorded to the path specified via `--record-file`.

use audio_capture::{AudioCaptureConfig, read_microphone_audio_with_config};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "AudioCapture CLI tool", long_about = None)]
struct Opt {
    /// How long to record, in seconds
    #[arg(short('d'), long, default_value_t = 3)]
    duration: u64,

    /// Device ID to use
    #[arg(short('D'), long)]
    device_id: Option<String>,

    /// Number of channels
    #[arg(short, long, default_value_t = 1)]
    channels: u16,

    /// Sample rate in Hz
    #[arg(short('r'), long, default_value_t = 48000)]
    sample_rate: u32,

    /// Buffer size
    #[arg(long)]
    buffer_size: Option<u32>,

    /// Sample format (e.g., FLOAT32, I16, I32, I8)
    #[arg(short('f'), long, default_value = "FLOAT32")]
    sample_format: String,

    /// Path to the output WAV file
    #[arg(short('o'), long)]
    record_file: Option<String>,
}

fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let config = AudioCaptureConfig {
        device_id: opt.device_id,
        channels: opt.channels,
        sample_rate: opt.sample_rate,
        buffer_size: opt.buffer_size,
        sample_format: opt.sample_format,
        start: true,
        duration: opt.duration,
        record_file: opt.record_file,
    };

    println!("Starting audio capture with config: {:?}", config);
    let mut capture = read_microphone_audio_with_config(config)?;

    println!("Recording...");

    // Consume the iterator. If duration > 0, AudioCapture will stop automatically.
    while let Some(_chunk) = capture.next() {
        // We are just consuming the chunks to keep the stream alive.
    }

    println!("Recording complete!");
    Ok(())
}
