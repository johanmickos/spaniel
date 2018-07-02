use futures::task::Task;
use std::collections::HashMap;
use stream::StreamId;
use stream::StreamState;
use protocol::frames::FramingError;
use protocol::frames::Frame;
use protocol::frames;
use tokio_io::AsyncRead;
use tokio_io::AsyncWrite;
use std::sync::Mutex;
use std::sync::Arc;
use protocol::codec::writer::FrameWriter;
use protocol::codec::reader::FrameReader;
use futures::Poll;
use futures::Async;
use futures::task;
use futures::Future;
use stream::IncomingStreams;
use futures::sync::mpsc::Sender;
use futures::sync::mpsc;
use futures::sync::mpsc::Receiver;
use std::collections::VecDeque;


type ConnectionId = u32;

#[derive(Debug)]
pub enum ConnectionError {
    InvalidStreamId,
    UnknownFrame,
    General,
    InsufficientCredit, // TODO this should really be in its own category, maybe in some nested ConnError
}

impl From<()> for ConnectionError {
    fn from(_: ()) -> Self {
        ConnectionError::General
    }
}

impl From<FramingError> for ConnectionError {
    fn from(err: FramingError) -> Self {
        ConnectionError::General
    }
}

/// Tracks connection-related state needed for driving I/O progress
pub struct ConnectionContext {
    /// ID of this connection
    id: ConnectionId,
    /// Stores the current connection error, if there is one
    err: Option<ConnectionError>,
    /// Stream management store
    stream_states: HashMap<StreamId, StreamState>,
    /// Channel for submitting frames for writing over the network
    outbound: Sender<Frame>,
    outbound_listener: Receiver<Frame>,
    new_streams: VecDeque<frames::StreamRequest>,

    /// Task which drives the connection's I/O progress
    pub(crate) conn_task: Option<Task>,
    /// Task which awaits new streams
    pub(crate) new_stream_task: Option<Task>,
}

/// Frame-handling helper
enum AsyncHandle<T> {
    Ready,
    NotReady(T),
}

// impl ConnectionContext
impl ConnectionContext {
    pub fn new(id: ConnectionId) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        ConnectionContext {
            id,
            err: None,
            conn_task: None,
            new_stream_task: None,
            stream_states: HashMap::new(),
            outbound: tx,
            outbound_listener: rx,
            new_streams: VecDeque::new(),
        }
    }

    pub fn get_stream_state_mut(&mut self, stream_id: &StreamId) -> Option<&mut StreamState> {
        self.stream_states.get_mut(stream_id)
    }

    /// Delegates work according to frame type
    fn handle_frame(&mut self, f: Frame) -> Result<AsyncHandle<Frame>, ConnectionError> {
        match f {
            Frame::StreamRequest(frame) => self.on_stream_request(frame),
            Frame::CreditUpdate(frame) => self.on_credit_update(frame),
            Frame::Data(frame) => self.on_data(frame),
            Frame::Ping(_, _) => Ok(AsyncHandle::Ready),
            Frame::Pong(_, _) => Ok(AsyncHandle::Ready),
            Frame::Unknown => Err(ConnectionError::UnknownFrame),
        }
    }

    fn on_stream_request(&mut self, request: frames::StreamRequest) -> Result<AsyncHandle<Frame>, ConnectionError> {
        // TODO
        Ok(AsyncHandle::Ready)
    }

    fn on_credit_update(&mut self, request: frames::CreditUpdate) -> Result<AsyncHandle<Frame>, ConnectionError> {
        // TODO
        Ok(AsyncHandle::Ready)
    }

    fn on_data(&mut self, request: frames::Data) -> Result<AsyncHandle<Frame>, ConnectionError> {
        // TODO
        Ok(AsyncHandle::Ready)
    }


    /// Returns true if the connection has an error
    pub fn has_err(&self) -> bool {
        self.err.is_some()
    }

    /// Stores an error for this connection
    fn set_err(&mut self, err: ConnectionError) {
        self.err = Some(err);
    }

    /// Notifies all connection-related `Task`s
    fn notify_all(&mut self) {
        self.notify_conn_task();
        self.notify_new_stream_task();
    }

    // Notifies connection-driving task to wake up
    fn notify_conn_task(&mut self) {
        if let Some(task) = self.conn_task.take() {
            task.notify()
        }
    }
    // Notifies stream-listening task to wake up
    fn notify_new_stream_task(&mut self) {
        if let Some(task) = self.new_stream_task.take() {
            task.notify()
        }
    }

    pub fn next_stream(&mut self) -> Option<frames::StreamRequest> {
        self.new_streams.pop_front()
    }

    /// Returns the number of credits available for the stream, or `Async::NotReady` if there are none.
     ///
     /// Upon returning `Async::NotReady` the current task is stored and will be woken up once
     /// additional credits are assigned in `on_credit_update`.
    pub fn poll_stream_capacity(&mut self, stream_id: StreamId) -> Poll<u32, ConnectionError> {
        if self.has_err() {
            return Err(ConnectionError::General);
        }
        try_ready!(self.poll_conn_capacity().map_err(|_| ConnectionError::InsufficientCredit));
        let stream_state = match self.stream_states.get_mut(&stream_id) {
            None => {
                return Err(ConnectionError::InvalidStreamId);
            }
            Some(state) => state,
        };
        let remaining = stream_state.credits.available();
        if remaining == 0 {
            stream_state.send_task = Some(task::current());
            return Ok(Async::NotReady);
        }
        Ok(Async::Ready(remaining))
    }


    pub fn poll_conn_capacity(&mut self) -> Poll<(), ()> {
        self.outbound.poll_ready().map_err(|_| ())
    }

    pub fn send_frame(&mut self, frame: Frame) -> Result<(), ConnectionError> {
        if let Frame::Data(ref data) = frame {
            // TODO refactor around this flag
            // Flow control checks
            let stream_state = match self.stream_states.get_mut(&data.stream_id) {
                None => {
                    return Err(ConnectionError::InvalidStreamId);
                }
                Some(state) => state,
            };

            let size = data.payload_ref().len() as u32;
            if !stream_state.credits.has_capacity(size) {
                return Err(ConnectionError::InsufficientCredit);
            }
            stream_state.credits.use_credit(size);
        }
        // TODO handle res error
        let res = self.outbound.try_send(frame);
        self.notify_conn_task();
        Ok(())
    }

    pub fn poll_complete<T: AsyncWrite>(&mut self, tx: &mut FrameWriter<T>) -> Poll<(), ()> {
        use futures::Stream;

        try_ready!(tx.poll_buffer_ready().map_err(|_| ()));

        while let Some(frame) = try_ready!(self.outbound_listener.poll()) {
            let res = try_ready!(tx.buffer_and_flush(frame).map_err(|_| ()));
            try_ready!(tx.poll_buffer_ready().map_err(|_| ()));
        }
        Ok(Async::Ready(()))
    }

}


