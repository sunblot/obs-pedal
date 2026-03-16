use std::io::Write;

const STATE_PATH: &str = "/tmp/obs-pedal-state.json";
// Waybar signal 11 = SIGRTMIN(34) + 11 = signal 45
const WAYBAR_SIGNAL: &str = "45";

pub struct Status {
    pub current_scene: String,
    pub recording: bool,
    pub scenes: Vec<String>,
}

impl Status {
    pub fn new(scenes: Vec<String>) -> Self {
        let current_scene = scenes.first().cloned().unwrap_or_default();
        let status = Self {
            current_scene,
            recording: false,
            scenes,
        };
        status.write();
        status
    }

    pub fn set_scene(&mut self, scene: &str) {
        self.current_scene = scene.to_string();
        self.write();
    }

    pub fn toggle_recording(&mut self) {
        self.recording = !self.recording;
        self.write();
    }

    fn write(&self) {
        let json = serde_json::json!({
            "current_scene": self.current_scene,
            "recording": self.recording,
            "scenes": self.scenes,
        });

        // Write atomically via temp file + rename
        let tmp = format!("{}.tmp", STATE_PATH);
        if let Ok(mut f) = std::fs::File::create(&tmp) {
            let _ = f.write_all(json.to_string().as_bytes());
            let _ = std::fs::rename(&tmp, STATE_PATH);
        }

        signal_waybar();
    }
}

impl Drop for Status {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(STATE_PATH);
        signal_waybar();
    }
}

fn signal_waybar() {
    let _ = std::process::Command::new("pkill")
        .args([&format!("-{}", WAYBAR_SIGNAL), "-x", "waybar"])
        .spawn();
}
