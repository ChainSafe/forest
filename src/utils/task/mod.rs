// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use tokio::task::AbortHandle;

/// Holds a collection of [`AbortHandle`] and aborts them automatically on drop
#[derive(Default, derive_more::Deref, derive_more::DerefMut)]
pub struct AbortHandles(Vec<AbortHandle>);

impl Drop for AbortHandles {
    fn drop(&mut self) {
        for handle in self.0.iter() {
            handle.abort();
        }
    }
}
