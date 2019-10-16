mod session;

use async_std::net::{UdpSocket};
use std::net::{SocketAddr, IpAddr};
use std::error::Error;
use log::{info, warn, trace, LevelFilter};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use std::thread::{Thread, JoinHandle};
use std::io::{Read};
use bytes::Bytes;
use session::*;
use std::pin::Pin;
use std::borrow::{BorrowMut, Borrow};
use shared::{packet::Packet, hexdump, proto};
use rand::random;
use std::str::FromStr;
use shared::Result;
use futures::channel::mpsc as chan;
use futures::lock::Mutex;
use async_std::sync::Arc;
use async_std::task;
use futures::executor;
use shared::proto::Message;
use futures::future;

#[derive(Debug)]
pub enum SessionMessage {
    Send(proto::Message),
    Recv(Packet),
    Stop
}

#[derive(Debug)]
struct State {
    sessions: HashMap<SocketAddr, Session>
}

impl State {
    fn new() -> Self {
        State { sessions: HashMap::new() }
    }

    async fn broadcast(&mut self, msg: Message) {
        future::join_all(self.sessions.values_mut().map(|s| s.send(&msg))).await;
    }
}

async fn async_main() {
    shared::logging::setup().unwrap();

    let addr = SocketAddr::from_str("0.0.0.0:12345").unwrap();

    let socket = Arc::new(UdpSocket::bind("0.0.0.0:12345").await.unwrap());
    info!("udp socket bound to {}", addr);
    let state = Arc::new(Mutex::new(State::new()));
    future::join(read_socket(state.clone(), socket.clone()), tick_loop(state.clone())).await;
}

fn main() -> () {
    executor::block_on(async_main());
}

async fn tick_loop(state: Arc<Mutex<State>>) -> () {
    loop {
        task::sleep(Duration::from_millis(16)).await;
        let mut state = state.lock().await;
        let positions = state.sessions.values().map(|s| s.pos()).collect();
        state.broadcast(Message::Refresh({ positions })).await
    }
}

async fn read_socket(state: Arc<Mutex<State>>, socket: Arc<UdpSocket>) -> () {
    let mut buffer = [0u8;65507];
    loop {
        let (size, remote) = socket.recv_from(&mut buffer).await.unwrap();
        let dgram = Bytes::from(&buffer[..size]);
        trace!("RECV <bytes> from {}:\n{}", remote, hexdump(&dgram));
        let mut state = state.lock().await;
        trace!("acquired read lock on server state");
        let mut session = state.sessions.entry(remote).or_insert_with(|| Session::new(remote, socket.clone()));
        match Packet::from_bytes(dgram) {
            Ok(packet) => {
                trace!("valid packet, forwarding");
                session.on_packet(packet).await;
            },
            Err(err) => {
                warn!("decode error: {}", err);
            }
        }
    }
}
