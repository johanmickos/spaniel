use std::collections::VecDeque;
use futures::sync::mpsc::Receiver;
use protocol::frames;
use futures::task::Task;
use flow_control::Credits;

#[derive(Debug, Clone, Copy, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct StreamId(pub u32);

impl StreamId {
    pub const ZERO: StreamId = StreamId(0);

    pub fn new(id: u32) -> Self {
        StreamId(id)
    }
}

impl From<StreamId> for u32 {
    fn from(it: StreamId) -> u32 {
        it.0
    }
}

impl From<StreamId> for usize {
    fn from(it: StreamId) -> usize {
        it.0 as usize
    }
}

impl From<u32> for StreamId {
    fn from(it: u32) -> Self {
        StreamId(it)
    }
}

/// Data structure tracking an individual stream
pub struct StreamState {
    pub credits: Credits,
    pub data_buffer: VecDeque<frames::Data>,
    pub data: Receiver<frames::Frame>,
    // Task waiting to be able to send data
    pub send_task: Option<Task>,
    // Task waiting to receive data from `data_buffer`
    pub recv_task: Option<Task>,
}

impl StreamState {
    pub fn notify_data_rx(&mut self) {
        if let Some(task) = self.recv_task.take() {
            task.notify();
        }
    }
    pub fn notify_data_tx(&mut self) {
        if let Some(task) = self.send_task.take() {
            task.notify();
        }
    }
}
