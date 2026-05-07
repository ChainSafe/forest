// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

use futures::FutureExt as _;

/// Returns `true` if there are any active subscribers to the given broadcast channel.
pub fn has_subscribers<T>(tx: &tokio::sync::broadcast::Sender<T>) -> bool {
    tx.closed().now_or_never().is_none()
}
