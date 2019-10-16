use async_std::sync::Arc;
use async_std::net::UdpSocket;
use std::net::SocketAddr;
use shared::{Result, hexdump};
use std::time::Duration;
use shared::proto::Message;
use bytes::Bytes;
use shared::packet::Packet;
use shared::handshake::HandshakeMessage;
use snafu::{Snafu};
use log::*;
use std::sync::atomic::{Ordering, AtomicU32};
use async_std::future::timeout;
use futures::executor;

#[derive(Snafu, Debug)]
enum ConnError {
    #[snafu(display("handshake failure"))]
    HandshakeFailure
}

pub struct Conn {
    server_sequence: Arc<AtomicU32>,
    client_sequence: Arc<AtomicU32>,
    socket: UdpSocket,
    remote: SocketAddr
}

impl Conn {
    pub async fn connect(remote: SocketAddr) -> Result<Conn> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        trace!("socket created");
        let conn = Conn { server_sequence: Arc::new(AtomicU32::new(0)), client_sequence: Arc::new(AtomicU32::new(1)), socket, remote };

        // First send connect message
        trace!("sending connect msg");
        conn.send(Message::Connect).await?;

        trace!("sent connect msg");

        // Now we should receive a challenge nonce
        match timeout(Duration::from_secs(5), conn.next_message()).await? {
            Message::Handshake(HandshakeMessage::Challenge(nonce)) => {
                conn.send(Message::Handshake(HandshakeMessage::Challenge(nonce))).await?;
            }
            _ => {
                return Err(ConnError::HandshakeFailure.into())
            }
        }
        match timeout(Duration::from_secs(5), conn.next_message()).await? {
            Message::Handshake(HandshakeMessage::Success) => {
                Ok(conn)
            }
            _ => {
                return Err(ConnError::HandshakeFailure.into())
            }
        }
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        let data = Bytes::from(bincode::serialize(&msg)?);
        let packet = Packet::new(self.client_sequence.load(Ordering::SeqCst), data);
        let bytes = packet.to_bytes()?;
        trace!("SEND {:?}\n{}", msg, hexdump(&bytes));
        self.socket.send_to(&bytes, &self.remote).await?;
        self.client_sequence.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn recv1(&self) -> Result<Packet> {
        trace!("recv1");
        let mut buffer = [0u8; 65507];
        let (size, remote) = self.socket.recv_from(&mut buffer).await?;
        if remote != self.remote {
            panic!("wrong remote")
        }
        let datagram = Bytes::from(&buffer[..size]);
        trace!("RECV <bytes>\n{}", hexdump(&datagram));
        let packet = Packet::from_bytes(datagram)?;
        Ok(packet)
    }

    pub async fn next_message(&self) -> Message {
        loop {
            let server_sequence = self.server_sequence.clone();
            if let Ok(packet) = self.recv1().await {
                if packet.sequence_number > server_sequence.load(Ordering::SeqCst) {
                    match bincode::deserialize::<Message>(&packet.message) {
                        Ok(message) => {
                            self.server_sequence.store(packet.sequence_number, Ordering::SeqCst);
                            debug!("RECV {:?}", message);
                            return message;
                        }
                        Err(err) => {
                            warn!("error decoding message: {}", err);
                        }
                    }
                } else {
                    warn!("received packet with stale sequence number");
                }
            }
            // otherwise reading failed and we try again
        }
    }
}
