// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::chain::SAFE_HEIGHT_DISTANCE;

pub struct TipsetResolver<'a, DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    ctx: &'a Ctx<DB>,
    api_version: ApiPaths,
}

impl<'a, DB> TipsetResolver<'a, DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Creates a TipsetResolver that holds a reference to the given chain context and the API version to use for tipset resolution.
    ///
    /// # Examples
    ///
    /// ```
    /// let resolver = TipsetResolver::new(ctx, ApiPaths::V2);
    /// ```
    pub fn new(ctx: &'a Ctx<DB>, api_version: ApiPaths) -> Self {
        Self { ctx, api_version }
    }

    /// Resolve a tipset from a block identifier that may be a predefined tag, block height, or block hash.
    ///
    /// Attempts to resolve the provided `block_param` into a concrete `Tipset`. The parameter may be:
    /// - a predefined tag (e.g., `Predefined::Latest`, `Predefined::Safe`, `Predefined::Finalized`),
    /// - a block height (number or object form), or
    /// - a block hash (raw hash or object form that can require canonicalization).
    ///
    /// # Parameters
    ///
    /// - `block_param` — block identifier to resolve; accepts any type convertible to `BlockNumberOrHash`.
    /// - `resolve` — rule for how to treat null/unknown tipsets when resolving by height/hash.
    ///
    /// # Returns
    ///
    /// The resolved `Tipset` on success.
    ///
    /// # Examples
    ///
    /// ```
    /// # use some_crate::{TipsetResolver, Predefined, ResolveNullTipset};
    /// # async fn doc_example(resolver: &TipsetResolver<'_, _>) {
    /// let ts = resolver
    ///     .tipset_by_block_number_or_hash(Predefined::Safe, ResolveNullTipset::TakeOlder)
    ///     .await
    ///     .unwrap();
    /// // `ts` is a concrete Tipset matching the requested identifier.
    /// # }
    /// ```
    pub async fn tipset_by_block_number_or_hash(
        &self,
        block_param: impl Into<BlockNumberOrHash>,
        resolve: ResolveNullTipset,
    ) -> anyhow::Result<Tipset> {
        match block_param.into() {
            BlockNumberOrHash::PredefinedBlock(tag) => self.resolve_predefined_tipset(tag).await,
            BlockNumberOrHash::BlockNumber(block_number)
            | BlockNumberOrHash::BlockNumberObject(BlockNumber { block_number }) => {
                resolve_block_number_tipset(self.ctx.chain_store(), block_number, resolve)
            }
            BlockNumberOrHash::BlockHash(block_hash) => {
                resolve_block_hash_tipset(self.ctx.chain_store(), &block_hash, false, resolve)
            }
            BlockNumberOrHash::BlockHashObject(BlockHash {
                block_hash,
                require_canonical,
            }) => resolve_block_hash_tipset(
                self.ctx.chain_store(),
                &block_hash,
                require_canonical,
                resolve,
            ),
        }
    }

    /// Resolve a predefined tipset according to the resolver's API version.
    ///
    /// This dispatches to the API-version-specific resolution logic and returns the resolved tipset for the given predefined tag.
    ///
    /// # Returns
    ///
    /// The resolved `Tipset`, or an error if resolution fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use crate::rpc::TipsetResolver;
    /// # use crate::rpc::Predefined;
    /// # async fn example(resolver: &TipsetResolver<'_, impl Send + Sync>) -> anyhow::Result<()> {
    /// let ts = resolver.resolve_predefined_tipset(Predefined::Safe).await?;
    /// println!("resolved tipset: {:?}", ts);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn resolve_predefined_tipset(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        match self.api_version {
            ApiPaths::V2 => self.resolve_predefined_tipset_v2(tag).await,
            _ => self.resolve_predefined_tipset_v1(tag).await,
        }
    }

    /// Resolves a predefined tipset using the V1 resolution policy, or delegates to the V2 resolver when the
    /// V1 finality-resolution override is not enabled.
    ///
    /// If the environment variable `FOREST_ETH_V1_DISABLE_F3_FINALITY_RESOLUTION` is set to a truthy value,
    /// this function first attempts common predefined tag resolution (e.g., Pending, Latest). If that yields
    /// no result, the function returns the "safe" or "finalized" tipset for the corresponding `Predefined` tag.
    /// When the environment variable is not set or is falsy, resolution is delegated to the V2 resolver.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested predefined tag is unknown or if tipset resolution fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # async fn run(resolver: &TipsetResolver<'_, impl Blockstore>) -> anyhow::Result<()> {
    /// let ts = resolver.resolve_predefined_tipset_v1(Predefined::Safe).await?;
    /// # Ok::<(), anyhow::Error>(())
    /// # }
    /// ```
    async fn resolve_predefined_tipset_v1(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        const ETH_V1_DISABLE_F3_FINALITY_RESOLUTION_ENV_KEY: &str =
            "FOREST_ETH_V1_DISABLE_F3_FINALITY_RESOLUTION";
        static ETH_V1_F3_FINALITY_RESOLUTION_DISABLED: LazyLock<bool> =
            LazyLock::new(|| is_env_truthy(ETH_V1_DISABLE_F3_FINALITY_RESOLUTION_ENV_KEY));

        if *ETH_V1_F3_FINALITY_RESOLUTION_DISABLED {
            if let Some(ts) = self.resolve_common_predefined_tipset(tag)? {
                Ok(ts)
            } else {
                match tag {
                    Predefined::Safe => Ok(self.get_ec_safe_tipset()?),
                    Predefined::Finalized => Ok(self.get_ec_finalized_tipset()?),
                    tag => anyhow::bail!("unknown block tag: {tag}"),
                }
            }
        } else {
            self.resolve_predefined_tipset_v2(tag).await
        }
    }

    /// Resolves a predefined tipset according to the v2 API behavior.
    ///
    /// Uses a common predefined-tipset lookup first; if that yields no result, resolves
    /// `Safe` and `Finalized` tags via the v2 chain getters. Returns an error for unknown tags
    /// or on underlying resolution failures.
    ///
    /// # Returns
    ///
    /// The resolved `Tipset` on success.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures::executor::block_on;
    ///
    /// // `resolver` is a TipsetResolver and must be created in the calling context.
    /// // This example demonstrates the call form; actual construction depends on your setup.
    /// let resolver: &TipsetResolver<'_, _> = unimplemented!();
    /// let tipset = block_on(resolver.resolve_predefined_tipset_v2(Predefined::Safe)).unwrap();
    /// ```
    async fn resolve_predefined_tipset_v2(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        if let Some(ts) = self.resolve_common_predefined_tipset(tag)? {
            Ok(ts)
        } else {
            match tag {
                Predefined::Safe => ChainGetTipSetV2::get_latest_safe_tipset(self.ctx).await,
                Predefined::Finalized => {
                    ChainGetTipSetV2::get_latest_finalized_tipset(self.ctx).await
                }
                tag => anyhow::bail!("unknown block tag: {tag}"),
            }
        }
    }

    /// Attempt to resolve a predefined block tag to a commonly-handled tipset.
    ///
    /// Returns `Some(Tipset)` for `Predefined::Pending` (current head) and
    /// `Predefined::Latest` (the tipset at the head's parents). Returns `Ok(None)`
    /// when the tag is not handled by this common-resolution path (caller should
    /// try other resolution strategies). Resolving `Predefined::Earliest` fails
    /// with an error.
    ///
    /// # Examples
    ///
    /// ```
    /// // illustrative usage; assumes `resolver` is a configured TipsetResolver
    /// // let resolver: TipsetResolver<_> = ...;
    /// // let ts_opt = resolver.resolve_common_predefined_tipset(Predefined::Pending).unwrap();
    /// ```
    fn resolve_common_predefined_tipset(&self, tag: Predefined) -> anyhow::Result<Option<Tipset>> {
        let head = self.ctx.chain_store().heaviest_tipset();
        match tag {
            Predefined::Earliest => bail!("block param \"earliest\" is not supported"),
            Predefined::Pending => Ok(Some(head)),
            Predefined::Latest => Ok(Some(
                self.ctx
                    .chain_index()
                    .load_required_tipset(head.parents())?,
            )),
            _ => Ok(None),
        }
    }

    /// Returns the tipset considered "safe" relative to the current heaviest tipset.
    ///
    /// The safe tipset is the tipset at height `max(head.epoch() - SAFE_HEIGHT_DISTANCE, 0)`.
    ///
    /// # Examples
    ///
    /// ```
    /// // `resolver` is a `TipsetResolver` with a valid context.
    /// let safe = resolver.get_ec_safe_tipset().unwrap();
    /// let head = resolver.ctx.chain_store().heaviest_tipset();
    /// assert!(safe.epoch() <= head.epoch());
    /// ```
    pub fn get_ec_safe_tipset(&self) -> anyhow::Result<Tipset> {
        let head = self.ctx.chain_store().heaviest_tipset();
        let safe_height = (head.epoch() - SAFE_HEIGHT_DISTANCE).max(0);
        Ok(self.ctx.chain_index().tipset_by_height(
            safe_height,
            head,
            ResolveNullTipset::TakeOlder,
        )?)
    }

    /// Returns the tipset considered finalized by election-confirmation finality.
    ///
    /// The finalized epoch is computed as head.epoch() minus the chain's `policy.chain_finality`, clamped to zero. The tipset at that epoch is returned; when the exact height is unavailable, an older tipset is selected.
    ///
    /// # Examples
    ///
    /// ```
    /// # use anyhow::Result;
    /// # // `resolver` should be an initialized `TipsetResolver` in the test environment.
    /// let ts = resolver.get_ec_finalized_tipset()?;
    /// assert!(ts.epoch() <= resolver.ctx.chain_store().heaviest_tipset().epoch());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn get_ec_finalized_tipset(&self) -> anyhow::Result<Tipset> {
        let head = self.ctx.chain_store().heaviest_tipset();
        let ec_finality_epoch =
            (head.epoch() - self.ctx.chain_config().policy.chain_finality).max(0);
        Ok(self.ctx.chain_index().tipset_by_height(
            ec_finality_epoch,
            head,
            ResolveNullTipset::TakeOlder,
        )?)
    }
}
