use std::collections::VecDeque;

use {Poll, Async};
use {StartSend, AsyncSink};
use sink::Sink;
use stream::Stream;

/// Sink for the `Sink::buffer` combinator, which buffers up to some fixed
/// number of values when the underlying sink is unable to accept them.
#[derive(Debug)]
#[must_use = "sinks do nothing unless polled"]
pub struct Buffer<S: Sink> {
    sink: S,
    buf: VecDeque<S::SinkItem>,

    // Track capacity separately from the `VecDeque`, which may be rounded up
    cap: usize,
}

pub fn new<S: Sink>(sink: S, amt: usize) -> Buffer<S> {
    Buffer {
        sink: sink,
        buf: VecDeque::with_capacity(amt),
        cap: amt,
    }
}

impl<S: Sink> Buffer<S> {
    /// Get a shared reference to the inner sink.
    pub fn get_ref(&self) -> &S {
        &self.sink
    }

    /// Get a mutable reference to the inner sink.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    fn try_empty_buffer(&mut self) -> Poll<(), S::SinkError> {
        while let Some(item) = self.buf.pop_front() {
            if let AsyncSink::NotReady(item) = try!(self.sink.start_send(item)) {
                self.buf.push_front(item);

                // ensure that we attempt to complete any pushes we've started
                try!(self.sink.poll_complete());

                return Ok(Async::NotReady);
            }
        }

        Ok(Async::Ready(()))
    }
}

// Forwarding impl of Stream from the underlying sink
impl<S> Stream for Buffer<S> where S: Sink + Stream {
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<S::Item>, S::Error> {
        self.sink.poll()
    }
}

impl<S: Sink> Sink for Buffer<S> {
    type SinkItem = S::SinkItem;
    type SinkError = S::SinkError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        try!(self.try_empty_buffer());
        if self.buf.len() > self.cap {
            return Ok(AsyncSink::NotReady(item));
        }
        self.buf.push_back(item);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.try_empty_buffer());
        debug_assert!(self.buf.is_empty());
        self.sink.poll_complete()
    }
}
