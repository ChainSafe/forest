// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use cid::Cid;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{BytesDe, Cbor, CborStore, RawBytes};
use fvm_shared::address::{Address, Protocol};
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::*;
use fvm_shared::sector::*;
use fvm_shared::METHOD_CONSTRUCTOR;
use multihash::Code::Blake2b256;
use num_derive::FromPrimitive;
use num_traits::Zero;

pub use beneficiary::*;
pub use bitfield_queue::*;
pub use commd::*;
pub use deadline_assignment::*;
pub use deadline_info::*;
pub use deadline_state::*;
pub use deadlines::*;
pub use expiration_queue::*;
use fil_actors_runtime_v9::cbor::deserialize;
use fil_actors_runtime_v9::runtime::builtins::Type;
use fil_actors_runtime_v9::runtime::{Policy, Runtime};
use fil_actors_runtime_v9::{
    actor_error, ActorDowncast, ActorError, CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR,
};
pub use monies::*;
pub use partition_state::*;
pub use policy::*;
pub use sector_map::*;
pub use sectors::*;
pub use state::*;
pub use termination::*;
pub use types::*;
pub use vesting_state::*;

mod beneficiary;
mod bitfield_queue;
mod commd;
mod deadline_assignment;
mod deadline_info;
mod deadline_state;
mod deadlines;
mod expiration_queue;
#[doc(hidden)]
pub mod ext;
mod monies;
mod partition_state;
mod policy;
mod sector_map;
mod sectors;
mod state;
mod termination;
pub mod testing;
mod types;
mod vesting_state;

// The first 1000 actor-specific codes are left open for user error, i.e. things that might
// actually happen without programming error in the actor code.

// * Updated to specs-actors commit: 17d3c602059e5c48407fb3c34343da87e6ea6586 (v0.9.12)

/// Storage Miner actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    ControlAddresses = 2,
    ChangeWorkerAddress = 3,
    ChangePeerID = 4,
    SubmitWindowedPoSt = 5,
    PreCommitSector = 6,
    ProveCommitSector = 7,
    ExtendSectorExpiration = 8,
    TerminateSectors = 9,
    DeclareFaults = 10,
    DeclareFaultsRecovered = 11,
    OnDeferredCronEvent = 12,
    CheckSectorProven = 13,
    ApplyRewards = 14,
    ReportConsensusFault = 15,
    WithdrawBalance = 16,
    ConfirmSectorProofsValid = 17,
    ChangeMultiaddrs = 18,
    CompactPartitions = 19,
    CompactSectorNumbers = 20,
    ConfirmUpdateWorkerKey = 21,
    RepayDebt = 22,
    ChangeOwnerAddress = 23,
    DisputeWindowedPoSt = 24,
    PreCommitSectorBatch = 25,
    ProveCommitAggregate = 26,
    ProveReplicaUpdates = 27,
    PreCommitSectorBatch2 = 28,
    ProveReplicaUpdates2 = 29,
    ChangeBeneficiary = 30,
    GetBeneficiary = 31,
    ExtendSectorExpiration2 = 32,
}

pub const ERR_BALANCE_INVARIANTS_BROKEN: ExitCode = ExitCode::new(1000);

/// Miner Actor
/// here in order to update the Power Actor to v3.
pub struct Actor;

impl Actor {
    pub fn constructor<BS, RT>(
        rt: &mut RT,
        params: MinerConstructorParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&INIT_ACTOR_ADDR))?;

        check_control_addresses(rt.policy(), &params.control_addresses)?;
        check_peer_info(rt.policy(), &params.peer_id, &params.multi_addresses)?;
        check_valid_post_proof_type(rt.policy(), params.window_post_proof_type)?;

        let owner = resolve_control_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;
        let control_addresses: Vec<_> = params
            .control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        let policy = rt.policy();
        let current_epoch = rt.curr_epoch();
        let blake2b = |b: &[u8]| rt.hash_blake2b(b);
        let offset =
            assign_proving_period_offset(policy, rt.message().receiver(), current_epoch, blake2b)
                .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_SERIALIZATION,
                    "failed to assign proving period offset",
                )
            })?;

        let period_start = current_proving_period_start(policy, current_epoch, offset);
        if period_start > current_epoch {
            return Err(actor_error!(
                illegal_state,
                "computed proving period start {} after current epoch {}",
                period_start,
                current_epoch
            ));
        }

        let deadline_idx = current_deadline_index(policy, current_epoch, period_start);
        if deadline_idx >= policy.wpost_period_deadlines {
            return Err(actor_error!(
                illegal_state,
                "computed proving deadline index {} invalid",
                deadline_idx
            ));
        }

        let info = MinerInfo::new(
            owner,
            worker,
            control_addresses,
            params.peer_id,
            params.multi_addresses,
            params.window_post_proof_type,
        )?;
        let info_cid = rt.store().put_cbor(&info, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to construct illegal state",
            )
        })?;

        let st =
            State::new(policy, rt.store(), info_cid, period_start, deadline_idx).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to construct state")
            })?;
        rt.create(&st)?;

        Ok(())
    }
}

