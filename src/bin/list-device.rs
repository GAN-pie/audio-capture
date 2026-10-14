//! List audio capture devices or dump their configuration.

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use cpal::traits::{DeviceTrait, HostTrait};
use std::collections::BTreeSet;

#[derive(Parser, Debug)]
#[command(
    name = "list-device",
    about = "List audio capture devices or dump their configuration"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all audio capture devices
    List,
    /// Dump the configuration of a given device
    Dump {
        /// The ID of the device to dump
        device_id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let host = cpal::default_host();

    match cli.command {
        Commands::List => {
            let devices = host.input_devices()?;
            println!("Available Audio Capture Devices:");
            for (i, device) in devices.enumerate() {
                println!(
                    "{}. ID: {:?} | Description: {}",
                    i + 1,
                    device.id().map_or(String::from("NA"), |id| id.to_string()),
                    device.description()?
                );
            }
        }
        Commands::Dump { device_id } => {
            let device = match device_id {
                Some(device_id) => {
                    let mut devices = host.input_devices()?;
                    devices
                        .find(|d| {
                            d.id()
                                .map(|id| id.to_string() == device_id)
                                .unwrap_or(false)
                        })
                        .ok_or_else(|| anyhow!("Device with ID '{}' not found", device_id))?
                }
                None => host
                    .default_input_device()
                    .ok_or_else(|| anyhow!("Cannot found default input device"))?,
            };

            let configs = device.supported_input_configs()?;
            let mut channels = BTreeSet::new();
            let mut formats = BTreeSet::new();
            let mut min_sr = u32::MAX;
            let mut max_sr = 0;
            let mut min_bs = None;
            let mut max_bs = None;
            let mut found_any = false;

            for config in configs {
                found_any = true;
                channels.insert(config.channels());
                formats.insert(config.sample_format());

                let sr_min = config.min_sample_rate();
                let sr_max = config.max_sample_rate();
                min_sr = min_sr.min(sr_min);
                max_sr = max_sr.max(sr_max);

                match config.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        let min_val = *min;
                        let max_val = *max;
                        if min_bs.map_or(true, |m| min_val < m) {
                            min_bs = Some(min_val);
                        }
                        if max_bs.map_or(true, |m| max_val > m) {
                            max_bs = Some(max_val);
                        }
                    }
                    _ => {}
                }
            }

            println!("--- Device Information ---");
            println!("ID:             {:?}", device.id()?);
            println!("Description:    {}", device.description()?);

            if !found_any {
                println!("No supported configurations found.");
            } else {
                println!("Channels:       {:?}", channels);
                println!("Sample Formats: {:?}", formats);
                println!("Sample Rate:    [{:?}, {:?}]", min_sr, max_sr);
                if let (Some(min), Some(max)) = (min_bs, max_bs) {
                    println!("Buffer Size:    [{}, {}]", min, max);
                } else {
                    println!("Buffer Size:    Not supported or variable");
                }
            }
        }
    }

    Ok(())
}
