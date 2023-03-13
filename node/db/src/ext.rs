// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;
use chrono::Utc;
use human_repr::HumanCount;
use log::info;

use crate::*;

#[async_trait]
pub trait StoreExt: Store {
    async fn buffered_write(
        &self,
        rx: flume::Receiver<(Vec<u8>, Vec<u8>)>,
        buffer_capacity_bytes: usize,
    ) -> anyhow::Result<()> {
        let start = Utc::now();
        let mut total_bytes = 0;
        let mut total_entries = 0;
        let mut estimated_buffer_bytes = 0;
        let mut buffer = vec![];
        while let Ok((key, value)) = rx.recv_async().await {
            estimated_buffer_bytes += key.len() + value.len();
            total_bytes += key.len() + value.len();
            total_entries += 1;
            buffer.push((key, value));
            if estimated_buffer_bytes >= buffer_capacity_bytes {
                self.bulk_write(std::mem::take(&mut buffer))?;
                estimated_buffer_bytes = 0;
            }
        }
        self.bulk_write(buffer)?;
        info!(
            "Buffered write completed: total entries: {total_entries}, total size: {}, took: {}s",
            total_bytes.human_count_bytes(),
            (Utc::now() - start).num_seconds()
        );

        Ok(())
    }
}

impl<T: Store> StoreExt for T {}
