use std::thread;
use log::{info, warn, trace, debug, error};
use crossbeam::channel as chan;
use super::SessionMessage;
use std::pin::Pin;
use std::net::{UdpSocket, SocketAddr};
use std::error::Error;
use shared::{hexdump, packet::Packet, proto::*, handshake::*};
use bytes::Bytes;
use rand::random;

#[derive(Debug)]
pub struct Session {
    client_sequence: u32,
    server_sequence: u32,
    remote: SocketAddr,
    rx: chan::Receiver<SessionMessage>,
    socket: UdpSocket,
    handshake: HandshakeState
}

impl Session {
    pub fn create(remote: SocketAddr, socket: UdpSocket) -> chan::Sender<SessionMessage> {
        // Make a new channel as sender/recipient pair
        let (tx, rx) = chan::unbounded();

        // Create a new session actor and start it in a separate thread
        let mut session = Session { remote, rx, socket, client_sequence: 0, server_sequence: 1, handshake: HandshakeState::Disconnected };
        thread::Builder::new().name(format!("session/{}", remote)).spawn(move || session.act());

        // Return the transmission part of the channel
        tx
    }

    fn act(&mut self) -> () {
        trace!("starting actor");
        while let Ok(msg) = self.rx.recv() {
            match msg {
                SessionMessage::Stop => break,
                SessionMessage::Recv(packet) => {
                    if packet.sequence_number > self.client_sequence {
                        trace!("RECV {:?}", packet);
                        self.client_sequence = packet.sequence_number;
                        let message = bincode::deserialize::<Message>(&packet.message).unwrap();
                        debug!("RECV {:?}", message);
                        match message {
                            Message::Connect => {
                                self.on_connect();
                            },
                            Message::Handshake(handshake_msg) => {
                                self.on_handshake_message(handshake_msg);
                            },
                            Message::Heartbeat => {
                                self.send(Message::Heartbeat).unwrap();
                            }
                        }
                    } else {
                        warn!("sequence out of sync, ignoring packet");
                    }
                },
                SessionMessage::Send(msg) => {
                    self.send(msg).unwrap();
                }
            }
        }
        trace!("session handler thread exhausted of work");
    }

    fn on_connect(&mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        let nonce = random::<u32>();
        if self.handshake == HandshakeState::Disconnected {
            self.handshake = HandshakeState::Negotiating { nonce };
            self.send(Message::Handshake(HandshakeMessage::Challenge(nonce)))?;
        } else {
            // Handshake is already in progress, ignore packet
            warn!("duplicate connection attempt, handshake already in progress");
        }
        Ok(())
    }

    fn on_handshake_message(&mut self, msg: HandshakeMessage) {
        match (self.handshake.clone(), msg) {
            (HandshakeState::Negotiating { nonce }, HandshakeMessage::Challenge(ack)) => {
                if nonce == ack {
                    // challenge authorized
                    self.handshake = HandshakeState::Connected;
                    info!("connection transitioned to CONNECTED");
                    self.send(Message::Handshake(HandshakeMessage::Success));
                } else {
                    // invalid nonce
                    warn!("received nonce differs");
                    self.handshake = HandshakeState::Disconnected;
                    error!("connection transitioned to DISCONNECTED");
                    self.send(Message::Handshake(HandshakeMessage::Failure));
                }
            },
            (state, msg) => {
                warn!("invalid state, message pair: ({:?}, {:?})", state, msg);
            }
        }
    }

    fn send(&mut self, msg: Message) -> Result<(), Box<dyn Error + Sync + Send>> {
        let data = Bytes::from(bincode::serialize(&msg)?);
        debug!("SEND {:?}", msg);
        let packet = Packet::new(self.server_sequence, data);
        let wire_bytes = packet.to_bytes()?;
        trace!("SEND to {}\n{:?}\n{}", self.remote, packet, hexdump(&wire_bytes));
        self.socket.send_to(&wire_bytes, self.remote)?;
        self.server_sequence += 1;
        Ok(())
    }
}