#[derive(Debug, PartialEq, Clone)]
struct SectorPreCommitInfoInner {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// CommR
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    /// CommD
    pub unsealed_cid: Option<CompactCommD>,
}

/// ReplicaUpdate param with `Option<Cid>` for CommD
/// None means unknown
pub struct ReplicaUpdateInner {
    pub sector_number: SectorNumber,
    pub deadline: u64,
    pub partition: u64,
    pub new_sealed_cid: Cid,
    /// None means unknown
    pub new_unsealed_cid: Option<Cid>,
    pub deals: Vec<DealID>,
    pub update_proof_type: RegisteredUpdateProof,
    pub replica_proof: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedExpirationExtension {
    pub deadline: u64,
    pub partition: u64,
    pub sectors: BitField,
    pub new_expiration: ChainEpoch,
}

#[allow(clippy::too_many_arguments)] // validate mut prevents implementing From
impl From<ExpirationExtension2> for ValidatedExpirationExtension {
    fn from(e2: ExpirationExtension2) -> Self {
        let mut sectors = BitField::new();
        for sc in e2.sectors_with_claims {
            sectors.set(sc.sector_number)
        }
        sectors |= &e2.sectors;

        Self {
            deadline: e2.deadline,
            partition: e2.partition,
            sectors,
            new_expiration: e2.new_expiration,
        }
    }
}

/// Resolves an address to an ID address and verifies that it is address of an account or multisig actor.
fn resolve_control_address<BS, RT>(rt: &RT, raw: Address) -> Result<Address, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let resolved = rt
        .resolve_address(&raw)
        .ok_or_else(|| actor_error!(illegal_argument, "unable to resolve address: {}", raw))?;

    let owner_code = rt
        .get_actor_code_cid(&resolved)
        .ok_or_else(|| actor_error!(illegal_argument, "no code for address: {}", resolved))?;

    let is_principal = rt
        .resolve_builtin_actor_type(&owner_code)
        .as_ref()
        .map(|t| CALLER_TYPES_SIGNABLE.contains(t))
        .unwrap_or(false);

    if !is_principal {
        return Err(actor_error!(
            illegal_argument,
            "owner actor type must be a principal, was {}",
            owner_code
        ));
    }

    Ok(Address::new_id(resolved))
}

/// Resolves an address to an ID address and verifies that it is address of an account actor with an associated BLS key.
/// The worker must be BLS since the worker key will be used alongside a BLS-VRF.
fn resolve_worker_address<BS, RT>(rt: &mut RT, raw: Address) -> Result<Address, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let resolved = rt
        .resolve_address(&raw)
        .ok_or_else(|| actor_error!(illegal_argument, "unable to resolve address: {}", raw))?;

    let worker_code = rt
        .get_actor_code_cid(&resolved)
        .ok_or_else(|| actor_error!(illegal_argument, "no code for address: {}", resolved))?;
    if rt.resolve_builtin_actor_type(&worker_code) != Some(Type::Account) {
        return Err(actor_error!(
            illegal_argument,
            "worker actor type must be an account, was {}",
            worker_code
        ));
    }

    if raw.protocol() != Protocol::BLS {
        let ret = rt.send(
            &Address::new_id(resolved),
            ext::account::PUBKEY_ADDRESS_METHOD,
            RawBytes::default(),
            TokenAmount::zero(),
        )?;
        let pub_key: Address = deserialize(&ret, "address response")?;
        if pub_key.protocol() != Protocol::BLS {
            return Err(actor_error!(
                illegal_argument,
                "worker account {} must have BLS pubkey, was {}",
                resolved,
                pub_key.protocol()
            ));
        }
    }
    Ok(Address::new_id(resolved))
}

