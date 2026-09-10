// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Per-network reward bootstrap for FIP-0118, the only network-specific input of the Solstice
//! migration. Values from the Lotus `params_<network>.go` files at
//! <https://github.com/filecoin-project/lotus/blob/1b0155685292f691babd930f1060562ecff645c3/build/buildconstants/params.go#L23-L31>.

use crate::networks::NetworkChain;
use crate::shim::address::Address;
use crate::shim::clock::{ChainEpoch, EPOCHS_IN_DAY, EPOCHS_IN_HOUR};
use fil_actor_reward_state::v19::DENOM;

/// The network's input to the reward migration: timelock, ramp, weights and contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolsticeRewardBootstrapParams {
    /// Delay before a stream weight authority (SWA) write takes effect.
    pub swa_timelock_epochs: ChainEpoch,
    /// Epochs over which the consensus stream weight ramps down from its start to its floor.
    /// Zero installs the consensus stream alone at constant `DENOM`.
    pub consensus_weight_ramp_duration_epochs: ChainEpoch,
    pub consensus_weight: SolsticeRewardWeightParams,
    pub service_weight: SolsticeRewardWeightParams,
    /// Stream weight authority contract, `None` until it is deployed and has an `f0` address.
    pub swa_actor: Option<Address>,
    /// Service reward authority contract, the writer of the service stream's shares.
    pub sra_actor: Option<Address>,
    /// Sole initial recipient of the service stream.
    pub initial_orchestrator: Option<Address>,
}

/// A clamped linear stream weight in `DENOM` fixed point, without its slope and start epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolsticeRewardWeightParams {
    pub v_start: u64,
    pub floor: u64,
    pub cap: u64,
}

pub(super) const PERCENT: u64 = DENOM / 100;

/// The FIP-0118 weights, with the timelock, ramp and contract addresses filled in per network.
const FIP0118: SolsticeRewardBootstrapParams = SolsticeRewardBootstrapParams {
    swa_timelock_epochs: 0,
    consensus_weight_ramp_duration_epochs: 0,
    consensus_weight: SolsticeRewardWeightParams {
        v_start: 95 * PERCENT,
        floor: 50 * PERCENT,
        cap: 95 * PERCENT,
    },
    service_weight: SolsticeRewardWeightParams {
        v_start: 5 * PERCENT,
        floor: 5 * PERCENT,
        cap: 10 * PERCENT,
    },
    swa_actor: None,
    sra_actor: None,
    initial_orchestrator: None,
};

impl SolsticeRewardBootstrapParams {
    /// The bootstrap of `chain`, with the contract addresses unset where none is deployed.
    pub fn for_chain(chain: &NetworkChain) -> Self {
        match chain {
            NetworkChain::Mainnet | NetworkChain::Butterflynet => Self {
                swa_timelock_epochs: 7 * EPOCHS_IN_DAY,
                consensus_weight_ramp_duration_epochs: 9 * 90 * EPOCHS_IN_DAY,
                ..FIP0118
            },
            NetworkChain::Calibnet => Self {
                swa_timelock_epochs: EPOCHS_IN_HOUR,
                consensus_weight_ramp_duration_epochs: 7 * EPOCHS_IN_DAY,
                ..FIP0118
            },
            // The burnt-funds orchestrator copies the Lotus 2k network; the reward actor rejects
            // it as a stored recipient, see the reward migration tests.
            NetworkChain::Devnet(_) => Self {
                swa_timelock_epochs: 50,
                consensus_weight_ramp_duration_epochs: 900,
                swa_actor: Some(Address::SYSTEM_ACTOR),
                sra_actor: Some(Address::SYSTEM_ACTOR),
                initial_orchestrator: Some(Address::BURNT_FUNDS_ACTOR),
                ..FIP0118
            },
        }
    }
}
