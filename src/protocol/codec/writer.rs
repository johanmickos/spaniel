//! Buffer-backed writer based on Netty's `ChannelOutboundBuffer` and writing/flushing semantics.

use std;
use std::fmt::Formatter;
use std::error::Error;
use protocol::codec::buffer::OutboundBuffer;
use futures::task::Task;
use tokio_io::AsyncWrite;
use futures::Poll;
use futures::task;
use futures::Async;
use protocol::frames::Frame;
use bytes::{Buf, Bytes, BytesMut, IntoBuf};
use protocol::frames;

const LOW_WATERMARK: usize = 32 * 1024;
const HIGH_WATERMARK: usize = 64 * 1024;
const INIT_BUF_CAPACITY: usize = 64 * 1024;

pub struct Watermarks {
    low: usize,
    high: usize,
}

// Helper for converting a tuple into a Watermarks
impl From<(usize, usize)> for Watermarks {
    fn from((low, high): (usize, usize)) -> Self {
        if high < low {
            Watermarks {
                low: high,
                high: low,
            }
        } else {
            Watermarks {
                low,
                high,
            }
        }
    }
}

#[derive(Debug, PartialOrd, PartialEq)]
pub enum WriteError {
    HighWatermark,
    NotReady(Bytes),
    WouldBlock,
    Io,
}


impl std::error::Error for WriteError {
    fn description(&self) -> &str {
        match *self {
            WriteError::HighWatermark => "Buffering would exceed high watermark",
            WriteError::NotReady(ref data) => "Writer is not ready",
            WriteError::WouldBlock => "Writer would block",
            WriteError::Io => "I/O error",
        }
    }
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "writer error: {}", self.description())
    }
}


impl From<WriteError> for std::io::Error {
    fn from(src: WriteError) -> Self {
        std::io::Error::new(std::io::ErrorKind::WouldBlock, src)
    }
}


#[derive(PartialOrd, PartialEq)]
pub enum WriteState {
    Error,
    Writable,
    HighWatermarkReached,
}

impl WriteState {
    /// Returns whether the state is currently blocked from writing.
    pub fn is_blocked(&self) -> bool {
        match *self {
            WriteState::HighWatermarkReached | WriteState::Error => true,
            WriteState::Writable => false,
        }
    }
}

pub struct Writer<T, B: IntoBuf = Bytes> {
    /// Destination for writing bytes
    dst: T,

    /// Watermark-based buffer for storing byte buffers before writing
    buffer: OutboundBuffer<B>,
    /// Holds the next buffer to be written to the destination
    current: Option<B::Buf>,

    /// Tracks the writer's current state
    write_state: WriteState,
    /// Indicates whether the writer needs to flush its buffered contents to the destination
    need_flush: bool,
    /// Counter of the number of bytes waiting to be written TODO
    pending_bytes: usize,
    /// Task which waits for watermark progress on this writer
    waiting_task: Option<Task>,
    /// Configured waterark levels for this writer
    watermarks: Watermarks,
}

impl<T: AsyncWrite> Writer<T> {
    pub fn new(dst: T) -> Self {
        Writer {
            dst,
            buffer: OutboundBuffer::with_capacity(INIT_BUF_CAPACITY),
            current: None,
            write_state: WriteState::Writable,
            need_flush: false,
            pending_bytes: 0,
            waiting_task: None,
            watermarks: (LOW_WATERMARK, HIGH_WATERMARK).into(),
        }
    }

    pub fn set_watermarks(&mut self, high: usize, low: usize) {
        self.watermarks = (high, low).into();
    }

    /// Attempts to buffer the data for transmission.
    ///
    /// Returns WriteError::NotReady if the buffer is in high water.
    /// Use `poll_buffer_ready()` to ensure the buffer can accept more data.
    pub fn buffer_data(&mut self, data: Bytes) -> Result<(usize), WriteError> {
        if self.write_state == WriteState::HighWatermarkReached {
            return Err(WriteError::NotReady(data));
        }

        self.pending_bytes += data.len();
        self.buffer.add_data(data);

        if self.watermarks.high < self.pending_bytes {
            self.write_state = WriteState::HighWatermarkReached;
        }

        let remaining = {
            if self.watermarks.high < self.pending_bytes {
                0
            } else {
                self.watermarks.high - self.pending_bytes
            }
        };
        Ok(remaining)
    }

