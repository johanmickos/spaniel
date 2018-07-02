//! Frames are the core of the message transport layer, allowing applications to build
//! custom protocols atop this library.

use std;
use stream::StreamId;
use bytes::Bytes;
use bytes::BufMut;
use bytes::Buf;

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
    pub stream_id: StreamId,
    pub credit_capacity: u32,
}

#[derive(Debug)]
pub struct CreditUpdate {
    pub stream_id: StreamId,
    pub credit: u32,
}

#[derive(Debug)]
pub struct Data<B = Bytes> {
    pub stream_id: StreamId,
    pub seq_num: u32,
    pub payload: B,
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

impl Data {
    pub fn new(stream_id: StreamId, seq_num: u32, payload: Bytes) -> Self {
        Data {
            stream_id,
            seq_num,
            // TODO Could piggy-back "backlog" of buffers like Flink to proactively request more consumer buffers
            payload,
        }
    }

    pub fn encoded_len(&self) -> usize {
        4 + 4 + 4 + Bytes::len(&self.payload)
    }

    pub fn payload_ref(&self) -> &Bytes {
        &self.payload
    }

    pub fn encode_into<B: BufMut>(&self, dst: &mut B) {
        // NOTE: This method _COPIES_ the owned bytes into `dst` rather than extending with the owned bytes
        let payload_len = Bytes::len(&self.payload);
        assert!(dst.remaining_mut() >= (self.encoded_len()));
        dst.put_u32_be(self.stream_id.into());
        dst.put_u32_be(self.seq_num);
        dst.put_u32_be(payload_len as u32);
        dst.put_slice(&self.payload);
    }

    pub fn decode_from<B: Buf>(src: &mut B) -> Result<Self, FramingError> {
        if src.remaining() < 12 {
            return Err(FramingError::InvalidFrame);
        }
        let stream_id = src.get_u32_be().into();
        let seq_num = src.get_u32_be();
        let len = src.get_u32_be();
        let payload = src.collect();
        let data_frame = Data {
            stream_id,
            seq_num,
            payload,
        };
        Ok(data_frame)
    }
}
