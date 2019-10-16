use serde_derive::{Deserialize, Serialize};
use super::handshake::HandshakeMessage;
use crate::state::State;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum Message {
    Connect,
    Handshake(HandshakeMessage),
    Heartbeat,
    Refresh(Vec<(f32, f32)>),
    Move { dx: f32, dy: f32 }
}

