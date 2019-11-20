use super::handshake::HandshakeMessage;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum Message {
    Connect,
    Disconnect,
    Handshake(HandshakeMessage),
    Heartbeat,
    Refresh(Vec<(f32, f32)>),
    Move { dx: f32, dy: f32 },
}
