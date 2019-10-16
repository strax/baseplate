use std::thread;
use log::{info, warn, trace, debug, error};
use futures::channel::mpsc as chan;
use futures::stream::StreamExt;
use super::SessionMessage;
use std::pin::Pin;
use std::net::{SocketAddr};
use async_std::net::{UdpSocket};
use std::error::Error;
use shared::{hexdump, packet::Packet, proto::*, handshake::*};
use bytes::Bytes;
use rand::random;
use async_std::sync::Arc;
use async_std::task;
use futures::future::{Abortable, AbortHandle};

#[derive(Debug)]
pub struct Session {
    client_sequence: u32,
    server_sequence: u32,
    remote: SocketAddr,
    // rx: chan::UnboundedReceiver<SessionMessage>,
    socket: Arc<UdpSocket>,
    handshake: HandshakeState,
    pos: (f32, f32),
    disconnected: bool
}

impl Session {
    pub fn pos(&self) -> (f32, f32) {
        self.pos
    }

    pub fn new(remote: SocketAddr, socket: Arc<UdpSocket>) -> Session {
        Session { remote, socket, client_sequence: 0, server_sequence: 1, handshake: HandshakeState::Disconnected, pos: (0.0, 0.0), disconnected: false }
    }

    pub fn disconnected(&self) -> bool {
        self.disconnected
    }

    pub async fn on_packet(&mut self, packet: Packet) -> () {
        if packet.sequence_number > self.client_sequence {
            trace!("RECV {:?}", packet);
            self.client_sequence = packet.sequence_number;
            let message = bincode::deserialize::<Message>(&packet.message).unwrap();
            debug!("RECV {:?}", message);
            match message {
                Message::Connect => {
                    self.on_connect().await;
                },
                Message::Handshake(handshake_msg) => {
                    self.on_handshake_message(handshake_msg).await;
                },
                Message::Heartbeat => {
                    self.send(&Message::Heartbeat).await.unwrap();
                },
                Message::Move { dx, dy } => {
                    self.pos = (self.pos.0 + dx, self.pos.1 + dy);
                },
                Message::Disconnect => {
                    self.disconnected = true;
                }
                _ => {}
            }
        } else {
            warn!("sequence out of sync, ignoring packet");
        }
    }

    async fn on_connect(&mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        let nonce = random::<u32>();
        if self.handshake == HandshakeState::Disconnected {
            self.handshake = HandshakeState::Negotiating { nonce };
            self.send(&Message::Handshake(HandshakeMessage::Challenge(nonce))).await?;
        } else {
            // Handshake is already in progress, ignore packet
            warn!("duplicate connection attempt, handshake already in progress");
        }
        Ok(())
    }

    async fn on_handshake_message(&mut self, msg: HandshakeMessage) {
        match (self.handshake.clone(), msg) {
            (HandshakeState::Negotiating { nonce }, HandshakeMessage::Challenge(ack)) => {
                if nonce == ack {
                    // challenge authorized
                    self.handshake = HandshakeState::Connected;
                    info!("connection transitioned to CONNECTED");
                    self.send(&Message::Handshake(HandshakeMessage::Success)).await;
                } else {
                    // invalid nonce
                    warn!("received nonce differs");
                    self.handshake = HandshakeState::Disconnected;
                    error!("connection transitioned to DISCONNECTED");
                    self.send(&Message::Handshake(HandshakeMessage::Failure)).await;
                }
            },
            (state, msg) => {
                warn!("invalid state, message pair: ({:?}, {:?})", state, msg);
            }
        }
    }

    pub async fn send(&mut self, msg: &Message) -> Result<(), Box<dyn Error + Sync + Send>> {
        let data = Bytes::from(bincode::serialize(&msg)?);
        debug!("SEND {:?}", msg);
        let packet = Packet::new(self.server_sequence, data);
        let wire_bytes = packet.to_bytes()?;
        trace!("SEND to {}\n{:?}\n{}", self.remote, packet, hexdump(&wire_bytes));
        self.socket.send_to(&wire_bytes, self.remote).await?;
        self.server_sequence += 1;
        Ok(())
    }
}
