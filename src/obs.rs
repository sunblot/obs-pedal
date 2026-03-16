use base64::Engine;
use sha2::{Digest, Sha256};
use std::net::TcpStream;
use tungstenite::{connect, Message, WebSocket, stream::MaybeTlsStream};
use serde_json::{json, Value};

pub struct ObsClient {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl ObsClient {
    /// Connect to OBS WebSocket and authenticate.
    pub fn connect(host: &str, port: u16, password: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let url = format!("ws://{}:{}", host, port);
        log::info!("Connecting to OBS at {}", url);
        let (mut ws, _response) = connect(&url)?;

        // Read Hello message
        let hello_msg = ws.read()?;
        let hello: Value = serde_json::from_str(hello_msg.to_text()?)?;
        log::debug!("OBS Hello: {}", hello);

        // Authenticate if required
        if let Some(auth) = hello.get("d").and_then(|d| d.get("authentication")) {
            let challenge = auth["challenge"]
                .as_str()
                .ok_or("Missing challenge in Hello")?;
            let salt = auth["salt"]
                .as_str()
                .ok_or("Missing salt in Hello")?;

            let auth_string = Self::compute_auth(password, salt, challenge);

            let identify = json!({
                "op": 1,
                "d": {
                    "rpcVersion": 1,
                    "authentication": auth_string,
                }
            });
            ws.send(Message::Text(identify.to_string().into()))?;

            // Read Identified response
            let resp_msg = ws.read()?;
            let resp: Value = serde_json::from_str(resp_msg.to_text()?)?;
            let op = resp.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
            if op != 2 {
                return Err(format!("Expected Identified (op=2), got op={}: {}", op, resp).into());
            }
            log::info!("Authenticated with OBS");
        } else {
            // No auth required, send Identify without auth
            let identify = json!({
                "op": 1,
                "d": {
                    "rpcVersion": 1,
                }
            });
            ws.send(Message::Text(identify.to_string().into()))?;
            let _ = ws.read()?;
            log::info!("Connected to OBS (no auth required)");
        }

        Ok(Self { ws })
    }

    /// OBS WebSocket 5.x auth: Base64(SHA256(Base64(SHA256(password + salt)) + challenge))
    fn compute_auth(password: &str, salt: &str, challenge: &str) -> String {
        let b64 = base64::engine::general_purpose::STANDARD;

        // Step 1: secret = Base64(SHA256(password + salt))
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt.as_bytes());
        let secret = b64.encode(hasher.finalize());

        // Step 2: auth = Base64(SHA256(secret + challenge))
        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        hasher.update(challenge.as_bytes());
        b64.encode(hasher.finalize())
    }

    /// Switch to the named scene (fire-and-forget).
    pub fn set_scene(&mut self, scene_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let request_id = uuid_simple();
        let msg = json!({
            "op": 6,
            "d": {
                "requestType": "SetCurrentProgramScene",
                "requestId": request_id,
                "requestData": {
                    "sceneName": scene_name,
                }
            }
        });
        self.ws.send(Message::Text(msg.to_string().into()))?;
        log::info!("Sent SetCurrentProgramScene: {}", scene_name);
        Ok(())
    }
}

/// Simple unique ID without pulling in the uuid crate.
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}
