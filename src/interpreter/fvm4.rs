// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::clock::ChainEpoch;
use cid::Cid;
use fvm_shared4::consensus::ConsensusFault;
use fvm4::externs::{Chain, Consensus, Externs, Rand};
use std::sync::atomic::{self, AtomicBool};

pub static LOG_ON: AtomicBool = AtomicBool::new(true);

pub type ForestExternsV4 = super::externs::ForestExterns;

impl Externs for ForestExternsV4 {}

impl Rand for ForestExternsV4 {
    fn get_chain_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        let r = crate::shim::externs::Rand::get_chain_randomness(self.rand.as_ref(), round)?;
        if LOG_ON.load(atomic::Ordering::Relaxed) {
            tracing::info!(
                "get_chain_randomness round: {round}, randomness: {}",
                hex::encode(r.as_slice())
            );
        }
        Ok(r)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]> {
        let r = crate::shim::externs::Rand::get_beacon_randomness(self.rand.as_ref(), round)?;
        if LOG_ON.load(atomic::Ordering::Relaxed) {
            tracing::info!(
                "get_beacon_randomness round: {round}, randomness: {}",
                hex::encode(r.as_slice())
            );
        }
        Ok(r)
    }
}

impl Consensus for ForestExternsV4 {
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> anyhow::Result<(Option<ConsensusFault>, i64)> {
        let f = Self::verify_consensus_fault(self, h1, h2, extra)
            .map(|(fault, gas_used)| (fault.map(From::from), gas_used))?;
        if LOG_ON.load(atomic::Ordering::Relaxed) {
            tracing::info!(
                "verify_consensus_fault h1: {}, h2: {}, extra: {}, f: {f:?}",
                hex::encode(h1),
                hex::encode(h2),
                hex::encode(extra)
            );
        }
        Ok(f)
    }
}

impl Chain for ForestExternsV4 {
    fn get_tipset_cid(&self, epoch: ChainEpoch) -> anyhow::Result<Cid> {
        let cid = Self::get_tipset_cid(self, epoch)?;
        if LOG_ON.load(atomic::Ordering::Relaxed) {
            tracing::info!("get_tipset_cid epoch: {epoch}, cid: {cid}");
        }
        Ok(cid)
    }
}
