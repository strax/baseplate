mod session;

use async_std::net::UdpSocket;
use std::net::SocketAddr;

use log::{info, trace, warn};
use std::collections::HashMap;

use std::time::Duration;

use bytes::Bytes;
use session::*;

use shared::{hexdump, packet::Packet, proto};

use std::str::FromStr;

use async_std::sync::Arc;
use async_std::task;
use futures::executor;
use futures::future;
use futures::lock::Mutex;
use shared::proto::Message;

#[derive(Debug)]
pub enum SessionMessage {
    Send(proto::Message),
    Recv(Packet),
    Stop,
}

#[derive(Debug)]
struct State {
    sessions: HashMap<SocketAddr, Session>,
}

impl State {
    fn new() -> Self {
        State {
            sessions: HashMap::new(),
        }
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
    future::join(
        read_socket(state.clone(), socket.clone()),
        tick_loop(state.clone()),
    )
    .await;
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
    let mut buffer = [0u8; 65507];
    loop {
        let (size, remote) = socket.recv_from(&mut buffer).await.unwrap();
        let dgram = Bytes::from(&buffer[..size]);
        trace!("RECV <bytes> from {}:\n{}", remote, hexdump(&dgram));
        let mut state = state.lock().await;
        trace!("acquired read lock on server state");
        let session = state
            .sessions
            .entry(remote)
            .or_insert_with(|| Session::new(remote, socket.clone()));
        match Packet::from_bytes(dgram) {
            Ok(packet) => {
                trace!("valid packet, forwarding");
                session.on_packet(packet).await;
                if session.disconnected() {
                    info!("client disconnected");
                    state.sessions.remove(&remote);
                }
            }
            Err(err) => {
                warn!("decode error: {}", err);
            }
        }
    }
}
