// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod actor_code;

pub use self::actor_code::*;

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use commcid::data_commitment_v1_to_cid;
use crypto::{DomainSeparationTag, Signature};
use fil_types::{
    zero_piece_commitment, NetworkVersion, PaddedPieceSize, PieceInfo, Randomness,
    RegisteredSealProof, SealVerifyInfo, WindowPoStVerifyInfo,
};
use filecoin_proofs_api::seal::compute_comm_d;
use filecoin_proofs_api::{self as proofs};
use forest_encoding::{blake2b_256, de, Cbor};
use ipld_blockstore::BlockStore;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use vm::{ActorError, ExitCode, MethodNum, Serialized, TokenAmount};

/// Runtime is the VM's internal runtime object.
/// this is everything that is accessible to actors, beyond parameters.
pub trait Runtime<BS: BlockStore>: Syscalls {
    /// The network protocol version number at the current epoch.
    fn network_version(&self) -> NetworkVersion;

    /// Information related to the current message being executed.
    fn message(&self) -> &dyn MessageInfo;

    /// The current chain epoch number. The genesis block has epoch zero.
    fn curr_epoch(&self) -> ChainEpoch;

    /// Validates the caller against some predicate.
    /// Exported actor methods must invoke at least one caller validation before returning.
    fn validate_immediate_caller_accept_any(&mut self) -> Result<(), ActorError>;
    fn validate_immediate_caller_is<'a, I>(&mut self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Address>;
    fn validate_immediate_caller_type<'a, I>(&mut self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Cid>;

    /// The balance of the receiver.
    fn current_balance(&self) -> Result<TokenAmount, ActorError>;

    /// Resolves an address of any protocol to an ID address (via the Init actor's table).
    /// This allows resolution of externally-provided SECP, BLS, or actor addresses to the canonical form.
    /// If the argument is an ID address it is returned directly.
    fn resolve_address(&self, address: &Address) -> Result<Option<Address>, ActorError>;

    /// Look up the code ID at an actor address.
    fn get_actor_code_cid(&self, addr: &Address) -> Result<Option<Cid>, ActorError>;

    /// Randomness returns a (pseudo)random byte array drawing from the latest
    /// ticket chain from a given epoch and incorporating requisite entropy.
    /// This randomness is fork dependant but also biasable because of this.
    fn get_randomness_from_tickets(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError>;

    /// Randomness returns a (pseudo)random byte array drawing from the latest
    /// beacon from a given epoch and incorporating requisite entropy.
    /// This randomness is not tied to any fork of the chain, and is unbiasable.
    fn get_randomness_from_beacon(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError>;

    /// Initializes the state object.
    /// This is only valid in a constructor function and when the state has not yet been initialized.
    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError>;

    /// Loads a readonly copy of the state of the receiver into the argument.
    ///
    /// Any modification to the state is illegal and will result in an abort.
    fn state<C: Cbor>(&self) -> Result<C, ActorError>;

    /// Loads a mutable version of the state into the `obj` argument and protects
    /// the execution from side effects (including message send).
    ///
    /// The second argument is a function which allows the caller to mutate the state.
    /// The return value from that function will be returned from the call to Transaction().
    ///
    /// If the state is modified after this function returns, execution will abort.
    ///
    /// The gas cost of this method is that of a Store.Put of the mutated state object.
    fn transaction<C, RT, F>(&mut self, f: F) -> Result<RT, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &mut Self) -> Result<RT, ActorError>;

    /// Returns reference to blockstore
    fn store(&self) -> &BS;

    /// Sends a message to another actor, returning the exit code and return value envelope.
    /// If the invoked method does not return successfully, its state changes
    /// (and that of any messages it sent in turn) will be rolled back.
    fn send(
        &mut self,
        to: Address,
        method: MethodNum,
        params: Serialized,
        value: TokenAmount,
    ) -> Result<Serialized, ActorError>;

    /// Computes an address for a new actor. The returned address is intended to uniquely refer to
    /// the actor even in the event of a chain re-org (whereas an ID-address might refer to a
    /// different actor after messages are re-ordered).
    /// Always an ActorExec address.
    fn new_actor_address(&mut self) -> Result<Address, ActorError>;

    /// Creates an actor with code `codeID` and address `address`, with empty state.
    /// May only be called by Init actor.
    fn create_actor(&mut self, code_id: Cid, address: &Address) -> Result<(), ActorError>;

    /// Deletes the executing actor from the state tree, transferring any balance to beneficiary.
    /// Aborts if the beneficiary does not exist.
    /// May only be called by the actor itself.
    fn delete_actor(&mut self, beneficiary: &Address) -> Result<(), ActorError>;

    /// Returns the total token supply in circulation at the beginning of the current epoch.
    /// The circulating supply is the sum of:
    /// - rewards emitted by the reward actor,
    /// - funds vested from lock-ups in the genesis state,
    /// less the sum of:
    /// - funds burnt,
    /// - pledge collateral locked in storage miner actors (recorded in the storage power actor)
    /// - deal collateral locked by the storage market actor
    fn total_fil_circ_supply(&self) -> Result<TokenAmount, ActorError>;

    /// ChargeGas charges specified amount of `gas` for execution.
    /// `name` provides information about gas charging point
    fn charge_gas(&mut self, name: &'static str, compute: i64) -> Result<(), ActorError>;

    /// This function is a workaround for go-implementation's faulty exit code handling of
    /// parameters before version 7
    fn deserialize_params<O: de::DeserializeOwned>(
        &self,
        params: &Serialized,
    ) -> Result<O, ActorError> {
        params.deserialize().map_err(|e| {
            if self.network_version() < NetworkVersion::V7 {
                ActorError::new(
                    ExitCode::SysErrSenderInvalid,
                    format!("failed to decode parameters: {}", e),
                )
            } else {
                ActorError::from(e).wrap("failed to decode parameters")
            }
        })
    }
}

/// Message information available to the actor about executing message.
pub trait MessageInfo {
    /// The address of the immediate calling actor. Always an ID-address.
    fn caller(&self) -> &Address;

    /// The address of the actor receiving the message. Always an ID-address.
    fn receiver(&self) -> &Address;

    /// The value attached to the message being processed, implicitly
    /// added to current_balance() before method invocation.
    fn value_received(&self) -> &TokenAmount;
}

/// Pure functions implemented as primitives by the runtime.
pub trait Syscalls {
    /// Verifies that a signature is valid for an address and plaintext.
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), Box<dyn StdError>>;
    /// Hashes input data using blake2b with 256 bit output.
    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(blake2b_256(data))
    }
    /// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
    fn compute_unsealed_sector_cid(
        &self,
        proof_type: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, Box<dyn StdError>> {
        compute_unsealed_sector_cid(proof_type, pieces)
    }
    /// Verifies a sector seal proof.
    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), Box<dyn StdError>>;

