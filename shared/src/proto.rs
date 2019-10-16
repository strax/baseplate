use serde_derive::{Deserialize, Serialize};
use super::handshake::HandshakeMessage;

#[derive(Debug, Deserialize, Serialize)]
pub enum Message {
    Connect,
    Handshake(HandshakeMessage),
    Heartbeat
}

