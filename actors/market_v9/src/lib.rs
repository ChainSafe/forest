// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeSet;

use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::BytesKey;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::{ChainEpoch, QuantSpec};
use fvm_shared::deal::DealID;
use fvm_shared::error::ExitCode;
use fvm_shared::sector::SectorSize;
use fvm_shared::METHOD_CONSTRUCTOR;
use integer_encoding::VarInt;
use num_derive::FromPrimitive;
use num_traits::Zero;

use fil_actors_runtime_v9::runtime::Policy;
use fil_actors_runtime_v9::{actor_error, ActorContext, ActorError, AsActorError};

pub use self::deal::*;
pub use self::state::*;
pub use self::types::*;

// exports for testing
pub mod balance_table;
pub mod policy;

mod deal;
mod state;
mod types;

#[cfg(feature = "fil-actor")]
fil_actors_runtime::wasm_trampoline!(Actor);

pub const NO_ALLOCATION_ID: u64 = 0;

/// Market actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddBalance = 2,
    WithdrawBalance = 3,
    PublishStorageDeals = 4,
    VerifyDealsForActivation = 5,
    ActivateDeals = 6,
    OnMinerSectorsTerminate = 7,
    ComputeDataCommitment = 8,
    CronTick = 9,
}

pub fn validate_and_return_deal_space<BS>(
    proposals: &DealArray<BS>,
    deal_ids: &[DealID],
    miner_addr: &Address,
    sector_expiry: ChainEpoch,
    sector_activation: ChainEpoch,
    sector_size: Option<SectorSize>,
) -> Result<DealSpaces, ActorError>
where
    BS: Blockstore,
{
    let mut seen_deal_ids = BTreeSet::new();
    let mut deal_space = BigInt::zero();
    let mut verified_deal_space = BigInt::zero();
    for deal_id in deal_ids {
        if !seen_deal_ids.insert(deal_id) {
            return Err(actor_error!(
                illegal_argument,
                "deal id {} present multiple times",
                deal_id
            ));
        }
        let proposal = proposals
            .get(*deal_id)
            .context_code(ExitCode::USR_ILLEGAL_STATE, "failed to load deal")?
            .ok_or_else(|| actor_error!(not_found, "no such deal {}", deal_id))?;

        validate_deal_can_activate(proposal, miner_addr, sector_expiry, sector_activation)
            .with_context(|| format!("cannot activate deal {}", deal_id))?;

        if proposal.verified_deal {
            verified_deal_space += proposal.piece_size.0;
        } else {
            deal_space += proposal.piece_size.0;
        }
    }
    if let Some(sector_size) = sector_size {
        let total_deal_space = deal_space.clone() + verified_deal_space.clone();
        if total_deal_space > BigInt::from(sector_size as u64) {
            return Err(actor_error!(
                illegal_argument,
                "deals too large to fit in sector {} > {}",
                total_deal_space,
                sector_size
            ));
        }
    }

    Ok(DealSpaces {
        deal_space,
        verified_deal_space,
    })
}

pub fn gen_rand_next_epoch(
    policy: &Policy,
    start_epoch: ChainEpoch,
    deal_id: DealID,
) -> ChainEpoch {
    let offset = deal_id as i64 % policy.deal_updates_interval;
    let q = QuantSpec {
        unit: policy.deal_updates_interval,
        offset: 0,
    };
    let prev_day = q.quantize_down(start_epoch);
    if prev_day + offset >= start_epoch {
        return prev_day + offset;
    }
    let next_day = q.quantize_up(start_epoch);
    next_day + offset
}

////////////////////////////////////////////////////////////////////////////////
// Checks
////////////////////////////////////////////////////////////////////////////////
fn validate_deal_can_activate(
    proposal: &DealProposal,
    miner_addr: &Address,
    sector_expiration: ChainEpoch,
    curr_epoch: ChainEpoch,
) -> Result<(), ActorError> {
    if &proposal.provider != miner_addr {
        return Err(actor_error!(
            forbidden,
            "proposal has provider {}, must be {}",
            proposal.provider,
            miner_addr
        ));
    };

    if curr_epoch > proposal.start_epoch {
        return Err(actor_error!(
            illegal_argument,
            "proposal start epoch {} has already elapsed at {}",
            proposal.start_epoch,
            curr_epoch
        ));
    };

    if proposal.end_epoch > sector_expiration {
        return Err(actor_error!(
            illegal_argument,
            "proposal expiration {} exceeds sector expiration {}",
            proposal.end_epoch,
            sector_expiration
        ));
    };

    Ok(())
}

pub const DAG_CBOR: u64 = 0x71; // TODO is there a better place to get this?

pub fn deal_id_key(k: DealID) -> BytesKey {
    let bz = k.encode_var_vec();
    bz.into()
}
