use address::Address;
use clock::ChainEpoch;
use encoding::BytesDe;
use fil_types::{RegisteredSealProof, SectorSize};
use ipld_blockstore::BlockStore;
use libp2p::PeerId;
use serde::Serialize;
use std::error::Error;
use vm::ActorState;

/// Miner actor method.
pub type Method = actorv2::miner::Method;

/// Miner actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::miner::State),
    V2(actorv2::miner::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<Option<State>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::MINER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V0))
        } else if actor.code == *actorv2::MINER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V2))
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    pub fn info<BS: BlockStore>(&self, store: &BS) -> Result<MinerInfo, Box<dyn Error>> {
        match self {
            State::V0(st) => {
                let info = st.get_info(store)?;

                let peer_id = PeerId::from_bytes(info.peer_id)
                    .map_err(|e| format!("bytes {:?} cannot be converted into a PeerId", e))?;

                Ok(MinerInfo {
                    owner: info.owner,
                    worker: info.worker,
                    control_addresses: info.control_addresses,
                    new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
                    worker_change_epoch: info
                        .pending_worker_key
                        .map(|k| k.effective_at)
                        .unwrap_or(-1),
                    peer_id,
                    multiaddrs: info.multi_address,
                    seal_proof_type: info.seal_proof_type,
                    sector_size: info.sector_size,
                    window_post_partition_sectors: info.window_post_partition_sectors,
                    consensus_fault_elapsed: -1,
                })
            }
            State::V2(st) => {
                let info = st.get_info(store)?;

                let peer_id = PeerId::from_bytes(info.peer_id)
                    .map_err(|e| format!("bytes {:?} cannot be converted into a PeerId", e))?;

                Ok(MinerInfo {
                    owner: info.owner,
                    worker: info.worker,
                    control_addresses: info.control_addresses,
                    new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
                    worker_change_epoch: info
                        .pending_worker_key
                        .map(|k| k.effective_at)
                        .unwrap_or(-1),
                    peer_id,
                    multiaddrs: info.multi_address,
                    seal_proof_type: info.seal_proof_type,
                    sector_size: info.sector_size,
                    window_post_partition_sectors: info.window_post_partition_sectors,
                    // TODO update on v2 update
                    consensus_fault_elapsed: -1,
                })
            }
        }
    }
}

/// Static information about miner
#[derive(Debug, PartialEq)]
pub struct MinerInfo {
    pub owner: Address,
    pub worker: Address,
    pub new_worker: Option<Address>,
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    pub peer_id: PeerId,
    pub multiaddrs: Vec<BytesDe>,
    pub seal_proof_type: RegisteredSealProof,
    pub sector_size: SectorSize,
    pub window_post_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
}