    /// Returns `Async::Ready` when the outbound buffer can accept another entry.
    ///
    /// This function will attempt to flush the buffer if it is currently above the
    /// high watermark.
    ///
    /// # Errors
    /// An IO error is returned if the writer has encountered an error prior to this call.
    pub fn poll_buffer_ready(&mut self) -> Poll<(), std::io::Error> {
        if let WriteState::Error = self.write_state {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "writer error"));
        } else if self.write_state == WriteState::HighWatermarkReached {
            self.poll_flush()?;

            if self.write_state == WriteState::HighWatermarkReached {
                // TODO should task be passed into function?
                self.waiting_task = Some(task::current());
                return Ok(Async::NotReady);
            }
        }
        Ok(Async::Ready(()))
    }


    /// Returns whether the current buffer has bytes remaining to be written
    pub fn has_remaining(&self) -> bool {
        match self.current {
            None => false,
            Some(ref buf) => buf.has_remaining(),
        }
    }

    /// Grabs the current buffer to send in the outbound buffer and stores it in `self.current`
    pub fn advance(&mut self) {
        self.current = self.buffer.next_buf();
    }

    /// Writes outbound buffer's entries to `self.dst`, flushing `dst` after all entries have
    /// been written.
    pub fn poll_flush(&mut self) -> Poll<(), std::io::Error> {
        if self.current.is_none() {
            self.advance();
        }

        while self.has_remaining() {
            let (bytes_flushed, remaining) = {
                let mut buf = self.current.as_mut().unwrap();
                let remaining = buf.remaining();
                let bytes_flushed = try_ready!(AsyncWrite::write_buf(&mut self.dst, buf));
                self.pending_bytes -= bytes_flushed;
                (bytes_flushed, remaining)
            };

            if self.watermarks.low > self.pending_bytes && self.write_state.is_blocked() {
                self.write_state = WriteState::Writable;
                if let Some(task) = self.waiting_task.take() {
                    task.notify();
                }
            }

            if bytes_flushed == remaining {
                // Recycle buffer
                self.advance();
            }
        }

        try_ready!(self.dst.poll_flush());
        Ok(Async::Ready(()))
    }

    pub fn write_and_flush(&mut self, data: Bytes) -> Poll<(), std::io::Error> {
        self.buffer_data(data)?;
        self.poll_flush()
    }

    /// Returns whether the internal buffer can be written to
    pub fn is_writable(&self) -> bool {
        !self.write_state.is_blocked()
    }
}

/// Wraps `Writer` with a frame-friendly API
pub struct FrameWriter<T> {
    writer: Writer<T>,
}

impl<T: AsyncWrite> FrameWriter<T> {
    pub fn new(dst: T) -> Self {
        FrameWriter {
            writer: Writer::new(dst),
        }
    }

    pub fn is_writable(&self) -> bool {
        self.writer.is_writable()
    }

    pub fn poll_buffer_ready(&mut self) -> Poll<(), std::io::Error> {
        self.writer.poll_buffer_ready()
    }

    pub fn buffer_frame(&mut self, frame: Frame) -> Result<(usize), WriteError> {
        let size = frame.encoded_len() + frames::FRAME_HEAD_LEN as usize;
//        // TODO buffer provider
        let mut buf = BytesMut::with_capacity(size);
        frame.encode_into(&mut buf);
        let buf = buf.freeze();
        self.writer.buffer_data(buf)
    }

    pub fn buffer_and_flush(&mut self, frame: Frame) -> Poll<(usize), WriteError> {
        let remaining = self.buffer_frame(frame)?;
        match self.writer.poll_flush().map_err(|_| WriteError::Io)? {
            Async::Ready(_) => {
                Ok(Async::Ready(remaining))
            }
            Async::NotReady => {
                Ok(Async::NotReady)
            }
        }
    }
}
