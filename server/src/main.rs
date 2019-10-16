mod session;

use std::net::{UdpSocket, IpAddr, SocketAddr, TcpStream, TcpListener};
use std::error::Error;
use log::{info, warn, trace, LevelFilter};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use std::time::Duration;
use std::thread::{Thread, JoinHandle};
use crossbeam::channel as chan;
use std::io::{Read};
use bytes::Bytes;
use session::*;
use std::pin::Pin;
use std::borrow::BorrowMut;
use shared::{packet::Packet, hexdump, proto};
use rand::random;
use std::str::FromStr;

pub enum SessionMessage {
    Send(proto::Message),
    Recv(Packet),
    Stop
}

#[derive(Debug)]
struct State {
    sessions: HashMap<SocketAddr, chan::Sender<SessionMessage>>
}

impl State {
    fn new() -> Self {
        State { sessions: HashMap::new() }
    }
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    shared::logging::setup()?;
    trace!("starting server");

    let addr = SocketAddr::from_str("0.0.0.0:12345")?;
    let socket = UdpSocket::bind("0.0.0.0:12345").unwrap();
    info!("udp socket bound to {}", addr);
    let mut state = Arc::new(RwLock::new(State::new()));
    read_socket(state, socket);
    Ok(())
}

fn read_socket(state: Arc<RwLock<State>>, socket: UdpSocket) -> () {
    let mut buffer = [0u8;65507];
    loop {
        let (size, remote) = socket.recv_from(&mut buffer).unwrap();
        let dgram = Bytes::from(&buffer[..size]);
        trace!("RECV <bytes> from {}:\n{}", remote, hexdump(&dgram));
        let mut rstate = state.read().unwrap();
        trace!("acquired read lock on server state");
        let tx = match rstate.sessions.get(&remote) {
            None => {
                // create session
                let tx = Session::create(remote, socket.try_clone().unwrap());
                // need to drop the read lock before acquiring a write lock
                drop(rstate);
                let mut wstate = state.write().unwrap();
                trace!("acquired write lock on server state");
                wstate.sessions.insert(remote, tx.clone());
                tx
            },
            Some(tx) => tx.clone()
        };
        match Packet::from_bytes(dgram) {
            Ok(packet) => {
                trace!("valid packet, forwarding");
                tx.send(SessionMessage::Recv(packet));
            },
            Err(err) => {
                warn!("decode error: {}", err);
                tx.send(SessionMessage::Stop);
            }
        }
    }
}
