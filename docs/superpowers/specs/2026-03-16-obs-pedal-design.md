# obs-pedal: USB-MIDI Foot Pedal to OBS Scene Switcher

## Purpose

A Rust daemon that listens to a Roland FS-1-WL USB-MIDI foot pedal (3 pedals) and switches OBS scenes via the OBS WebSocket 5.x protocol. Each pedal maps to one scene.

## Hardware

- **Device:** Roland FS-1-WL USB-MIDI (USB ID `0582:02bd`)
- **MIDI port:** Discovered by substring match on port name containing "FS-1-WL" (not hardcoded)
- **Pedals:** 3 foot switches

## Architecture

```
obs-pedal (single binary)
├── main.rs      — CLI entry, config loading, opens MIDI port, runs event loop
├── midi.rs      — MIDI port discovery and pedal event parsing
├── obs.rs       — OBS WebSocket 5.x client for scene switching
└── config.rs    — Config file parsing
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| `midir` | Cross-platform MIDI I/O (wraps ALSA on Linux) |
| `tungstenite` | WebSocket client for OBS connection |
| `serde` + `toml` | Config file parsing |
| `sha2` + `base64` | OBS WebSocket authentication (RustCrypto SHA-256) |
| `log` + `env_logger` | Structured logging |

### Concurrency Model

`midir` delivers MIDI messages via a callback on its own thread. The main thread owns the WebSocket connection to OBS.

- MIDI callback sends pedal events over an `std::sync::mpsc::channel`
- Main thread loop receives from the channel and sends WebSocket messages
- No shared mutable state, no mutexes needed

## Stages

### Stage 1: MIDI Listener

- Enumerate MIDI input ports via `midir`
- Find the Roland FS-1-WL port by matching on port name (substring match on "FS-1-WL")
- Open the port and listen for incoming MIDI messages
- Print each message to stdout: timestamp, message type, channel, note/CC number, velocity
- This stage is for discovery — we need to learn what exact MIDI messages each pedal sends (Note On, Control Change, which note numbers, etc.)
- Scene switch triggers on Note On (velocity > 0) only; Note Off and zero-velocity events are ignored. (To be confirmed after stage 1 discovery.)
- Accept all MIDI channels (no channel filtering) since this is a single-device setup.

### Stage 2: OBS Integration

- Connect to OBS WebSocket at configured host:port
- Authenticate using the OBS WebSocket 5.x auth protocol:
  1. Receive `Hello` message with `authentication.challenge` and `authentication.salt`
  2. Compute `secret = Base64(SHA256(password + salt))`
  3. Compute `auth = Base64(SHA256(secret + challenge))`
  4. Send `Identify` message with `authentication` field
- On pedal press, send `SetCurrentProgramScene` request with the mapped scene name
- Reconnect on connection loss with exponential backoff: retry every 2s, doubling up to 30s max, retry indefinitely

## Configuration

File: `config.toml` in the working directory (overridable with `--config <path>` CLI flag).

```toml
[obs]
host = "127.0.0.1"
port = 4455
password = "your-obs-websocket-password"

# Keys are MIDI note/CC numbers as strings (discovered in stage 1)
# Values are OBS scene names
[[pedal]]
note = 60
scene = "Camera 1"

[[pedal]]
note = 62
scene = "Screen Share"

[[pedal]]
note = 64
scene = "Wide Shot"
```

The pedal MIDI note numbers are placeholders — we'll fill them in after stage 1 discovery.

## Error Handling

- If the MIDI device is not found: print available ports and exit with a clear error message
- If OBS WebSocket is unreachable (stage 2): retry with exponential backoff, log warnings
- If a scene name doesn't exist in OBS: log a warning, don't crash
- On Ctrl+C: close MIDI port and WebSocket cleanly (handle SIGINT)

## Non-Goals

- No GUI
- No hot-reload of config (restart to pick up changes)
- No systemd integration (manual start for now)
- No support for multiple MIDI devices simultaneously
