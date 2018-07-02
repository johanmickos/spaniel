use tokio_io::codec::length_delimited;
use tokio_io::AsyncRead;
use protocol::frames::Frame;
use protocol::frames::FramingError;
use bytes::BytesMut;
use futures::Poll;
use futures::Async;
use futures::Stream;

/// Reads and decodes frames from the underlying `Stream`
pub struct FrameReader<T> {
    src: length_delimited::FramedRead<T>,
}

// impl FrameRader
impl<T: AsyncRead> FrameReader<T> {
    /// Creates a new FrameReader backed by a length-delimited wire protocol
    pub fn new(src: T) -> Self {
        let src = length_delimited::Builder::new()
            .big_endian()
            .length_adjustment(-4)
            .length_field_offset(0)
            .length_field_length(4)
            .new_read(src);
        FrameReader {
            src
        }
    }

    /// Decodes a `Frame` object from the provided `bytes`.
    ///
    /// This method assumes that the `bytes` represent a complete frame,
    /// successfully extracted by the `length_delimited` protocol.
    pub fn decode_frame(&self, bytes: BytesMut) -> Result<Frame, FramingError> {
        // TODO
        unimplemented!();
    }

    /// Attempts to extract bytes into a `Frame` from the underlying `AsyncRead`.
    pub fn poll_frame(&mut self) -> Poll<Option<Frame>, FramingError> {
        // Extract bytes from underlying source, delegating Async::NotReady responsibility to it
        let bytes_res = try_ready!(self.src.poll().map_err(|err| {
            FramingError::Io(err)
        }));

        match bytes_res {
            Some(bytes) => {
                let frame = self.decode_frame(bytes)?;
                Ok(Async::Ready(Some(frame)))
            }
            None => {
                // Underlying codec is closed; need to propagate this upwards
                Ok(Async::Ready(None))
            }
        }
    }
}


/// Continuous `Frame` stream wrapper around the `FrameReader`
impl<T: AsyncRead> Stream for FrameReader<T> {
    type Item = Frame;
    type Error = FramingError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.poll_frame()
    }
}