    /// Verifies a window proof of spacetime.
    fn verify_post(&self, verify_info: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>>;

    /// Verifies that two block headers provide proof of a consensus fault:
    /// - both headers mined by the same actor
    /// - headers are different
    /// - first header is of the same or lower epoch as the second
    /// - at least one of the headers appears in the current chain at or after epoch `earliest`
    /// - the headers provide evidence of a fault (see the spec for the different fault types).
    /// The parameters are all serialized block headers. The third "extra" parameter is consulted only for
    /// the "parent grinding fault", in which case it must be the sibling of h1 (same parent tipset) and one of the
    /// blocks in the parent of h2 (i.e. h2's grandparent).
    /// Returns nil and an error if the headers don't prove a fault.
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>>;

    fn batch_verify_seals(
        &self,
        vis: &[(&Address, &Vec<SealVerifyInfo>)],
    ) -> Result<HashMap<Address, Vec<bool>>, Box<dyn StdError>> {
        let mut verified = HashMap::new();
        for (&addr, s) in vis.iter() {
            let vals = s.iter().map(|si| self.verify_seal(si).is_ok()).collect();
            verified.insert(addr, vals);
        }
        Ok(verified)
    }
}

/// Result of checking two headers for a consensus fault.
#[derive(Clone)]
pub struct ConsensusFault {
    /// Address of the miner at fault (always an ID address).
    pub target: Address,
    /// Epoch of the fault, which is the higher epoch of the two blocks causing it.
    pub epoch: ChainEpoch,
    /// Type of fault.
    pub fault_type: ConsensusFaultType,
}

/// Consensus fault types in VM.
#[derive(Clone, Copy)]
pub enum ConsensusFaultType {
    DoubleForkMining = 1,
    ParentGrinding = 2,
    TimeOffsetMining = 3,
}

fn get_required_padding(
    old_length: PaddedPieceSize,
    new_piece_length: PaddedPieceSize,
) -> (Vec<PaddedPieceSize>, PaddedPieceSize) {
    let mut sum = 0;

    let mut to_fill = 0u64.wrapping_sub(old_length.0) % new_piece_length.0;
    let n = to_fill.count_ones();
    let mut pad_pieces = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let next = to_fill.trailing_zeros();
        let p_size = 1 << next;
        to_fill ^= p_size;

        let padded = PaddedPieceSize(p_size);
        pad_pieces.push(padded);
        sum += padded.0;
    }

    (pad_pieces, PaddedPieceSize(sum))
}

/// Computes sector [Cid] from proof type and pieces for verification.
pub fn compute_unsealed_sector_cid(
    proof_type: RegisteredSealProof,
    pieces: &[PieceInfo],
) -> Result<Cid, Box<dyn StdError>> {
    let ssize = proof_type.sector_size()? as u64;

    let mut all_pieces = Vec::<proofs::PieceInfo>::with_capacity(pieces.len());

    let pssize = PaddedPieceSize(ssize);
    if pieces.is_empty() {
        all_pieces.push(proofs::PieceInfo {
            size: pssize.unpadded().into(),
            commitment: zero_piece_commitment(pssize),
        })
    } else {
        // pad remaining space with 0 piece commitments
        let mut sum = PaddedPieceSize(0);
        let pad_to = |pads: Vec<PaddedPieceSize>,
                      all_pieces: &mut Vec<proofs::PieceInfo>,
                      sum: &mut PaddedPieceSize| {
            for p in pads {
                all_pieces.push(proofs::PieceInfo {
                    size: p.unpadded().into(),
                    commitment: zero_piece_commitment(p),
                });

                sum.0 += p.0;
            }
        };
        for p in pieces {
            let (ps, _) = get_required_padding(sum, p.size);
            pad_to(ps, &mut all_pieces, &mut sum);

            all_pieces.push(proofs::PieceInfo::try_from(p)?);
            sum.0 += p.size.0;
        }

        let (ps, _) = get_required_padding(sum, pssize);
        pad_to(ps, &mut all_pieces, &mut sum);
    }

    let comm_d = compute_comm_d(proof_type.try_into()?, &all_pieces)?;

    Ok(data_commitment_v1_to_cid(&comm_d)?)
}
