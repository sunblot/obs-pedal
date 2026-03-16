mod config;
mod midi;
mod obs;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    env_logger::init();

    let config_path = std::env::args()
        .skip_while(|a| a != "--config")
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    let config = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config from {}: {}", config_path.display(), e);
            std::process::exit(1);
        }
    };

    let pedal_map = config.pedal_map();
    println!("Loaded {} pedal mappings.", pedal_map.len());

    // Find the MIDI device
    let midi_in = midir::MidiInput::new("obs-pedal-discovery").expect("Failed to create MIDI input");
    let port = match midi::find_port(&midi_in, "FS-1-WL") {
        Some(p) => p,
        None => {
            eprintln!("Could not find FS-1-WL MIDI device. Available ports:");
            for name in midi::list_ports(&midi_in) {
                eprintln!("  - {}", name);
            }
            std::process::exit(1);
        }
    };

    // Open the MIDI listener
    let (_conn, rx) = midi::open_listener(port).expect("Failed to open MIDI port");
    println!("Listening for MIDI events from FS-1-WL...");

    // Graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Failed to set Ctrl+C handler");

    // Connect to OBS
    let mut obs_client = connect_obs(&config.obs, &running);

    println!("Ready. Tap a pedal to switch scenes. Ctrl+C to quit.");

    // Event loop — trigger on CC press (val>0), ignore release (val=0)
    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(midi::MidiEvent::ControlChange { controller, value, .. }) if value > 0 => {
                if let Some(scene) = pedal_map.get(&controller) {
                    println!("Pedal CC {} → switching to scene: {}", controller, scene);
                    if let Err(e) = obs_client.set_scene(scene) {
                        log::warn!("Failed to switch scene: {}. Reconnecting...", e);
                        obs_client = connect_obs(&config.obs, &running);
                    }
                } else {
                    println!("CC {} not mapped to any scene (val={})", controller, value);
                }
            }
            Ok(event) => {
                log::debug!("Ignoring event: {:?}", event);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    println!("Goodbye.");
}

/// Connect to OBS with exponential backoff. Blocks until connected or shutdown.
fn connect_obs(obs_config: &config::ObsConfig, running: &Arc<AtomicBool>) -> obs::ObsClient {
    let mut delay = std::time::Duration::from_secs(2);
    let max_delay = std::time::Duration::from_secs(30);

    loop {
        if !running.load(Ordering::SeqCst) {
            eprintln!("Shutdown requested during OBS connection. Exiting.");
            std::process::exit(0);
        }
        match obs::ObsClient::connect(&obs_config.host, obs_config.port, &obs_config.password) {
            Ok(client) => {
                println!("Connected to OBS at {}:{}", obs_config.host, obs_config.port);
                return client;
            }
            Err(e) => {
                log::warn!(
                    "Failed to connect to OBS: {}. Retrying in {}s...",
                    e,
                    delay.as_secs()
                );
                // Sleep in small increments so we can respond to Ctrl+C
                let sleep_until = std::time::Instant::now() + delay;
                while std::time::Instant::now() < sleep_until {
                    if !running.load(Ordering::SeqCst) {
                        eprintln!("Shutdown requested during OBS connection. Exiting.");
                        std::process::exit(0);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                delay = (delay * 2).min(max_delay);
            }
        }
    }
}
