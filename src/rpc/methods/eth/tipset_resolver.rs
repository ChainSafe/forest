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
    pub fn new(ctx: &'a Ctx<DB>, api_version: ApiPaths) -> Self {
        Self { ctx, api_version }
    }

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

    pub async fn resolve_predefined_tipset(&self, tag: Predefined) -> anyhow::Result<Tipset> {
        match self.api_version {
            ApiPaths::V2 => self.resolve_predefined_tipset_v2(tag).await,
            _ => self.resolve_predefined_tipset_v1(tag).await,
        }
    }

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

    pub fn get_ec_safe_tipset(&self) -> anyhow::Result<Tipset> {
        let head = self.ctx.chain_store().heaviest_tipset();
        let safe_height = (head.epoch() - SAFE_HEIGHT_DISTANCE).max(0);
        Ok(self.ctx.chain_index().tipset_by_height(
            safe_height,
            head,
            ResolveNullTipset::TakeOlder,
        )?)
    }

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
