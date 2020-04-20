// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod actor_code;

pub use self::actor_code::*;

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use forest_encoding::{blake2b_256, Cbor};
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use vm::{
    ActorError, ExitCode, MethodNum, PieceInfo, Randomness, RegisteredProof, SealVerifyInfo,
    Serialized, TokenAmount, WindowPoStVerifyInfo,
};

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

/// Default syscalls information
#[derive(Copy, Clone, Debug)]
pub struct DefaultSyscalls;
impl Syscalls for DefaultSyscalls {}

/// Pure functions implemented as primitives by the runtime.
pub trait Syscalls {
    /// Verifies that a signature is valid for an address and plaintext.
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), ActorError> {
        signature
            .verify(plaintext, signer)
            .map_err(|e| ActorError::new(ExitCode::ErrPlaceholder, e))
    }
    /// Hashes input data using blake2b with 256 bit output.
    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], ActorError> {
        Ok(blake2b_256(data))
    }
    /// Computes an unsealed sector CID (CommD) from its constituent piece CIDs (CommPs) and sizes.
    fn compute_unsealed_sector_cid(
        &self,
        _reg: RegisteredProof,
        _pieces: &[PieceInfo],
    ) -> Result<Cid, ActorError> {
        // TODO
        todo!()
    }
    /// Verifies a sector seal proof.
    fn verify_seal(&self, _vi: &SealVerifyInfo) -> Result<(), ActorError> {
        // TODO
        todo!()
    }
    /// Verifies a proof of spacetime.
    fn verify_post(&self, _vi: &WindowPoStVerifyInfo) -> Result<(), ActorError> {
        // TODO
        todo!()
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
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
        _earliest: ChainEpoch,
    ) -> Result<ConsensusFault, ActorError> {
        // TODO
        todo!()
    }
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
