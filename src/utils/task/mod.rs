// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use tokio::task::AbortHandle;

/// Holds a collection of [`AbortHandle`] and aborts them automatically on drop
#[derive(Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct AbortHandles(Vec<AbortHandle>);

impl Drop for AbortHandles {
    fn drop(&mut self) {
        for handle in self.iter() {
            handle.abort();
        }
    }
}
