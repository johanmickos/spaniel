use bytes::Bytes;
use bytes::BytesMut;
use futures::Async;
use futures::AsyncSink;
use futures::Poll;
use futures::Sink;
use futures::StartSend;
use futures::Stream;
use protocol::frames::Frame;
use protocol::frames::FrameHead;
use tokio_io::codec::length_delimited::{self, Framed};
use tokio_io::AsyncRead;
use tokio_io::AsyncWrite;

mod buffer;
pub mod reader;
pub mod writer;

pub struct FrameCodec<T>
where
    T: AsyncRead + AsyncWrite,
{
    inner: Framed<T, Bytes>,
}

impl<T> FrameCodec<T>
where
    T: AsyncRead + AsyncWrite,
{
    pub fn new(conn: T) -> Self {
        Self {
            inner: length_delimited::Builder::new()
                .big_endian()
                .length_adjustment(-4)
                .length_field_offset(0)
                .length_field_length(4)
                .max_frame_length(::std::u32::MAX as usize)
                .new_framed(conn),
        }
    }
}

impl<T> Stream for FrameCodec<T>
where
    T: AsyncRead + AsyncWrite,
{
    type Item = Frame;
    type Error = (); // TODO err

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let frame = match try_ready!(self.inner.poll().map_err(|_err| {
            // TODO err
            ()
        })) {
            Some(buf) => Some(Frame::decode_from(buf).expect("deserialization")),
            None => None,
        };
        Ok(Async::Ready(frame))
    }
}

impl<T> Sink for FrameCodec<T>
where
    T: AsyncRead + AsyncWrite,
{
    // TODO err
    type SinkItem = Option<Frame>;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        match item {
            None => {
                return Ok(AsyncSink::Ready);
            }
            Some(frame) => {
                // TODO buffer provider
                let size = FrameHead::encoded_len() + frame.encoded_len();
                let mut buf = BytesMut::with_capacity(size);
                frame.encode_into(&mut buf).expect("serialization");

                match self.inner.start_send(buf.freeze()) {
                    Ok(AsyncSink::NotReady(_)) => Ok(AsyncSink::NotReady(Some(frame))),
                    Ok(AsyncSink::Ready) => Ok(AsyncSink::Ready),
                    Err(_err) => Err(()),
                }
            }
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete().map_err(|_| ()) // TODO err
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        self.poll_complete()
    }
}
