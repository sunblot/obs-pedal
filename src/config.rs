use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub obs: ObsConfig,
    #[serde(default)]
    pub pedal: Vec<PedalMapping>,
}

#[derive(Debug, Deserialize)]
pub struct ObsConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct PedalMapping {
    pub cc: u8,
    pub scene: String,
    pub long_press: Option<String>,
    /// Hold duration in milliseconds to trigger long press (default 500)
    pub hold_ms: Option<u64>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Build a HashMap from MIDI CC number to scene name for fast lookup.
    pub fn pedal_map(&self) -> HashMap<u8, String> {
        self.pedal.iter().map(|p| (p.cc, p.scene.clone())).collect()
    }

    /// Build a HashMap from MIDI CC number to (action, hold_ms) for long press.
    pub fn long_press_map(&self) -> HashMap<u8, (String, u64)> {
        self.pedal
            .iter()
            .filter_map(|p| {
                p.long_press.as_ref().map(|action| {
                    (p.cc, (action.clone(), p.hold_ms.unwrap_or(500)))
                })
            })
            .collect()
    }
}
