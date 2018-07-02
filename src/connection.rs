use futures::task::Task;
use std::collections::HashMap;
use stream::StreamId;
use stream::StreamState;
use protocol::frames::FramingError;
use protocol::frames::Frame;
use protocol::frames;

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
    /// Task which drives the connection's I/O progress
    conn_task: Option<Task>,
    /// Task which awaits new streams
    new_stream_task: Option<Task>,
    /// Stream management store
    stream_states: HashMap<StreamId, StreamState>,
}

/// Frame-handling helper
enum AsyncHandle<T> {
    Ready,
    NotReady(T),
}

// impl ConnectionContext
impl ConnectionContext {
    pub fn new(id: ConnectionId) -> Self {
        ConnectionContext {
            id,
            err: None,
            conn_task: None,
            new_stream_task: None,
            stream_states: HashMap::new(),
        }
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
}
