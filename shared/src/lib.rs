#![feature(trace_macros)]
#![recursion_limit="256"]

use bytes::{Bytes, Buf, Reader, Writer, BytesMut, BufMut, ByteOrder, LittleEndian as LE};
use std::error::Error;
use std::io::{Read, Cursor, Write};
use pretty_hex::*;
use std::trace_macros;
use snafu::{Snafu, ensure};
use crc::crc32;

#[derive(Debug, Snafu)]
pub enum PacketError {
    #[snafu(display("expected {} bytes for reading, got {}", expected, actual))]
    InvalidLength {
        expected: usize,
        actual: usize
    },
    #[snafu(display("invalid message checksum (expected {:#x}, got {:#x})", received, computed))]
    InvalidChecksum {
        received: u32,
        computed: u32
    }
}

macro_rules! size {
    ($type:ty) => {
        std::mem::size_of::<$type>()
    };
}

macro_rules! ensure_size {
    ($size:expr, $cur:expr) => {{
        let expected = $size;
        let actual = $cur.remaining();
        ensure!(actual >= expected, InvalidLength { expected, actual });
    }};
}

macro_rules! read {
    ($type:ty, $cur:expr) => {{
        ensure_size!(std::mem::size_of::<$type>(), $cur);
        paste::expr! {
            Ok($cur.[<get_ $type _be>]())
        }
    }};
}

fn take(count: usize, cursor: Cursor<Bytes>) -> Result<Bytes, PacketError> {
    ensure_size!(count, cursor);
    Ok(cursor.get_ref().slice(cursor.position() as usize, cursor.position() as usize + count))
}

macro_rules! take {
    ($count:expr, $cur:expr) => {{
        ensure_size!($count as usize, $cur);
    }}
}

#[derive(Debug, Eq, PartialEq)]
pub struct Packet {
    pub sequence_number: u32,
    timestamp: i64,
    pub message: Bytes
}

impl Packet {
    pub fn new(sequence_number: u32, message: Bytes) -> Packet {
        let checksum = crc32::checksum_ieee(&message);
        let timestamp = chrono::Local::now().timestamp();
        Packet { sequence_number, timestamp, message }
    }

    pub fn from_bytes(bytes: Bytes) -> Result<Packet, PacketError> {
        trace_macros!(true);
        let mut cur = Cursor::new(bytes);

        let sequence_number = read!(u32, cur)?;
        let received_checksum = read!(u32, cur)?;
        let message_length = read!(u32, cur)? as usize;
        let timestamp = read!(i64, cur)?;
        let message = take(message_length, cur)?;

        // Check that the received message matches the checksum transmitted in the header
        let computed_checksum = crc32::checksum_ieee(&message);
        ensure!(received_checksum == computed_checksum, InvalidChecksum { received: received_checksum, computed: computed_checksum });
        trace_macros!(false);
        Ok(Packet { sequence_number, timestamp, message })
    }

    pub fn to_bytes(&self) -> Result<Bytes, Box<dyn Error>> {
        let mut bytes = BytesMut::with_capacity(65507);
        dbg!(bytes.len());
        bytes.put_u32_be(self.sequence_number);
        bytes.put_u32_be(crc32::checksum_ieee(&self.message));
        bytes.put_u32_be(self.message.len() as u32);
        bytes.put_i64_be(self.timestamp);
        bytes.put(&self.message);
        Ok(bytes.freeze())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write() {
        let packet = Packet {
            message: Bytes::from_static(b"HELLO WORLD!"),
            sequence_number: 5,
            timestamp: 1038021983902183
        };
        hexdump!(packet.to_bytes().unwrap());
    }

    #[test]
    fn test_roundtrip_id() {
        let packet = Packet {
            message: Bytes::from_static(b"HELLO WORLD!"),
            sequence_number: 5,
            timestamp: 1038021983902183
        };
        let encoded = packet.to_bytes().unwrap();
        hexdump!(encoded);
        let decoded = Packet::from_bytes(encoded).unwrap();
        hexdump!(decoded.to_bytes().unwrap());
        assert_eq!(decoded, packet);
    }

    #[test]
    fn test_bad_input() {
        match Packet::from_bytes(Bytes::from_static(b"osaijioajdoiasjdoisajidjisoajdoiajiojdas")) {
            Ok(_) => panic!("should not happen"),
            Err(err) => { dbg!(err); }
        }
    }
}
