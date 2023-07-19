// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cid;
pub mod db;
pub mod encoding;
pub mod io;
pub mod json;
pub mod misc;
pub mod monitoring;
pub mod net;
pub mod proofs_api;
pub mod version;

use std::{
    marker::PhantomData,
    ops::ControlFlow,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

use futures::{
    future::{pending, FusedFuture},
    select, Future, FutureExt,
};
use futures_util::{Stream, TryStream};
use pin_project_lite::pin_project;
use tokio::time::sleep;
use tracing::error;

/// Keep running the future created by `make_fut` until the timeout or retry
/// limit in `args` is reached.
/// `F` _must_ be cancel safe.
#[tracing::instrument(skip_all)]
pub async fn retry<F, T, E>(
    args: RetryArgs,
    mut make_fut: impl FnMut() -> F,
) -> Result<T, RetryError>
where
    F: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut timeout: Pin<Box<dyn FusedFuture<Output = ()>>> = match args.timeout {
        Some(duration) => Box::pin(sleep(duration).fuse()),
        None => Box::pin(pending()),
    };
    let max_retries = args.max_retries.unwrap_or(usize::MAX);
    let mut task = Box::pin(
        async {
            for _ in 0..max_retries {
                match make_fut().await {
                    Ok(ok) => return Ok(ok),
                    Err(err) => error!("retrying operation after {err:?}"),
                }
                if let Some(delay) = args.delay {
                    sleep(delay).await;
                }
            }
            Err(RetryError::RetriesExceeded)
        }
        .fuse(),
    );
    select! {
        _ = timeout => Err(RetryError::TimeoutExceeded),
        res = task => res,
    }
}

