extern crate bytes;
#[macro_use]
extern crate futures;
extern crate tokio_io;

pub mod bytes_ext {
    pub use bytes::*;
}

mod buffer;
pub mod connection;
pub(crate) mod flow_control;
mod protocol;
pub mod stream;

pub use connection::ConnectionDriver;

pub mod frames {
    pub use protocol::frames::Frame;
    pub use protocol::frames::{Data, FrameHead, StreamRequest};
}

// Export codec-specific details
pub mod codec {
    pub use protocol::codec::FrameCodec;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
