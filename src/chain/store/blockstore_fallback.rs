//! Fallback blockstore for Forest
//!
//! This module provides a wrapper around an existing blockstore that adds a
//! fallback mechanism: if a requested block is missing locally and the
//! environment variable `FOREST_ENABLE_CHAINSTORE_FALLBACK` is set to `1`,
//! the block will be fetched from the network via bitswap and then
//! re-attempted from the local store.

use std::env;
use std::sync::Arc;

use cid::Cid;
use anyhow::Result;

/// A simple trait representing the ability to fetch blocks by CID from a
/// local store.  In Forest this corresponds to the `Blockstore` trait.
pub trait BlockGetter {
    type Block;
    fn get(&self, cid: &Cid) -> Result<Option<Self::Block>>;
}

/// Trait representing a minimal bitswap request manager.  Implementors
/// should fetch the given CID from the network and insert it into the
/// local blockstore on success.
#[async_trait::async_trait]
pub trait BitswapFetcher {
    async fn fetch_block(&self, cid: &Cid) -> Result<()>;
}

/// A wrapper that adds fallback behaviour to a blockstore.
///
/// When `FOREST_ENABLE_CHAINSTORE_FALLBACK=1` and a block is missing
/// locally, it will be fetched from the network via `bitswap` and then
/// reloaded from the underlying store.
pub struct FallbackBlockstore<S, B> {
    store: S,
    bitswap: Arc<B>,
}

impl<S, B> FallbackBlockstore<S, B>
where
    S: BlockGetter + Send + Sync,
    B: BitswapFetcher + Send + Sync,
{
    /// Create a new fallback wrapper around an existing blockstore and
    /// bitswap request manager.
    pub fn new(store: S, bitswap: Arc<B>) -> Self {
        Self { store, bitswap }
    }

    /// Retrieve a block from the local store, falling back to bitswap when
    /// enabled and necessary.  If the block is still missing after the
    /// network fetch, `None` is returned.
    pub async fn get_with_fallback(&self, cid: &Cid) -> Result<Option<S::Block>> {
        // Attempt to read from the underlying store first.
        if let Some(block) = self.store.get(cid)? {
            return Ok(Some(block));
        }

        // Check environment variable to decide if fallback is enabled.
        let enable = env::var("FOREST_ENABLE_CHAINSTORE_FALLBACK")
            .map(|v| v == "1")
            .unwrap_or(false);
        if !enable {
            return Ok(None);
        }

        // Fetch the block from the network via bitswap.
        // Errors from bitswap are propagated so callers can decide how to
        // handle network failures.
        self.bitswap.fetch_block(cid).await?;

        // Try again from the local store after network fetch.
        self.store.get(cid)
    }
}
