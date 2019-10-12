#![feature(cfg_target_has_atomic)]

use std::net::{TcpStream, UdpSocket};
use std::error::Error;
use log::{trace, info, warn, error};
use fern;
use std::thread;
use crossbeam::channel as chan;
use std::time::Duration;
use shared::{Packet, hexdump};
use crossbeam::atomic::AtomicCell;
use std::sync::Arc;
use bytes::Bytes;

fn on_connection(socket: UdpSocket, rx: chan::Receiver<Vec<u8>>) {

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

    trace!("client starting");

    // First initialize a TCP connection to the server
    let mut tcp_stream = TcpStream::connect("127.0.0.1:12345")?;
    trace!("tcp stream created");
    let addr = tcp_stream.local_addr()?;
    trace!("local port is {}", addr);
    // Bind an UDP socket to the same port
    let mut udp_stream = UdpSocket::bind(addr)?;
    udp_stream.connect(tcp_stream.peer_addr()?)?;
    trace!("udp socket created");

    {
        let udp_stream = udp_stream.try_clone()?;
        thread::Builder::new().name("tick".to_string()).spawn(move || {
            let mut seqid = 0u32;
            loop {
                let packet = Packet::new(seqid, Bytes::from_static(b"BEGIN HANDSHAKE"));
                udp_stream.send(&packet.to_bytes().unwrap());
                thread::sleep(Duration::from_secs(1));
                seqid += 1;
            }
        });
    }


    loop {
        let mut buf = [0u8;65507];
        let read = udp_stream.recv(&mut buf)?;
        trace!("received {} bytes", read);
    }

    Ok(())
}
