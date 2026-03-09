use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;

fn main() {
    let spec = Spec {
        format: Format::S16le,
        channels: 1,
        rate: 16000,
    };
    
    match Simple::new(
        None,
        "GeminiAudio",
        Direction::Record,
        None,
        "Voice Recording",
        &spec,
        None,
        None,
    ) {
        Ok(_) => println!("PulseAudio initialized successfully."),
        Err(e) => println!("Error initializing PulseAudio: {:?}", e),
    }
}
