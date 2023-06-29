// Copyright (c) 2023 Tokio Contributors

// Derived from <https://docs.rs/tokio-stream/latest/tokio_stream/trait.StreamExt.html#method.timeout>
// but modified so timeouts can repeat even if there are no new events on the
// wrapped stream. Licensed under the MIT license. Any additional changes are:
// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use core::pin::Pin;
use core::task::{Context, Poll};
use std::fmt;
use std::time::Duration;

use futures::stream::Fuse;
use futures::{ready, Future, Stream, StreamExt};
use pin_project_lite::pin_project;
use tokio::time::{Instant, Sleep};

pin_project! {
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct RepeatingTimeout<S> {
        #[pin]
        stream: Fuse<S>,
        #[pin]
        deadline: Sleep,
        duration: Duration,
    }
}

/// Error returned by `RepeatingTimeout`.
#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl<S: Stream> RepeatingTimeout<S> {
    pub(super) fn new(stream: S, duration: Duration) -> Self {
        let next = Instant::now() + duration;
        let deadline = tokio::time::sleep_until(next);

        RepeatingTimeout {
            stream: stream.fuse(),
            deadline,
            duration,
        }
    }
}

impl<S: Stream> Stream for RepeatingTimeout<S> {
    type Item = Result<S::Item, Elapsed>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.project();

        match me.stream.poll_next(cx) {
            Poll::Ready(v) => {
                if v.is_some() {
                    let next = Instant::now() + *me.duration;
                    me.deadline.reset(next);
                }
                return Poll::Ready(v.map(Ok));
            }
            Poll::Pending => {}
        };

        ready!(me.deadline.as_mut().poll(cx));

        let next = Instant::now() + *me.duration;
        me.deadline.reset(next);

        Poll::Ready(Some(Err(Elapsed::new())))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, _) = self.stream.size_hint();

        // The timeout stream may insert an infinite number of timeouts.

        (lower, None)
    }
}

// ===== impl Elapsed =====

impl Elapsed {
    pub(crate) fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl std::error::Error for Elapsed {}

impl From<Elapsed> for std::io::Error {
    fn from(_err: Elapsed) -> std::io::Error {
        std::io::ErrorKind::TimedOut.into()
    }
}
