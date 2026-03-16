mod config;
mod midi;
mod obs;
mod status;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    let long_press_map = config.long_press_map();
    let scene_names: Vec<String> = config.pedal.iter().map(|p| p.scene.clone()).collect();
    println!("Loaded {} pedal mappings ({} with long press).", pedal_map.len(), long_press_map.len());

    // Initialize status for waybar
    let mut _status = status::Status::new(scene_names);

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

    // Connect to OBS and sync initial state
    let mut obs_client = connect_obs(&config.obs, &running);
    sync_status(&mut obs_client, &mut _status);

    println!("Ready. Tap a pedal to switch scenes. Ctrl+C to quit.");

    // Track press timestamps for long press detection
    let mut press_times: HashMap<u8, Instant> = HashMap::new();

    // Event loop
    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            // CC press (val > 0)
            Ok(midi::MidiEvent::ControlChange { controller, value, .. }) if value > 0 => {
                press_times.insert(controller, Instant::now());

                // If no long press configured for this CC, fire scene switch immediately
                if !long_press_map.contains_key(&controller) {
                    if let Some(scene) = pedal_map.get(&controller) {
                        println!("Pedal CC {} → switching to scene: {}", controller, scene);
                        if let Err(e) = obs_client.set_scene(scene) {
                            log::warn!("Failed to switch scene: {}. Reconnecting...", e);
                            obs_client = connect_obs(&config.obs, &running);
                        }
                        _status.set_scene(scene);
                    } else {
                        println!("CC {} not mapped to any scene (val={})", controller, value);
                    }
                }
            }
            // CC release (val = 0)
            Ok(midi::MidiEvent::ControlChange { controller, value: 0, .. }) => {
                if let Some(press_time) = press_times.remove(&controller) {
                    let held_ms = press_time.elapsed().as_millis() as u64;

                    if let Some((action, threshold_ms)) = long_press_map.get(&controller) {
                        if held_ms >= *threshold_ms {
                            // Long press — execute action
                            println!("Pedal CC {} long press ({}ms) → {}", controller, held_ms, action);
                            if let Err(e) = execute_action(&mut obs_client, action) {
                                log::warn!("Failed to execute {}: {}. Reconnecting...", action, e);
                                obs_client = connect_obs(&config.obs, &running);
                            }
                            if action == "toggle_record" {
                                _status.toggle_recording();
                            }
                        } else {
                            // Short press — switch scene
                            if let Some(scene) = pedal_map.get(&controller) {
                                println!("Pedal CC {} tap ({}ms) → switching to scene: {}", controller, held_ms, scene);
                                if let Err(e) = obs_client.set_scene(scene) {
                                    log::warn!("Failed to switch scene: {}. Reconnecting...", e);
                                    obs_client = connect_obs(&config.obs, &running);
                                }
                                _status.set_scene(scene);
                            }
                        }
                    }
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

/// Query OBS for current scene and recording status, update waybar.
fn sync_status(obs_client: &mut obs::ObsClient, status: &mut status::Status) {
    match obs_client.get_current_scene() {
        Ok(scene) if !scene.is_empty() => {
            println!("OBS current scene: {}", scene);
            status.set_scene(&scene);
        }
        Ok(_) => log::warn!("Could not determine current scene"),
        Err(e) => log::warn!("Failed to query current scene: {}", e),
    }
    match obs_client.get_record_status() {
        Ok(recording) => {
            println!("OBS recording: {}", recording);
            status.recording = recording;
            // Force a write to update waybar
            status.set_scene(&status.current_scene.clone());
        }
        Err(e) => log::warn!("Failed to query recording status: {}", e),
    }
}

/// Execute a named action on OBS.
fn execute_action(obs_client: &mut obs::ObsClient, action: &str) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        "toggle_record" => obs_client.toggle_record(),
        _ => {
            log::warn!("Unknown action: {}", action);
            Ok(())
        }
    }
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
