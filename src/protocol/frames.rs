//! Frames are the core of the message transport layer, allowing applications to build
//! custom protocols atop this library.

use std;
use stream::StreamId;

pub const MAGIC_NUM: u32 = 0xC0A1BA11;

#[derive(Debug)]
pub enum FramingError {
    BufferCapacity,
    UnsupportedFrameType,
    InvalidMagicNum,
    InvalidFrame,
    Io(std::io::Error),
}

impl From<std::io::Error> for FramingError {
    fn from(src: std::io::Error) -> Self {
        FramingError::Io(src)
    }
}

/// Core network frame definition
#[derive(Debug)]
pub enum Frame {
    StreamRequest(StreamRequest),
    CreditUpdate(CreditUpdate),
    Data(Data),
    Ping(u32, StreamId),
    Pong(u32, StreamId),

    /// Catch-all for unknown frame types
    Unknown,
}

#[derive(Debug)]
pub struct StreamRequest {

}

#[derive(Debug)]
pub struct CreditUpdate {

}

#[derive(Debug)]
pub struct Data {

}

/// Byte-mappings for frame types
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub enum FrameType {
    StreamRequest   = 0x01,
    Data            = 0x02,
    CreditUpdate    = 0x03,
    Ping            = 0x04,
    Pong            = 0x05,
    Unknown,        // Not needed
}

impl From<u8> for FrameType {
    fn from(byte: u8) -> Self {
        match byte {
            0x01 => FrameType::StreamRequest,
            0x02 => FrameType::Data,
            0x03 => FrameType::CreditUpdate,
            0x04 => FrameType::Ping,
            0x05 => FrameType::Pong,
            _ => FrameType::Unknown,
        }
    }
}