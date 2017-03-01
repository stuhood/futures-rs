//! Definition of the JoinAll combinator, waiting for all of a list of futures
//! to finish.

use std::prelude::v1::*;

use std::fmt;
use std::mem;

use {Future, IntoFuture, Poll, Async};

#[derive(Debug)]
enum ElemState<T> where T: Future {
    Pending(T),
    Done(T::Item),
}

/// A future which takes a list of futures and resolves with a vector of the
/// completed values.
///
/// This future is created with the `join_all` method.
#[must_use = "futures do nothing unless polled"]
pub struct JoinAll<I>
    where I: IntoIterator,
          I::Item: IntoFuture,
{
    elems: Vec<ElemState<<I::Item as IntoFuture>::Future>>,
    /// Set to a non-None value to trigger debug during poll of this JoinAll.
    pub debug: Option<String>,
}

impl<I> JoinAll<I>
    where I: IntoIterator,
          I::Item: IntoFuture,
{
    fn if_debug(&self, msg: &str) {
        if let Some(id) = self.debug.as_ref() {
            println!("<join_all> {}: {}", id, msg);
        }
    }
}

impl<I> fmt::Debug for JoinAll<I>
    where I: IntoIterator,
          I::Item: IntoFuture,
          <<I as IntoIterator>::Item as IntoFuture>::Future: fmt::Debug,
          <<I as IntoIterator>::Item as IntoFuture>::Item: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("JoinAll")
            .field("elems", &self.elems)
            .finish()
    }
}

/// Creates a future which represents a collection of the results of the futures
/// given.
///
/// The returned future will drive execution for all of its underlying futures,
/// collecting the results into a destination `Vec<T>`. If any future returns
/// an error then all other futures will be canceled and an error will be
/// returned immediately. If all futures complete successfully, however, then
/// the returned future will succeed with a `Vec` of all the successful results.
///
/// # Examples
///
/// ```
/// use futures::future::*;
///
/// let f = join_all(vec![
///     ok::<u32, u32>(1),
///     ok::<u32, u32>(2),
///     ok::<u32, u32>(3),
/// ]);
/// let f = f.map(|x| {
///     assert_eq!(x, [1, 2, 3]);
/// });
///
/// let f = join_all(vec![
///     ok::<u32, u32>(1).boxed(),
///     err::<u32, u32>(2).boxed(),
///     ok::<u32, u32>(3).boxed(),
/// ]);
/// let f = f.then(|x| {
///     assert_eq!(x, Err(2));
///     x
/// });
/// ```
pub fn join_all<I>(i: I) -> JoinAll<I>
    where I: IntoIterator,
          I::Item: IntoFuture,
{
    let elems = i.into_iter().map(|f| {
        ElemState::Pending(f.into_future())
    }).collect();
    JoinAll {
        elems: elems,
        debug: None,
    }
}

impl<I> Future for JoinAll<I>
    where I: IntoIterator,
          I::Item: IntoFuture,
{
    type Item = Vec<<I::Item as IntoFuture>::Item>;
    type Error = <I::Item as IntoFuture>::Error;


    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.if_debug(&format!("polling {}", self.elems.len()));
        let mut all_done = true;

        for idx in 0 .. self.elems.len() {
            self.if_debug(&format!("  inspecting {:?}", idx));
            let done_val = match &mut self.elems[idx] {
                &mut ElemState::Pending(ref mut t) => {
                    match t.poll() {
                        Ok(Async::Ready(v)) => Ok(v),
                        Ok(Async::NotReady) => {
                            all_done = false;
                            continue
                        }
                        Err(e) => Err(e),
                    }
                }
                &mut ElemState::Done(ref mut _v) => continue,
            };

            self.if_debug(&format!("  elem {:?} was {:?}", idx, match &done_val { &Ok(_) => "ok", &Err(_) => "err"}));

            match done_val {
                Ok(v) => self.elems[idx] = ElemState::Done(v),
                Err(e) => {
                    // On completion drop all our associated resources
                    // ASAP.
                    self.if_debug(&format!("completing with \"err\": dropping {:?}", self.elems.len()));
                    self.elems = Vec::new();
                    return Err(e)
                }
            }
        }

        self.if_debug(&format!("is all_done? {:?}", all_done));

        if all_done {
            let elems = mem::replace(&mut self.elems, Vec::new());
            let result = elems.into_iter().map(|e| {
                match e {
                    ElemState::Done(t) => t,
                    _ => unreachable!(),
                }
            }).collect();
            Ok(Async::Ready(result))
        } else {
            Ok(Async::NotReady)
        }
    }
}
