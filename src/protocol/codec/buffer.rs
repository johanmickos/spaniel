//! Buffer responsible for collecting and emitting buffers before they're sent over the network

use bytes::IntoBuf;
use std::collections::VecDeque;

pub struct OutboundBuffer<B: IntoBuf> {
    buf: VecDeque<B>,
}

impl<B: IntoBuf> OutboundBuffer<B> {
    pub fn with_capacity(capacity: usize) -> Self {
        OutboundBuffer {
            buf: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push_back(&mut self, value: B) {
        self.buf.push_back(value);
    }

    pub fn push_front(&mut self, value: B) {
        self.buf.push_front(value);
    }

    pub fn add_data(&mut self, value: B) {
        self.push_back(value);
    }

    pub fn next(&mut self) -> Option<B> {
        self.buf.pop_back()
    }

    pub fn next_buf(&mut self) -> Option<B::Buf> {
        if let Some(bytes) = self.next() {
            return Some(bytes.into_buf());
        } else {
            return None;
        }
    }
}
