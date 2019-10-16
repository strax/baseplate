#![feature(trace_macros)]

pub mod packet;
pub mod proto;
pub mod handshake;
pub mod logging;
pub mod future;

use packet::*;
use bytes::Bytes;
use pretty_hex::PrettyHex;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[macro_export]
macro_rules! hexdump {
    ($expr: expr) => {
        dbg!($expr.as_ref().hex_dump());
    }
}

#[inline]
pub fn hexdump(bytes: &Bytes) -> String {
    format!("{:?}", bytes.as_ref().hex_dump())
}
