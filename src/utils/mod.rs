// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cache;
pub mod cid;
pub mod db;
pub mod encoding;
pub mod flume;
pub mod get_size;
pub mod io;
pub mod misc;
pub mod multihash;
pub mod net;
pub mod p2p;
pub mod proofs_api;
pub mod rand;
pub mod reqwest_resume;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod stats;
pub mod stream;
pub mod version;

use anyhow::{Context as _, bail};
use futures::Future;
use multiaddr::{Multiaddr, Protocol};
use std::{str::FromStr, time::Duration};
use tokio::time::sleep;
use tracing::error;
use url::Url;

/// `"hunter2:/ip4/127.0.0.1/wss" -> "wss://:hunter2@127.0.0.1/"`
#[derive(Clone, Debug)]
pub struct UrlFromMultiAddr(pub Url);

impl FromStr for UrlFromMultiAddr {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (p, s) = match s.split_once(':') {
            Some((first, rest)) => (Some(first), rest),
            None => (None, s),
        };
        let m = Multiaddr::from_str(s).context("invalid multiaddr")?;
        let mut u = multiaddr2url(&m).context("unsupported multiaddr")?;
        if u.set_password(p).is_err() {
            bail!("unsupported password")
        }
        Ok(Self(u))
    }
}

/// `"/dns/example.com/tcp/8080/http" -> "http://example.com:8080/"`
///
/// Returns [`None`] on unsupported formats, or if there is a URL parsing error.
///
/// Note that [`Multiaddr`]s do NOT support a (URL) `path`, so that must be handled
/// out-of-band.
fn multiaddr2url(m: &Multiaddr) -> Option<Url> {
    let mut components = m.iter().peekable();
    let host = match components.next()? {
        Protocol::Dns(it) | Protocol::Dns4(it) | Protocol::Dns6(it) | Protocol::Dnsaddr(it) => {
            it.to_string()
        }
        Protocol::Ip4(it) => it.to_string(),
        Protocol::Ip6(it) => it.to_string(),
        _ => return None,
    };
    let port = components
        .next_if(|it| matches!(it, Protocol::Tcp(_)))
        .map(|it| match it {
            Protocol::Tcp(port) => port,
            _ => unreachable!(),
        });
    // ENHANCEMENT: could recognise `Tcp/443/Tls` as `https`
    let scheme = match components.next()? {
        Protocol::Http => "http",
        Protocol::Https => "https",
        Protocol::Ws(it) if it == "/" => "ws",
        Protocol::Wss(it) if it == "/" => "wss",
        _ => return None,
    };
    let None = components.next() else { return None };
    let parse_me = match port {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    };
    parse_me.parse().ok()
}

#[test]
fn test_url_from_multiaddr() {
    #[track_caller]
    fn do_test(input: &str, expected: &str) {
        let UrlFromMultiAddr(url) = input.parse().unwrap();
        assert_eq!(url.as_str(), expected, "input: {input}");
    }
    do_test("/dns/example.com/http", "http://example.com/");
    do_test("/dns/example.com/tcp/8080/http", "http://example.com:8080/");
    do_test("/dns/example.com/tcp/8081/ws", "ws://example.com:8081/");
    do_test("/ip4/127.0.0.1/wss", "wss://127.0.0.1/");

    // with password
    do_test(
        "hunter2:/dns/example.com/http",
        "http://:hunter2@example.com/",
    );
    do_test(
        "hunter2:/dns/example.com/tcp/8080/http",
        "http://:hunter2@example.com:8080/",
    );
    do_test("hunter2:/ip4/127.0.0.1/wss", "wss://:hunter2@127.0.0.1/");
}

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
    let max_retries = args.max_retries.unwrap_or(usize::MAX);
    let task = async {
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
    };

    if let Some(timeout) = args.timeout {
        tokio::time::timeout(timeout, task)
            .await
            .map_err(|_| RetryError::TimeoutExceeded)?
    } else {
        task.await
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

#[allow(dead_code)]
#[cfg(test)]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

#[allow(dead_code)]
#[cfg(test)]
pub fn is_ci() -> bool {
    // https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/store-information-in-variables#default-environment-variables
    misc::env::is_env_truthy("CI")
}

#[cfg(test)]
mod tests {
    mod files;

    use RetryError::{RetriesExceeded, TimeoutExceeded};
    use futures::future::pending;
    use std::{future::ready, sync::atomic::AtomicUsize};

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
