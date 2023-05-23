// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{pin::Pin, time::Duration};

use futures::{
    future::{pending, FusedFuture},
    select, Future, FutureExt,
};
use tokio::time::sleep;

/// `F` _must_ be cancel safe
// TODO(aatifsyed): store most recent error in RetryError
// REFACTOR?(aatifsyed): use futures-retry instead
// ENHANCE?(aatifsyed): panic if would retry forever
pub async fn retry<F, T, E>(
    args: RetryArgs,
    mut make_fut: impl FnMut() -> F,
) -> Result<T, RetryError>
where
    F: Future<Output = Result<T, E>>,
{
    let mut timeout: Pin<Box<dyn FusedFuture<Output = ()>>> = match args.timeout {
        Some(duration) => Box::pin(sleep(duration).fuse()),
        None => Box::pin(pending()),
    };
    let max_retries = args.max_retries.unwrap_or(usize::MAX);
    let mut task = Box::pin(
        async {
            for i in 0..max_retries {
                println!("try {i}");
                if let Ok(ok) = make_fut().await {
                    println!("ok");
                    return Ok(ok);
                }
                println!("err");
                if let Some(delay) = args.delay {
                    sleep(delay).await;
                }
            }
            println!("retries exceeded");
            Err(RetryError::RetriesExceeded)
        }
        .fuse(),
    );
    select! {
        _ = timeout => {
            println!("timeout exceeded");
            Err(RetryError::TimeoutExceeded)
        },
        res = task => res,
    }
}

#[derive(Debug, Clone, Copy, smart_default::SmartDefault)]
pub struct RetryArgs {
    #[default(Some(Duration::from_secs(1)))]
    timeout: Option<Duration>,
    #[default(Some(5))]
    max_retries: Option<usize>,
    #[default(Some(Duration::from_millis(200)))]
    delay: Option<Duration>,
}

impl RetryArgs {
    pub fn new_ms(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryError {
    TimeoutExceeded,
    RetriesExceeded,
}

#[cfg(test)]
mod tests {
    use std::future::ready;

    use RetryError::{RetriesExceeded, TimeoutExceeded};

    use super::*;

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
}

/// Retries a function call until `max_retries` is exceeded with a delay
// TODO(aatifsyed): this should be a function
#[macro_export]
macro_rules! retry {
    ($func:ident, $max_retries:expr, $delay:expr $(, $arg:expr)*) => {{
        let mut retry_count = 0;
        loop {
            match $func($($arg),*).await {
                Ok(val) => break Ok(val),
                Err(err) => {
                    retry_count += 1;
                    if retry_count >= $max_retries {
                        info!("Maximum retries exceeded.");
                        break Err(err);
                    }
                    log::warn!("{err:?}");
                    info!("Retry attempt {} started with delay {:#?}.", retry_count, $delay);
                    sleep($delay).await;
                }
            }
        }
    }};
}
