use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    println!("Default Host: {:?}", host.id());

    if let Some(device) = host.default_input_device() {
        println!("Default Input Device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));
        if let Ok(config) = device.default_input_config() {
            println!("  Default Config: {:?}", config);
        }
    } else {
        println!("No default input device found.");
    }
}
