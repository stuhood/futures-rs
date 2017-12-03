use {Async, Poll, Future};
use stream::Stream;
use sync::oneshot;

/// A stream combinator which splits a stream into a prefix that satisfies a predicate, and a
/// suffix which starts with the first element that does not satisfy the predicate.
///
/// This structure is produced by the `Stream::split_off` method.
pub fn new<S, F>(s: S, f: F) -> (SplitOffPrefix<S, F>, SplitOffSuffix<S>)
    where S: Stream,
          F: FnMut(S::Item) -> bool,
{
    let (tx, rx) = oneshot::channel();

    let prefix = SplitOffPrefix {
        stream: Some(s),
        f: f,
        suffix: tx,
    };
    let suffix = SplitOffSuffix {
        inner: Inner::Waiting(rx)
    };
    (prefix, suffix)
}

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct SplitOffPrefix<S, F> where S: Stream {
    stream: Option<S>,
    f: F,
    suffix: oneshot::Sender<(S, Option<S::Item>)>,
}

impl<S, F> SplitOffPrefix<S, F> where S: Stream {
    ///
    /// Finalizes the Prefix by consuming its stream to send to the Suffix.
    ///
    fn handoff(&mut self, item: Option<S::Item>) -> Poll<Option<S::Item>, S::Error> {
        let stream = self.stream.take().unwrap();
        // If the tail has been dropped, drop the stream and item.
        let _ = self.suffix.send((stream, item));
        return Ok(Async::Ready(None))
    }
}

impl<S, F> Stream for SplitOffPrefix<S, F>
    where S: Stream,
          F: FnMut(&S::Item) -> bool,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<S::Item>, S::Error> {
        let mut stream = self.stream.as_mut().expect("poll called after eof");
        loop {
            match try_ready!(stream.poll()) {
                Some(e) => {
                    if (self.f)(&e) {
                        return Ok(Async::Ready(Some(e)))
                    } else {
                        return self.handoff(Some(e))
                    }
                }
                None => {
                    return self.handoff(None)
                },
            }
        }
    }
}

pub struct SplitOffSuffix<S> where S: Stream {
    inner: Inner<S>,
}

enum Inner<S> where S: Stream {
    Waiting(oneshot::Receiver<(S, Option<S::Item>)>),
    Became(S),
}

impl<S> Stream for SplitOffSuffix<S>
    where S: Stream,
{
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<S::Item>, S::Error> {

        let (stream, item) =
            match &self.inner {
                &Inner::Became(ref stream) => return stream.poll(),
                &Inner::Waiting(ref prefix) => try_ready!(prefix.poll()),
            };
        self.inner = Inner::Became(stream);
        Ok(Async::Ready(item))
    }
}
