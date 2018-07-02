extern crate bytes;
#[macro_use]
extern crate futures;
extern crate tokio_io;
extern crate tokio_tcp;

mod buffer;
mod connection;
mod flow_control;
mod protocol;
mod stream;

pub use connection::ConnectionDriver;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
