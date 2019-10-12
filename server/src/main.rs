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
use fern;
use session::*;
use std::pin::Pin;
use std::borrow::BorrowMut;
use shared::{Packet, hexdump};

pub enum SessionMessage {
    Send(Bytes),
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

fn main() -> Result<(), Box<dyn Error>> {
    fern::Dispatch::new()
        .format(|out, msg, record| {
            out.finish(format_args!(
                "{}:{} | {} | {} | {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                thread::current().name().unwrap_or("unknown"),
                record.level(),
                msg
            ))
        }).chain(std::io::stdout()).apply()?;
    trace!("starting server");

    let socket = UdpSocket::bind("0.0.0.0:12345").unwrap();
    let mut state = Arc::new(RwLock::new(State::new()));
    let mut threads: Vec<JoinHandle<()>> = vec!();
    {
        let state = state.clone();
        let socket = socket.try_clone().unwrap();

        let t = thread::Builder::new().name("tcp-acceptor".to_string()).spawn(move || {
            start_tcp_server(state, socket);
        }).unwrap();
        threads.push(t);
    }
    {
        let state = state.clone();
        let socket = socket.try_clone().unwrap();
        let t = thread::Builder::new().name("udp-dispatch".to_string()).spawn(move || {
            start_udp_server(state, socket);
        }).unwrap();
        threads.push(t);
    }

    for t in threads {
        t.join();
    }

    Ok(())
}

fn start_tcp_server(state: Arc<RwLock<State>>, socket: UdpSocket) -> () {
    let mut acceptor = TcpListener::bind("0.0.0.0:12345").unwrap();
    info!("tcp acceptor bound");

    loop {
        let (mut stream, remote) = acceptor.accept().unwrap();
        info!("new connection from {}", remote);
        let state = state.clone();
        let socket = socket.try_clone().unwrap();
        thread::Builder::new().name(format!("tcp-stream/{}", remote)).spawn(move || {
            let tx = Session::create(remote, socket);
            trace!("created new session");
            let mut wstate = state.write().unwrap();
            trace!("acquired write lock on state");
            wstate.sessions.insert(remote, tx.clone());
            trace!("inserted tx to session table");
            drop(wstate);

            trace!("start now to read from socket");

            let mut buf = [0u8;1024];
            loop {
                let size = stream.read(&mut buf).unwrap();
                if size == 0 {
                    trace!("peer disconnected");
                    tx.send(SessionMessage::Stop).unwrap();
                    state.write().unwrap().sessions.remove(&remote);
                    break;
                } else {
                    // ignore
                }
            }
        });
    }
}

fn start_udp_server(state: Arc<RwLock<State>>, socket: UdpSocket) -> () {
    let mut buffer = [0u8;65507];
    loop {
        let (size, remote) = socket.recv_from(&mut buffer).unwrap();
        let dgram = Bytes::from(&buffer[..size]);
        trace!("RECV {} {}", remote, hexdump(&dgram));
        let mut state = state.read().unwrap();
        trace!("acquired read lock on server state");
        match state.sessions.get(&remote) {
            None => {
                warn!("unknown remote, ignoring datagram");
            },
            Some(tx) => {
                trace!("decoding packet");
                match Packet::from_bytes(dgram) {
                    Ok(packet) => {
                        trace!("decode ok, forward to tx");
                        tx.send(SessionMessage::Recv(packet));
                    },
                    Err(err) => {
                        warn!("decode error: {}", err);
                        // ignore packet
                    }
                }
            }
        }
    }
}
