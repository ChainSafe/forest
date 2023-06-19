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

use std::{pin::Pin, time::Duration};

use futures::{
    future::{pending, FusedFuture},
    select, Future, FutureExt,
};
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

#[cfg(test)]
mod tests {
    mod files;

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
}
