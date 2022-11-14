// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use async_trait::async_trait;
use core::time::Duration;
use futures::StreamExt;
use log::{error, info};
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::IntervalStream;

use forest_blocks::{BlockHeader, GossipBlock, Tipset};
use forest_chain::Scale;
use forest_chain_sync::consensus::{MessagePoolApi, Proposer, SyncGossipSubmitter};
use forest_db::Store;
use forest_key_management::Key;
use forest_networks::Height;
use forest_state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;

use crate::DelegatedConsensus;

// `DelegatedProposer` could have fields such as the `chain_config`,
// but since everything is accessible through the `StateManager`
// there is little incentive for that at the moment.
// In other consensus types it could share some fields with the
// validations, for example this component could maintain the
// finalized total order of transactions, which the validations
// also access to check if the Filecoin blocks reflect the same.

/// `DelegatedProposer` is a transient construct only created on the
/// node doing all block proposals, it is responsible for doing the
/// infinite loop of block creation. It needs access to the private
/// key corresponding to the ID of the only actor allowed to sign
/// blocks.
pub struct DelegatedProposer {
    miner_addr: Address,
    key: Key,
}

impl DelegatedProposer {
    pub(crate) fn new(miner_addr: Address, key: Key) -> Self {
        Self { miner_addr, key }
    }

    async fn create_block<DB>(
        &self,
        mpool: &impl MessagePoolApi,
        state_manager: &Arc<StateManager<DB>>,
        base: &Arc<Tipset>,
    ) -> anyhow::Result<GossipBlock>
    where
        DB: Blockstore + Store + Clone + Sync + Send + 'static,
    {
        let block_delay = state_manager.chain_config().block_delay_secs;
        let smoke_height = state_manager.chain_config().epoch(Height::Smoke);

        let (parent_state_root, parent_receipts) = state_manager.tipset_state(base).await?;
        let parent_base_fee =
            forest_chain::compute_base_fee(state_manager.blockstore(), base, smoke_height)?;

        let parent_weight = DelegatedConsensus::weight(state_manager.blockstore(), base)?;
        let msgs = mpool.select_signed(state_manager, base).await?;
        let msgs = msgs.iter().map(|m| m.as_ref()).collect();
        let persisted = forest_chain::persist_block_messages(state_manager.blockstore(), msgs)?;

        let mut header = BlockHeader::builder()
            .messages(persisted.msg_cid)
            .bls_aggregate(Some(persisted.bls_agg))
            .miner_address(self.miner_addr)
            .weight(parent_weight)
            .parent_base_fee(parent_base_fee)
            .parents(base.key().clone())
            .epoch(base.epoch() + 1)
            .timestamp(base.min_timestamp() + block_delay)
            .state_root(parent_state_root)
            .message_receipts(parent_receipts)
            .build()?;

        let sig = forest_key_management::sign(
            *self.key.key_info.key_type(),
            self.key.key_info.private_key(),
            &header.to_signing_bytes(),
        )?;

        header.signature = Some(sig);

        Ok(GossipBlock {
            header,
            bls_messages: persisted.bls_cids,
            secpk_messages: persisted.secp_cids,
        })
    }
}

#[async_trait]
impl Proposer for DelegatedProposer {
    async fn spawn<DB, MP>(
        self,
        state_manager: Arc<StateManager<DB>>,
        mpool: Arc<MP>,
        submitter: SyncGossipSubmitter,
        services: &mut JoinSet<()>,
    ) -> anyhow::Result<()>
    where
        DB: Blockstore + Store + Clone + Sync + Send + 'static,
        MP: MessagePoolApi + Send + Sync + 'static,
    {
        services.spawn(async move {
            if let Err(e) = self.run(state_manager, mpool.as_ref(), &submitter).await {
                error!("block proposal stopped: {}", e)
            }
        });
        Ok(())
    }
}

impl DelegatedProposer {
    async fn run<DB, MP>(
        self,
        state_manager: Arc<StateManager<DB>>,
        mpool: &MP,
        submitter: &SyncGossipSubmitter,
    ) -> anyhow::Result<()>
    where
        DB: Blockstore + Store + Clone + Sync + Send + 'static,
        MP: MessagePoolApi + Send + Sync + 'static,
    {
        // TODO: Ideally these should not be coming through the `StateManager`.
        let chain_config = state_manager.chain_config();
        let chain_store = state_manager.chain_store();

        let mut interval = IntervalStream::new(tokio::time::interval(Duration::from_secs(
            chain_config.block_delay_secs,
        )));

        while interval.next().await.is_some() {
            if let Some(base) = chain_store.heaviest_tipset().await {
                info!(
                    "Proposing a block on top {} in epoch {}",
                    base.min_ticket_block().cid(),
                    base.epoch(),
                );
                match self.create_block(mpool, &state_manager, &base).await {
                    Ok(block) => {
                        let cid = *block.header.cid();
                        let msg_cnt = block.secpk_messages.len() + block.bls_messages.len();
                        match submitter.submit_block(block).await {
                            Ok(()) => info!("Proposed block {} with {} messages", cid, msg_cnt),
                            Err(e) => error!("Failed to submit block: {}", e),
                        }
                    }
                    Err(e) => {
                        // The eudico version keeps going, but if we can't create blocks,
                        // maybe that's a good enough reason to throw in the towel.
                        return Err(anyhow!(e));
                    }
                }
            }
        }

        Ok(())
    }
}
