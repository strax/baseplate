#![feature(mem_take)]

use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use async_std::{net::UdpSocket, task};
use async_std::net::ToSocketAddrs;
use async_std::sync::Arc;
use bincode;
use bytes::Bytes;
use crossbeam::atomic::AtomicCell;
use crossbeam::channel as chan;
use futures::channel::mpsc;
use futures::executor;
use futures::executor::ThreadPool;
use futures::lock::Mutex;
use futures::prelude::*;
use futures::select;
use futures::stream::FusedStream;
use futures_timer::Delay;
use log::{debug, error, info, trace, warn};
use pin_utils::pin_mut;

use shared::{hexdump, packet::Packet, proto};
use shared::{handshake::*, logging, proto::*};

struct Conn {
    server_sequence: Arc<AtomicU32>,
    client_sequence: Arc<AtomicU32>,
    socket: UdpSocket,
    remote: SocketAddr
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn keep_alive(conn: Arc<Conn>) {
    loop {
        Delay::new(Duration::from_secs(5)).await;
        conn.send(Message::Heartbeat).await;
    }
}

impl Conn {
    async fn connect(remote: SocketAddr) -> Result<Conn> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        trace!("socket created");
        let conn = Conn { server_sequence: Arc::new(AtomicU32::new(0)), client_sequence: Arc::new(AtomicU32::new(1)), socket, remote };

        // First send connect message
        trace!("sending connect msg");
        conn.send(Message::Connect).await;

        trace!("sent connect msg");

        // Now we should receive a challenge nonce
        match conn.next_message().await {
            Message::Handshake(HandshakeMessage::Challenge(nonce)) => {
                conn.send(Message::Handshake(HandshakeMessage::Challenge(nonce))).await;
            }
            _ => {
                panic!("handshake failed");
            }
        }
        match conn.next_message().await {
            Message::Handshake(HandshakeMessage::Success) => {
                Ok(conn)
            }
            _ => {
                panic!("handshake failed");
            }
        }
    }

    async fn send(&self, msg: Message) -> Result<()> {
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

    async fn next_message(&self) -> Message {
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

async fn run() -> Result<()> {
    trace!("client starting");

    // Bind an UDP socket to the same port
    let conn = Arc::new(Conn::connect(SocketAddr::from_str("127.0.0.1:12345")?).await?);
    info!("connection estabilished");

    task::spawn(keep_alive(conn.clone()));

    loop {
        let message = conn.next_message().await;
        trace!("should handle message");
        // let next_message = conn.next_message().boxed().fuse();
        // pin_mut!(next_message);
//        select! {
//            msg = next_message => {
//                trace!("{:?}", msg);
//                panic!();
//            }
//        };
    }


    Ok(())
}

fn main() -> Result<()> {
    logging::setup()?;

    executor::block_on(run())?;
    Ok(())
}
