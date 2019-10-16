use bytes::{Bytes, Buf, Reader, Writer, BytesMut, BufMut, ByteOrder, LittleEndian as LE};
use std::error::Error;
use std::io::{Read, Cursor, Write};
use pretty_hex::*;
use std::trace_macros;
use snafu::{Snafu, ensure};
use crc::crc32;
use std::fmt;
use chrono::prelude::*;
use super::hexdump;

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
    },
    #[snafu(display("invalid timestamp"))]
    InvalidTimestamp
}

#[derive(Eq, PartialEq)]
pub struct Packet {
    pub sequence_number: u32,
    pub timestamp: DateTime<Utc>,
    pub message: Bytes
}

impl fmt::Debug for Packet {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Packet")
            .field("sequence_number", &self.sequence_number)
            .field("timestamp", &self.timestamp)
            .field("message", &format!("<{} bytes>", self.message.len()))
            .finish()
    }
}

impl Packet {
    pub fn new(sequence_number: u32, message: Bytes) -> Packet {
        let checksum = crc32::checksum_ieee(&message);
        let timestamp = Utc::now();
        Packet { sequence_number, timestamp, message }
    }

    pub fn from_bytes(bytes: Bytes) -> Result<Packet, PacketError> {
        trace_macros!(true);
        let mut cur = Cursor::new(bytes);

        let sequence_number = read!(u32, cur)?;
        let received_checksum = read!(u32, cur)?;
        let message_length = read!(u32, cur)? as usize;
        let timestamp = Utc.timestamp_nanos(read!(i64, cur)?);
        let message = take(message_length, cur)?;

        // Check that the received message matches the checksum transmitted in the header
        let computed_checksum = crc32::checksum_ieee(&message);
        ensure!(received_checksum == computed_checksum, InvalidChecksum { received: received_checksum, computed: computed_checksum });
        trace_macros!(false);
        Ok(Packet { sequence_number, timestamp, message })
    }

    pub fn to_bytes(&self) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let mut bytes = BytesMut::with_capacity(65507);
        bytes.put_u32_be(self.sequence_number);
        bytes.put_u32_be(crc32::checksum_ieee(&self.message));
        bytes.put_u32_be(self.message.len() as u32);
        bytes.put_i64_be(self.timestamp.timestamp_nanos());
        bytes.put(&self.message);
        Ok(bytes.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write() {
        let packet = Packet {
            message: Bytes::from_static(b"HELLO WORLD!"),
            sequence_number: 5,
            timestamp: Utc::now()
        };
        hexdump!(packet.to_bytes().unwrap());
    }

    #[test]
    fn test_roundtrip_id() {
        let packet = Packet {
            message: Bytes::from_static(b"HELLO WORLD!"),
            sequence_number: 5,
            timestamp: Utc::now()
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
