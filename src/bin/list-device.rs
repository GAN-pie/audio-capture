//! List audio capture devices or dump their configuration.

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use cpal::traits::{DeviceTrait, HostTrait};

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

            let config = device.default_input_config()?;
            println!("--- Device Information ---");
            println!("ID:             {:?}", device.id()?);
            println!("Description:    {}", device.description()?);
            println!("Sample Format:  {:?}", config.sample_format());
            println!("Channels:       {}", config.channels());
            println!("Sample Rate:    {:?}", config.sample_rate());
        }
    }

    Ok(())
}
