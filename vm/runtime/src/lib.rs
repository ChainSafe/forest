// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod actor_code;

pub use self::actor_code::*;

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use commcid::{cid_to_data_commitment_v1, cid_to_replica_commitment_v1, data_commitment_v1_to_cid};
use crypto::{DomainSeparationTag, Signature};
use fil_types::{
    zero_piece_commitment, PaddedPieceSize, PieceInfo, RegisteredProof, SealVerifyInfo, SectorInfo,
    WindowPoStVerifyInfo,
};
use filecoin_proofs_api::{
    post::verify_window_post,
    seal::{compute_comm_d, verify_seal as proofs_verify_seal},
    PublicReplicaInfo,
};
use filecoin_proofs_api::{ProverId, SectorId};
use forest_encoding::{blake2b_256, Cbor};
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::error::Error as StdError;
use vm::{ActorError, ExitCode, MethodNum, Randomness, Serialized, TokenAmount};

/// Runtime is the VM's internal runtime object.
/// this is everything that is accessible to actors, beyond parameters.
pub trait Runtime<BS: BlockStore> {
    /// Information related to the current message being executed.
    fn message(&self) -> &UnsignedMessage;

    /// The current chain epoch number. The genesis block has epoch zero.
    fn curr_epoch(&self) -> ChainEpoch;

    /// Validates the caller against some predicate.
    /// Exported actor methods must invoke at least one caller validation before returning.
    fn validate_immediate_caller_accept_any(&self);
    fn validate_immediate_caller_is<'a, I>(&self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Address>;
    fn validate_immediate_caller_type<'a, I>(&self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Cid>;

    /// The balance of the receiver.
    fn current_balance(&self) -> Result<TokenAmount, ActorError>;

    /// Resolves an address of any protocol to an ID address (via the Init actor's table).
    /// This allows resolution of externally-provided SECP, BLS, or actor addresses to the canonical form.
    /// If the argument is an ID address it is returned directly.
    fn resolve_address(&self, address: &Address) -> Result<Address, ActorError>;

    /// Look up the code ID at an actor address.
    fn get_actor_code_cid(&self, addr: &Address) -> Result<Cid, ActorError>;

    /// Randomness returns a (pseudo)random byte array drawing from a
    /// random beacon at a given epoch and incorporating reequisite entropy
    fn get_randomness(
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Randomness;

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
    fn transaction<C: Cbor, R, F>(&mut self, f: F) -> Result<R, ActorError>
    where
        F: FnOnce(&mut C, &Self) -> R;

    /// Returns reference to blockstore
    fn store(&self) -> &BS;

    /// Sends a message to another actor, returning the exit code and return value envelope.
    /// If the invoked method does not return successfully, its state changes (and that of any messages it sent in turn)
    /// will be rolled back.
    fn send(
        &mut self,
        to: &Address,
        method: MethodNum,
        params: &Serialized,
        value: &TokenAmount,
    ) -> Result<Serialized, ActorError>;

    /// Halts execution upon an error from which the receiver cannot recover. The caller will receive the exitcode and
    /// an empty return value. State changes made within this call will be rolled back.
    /// This method does not return.
    /// The message and args are for diagnostic purposes and do not persist on chain.
    fn abort<S: AsRef<str>>(&self, exit_code: ExitCode, msg: S) -> ActorError;

    /// Computes an address for a new actor. The returned address is intended to uniquely refer to
    /// the actor even in the event of a chain re-org (whereas an ID-address might refer to a
    /// different actor after messages are re-ordered).
    /// Always an ActorExec address.
    fn new_actor_address(&mut self) -> Result<Address, ActorError>;

    /// Creates an actor with code `codeID` and address `address`, with empty state. May only be called by Init actor.
    fn create_actor(&mut self, code_id: &Cid, address: &Address) -> Result<(), ActorError>;

    /// Deletes the executing actor from the state tree. May only be called by the actor itself.
    fn delete_actor(&mut self) -> Result<(), ActorError>;

    /// Provides the system call interface.
    fn syscalls(&self) -> &dyn Syscalls;
}

/// Message information available to the actor about executing message.
pub trait MessageInfo {
    // The address of the immediate calling actor. Always an ID-address.
    fn caller(&self) -> Address;

