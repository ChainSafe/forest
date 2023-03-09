// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;

use crate::*;

#[async_trait]
pub trait StoreExt: Store {
    async fn buffered_write(
        &self,
        rx: flume::Receiver<(Vec<u8>, Vec<u8>)>,
        buffer_capacity_bytes: usize,
    ) -> anyhow::Result<()> {
        let mut estimated_size = 0;
        let mut buffer = vec![];
        while let Ok((key, value)) = rx.recv_async().await {
            estimated_size += key.len() + value.len();
            buffer.push((key, value));
            if estimated_size >= buffer_capacity_bytes {
                self.bulk_write(std::mem::take(&mut buffer))?;
                estimated_size = 0;
            }
        }
        Ok(self.bulk_write(buffer)?)
    }
}

impl<T: Store> StoreExt for T {}
