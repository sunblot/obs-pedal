use midir::{MidiInput, MidiInputPort};
use std::sync::mpsc;

/// A parsed MIDI event from the pedal.
#[derive(Debug)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8, velocity: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    Unknown(Vec<u8>),
}

/// Find the first MIDI input port whose name contains `needle`.
pub fn find_port(midi_in: &MidiInput, needle: &str) -> Option<MidiInputPort> {
    for port in midi_in.ports() {
        if let Ok(name) = midi_in.port_name(&port) {
            log::debug!("Found MIDI port: {}", name);
            if name.contains(needle) {
                log::info!("Matched MIDI port: {}", name);
                return Some(port);
            }
        }
    }
    None
}

/// List all available MIDI input port names.
pub fn list_ports(midi_in: &MidiInput) -> Vec<String> {
    midi_in
        .ports()
        .iter()
        .filter_map(|p| midi_in.port_name(p).ok())
        .collect()
}

/// Parse raw MIDI bytes into a MidiEvent.
pub fn parse_message(data: &[u8]) -> MidiEvent {
    if data.len() < 3 {
        return MidiEvent::Unknown(data.to_vec());
    }
    let status = data[0] & 0xF0;
    let channel = data[0] & 0x0F;
    match status {
        0x90 => MidiEvent::NoteOn {
            channel,
            note: data[1],
            velocity: data[2],
        },
        0x80 => MidiEvent::NoteOff {
            channel,
            note: data[1],
            velocity: data[2],
        },
        0xB0 => MidiEvent::ControlChange {
            channel,
            controller: data[1],
            value: data[2],
        },
        _ => MidiEvent::Unknown(data.to_vec()),
    }
}

/// Open the MIDI port and send parsed events to the returned receiver.
/// Returns the connection (must be kept alive) and the receiver.
pub fn open_listener(
    port: MidiInputPort,
) -> Result<
    (midir::MidiInputConnection<()>, mpsc::Receiver<MidiEvent>),
    Box<dyn std::error::Error>,
> {
    let (tx, rx) = mpsc::channel();
    let midi_in = MidiInput::new("obs-pedal-listener")?;
    let conn = midi_in
        .connect(
            &port,
            "obs-pedal",
            move |timestamp, data, _| {
                let event = parse_message(data);
                log::debug!("[{}µs] Raw: {:02X?} → {:?}", timestamp, data, event);
                let _ = tx.send(event);
            },
            (),
        )
        .map_err(|e| format!("Failed to connect to MIDI port: {}", e))?;
    Ok((conn, rx))
}
