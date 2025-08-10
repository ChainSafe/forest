// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::CachingBlockHeader;
use crate::rpc::Client;
use crate::rpc::RpcMethodExt;
use crate::rpc::chain::ChainGetBlock;
use crate::slasher::db::*;
use crate::slasher::types::{ConsensusFault, ConsensusFaultType};
use anyhow::{Context, Result};
use cid::Cid;
use fvm_ipld_encoding;
use fvm_ipld_encoding::to_vec;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Slasher service that orchestrates consensus fault detection and reporting
pub struct SlasherService {
    /// Database for storing slasher history
    db: Arc<RwLock<SlasherDb>>,
    /// RPC Client
    client: Client,
    /// Reporter address for submitting fault reports
    reporter_address: Option<crate::shim::address::Address>,
}

impl SlasherService {
    pub fn new() -> Result<Self> {
        let data_dir = std::env::var("FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERDATADIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(".forest/slasher"));

        let reporter_address = std::env::var("FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERADDRESS")
            .ok()
            .and_then(|addr_str| addr_str.parse().ok());

        let client =
            crate::rpc::Client::default_or_from_env(None).context("Failed to create RPC client")?;

        let db = Arc::new(RwLock::new(SlasherDb::new(data_dir.clone()).with_context(
            || format!("Failed to create slasher db in directory: {data_dir:?}"),
        )?));

        info!(
            "Slasher service initialized with data directory: {:?}",
            data_dir
        );
        if let Some(addr) = &reporter_address {
            info!("Fault reporter address configured: {}", addr);
        } else {
            info!("No fault reporter address configured - will use default wallet address");
        }

        Ok(Self {
            db,
            client,
            reporter_address,
        })
    }

    async fn check_fault(&self, header: &CachingBlockHeader) -> Result<Option<ConsensusFault>> {
        if let Some(fault) = self.check_double_fork_mining(header).await? {
            return Ok(Some(fault));
        }

        if let Some(fault) = self.check_time_offset_mining(header).await? {
            return Ok(Some(fault));
        }

        if let Some(fault) = self.check_parent_grinding(header).await? {
            return Ok(Some(fault));
        }

        self.db
            .write()
            .await
            .put(header)
            .context("Failed to add block header to slasher history")?;
        Ok(None)
    }

    async fn check_double_fork_mining(
        &self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        let miner = header.miner_address;
        let epoch = header.epoch;
        let epoch_key = format!("{}/{}", miner, epoch);

        // Check if we already have a block from this miner at this epoch
        let existing_block_cid = {
            let db = self.db.read().await;
            db.get(SlasherDbColumns::ByEpoch as u8, epoch_key.as_bytes())?
        };

        if let Some(block_cid_bytes) = existing_block_cid {
            let existing_block_cid = Cid::try_from(block_cid_bytes)?;

            // Get the existing block to compare
            let existing_block = ChainGetBlock::call(&self.client, (existing_block_cid,)).await?;

            // Check if this is a different block (double fork mining)
            if existing_block.cid() != header.cid() && existing_block.epoch == header.epoch {
                return Ok(Some(ConsensusFault {
                    miner_address: miner,
                    detection_epoch: epoch,
                    fault_type: ConsensusFaultType::DoubleForkMining,
                    block_header_1: existing_block,
                    block_header_2: header.clone(),
                    block_header_extra: None,
                }));
            }
        }
        Ok(None)
    }

    async fn check_time_offset_mining(
        &self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        let miner = header.miner_address;
        let parents = &header.parents;
        let parent_key = format!("{}/{}", miner, parents);

        // Check if we already have a block from this miner with the same parents
        let existing_block_cid = {
            let db = self.db.read().await;
            db.get(SlasherDbColumns::ByParents as u8, parent_key.as_bytes())?
        };

        if let Some(block_cid_bytes) = existing_block_cid {
            let existing_block_cid = Cid::try_from(block_cid_bytes)?;

            // Get the existing block to compare
            let existing_block = ChainGetBlock::call(&self.client, (existing_block_cid,)).await?;

            // Check if this is a time offset mining fault
            if existing_block.cid() != header.cid()
                && existing_block.parents == header.parents
                && existing_block.epoch != header.epoch
            {
                return Ok(Some(ConsensusFault {
                    miner_address: miner,
                    detection_epoch: header.epoch,
                    fault_type: ConsensusFaultType::TimeOffsetMining,
                    block_header_1: existing_block,
                    block_header_2: header.clone(),
                    block_header_extra: None,
                }));
            }
        }

        Ok(None)
    }

    async fn check_parent_grinding(
        &self,
        header: &CachingBlockHeader,
    ) -> Result<Option<ConsensusFault>> {
        // Parent grinding detection is complex and requires:
        // 1. Block A: Mined by same miner at epoch N with parents P
        // 2. Block C: Sibling of A (same epoch N, same parents P, different miner)
        // 3. Block B: Current block at later epoch, includes C but excludes A

        let miner = header.miner_address;
        let epoch = header.epoch;
        let parents = &header.parents;
        let parent_block = ChainGetBlock::call(&self.client, (parents.cid()?,)).await?;
        let parent_epoch = parent_block.epoch;

        let parent_epoch_key = format!("{}/{}", miner, parent_epoch);

        // Check if we already have a block from this miner at this epoch
        let existing_parent_block_cid = {
            let db = self.db.read().await;
            db.get(SlasherDbColumns::ByEpoch as u8, parent_epoch_key.as_bytes())?
        };

        if let Some(block_cid_bytes) = existing_parent_block_cid {
            let parent_cid = Cid::try_from(block_cid_bytes)?;
            if parents.contains(parent_cid) {
                let existing_parent_block =
                    ChainGetBlock::call(&self.client, (parent_cid,)).await?;
                if existing_parent_block.parents == parent_block.parents
                    && existing_parent_block.epoch == parent_block.epoch
                    && header.parents.contains(*parent_block.cid())
                    && !header.parents.contains(*existing_parent_block.cid())
                {
                    // Detected parent grinding fault
                    return Ok(Some(ConsensusFault {
                        miner_address: miner,
                        detection_epoch: epoch,
                        fault_type: ConsensusFaultType::ParentGrinding,
                        block_header_1: existing_parent_block,
                        block_header_2: header.clone(),
                        block_header_extra: Some(parent_block),
                    }));
                }
            }
        }

        Ok(None)
    }

    pub async fn process_block(&self, header: &CachingBlockHeader) -> Result<()> {
        info!(
            "Checking consensus fault for {} by miner address {} at epoch {}",
            header.cid(),
            header.miner_address,
            header.epoch
        );

        let fault = self.check_fault(header).await?;

        if let Some(fault) = fault {
            info!(
                "Consensus fault detected: {:?} by miner {} at epoch {}",
                fault.fault_type, fault.miner_address, fault.detection_epoch
            );

            info!(
                "Fault details - Block header 1: {:?}, Block header 2: {:?}, Extra evidence: {:?}",
                fault.block_header_1, fault.block_header_2, fault.block_header_extra
            );

            if let Err(e) = self.submit_fault_report(&fault).await {
                warn!("Failed to submit fault report: {}", e);
            }
        }

        Ok(())
    }

    pub async fn submit_fault_report(&self, fault: &ConsensusFault) -> Result<()> {
        info!(
            "Submitting consensus fault report for miner {} at epoch {}",
            fault.miner_address, fault.detection_epoch
        );

        // Get the reporter address (use configured address or default wallet)
        let from = if let Some(addr) = &self.reporter_address {
            *addr
        } else {
            match crate::rpc::wallet::WalletDefaultAddress::call(&self.client, ()).await {
                Ok(Some(addr)) => addr,
                Ok(None) | Err(_) => {
                    return Err(anyhow::anyhow!(
                        "No wallet address configured for slasher. Set FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERADDRESS or configure a default wallet."
                    ));
                }
            }
        };
        let params = fil_actor_miner_state::v16::ReportConsensusFaultParams {
            header1: to_vec(&fault.block_header_1)?,
            header2: to_vec(&fault.block_header_2)?,
            header_extra: match &fault.block_header_extra {
                Some(header) => to_vec(header)?,
                None => Vec::new(),
            },
        };

        let message = crate::shim::message::Message {
            from,
            to: fault.miner_address,
            value: crate::shim::econ::TokenAmount::default(),
            method_num: fil_actor_miner_state::v16::Method::ReportConsensusFault as u64,
            params: fvm_ipld_encoding::to_vec(&params)
                .context("Failed to convert params to bytes")?
                .into(),
            ..Default::default()
        };

        let signed_msg = crate::rpc::mpool::MpoolPushMessage::call(&self.client, (message, None))
            .await
            .context("Failed to submit consensus fault report")?;

        info!(
            "Consensus fault report submitted successfully: {} (reporter={}, miner={}, fault_type={:?})",
            signed_msg.message.cid(),
            from,
            fault.miner_address,
            fault.fault_type
        );

        Ok(())
    }
}
