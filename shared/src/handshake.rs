use serde_derive::{Deserialize, Serialize};

#[derive(Debug, PartialOrd, PartialEq, Eq, Clone)]
pub enum HandshakeState {
    Disconnected,
    Negotiating { nonce: u32 },
    Connected
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum HandshakeMessage {
    Challenge(u32),
    Success,
    Failure
}
