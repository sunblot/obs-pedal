# obs-pedal

A lightweight Rust daemon that maps USB-MIDI foot pedal presses to OBS scene switches via WebSocket.

Built for the **Roland FS-1-WL** (3 pedals), but should work with any MIDI controller that sends Control Change messages.
...

## How it works

1. Discovers the MIDI device by name (no hardcoded paths)
2. Listens for Control Change messages (press = val > 0)
3. Maps CC numbers to OBS scenes via config file
4. Switches scenes over OBS WebSocket 5.x
5. Auto-reconnects to OBS if the connection drops

## Setup

### Build

```bash
cargo build --release
```

### Configure

```bash
cp config.toml.example config.toml
```

Edit `config.toml` with your OBS WebSocket details and scene names:

```toml
[obs]
host = "127.0.0.1"
port = 4455
password = "your-obs-websocket-password"

[[pedal]]
cc = 80
scene = "Camera"

[[pedal]]
cc = 81
scene = "Screen"

[[pedal]]
cc = 82
scene = "Both"
```

To discover your pedal's CC numbers, run the daemon and tap each pedal — unmapped CCs are printed to stdout.

### Run

```bash
cargo run
# or
./target/release/obs-pedal --config /path/to/config.toml
```

### Systemd (optional)

Create `~/.config/systemd/user/obs-pedal.service`:

```ini
[Unit]
Description=OBS Pedal - MIDI foot pedal to OBS scene switcher
After=graphical-session.target

[Service]
Type=simple
ExecStart=/path/to/obs-pedal --config /path/to/config.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=graphical-session.target
```

Then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now obs-pedal
```

## Requirements

- Linux (uses ALSA via `midir`)
- OBS with WebSocket server enabled (OBS 28+ has it built in)
- A USB-MIDI device

## License

MIT
