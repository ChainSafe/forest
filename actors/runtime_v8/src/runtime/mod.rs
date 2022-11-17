// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{Cbor, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::consensus::ConsensusFault;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::econ::TokenAmount;
use fvm_shared::piece::PieceInfo;
use fvm_shared::randomness::RANDOMNESS_LENGTH;
use fvm_shared::sector::{
    AggregateSealVerifyProofAndInfos, RegisteredSealProof, ReplicaUpdateInfo, SealVerifyInfo,
    WindowPoStVerifyInfo,
};
use fvm_shared::version::NetworkVersion;
use fvm_shared::{ActorID, MethodNum};

pub use self::actor_code::*;
pub use self::policy::*;
pub use self::randomness::DomainSeparationTag;
use crate::runtime::builtins::Type;
use crate::ActorError;

mod actor_code;
pub mod builtins;
pub mod policy;
mod randomness;

#[cfg(feature = "fil-actor")]
mod actor_blockstore;
#[cfg(feature = "fil-actor")]
pub mod fvm;
#[cfg(feature = "fil-actor")]
pub(crate) mod hash_algorithm;

pub(crate) mod empty;
pub use empty::EMPTY_ARR_CID;

/// Runtime is the VM's internal runtime object.
/// this is everything that is accessible to actors, beyond parameters.
pub trait Runtime<BS: Blockstore>: Primitives + Verifier + RuntimePolicy {
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
        I: IntoIterator<Item = &'a Type>;

    /// The balance of the receiver.
    fn current_balance(&self) -> TokenAmount;

    /// Resolves an address of any protocol to an ID address (via the Init actor's table).
    /// This allows resolution of externally-provided SECP, BLS, or actor addresses to the canonical form.
    /// If the argument is an ID address it is returned directly.
    fn resolve_address(&self, address: &Address) -> Option<ActorID>;

    /// Look up the code ID at an actor address.
    fn get_actor_code_cid(&self, id: &ActorID) -> Option<Cid>;

    /// Randomness returns a (pseudo)random byte array drawing from the latest
    /// ticket chain from a given epoch and incorporating requisite entropy.
    /// This randomness is fork dependant but also biasable because of this.
    fn get_randomness_from_tickets(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH], ActorError>;

    /// Randomness returns a (pseudo)random byte array drawing from the latest
    /// beacon from a given epoch and incorporating requisite entropy.
    /// This randomness is not tied to any fork of the chain, and is unbiasable.
    fn get_randomness_from_beacon(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH], ActorError>;

    /// Initializes the state object.
    /// This is only valid when the state has not yet been initialized.
    /// NOTE: we should also limit this to being invoked during the constructor method
    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError>;

    /// Loads a readonly copy of the state of the receiver into the argument.
    fn state<C: Cbor>(&self) -> Result<C, ActorError>;

    /// Loads a mutable copy of the state of the receiver, passes it to `f`,
    /// and after `f` completes puts the state object back to the store and sets it as
    /// the receiver's state root.
    ///
    /// During the call to `f`, execution is protected from side-effects, (including message send).
    ///
    /// Returns the result of `f`.
    fn transaction<C, RT, F>(&mut self, f: F) -> Result<RT, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &mut Self) -> Result<RT, ActorError>;

    /// Returns reference to blockstore
    fn store(&self) -> &BS;

    /// Sends a message to another actor, returning the exit code and return value envelope.
    /// If the invoked method does not return successfully, its state changes
    /// (and that of any messages it sent in turn) will be rolled back.
    /// Note that the current return type cannot distinguish between a successful invocation
    /// that returns an error code, and an error originating from the syscall prior to
    /// invoking the target actor/method.
    fn send(
        &self,
        to: &Address,
        method: MethodNum,
        params: RawBytes,
        value: TokenAmount,
    ) -> Result<RawBytes, ActorError>;

    /// Computes an address for a new actor. The returned address is intended to uniquely refer to
    /// the actor even in the event of a chain re-org (whereas an ID-address might refer to a
    /// different actor after messages are re-ordered).
    /// Always an ActorExec address.
    fn new_actor_address(&mut self) -> Result<Address, ActorError>;

    /// Creates an actor with code `codeID` and address `address`, with empty state.
    /// May only be called by Init actor.
    fn create_actor(&mut self, code_id: Cid, address: ActorID) -> Result<(), ActorError>;

    /// Deletes the executing actor from the state tree, transferring any balance to beneficiary.
    /// Aborts if the beneficiary does not exist.
    /// May only be called by the actor itself.
    fn delete_actor(&mut self, beneficiary: &Address) -> Result<(), ActorError>;

    /// Returns whether the specified CodeCID belongs to a built-in actor.
    fn resolve_builtin_actor_type(&self, code_id: &Cid) -> Option<Type>;

    /// Returns the CodeCID for a built-in actor type. The kernel will abort
    /// if the supplied type is invalid.
    fn get_code_cid_for_type(&self, typ: Type) -> Cid;

    /// Returns the total token supply in circulation at the beginning of the current epoch.
    /// The circulating supply is the sum of:
    /// - rewards emitted by the reward actor,
    /// - funds vested from lock-ups in the genesis state,
    /// less the sum of:
    /// - funds burnt,
    /// - pledge collateral locked in storage miner actors (recorded in the storage power actor)
    /// - deal collateral locked by the storage market actor
    fn total_fil_circ_supply(&self) -> TokenAmount;

    /// ChargeGas charges specified amount of `gas` for execution.
    /// `name` provides information about gas charging point
    fn charge_gas(&mut self, name: &'static str, compute: i64);

    fn base_fee(&self) -> TokenAmount;
}

/// Message information available to the actor about executing message.
pub trait MessageInfo {
    /// The address of the immediate calling actor. Always an ID-address.
    fn caller(&self) -> Address;

    /// The address of the actor receiving the message. Always an ID-address.
    fn receiver(&self) -> Address;

    /// The value attached to the message being processed, implicitly
    /// added to current_balance() before method invocation.
    fn value_received(&self) -> TokenAmount;
}

/// Pure functions implemented as primitives by the runtime.
pub trait Primitives {
    /// Hashes input data using blake2b with 256 bit output.
    fn hash_blake2b(&self, data: &[u8]) -> [u8; 32];

    /// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
    fn compute_unsealed_sector_cid(
        &self,
        proof_type: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, anyhow::Error>;

    /// Verifies that a signature is valid for an address and plaintext.
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), anyhow::Error>;
}

/// filcrypto verification primitives provided by the runtime
pub trait Verifier {
    /// Verifies a sector seal proof.
    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), anyhow::Error>;

    /// Verifies a window proof of spacetime.
    fn verify_post(&self, verify_info: &WindowPoStVerifyInfo) -> Result<(), anyhow::Error>;

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
    ) -> Result<Option<ConsensusFault>, anyhow::Error>;

    fn batch_verify_seals(&self, batch: &[SealVerifyInfo]) -> anyhow::Result<Vec<bool>>;

    fn verify_aggregate_seals(
        &self,
        aggregate: &AggregateSealVerifyProofAndInfos,
    ) -> Result<(), anyhow::Error>;

    fn verify_replica_update(&self, replica: &ReplicaUpdateInfo) -> Result<(), anyhow::Error>;
}
