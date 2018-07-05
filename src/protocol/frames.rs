//! Frames are the core of the message transport layer, allowing applications to build
//! custom protocols atop this library.

use std;
use stream::StreamId;
use bytes::Bytes;
use bytes::BufMut;
use bytes::Buf;
use bytes::IntoBuf;
use std::fmt::Debug;

pub const MAGIC_NUM: u32 = 0xC0A1BA11;
// (frame length) + (magic # length) + (frame type)
pub const FRAME_HEAD_LEN: u32 = 4 + 4 + 1;

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

impl Frame {
    pub fn frame_type(&self) -> FrameType {
        match *self {
            Frame::StreamRequest(_) => FrameType::StreamRequest,
            Frame::CreditUpdate(_)  => FrameType::CreditUpdate,
            Frame::Data(_)          => FrameType::Data,
            Frame::Ping( .. )       => FrameType::Ping,
            Frame::Pong( .. )       => FrameType::Pong,
            Frame::Unknown          => FrameType::Unknown,
        }
    }

    pub fn decode_from<B: IntoBuf + Debug>(buf: B) -> Result<Self, FramingError> {
        let mut buf = buf.into_buf();
        let head = FrameHead::decode_from(&mut buf)?;
        match head.frame_type {
            FrameType::StreamRequest    => StreamRequest::decode_from(&mut buf),
            FrameType::Data             => Data::decode_from(&mut buf),
            FrameType::CreditUpdate     => CreditUpdate::decode_from(&mut buf),
            _ => unimplemented!()
        }
    }

    pub fn encode_into<B: BufMut>(&self, dst: &mut B) -> Result<(), ()> {
        let head = FrameHead::new(self.frame_type());
        head.encode_into(dst, self.encoded_len() as u32);
        match *self {
            Frame::StreamRequest(ref frame) => frame.encode_into(dst),
            Frame::CreditUpdate(ref frame) => frame.encode_into(dst),
            Frame::Data(ref frame) => frame.encode_into(dst),
            _ => Err(()),
        }
    }

    pub fn encoded_len(&self) -> usize {
        match *self {
            Frame::StreamRequest(ref frame) => frame.encoded_len(),
            Frame::CreditUpdate(ref frame) => frame.encoded_len(),
            Frame::Data(ref frame) => frame.encoded_len(),
            _ => 0
        }
    }
}

pub trait FrameExt {
    fn decode_from<B: Buf>(src: &mut B) -> Result<Frame, FramingError>;
    fn encode_into<B: BufMut>(&self, dst: &mut B) -> Result<(), ()>;
    fn encoded_len(&self) -> usize;
}

// Head of each frame
#[derive(Debug)]
pub struct FrameHead {
    frame_type: FrameType,
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

impl FrameHead {
    pub fn new(frame_type: FrameType) -> Self {
        FrameHead {
            frame_type,
        }
    }

    // Encodes own fields and entire frame length into `dst`.
    // This conforms to the length_delimited decoder found in the framed writer
    pub fn encode_into<B: BufMut>(&self, dst: &mut B, content_len: u32) {
        assert!(dst.remaining_mut() >= FRAME_HEAD_LEN as usize);
        // Represents total length, including bytes for encoding length
        let len = FRAME_HEAD_LEN + content_len;
        dst.put_u32_be(len);
        dst.put_u32_be(MAGIC_NUM);
        dst.put_u8(self.frame_type as u8);
    }

    pub fn decode_from<B: Buf>(src: &mut B) -> Result<Self, FramingError> {
        // length_delimited's decoder will have parsed the length out of `src`, subtract that out
        if src.remaining() < (FRAME_HEAD_LEN - 4) as usize {
            return Err(FramingError::BufferCapacity);
        }

        let magic_check = src.get_u32_be();

        if magic_check != MAGIC_NUM {
            return Err(FramingError::InvalidMagicNum);
        }

        let frame_type: FrameType = src.get_u8().into();
        let head = FrameHead::new(frame_type);
        Ok(head)
    }

    pub fn frame_type(&self) -> FrameType {
        self.frame_type
    }
}

impl StreamRequest {
    pub fn new(stream_id: StreamId, credit_capacity: u32) -> Self {
        StreamRequest {
            stream_id,
            credit_capacity,
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

    pub fn with_raw_payload(stream_id: StreamId, seq_num: u32, raw_bytes: &[u8]) -> Self {
        Data::new(stream_id, seq_num, Bytes::from(raw_bytes))
    }

    pub fn encoded_len(&self) -> usize {
        4 + 4 + 4 + Bytes::len(&self.payload)
    }

    pub fn payload_ref(&self) -> &Bytes {
        &self.payload
    }

    /// Consumes this frame and returns the raw payload buffer
    pub fn payload(self) -> Bytes {
        self.payload
    }
}


impl FrameExt for StreamRequest {
    fn decode_from<B: Buf>(src: &mut B) -> Result<Frame, FramingError> {
        if src.remaining() < 8 {
            return Err(FramingError::InvalidFrame);
        }
        let stream_id: StreamId = src.get_u32_be().into();
        let credit = src.get_u32_be();
        let stream_req = StreamRequest {
            stream_id,
            credit_capacity: credit,
        };
        Ok(Frame::StreamRequest(stream_req))
    }

    fn encode_into<B: BufMut>(&self, dst: &mut B) -> Result<(), ()> {
        assert!(dst.remaining_mut() >= self.encoded_len());
        dst.put_u32_be(self.stream_id.into());
        dst.put_u32_be(self.credit_capacity);
        Ok(())
    }

    fn encoded_len(&self) -> usize {
        4 + 4 //stream_id + credit_capacity
    }
}

impl FrameExt for Data {
    fn decode_from<B: Buf>(src: &mut B) -> Result<Frame, FramingError> {
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
        Ok(Frame::Data(data_frame))
    }

    fn encode_into<B: BufMut>(&self, dst: &mut B) -> Result<(), ()> {
        // NOTE: This method _COPIES_ the owned bytes into `dst` rather than extending with the owned bytes
        let payload_len = Bytes::len(&self.payload);
        assert!(dst.remaining_mut() >= (self.encoded_len()));
        dst.put_u32_be(self.stream_id.into());
        dst.put_u32_be(self.seq_num);
        dst.put_u32_be(payload_len as u32);
        dst.put_slice(&self.payload);
        Ok(())
    }

    fn encoded_len(&self) -> usize {
        4 + 4 + 4 + Bytes::len(&self.payload)
    }
}


impl FrameExt for CreditUpdate {
    fn decode_from<B: Buf>(src: &mut B) -> Result<Frame, FramingError> {
        unimplemented!()
    }

    fn encode_into<B: BufMut>(&self, dst: &mut B) -> Result<(), ()> {
        unimplemented!()
    }

    fn encoded_len(&self) -> usize {
        4 + 4 // stream_id + credit
    }
}