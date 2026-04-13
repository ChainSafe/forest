// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::rpc::chain::{ChainGetTipSetFinalityStatus, SAFE_HEIGHT_DISTANCE};

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
    /// # Returns
    ///
    /// The resolved `Tipset`, or an error if resolution fails.
    async fn resolve_predefined_tipset(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        match self.api_version {
            ApiPaths::V2 => self.resolve_predefined_tipset_v2(tag).await,
            ApiPaths::V1 | ApiPaths::V0 => self.resolve_predefined_tipset_v1(tag).await,
        }
    }

    /// Resolves a predefined tipset using the V1 resolution policy, or delegates to the V2 resolver when the
    /// V1 finality-resolution override is not enabled.
    ///
    /// If the environment variable `FOREST_ETH_V1_DISABLE_F3_FINALITY_RESOLUTION` is set to a truthy value,
    /// this function first attempts common predefined tag resolution (e.g., Pending, Latest). If that yields
    /// no result, the function uses expected-consensus finality to resolve the "safe" or "finalized" tipset
    /// for the corresponding `Predefined` tag. When the environment variable is not set or is falsy,
    /// resolution is delegated to the V2 resolver.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested predefined tag is unknown or if tipset resolution fails.
    async fn resolve_predefined_tipset_v1(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        const ETH_V1_DISABLE_F3_FINALITY_RESOLUTION_ENV_KEY: &str =
            "FOREST_ETH_V1_DISABLE_F3_FINALITY_RESOLUTION";

        crate::def_is_env_truthy!(
            f3_finality_disabled,
            ETH_V1_DISABLE_F3_FINALITY_RESOLUTION_ENV_KEY
        );

        if f3_finality_disabled() {
            if let Some(ts) = self.resolve_common_predefined_tipset(tag)? {
                Ok(ts)
            } else {
                match tag {
                    Predefined::Safe => self.get_ec_safe_tipset(),
                    Predefined::Finalized => self.get_ec_finalized_tipset(),
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
            Predefined::Safe | Predefined::Finalized => Ok(None),
        }
    }

    /// Returns the tipset considered "safe" relative to the current heaviest tipset.
    ///
    /// The safe tipset is the tipset at height `max(head.epoch() - SAFE_HEIGHT_DISTANCE, 0)`.
    pub fn get_ec_safe_tipset(&self) -> anyhow::Result<Tipset> {
        let head = self.ctx.chain_store().heaviest_tipset();
        let safe_height = (head.epoch() - SAFE_HEIGHT_DISTANCE).max(0);
        Ok(self.ctx.chain_index().tipset_by_height(
            safe_height,
            head,
            ResolveNullTipset::TakeOlder,
        )?)
    }

    /// Returns the tipset considered finalized by the expected-consensus finality calculator(`FRC-0089`).
    pub fn get_ec_finalized_tipset(&self) -> anyhow::Result<Tipset> {
        let head = self.ctx.chain_store().heaviest_tipset();
        let (_, ec_finalized_tipset) =
            ChainGetTipSetFinalityStatus::get_ec_finality_threshold_depth_and_tipset_with_cache(
                self.ctx,
                head.clone(),
            );
        ec_finalized_tipset.context("failed to resolve EC finalized tipset")
    }
}
