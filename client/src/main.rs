#![feature(cfg_target_has_atomic)]

use std::net::{TcpStream, UdpSocket, SocketAddr};
use std::error::Error;
use log::{trace, info, warn, error, debug};
use std::thread;
use crossbeam::channel as chan;
use std::time::Duration;
use shared::{packet::Packet, proto, hexdump};
use crossbeam::atomic::AtomicCell;
use std::sync::Arc;
use bytes::Bytes;
use bincode;
use shared::{proto::*, handshake::*, logging};

struct Conn {
    server_sequence: u32,
    client_sequence: u32,
    socket: UdpSocket
}

impl Conn {
    fn connect(binding: SocketAddr, remote: SocketAddr) -> Result<Conn, Box<dyn Error>> {
        let socket = UdpSocket::bind(binding)?;
        socket.connect(remote)?;
        let mut conn = Conn { server_sequence: 0, client_sequence: 1, socket };

        // First send connect message
        conn.send(Message::Connect);
        // Now we should receive a challenge nonce
        match conn.next_message()? {
            Message::Handshake(HandshakeMessage::Challenge(nonce)) => {
                conn.send(Message::Handshake(HandshakeMessage::Challenge(nonce)))?;
            },
            _ => {
                panic!("handshake failed");
            }
        }
        match conn.next_message()? {
            Message::Handshake(HandshakeMessage::Success) => {
                conn.start_heartbeat_tick();
                Ok(conn)
            },
            _ => {
                panic!("handshake failed");
            }
        }
    }

    fn send(&mut self, msg: Message) -> Result<(), Box<dyn Error>> {
        let data = Bytes::from(bincode::serialize(&msg)?);
        let packet = Packet::new(self.client_sequence, data);
        let bytes = packet.to_bytes()?;
        trace!("SEND {:?}\n{}", msg, hexdump(&bytes));
        self.socket.send(&bytes);
        self.client_sequence += 1;
        Ok(())
    }

    fn recv1(&self) -> Result<Packet, Box<dyn Error>> {
        let mut buffer = [0u8;65507];
        let size = self.socket.recv(&mut buffer)?;
        let datagram = Bytes::from(&buffer[..size]);
        trace!("RECV <bytes>\n{}", hexdump(&datagram));
        let packet = Packet::from_bytes(datagram)?;
        Ok(packet)
    }

    fn next_message(&mut self) -> Result<Message, Box<dyn Error>> {
        loop {
            if let Ok(packet) = self.recv1() {
                if packet.sequence_number > self.server_sequence {
                    match bincode::deserialize::<Message>(&packet.message) {
                        Ok(message) => {
                            self.server_sequence = packet.sequence_number;
                            debug!("RECV {:?}", message);
                            return Ok(message);
                        },
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

fn main() -> Result<(), Box<dyn Error>> {
    logging::setup()?;

    trace!("client starting");

    // First initialize a TCP connection to the server
    let mut tcp_stream = TcpStream::connect("127.0.0.1:12345")?;
    trace!("tcp stream created");
    let addr = tcp_stream.local_addr()?;
    trace!("local port is {}", addr);
    // Bind an UDP socket to the same port
    let mut conn = Conn::connect(tcp_stream.local_addr()?, tcp_stream.peer_addr()?)?;
    info!("connection estabilished");

    loop {
        match conn.next_message() {
            Ok(msg) => {
                trace!("{:?}", msg);
            },
            Err(err) => {
                error!("{}", err);
            }
        }
    }

    Ok(())
}
