// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Per-network reward bootstrap for FIP-0118, the only network-specific input of the Solstice
//! migration. Values from the Lotus `params_<network>.go` files at
//! <https://github.com/filecoin-project/lotus/blob/1c89ca8f80d58717d61b6a8de460f38febe4346a/build/buildconstants/params.go#L23-L36>.

use crate::networks::NetworkChain;
use crate::rpc::eth::types::EthAddress;
use crate::shim::address::Address;
use crate::shim::clock::{ChainEpoch, EPOCHS_IN_DAY, EPOCHS_IN_HOUR};
use crate::shim::state_tree::StateTree;
use anyhow::{Context as _, ensure};
use fil_actor_reward_state::v19::DENOM;
use fvm_ipld_blockstore::Blockstore;
use std::str::FromStr as _;

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
    /// Stream weight authority contract, `None` until it is deployed.
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

/// The consensus weight ramps over nine of the quarters the network's SRA is deployed with.
const RAMP_QUARTERS: ChainEpoch = 9;
/// A quarter of the builtin-actors year, 31_556_925 seconds of 30 second epochs.
const MAINNET_EPOCHS_PER_QUARTER: ChainEpoch = 262_974;

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
            NetworkChain::Mainnet => Self {
                swa_timelock_epochs: 7 * EPOCHS_IN_DAY,
                consensus_weight_ramp_duration_epochs: RAMP_QUARTERS * MAINNET_EPOCHS_PER_QUARTER,
                swa_actor: Some(evm_address("0xDE4fBd083F18f96C241DdE0A83C3EDC422Be9BA6")),
                sra_actor: Some(evm_address("0xeDfCd0947F7E9d58E0035f032520d75ce8eCA451")),
                initial_orchestrator: Some(evm_address(
                    "0x97A90f5696be5E3C8d3752C92Adac287c2b4484e",
                )),
                ..FIP0118
            },
            NetworkChain::Calibnet => Self {
                swa_timelock_epochs: EPOCHS_IN_HOUR * 6,
                consensus_weight_ramp_duration_epochs: RAMP_QUARTERS * EPOCHS_IN_DAY,
                swa_actor: Some(evm_address("0x66C11A9F6dfEC3c1557958cF9f575a023EB01421")),
                sra_actor: Some(evm_address("0x0339f205314C8210AF7Cb075d1A96D012e7896a9")),
                initial_orchestrator: Some(evm_address(
                    "0x97A90f5696be5E3C8d3752C92Adac287c2b4484e",
                )),
                ..FIP0118
            },
            NetworkChain::Butterflynet => Self {
                swa_timelock_epochs: 40,
                consensus_weight_ramp_duration_epochs: RAMP_QUARTERS * 2 * EPOCHS_IN_HOUR,
                swa_actor: Some(evm_address("0x17c43bC9d8E8600ebE7599C18f2dA2D5CED68D95")),
                sra_actor: Some(evm_address("0xea340224F4df7D01d2657964215E37452165b0A1")),
                initial_orchestrator: Some(evm_address(
                    "0x48C7DC38e74C9fA9eA6484Ad6Ad0520349dC9B40",
                )),
                ..FIP0118
            },
            // A devnet deploys no contracts, so the system actor stands in for all three.
            NetworkChain::Devnet(_) => Self {
                swa_timelock_epochs: 50,
                consensus_weight_ramp_duration_epochs: 900,
                swa_actor: Some(Address::SYSTEM_ACTOR),
                sra_actor: Some(Address::SYSTEM_ACTOR),
                initial_orchestrator: Some(Address::SYSTEM_ACTOR),
                ..FIP0118
            },
        }
    }

    /// Resolves the contract addresses to `f0` addresses against `actors`, the state tree the
    /// migration reads.
    ///
    /// # Errors
    /// The SWA is unset, the orchestrator is the burn actor, or a set address is not on chain.
    pub fn resolve<BS: Blockstore>(mut self, actors: &StateTree<BS>) -> anyhow::Result<Self> {
        ensure!(
            self.swa_actor.is_some(),
            "Solstice bootstrap SWA actor is unset"
        );
        // The reward actor strips the burn actor from share maps, so it cannot be a recipient.
        ensure!(
            self.initial_orchestrator != Some(Address::BURNT_FUNDS_ACTOR),
            "Solstice bootstrap initial orchestrator is the burn actor"
        );
        // A consensus-only bootstrap leaves the service stream addresses unset.
        for (name, address) in [
            ("SWA actor", &mut self.swa_actor),
            ("SRA actor", &mut self.sra_actor),
            ("initial orchestrator", &mut self.initial_orchestrator),
        ] {
            let Some(unresolved) = address.take() else {
                continue;
            };
            let id = actors.lookup_id(&unresolved)?.with_context(|| {
                format!("Solstice bootstrap {name} {unresolved} is not on chain")
            })?;
            *address = Some(Address::new_id(id));
        }
        Ok(self)
    }
}

