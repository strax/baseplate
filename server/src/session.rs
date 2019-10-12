use std::thread;
use log::{info, warn, trace};
use crossbeam::channel as chan;
use super::SessionMessage;
use std::pin::Pin;
use std::net::{UdpSocket, SocketAddr};
use std::error::Error;
use shared::{hexdump, Packet};
use bytes::Bytes;

#[derive(Debug)]
pub struct Session {
    client_sequence: u32,
    server_sequence: u32,
    remote: SocketAddr,
    rx: chan::Receiver<SessionMessage>,
    socket: UdpSocket
}

impl Session {
    pub fn create(remote: SocketAddr, socket: UdpSocket) -> chan::Sender<SessionMessage> {
        // Make a new channel as sender/recipient pair
        let (tx, rx) = chan::unbounded();

        // Create a new session actor and start it in a separate thread
        let mut session = Session { remote, rx, socket, client_sequence: 0, server_sequence: 0 };
        thread::Builder::new().name(format!("session/{}", remote)).spawn(move || session.act());

        // Return the transmission part of the channel
        tx
    }

    fn act(&mut self) -> () {
        trace!("starting actor");
        for msg in self.rx.iter() {
            match msg {
                SessionMessage::Stop => break,
                SessionMessage::Recv(packet) => {
                    trace!("received sequence number = {}", packet.sequence_number);
                    self.client_sequence += 1;
                    self.send(Bytes::from_static(b"HELLO"));
                },
                SessionMessage::Send(bytes) => self.send(bytes).unwrap()
            }
        }
        trace!("session handler thread exhausted of work");
    }

    fn send(&self, bytes: Bytes) -> Result<(), Box<dyn Error>> {
        trace!("SEND {}", hexdump(&bytes));
        self.socket.send_to(&bytes, self.remote)?;
        Ok(())
    }
}