/// Assigns proving period offset randomly in the range [0, WPoStProvingPeriod) by hashing
/// the actor's address and current epoch.
fn assign_proving_period_offset(
    policy: &Policy,
    addr: Address,
    current_epoch: ChainEpoch,
    blake2b: impl FnOnce(&[u8]) -> [u8; 32],
) -> anyhow::Result<ChainEpoch> {
    let mut my_addr = addr.marshal_cbor()?;
    my_addr.write_i64::<BigEndian>(current_epoch)?;

    let digest = blake2b(&my_addr);

    let mut offset: u64 = BigEndian::read_u64(&digest);
    offset %= policy.wpost_proving_period as u64;

    // Conversion from i64 to u64 is safe because it's % WPOST_PROVING_PERIOD which is i64
    Ok(offset as ChainEpoch)
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Winow PoSt until the first full proving period starts.
fn current_proving_period_start(
    policy: &Policy,
    current_epoch: ChainEpoch,
    offset: ChainEpoch,
) -> ChainEpoch {
    let curr_modulus = current_epoch % policy.wpost_proving_period;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        policy.wpost_proving_period - (offset - curr_modulus)
    };

    current_epoch - period_progress
}

fn current_deadline_index(
    policy: &Policy,
    current_epoch: ChainEpoch,
    period_start: ChainEpoch,
) -> u64 {
    ((current_epoch - period_start) / policy.wpost_challenge_window) as u64
}

/// Validates that a partition contains the given sectors.
fn validate_partition_contains_sectors(
    partition: &Partition,
    sectors: &BitField,
) -> anyhow::Result<()> {
    // Check that the declared sectors are actually assigned to the partition.
    if partition.sectors.contains_all(sectors) {
        Ok(())
    } else {
        Err(anyhow!("not all sectors are assigned to the partition"))
    }
}

pub fn power_for_sector(sector_size: SectorSize, sector: &SectorOnChainInfo) -> PowerPair {
    PowerPair {
        raw: BigInt::from(sector_size as u64),
        qa: qa_power_for_sector(sector_size, sector),
    }
}

/// Returns the sum of the raw byte and quality-adjusted power for sectors.
pub fn power_for_sectors(sector_size: SectorSize, sectors: &[SectorOnChainInfo]) -> PowerPair {
    let qa = sectors
        .iter()
        .map(|s| qa_power_for_sector(sector_size, s))
        .sum();

    PowerPair {
        raw: BigInt::from(sector_size as u64) * BigInt::from(sectors.len()),
        qa,
    }
}

fn check_control_addresses(policy: &Policy, control_addrs: &[Address]) -> Result<(), ActorError> {
    if control_addrs.len() > policy.max_control_addresses {
        return Err(actor_error!(
            illegal_argument,
            "control addresses length {} exceeds max control addresses length {}",
            control_addrs.len(),
            policy.max_control_addresses
        ));
    }

    Ok(())
}

fn check_valid_post_proof_type(
    policy: &Policy,
    proof_type: RegisteredPoStProof,
) -> Result<(), ActorError> {
    if policy.valid_post_proof_type.contains(&proof_type) {
        Ok(())
    } else {
        Err(actor_error!(
            illegal_argument,
            "proof type {:?} not allowed for new miner actors",
            proof_type
        ))
    }
}

fn check_peer_info(
    policy: &Policy,
    peer_id: &[u8],
    multiaddrs: &[BytesDe],
) -> Result<(), ActorError> {
    if peer_id.len() > policy.max_peer_id_length {
        return Err(actor_error!(
            illegal_argument,
            "peer ID size of {} exceeds maximum size of {}",
            peer_id.len(),
            policy.max_peer_id_length
        ));
    }

    let mut total_size = 0;
    for ma in multiaddrs {
        if ma.0.is_empty() {
            return Err(actor_error!(illegal_argument, "invalid empty multiaddr"));
        }
        total_size += ma.0.len();
    }

    if total_size > policy.max_multiaddr_data {
        return Err(actor_error!(
            illegal_argument,
            "multiaddr size of {} exceeds maximum of {}",
            total_size,
            policy.max_multiaddr_data
        ));
    }

    Ok(())
}

#[cfg(test)]
mod internal_tests;