#[derive(Debug, Clone, Copy, smart_default::SmartDefault)]
pub struct RetryArgs {
    #[default(Some(Duration::from_secs(1)))]
    pub timeout: Option<Duration>,
    #[default(Some(5))]
    pub max_retries: Option<usize>,
    #[default(Some(Duration::from_millis(200)))]
    pub delay: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RetryError {
    #[error("operation timed out")]
    TimeoutExceeded,
    #[error("retry limit exceeded")]
    RetriesExceeded,
}

/// _Collation_ is a mixture between [`futures::StreamExt::fold`] and [`futures::StreamExt::chunks`].
/// It allows a user to fold into collections, like `fold`,
/// but without consuming the entire stream, like `chunks`.
///
/// `collate_fn` should accept a [`Collate`] and return:
/// - [`ControlFlow::Continue`] to add the next stream item to the current collation
/// - [`ControlFlow::Break`] to yield the current collation, and start a new one with the next stream item
///
/// If the underlying stream returns [`None`], `finish_fn` is called to handle a partial collation.
pub fn try_collate<Inner, Accumulator, CollateFn, FinishFn, Collection>(
    inner: Inner,
    collate_fn: CollateFn,
    finish_fn: FinishFn,
) -> TryCollate<Inner, Accumulator, CollateFn, FinishFn, Collection>
where
    Inner: TryStream,
    CollateFn: FnMut(Collate<Accumulator, Inner::Ok>) -> ControlFlow<Collection, Accumulator>,
    FinishFn: FnMut(Accumulator) -> Collection,
{
    fn assert_try_stream<T: TryStream>(t: T) -> T {
        t
    }

    assert_try_stream(TryCollate {
        inner,
        accumulator: None,
        collate_fn,
        finish_fn,
        collection: PhantomData,
    })
}

pin_project! {
    /// Stream for [`try_collate`], see that function for more.
    pub struct TryCollate<Inner, Accumulator, CollateFn, FinishFn, Collection> {
        #[pin]
        inner: Inner,
        accumulator: Option<Accumulator>,
        collate_fn: CollateFn,
        finish_fn: FinishFn,
        collection: PhantomData<Collection>
    }
}

pub enum Collate<Accumulator, Item> {
    /// Handle the first item since the last collation
    Started(Item),
    /// Fold into the existing collator
    Continued(Accumulator, Item),
}

impl<Inner, Accumulator, CollateFn, FinishFn, Collection> Stream
    for TryCollate<Inner, Accumulator, CollateFn, FinishFn, Collection>
where
    Inner: TryStream,
    CollateFn: FnMut(Collate<Accumulator, Inner::Ok>) -> ControlFlow<Collection, Accumulator>,
    FinishFn: FnMut(Accumulator) -> Collection,
{
    type Item = Result<Collection, Inner::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.as_mut().project();
        loop {
            match ready!(this.inner.as_mut().try_poll_next(cx)) {
                Some(Ok(ok)) => {
                    let action = match this.accumulator.take() {
                        Some(accumulator) => (this.collate_fn)(Collate::Continued(accumulator, ok)),
                        None => (this.collate_fn)(Collate::Started(ok)),
                    };
                    match action {
                        ControlFlow::Continue(accumulator) => *this.accumulator = Some(accumulator),
                        ControlFlow::Break(collated) => break Poll::Ready(Some(Ok(collated))),
                    }
                }
                Some(Err(error)) => break Poll::Ready(Some(Err(error))),
                None => match this.accumulator.take() {
                    Some(accumulator) => {
                        break Poll::Ready(Some(Ok((this.finish_fn)(accumulator))))
                    }
                    None => break Poll::Ready(None),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    mod files;

    use futures::stream::StreamExt as _;
    use std::{future::ready, sync::atomic::AtomicUsize};
    use RetryError::{RetriesExceeded, TimeoutExceeded};

    use super::*;

    impl RetryArgs {
        fn new_ms(
            timeout: impl Into<Option<u64>>,
            max_retries: impl Into<Option<usize>>,
            delay: impl Into<Option<u64>>,
        ) -> Self {
            Self {
                timeout: timeout.into().map(Duration::from_millis),
                max_retries: max_retries.into(),
                delay: delay.into().map(Duration::from_millis),
            }
        }
    }

    #[tokio::test]
    async fn timeout() {
        let res = retry(RetryArgs::new_ms(1, None, None), pending::<Result<(), ()>>).await;
        assert_eq!(Err(TimeoutExceeded), res);
    }

    #[tokio::test]
    async fn retries() {
        let res = retry(RetryArgs::new_ms(None, 1, None), || ready(Err::<(), _>(()))).await;
        assert_eq!(Err(RetriesExceeded), res);
    }

    #[tokio::test]
    async fn ok() {
        let res = retry(RetryArgs::default(), || ready(Ok::<_, ()>(()))).await;
        assert_eq!(Ok(()), res);
    }

    #[tokio::test]
    async fn needs_retry() {
        use std::sync::atomic::Ordering::SeqCst;
        let count = AtomicUsize::new(0);
        let res = retry(RetryArgs::new_ms(None, None, None), || async {
            match count.fetch_add(1, SeqCst) > 5 {
                true => Ok(()),
                false => Err(()),
            }
        })
        .await;
        assert_eq!(Ok(()), res);
        assert!(count.load(SeqCst) > 5);
    }

    #[tokio::test]
    async fn test_try_collate() {
        let source = futures::stream::iter(["the", "cuttlefish", "is", "not", "a", "fish"])
            .map(Ok)
            .chain(futures::stream::iter([Err(())]));

        let mut collated = try_collate(
            source,
            |request| {
                let buffer = match request {
                    Collate::Started(el) => String::from(el),
                    Collate::Continued(already, el) => already + el,
                };
                match buffer.len() >= 5 {
                    true => ControlFlow::Break(buffer),
                    false => ControlFlow::Continue(buffer),
                }
            },
            std::convert::identity,
        );

        assert_eq!(collated.next().await.unwrap().unwrap(), "thecuttlefish");
        assert_eq!(collated.next().await.unwrap().unwrap(), "isnot");
        assert_eq!(collated.next().await.unwrap().unwrap(), "afish");
        collated.next().await.unwrap().unwrap_err();
        assert!(collated.next().await.is_none());
    }
}