/// An EVM address as Lotus writes it: `f0` for a masked ID, `f410` for anything else.
fn evm_address(hex: &str) -> Address {
    EthAddress::from_str(hex)
        .expect("hard-coded EVM address is well-formed")
        .to_filecoin_address()
        .expect("EVM address has a Filecoin form")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::shim::state_tree::{ActorState, StateTreeVersion};
    use crate::utils::cid::CidCborExt as _;
    use crate::utils::db::CborStoreExt as _;
    use cid::Cid;
    use std::sync::Arc;

    /// A state tree whose init actor maps each address to a fresh ID, returned in order.
    fn state_tree_with(addresses: &[Address]) -> (StateTree<Arc<MemoryDB>>, Vec<Address>) {
        let store = Arc::new(MemoryDB::default());
        let mut init_state =
            fil_actor_init_state::v19::State::new(&store, "migrationtest".into()).unwrap();
        let ids = addresses
            .iter()
            .map(|address| {
                let (id, _) = init_state
                    .map_addresses_to_id(&store, &address.into(), None)
                    .unwrap();
                Address::new_id(id)
            })
            .collect();
        let init_head = store.put_cbor_default(&init_state).unwrap();
        let init_actor = ActorState::new(
            Cid::from_cbor_blake2b256(&"init code").unwrap(),
            init_head,
            Default::default(),
            0,
            None,
        );
        let mut actors = StateTree::new(&store, StateTreeVersion::V5).unwrap();
        actors.set_actor(&Address::INIT_ACTOR, init_actor).unwrap();
        (actors, ids)
    }

    fn params_with(
        swa: Option<Address>,
        sra: Option<Address>,
        orchestrator: Option<Address>,
    ) -> SolsticeRewardBootstrapParams {
        SolsticeRewardBootstrapParams {
            swa_actor: swa,
            sra_actor: sra,
            initial_orchestrator: orchestrator,
            ..FIP0118
        }
    }

    fn contract(seed: u8) -> Address {
        Address::new_delegated(
            Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id().unwrap(),
            &[seed; 20],
        )
        .unwrap()
    }

    fn wallet(seed: u8) -> Address {
        Address::new_secp256k1(&[seed; 65]).unwrap()
    }

    #[test]
    fn resolves_contract_and_wallet_addresses_to_ids() {
        let (swa, sra, orchestrator) = (contract(1), contract(2), wallet(3));
        let (actors, ids) = state_tree_with(&[swa, sra, orchestrator]);

        let resolved = params_with(Some(swa), Some(sra), Some(orchestrator))
            .resolve(&actors)
            .unwrap();

        assert_eq!(
            (
                resolved.swa_actor,
                resolved.sra_actor,
                resolved.initial_orchestrator
            ),
            (Some(ids[0]), Some(ids[1]), Some(ids[2]))
        );
    }

    #[test]
    fn unset_service_stream_addresses_pass_through() {
        let swa = contract(1);
        let (actors, ids) = state_tree_with(&[swa]);

        let resolved = params_with(Some(swa), None, None).resolve(&actors).unwrap();

        assert_eq!(resolved.swa_actor, Some(ids[0]));
        assert_eq!(resolved.sra_actor, None);
        assert_eq!(resolved.initial_orchestrator, None);
    }

    #[test]
    fn rejects_an_unset_swa() {
        let (actors, _) = state_tree_with(&[]);

        let error = params_with(None, Some(contract(2)), Some(wallet(3)))
            .resolve(&actors)
            .unwrap_err();

        assert!(
            error.to_string().contains("SWA actor is unset"),
            "{error:#}"
        );
    }

    #[test]
    fn rejects_the_burn_actor_as_orchestrator() {
        let (swa, sra) = (contract(1), contract(2));
        let (actors, _) = state_tree_with(&[swa, sra]);

        let error = params_with(Some(swa), Some(sra), Some(Address::BURNT_FUNDS_ACTOR))
            .resolve(&actors)
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("initial orchestrator is the burn actor"),
            "{error:#}"
        );
    }

    #[test]
    fn rejects_an_address_missing_from_the_state_tree() {
        let (swa, sra, orchestrator) = (contract(1), contract(2), wallet(3));
        let cases = [
            ("SWA actor", swa, [sra, orchestrator]),
            ("SRA actor", sra, [swa, orchestrator]),
            ("initial orchestrator", orchestrator, [swa, sra]),
        ];
        for (name, missing, on_chain) in cases {
            let (actors, _) = state_tree_with(&on_chain);

            let error = params_with(Some(swa), Some(sra), Some(orchestrator))
                .resolve(&actors)
                .unwrap_err();

            let expected = format!("Solstice bootstrap {name} {missing} is not on chain");
            assert!(error.to_string().contains(&expected), "{error:#}");
        }
    }
}
