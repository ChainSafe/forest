// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::CachingBlockHeader;
use crate::rpc::RpcMethodExt;
use crate::slasher::filter::SlasherFilter;
use crate::slasher::types::ConsensusFault;
use anyhow::{Context, Result};
use fvm_ipld_encoding;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Slasher service that orchestrates consensus fault detection and reporting
pub struct SlasherService {
    /// Core fault detection filter
    filter: Arc<RwLock<SlasherFilter>>,
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

        let filter = Arc::new(RwLock::new(
            SlasherFilter::new(data_dir.clone()).with_context(|| {
                format!("Failed to create slasher filter in directory: {data_dir:?}")
            })?,
        ));

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
            filter,
            reporter_address,
        })
    }

    pub async fn process_block(&self, header: &CachingBlockHeader) -> Result<()> {
        info!(
            "Checking consensus fault for {} by miner address {} at epoch {}",
            header.cid(),
            header.miner_address,
            header.epoch
        );

        let fault = {
            let mut filter = self.filter.write().await;
            filter.process_block(header)?
        };

        if let Some(fault) = fault {
            info!(
                "Consensus fault detected: {:?} by miner {} at epoch {}",
                fault.fault_type, fault.miner_address, fault.detection_epoch
            );

            warn!(
                "Fault details - Block headers: {:?}, Extra evidence: {:?}",
                fault.block_headers, fault.extra_evidence
            );

            if let Err(e) = self.submit_fault_report(&fault).await {
                warn!("Failed to submit fault report: {}", e);
            }
        }

        Ok(())
    }

    async fn submit_fault_report(&self, fault: &ConsensusFault) -> Result<()> {
        info!(
            "Submitting consensus fault report for miner {} at epoch {}",
            fault.miner_address, fault.detection_epoch
        );

        let client =
            crate::rpc::Client::default_or_from_env(None).context("Failed to create RPC client")?;

        // Get the reporter address (use configured address or default wallet)
        let from = if let Some(addr) = &self.reporter_address {
            *addr
        } else {
            match crate::rpc::wallet::WalletDefaultAddress::call(&client, ()).await {
                Ok(Some(addr)) => addr,
                Ok(None) | Err(_) => {
                    return Err(anyhow::anyhow!(
                        "No wallet address configured for slasher. Set FOREST_FAULTREPORTER_CONSENSUSFAULTREPORTERADDRESS or configure a default wallet."
                    ));
                }
            }
        };

        let params = fil_actor_miner_state::v16::ReportConsensusFaultParams {
            header1: fault
                .block_headers
                .first()
                .ok_or_else(|| anyhow::anyhow!("No first block header found"))?
                .to_bytes(),
            header2: fault
                .block_headers
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("No second block header found"))?
                .to_bytes(),
            header_extra: fault
                .extra_evidence
                .map(|cid| cid.to_bytes())
                .unwrap_or_default(),
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

        let signed_msg = crate::rpc::mpool::MpoolPushMessage::call(&client, (message, None))
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