    // The address of the actor receiving the message. Always an ID-address.
    fn receiver(&self) -> Address;

    // The value attached to the message being processed, implicitly added to current_balance() before method invocation.
    fn value_received(&self) -> TokenAmount;
}

/// Pure functions implemented as primitives by the runtime.
pub trait Syscalls {
    /// Verifies that a signature is valid for an address and plaintext.
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        Ok(signature.verify(plaintext, signer)?)
    }
    /// Hashes input data using blake2b with 256 bit output.
    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(blake2b_256(data))
    }
    /// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
    fn compute_unsealed_sector_cid(
        &self,
        proof_type: RegisteredProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, Box<dyn StdError>> {
        let sum: u64 = pieces.iter().map(|p| p.size.0).sum();

        let ssize = proof_type.sector_size() as u64;

        let mut fcp_pieces: Vec<filecoin_proofs_api::PieceInfo> = pieces
            .iter()
            .map(filecoin_proofs_api::PieceInfo::try_from)
            .collect::<Result<_, &'static str>>()
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e.to_string()))?;

        // pad remaining space with 0 piece commitments
        {
            let mut to_fill = ssize - sum;
            let n = to_fill.count_ones();
            for _ in 0..n {
                let next = to_fill.trailing_zeros();
                let p_size = 1 << next;
                to_fill ^= p_size;
                let padded = PaddedPieceSize(p_size);
                fcp_pieces.push(filecoin_proofs_api::PieceInfo {
                    commitment: zero_piece_commitment(padded),
                    size: padded.unpadded().into(),
                });
            }
        }

        let comm_d = compute_comm_d(proof_type.into(), &fcp_pieces)
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e.to_string()))?;

        Ok(data_commitment_v1_to_cid(&comm_d))
    }
    /// Verifies a sector seal proof.
    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        let commd = cid_to_data_commitment_v1(&vi.unsealed_cid)?;
        let commr = cid_to_replica_commitment_v1(&vi.on_chain.sealed_cid)?;
        let miner_addr = Address::new_id(vi.sector_id.miner);
        let miner_payload = miner_addr.payload_bytes();
        let mut prover_id = ProverId::default();
        prover_id[..miner_payload.len()].copy_from_slice(&miner_payload);

        if !proofs_verify_seal(
            vi.on_chain.registered_proof.into(),
            commr,
            commd,
            prover_id,
            SectorId::from(vi.sector_id.number),
            vi.randomness,
            vi.interactive_randomness,
            &vi.on_chain.proof,
        )? {
            return Err(format!(
                "Invalid proof detected: {:?}",
                base64::encode(&vi.on_chain.proof)
            )
            .into());
        }

        Ok(())
    }
    /// Verifies a proof of spacetime.
    fn verify_post(&self, verify_info: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>> {
        type ReplicaMapResult = Result<(SectorId, PublicReplicaInfo), String>;

        // collect proof bytes
        let proofs = &verify_info.proofs.iter().fold(Vec::new(), |mut proof, p| {
            proof.extend_from_slice(&p.proof_bytes);
            proof
        });

        // collect replicas
        let replicas = verify_info
            .challenged_sectors
            .iter()
            .map::<ReplicaMapResult, _>(|sector_info: &SectorInfo| {
                let commr = cid_to_replica_commitment_v1(&sector_info.sealed_cid)?;
                let replica = PublicReplicaInfo::new(
                    sector_info.proof.registered_window_post_proof()?.into(),
                    commr,
                );
                Ok((SectorId::from(sector_info.sector_number), replica))
            })
            .collect::<Result<BTreeMap<SectorId, PublicReplicaInfo>, _>>()?;

        // construct prover id
        let mut prover_id = ProverId::default();
        let prover_bytes = verify_info.prover.to_be_bytes();
        prover_id[..prover_bytes.len()].copy_from_slice(&prover_bytes);

        // verify
        if !verify_window_post(&verify_info.randomness, &proofs, &replicas, prover_id)? {
            return Err("Proof was invalid".to_string().into());
        }

        Ok(())
    }
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
        _earliest: ChainEpoch,
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>>;
}

/// Result of checking two headers for a consensus fault.
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
