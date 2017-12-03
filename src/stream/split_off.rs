use core::fmt::{Debug, Formatter, Result as FmtResult};

use {Async, Poll, Future};
use stream::Stream;
use sync::oneshot;

/// A stream combinator which splits a stream into a prefix that satisfies a predicate, and a
/// suffix which starts with the first element that does not satisfy the predicate.
///
/// This structure is produced by the `Stream::split_off` method.
pub fn new<S, F>(s: S, f: F) -> (SplitOffPrefix<S, F>, SplitOffSuffix<S>)
    where S: Stream,
          F: FnMut(&S::Item) -> bool,
{
    let (tx, rx) = oneshot::channel();

    let prefix = SplitOffPrefix {
        inner: Some(PrefixInner {
            stream: s,
            suffix: tx,
        }),
        f: f,
    };
    let suffix = SplitOffSuffix {
        inner: SuffixInner::Waiting(rx)
    };
    (prefix, suffix)
}

/// This structure is produced by the `Stream::split_off` method.
#[must_use = "streams do nothing unless polled"]
pub struct SplitOffPrefix<S, F> where S: Stream {
    inner: Option<PrefixInner<S>>,
    f: F,
}

impl<S, F> Debug for SplitOffPrefix<S, F> where S: Stream + Debug, S::Item: Debug {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        // TODO
        fmt.debug_struct("SplitOffPrefix").finish()
    }
}

struct PrefixInner<S> where S: Stream {
    stream: S,
    suffix: oneshot::Sender<(S, Option<S::Item>)>,
}

impl<S, F> SplitOffPrefix<S, F> where S: Stream {
    ///
    /// Finalizes the Prefix by consuming its stream to send to the Suffix.
    ///
    fn handoff(&mut self, item: Option<S::Item>) -> Poll<Option<S::Item>, S::Error> {
        let inner = self.inner.take().unwrap();
        // If the tail has been dropped, drop the stream and item.
        let _ = inner.suffix.send((inner.stream, item));
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
        loop {
            match try_ready!(self.inner.as_mut().expect("poll called after eof").stream.poll()) {
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

/// This structure is produced by the `Stream::split_off` method.
#[must_use = "streams do nothing unless polled"]
pub struct SplitOffSuffix<S> where S: Stream {
    inner: SuffixInner<S>,
}

impl<S> Debug for SplitOffSuffix<S> where S: Stream + Debug, S::Item: Debug {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        // TODO
        fmt.debug_struct("SplitOffSuffix").finish()
    }
}

enum SuffixInner<S> where S: Stream {
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
            match &mut self.inner {
                &mut SuffixInner::Became(ref mut stream) => return stream.poll(),
                &mut SuffixInner::Waiting(ref mut prefix) => {
                    // NB: Can't use try_ready, because the err type for oneshot cancellation
                    // doesn't align.
                    match prefix.poll() {
                        Ok(Async::Ready(t)) => t,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(_) => {
                            // Will need a Drop impl for Prefix that sends the Stream so that we
                            // can assert that this doesn't happen.
                            unimplemented!("Dropping the prefix is not yet supported.")
                        },
                    }
                },
            };
        self.inner = SuffixInner::Became(stream);
        Ok(Async::Ready(item))
    }
}
