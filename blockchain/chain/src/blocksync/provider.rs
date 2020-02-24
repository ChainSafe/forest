// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use blocks::{TipSetKeys, Tipset};

#[async_trait]
pub trait BlockSyncProvider {
    async fn get_headers(&self, tsk: &TipSetKeys, count: u64) -> Result<Vec<Tipset>, String>;
}
