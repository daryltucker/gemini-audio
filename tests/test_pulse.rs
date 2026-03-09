use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;

fn main() {
    let spec = Spec {
        format: Format::S16le,
        channels: 1,
        rate: 16000,
    };
    
    let s = Simple::new(
        None,
        "test_pulse",
        Direction::Record,
        None,
        "record",
        &spec,
        None,
        None,
    ).unwrap();
    
    let mut buf = [0u8; 1024];
    s.read(&mut buf).unwrap();
    println!("Read {} bytes", buf.len());
}