pub type SharedConnectionContext = Arc<Mutex<ConnectionContext>>;
pub type SharedFrameWriter<O: AsyncWrite> = Arc<Mutex<FrameWriter<O>>>;

struct IoHandle<I: AsyncRead, O: AsyncWrite> {
    rx: FrameReader<I>,
    tx: SharedFrameWriter<O>,
}

impl<I: AsyncRead, O: AsyncWrite> IoHandle<I, O> {
    pub fn new(rx: I, tx: O) -> Self {
        IoHandle {
            rx: FrameReader::new(rx),
            tx: Arc::new(Mutex::new(FrameWriter::new(tx))),
        }
    }

    pub fn clone_writer(&mut self) -> SharedFrameWriter<O> {
        self.tx.clone()
    }
}

pub struct ConnectionDriver<I: AsyncRead, O: AsyncWrite> {
    handle: IoHandle<I, O>,
    ctx: SharedConnectionContext,
    head_of_line: Option<Frame>,
}



impl<I: AsyncRead, O: AsyncWrite> ConnectionDriver<I, O> {
    pub fn with_io(reader: I, writer: O, id: u32) -> Self {
        let mut ctx = ConnectionContext::new(id);
        let ctx = Arc::new(Mutex::new(ctx));
        let handle = IoHandle::new(reader, writer);

        ConnectionDriver {
            head_of_line: None,
            handle,
            ctx,
        }
    }

    /// Returns a Stream which resolves to new stream IDs
    pub fn incoming_streams(&mut self) -> IncomingStreams {
        IncomingStreams::new(self.clone_ctx())
    }

    pub fn clone_ctx(&mut self) -> SharedConnectionContext {
        self.ctx.clone()
    }

    pub fn clone_writer(&mut self) -> SharedFrameWriter<O> {
        self.handle.clone_writer()
    }

    pub fn poll_read_progress(&mut self) -> Poll<(), ConnectionError> {
        use protocol::frames::Frame::*;
        use std::borrow::BorrowMut;

        let rx = self.handle.rx.borrow_mut();

        loop {
            // Continue looping until error, connection is closed, or there is nothing more to read
            let cur = match self.head_of_line.take() {
                None => {
                    try_ready!(rx.poll_frame())
                }
                Some(head) => Some(head),
            };
            match cur {
                None => return Ok(Async::Ready(())),
                Some(frame) => {
                    let mut ctx = self.ctx.lock().unwrap();
                    match ctx.handle_frame(frame) {
                        Ok(AsyncHandle::Ready) => (),
                        Ok(AsyncHandle::NotReady(f)) => {
                            self.head_of_line = Some(f);
                            return Ok(Async::NotReady);
                        }
                        Err(why) => {
                            println!("handle_frame err: {:?}", why);
                        }
                    }
                }
            }
        }
    }

    // TODO errors
    pub fn poll_write_progress(&mut self) -> Poll<(), ()> {
        let mut ctx = self.ctx.lock().unwrap();
        let ctx = &mut *ctx;

        let mut tx = self.handle.tx.lock().unwrap();
        let tx = &mut *tx;

        ctx.poll_complete(tx)
    }
}

impl<I: AsyncRead, O: AsyncWrite> Future for ConnectionDriver<I, O> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.poll_read_progress() {
                Ok(Async::Ready(())) => {
                    return Ok(Async::Ready(()));
                }
                Ok(Async::NotReady) => {
                    try_ready!(self.poll_write_progress());

                    // Store this task as the one responsible for making connection progress
                    match self.ctx.lock() {
                        Ok(mut ctx) => {
                            ctx.conn_task = Some(task::current());
                        }
                        Err(_) => {
                            // Should this ever be possible?
                            let mut ctx = self.ctx.lock().unwrap();
                            println!("Mutex poisoned");
                            ctx.set_err(ConnectionError::General);
                            return Err(());
                        }
                    }
                    return Ok(Async::NotReady);
                }
                Err(err) => {
                    let mut ctx = self.ctx.lock().unwrap();
                    println!("Closing conn with err: {:?}", err);
                    ctx.set_err(err);
                    return Err(());
                }
            }
        }
    }
}